//! Plaintext des paquets DATA : démultiplexage par canal (SPEC §3) et
//! messages des canaux CONTROL (0x00), VOICE (0x03) et RELAY (0x05).

use crate::core_msg::CoreMsg;
use crate::dht_msg::DhtMessage;
use crate::file_msg::FileMsg;
use crate::limits;
use crate::types::WireAddr;
use crate::wire::{DecodeError, Reader, WireDecode, WireEncode, Writer};

/// Message du canal CONTROL (0x00).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlMsg {
    /// Sonde de vivacité.
    Ping {
        /// Jeton opaque répété dans le PONG.
        token: u64,
    },
    /// Réponse à [`ControlMsg::Ping`].
    Pong {
        /// Jeton du PING correspondant.
        token: u64,
    },
    /// Fermeture propre de session.
    Close {
        /// Code de raison (SPEC §12, informatif).
        reason: u8,
    },
    /// Annonce de passage à une nouvelle génération de clé (SPEC §2.4).
    Rekey {
        /// Nouvel epoch (ancien + 1).
        new_epoch: u8,
    },
    /// Demande d'observation d'adresse publique (SPEC §11).
    ObserveAddrReq,
    /// Adresse source observée par le pair.
    ObserveAddrResp {
        /// Adresse UDP/TCP vue par le répondeur.
        addr: WireAddr,
    },
    /// Demande de poinçonnage coordonné (SPEC §11.2) : l'émetteur communique
    /// ses candidats d'adresse frais et invite le pair à poinçonner vers eux
    /// immédiatement. Transite par un lien déjà établi (typiquement une session
    /// relayée) : le rendez-vous reste sans serveur central.
    PunchRequest {
        /// Jeton opaque répété dans la réponse (corrélation, anti-réponse
        /// non sollicitée).
        token: u64,
        /// Candidats d'adresse de l'émetteur (borné à
        /// [`limits::MAX_PUNCH_CANDIDATES`] au décodage).
        candidates: Vec<WireAddr>,
    },
    /// Réponse à [`ControlMsg::PunchRequest`] : le répondeur renvoie ses
    /// propres candidats puis poinçonne aussitôt — les deux salves se croisent.
    PunchResponse {
        /// Jeton de la demande correspondante.
        token: u64,
        /// Candidats d'adresse du répondeur (même borne qu'à la demande).
        candidates: Vec<WireAddr>,
    },
    /// Auto-annonce DHT dans une session directe (SPEC §11.3, premier
    /// contact) : porte la preuve de travail et les drapeaux de capacité du
    /// nœud émetteur. Le récepteur reconstruit un `NodeInfo` dont l'identité
    /// est celle, AUTHENTIFIÉE, de la session et l'adresse celle OBSERVÉE
    /// (jamais déclarée), puis l'insère dans sa table de routage.
    NodeAnnounce {
        /// Nonce de preuve de travail de l'identité (re-vérifié à l'insertion).
        pow_nonce: u64,
        /// Drapeaux de capacité ([`crate::types::node_flags`]).
        flags: u8,
    },
}

impl WireEncode for ControlMsg {
    fn encode(&self, w: &mut Writer) {
        match self {
            ControlMsg::Ping { token } => {
                w.put_u8(0x00);
                w.put_u64(*token);
            }
            ControlMsg::Pong { token } => {
                w.put_u8(0x01);
                w.put_u64(*token);
            }
            ControlMsg::Close { reason } => {
                w.put_u8(0x02);
                w.put_u8(*reason);
            }
            ControlMsg::Rekey { new_epoch } => {
                w.put_u8(0x03);
                w.put_u8(*new_epoch);
            }
            ControlMsg::ObserveAddrReq => w.put_u8(0x04),
            ControlMsg::ObserveAddrResp { addr } => {
                w.put_u8(0x05);
                addr.encode(w);
            }
            ControlMsg::PunchRequest { token, candidates } => {
                w.put_u8(0x06);
                w.put_u64(*token);
                w.put_list(candidates, |w, a| a.encode(w));
            }
            ControlMsg::PunchResponse { token, candidates } => {
                w.put_u8(0x07);
                w.put_u64(*token);
                w.put_list(candidates, |w, a| a.encode(w));
            }
            ControlMsg::NodeAnnounce { pow_nonce, flags } => {
                w.put_u8(0x08);
                w.put_u64(*pow_nonce);
                w.put_u8(*flags);
            }
        }
    }
}

