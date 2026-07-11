//! État d'un salon vocal full-mesh (SPEC §8).
//!
//! Sans E/S ni horloge propre : l'hôte (démon/UI) fournit les trames PCM
//! capturées et l'horloge média, transmet les [`VoiceMsg`] produits à chaque
//! participant via sa session transport chiffrée, et pousse les trames
//! décodées vers la sortie audio. Le chiffrement est assuré par le transport
//! (canal VOICE) ; ce module ne manipule que du clair local.

use std::collections::BTreeMap;

use accord_proto::plaintext::VoiceMsg;

use crate::bitrate;
use crate::codec::{AudioCodec, CodecError};
use crate::dsp::CaptureDsp;
use crate::gain;
use crate::jitter::{JitterBuffer, Playout};
use crate::loss::LossEstimator;
use crate::params::{BITRATE_MIN, FRAME_MS, MAX_PARTICIPANTS};
use crate::vad::Vad;

/// Type de média audio Opus (SPEC §8).
const MEDIA_AUDIO_OPUS: u8 = 0x01;

/// Fabrique de codecs (un décodeur par participant, un encodeur local).
pub type CodecFactory = Box<dyn Fn() -> Box<dyn AudioCodec> + Send>;

/// État d'un participant distant.
struct Peer {
    jitter: JitterBuffer,
    loss: LossEstimator,
    decoder: Box<dyn AudioCodec>,
    /// Linear output gain for this participant (1.0 = unity).
    gain: f32,
    /// Transient attenuation (0.0..=1.0, 1.0 = none) applied on top of the
    /// gains — priority-speaker ducking, never persisted.
    duck: f32,
}

/// Erreur d'opération de salon.
#[derive(Debug, thiserror::Error)]
pub enum RoomError {
    /// Salon plein (full mesh borné).
    #[error("salon vocal plein")]
    Full,
    /// Participant inconnu.
    #[error("participant inconnu")]
    UnknownPeer,
    /// Erreur de codec.
    #[error(transparent)]
    Codec(#[from] CodecError),
}

/// Salon vocal actif.
pub struct VoiceRoom {
    room_id: [u8; 16],
    make_codec: CodecFactory,
    encoder: Box<dyn AudioCodec>,
    bitrate: u32,
    vad: Vad,
    seq: u16,
    ts_ms: u32,
    peers: BTreeMap<[u8; 32], Peer>,
    /// Linear master output gain applied to every decoded frame (1.0 = unity).
    master_gain: f32,
    /// Deafened: incoming frames are drained without decoding or playback.
    deafened: bool,
    /// Capture DSP chain (noise suppression + AGC), applied before the VAD.
    dsp: CaptureDsp,
}

impl VoiceRoom {
    /// Crée un salon avec une fabrique de codecs (encodeur local créé aussitôt).
    pub fn new(room_id: [u8; 16], make_codec: CodecFactory) -> Self {
        let encoder = make_codec();
        Self {
            room_id,
            make_codec,
            encoder,
            bitrate: BITRATE_MIN,
            vad: Vad::default(),
            seq: 0,
            ts_ms: 0,
            peers: BTreeMap::new(),
            master_gain: 1.0,
            deafened: false,
            dsp: CaptureDsp::default(),
        }
    }

    /// Identifiant du salon.
    pub fn room_id(&self) -> [u8; 16] {
        self.room_id
    }

    /// Débit d'encodage courant (bit/s).
    pub fn bitrate(&self) -> u32 {
        self.bitrate
    }

    /// Participants actuels.
    pub fn participant_count(&self) -> usize {
        self.peers.len()
    }

    /// Ajoute un participant (borne full mesh incluant soi-même).
    pub fn add_participant(&mut self, pubkey: [u8; 32]) -> Result<(), RoomError> {
        if self.peers.contains_key(&pubkey) {
            return Ok(());
        }
        if self.peers.len() + 1 >= MAX_PARTICIPANTS {
            return Err(RoomError::Full);
        }
        self.peers.insert(
            pubkey,
            Peer {
                jitter: JitterBuffer::new(),
                loss: LossEstimator::new(),
                decoder: (self.make_codec)(),
                gain: 1.0,
                duck: 1.0,
            },
        );
        Ok(())
    }

