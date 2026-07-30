//! Moteur voix : tâche unique cadencée à 20 ms qui possède le salon actif
//! ([`VoiceRoom`]), le codec et les rosters de présence, et exécute les
//! commandes du [`super::VoiceHandle`].
//!
//! Trames et pings partent directement dans les sessions chiffrées (canal
//! VOICE, [`super::FrameSender`]) ; la signalisation `VoiceSignal` passe par
//! le canal CORE ([`crate::outbound`]). Tout signal entrant est re-validé :
//! seuls les membres du groupe peuvent peupler un salon.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use accord_api::NotificationHub;
use accord_core::group::GroupState;
use accord_proto::core_msg::CoreMsg;
use accord_proto::limits::VOICE_MAX_PARTICIPANTS;
use accord_proto::plaintext::{VoiceMsg, VIDEO_FLAG_KEYFRAME};
use accord_voice::gain;
use accord_voice::params::{FRAME_MS, FRAME_SAMPLES};
use accord_voice::room::CodecFactory;
use accord_voice::{Pcm8Codec, VoiceRoom};
use rand::RngCore;
use serde_json::json;
use tokio::sync::mpsc;

use super::calls::{CallAction, CallMachine, CallPhase};
use super::interest::{DeclaredHidden, HiddenByPeer, REFRESH_PERIOD};
use super::media::VideoStream;
use super::roster::{Roster, RosterEvent, ACTIVE_TIMEOUT_MS, PASSIVE_TTL_MS};
use super::{
    Cmd, FrameSender, VoiceBackend, VoiceDeps, VoiceDevices, VoiceParticipant, VoiceRoomPresence,
    VoiceStatus,
};
use crate::error::NodeError;
use crate::hex;
use crate::node::Node;
use crate::outbound::{Outbound, OutboundSink};

/// Action `VoiceSignal` : rejoint le salon.
const ACTION_JOIN: u8 = 0;
/// Action `VoiceSignal` : quitte le salon.
const ACTION_LEAVE: u8 = 1;
/// Action `VoiceSignal` : présence (réponse à un join, rafraîchissement).
const ACTION_STATE: u8 = 2;
/// Bitflag média : audio.
const MEDIA_AUDIO: u8 = 0x01;
/// `media_kinds` bitflag carrying the sender's deafen state (SPEC §6:
/// receivers ignore unknown bits, which keeps the wire backward compatible).
const MEDIA_DEAFENED: u8 = 0x80;

/// Un ping de qualité par participant chaque seconde (50 trames de 20 ms).
const PING_PERIOD_TICKS: u64 = 50;
/// Diffusion d'état aux membres du groupe toutes les 30 s (rafraîchit la
/// présence passive des membres hors salon).
const STATE_PERIOD_TICKS: u64 = 1_500;
/// Balayage des rosters passifs chaque seconde.
const PASSIVE_SWEEP_TICKS: u64 = 50;
/// Salons dont la présence est suivie au plus (le salon rejoint et les
/// présences passives réunis). Borne mémoire du suivi passif, très au-dessus
/// de tout usage réel — voir la garde dans `handle_peer_signal`.
const MAX_PASSIVE_ROOMS: usize = 512;
/// Réaffirmation des masques de vidéo sélective (voir [`REFRESH_PERIOD`]),
/// exprimée en trames de 20 ms. Portée par le moteur et non par l'UI : la
/// déclaration doit survivre à un onglet inactif ou à une UI qui ne re-rend
/// plus, sans quoi l'expiration rallumerait des flux qu'on masque toujours.
const INTEREST_REFRESH_TICKS: u64 = REFRESH_PERIOD.as_millis() as u64 / FRAME_MS as u64;
/// Trames de capture injectées en attente au maximum.
const MAX_INJECTED_FRAMES: usize = 64;
/// Une émission `event.voice_level` toutes les 5 trames de 20 ms (~10 Hz).
#[cfg(feature = "hardware")]
const LEVEL_PERIOD_TICKS: u64 = 5;

/// `group_id` sentinelle des sessions audio d'appel 1-à-1 (les identifiants
/// de groupe réels sont tirés aléatoirement : la collision est négligeable).
const CALL_GROUP_SENTINEL: [u8; 16] = [0u8; 16];

/// Atténuation appliquée aux participants non prioritaires pendant qu'un
/// orateur prioritaire parle (priority speaker, ≈ −10 dB).
const PRIORITY_DUCK: f32 = 0.3;

/// Drapeaux de modération vocale et de priorité d'un participant du salon de
/// groupe actif (repli de l'op-log, cache rafraîchi sur changement).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct ModFlags {
    /// Micro forcé coupé par un modérateur (op 0x1F).
    muted: bool,
    /// Sortie forcée coupée par un modérateur (op 0x1F).
    deafened: bool,
    /// Porteur de la permission `PRIORITY_SPEAKER`.
    priority: bool,
}

/// Drapeaux de modération/priorité de `pk` d'après l'état replié du groupe.
fn mod_flags_of(state: &GroupState, pk: &[u8; 32]) -> ModFlags {
    let moderation = state.voice_moderation_of(pk);
    ModFlags {
        muted: moderation.mute,
        deafened: moderation.deafen,
        priority: state.is_priority_speaker(pk),
    }
}

/// Identifiant d'appel frais (16 octets aléatoires).
fn new_call_id() -> [u8; 16] {
    let mut id = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut id);
    id
}

/// Erreur uniforme quand la capture réelle n'existe pas (mode simulé ou
/// feature `hardware` absente) — message du contrat gelé (D-029).
fn audio_unavailable() -> NodeError {
    NodeError::Audio("matériel audio indisponible".into())
}

/// Convertit une erreur d'E/S audio en erreur du nœud.
#[cfg(feature = "hardware")]
fn audio_error(e: accord_voice::IoError) -> NodeError {
    NodeError::Audio(e.to_string())
}

/// Identifiant d'un salon : (groupe, salon).
type RoomKey = ([u8; 16], [u8; 16]);

/// Salon vocal actif (celui que l'on a rejoint) ou session audio d'appel.
struct Active {
    group_id: [u8; 16],
    channel_id: [u8; 16],
    /// Session audio d'un appel 1-à-1 (`group_id` sentinelle,
    /// `channel_id == call_id`) : pas de signalisation de groupe.
    is_call: bool,
    /// Effective mute (forced to `true` while deafened).
    muted: bool,
    /// Local output deafened (session-scoped, never persisted).
    deafened: bool,
    /// Last user-requested mute state, restored on undeafen.
    mute_restore: bool,
    room: VoiceRoom,
}

impl Active {
    fn key(&self) -> RoomKey {
        (self.group_id, self.channel_id)
    }
}

/// Fabrique de codecs selon le mode d'exécution.
fn codec_factory(backend: VoiceBackend) -> CodecFactory {
    match backend {
        VoiceBackend::Simule => Box::new(|| Box::new(Pcm8Codec)),
        VoiceBackend::Materiel => materiel_codec_factory(),
    }
}

/// Codec du mode matériel : Opus si la feature `hardware` est compilée
/// (repli PCM 8 bits si l'initialisation échoue), PCM 8 bits sinon.
#[cfg(feature = "hardware")]
fn materiel_codec_factory() -> CodecFactory {
    Box::new(
        || match accord_voice::OpusCodec::new(accord_voice::params::BITRATE_MIN) {
            Ok(codec) => Box::new(codec),
            Err(e) => {
                tracing::warn!(erreur = %e, "voix : codec Opus indisponible, repli PCM 8 bits");
                Box::new(Pcm8Codec)
            }
        },
    )
}

#[cfg(not(feature = "hardware"))]
fn materiel_codec_factory() -> CodecFactory {
    Box::new(|| Box::new(Pcm8Codec))
}

/// Moteur voix (une instance par nœud, tâche unique).
pub(crate) struct Engine {
    node: Arc<Node>,
    outbound: OutboundSink,
    hub: Option<NotificationHub>,
    sender: Arc<dyn FrameSender>,
    backend: VoiceBackend,
    rx: mpsc::UnboundedReceiver<Cmd>,
    /// Présence connue par salon (y compris les salons non rejoints).
    rooms: HashMap<RoomKey, Roster>,
    active: Option<Active>,
    /// Capture de substitution (mode simulé / tests).
    injected: VecDeque<Vec<i16>>,
    /// Master output volume in percent (persisted, 100 = unity).
    master_volume: u16,
    /// Per-peer output volumes in percent (cache over the persisted values).
    peer_volumes: HashMap<[u8; 32], u16>,
    /// Périphérique d'entrée choisi (`None` = défaut ; persisté, D-029).
    input_device: Option<String>,
    /// Périphérique de sortie choisi (`None` = défaut ; persisté, D-029).
    output_device: Option<String>,
    /// Machine d'états des appels 1-à-1 (sonnerie, occupé, timeout).
    calls: CallMachine,
    /// Modération vocale et priorité des participants du salon de GROUPE
    /// actif (vide pour un appel ; rafraîchi à l'entrée et sur op de groupe).
    mod_flags: HashMap<[u8; 32], ModFlags>,
    /// Suppression de bruit de capture (persistée, appliquée à chaud).
    dsp_noise_suppression: bool,
    /// Contrôle automatique de gain de capture (persisté, appliqué à chaud).
    dsp_agc: bool,
    /// Annulation d'écho acoustique (persistée, appliquée à chaud, D-051).
    dsp_echo_cancel: bool,
    /// Annuleur d'écho : capture nettoyée de ce que la sortie vient de jouer
    /// (référence far-end = trame mixée poussée au haut-parleur, silence
    /// compris — la chronologie doit suivre la lecture). Remis à neuf à
    /// chaque session et au changement de périphérique (le délai change).
    aec: accord_voice::EchoCanceller,
    #[cfg(feature = "hardware")]
    hw: Option<super::hw::HardwareIo>,
    /// Test micro en cours (`event.voice_level` à ~10 Hz, D-029).
    #[cfg(feature = "hardware")]
    mic_test: Option<MicTest>,
    epoch: Instant,
    tick_count: u64,
    /// Réassembleurs des trames vidéo reçues, par (pair émetteur, flux) : les
    /// deux flux d'un même pair sont indépendants. Vidés à la fin de l'appel.
    video_rx: HashMap<([u8; 32], VideoStream), super::media::Reassembler>,
    /// Compteur de trame sortante par flux (clé de réassemblage côté récepteur).
    video_next_frame_id: HashMap<VideoStream, u32>,
    /// Vidéo sélective, côté ÉMETTEUR : ce que chaque destinataire déclare ne
    /// pas afficher de nos flux. Vide = on émet tout, comme avant.
    video_hidden: HiddenByPeer,
    /// Vidéo sélective, côté RÉCEPTEUR : ce que l'on a déclaré à chaque
    /// émetteur (pour n'émettre que les changements et réaffirmer le reste).
    video_declared: DeclaredHidden,
}

/// Test micro actif : capture dédiée, VAD et crête de niveau (D-029).
#[cfg(feature = "hardware")]
struct MicTest {
    io: super::hw::MicCapture,
    vad: accord_voice::Vad,
    /// Crête RMS (0..1) observée depuis la dernière émission.
    peak: f32,
    /// État « parle » de la VAD (hystérésis) à la dernière trame.
    speaking: bool,
    /// Trames de 20 ms écoulées depuis l'activation.
    ticks: u64,
}

#[cfg(feature = "hardware")]
impl MicTest {
    fn new(io: super::hw::MicCapture) -> Self {
        Self {
            io,
            vad: accord_voice::Vad::default(),
            peak: 0.0,
            speaking: false,
            ticks: 0,
        }
    }
}

impl Engine {
    /// Assemble le moteur (voir [`super::spawn`]).
    pub(crate) fn new(deps: VoiceDeps, rx: mpsc::UnboundedReceiver<Cmd>) -> Self {
        // Choix de périphériques persisté (D-029) ; illisible = défauts.
        let (input_device, output_device) =
            deps.node.voice_devices_config().unwrap_or_else(|e| {
                tracing::warn!(erreur = %e, "voix : choix de périphériques illisible, défauts appliqués");
                (None, None)
            });
        // Volume principal persisté ; illisible = 100 %.
        let master_volume = deps.node.voice_master_volume().unwrap_or_else(|e| {
            tracing::warn!(erreur = %e, "voix : volume principal illisible, défaut appliqué");
            gain::VOLUME_DEFAULT_PCT
        });
        // Réglages DSP persistés ; illisibles = défauts (annulation d'écho
        // active, le reste désactivé).
        let (dsp_noise_suppression, dsp_agc, dsp_echo_cancel) =
            deps.node.voice_dsp_config().unwrap_or_else(|e| {
                tracing::warn!(erreur = %e, "voix : réglages DSP illisibles, défauts appliqués");
                (false, false, true)
            });
        let me = deps.node.public_key();
        Self {
            node: deps.node,
            outbound: deps.outbound,
            hub: deps.hub,
            sender: deps.sender,
            backend: deps.backend,
            rx,
            rooms: HashMap::new(),
            active: None,
            injected: VecDeque::new(),
            master_volume,
            peer_volumes: HashMap::new(),
            input_device,
            output_device,
            calls: CallMachine::new(me),
            mod_flags: HashMap::new(),
            dsp_noise_suppression,
            dsp_agc,
            dsp_echo_cancel,
            aec: accord_voice::EchoCanceller::new(),
            #[cfg(feature = "hardware")]
            hw: None,
            #[cfg(feature = "hardware")]
            mic_test: None,
            epoch: Instant::now(),
            tick_count: 0,
            video_rx: HashMap::new(),
            video_next_frame_id: HashMap::new(),
            video_hidden: HiddenByPeer::default(),
            video_declared: DeclaredHidden::default(),
        }
    }