impl WireDecode for ControlMsg {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        match r.u8()? {
            0x00 => Ok(ControlMsg::Ping { token: r.u64()? }),
            0x01 => Ok(ControlMsg::Pong { token: r.u64()? }),
            0x02 => Ok(ControlMsg::Close { reason: r.u8()? }),
            0x03 => Ok(ControlMsg::Rekey { new_epoch: r.u8()? }),
            0x04 => Ok(ControlMsg::ObserveAddrReq),
            0x05 => Ok(ControlMsg::ObserveAddrResp {
                addr: WireAddr::decode(r)?,
            }),
            0x06 => Ok(ControlMsg::PunchRequest {
                token: r.u64()?,
                candidates: r.list(
                    limits::MAX_PUNCH_CANDIDATES,
                    "punch.candidates",
                    WireAddr::decode,
                )?,
            }),
            0x07 => Ok(ControlMsg::PunchResponse {
                token: r.u64()?,
                candidates: r.list(
                    limits::MAX_PUNCH_CANDIDATES,
                    "punch.candidates",
                    WireAddr::decode,
                )?,
            }),
            0x08 => Ok(ControlMsg::NodeAnnounce {
                pow_nonce: r.u64()?,
                flags: r.u8()?,
            }),
            _ => Err(DecodeError::InvalidValue("control kind")),
        }
    }
}

/// Message du canal VOICE (0x03) — SPEC §8.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VoiceMsg {
    /// Trame audio Opus de 20 ms.
    AudioFrame {
        /// Identifiant du salon vocal.
        room: [u8; 16],
        /// Type de média (0x01 = audio Opus ; réservé vidéo/écran).
        media_type: u8,
        /// Numéro de séquence (détection de perte).
        seq: u16,
        /// Horodatage média relatif en millisecondes.
        ts_ms: u32,
        /// Trame Opus encodée.
        payload: Vec<u8>,
    },
    /// Retour de qualité pour l'adaptation de débit.
    VoicePing {
        /// Perte mesurée en pourcent (0–100).
        loss_pct: u8,
        /// RTT estimé en millisecondes.
        rtt_ms: u16,
    },
    /// Fragment d'une trame vidéo de partage d'écran (v5). La trame encodée
    /// (WebCodecs, VP8/H.264) est découpée par l'émetteur en tranches
    /// ≤ [`MAX_SCREEN_FRAGMENT`], chacune portée par un `ScreenFrame` dans un
    /// unique datagramme UDP (jamais refragmenté par le transport). Le
    /// récepteur réassemble par `frame_id` et ne garde QUE la trame la plus
    /// récente — sémantique temps réel : une trame incomplète est abandonnée
    /// dès qu'une plus récente arrive. Voie best-effort : un fragment perdu
    /// jette la trame et l'on attend la keyframe suivante.
    ScreenFrame {
        /// Salon (== `call_id` de l'appel 1-à-1).
        room: [u8; 16],
        /// Identifiant de trame, croissant chez l'émetteur (clé de réassemblage).
        frame_id: u32,
        /// Nombre total de fragments de cette trame (≥ 1).
        frag_count: u16,
        /// Position 0-based de ce fragment (`< frag_count`).
        frag_idx: u16,
        /// Drapeaux : bit 0 = keyframe (image décodable indépendamment).
        flags: u8,
        /// Tranche encodée (≤ [`MAX_SCREEN_FRAGMENT`]).
        payload: Vec<u8>,
    },
    /// Démarrage (`on == true`) ou arrêt (`on == false`) d'un partage d'écran
    /// dans l'appel `room`. Éphémère (canal VOICE, best-effort) ; l'arrêt est
    /// aussi déduit d'une absence prolongée de `ScreenFrame`.
    ScreenControl {
        /// Salon (== `call_id`).
        room: [u8; 16],
        /// Vrai = partage démarré, faux = arrêté.
        on: bool,
    },
}

const MAX_OPUS_FRAME: usize = 1024;

/// Drapeau `ScreenFrame::flags` : la tranche appartient à une keyframe.
pub const SCREEN_FLAG_KEYFRAME: u8 = 0x01;

/// Taille maximale d'une tranche portée par un [`VoiceMsg::ScreenFrame`] :
/// bornée pour tenir, une fois scellée, dans un unique datagramme UDP
/// (`UDP_MTU`) — le fragmenteur écran (accord-node) ne dépasse jamais cette
/// taille, la borne au décodage n'est qu'un garde-fou anti-DoS.
const MAX_SCREEN_FRAGMENT: usize = 1200;