    /// Retire un participant.
    pub fn remove_participant(&mut self, pubkey: &[u8; 32]) {
        self.peers.remove(pubkey);
    }

    /// Sets the master output gain (clamped to 0.0..=2.0, 1.0 = unity),
    /// applied to every decoded frame before playback.
    pub fn set_master_gain(&mut self, gain: f32) {
        self.master_gain = gain.clamp(0.0, gain::gain_of_pct(gain::VOLUME_MAX_PCT));
    }

    /// Sets the output gain of one participant (clamped to 0.0..=2.0,
    /// 1.0 = unity). Unknown participants are ignored.
    pub fn set_peer_gain(&mut self, pubkey: &[u8; 32], gain: f32) {
        if let Some(peer) = self.peers.get_mut(pubkey) {
            peer.gain = gain.clamp(0.0, gain::gain_of_pct(gain::VOLUME_MAX_PCT));
        }
    }

    /// Sets the transient ducking attenuation of one participant (clamped to
    /// 0.0..=1.0, 1.0 = none) — priority-speaker attenuation, applied on top
    /// of the peer and master gains. Unknown participants are ignored.
    pub fn set_peer_duck(&mut self, pubkey: &[u8; 32], duck: f32) {
        if let Some(peer) = self.peers.get_mut(pubkey) {
            peer.duck = duck.clamp(0.0, 1.0);
        }
    }

    /// Enables/disables capture noise suppression (RNNoise) at runtime.
    pub fn set_noise_suppression(&mut self, enabled: bool) {
        self.dsp.set_noise_suppression(enabled);
    }

    /// Enables/disables the capture automatic gain control at runtime.
    pub fn set_agc(&mut self, enabled: bool) {
        self.dsp.set_agc(enabled);
    }

    /// Deafens (`true`) or restores (`false`) the local output: while
    /// deafened, [`Self::play`] drains jitter buffers without decoding so no
    /// stale audio accumulates for later playback.
    pub fn set_deafened(&mut self, deafened: bool) {
        self.deafened = deafened;
    }

    /// Capture une trame PCM locale : applique la chaîne DSP (suppression de
    /// bruit puis AGC, si actives) puis renvoie la trame à diffuser à tous
    /// les participants, ou `None` si la VAD la juge silencieuse. L'horloge
    /// média avance à chaque appel (cadence de 20 ms côté hôte).
    pub fn capture(&mut self, pcm: &[i16]) -> Result<Option<VoiceMsg>, RoomError> {
        let mut pcm = pcm.to_vec();
        self.dsp.process(&mut pcm);
        let active = self.vad.is_active(&pcm);
        self.ts_ms = self.ts_ms.wrapping_add(FRAME_MS);
        if !active {
            return Ok(None);
        }
        let payload = self.encoder.encode(&pcm)?;
        let frame = VoiceMsg::AudioFrame {
            room: self.room_id,
            media_type: MEDIA_AUDIO_OPUS,
            seq: self.seq,
            ts_ms: self.ts_ms,
            payload,
        };
        self.seq = self.seq.wrapping_add(1);
        Ok(Some(frame))
    }

    /// Ingest une trame reçue d'un participant (dans sa session chiffrée).
    pub fn on_frame(&mut self, from: &[u8; 32], frame: VoiceMsg, now_ms: u32) {
        let VoiceMsg::AudioFrame {
            room, seq, payload, ..
        } = frame
        else {
            return;
        };
        if room != self.room_id {
            return;
        }
        if let Some(peer) = self.peers.get_mut(from) {
            peer.loss.observe(seq, now_ms);
            peer.jitter.push(seq, payload, now_ms);
        }
    }