    /// Boucle principale : commandes + cadence de 20 ms.
    pub(crate) async fn run(mut self) {
        let mut tick = tokio::time::interval(Duration::from_millis(u64::from(FRAME_MS)));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                cmd = self.rx.recv() => match cmd {
                    None | Some(Cmd::Stop) => break,
                    Some(cmd) => self.handle_cmd(cmd).await,
                },
                _ = tick.tick() => self.on_tick().await,
            }
        }
        // Arrêt : on quitte proprement le salon actif (signal de départ).
        self.leave_active();
    }

    /// Millisecondes écoulées depuis le démarrage du moteur (horloge média).
    fn now_ms(&self) -> u64 {
        self.epoch.elapsed().as_millis() as u64
    }

    async fn handle_cmd(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::Join {
                group_id,
                channel_id,
                resp,
            } => {
                let _ = resp.send(self.handle_join(group_id, channel_id));
            }
            Cmd::Leave { resp } => {
                self.leave_or_hangup();
                let _ = resp.send(());
            }
            Cmd::Mute { muted, resp } => {
                self.handle_mute(muted);
                let _ = resp.send(());
            }
            Cmd::Deafen { deafened, resp } => {
                self.handle_deafen(deafened);
                let _ = resp.send(());
            }
            Cmd::SetVolume { peer, volume, resp } => {
                let _ = resp.send(self.handle_set_volume(peer, volume));
            }
            Cmd::MasterVolume { resp } => {
                let _ = resp.send(self.master_volume);
            }
            Cmd::Status { resp } => {
                let _ = resp.send(self.handle_status());
            }
            Cmd::Rooms { resp } => {
                let _ = resp.send(self.handle_rooms());
            }
            Cmd::Devices { resp } => {
                let _ = resp.send(self.handle_devices());
            }
            Cmd::SetDevices {
                input,
                output,
                resp,
            } => {
                let _ = resp.send(self.handle_set_devices(input, output).await);
            }
            Cmd::MicTest { enabled, resp } => {
                let _ = resp.send(self.handle_mic_test(enabled).await);
            }
            Cmd::CallStart { peer, resp } => {
                let _ = resp.send(self.handle_call_start(peer));
            }
            Cmd::CallAccept { call_id, resp } => {
                let result = self
                    .calls
                    .accept(call_id, self.now_ms())
                    .map_err(NodeError::Invalid);
                let _ = resp.send(match result {
                    Ok(actions) => {
                        self.run_call_actions(actions);
                        Ok(())
                    }
                    Err(e) => Err(e),
                });
            }
            Cmd::CallDecline { call_id, resp } => {
                let result = self.calls.decline(call_id).map_err(NodeError::Invalid);
                let _ = resp.send(match result {
                    Ok(actions) => {
                        self.run_call_actions(actions);
                        Ok(())
                    }
                    Err(e) => Err(e),
                });
            }
            Cmd::CallHangup { resp } => {
                let actions = self.calls.hangup();
                self.run_call_actions(actions);
                let _ = resp.send(());
            }
            Cmd::CallStatus { resp } => {
                let _ = resp.send(self.calls.snapshot());
            }
            Cmd::PeerCall { from, msg } => self.handle_peer_call(from, msg),
            Cmd::SetDsp {
                noise_suppression,
                agc,
                echo_cancel,
                resp,
            } => {
                let _ = resp.send(self.handle_set_dsp(noise_suppression, agc, echo_cancel));
            }
            Cmd::DspConfig { resp } => {
                let _ = resp.send((
                    self.dsp_noise_suppression,
                    self.dsp_agc,
                    self.dsp_echo_cancel,
                ));
            }
            Cmd::GroupChanged { group_id } => self.refresh_moderation(group_id),
            Cmd::PeerSignal {
                from,
                group_id,
                channel_id,
                action,
                media_kinds,
                mute,
            } => self.handle_peer_signal(from, group_id, channel_id, action, media_kinds, mute),
            Cmd::PeerFrame { from, msg } => self.handle_peer_frame(from, msg),
            Cmd::InjectPcm { pcm } => {
                if pcm.len() == FRAME_SAMPLES && self.injected.len() < MAX_INJECTED_FRAMES {
                    self.injected.push_back(pcm);
                }
            }
            Cmd::VideoSend {
                stream,
                keyframe,
                encoded,
            } => self.handle_video_send(stream, keyframe, encoded).await,
            Cmd::VideoAnnounce { stream, on } => self.handle_video_announce(stream, on).await,
            Cmd::VideoInterest { hidden } => self.handle_video_interest(hidden).await,
            // `Stop` est intercepté par la boucle avant d'arriver ici. S'il y
            // parvenait malgré tout, l'ignorer laisse le moteur vivant : une
            // panique dans l'acteur voix emporterait tout le sous-système
            // sonore pour une commande sans effet.
            Cmd::Stop => tracing::debug!("voix : Stop reçu hors de la boucle, ignoré"),
        }
    }

    /// Rejoint un salon ; quitte l'ancien implicitement (contrat gelé). Un
    /// appel 1-à-1 ACTIF est raccroché (le salon prend la session audio) ;
    /// une simple sonnerie survit.
    fn handle_join(
        &mut self,
        group_id: [u8; 16],
        channel_id: [u8; 16],
    ) -> Result<Vec<[u8; 32]>, NodeError> {
        let takeover = self.calls.on_room_takeover();
        self.run_call_actions(takeover);
        let me = self.node.public_key();
        let state = self
            .node
            .group_state(&group_id)
            .map_err(|_| NodeError::NotFound("groupe inconnu"))?;
        if !state.is_member(&me) {
            return Err(NodeError::Invalid("non membre du groupe"));
        }
        let key = (group_id, channel_id);
        if self.active.as_ref().map(Active::key) == Some(key) {
            // Déjà dans ce salon : idempotent.
            return Ok(self
                .rooms
                .get(&key)
                .map(Roster::pubkeys)
                .unwrap_or_default());
        }
        // Plafond full mesh vérifié avant toute mutation.
        if let Some(roster) = self.rooms.get(&key) {
            if !roster.contains(&me) && roster.len() >= VOICE_MAX_PARTICIPANTS {
                return Err(NodeError::Invalid("salon vocal plein (10 participants)"));
            }
        }
        self.leave_active();

        let now = self.now_ms();
        let mut room = VoiceRoom::new(channel_id, codec_factory(self.backend));
        room.set_master_gain(gain::gain_of_pct(self.master_volume));
        room.set_noise_suppression(self.dsp_noise_suppression);
        room.set_agc(self.dsp_agc);
        let mut events = Vec::new();
        let (existing, participants) = {
            let roster = self.rooms.entry(key).or_default();
            // Grâce de vivacité : les entrées passives repartent d'un délai
            // plein (elles seront confirmées par trames/pings, ou expireront).
            roster.refresh_all(now);
            let existing = roster.pubkeys();
            if roster.join(me, now).unwrap_or(false) {
                events.push(RosterEvent::Joined(me));
            }
            (existing, roster.pubkeys())
        };
        for pk in &existing {
            let _ = room.add_participant(*pk);
            let volume = self.volume_for(pk);
            room.set_peer_gain(pk, gain::gain_of_pct(volume));
        }
        for event in &events {
            self.emit_room(key, event);
        }
        self.broadcast_signal(group_id, channel_id, ACTION_JOIN, false, false);
        #[cfg(feature = "hardware")]
        if self.backend == VoiceBackend::Materiel {
            // Le salon prend la main sur la capture : fin du test micro.
            self.mic_test = None;
            // Session neuve : annuleur d'écho neuf (délai et filtre à zéro).
            self.aec = accord_voice::EchoCanceller::new();
            self.hw = Some(super::hw::HardwareIo::open(
                self.input_device.clone(),
                self.output_device.clone(),
            ));
        }
        self.active = Some(Active {
            group_id,
            channel_id,
            is_call: false,
            muted: false,
            deafened: false,
            mute_restore: false,
            room,
        });
        self.refresh_moderation(group_id);
        Ok(participants)
    }

    /// Démarre la session audio d'un appel accepté : salon dédié
    /// (`room == call_id`), participants figés à { moi, pair }, aucune
    /// signalisation de groupe.
    fn join_call_room(&mut self, peer: [u8; 32], call_id: [u8; 16]) {
        self.leave_active();
        let me = self.node.public_key();
        let now = self.now_ms();
        let mut room = VoiceRoom::new(call_id, codec_factory(self.backend));
        room.set_master_gain(gain::gain_of_pct(self.master_volume));
        room.set_noise_suppression(self.dsp_noise_suppression);
        room.set_agc(self.dsp_agc);
        let volume = self.volume_for(&peer);
        let _ = room.add_participant(peer);
        room.set_peer_gain(&peer, gain::gain_of_pct(volume));
        let key = (CALL_GROUP_SENTINEL, call_id);
        let mut events = Vec::new();
        {
            let roster = self.rooms.entry(key).or_default();
            if roster.join(me, now).unwrap_or(false) {
                events.push(RosterEvent::Joined(me));
            }
            if roster.join(peer, now).unwrap_or(false) {
                events.push(RosterEvent::Joined(peer));
            }
        }
        for event in &events {
            self.emit_room(key, event);
        }
        #[cfg(feature = "hardware")]
        if self.backend == VoiceBackend::Materiel {
            // La session d'appel prend la main sur la capture.
            self.mic_test = None;
            // Session neuve : annuleur d'écho neuf (délai et filtre à zéro).
            self.aec = accord_voice::EchoCanceller::new();
            self.hw = Some(super::hw::HardwareIo::open(
                self.input_device.clone(),
                self.output_device.clone(),
            ));
        }
        self.mod_flags.clear();
        self.active = Some(Active {
            group_id: CALL_GROUP_SENTINEL,
            channel_id: call_id,
            is_call: true,
            muted: false,
            deafened: false,
            mute_restore: false,
            room,
        });
    }

    /// `voice.leave` : quitter la session active. Pour la session audio d'un
    /// appel, quitter = raccrocher (signalisation d'appel comprise).
    fn leave_or_hangup(&mut self) {
        if self.active.as_ref().is_some_and(|a| a.is_call) {
            let actions = self.calls.hangup();
            self.run_call_actions(actions);
            return;
        }
        self.leave_active();
    }

    /// Persisted output volume of a peer, through the in-engine cache.
    fn volume_for(&mut self, pubkey: &[u8; 32]) -> u16 {
        if let Some(volume) = self.peer_volumes.get(pubkey) {
            return *volume;
        }
        let volume = self.node.voice_peer_volume(pubkey).unwrap_or_else(|e| {
            tracing::debug!(erreur = %e, "voix : volume d'un pair illisible, défaut appliqué");
            gain::VOLUME_DEFAULT_PCT
        });
        self.peer_volumes.insert(*pubkey, volume);
        volume
    }

    /// Quitte le salon actif : signal de départ (salons de groupe
    /// uniquement), événements, libération du matériel. Sans effet hors
    /// salon. La présence d'un salon d'appel est retirée entièrement (aucune
    /// notion de présence passive pour un appel terminé).
    fn leave_active(&mut self) {
        // Les flux vidéo sont liés à la session : on abandonne tout
        // réassemblage en cours à la fin de l'appel, et l'on oublie les
        // masques de vidéo sélective des deux côtés — la session suivante
        // repart de « tout le monde reçoit tout ».
        self.video_rx.clear();
        self.video_hidden.clear();
        self.video_declared.clear();
        let Some(active) = self.active.take() else {
            return;
        };
        let key = active.key();
        if !active.is_call {
            self.broadcast_signal(
                active.group_id,
                active.channel_id,
                ACTION_LEAVE,
                active.muted,
                active.deafened,
            );
        }
        self.mod_flags.clear();
        let me = self.node.public_key();
        let mut events = Vec::new();
        if let Some(roster) = self.rooms.get_mut(&key) {
            if let Some(event) = roster.force_silent(&me) {
                events.push(event);
            }
            if roster.leave(&me) {
                events.push(RosterEvent::Left(me));
            }
            if active.is_call {
                // Fin d'appel : le pair sort du roster avec nous.
                let peer_left: Vec<[u8; 32]> = roster.pubkeys();
                for pk in peer_left {
                    if roster.leave(&pk) {
                        events.push(RosterEvent::Left(pk));
                    }
                }
            }
            if roster.is_empty() {
                self.rooms.remove(&key);
            }
        }
        for event in &events {
            self.emit_room(key, event);
        }
        #[cfg(feature = "hardware")]
        {
            self.hw = None;
        }
    }

    fn handle_mute(&mut self, muted: bool) {
        let Some(active) = self.active.as_mut() else {
            return;
        };
        // The requested state is remembered so that undeafen restores it.
        active.mute_restore = muted;
        if active.deafened || active.muted == muted {
            // Deafened: mute stays forced (Discord semantics). Unchanged
            // state: idempotent, nothing to re-broadcast.
            return;
        }
        active.muted = muted;
        self.apply_local_voice_state();
    }

    /// `voice.deafen` : stops (or restores) decoding/playing every incoming
    /// voice locally. Deafen forces mute; undeafen restores the last
    /// requested mute state. Idempotent; no effect outside a channel. A
    /// server-side deafen (op 0x1F) keeps the room deafened regardless.
    fn handle_deafen(&mut self, deafened: bool) {
        let me = self.node.public_key();
        let server_deafened = self.mod_flags.get(&me).is_some_and(|f| f.deafened);
        let Some(active) = self.active.as_mut() else {
            return;
        };
        if active.deafened == deafened {
            return;
        }
        active.deafened = deafened;
        active.muted = if deafened { true } else { active.mute_restore };
        active.room.set_deafened(deafened || server_deafened);
        self.apply_local_voice_state();
    }

    /// Reflects a local mute/deafen change: closes our speaking indicator,
    /// records our roster flags, notifies the UI (`event.voice_mute`) and
    /// broadcasts the new state right away (ahead of the periodic refresh).
    fn apply_local_voice_state(&mut self) {
        let Some(active) = self.active.as_ref() else {
            return;
        };
        let key = active.key();
        let (gid, cid, muted, deafened) = (
            active.group_id,
            active.channel_id,
            active.muted,
            active.deafened,
        );
        let me = self.node.public_key();
        let now = self.now_ms();
        let mut events = Vec::new();
        if let Some(roster) = self.rooms.get_mut(&key) {
            if muted {
                if let Some(event) = roster.force_silent(&me) {
                    events.push(event);
                }
            }
            if let Some(event) = roster.set_mute_state(&me, muted, deafened, now) {
                events.push(event);
            }
        }
        for event in &events {
            self.emit_room(key, event);
        }
        if !self.active.as_ref().is_some_and(|a| a.is_call) {
            self.broadcast_signal(gid, cid, ACTION_STATE, muted, deafened);
        }
    }

    /// `voice.set_volume` : validates, persists (meta table, keyed by peer
    /// public key for participants) and applies the gain live to the active
    /// room. `peer: None` targets the master output volume.
    fn handle_set_volume(&mut self, peer: Option<[u8; 32]>, volume: u16) -> Result<(), NodeError> {
        match peer {
            None => {
                self.node.set_voice_master_volume(volume)?;
                self.master_volume = volume;
                if let Some(active) = self.active.as_mut() {
                    active.room.set_master_gain(gain::gain_of_pct(volume));
                }
            }
            Some(pk) => {
                self.node.set_voice_peer_volume(&pk, volume)?;
                self.peer_volumes.insert(pk, volume);
                if let Some(active) = self.active.as_mut() {
                    active.room.set_peer_gain(&pk, gain::gain_of_pct(volume));
                }
            }
        }
        Ok(())
    }

    /// Occupants d'un salon connu, enrichis des volumes locaux et de la
    /// modération de groupe (rosters passifs inclus).
    fn participants_of(&mut self, key: &RoomKey) -> Vec<VoiceParticipant> {
        self.rooms
            .get(key)
            .map(Roster::participants)
            .unwrap_or_default()
            .into_iter()
            .map(|p| {
                let volume = self.volume_for(&p.pubkey);
                let flags = self.mod_flags.get(&p.pubkey).copied().unwrap_or_default();
                VoiceParticipant {
                    pubkey: p.pubkey,
                    speaking: p.speaking,
                    muted: p.muted,
                    deafened: p.deafened,
                    volume,
                    server_muted: flags.muted,
                    server_deafened: flags.deafened,
                    priority_speaker: flags.priority,
                }
            })
            .collect()
    }

    fn handle_status(&mut self) -> Option<VoiceStatus> {
        let active = self.active.as_ref()?;
        let (group_id, channel_id, is_call, muted, deafened) = (
            active.group_id,
            active.channel_id,
            active.is_call,
            active.muted,
            active.deafened,
        );
        let participants = self.participants_of(&(group_id, channel_id));
        Some(VoiceStatus {
            group_id,
            channel_id,
            is_call,
            muted,
            deafened,
            participants,
        })
    }

    /// Snapshot de présence de tous les salons connus (actif et passifs).
    fn handle_rooms(&mut self) -> Vec<VoiceRoomPresence> {
        let keys: Vec<RoomKey> = self.rooms.keys().copied().collect();
        keys.into_iter()
            .map(|key| VoiceRoomPresence {
                group_id: key.0,
                channel_id: key.1,
                participants: self.participants_of(&key),
            })
            .collect()
    }

    /// `voice.devices` : périphériques `cpal` disponibles et sélection
    /// persistée. Sans matériel (mode simulé, feature absente), le contrat
    /// gelé impose listes vides et sélections `None` (D-029).
    fn handle_devices(&self) -> Result<VoiceDevices, NodeError> {
        #[cfg(feature = "hardware")]
        if self.backend == VoiceBackend::Materiel {
            return Ok(VoiceDevices {
                inputs: accord_voice::io::input_devices().map_err(audio_error)?,
                outputs: accord_voice::io::output_devices().map_err(audio_error)?,
                selected_input: self.input_device.clone(),
                selected_output: self.output_device.clone(),
            });
        }
        Ok(VoiceDevices::default())
    }

    /// `voice.set_devices` : valide (mode matériel : nom inconnu = erreur
    /// explicite), persiste (table `meta`, motif du pseudo D-027) puis
    /// applique à chaud au salon actif et au test micro en cours (D-029).
    async fn handle_set_devices(
        &mut self,
        input: Option<Option<String>>,
        output: Option<Option<String>>,
    ) -> Result<(), NodeError> {
        #[cfg(feature = "hardware")]
        if self.backend == VoiceBackend::Materiel {
            if let Some(Some(name)) = &input {
                let known = accord_voice::io::input_devices().map_err(audio_error)?;
                if !known.iter().any(|n| n == name) {
                    return Err(NodeError::Audio(format!(
                        "périphérique d'entrée inconnu : {name}"
                    )));
                }
            }
            if let Some(Some(name)) = &output {
                let known = accord_voice::io::output_devices().map_err(audio_error)?;
                if !known.iter().any(|n| n == name) {
                    return Err(NodeError::Audio(format!(
                        "périphérique de sortie inconnu : {name}"
                    )));
                }
            }
        }
        self.node.set_voice_devices_config(
            input.as_ref().map(|choice| choice.as_deref()),
            output.as_ref().map(|choice| choice.as_deref()),
        )?;
        if let Some(choice) = input {
            self.input_device = choice;
        }
        if let Some(choice) = output {
            self.output_device = choice;
        }
        // Application à chaud : réouverture des flux concernés.
        #[cfg(feature = "hardware")]
        if self.backend == VoiceBackend::Materiel {
            if self.active.is_some() {
                // Libère les anciens périphériques avant de rouvrir.
                self.hw = None;
                // Nouveau périphérique = nouveau délai d'écho : état neuf.
                self.aec = accord_voice::EchoCanceller::new();
                self.hw = Some(super::hw::HardwareIo::open(
                    self.input_device.clone(),
                    self.output_device.clone(),
                ));
            }
            if self.mic_test.take().is_some() {
                match super::hw::MicCapture::open(self.input_device.clone()).await {
                    Ok(io) => self.mic_test = Some(MicTest::new(io)),
                    Err(e) => {
                        tracing::warn!(erreur = %e, "voix : test micro interrompu à la bascule");
                    }
                }
            }
        }
        Ok(())
    }

    /// `voice.mic_test` : démarre/arrête la capture de test. L'activation
    /// exige la capture réelle (erreur explicite sinon) et un salon inactif ;
    /// la désactivation est toujours idempotente (D-029).
    async fn handle_mic_test(&mut self, enabled: bool) -> Result<(), NodeError> {
        if !enabled {
            #[cfg(feature = "hardware")]
            {
                self.mic_test = None;
            }
            return Ok(());
        }
        #[cfg(feature = "hardware")]
        if self.backend == VoiceBackend::Materiel {
            if self.hub.is_none() {
                // Aucun canal pour émettre event.voice_level.
                return Err(audio_unavailable());
            }
            if self.active.is_some() {
                return Err(NodeError::Invalid(
                    "salon vocal actif : le test micro est indisponible",
                ));
            }
            if self.mic_test.is_some() {
                return Ok(()); // Déjà actif : idempotent.
            }
            let io = super::hw::MicCapture::open(self.input_device.clone())
                .await
                .map_err(NodeError::Audio)?;
            self.mic_test = Some(MicTest::new(io));
            return Ok(());
        }
        Err(audio_unavailable())
    }

    /// Passe de 20 ms du test micro : agrège la capture (crête RMS + VAD) et
    /// émet `event.voice_level` à ~10 Hz. S'arrête tout seul quand plus
    /// aucune connexion API n'écoute (D-029).
    #[cfg(feature = "hardware")]
    fn tick_mic_test(&mut self) {
        if self.mic_test.is_none() {
            return;
        }
        // Dernière connexion API fermée : libérer le micro.
        let listeners = self
            .hub
            .as_ref()
            .map(NotificationHub::subscriber_count)
            .unwrap_or(0);
        if listeners == 0 {
            self.mic_test = None;
            return;
        }
        let Some(test) = self.mic_test.as_mut() else {
            return;
        };
        while let Some(frame) = test.io.try_frame() {
            test.peak = test.peak.max(accord_voice::Vad::frame_rms(&frame));
            test.speaking = test.vad.is_active(&frame);
        }
        test.ticks += 1;
        if test.ticks % LEVEL_PERIOD_TICKS == 0 {
            let params = json!({ "level": test.peak, "speaking": test.speaking });
            test.peak = 0.0;
            if let Some(hub) = &self.hub {
                hub.notify("event.voice_level", params);
            }
        }
    }

    /// Signalisation reçue d'un pair authentifié : re-valide l'adhésion au
    /// groupe puis met à jour la présence, l'état micro/sortie diffusé (et le
    /// salon actif le cas échéant).
    fn handle_peer_signal(
        &mut self,
        from: [u8; 32],
        group_id: [u8; 16],
        channel_id: [u8; 16],
        action: u8,
        media_kinds: u8,
        mute: bool,
    ) {
        let me = self.node.public_key();
        if from == me {
            return;
        }
        let Ok(state) = self.node.group_state(&group_id) else {
            return;
        };
        if !state.is_member(&from) {
            tracing::debug!("voix : signal d'un non-membre ignoré");
            return;
        }
        let key = (group_id, channel_id);
        let now = self.now_ms();
        let mut events = Vec::new();
        match action {
            ACTION_JOIN | ACTION_STATE => {
                let peer_deafened = media_kinds & MEDIA_DEAFENED != 0;
                if action == ACTION_JOIN {
                    // Vidéo sélective : l'arrivant repart d'une UI vierge. Un
                    // masque hérité de son passage précédent lui vaudrait une
                    // tuile noire jusqu'à expiration — on l'oublie tout de suite.
                    self.video_hidden.forget(&from);
                }
                // 🔒 Borne du suivi de présence passive. `channel_id` vient du
                // message et n'est confronté à aucun salon existant — un membre
                // modifié pourrait donc ouvrir une table de présence par
                // identifiant tiré au hasard. Le balayage périodique les efface
                // au bout de [`PASSIVE_TTL_MS`], mais rien ne bornait ce qui
                // s'accumule ENTRE deux expirations. Refuser au-delà du plafond
                // ne coûte rien de légitime : un serveur réel n'a pas mille
                // salons vocaux occupés, et un salon déjà suivi n'est jamais
                // écarté (l'entrée existe, la borne ne s'applique qu'aux
                // créations).
                if !self.rooms.contains_key(&key) && self.rooms.len() >= MAX_PASSIVE_ROOMS {
                    tracing::debug!("voix : trop de salons suivis, présence ignorée");
                    return;
                }
                {
                    let roster = self.rooms.entry(key).or_default();
                    match roster.join(from, now) {
                        Ok(true) => events.push(RosterEvent::Joined(from)),
                        Ok(false) => {}
                        Err(e) => {
                            tracing::debug!(erreur = %e, "voix : signal d'entrée ignoré");
                            return;
                        }
                    }
                    if let Some(event) = roster.set_mute_state(&from, mute, peer_deafened, now) {
                        events.push(event);
                    }
                }
                let volume = self.volume_for(&from);
                let mut reply_state = false;
                if let Some(active) = self.active.as_mut() {
                    if active.key() == key {
                        let _ = active.room.add_participant(from);
                        active.room.set_peer_gain(&from, gain::gain_of_pct(volume));
                        reply_state = action == ACTION_JOIN;
                    }
                }
                if self.active.as_ref().is_some_and(|a| a.key() == key) {
                    // Modération/priorité du nouvel arrivant (état déjà lu).
                    self.mod_flags.insert(from, mod_flags_of(&state, &from));
                }
                if reply_state {
                    // Le nouvel arrivant apprend notre présence directement.
                    let (muted, deafened) = self
                        .active
                        .as_ref()
                        .map(|a| (a.muted, a.deafened))
                        .unwrap_or((false, false));
                    self.outbound.send(Outbound::Core {
                        to: from,
                        msg: Box::new(CoreMsg::VoiceSignal {
                            group_id,
                            channel_id,
                            action: ACTION_STATE,
                            media_kinds: media_flags(deafened),
                            mute: muted,
                        }),
                    });
                }
            }
            ACTION_LEAVE => {
                let is_active = self.active.as_ref().map(Active::key) == Some(key);
                if let Some(roster) = self.rooms.get_mut(&key) {
                    if roster.leave(&from) {
                        events.push(RosterEvent::Left(from));
                    }
                    if roster.is_empty() && !is_active {
                        self.rooms.remove(&key);
                    }
                }
                if let Some(active) = self.active.as_mut() {
                    if active.key() == key {
                        active.room.remove_participant(&from);
                    }
                }
            }
            _ => {}
        }
        for event in &events {
            self.emit_room(key, event);
        }
    }

    /// Message du canal VOICE reçu d'un pair : trame audio (gigue + état
    /// « parle ») ou ping de qualité (adaptation de débit + vivacité).
    fn handle_peer_frame(&mut self, from: [u8; 32], msg: VoiceMsg) {
        // Vidéo sélective : la déclaration d'un destinataire ne touche ni le
        // moteur audio ni les réassembleurs — elle ne concerne que ce que NOUS
        // émettons vers lui.
        if let VoiceMsg::VideoInterest { room, hidden } = msg {
            self.handle_video_interest_from(from, room, hidden);
            return;
        }
        // Le partage d'écran suit un chemin distinct (réassemblage + événements
        // UI), sans toucher au moteur audio.
        if matches!(
            msg,
            VoiceMsg::ScreenFrame { .. }
                | VoiceMsg::ScreenControl { .. }
                | VoiceMsg::CameraFrame { .. }
                | VoiceMsg::CameraControl { .. }
        ) {
            self.handle_video_peer_frame(from, msg);
            return;
        }
        let now = self.now_ms();
        let media_now = now as u32;
        let Some(active) = self.active.as_mut() else {
            return;
        };
        let key = active.key();
        let mut event = None;
        match &msg {
            VoiceMsg::AudioFrame { room, .. } => {
                if *room != active.channel_id {
                    return;
                }
                // Défense en profondeur : les trames d'un membre réduit au
                // silence par la modération (op 0x1F) sont jetées à la
                // réception — un client modifié ne se fait pas entendre.
                if !active.is_call && self.mod_flags.get(&from).is_some_and(|f| f.muted) {
                    return;
                }
                let Some(roster) = self.rooms.get_mut(&key) else {
                    return;
                };
                if !roster.contains(&from) {
                    return;
                }
                active.room.on_frame(&from, msg, media_now);
                event = roster.on_frame(&from, now);
            }
            VoiceMsg::VoicePing { .. } => {
                active.room.on_ping(&msg);
                if let Some(roster) = self.rooms.get_mut(&key) {
                    roster.touch(&from, now);
                }
            }
            // Routées en amont (vidéo, intérêt vidéo) : inatteignable.
            VoiceMsg::ScreenFrame { .. }
            | VoiceMsg::ScreenControl { .. }
            | VoiceMsg::CameraFrame { .. }
            | VoiceMsg::CameraControl { .. }
            | VoiceMsg::VideoInterest { .. } => {}
        }
        if let Some(event) = event {
            self.emit_room(key, &event);
        }
    }

    /// Flux vidéo reçu d'un pair : réassemblage des trames et signalisation
    /// vers l'UI. N'accepte que dans un appel 1-à-1 actif, sur le salon de
    /// l'appel courant. Les deux flux (écran, caméra) sont réassemblés
    /// séparément et remontés sur des événements distincts.
    fn handle_video_peer_frame(&mut self, from: [u8; 32], msg: VoiceMsg) {
        let (key, room) = match self.active.as_ref() {
            Some(active) => (active.key(), active.channel_id),
            None => return,
        };
        // Défense en profondeur (comme l'audio) : on n'accepte de vidéo que d'un
        // participant du salon actif — un pair hors salon ne peut rien afficher.
        if !self
            .rooms
            .get(&key)
            .is_some_and(|roster| roster.contains(&from))
        {
            return;
        }
        // Décomposition commune aux deux flux.
        let (stream, frame) = match msg {
            VoiceMsg::ScreenControl { room: r, on } => (VideoStream::Screen, Err((r, on))),
            VoiceMsg::CameraControl { room: r, on } => (VideoStream::Camera, Err((r, on))),
            VoiceMsg::ScreenFrame {
                room: r,
                frame_id,
                frag_count,
                frag_idx,
                flags,
                payload,
            } => (
                VideoStream::Screen,
                Ok((r, frame_id, frag_count, frag_idx, flags, payload)),
            ),
            VoiceMsg::CameraFrame {
                room: r,
                frame_id,
                frag_count,
                frag_idx,
                flags,
                payload,
            } => (
                VideoStream::Camera,
                Ok((r, frame_id, frag_count, frag_idx, flags, payload)),
            ),
            _ => return,
        };

        match frame {
            Err((r, on)) => {
                if r != room {
                    return;
                }
                if !on {
                    self.video_rx.remove(&(from, stream));
                }
                self.emit_video_state(stream, &from, on);
            }
            Ok((r, frame_id, frag_count, frag_idx, flags, payload)) => {
                if r != room {
                    return;
                }
                let keyframe = flags & VIDEO_FLAG_KEYFRAME != 0;
                let done = self
                    .video_rx
                    .entry((from, stream))
                    .or_default()
                    .push(frame_id, frag_count, frag_idx, keyframe, payload);
                if let Some((encoded, keyframe)) = done {
                    self.emit_video_frame(stream, &from, keyframe, &encoded);
                }
            }
        }
    }

    /// Diffuse une trame vidéo encodée au pair de l'appel actif sur le flux
    /// `stream` : fragmentée en trames ≤ MTU sur le canal VOICE. Sans effet
    /// hors appel actif.
    async fn handle_video_send(&mut self, stream: VideoStream, keyframe: bool, encoded: Vec<u8>) {
        // Vidéo sélective : seuls les destinataires qui affichent CE flux.
        let Some((peers, room)) = self.video_targets(Some(stream)) else {
            return;
        };
        if peers.is_empty() {
            return;
        }
        let counter = self.video_next_frame_id.entry(stream).or_insert(0);
        let frame_id = *counter;
        *counter = counter.wrapping_add(1);
        // Une seule fragmentation, diffusée à tous (full mesh, comme l'audio).
        let frames = super::media::fragment(stream, room, frame_id, keyframe, &encoded);
        for peer in &peers {
            for msg in &frames {
                if !self.sender.send_voice(peer, msg.clone()).await {
                    tracing::trace!("vidéo : pair injoignable, fragment perdu");
                }
            }
        }
    }

    /// Annonce le démarrage/arrêt d'un flux vidéo au pair de l'appel actif.
    /// Sans effet hors appel actif.
    async fn handle_video_announce(&mut self, stream: VideoStream, on: bool) {
        // L'annonce ignore délibérément la vidéo sélective (`None`) : elle part
        // à TOUT LE MONDE, même à qui a masqué ce flux. C'est elle qui fait
        // apparaître la tuile chez le récepteur ; la filtrer enfermerait un pair
        // dans son propre masquage — il n'apprendrait jamais qu'une caméra
        // vient de s'allumer, donc ne pourrait jamais demander à la voir.
        let Some((peers, room)) = self.video_targets(None) else {
            return;
        };
        let msg = match stream {
            VideoStream::Screen => VoiceMsg::ScreenControl { room, on },
            VideoStream::Camera => VoiceMsg::CameraControl { room, on },
        };
        for peer in &peers {
            let _ = self.sender.send_voice(peer, msg.clone()).await;
        }
    }

    /// Destinataires des trames vidéo sortantes et salon associé :
    /// - appel 1-à-1 actif → le seul pair de l'appel, `room == call_id` ;
    /// - salon vocal de groupe → TOUS les participants sauf soi (même diffusion
    ///   full mesh que l'audio), `room == channel_id`.
    ///
    /// `stream` porte la vidéo sélective : `Some(flux)` retire les destinataires
    /// qui ont déclaré ne pas afficher CE flux (déclaration encore valide) ;
    /// `None` ne filtre rien — c'est le mode des annonces, qui doivent atteindre
    /// jusqu'à ceux qui masquent. Un pair qui n'a rien déclaré passe toujours :
    /// le filtre ne peut que retirer sur demande explicite et fraîche.
    ///
    /// `None` hors session active. La liste peut être vide (salon où l'on est
    /// seul, ou tout le monde a masqué le flux) : l'émission est alors
    /// simplement sans destinataire.
    fn video_targets(&self, stream: Option<VideoStream>) -> Option<(Vec<[u8; 32]>, [u8; 16])> {
        let active = self.active.as_ref()?;
        let (peers, room) = if active.is_call {
            let snap = self.calls.snapshot();
            match (snap.phase, snap.peer, snap.call_id) {
                (CallPhase::Active, Some(peer), Some(call_id)) => (vec![peer], call_id),
                _ => return None,
            }
        } else {
            let me = self.node.public_key();
            let roster = self.rooms.get(&active.key())?;
            let peers: Vec<[u8; 32]> = roster
                .pubkeys()
                .into_iter()
                .filter(|pk| *pk != me)
                .collect();
            (peers, active.channel_id)
        };
        let Some(stream) = stream else {
            return Some((peers, room));
        };
        let now = Instant::now();
        let peers = peers
            .into_iter()
            .filter(|pk| self.video_hidden.wants(pk, stream, now))
            .collect();
        Some((peers, room))
    }

    /// Vidéo sélective, côté RÉCEPTEUR : l'UI déclare ce qu'elle n'affiche pas.
    /// Seuls les changements partent sur le fil (les trames arrivent à 24 Hz ;
    /// une déclaration par re-rendu coûterait plus qu'elle n'économise), et un
    /// pair qui n'est pas dans la session est écarté — déclarer à quelqu'un qui
    /// ne nous envoie rien ne ferait que révéler ce que l'on regarde.
    async fn handle_video_interest(&mut self, hidden: Vec<([u8; 32], u8)>) {
        let Some((peers, room)) = self.video_targets(None) else {
            // Hors session : rien à déclarer, et l'état repart de zéro à la
            // session suivante (où plus rien n'est masqué).
            self.video_declared.clear();
            return;
        };
        let wanted: Vec<([u8; 32], u8)> = hidden
            .into_iter()
            .filter(|(peer, _)| peers.contains(peer))
            .collect();
        for (peer, mask) in self.video_declared.apply(&wanted) {
            let msg = VoiceMsg::VideoInterest { room, hidden: mask };
            if !self.sender.send_voice(&peer, msg).await {
                // Perte sans gravité : le masque non nul sera réaffirmé au
                // prochain cycle, et un masque nul perdu se règle par
                // l'expiration chez l'émetteur.
                tracing::trace!("vidéo : déclaration d'intérêt non remise");
            }
        }
    }

    /// Vidéo sélective, côté ÉMETTEUR : déclaration reçue d'un destinataire de
    /// nos flux. Mêmes gardes que les trames vidéo (session active, bon salon,
    /// participant connu) : un tiers ne doit pas pouvoir éteindre notre caméra
    /// chez les autres.
    fn handle_video_interest_from(&mut self, from: [u8; 32], room: [u8; 16], hidden: u8) {
        let Some(active) = self.active.as_ref() else {
            return;
        };
        if room != active.channel_id {
            return;
        }
        let key = active.key();
        if !self
            .rooms
            .get(&key)
            .is_some_and(|roster| roster.contains(&from))
        {
            return;
        }
        self.video_hidden.note(from, hidden, Instant::now());
    }

    /// Émet la trame vidéo réassemblée vers l'UI (`event.screen_frame` ou
    /// `event.camera_frame`). Les octets encodés voyagent en hexadécimal.
    fn emit_video_frame(
        &self,
        stream: VideoStream,
        peer: &[u8; 32],
        keyframe: bool,
        encoded: &[u8],
    ) {
        let Some(hub) = &self.hub else {
            return;
        };
        let event = match stream {
            VideoStream::Screen => "event.screen_frame",
            VideoStream::Camera => "event.camera_frame",
        };
        hub.notify(
            event,
            json!({
                "peer": hex::encode(peer),
                "keyframe": keyframe,
                "data": hex::encode(encoded),
            }),
        );
    }

    /// Émet le changement d'état d'un flux vidéo du pair vers l'UI
    /// (`event.screen_state` ou `event.camera_state`).
    fn emit_video_state(&self, stream: VideoStream, peer: &[u8; 32], on: bool) {
        let Some(hub) = &self.hub else {
            return;
        };
        let (event, key) = match stream {
            VideoStream::Screen => ("event.screen_state", "sharing"),
            VideoStream::Camera => ("event.camera_state", "on"),
        };
        hub.notify(
            event,
            json!({
                "peer": hex::encode(peer),
                key: on,
            }),
        );
    }

    /// Passe cadencée à 20 ms : test micro, capture → encodage → diffusion,
    /// lecture, pings de qualité, vivacité et balayage des présences
    /// passives.
    async fn on_tick(&mut self) {
        #[cfg(feature = "hardware")]
        self.tick_mic_test();
        self.tick_count += 1;
        let now = self.now_ms();
        // Machine d'appels : timeouts de sonnerie et réémission d'offre.
        let call_actions = self.calls.tick(now);
        self.run_call_actions(call_actions);
        let me = self.node.public_key();
        let my_server_mute = self.mod_flags.get(&me).is_some_and(|f| f.muted);
        let mut pcm = self.next_capture();
        // Annulation d'écho (D-051) : la capture est nettoyée de ce que la
        // sortie a joué AVANT la suppression de bruit et l'AGC (l'ordre
        // standard — l'écho est un signal linéaire, il se soustrait sur le
        // micro brut). Sans session active, la référence est muette et le
        // traitement est transparent.
        if self.dsp_echo_cancel && self.active.is_some() {
            self.aec.process(&mut pcm);
        }
        let mut events: Vec<(RoomKey, RosterEvent)> = Vec::new();
        let mut to_send: Vec<([u8; 32], VoiceMsg)> = Vec::new();
        let mut call_peer_lost = false;

        if let Some(active) = self.active.as_mut() {
            let key = active.key();
            // Capture locale (la VAD décide de la transmission) ; une
            // sourdine de modération (op 0x1F) coupe l'émission à la source.
            if !active.muted && !my_server_mute {
                match active.room.capture(&pcm) {
                    Ok(Some(frame)) => {
                        if let Some(roster) = self.rooms.get_mut(&key) {
                            if let Some(event) = roster.on_frame(&me, now) {
                                events.push((key, event));
                            }
                            for pk in roster.pubkeys() {
                                if pk != me {
                                    to_send.push((pk, frame.clone()));
                                }
                            }
                        }
                    }
                    Ok(None) => {}
                    Err(e) => tracing::debug!(erreur = %e, "voix : capture ignorée"),
                }
            }
            if let Some(roster) = self.rooms.get_mut(&key) {
                // Lecture des participants (le tampon de gigue cadence). Les
                // trames décodées du tick sont MIXÉES en une seule (D-051) :
                // envoyées une à une, elles joueraient à la suite — voix
                // entrelacées, file de sortie qui enfle puis jette (à-coups)
                // dès trois participants.
                let mut decoded_frames: Vec<Vec<i16>> = Vec::new();
                for pk in roster.pubkeys() {
                    if pk == me {
                        continue;
                    }
                    if let Ok(Some(decoded)) = active.room.play(&pk) {
                        decoded_frames.push(decoded);
                    }
                    // Retour de qualité périodique (SPEC §8).
                    if self.tick_count % PING_PERIOD_TICKS == 0 {
                        if let Some(ping) = active.room.quality_ping(&pk, 0) {
                            to_send.push((pk, ping));
                        }
                    }
                }
                let mixed = accord_voice::mix::mix_frames(decoded_frames);
                // Référence far-end de l'AEC : la trame réellement poussée
                // vers le haut-parleur, silence compris (chronologie de la
                // lecture). Poussée APRÈS le traitement de la capture de ce
                // tick : l'écho ne peut contenir que des trames déjà rangées.
                if self.dsp_echo_cancel {
                    match &mixed {
                        Some(frame) => self.aec.push_far(frame),
                        None => self.aec.push_far(&[]),
                    }
                }
                #[cfg(feature = "hardware")]
                if let Some(hw) = &self.hw {
                    if let Some(frame) = mixed {
                        hw.play(frame);
                    }
                }
                #[cfg(not(feature = "hardware"))]
                drop(mixed);
                // Priority speaker : les non-prioritaires sont atténués tant
                // qu'un orateur prioritaire parle (salons de groupe).
                if !active.is_call {
                    let participants = roster.participants();
                    let any_priority_speaking = participants.iter().any(|p| {
                        p.speaking && self.mod_flags.get(&p.pubkey).is_some_and(|f| f.priority)
                    });
                    for p in &participants {
                        if p.pubkey == me {
                            continue;
                        }
                        let priority = self.mod_flags.get(&p.pubkey).is_some_and(|f| f.priority);
                        let duck = if any_priority_speaking && !priority {
                            PRIORITY_DUCK
                        } else {
                            1.0
                        };
                        active.room.set_peer_duck(&p.pubkey, duck);
                    }
                }
                // Vivacité : soi-même n'expire jamais (rafraîchi ici).
                roster.touch(&me, now);
                for event in roster.tick(now, ACTIVE_TIMEOUT_MS, Some(&me)) {
                    if let RosterEvent::Left(pk) = &event {
                        active.room.remove_participant(pk);
                        if active.is_call {
                            // Le pair d'appel a disparu : fin d'appel.
                            call_peer_lost = true;
                        }
                    }
                    events.push((key, event));
                }
            }
            // Rafraîchit la présence passive des membres hors salon (les
            // appels 1-à-1 n'ont aucune signalisation de groupe).
            if self.tick_count % STATE_PERIOD_TICKS == 0 && !active.is_call {
                let (gid, cid, muted, deafened) = (
                    active.group_id,
                    active.channel_id,
                    active.muted,
                    active.deafened,
                );
                self.broadcast_signal(gid, cid, ACTION_STATE, muted, deafened);
            }
        }
        if call_peer_lost {
            let actions = self.calls.on_audio_lost();
            self.run_call_actions(actions);
        }

        // Balayage des présences passives (salons non rejoints).
        if self.tick_count % PASSIVE_SWEEP_TICKS == 0 {
            let active_key = self.active.as_ref().map(Active::key);
            for (key, roster) in self.rooms.iter_mut() {
                if Some(*key) == active_key {
                    continue;
                }
                for event in roster.tick(now, PASSIVE_TTL_MS, None) {
                    events.push((*key, event));
                }
            }
            self.rooms
                .retain(|key, roster| Some(*key) == active_key || !roster.is_empty());
        }

        // Vidéo sélective : les masques que l'on impose aux autres sont un état
        // SOUPLE chez eux (il expire). On les réaffirme tant qu'on masque
        // quelque chose ; dès qu'on n'occulte plus rien, `refresh` est vide et
        // le silence rétablit tout de lui-même.
        if self.tick_count % INTEREST_REFRESH_TICKS == 0 {
            let refresh = self.video_declared.refresh();
            if !refresh.is_empty() {
                if let Some((_, room)) = self.video_targets(None) {
                    for (peer, hidden) in refresh {
                        to_send.push((peer, VoiceMsg::VideoInterest { room, hidden }));
                    }
                }
            }
        }

        for (key, event) in &events {
            self.emit_room(*key, event);
        }
        for (to, msg) in to_send {
            if !self.sender.send_voice(&to, msg).await {
                tracing::trace!("voix : pair injoignable, trame perdue");
            }
        }
    }

    /// Prochaine trame de capture : matériel, sinon injection, sinon silence.
    fn next_capture(&mut self) -> Vec<i16> {
        #[cfg(feature = "hardware")]
        if let Some(hw) = &self.hw {
            if let Some(frame) = hw.try_capture() {
                if frame.len() == FRAME_SAMPLES {
                    return frame;
                }
            }
        }
        self.injected
            .pop_front()
            .unwrap_or_else(|| vec![0i16; FRAME_SAMPLES])
    }

    // ---- Appels 1-à-1 (contrat `calls.*`) ----

    /// Vrai si `peer` est un ami confirmé (lecture de la base : réservée aux
    /// chemins déjà cadencés — démarrage d'appel local, offre entrante).
    fn is_friend(&self, peer: &[u8; 32]) -> bool {
        self.node
            .friend_pubkeys()
            .map(|friends| friends.contains(peer))
            .unwrap_or(false)
    }

    /// `calls.start` : amitié requise, un seul appel à la fois.
    fn handle_call_start(&mut self, peer: [u8; 32]) -> Result<[u8; 16], NodeError> {
        if peer == self.node.public_key() {
            return Err(NodeError::Invalid("impossible de s'appeler soi-même"));
        }
        if !self.is_friend(&peer) {
            return Err(NodeError::Invalid("l'appelé n'est pas un ami confirmé"));
        }
        let call_id = new_call_id();
        let actions = self
            .calls
            .start(peer, call_id, self.now_ms())
            .map_err(NodeError::Invalid)?;
        self.run_call_actions(actions);
        Ok(call_id)
    }

    /// Message d'appel reçu d'un pair authentifié. Une offre n'est honorée
    /// que d'un AMI (aucune réponse sinon : zéro amplification) ; réponses,
    /// refus et raccrochages ne sont honorés que s'ils corrèlent strictement
    /// l'appel courant (la machine ignore le reste en silence).
    fn handle_peer_call(&mut self, from: [u8; 32], msg: CoreMsg) {
        if from == self.node.public_key() {
            return;
        }
        let now = self.now_ms();
        let actions = match msg {
            CoreMsg::CallOffer { call_id } => {
                if !self.is_friend(&from) {
                    tracing::debug!("appel : offre d'un non-ami ignorée");
                    return;
                }
                self.calls.on_offer(from, call_id, now)
            }
            CoreMsg::CallAnswer { call_id } => self.calls.on_answer(from, call_id, now),
            CoreMsg::CallDecline { call_id, reason } => {
                self.calls.on_decline(from, call_id, reason)
            }
            CoreMsg::CallHangup { call_id } => self.calls.on_hangup(from, call_id),
            CoreMsg::CallTaken { call_id } => self.calls.on_taken(from, call_id),
            _ => return,
        };
        self.run_call_actions(actions);
    }

    /// Exécute les actions décidées par la machine d'appels : signalisation
    /// (canal CORE, éphémère — jamais mise en file hors-ligne), session
    /// audio et événements API.
    fn run_call_actions(&mut self, actions: Vec<CallAction>) {
        for action in actions {
            match action {
                CallAction::SendOffer { to, call_id } => {
                    self.send_call_msg(to, CoreMsg::CallOffer { call_id });
                }
                CallAction::SendAnswer { to, call_id } => {
                    self.send_call_msg(to, CoreMsg::CallAnswer { call_id });
                }
                CallAction::SendDecline {
                    to,
                    call_id,
                    reason,
                } => {
                    self.send_call_msg(to, CoreMsg::CallDecline { call_id, reason });
                }
                CallAction::SendHangup { to, call_id } => {
                    self.send_call_msg(to, CoreMsg::CallHangup { call_id });
                }
                CallAction::SendTaken { to, call_id } => {
                    self.send_call_msg(to, CoreMsg::CallTaken { call_id });
                }
                CallAction::JoinAudio { peer, call_id } => self.join_call_room(peer, call_id),
                CallAction::LeaveAudio => self.leave_active(),
                CallAction::EventIncoming { peer, call_id } => {
                    self.emit_call("event.call_incoming", &peer, &call_id, None);
                }
                CallAction::EventOutgoing { peer, call_id } => {
                    self.emit_call("event.call_outgoing", &peer, &call_id, None);
                }
                CallAction::EventAccepted { peer, call_id } => {
                    self.emit_call("event.call_accepted", &peer, &call_id, None);
                }
                CallAction::EventEnded {
                    peer,
                    call_id,
                    reason,
                } => {
                    self.emit_call("event.call_ended", &peer, &call_id, Some(reason));
                }
            }
        }
    }

    /// Émet un message d'appel au pair (session chiffrée, canal CORE).
    fn send_call_msg(&self, to: [u8; 32], msg: CoreMsg) {
        self.outbound.send(Outbound::Core {
            to,
            msg: Box::new(msg),
        });
    }

    /// Émet un événement `event.call_*` vers l'UI.
    fn emit_call(&self, event: &str, peer: &[u8; 32], call_id: &[u8; 16], reason: Option<&str>) {
        let Some(hub) = &self.hub else {
            return;
        };
        let mut params = json!({
            "peer": hex::encode(peer),
            "call_id": hex::encode(call_id),
        });
        if let Some(reason) = reason {
            params["reason"] = json!(reason);
        }
        hub.notify(event, params);
    }

    // ---- Modération vocale (op 0x1F) et priorité d'orateur ----

    /// Recalcule les drapeaux de modération/priorité des participants du
    /// salon de groupe actif d'après l'état replié, applique nos propres
    /// contraintes (sourdine/surdité forcées) et émet `event.voice_moderate`
    /// pour chaque participant dont l'état a changé.
    fn refresh_moderation(&mut self, group_id: [u8; 16]) {
        let key = match self.active.as_ref() {
            Some(active) if !active.is_call && active.group_id == group_id => active.key(),
            _ => return,
        };
        let Ok(state) = self.node.group_state(&group_id) else {
            return;
        };
        let members = self
            .rooms
            .get(&key)
            .map(Roster::pubkeys)
            .unwrap_or_default();
        let mut fresh = HashMap::new();
        let mut changed = Vec::new();
        for pk in members {
            let flags = mod_flags_of(&state, &pk);
            if self.mod_flags.get(&pk).copied().unwrap_or_default() != flags {
                changed.push((pk, flags));
            }
            fresh.insert(pk, flags);
        }
        self.mod_flags = fresh;
        self.apply_my_moderation();
        for (pk, flags) in changed {
            self.emit_voice_moderate(group_id, &pk, flags);
        }
    }

    /// Applique nos propres drapeaux de modération à la session active :
    /// surdité forcée sur la sortie, fermeture de l'indicateur « parle » si
    /// le micro est forcé coupé (l'émission est déjà bloquée à la capture).
    fn apply_my_moderation(&mut self) {
        let me = self.node.public_key();
        let flags = self.mod_flags.get(&me).copied().unwrap_or_default();
        let Some(active) = self.active.as_mut() else {
            return;
        };
        if active.is_call {
            return;
        }
        active.room.set_deafened(active.deafened || flags.deafened);
        if flags.muted {
            let key = active.key();
            let event = self
                .rooms
                .get_mut(&key)
                .and_then(|roster| roster.force_silent(&me));
            if let Some(event) = event {
                self.emit_room(key, &event);
            }
        }
    }

    /// Émet `event.voice_moderate` (transition de modération d'un
    /// participant du salon actif).
    fn emit_voice_moderate(&self, group_id: [u8; 16], pubkey: &[u8; 32], flags: ModFlags) {
        let Some(hub) = &self.hub else {
            return;
        };
        hub.notify(
            "event.voice_moderate",
            json!({
                "group_id": hex::encode(&group_id),
                "pubkey": hex::encode(pubkey),
                "server_muted": flags.muted,
                "server_deafened": flags.deafened,
                "priority_speaker": flags.priority,
            }),
        );
    }

    // ---- DSP de capture ----

    /// `voice.set_noise_suppression` / `voice.set_agc` : persiste puis
    /// applique à chaud à la session active.
    fn handle_set_dsp(
        &mut self,
        noise_suppression: Option<bool>,
        agc: Option<bool>,
        echo_cancel: Option<bool>,
    ) -> Result<(), NodeError> {
        self.node
            .set_voice_dsp_config(noise_suppression, agc, echo_cancel)?;
        if let Some(enabled) = noise_suppression {
            self.dsp_noise_suppression = enabled;
        }
        if let Some(enabled) = agc {
            self.dsp_agc = enabled;
        }
        if let Some(enabled) = echo_cancel {
            // Réactivation : état neuf (une référence périmée n'a aucun sens).
            if enabled && !self.dsp_echo_cancel {
                self.aec = accord_voice::EchoCanceller::new();
            }
            self.dsp_echo_cancel = enabled;
        }
        if let Some(active) = self.active.as_mut() {
            active
                .room
                .set_noise_suppression(self.dsp_noise_suppression);
            active.room.set_agc(self.dsp_agc);
        }
        Ok(())
    }

    /// Diffuse une signalisation à tous les membres du groupe (éphémère :
    /// jamais mise en file hors-ligne).
    fn broadcast_signal(
        &self,
        group_id: [u8; 16],
        channel_id: [u8; 16],
        action: u8,
        muted: bool,
        deafened: bool,
    ) {
        self.outbound.send(Outbound::GroupCast {
            group_id,
            msg: Box::new(CoreMsg::VoiceSignal {
                group_id,
                channel_id,
                action,
                media_kinds: media_flags(deafened),
                mute: muted,
            }),
        });
    }

    /// Émet l'événement API correspondant à une transition de salon.
    fn emit_room(&self, key: RoomKey, event: &RosterEvent) {
        let Some(hub) = &self.hub else {
            return;
        };
        let room = |pubkey: &[u8; 32]| {
            json!({
                "group_id": hex::encode(&key.0),
                "channel_id": hex::encode(&key.1),
                "pubkey": hex::encode(pubkey),
            })
        };
        match event {
            RosterEvent::Joined(pubkey) => hub.notify("event.voice_joined", room(pubkey)),
            RosterEvent::Left(pubkey) => hub.notify("event.voice_left", room(pubkey)),
            RosterEvent::Speaking(pubkey, speaking) => hub.notify(
                "event.voice_speaking",
                json!({ "pubkey": hex::encode(pubkey), "speaking": speaking }),
            ),
            RosterEvent::MuteState(pubkey, muted, deafened) => hub.notify(
                "event.voice_mute",
                json!({
                    "pubkey": hex::encode(pubkey),
                    "muted": muted,
                    "deafened": deafened,
                }),
            ),
        }
    }
}