impl WireEncode for VoiceMsg {
    fn encode(&self, w: &mut Writer) {
        match self {
            VoiceMsg::AudioFrame {
                room,
                media_type,
                seq,
                ts_ms,
                payload,
            } => {
                w.put_u8(0x01);
                w.put_arr(room);
                w.put_u8(*media_type);
                w.put_u16(*seq);
                w.put_u32(*ts_ms);
                w.put_vbytes(payload);
            }
            VoiceMsg::VoicePing { loss_pct, rtt_ms } => {
                w.put_u8(0x02);
                w.put_u8(*loss_pct);
                w.put_u16(*rtt_ms);
            }
            VoiceMsg::ScreenFrame {
                room,
                frame_id,
                frag_count,
                frag_idx,
                flags,
                payload,
            } => {
                w.put_u8(0x03);
                w.put_arr(room);
                w.put_u32(*frame_id);
                w.put_u16(*frag_count);
                w.put_u16(*frag_idx);
                w.put_u8(*flags);
                w.put_vbytes(payload);
            }
            VoiceMsg::ScreenControl { room, on } => {
                w.put_u8(0x04);
                w.put_arr(room);
                w.put_u8(u8::from(*on));
            }
        }
    }
}

impl WireDecode for VoiceMsg {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        match r.u8()? {
            0x01 => Ok(VoiceMsg::AudioFrame {
                room: r.arr()?,
                media_type: r.u8()?,
                seq: r.u16()?,
                ts_ms: r.u32()?,
                payload: r.vbytes(MAX_OPUS_FRAME, "voice.payload")?,
            }),
            0x02 => Ok(VoiceMsg::VoicePing {
                loss_pct: r.u8()?,
                rtt_ms: r.u16()?,
            }),
            0x03 => Ok(VoiceMsg::ScreenFrame {
                room: r.arr()?,
                frame_id: r.u32()?,
                frag_count: r.u16()?,
                frag_idx: r.u16()?,
                flags: r.u8()?,
                payload: r.vbytes(MAX_SCREEN_FRAGMENT, "voice.screen_payload")?,
            }),
            0x04 => Ok(VoiceMsg::ScreenControl {
                room: r.arr()?,
                on: r.u8()? != 0,
            }),
            _ => Err(DecodeError::InvalidValue("voice kind")),
        }
    }
}

/// Message du canal RELAY (0x05) — SPEC §10.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelayMsg {
    /// Demande d'ouverture de circuit vers un nœud cible.
    Open {
        /// NodeId de la cible.
        target: [u8; 32],
    },
    /// Circuit accepté par le relais.
    Accept {
        /// Identifiant de circuit attribué.
        circuit: u32,
    },
    /// Refus d'ouverture (cible injoignable, quota).
    Reject {
        /// Code d'erreur (SPEC §12).
        code: u8,
    },
    /// Blob opaque acheminé sur un circuit (paquet DATA bout-en-bout).
    Data {
        /// Identifiant de circuit.
        circuit: u32,
        /// Paquet chiffré de la session bout-en-bout.
        blob: Vec<u8>,
    },
    /// Fermeture de circuit.
    Close {
        /// Identifiant de circuit.
        circuit: u32,
    },
}

impl WireEncode for RelayMsg {
    fn encode(&self, w: &mut Writer) {
        match self {
            RelayMsg::Open { target } => {
                w.put_u8(0x01);
                w.put_arr(target);
            }
            RelayMsg::Accept { circuit } => {
                w.put_u8(0x02);
                w.put_u32(*circuit);
            }
            RelayMsg::Reject { code } => {
                w.put_u8(0x05);
                w.put_u8(*code);
            }
            RelayMsg::Data { circuit, blob } => {
                w.put_u8(0x03);
                w.put_u32(*circuit);
                w.put_lbytes(blob);
            }
            RelayMsg::Close { circuit } => {
                w.put_u8(0x04);
                w.put_u32(*circuit);
            }
        }
    }
}

impl WireDecode for RelayMsg {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        match r.u8()? {
            0x01 => Ok(RelayMsg::Open { target: r.arr()? }),
            0x02 => Ok(RelayMsg::Accept { circuit: r.u32()? }),
            0x05 => Ok(RelayMsg::Reject { code: r.u8()? }),
            0x03 => Ok(RelayMsg::Data {
                circuit: r.u32()?,
                blob: r.lbytes(limits::MAX_TCP_FRAME, "relay.blob")?,
            }),
            0x04 => Ok(RelayMsg::Close { circuit: r.u32()? }),
            _ => Err(DecodeError::InvalidValue("relay kind")),
        }
    }
}