    /// Produit la prochaine trame PCM à jouer pour un participant (cadence de
    /// 20 ms). `None` tant que le tampon s'amorce. While deafened, the jitter
    /// buffer is drained without decoding and nothing is played; otherwise
    /// the per-peer and master output gains are applied to the decoded PCM
    /// (saturating, before mixing at the output).
    pub fn play(&mut self, from: &[u8; 32]) -> Result<Option<Vec<i16>>, RoomError> {
        let deafened = self.deafened;
        let master_gain = self.master_gain;
        let peer = self.peers.get_mut(from).ok_or(RoomError::UnknownPeer)?;
        let playout = peer.jitter.pop();
        if deafened {
            return Ok(None);
        }
        let mut pcm = match playout {
            Playout::Frame(pkt) => peer.decoder.decode(Some(&pkt))?,
            Playout::Conceal => peer.decoder.decode(None)?,
            Playout::Starved => return Ok(None),
        };
        gain::apply_gain(&mut pcm, peer.gain * master_gain * peer.duck);
        Ok(Some(pcm))
    }

    /// Construit le retour de qualité à envoyer à un participant.
    pub fn quality_ping(&self, to: &[u8; 32], rtt_ms: u16) -> Option<VoiceMsg> {
        self.peers.get(to).map(|peer| VoiceMsg::VoicePing {
            loss_pct: peer.loss.loss_pct(),
            rtt_ms,
        })
    }