/// `media_kinds` bitflags of our own signals: audio, plus the deafen bit.
fn media_flags(deafened: bool) -> u8 {
    MEDIA_AUDIO | if deafened { MEDIA_DEAFENED } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outbound::OutboundSink;
    use accord_core::db::Db;
    use accord_crypto::Identity;
    use std::sync::Mutex;

    /// Puits d'envoi voix : capture les messages émis pour les asserter.
    struct TestSender(Mutex<Vec<([u8; 32], VoiceMsg)>>);

    #[async_trait::async_trait]
    impl FrameSender for TestSender {
        async fn send_voice(&self, to: &[u8; 32], msg: VoiceMsg) -> bool {
            self.0
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push((*to, msg));
            true
        }
    }

    fn node() -> Arc<Node> {
        let id = Identity::generate_with_pow_bits(1);
        let db = Db::open_in_memory(&[1u8; 32]).unwrap();
        Arc::new(Node::new(id, db, OutboundSink::null()))
    }

    fn spawn_engine(node: Arc<Node>) -> (super::super::VoiceHandle, Arc<TestSender>) {
        let sender = Arc::new(TestSender(Mutex::new(Vec::new())));
        let handle = super::super::spawn(VoiceDeps {
            node,
            outbound: OutboundSink::null(),
            hub: None,
            sender: Arc::clone(&sender) as Arc<dyn FrameSender>,
            backend: VoiceBackend::Simule,
        });
        (handle, sender)
    }

    /// Variante avec capture des actions réseau sortantes (signalisation
    /// d'appel émise par le moteur).
    fn spawn_engine_with_outbound(
        node: Arc<Node>,
    ) -> (
        super::super::VoiceHandle,
        Arc<TestSender>,
        mpsc::Receiver<Outbound>,
    ) {
        let (sink, outbound_rx) = OutboundSink::channel(256);
        let sender = Arc::new(TestSender(Mutex::new(Vec::new())));
        let handle = super::super::spawn(VoiceDeps {
            node,
            outbound: sink,
            hub: None,
            sender: Arc::clone(&sender) as Arc<dyn FrameSender>,
            backend: VoiceBackend::Simule,
        });
        (handle, sender, outbound_rx)
    }

    /// Établit une amitié confirmée avec une identité de test et rend sa clé.
    fn make_friend(node: &Node) -> [u8; 32] {
        let peer = Identity::generate_with_pow_bits(1).public_key();
        node.friend_request(&peer, "Pair").unwrap();
        node.ingest_core(&peer, CoreMsg::FriendResponse { accepted: true })
            .unwrap();
        peer
    }

    /// Prochain message d'appel émis (les autres actions réseau — demandes
    /// d'ami, signalisations de salon — sont ignorées). `None` après ~2 s.
    async fn next_call_msg(rx: &mut mpsc::Receiver<Outbound>) -> Option<([u8; 32], CoreMsg)> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        loop {
            let action = tokio::time::timeout_at(deadline, rx.recv()).await.ok()??;
            if let Outbound::Core { to, msg } = action {
                if matches!(
                    *msg,
                    CoreMsg::CallOffer { .. }
                        | CoreMsg::CallAnswer { .. }
                        | CoreMsg::CallDecline { .. }
                        | CoreMsg::CallHangup { .. }
                        | CoreMsg::CallTaken { .. }
                ) {
                    return Some((to, *msg));
                }
            }
        }
    }

    fn tone() -> Vec<i16> {
        (0..FRAME_SAMPLES)
            .map(|i| if i % 2 == 0 { 20_000 } else { -20_000 })
            .collect()
    }

    async fn eventually(mut cond: impl FnMut() -> bool) -> bool {
        for _ in 0..200 {
            if cond() {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        false
    }

    #[tokio::test]
    async fn join_requires_group_membership() {
        let (handle, _) = spawn_engine(node());
        let err = handle.join([9u8; 16], [9u8; 16]).await.unwrap_err();
        assert!(err.to_string().contains("introuvable"));
    }

    #[tokio::test]
    async fn join_status_mute_leave_lifecycle() {
        let n = node();
        let me = n.public_key();
        let gid: [u8; 16] = hex::decode(&n.group_create("Guilde").unwrap()).unwrap();
        let (handle, _) = spawn_engine(Arc::clone(&n));

        // Convention UI : le salon vocal par défaut a channel_id == group_id.
        let participants = handle.join(gid, gid).await.unwrap();
        assert_eq!(participants, vec![me]);

        let status = handle.status().await.unwrap().unwrap();
        assert_eq!(status.group_id, gid);
        assert_eq!(status.channel_id, gid);
        assert!(!status.muted);
        assert_eq!(status.participants.len(), 1);
        assert_eq!(status.participants[0].pubkey, me);

        handle.set_muted(true).await.unwrap();
        let status = handle.status().await.unwrap().unwrap();
        assert!(status.muted);

        handle.leave().await.unwrap();
        assert!(handle.status().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn deafen_forces_mute_and_undeafen_restores_requested_state() {
        let n = node();
        let gid: [u8; 16] = hex::decode(&n.group_create("Guilde").unwrap()).unwrap();
        let (handle, _) = spawn_engine(Arc::clone(&n));

        // Outside a channel: idempotent no-op, like voice.mute.
        handle.set_deafened(true).await.unwrap();

        handle.join(gid, gid).await.unwrap();
        // Join resets the session state: neither muted nor deafened.
        let status = handle.status().await.unwrap().unwrap();
        assert!(!status.muted && !status.deafened);

        // Deafen forces mute.
        handle.set_deafened(true).await.unwrap();
        let status = handle.status().await.unwrap().unwrap();
        assert!(status.muted && status.deafened);

        // Requesting unmute while deafened keeps the mute forced…
        handle.set_muted(false).await.unwrap();
        let status = handle.status().await.unwrap().unwrap();
        assert!(status.muted && status.deafened);

        // … and undeafen restores the last requested state (unmuted).
        handle.set_deafened(false).await.unwrap();
        let status = handle.status().await.unwrap().unwrap();
        assert!(!status.muted && !status.deafened);

        // Muted before deafen: undeafen keeps the mute.
        handle.set_muted(true).await.unwrap();
        handle.set_deafened(true).await.unwrap();
        handle.set_deafened(false).await.unwrap();
        let status = handle.status().await.unwrap().unwrap();
        assert!(status.muted && !status.deafened);
    }

    #[tokio::test]
    async fn peer_mute_and_deafen_states_surface_in_status() {
        let n = node();
        let gid: [u8; 16] = hex::decode(&n.group_create("Guilde").unwrap()).unwrap();
        let peer = Identity::generate_with_pow_bits(1).public_key();
        n.test_force_add_member(&gid, &peer).unwrap();
        let (handle, _) = spawn_engine(Arc::clone(&n));
        handle.join(gid, gid).await.unwrap();

        // The peer joins muted and deafened (bit 0x80 of media_kinds).
        handle.peer_signal(
            peer,
            gid,
            gid,
            ACTION_JOIN,
            MEDIA_AUDIO | MEDIA_DEAFENED,
            true,
        );
        let seen = eventually_status(&handle, |s| {
            s.participants
                .iter()
                .any(|p| p.pubkey == peer && p.muted && p.deafened)
        })
        .await;
        assert!(seen, "l'état muet/sourd du pair n'apparaît pas");

        // A state refresh clears both flags.
        handle.peer_signal(peer, gid, gid, ACTION_STATE, MEDIA_AUDIO, false);
        let cleared = eventually_status(&handle, |s| {
            s.participants
                .iter()
                .any(|p| p.pubkey == peer && !p.muted && !p.deafened)
        })
        .await;
        assert!(cleared, "l'état muet/sourd du pair ne se referme pas");
    }

    #[tokio::test]
    async fn set_volume_persists_and_surfaces_in_status() {
        let n = node();
        let gid: [u8; 16] = hex::decode(&n.group_create("Guilde").unwrap()).unwrap();
        let peer = Identity::generate_with_pow_bits(1).public_key();
        n.test_force_add_member(&gid, &peer).unwrap();
        let (handle, _) = spawn_engine(Arc::clone(&n));

        // Defaults: 100 % everywhere.
        assert_eq!(handle.master_volume().await.unwrap(), 100);

        // Master volume: persisted and exposed, even outside a channel.
        handle.set_volume(None, 150).await.unwrap();
        assert_eq!(handle.master_volume().await.unwrap(), 150);
        assert_eq!(n.voice_master_volume().unwrap(), 150);

        // Per-peer volume: persisted, applied and exposed in the status.
        handle.set_volume(Some(peer), 40).await.unwrap();
        assert_eq!(n.voice_peer_volume(&peer).unwrap(), 40);
        handle.join(gid, gid).await.unwrap();
        handle.peer_signal(peer, gid, gid, ACTION_JOIN, MEDIA_AUDIO, false);
        let seen = eventually_status(&handle, |s| {
            s.participants
                .iter()
                .any(|p| p.pubkey == peer && p.volume == 40)
        })
        .await;
        assert!(seen, "le volume du pair n'apparaît pas dans le statut");

        // Out of range: explicit error, nothing persisted.
        let err = handle.set_volume(None, 201).await.unwrap_err();
        assert!(
            err.to_string().contains("volume"),
            "erreur inattendue : {err}"
        );
        assert_eq!(handle.master_volume().await.unwrap(), 150);
    }

    #[tokio::test]
    async fn capture_is_sent_to_participants_and_gated_by_mute() {
        let n = node();
        let gid: [u8; 16] = hex::decode(&n.group_create("Guilde").unwrap()).unwrap();
        // Un pair membre du groupe rejoint le salon (signal simulé).
        let peer = Identity::generate_with_pow_bits(1).public_key();
        n.test_force_add_member(&gid, &peer).unwrap();
        let (handle, sender) = spawn_engine(Arc::clone(&n));

        handle.join(gid, gid).await.unwrap();
        handle.peer_signal(peer, gid, gid, ACTION_JOIN, MEDIA_AUDIO, false);

        // La parole injectée part vers le pair.
        for _ in 0..10 {
            handle.inject_pcm(tone());
        }
        let got_frame = eventually(|| {
            sender
                .0
                .lock()
                .unwrap()
                .iter()
                .any(|(to, m)| *to == peer && matches!(m, VoiceMsg::AudioFrame { .. }))
        })
        .await;
        assert!(got_frame, "aucune trame émise vers le pair");

        // Et l'indicateur « parle » local s'ouvre.
        let status = handle.status().await.unwrap().unwrap();
        let me = n.public_key();
        assert!(status
            .participants
            .iter()
            .any(|p| p.pubkey == me && p.speaking));

        // Micro coupé : plus aucune trame ne part.
        handle.set_muted(true).await.unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        let frames = |s: &TestSender| {
            s.0.lock()
                .unwrap()
                .iter()
                .filter(|(_, m)| matches!(m, VoiceMsg::AudioFrame { .. }))
                .count()
        };
        let before = frames(&sender);
        for _ in 0..10 {
            handle.inject_pcm(tone());
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(
            frames(&sender),
            before,
            "des trames sont parties micro coupé"
        );
    }

    #[tokio::test]
    async fn peer_frames_drive_speaking_indicator() {
        let n = node();
        let gid: [u8; 16] = hex::decode(&n.group_create("Guilde").unwrap()).unwrap();
        let peer = Identity::generate_with_pow_bits(1).public_key();
        n.test_force_add_member(&gid, &peer).unwrap();
        let (handle, _) = spawn_engine(Arc::clone(&n));
        handle.join(gid, gid).await.unwrap();
        handle.peer_signal(peer, gid, gid, ACTION_JOIN, MEDIA_AUDIO, false);

        // Trames du pair (encodées PCM 8 bits, room = channel_id).
        let mut codec = Pcm8Codec;
        use accord_voice::AudioCodec;
        for seq in 0..5u16 {
            handle.peer_frame(
                peer,
                VoiceMsg::AudioFrame {
                    room: gid,
                    media_type: MEDIA_AUDIO,
                    seq,
                    ts_ms: u32::from(seq) * 20,
                    payload: codec.encode(&tone()).unwrap(),
                },
            );
        }
        let saw_speaking = eventually_status(&handle, |s| {
            s.participants
                .iter()
                .any(|p| p.pubkey == peer && p.speaking)
        })
        .await;
        assert!(saw_speaking, "le pair n'est jamais passé « parle »");

        // Sans nouvelles trames, l'indicateur se referme (hystérésis).
        let silent = eventually_status(&handle, |s| {
            s.participants
                .iter()
                .any(|p| p.pubkey == peer && !p.speaking)
        })
        .await;
        assert!(silent, "l'indicateur « parle » ne s'est pas refermé");
    }

    /// Attend qu'un prédicat sur le statut devienne vrai.
    async fn eventually_status(
        handle: &super::super::VoiceHandle,
        mut cond: impl FnMut(&VoiceStatus) -> bool,
    ) -> bool {
        for _ in 0..200 {
            if let Ok(Some(status)) = handle.status().await {
                if cond(&status) {
                    return true;
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        false
    }

    #[tokio::test]
    async fn mesh_cap_yields_explicit_error_on_join() {
        let n = node();
        let gid: [u8; 16] = hex::decode(&n.group_create("Guilde").unwrap()).unwrap();
        // 10 membres déjà dans le salon (présence passive apprise).
        let mut members = Vec::new();
        for _ in 0..VOICE_MAX_PARTICIPANTS {
            let pk = Identity::generate_with_pow_bits(1).public_key();
            n.test_force_add_member(&gid, &pk).unwrap();
            members.push(pk);
        }
        let (handle, _) = spawn_engine(Arc::clone(&n));
        for pk in &members {
            handle.peer_signal(*pk, gid, gid, ACTION_JOIN, MEDIA_AUDIO, false);
        }
        // Les commandes sont traitées dans l'ordre : les 10 signaux précèdent
        // la jointure, qui déborde donc le full mesh.
        let err = handle.join(gid, gid).await.unwrap_err();
        assert!(
            err.to_string().contains("plein"),
            "erreur inattendue : {err}"
        );
    }

    #[tokio::test]
    async fn joining_another_room_leaves_the_first() {
        let n = node();
        let gid1: [u8; 16] = hex::decode(&n.group_create("Un").unwrap()).unwrap();
        let gid2: [u8; 16] = hex::decode(&n.group_create("Deux").unwrap()).unwrap();
        let (handle, _) = spawn_engine(Arc::clone(&n));
        handle.join(gid1, gid1).await.unwrap();
        handle.join(gid2, gid2).await.unwrap();
        let status = handle.status().await.unwrap().unwrap();
        assert_eq!(status.group_id, gid2);
    }

    // ---- Périphériques audio et test micro (D-029, mode simulé) ----

    #[tokio::test]
    async fn devices_are_empty_and_default_in_simulated_mode() {
        let n = node();
        let (handle, _) = spawn_engine(Arc::clone(&n));
        assert_eq!(handle.devices().await.unwrap(), VoiceDevices::default());

        // Le choix est accepté et persisté (prêt pour le retour du matériel)…
        handle
            .set_devices(Some(Some("Micro USB".into())), Some(None))
            .await
            .unwrap();
        assert_eq!(
            n.voice_devices_config().unwrap(),
            (Some("Micro USB".into()), None)
        );
        // … mais la sélection rendue reste `None` sans matériel (contrat
        // gelé) et les listes restent vides.
        assert_eq!(handle.devices().await.unwrap(), VoiceDevices::default());
    }

    #[tokio::test]
    async fn set_devices_rejects_invalid_names_in_simulated_mode() {
        let n = node();
        let (handle, _) = spawn_engine(Arc::clone(&n));
        let err = handle
            .set_devices(Some(Some(String::new())), None)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("périphérique"));
        assert_eq!(n.voice_devices_config().unwrap(), (None, None));
    }

    #[tokio::test]
    async fn mic_test_is_explicitly_unavailable_in_simulated_mode() {
        let (handle, _) = spawn_engine(node());
        let err = handle.mic_test(true).await.unwrap_err();
        assert!(
            err.to_string().contains("matériel audio indisponible"),
            "erreur inattendue : {err}"
        );
        // La désactivation reste idempotente, même sans matériel.
        handle.mic_test(false).await.unwrap();
    }

    // ---- Appels 1-à-1 ----

    use super::super::CallPhase;

    /// Attend que la machine d'appels atteigne la phase donnée.
    async fn eventually_phase(handle: &super::super::VoiceHandle, phase: CallPhase) -> bool {
        for _ in 0..200 {
            if handle.call_status().await.unwrap().phase == phase {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        false
    }

    #[tokio::test]
    async fn call_start_requires_friendship_and_rejects_self() {
        let n = node();
        let (handle, _) = spawn_engine(Arc::clone(&n));
        let stranger = Identity::generate_with_pow_bits(1).public_key();
        let err = handle.call_start(stranger).await.unwrap_err();
        assert!(err.to_string().contains("ami"), "erreur inattendue : {err}");
        let err = handle.call_start(n.public_key()).await.unwrap_err();
        assert!(
            err.to_string().contains("soi-même"),
            "erreur inattendue : {err}"
        );
        let snap = handle.call_status().await.unwrap();
        assert_eq!(snap.phase, CallPhase::Idle);
    }

    #[tokio::test]
    async fn outgoing_call_flow_offer_answer_audio_hangup() {
        let n = node();
        let friend = make_friend(&n);
        let (handle, _, mut out) = spawn_engine_with_outbound(Arc::clone(&n));

        let call_id = handle.call_start(friend).await.unwrap();
        let (to, msg) = next_call_msg(&mut out).await.expect("aucune offre émise");
        assert_eq!(to, friend);
        assert_eq!(msg, CoreMsg::CallOffer { call_id });
        assert_eq!(
            handle.call_status().await.unwrap().phase,
            CallPhase::OutgoingRinging
        );
        // Occupé : un second appel simultané est refusé.
        let err = handle.call_start(friend).await.unwrap_err();
        assert!(err.to_string().contains("en cours"));

        // Le pair répond : la session audio démarre (salon = call_id).
        handle.peer_call(friend, CoreMsg::CallAnswer { call_id });
        let active = eventually_status(&handle, |s| {
            s.is_call && s.channel_id == call_id && s.participants.len() == 2
        })
        .await;
        assert!(active, "la session audio d'appel n'a pas démarré");
        assert_eq!(handle.call_status().await.unwrap().phase, CallPhase::Active);

        // Les trames du pair (room == call_id) ouvrent son indicateur.
        let mut codec = Pcm8Codec;
        use accord_voice::AudioCodec;
        for seq in 0..5u16 {
            handle.peer_frame(
                friend,
                VoiceMsg::AudioFrame {
                    room: call_id,
                    media_type: MEDIA_AUDIO,
                    seq,
                    ts_ms: u32::from(seq) * 20,
                    payload: codec.encode(&tone()).unwrap(),
                },
            );
        }
        let speaking = eventually_status(&handle, |s| {
            s.participants
                .iter()
                .any(|p| p.pubkey == friend && p.speaking)
        })
        .await;
        assert!(speaking, "le pair d'appel n'est jamais passé « parle »");

        // Raccrochage local : signalisation émise, session fermée.
        handle.call_hangup().await.unwrap();
        assert_eq!(handle.call_status().await.unwrap().phase, CallPhase::Idle);
        assert!(handle.status().await.unwrap().is_none());
        let hangup = loop {
            match next_call_msg(&mut out).await {
                // Réémissions de l'offre, et annonces « décroché ailleurs »
                // destinées aux AUTRES appareils de l'appelé (lot 1.E) : ni
                // l'une ni l'autre ne conclut cet appel.
                Some((_, CoreMsg::CallOffer { .. } | CoreMsg::CallTaken { .. })) => continue,
                other => break other,
            }
        };
        assert_eq!(hangup, Some((friend, CoreMsg::CallHangup { call_id })));
    }

    #[tokio::test]
    async fn incoming_offer_rings_only_from_friends() {
        let n = node();
        let friend = make_friend(&n);
        let (handle, _, mut out) = spawn_engine_with_outbound(Arc::clone(&n));

        // Offre d'un inconnu : ignorée en silence (aucune réponse émise —
        // zéro amplification, pas de sonnerie).
        let stranger = Identity::generate_with_pow_bits(1).public_key();
        handle.peer_call(stranger, CoreMsg::CallOffer { call_id: [1; 16] });
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(handle.call_status().await.unwrap().phase, CallPhase::Idle);

        // Offre d'un ami : sonnerie entrante.
        handle.peer_call(friend, CoreMsg::CallOffer { call_id: [2; 16] });
        let ringing = eventually_phase(&handle, CallPhase::IncomingRinging).await;
        assert!(ringing, "l'offre d'un ami n'a pas déclenché la sonnerie");

        // Acceptation : réponse émise, session audio active.
        handle.call_accept([2; 16]).await.unwrap();
        assert_eq!(
            next_call_msg(&mut out).await,
            Some((friend, CoreMsg::CallAnswer { call_id: [2; 16] }))
        );
        assert_eq!(handle.call_status().await.unwrap().phase, CallPhase::Active);
        let status = handle.status().await.unwrap().unwrap();
        assert!(status.is_call);
        assert_eq!(status.group_id, [0u8; 16]);
        assert_eq!(status.channel_id, [2; 16]);
    }

    #[tokio::test]
    async fn ring_spam_is_limited_and_busy_decline_is_bounded() {
        let n = node();
        let friend = make_friend(&n);
        let caller2 = make_friend(&n);
        let (handle, _, mut out) = spawn_engine_with_outbound(Arc::clone(&n));

        // Rafale d'offres avec des call_id différents : une seule sonnerie
        // (cadence par pair), et AUCUNE réponse émise vers l'appelant.
        for i in 0..10u8 {
            handle.peer_call(friend, CoreMsg::CallOffer { call_id: [i; 16] });
        }
        let ringing = eventually_phase(&handle, CallPhase::IncomingRinging).await;
        assert!(ringing);
        // La sonnerie retenue est la première de la rafale.
        assert_eq!(handle.call_status().await.unwrap().call_id, Some([0; 16]));

        // Un second ami appelle pendant la sonnerie : refus « occupé »
        // (borné à un par fenêtre malgré la rafale).
        for _ in 0..5 {
            handle.peer_call(caller2, CoreMsg::CallOffer { call_id: [99; 16] });
        }
        let mut busy_replies = 0;
        while let Some((to, msg)) = next_call_msg(&mut out).await {
            if let CoreMsg::CallDecline { reason, .. } = msg {
                assert_eq!(to, caller2);
                assert_eq!(reason, accord_proto::core_msg::CALL_DECLINE_BUSY);
                busy_replies += 1;
            }
        }
        assert_eq!(busy_replies, 1, "réponses occupé non bornées");
    }

    #[tokio::test]
    async fn joining_a_group_room_hangs_up_the_active_call() {
        let n = node();
        let friend = make_friend(&n);
        let gid: [u8; 16] = hex::decode(&n.group_create("Guilde").unwrap()).unwrap();
        let (handle, _, mut out) = spawn_engine_with_outbound(Arc::clone(&n));

        handle.peer_call(friend, CoreMsg::CallOffer { call_id: [7; 16] });
        assert!(eventually_phase(&handle, CallPhase::IncomingRinging).await);
        handle.call_accept([7; 16]).await.unwrap();
        assert!(handle.status().await.unwrap().unwrap().is_call);

        // Rejoindre un salon de groupe raccroche l'appel actif.
        handle.join(gid, gid).await.unwrap();
        assert_eq!(handle.call_status().await.unwrap().phase, CallPhase::Idle);
        let status = handle.status().await.unwrap().unwrap();
        assert!(!status.is_call);
        assert_eq!(status.group_id, gid);
        // Le raccrochage a été signalé au pair.
        let mut saw_hangup = false;
        while let Some((to, msg)) = next_call_msg(&mut out).await {
            if msg == (CoreMsg::CallHangup { call_id: [7; 16] }) {
                assert_eq!(to, friend);
                saw_hangup = true;
            }
        }
        assert!(saw_hangup, "aucun CallHangup émis vers le pair");
    }

    // ---- Modération vocale (op 0x1F) ----

    #[tokio::test]
    async fn server_voice_mute_drops_member_frames_and_surfaces_flags() {
        let n = node();
        let gid: [u8; 16] = hex::decode(&n.group_create("Guilde").unwrap()).unwrap();
        let peer = Identity::generate_with_pow_bits(1).public_key();
        n.test_force_add_member(&gid, &peer).unwrap();
        let (handle, _) = spawn_engine(Arc::clone(&n));
        handle.join(gid, gid).await.unwrap();
        handle.peer_signal(peer, gid, gid, ACTION_JOIN, MEDIA_AUDIO, false);

        // Avant modération : les trames du pair ouvrent « parle ».
        let mut codec = Pcm8Codec;
        use accord_voice::AudioCodec;
        let mut seq = 0u16;
        let mut feed = |handle: &super::super::VoiceHandle, count: u16| {
            for _ in 0..count {
                handle.peer_frame(
                    peer,
                    VoiceMsg::AudioFrame {
                        room: gid,
                        media_type: MEDIA_AUDIO,
                        seq,
                        ts_ms: u32::from(seq) * 20,
                        payload: codec.encode(&tone()).unwrap(),
                    },
                );
                seq = seq.wrapping_add(1);
            }
        };
        feed(&handle, 5);
        let speaking = eventually_status(&handle, |s| {
            s.participants
                .iter()
                .any(|p| p.pubkey == peer && p.speaking)
        })
        .await;
        assert!(speaking, "le pair n'est jamais passé « parle »");

        // Le fondateur force le mute du pair (op 0x1F) ; le moteur est
        // notifié comme le fait le service après une op locale.
        n.group_voice_moderate(&gid, &peer, true, false).unwrap();
        handle.group_changed(gid);
        let flagged = eventually_status(&handle, |s| {
            s.participants
                .iter()
                .any(|p| p.pubkey == peer && p.server_muted && !p.server_deafened)
        })
        .await;
        assert!(flagged, "le drapeau server_muted n'apparaît pas");

        // L'indicateur « parle » se referme puis, malgré de nouvelles
        // trames, ne se rouvre jamais : elles sont jetées à la réception.
        let silent = eventually_status(&handle, |s| {
            s.participants
                .iter()
                .any(|p| p.pubkey == peer && !p.speaking)
        })
        .await;
        assert!(silent, "l'indicateur ne s'est pas refermé");
        feed(&handle, 10);
        tokio::time::sleep(Duration::from_millis(300)).await;
        let status = handle.status().await.unwrap().unwrap();
        assert!(
            status
                .participants
                .iter()
                .any(|p| p.pubkey == peer && !p.speaking),
            "des trames d'un membre server-muted ont été acceptées"
        );

        // Levée de la modération : les trames repassent.
        n.group_voice_moderate(&gid, &peer, false, false).unwrap();
        handle.group_changed(gid);
        let cleared = eventually_status(&handle, |s| {
            s.participants
                .iter()
                .any(|p| p.pubkey == peer && !p.server_muted)
        })
        .await;
        assert!(cleared, "la modération ne se lève pas");
        feed(&handle, 5);
        let speaking_again = eventually_status(&handle, |s| {
            s.participants
                .iter()
                .any(|p| p.pubkey == peer && p.speaking)
        })
        .await;
        assert!(speaking_again, "les trames ne repassent pas après la levée");
    }

    #[tokio::test]
    async fn signals_from_non_members_are_ignored() {
        let n = node();
        let gid: [u8; 16] = hex::decode(&n.group_create("Guilde").unwrap()).unwrap();
        let (handle, _) = spawn_engine(Arc::clone(&n));
        handle.join(gid, gid).await.unwrap();
        let stranger = Identity::generate_with_pow_bits(1).public_key();
        handle.peer_signal(stranger, gid, gid, ACTION_JOIN, MEDIA_AUDIO, false);
        tokio::time::sleep(Duration::from_millis(100)).await;
        let status = handle.status().await.unwrap().unwrap();
        assert_eq!(status.participants.len(), 1, "le non-membre a été admis");
    }

    /// v6.1 : dans un salon vocal de GROUPE, une trame de partage d'écran part
    /// vers TOUS les participants (full mesh, comme l'audio) — pas seulement
    /// vers un pair d'appel.
    #[tokio::test]
    async fn screen_frames_reach_every_group_room_participant() {
        let n = node();
        let gid: [u8; 16] = hex::decode(&n.group_create("Guilde").unwrap()).unwrap();
        let a = Identity::generate_with_pow_bits(1).public_key();
        let b = Identity::generate_with_pow_bits(1).public_key();
        n.test_force_add_member(&gid, &a).unwrap();
        n.test_force_add_member(&gid, &b).unwrap();
        let (handle, sender) = spawn_engine(Arc::clone(&n));
        handle.join(gid, gid).await.unwrap();
        handle.peer_signal(a, gid, gid, ACTION_JOIN, MEDIA_AUDIO, false);
        handle.peer_signal(b, gid, gid, ACTION_JOIN, MEDIA_AUDIO, false);
        tokio::time::sleep(Duration::from_millis(80)).await;

        handle.screen_send(true, vec![7u8; 32]);
        tokio::time::sleep(Duration::from_millis(120)).await;

        let sent = sender.0.lock().unwrap_or_else(|e| e.into_inner());
        let screen: Vec<[u8; 32]> = sent
            .iter()
            .filter(|(_, m)| matches!(m, VoiceMsg::ScreenFrame { .. }))
            .map(|(to, _)| *to)
            .collect();
        assert!(screen.contains(&a), "le participant A n'a rien reçu");
        assert!(screen.contains(&b), "le participant B n'a rien reçu");
        // La trame porte le salon du groupe, pas un identifiant d'appel.
        let room_ok = sent
            .iter()
            .any(|(_, m)| matches!(m, VoiceMsg::ScreenFrame { room, .. } if *room == gid));
        assert!(room_ok, "la trame ne porte pas le salon du groupe");
    }

    // ---- Vidéo sélective (§9.1) ----

    /// Salon de groupe avec deux participants, prêt à émettre de la vidéo.
    async fn room_with_two_peers(
        n: &Arc<Node>,
    ) -> (
        [u8; 16],
        [u8; 32],
        [u8; 32],
        super::super::VoiceHandle,
        Arc<TestSender>,
    ) {
        let gid: [u8; 16] = hex::decode(&n.group_create("Guilde").unwrap()).unwrap();
        let a = Identity::generate_with_pow_bits(1).public_key();
        let b = Identity::generate_with_pow_bits(1).public_key();
        n.test_force_add_member(&gid, &a).unwrap();
        n.test_force_add_member(&gid, &b).unwrap();
        let (handle, sender) = spawn_engine(Arc::clone(n));
        handle.join(gid, gid).await.unwrap();
        handle.peer_signal(a, gid, gid, ACTION_JOIN, MEDIA_AUDIO, false);
        handle.peer_signal(b, gid, gid, ACTION_JOIN, MEDIA_AUDIO, false);
        tokio::time::sleep(Duration::from_millis(80)).await;
        (gid, a, b, handle, sender)
    }

    /// Destinataires ayant reçu au moins un message satisfaisant `pred`, puis
    /// remise à zéro du journal (chaque phase de test part d'une page blanche).
    fn drain_recipients(sender: &TestSender, pred: impl Fn(&VoiceMsg) -> bool) -> Vec<[u8; 32]> {
        let mut sent = sender.0.lock().unwrap_or_else(|e| e.into_inner());
        let out = sent
            .iter()
            .filter(|(_, m)| pred(m))
            .map(|(to, _)| *to)
            .collect();
        sent.clear();
        out
    }

    fn is_camera_frame(m: &VoiceMsg) -> bool {
        matches!(m, VoiceMsg::CameraFrame { .. })
    }

    fn is_screen_frame(m: &VoiceMsg) -> bool {
        matches!(m, VoiceMsg::ScreenFrame { .. })
    }

    /// Un pair qui déclare ne pas afficher notre caméra cesse d'en recevoir les
    /// trames — c'est le gain d'émission recherché. Son voisin, muet, continue.
    #[tokio::test]
    async fn a_peer_that_hides_a_stream_stops_receiving_its_frames() {
        let n = node();
        let (gid, a, b, handle, sender) = room_with_two_peers(&n).await;

        // Avant toute déclaration : les deux reçoivent (comportement d'origine).
        handle.camera_send(true, vec![1u8; 32]);
        tokio::time::sleep(Duration::from_millis(120)).await;
        let before = drain_recipients(&sender, is_camera_frame);
        assert!(before.contains(&a) && before.contains(&b));

        // A déclare ne pas afficher notre caméra.
        handle.peer_frame(
            a,
            VoiceMsg::VideoInterest {
                room: gid,
                hidden: accord_proto::plaintext::VIDEO_HIDE_CAMERA,
            },
        );
        tokio::time::sleep(Duration::from_millis(60)).await;

        handle.camera_send(true, vec![2u8; 32]);
        tokio::time::sleep(Duration::from_millis(120)).await;
        let after = drain_recipients(&sender, is_camera_frame);
        assert!(
            !after.contains(&a),
            "A reçoit encore une caméra qu'il n'affiche pas"
        );
        assert!(after.contains(&b), "B, muet, ne reçoit plus rien");
    }

    /// Le masquage est PAR FLUX : masquer la caméra ne coupe pas l'écran (cas
    /// de l'épinglage — on regarde un partage d'écran, pas le visage).
    #[tokio::test]
    async fn hiding_the_camera_leaves_the_screen_share_flowing() {
        let n = node();
        let (gid, a, _b, handle, sender) = room_with_two_peers(&n).await;

        handle.peer_frame(
            a,
            VoiceMsg::VideoInterest {
                room: gid,
                hidden: accord_proto::plaintext::VIDEO_HIDE_CAMERA,
            },
        );
        tokio::time::sleep(Duration::from_millis(60)).await;

        handle.camera_send(true, vec![3u8; 32]);
        handle.screen_send(true, vec![4u8; 32]);
        tokio::time::sleep(Duration::from_millis(150)).await;
        let sent = sender.0.lock().unwrap_or_else(|e| e.into_inner());
        let cams: Vec<[u8; 32]> = sent
            .iter()
            .filter(|(_, m)| is_camera_frame(m))
            .map(|(to, _)| *to)
            .collect();
        let screens: Vec<[u8; 32]> = sent
            .iter()
            .filter(|(_, m)| is_screen_frame(m))
            .map(|(to, _)| *to)
            .collect();
        assert!(!cams.contains(&a), "la caméra masquée part quand même");
        assert!(
            screens.contains(&a),
            "l'écran a été coupé alors que seule la caméra était masquée"
        );
    }

    /// Revenir à l'affichage (masque nul) ramène les trames immédiatement.
    #[tokio::test]
    async fn unhiding_brings_the_stream_back() {
        let n = node();
        let (gid, a, _b, handle, sender) = room_with_two_peers(&n).await;

        handle.peer_frame(
            a,
            VoiceMsg::VideoInterest {
                room: gid,
                hidden: accord_proto::plaintext::VIDEO_HIDE_CAMERA,
            },
        );
        tokio::time::sleep(Duration::from_millis(60)).await;
        handle.camera_send(true, vec![5u8; 32]);
        tokio::time::sleep(Duration::from_millis(120)).await;
        assert!(!drain_recipients(&sender, is_camera_frame).contains(&a));

        // A ré-affiche : masque nul.
        handle.peer_frame(
            a,
            VoiceMsg::VideoInterest {
                room: gid,
                hidden: 0,
            },
        );
        tokio::time::sleep(Duration::from_millis(60)).await;
        handle.camera_send(true, vec![6u8; 32]);
        tokio::time::sleep(Duration::from_millis(120)).await;
        assert!(
            drain_recipients(&sender, is_camera_frame).contains(&a),
            "le flux ne revient pas après le ré-affichage"
        );
    }

    /// L'ANNONCE de flux ignore le masquage : sans elle, un pair qui a masqué
    /// une caméra n'apprendrait jamais qu'elle se rallume et resterait enfermé
    /// dans son propre masquage.
    #[tokio::test]
    async fn stream_announces_reach_even_a_peer_that_hid_the_stream() {
        let n = node();
        let (gid, a, _b, handle, sender) = room_with_two_peers(&n).await;

        handle.peer_frame(
            a,
            VoiceMsg::VideoInterest {
                room: gid,
                hidden: accord_proto::plaintext::VIDEO_HIDE_CAMERA
                    | accord_proto::plaintext::VIDEO_HIDE_SCREEN,
            },
        );
        tokio::time::sleep(Duration::from_millis(60)).await;

        handle.camera_announce(true);
        tokio::time::sleep(Duration::from_millis(120)).await;
        let announced = drain_recipients(&sender, |m| matches!(m, VoiceMsg::CameraControl { .. }));
        assert!(
            announced.contains(&a),
            "l'annonce a été filtrée : le pair ne saura jamais quoi ré-afficher"
        );
    }

    /// Une déclaration venue d'un pair HORS du salon actif, ou portant un autre
    /// salon, n'éteint rien : sinon un tiers couperait la vidéo des autres.
    #[tokio::test]
    async fn an_interest_from_an_outsider_or_another_room_is_ignored() {
        let n = node();
        let (gid, a, b, handle, sender) = room_with_two_peers(&n).await;
        let stranger = Identity::generate_with_pow_bits(1).public_key();

        handle.peer_frame(
            stranger,
            VoiceMsg::VideoInterest {
                room: gid,
                hidden: accord_proto::plaintext::VIDEO_HIDE_CAMERA,
            },
        );
        // Bon pair, mais salon étranger.
        handle.peer_frame(
            a,
            VoiceMsg::VideoInterest {
                room: [0x77; 16],
                hidden: accord_proto::plaintext::VIDEO_HIDE_CAMERA,
            },
        );
        tokio::time::sleep(Duration::from_millis(60)).await;

        handle.camera_send(true, vec![7u8; 32]);
        tokio::time::sleep(Duration::from_millis(120)).await;
        let got = drain_recipients(&sender, is_camera_frame);
        assert!(got.contains(&a) && got.contains(&b), "un flux a été coupé");
    }

    /// Côté récepteur : la déclaration locale part sur le fil, une seule fois
    /// par changement, vers les seuls pairs de la session.
    #[tokio::test]
    async fn local_interest_is_declared_once_per_change() {
        let n = node();
        let (_gid, a, b, handle, sender) = room_with_two_peers(&n).await;
        let outsider = Identity::generate_with_pow_bits(1).public_key();

        let hidden = accord_proto::plaintext::VIDEO_HIDE_CAMERA;
        handle.video_interest(vec![(a, hidden), (outsider, hidden)]);
        tokio::time::sleep(Duration::from_millis(80)).await;
        let declared = drain_recipients(&sender, |m| matches!(m, VoiceMsg::VideoInterest { .. }));
        assert_eq!(
            declared,
            vec![a],
            "la déclaration doit viser A seul (pas B, pas un pair hors session)"
        );

        // Même état re-déclaré : rien ne repart (les re-rendus sont fréquents).
        handle.video_interest(vec![(a, hidden)]);
        tokio::time::sleep(Duration::from_millis(80)).await;
        assert!(
            drain_recipients(&sender, |m| matches!(m, VoiceMsg::VideoInterest { .. })).is_empty(),
            "une déclaration inchangée a été réémise"
        );

        // Retour à l'affichage : un masque nul explicite part vers A.
        handle.video_interest(vec![]);
        tokio::time::sleep(Duration::from_millis(80)).await;
        let sent = sender.0.lock().unwrap_or_else(|e| e.into_inner());
        assert!(
            sent.iter()
                .any(|(to, m)| *to == a && matches!(m, VoiceMsg::VideoInterest { hidden: 0, .. })),
            "aucun masque nul émis au retour à l'affichage"
        );
        assert!(!sent.iter().any(|(to, _)| *to == b));
    }

    /// Une trame vidéo d'un pair qui n'est PAS dans le salon actif est ignorée
    /// (défense en profondeur, même règle que l'audio).
    #[tokio::test]
    async fn video_frames_from_non_participants_are_ignored() {
        let n = node();
        let gid: [u8; 16] = hex::decode(&n.group_create("Guilde").unwrap()).unwrap();
        let (handle, _) = spawn_engine(Arc::clone(&n));
        handle.join(gid, gid).await.unwrap();
        let stranger = Identity::generate_with_pow_bits(1).public_key();

        // Aucun hub branché : on vérifie surtout que le moteur ne panique pas
        // et reste cohérent (le salon garde son unique participant).
        handle.peer_frame(
            stranger,
            VoiceMsg::ScreenFrame {
                room: gid,
                frame_id: 1,
                frag_count: 1,
                frag_idx: 0,
                flags: VIDEO_FLAG_KEYFRAME,
                payload: vec![1, 2, 3],
            },
        );
        tokio::time::sleep(Duration::from_millis(80)).await;
        let status = handle.status().await.unwrap().unwrap();
        assert_eq!(status.participants.len(), 1);
    }
}