/// Plaintext complet d'un paquet DATA, démultiplexé par canal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelMsg {
    /// Canal 0x00 : contrôle de session.
    Control(ControlMsg),
    /// Canal 0x01 : RPC Kademlia.
    Dht(DhtMessage),
    /// Canal 0x02 : messagerie, groupes, présence.
    Core(CoreMsg),
    /// Canal 0x03 : audio temps réel.
    Voice(VoiceMsg),
    /// Canal 0x04 : transfert de fichiers.
    File(FileMsg),
    /// Canal 0x05 : relais de repli.
    Relay(RelayMsg),
}

impl WireEncode for ChannelMsg {
    fn encode(&self, w: &mut Writer) {
        match self {
            ChannelMsg::Control(m) => {
                w.put_u8(0x00);
                m.encode(w);
            }
            ChannelMsg::Dht(m) => {
                w.put_u8(0x01);
                m.encode(w);
            }
            ChannelMsg::Core(m) => {
                w.put_u8(0x02);
                m.encode(w);
            }
            ChannelMsg::Voice(m) => {
                w.put_u8(0x03);
                m.encode(w);
            }
            ChannelMsg::File(m) => {
                w.put_u8(0x04);
                m.encode(w);
            }
            ChannelMsg::Relay(m) => {
                w.put_u8(0x05);
                m.encode(w);
            }
        }
    }
}

impl WireDecode for ChannelMsg {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        match r.u8()? {
            0x00 => Ok(ChannelMsg::Control(ControlMsg::decode(r)?)),
            0x01 => Ok(ChannelMsg::Dht(DhtMessage::decode(r)?)),
            0x02 => Ok(ChannelMsg::Core(CoreMsg::decode(r)?)),
            0x03 => Ok(ChannelMsg::Voice(VoiceMsg::decode(r)?)),
            0x04 => Ok(ChannelMsg::File(FileMsg::decode(r)?)),
            0x05 => Ok(ChannelMsg::Relay(RelayMsg::decode(r)?)),
            _ => Err(DecodeError::InvalidValue("channel")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn voice_roundtrip(msg: &VoiceMsg) -> VoiceMsg {
        let mut w = Writer::new();
        msg.encode(&mut w);
        let bytes = w.into_bytes();
        let mut r = Reader::new(&bytes);
        let out = VoiceMsg::decode(&mut r).expect("decode");
        r.finish().expect("no trailing bytes");
        out
    }

    #[test]
    fn screen_frame_roundtrips() {
        let msg = VoiceMsg::ScreenFrame {
            room: [0x5A; 16],
            frame_id: 0xDEAD_BEEF,
            frag_count: 7,
            frag_idx: 3,
            flags: SCREEN_FLAG_KEYFRAME,
            payload: vec![1, 2, 3, 4, 5],
        };
        assert_eq!(voice_roundtrip(&msg), msg);
    }

    #[test]
    fn screen_frame_travels_inside_channel_msg() {
        let inner = VoiceMsg::ScreenFrame {
            room: [9; 16],
            frame_id: 42,
            frag_count: 1,
            frag_idx: 0,
            flags: 0,
            payload: vec![0xAB; 900],
        };
        let mut w = Writer::new();
        ChannelMsg::Voice(inner.clone()).encode(&mut w);
        let bytes = w.into_bytes();
        let mut r = Reader::new(&bytes);
        assert_eq!(
            ChannelMsg::decode(&mut r).expect("decode"),
            ChannelMsg::Voice(inner)
        );
    }

    #[test]
    fn screen_control_roundtrips_both_states() {
        for on in [true, false] {
            let msg = VoiceMsg::ScreenControl { room: [1; 16], on };
            assert_eq!(voice_roundtrip(&msg), msg);
        }
    }

    #[test]
    fn oversized_screen_fragment_is_rejected_cleanly() {
        // A fragment payload beyond the anti-DoS bound fails to decode without
        // panicking.
        let mut w = Writer::new();
        w.put_u8(0x03);
        w.put_arr(&[0u8; 16]);
        w.put_u32(1);
        w.put_u16(1);
        w.put_u16(0);
        w.put_u8(0);
        w.put_vbytes(&vec![0u8; 2000]);
        let bytes = w.into_bytes();
        let mut r = Reader::new(&bytes);
        assert!(VoiceMsg::decode(&mut r).is_err());
    }
}