    /// Applique un retour de qualité reçu : adapte le débit d'encodage.
    pub fn on_ping(&mut self, ping: &VoiceMsg) {
        if let VoiceMsg::VoicePing { loss_pct, .. } = ping {
            self.bitrate = bitrate::adapt(self.bitrate, *loss_pct);
            self.encoder.set_bitrate(self.bitrate);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::PassthroughCodec;
    use crate::params::FRAME_SAMPLES;

    fn room() -> VoiceRoom {
        VoiceRoom::new([7u8; 16], Box::new(|| Box::new(PassthroughCodec)))
    }

    fn tone(a: i16) -> Vec<i16> {
        (0..FRAME_SAMPLES)
            .map(|i| if i % 2 == 0 { a } else { -a })
            .collect()
    }

    #[test]
    fn capture_gates_on_vad_and_sequences() {
        let mut r = room();
        // Silence : rien à diffuser.
        assert!(r.capture(&vec![0i16; FRAME_SAMPLES]).unwrap().is_none());
        // Parole : trame séquencée.
        let f0 = r.capture(&tone(20_000)).unwrap().unwrap();
        let f1 = r.capture(&tone(20_000)).unwrap().unwrap();
        match (f0, f1) {
            (VoiceMsg::AudioFrame { seq: s0, .. }, VoiceMsg::AudioFrame { seq: s1, .. }) => {
                assert_eq!(s0, 0);
                assert_eq!(s1, 1);
            }
            _ => panic!("trames audio attendues"),
        }
    }

    #[test]
    fn end_to_end_capture_transport_playback() {
        let mut sender = room();
        let mut receiver = room();
        let spk = [1u8; 32];
        receiver.add_participant(spk).unwrap();

        // L'émetteur capture 3 trames de parole ; le récepteur les rejoue.
        let mut frames = Vec::new();
        for _ in 0..3 {
            frames.push(sender.capture(&tone(15_000)).unwrap().unwrap());
        }
        for (i, f) in frames.into_iter().enumerate() {
            receiver.on_frame(&spk, f, i as u32 * 20);
        }
        // Amorçage puis lecture : au moins une trame décodée non silencieuse.
        let mut decoded = None;
        for _ in 0..5 {
            if let Some(pcm) = receiver.play(&spk).unwrap() {
                if pcm.iter().any(|&s| s != 0) {
                    decoded = Some(pcm);
                    break;
                }
            }
        }
        assert!(decoded.is_some(), "aucune trame rejouée");
    }

    #[test]
    fn ping_drives_bitrate_adaptation() {
        let mut r = room();
        assert_eq!(r.bitrate(), BITRATE_MIN);
        // Réseau sain : le débit remonte.
        r.on_ping(&VoiceMsg::VoicePing {
            loss_pct: 0,
            rtt_ms: 30,
        });
        assert!(r.bitrate() > BITRATE_MIN);
        // Perte forte : chute immédiate au plancher.
        r.on_ping(&VoiceMsg::VoicePing {
            loss_pct: 20,
            rtt_ms: 30,
        });
        assert_eq!(r.bitrate(), BITRATE_MIN);
    }

    #[test]
    fn quality_ping_reports_measured_loss() {
        let mut r = room();
        let spk = [2u8; 32];
        r.add_participant(spk).unwrap();
        // 1 trame sur 2 reçue.
        for i in 0..40u16 {
            r.on_frame(
                &spk,
                VoiceMsg::AudioFrame {
                    room: [7u8; 16],
                    media_type: MEDIA_AUDIO_OPUS,
                    seq: i * 2,
                    ts_ms: 0,
                    payload: vec![0u8; FRAME_SAMPLES * 2],
                },
                i as u32 * 40,
            );
        }
        let ping = r.quality_ping(&spk, 50).unwrap();
        match ping {
            VoiceMsg::VoicePing { loss_pct, .. } => assert!(loss_pct >= 40),
            _ => panic!("ping attendu"),
        }
    }

    #[test]
    fn mesh_is_bounded() {
        let mut r = room();
        for i in 0..(MAX_PARTICIPANTS - 1) {
            let mut pk = [0u8; 32];
            pk[0] = i as u8;
            r.add_participant(pk).unwrap();
        }
        // La place pour soi-même est réservée : le suivant déborde.
        assert!(matches!(
            r.add_participant([200u8; 32]),
            Err(RoomError::Full)
        ));
    }

    /// Codec that panics on decode: proves deafened playback never decodes.
    struct NoDecodeCodec;

    impl AudioCodec for NoDecodeCodec {
        fn encode(&mut self, pcm: &[i16]) -> Result<Vec<u8>, CodecError> {
            PassthroughCodec.encode(pcm)
        }

        fn decode(&mut self, _packet: Option<&[u8]>) -> Result<Vec<i16>, CodecError> {
            panic!("decode must not be called while deafened");
        }
    }

    fn feed_frames(room: &mut VoiceRoom, from: &[u8; 32], count: u16) {
        for seq in 0..count {
            room.on_frame(
                from,
                VoiceMsg::AudioFrame {
                    room: [7u8; 16],
                    media_type: MEDIA_AUDIO_OPUS,
                    seq,
                    ts_ms: u32::from(seq) * 20,
                    payload: PassthroughCodec.encode(&tone(15_000)).unwrap(),
                },
                u32::from(seq) * 20,
            );
        }
    }

    #[test]
    fn deafened_room_drains_without_decoding_or_playing() {
        let mut r = VoiceRoom::new([7u8; 16], Box::new(|| Box::new(NoDecodeCodec)));
        let spk = [4u8; 32];
        r.add_participant(spk).unwrap();
        r.set_deafened(true);
        feed_frames(&mut r, &spk, 6);
        // Deafened: nothing is played, frames are drained, no decode occurs.
        for _ in 0..12 {
            assert!(r.play(&spk).unwrap().is_none());
        }
        // Undeafen: the buffer was drained, no stale audio bursts out.
        r.set_deafened(false);
        // Starved buffer: first pops yield nothing (jitter buffer priming).
        // Playing on an empty buffer must not decode stale packets.
        // (NoDecodeCodec would panic on Conceal, so verify Starved first.)
        feed_frames(&mut r, &spk, 0);
        // Nothing pending: any playout would be Conceal/Starved. A Starved
        // pop returns None without touching the decoder.
        let _ = r.play(&spk);
    }

    #[test]
    fn peer_and_master_gains_scale_decoded_frames() {
        let mut r = room();
        let spk = [5u8; 32];
        r.add_participant(spk).unwrap();
        r.set_peer_gain(&spk, 0.5);
        feed_frames(&mut r, &spk, 6);
        let pcm = play_until_audio(&mut r, &spk).expect("no frame played");
        assert!(pcm.iter().all(|&s| s.unsigned_abs() <= 7_500));
        assert!(pcm.iter().any(|&s| s != 0));

        // Master gain stacks with the peer gain (0.5 × 2.0 = unity).
        r.set_master_gain(2.0);
        feed_frames(&mut r, &spk, 6);
        let pcm = play_until_audio(&mut r, &spk).expect("no frame played");
        assert!(pcm.iter().any(|&s| s.unsigned_abs() > 7_500));
    }

    #[test]
    fn boosted_gain_saturates_at_i16_bounds() {
        let mut r = room();
        let spk = [6u8; 32];
        r.add_participant(spk).unwrap();
        r.set_peer_gain(&spk, 2.0);
        r.set_master_gain(2.0);
        feed_frames(&mut r, &spk, 6);
        let pcm = play_until_audio(&mut r, &spk).expect("no frame played");
        // tone(15_000) × 4 would overflow: samples must clip, never wrap.
        assert!(pcm
            .iter()
            .all(|&s| s == i16::MAX || s == i16::MIN || s == 0));
        assert!(pcm.contains(&i16::MAX));
    }

    /// Pops frames until one carries audio (skips jitter priming).
    fn play_until_audio(room: &mut VoiceRoom, from: &[u8; 32]) -> Option<Vec<i16>> {
        for _ in 0..10 {
            if let Some(pcm) = room.play(from).unwrap() {
                if pcm.iter().any(|&s| s != 0) {
                    return Some(pcm);
                }
            }
        }
        None
    }

    #[test]
    fn ducking_attenuates_playback_and_is_bounded() {
        let mut r = room();
        let spk = [8u8; 32];
        r.add_participant(spk).unwrap();
        r.set_peer_duck(&spk, 0.4);
        feed_frames(&mut r, &spk, 6);
        let pcm = play_until_audio(&mut r, &spk).expect("no frame played");
        // tone(15_000) × 0.4 = 6_000 : bien sous l'amplitude d'origine.
        assert!(pcm.iter().all(|&s| s.unsigned_abs() <= 6_200));
        assert!(pcm.iter().any(|&s| s != 0));

        // Le duck est borné à 0..=1 (jamais un boost).
        r.set_peer_duck(&spk, 5.0);
        feed_frames(&mut r, &spk, 6);
        let pcm = play_until_audio(&mut r, &spk).expect("no frame played");
        assert!(pcm.iter().all(|&s| s.unsigned_abs() <= 15_000));
    }

    #[test]
    fn capture_dsp_agc_applies_before_vad_and_encoding() {
        let mut r = room();
        r.set_agc(true);
        // Une source forte est ramenée vers la cible AGC après quelques
        // trames : l'amplitude encodée chute sous l'amplitude brute.
        let mut last: Option<Vec<i16>> = None;
        for _ in 0..30 {
            if let Some(VoiceMsg::AudioFrame { payload, .. }) = r.capture(&tone(20_000)).unwrap() {
                let mut codec = PassthroughCodec;
                last = Some(codec.decode(Some(&payload)).unwrap());
            }
        }
        let pcm = last.expect("aucune trame capturée");
        assert!(
            pcm.iter().all(|&s| s.unsigned_abs() < 10_000),
            "l'AGC n'a pas atténué la capture"
        );
        // Désactivation à chaud : la trame repart brute.
        r.set_agc(false);
        let frame = r.capture(&tone(20_000)).unwrap().expect("trame attendue");
        let VoiceMsg::AudioFrame { payload, .. } = frame else {
            panic!("trame audio attendue");
        };
        let mut codec = PassthroughCodec;
        let pcm = codec.decode(Some(&payload)).unwrap();
        assert!(pcm.iter().any(|&s| s.unsigned_abs() == 20_000));
    }

    #[test]
    fn frames_from_other_rooms_are_ignored() {
        let mut r = room();
        let spk = [3u8; 32];
        r.add_participant(spk).unwrap();
        r.on_frame(
            &spk,
            VoiceMsg::AudioFrame {
                room: [99u8; 16],
                media_type: MEDIA_AUDIO_OPUS,
                seq: 0,
                ts_ms: 0,
                payload: vec![0u8; FRAME_SAMPLES * 2],
            },
            0,
        );
        assert!(r.play(&spk).unwrap().is_none());
    }
}
