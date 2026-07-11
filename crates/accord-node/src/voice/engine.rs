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
use accord_proto::core_msg::CoreMsg;
use accord_proto::limits::VOICE_MAX_PARTICIPANTS;
use accord_proto::plaintext::VoiceMsg;
use accord_voice::gain;
use accord_voice::params::{FRAME_MS, FRAME_SAMPLES};
use accord_voice::room::CodecFactory;
use accord_voice::{Pcm8Codec, VoiceRoom};
use serde_json::json;
use tokio::sync::mpsc;

use super::roster::{Roster, RosterEvent, ACTIVE_TIMEOUT_MS, PASSIVE_TTL_MS};
use super::{
    Cmd, FrameSender, VoiceBackend, VoiceDeps, VoiceDevices, VoiceParticipant, VoiceStatus,
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
/// Trames de capture injectées en attente au maximum.
const MAX_INJECTED_FRAMES: usize = 64;
/// Une émission `event.voice_level` toutes les 5 trames de 20 ms (~10 Hz).
#[cfg(feature = "hardware")]
const LEVEL_PERIOD_TICKS: u64 = 5;

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

/// Salon vocal actif (celui que l'on a rejoint).
struct Active {
    group_id: [u8; 16],
    channel_id: [u8; 16],
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
    #[cfg(feature = "hardware")]
    hw: Option<super::hw::HardwareIo>,
    /// Test micro en cours (`event.voice_level` à ~10 Hz, D-029).
    #[cfg(feature = "hardware")]
    mic_test: Option<MicTest>,
    epoch: Instant,
    tick_count: u64,
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
            #[cfg(feature = "hardware")]
            hw: None,
            #[cfg(feature = "hardware")]
            mic_test: None,
            epoch: Instant::now(),
            tick_count: 0,
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
                self.leave_active();
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
            Cmd::Stop => unreachable!("Stop traité par la boucle"),
        }
    }

    /// Rejoint un salon ; quitte l'ancien implicitement (contrat gelé).
    fn handle_join(
        &mut self,
        group_id: [u8; 16],
        channel_id: [u8; 16],
    ) -> Result<Vec<[u8; 32]>, NodeError> {
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
            self.hw = Some(super::hw::HardwareIo::open(
                self.input_device.clone(),
                self.output_device.clone(),
            ));
        }
        self.active = Some(Active {
            group_id,
            channel_id,
            muted: false,
            deafened: false,
            mute_restore: false,
            room,
        });
        Ok(participants)
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

    /// Quitte le salon actif : signal de départ, événements, libération du
    /// matériel. Sans effet hors salon.
    fn leave_active(&mut self) {
        let Some(active) = self.active.take() else {
            return;
        };
        let key = active.key();
        self.broadcast_signal(
            active.group_id,
            active.channel_id,
            ACTION_LEAVE,
            active.muted,
            active.deafened,
        );
        let me = self.node.public_key();
        let mut events = Vec::new();
        if let Some(roster) = self.rooms.get_mut(&key) {
            if let Some(event) = roster.force_silent(&me) {
                events.push(event);
            }
            if roster.leave(&me) {
                events.push(RosterEvent::Left(me));
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
    /// requested mute state. Idempotent; no effect outside a channel.
    fn handle_deafen(&mut self, deafened: bool) {
        let Some(active) = self.active.as_mut() else {
            return;
        };
        if active.deafened == deafened {
            return;
        }
        active.deafened = deafened;
        active.muted = if deafened { true } else { active.mute_restore };
        active.room.set_deafened(deafened);
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
        self.broadcast_signal(gid, cid, ACTION_STATE, muted, deafened);
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

    fn handle_status(&mut self) -> Option<VoiceStatus> {
        let active = self.active.as_ref()?;
        let key = active.key();
        let (group_id, channel_id, muted, deafened) = (
            active.group_id,
            active.channel_id,
            active.muted,
            active.deafened,
        );
        let peers = self
            .rooms
            .get(&key)
            .map(Roster::participants)
            .unwrap_or_default();
        let participants = peers
            .into_iter()
            .map(|p| {
                let volume = self.volume_for(&p.pubkey);
                VoiceParticipant {
                    pubkey: p.pubkey,
                    speaking: p.speaking,
                    muted: p.muted,
                    deafened: p.deafened,
                    volume,
                }
            })
            .collect();
        Some(VoiceStatus {
            group_id,
            channel_id,
            muted,
            deafened,
            participants,
        })
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
        }
        if let Some(event) = event {
            self.emit_room(key, &event);
        }
    }

    /// Passe cadencée à 20 ms : test micro, capture → encodage → diffusion,
    /// lecture, pings de qualité, vivacité et balayage des présences
    /// passives.
    async fn on_tick(&mut self) {
        #[cfg(feature = "hardware")]
        self.tick_mic_test();
        self.tick_count += 1;
        let now = self.now_ms();
        let me = self.node.public_key();
        let pcm = self.next_capture();
        let mut events: Vec<(RoomKey, RosterEvent)> = Vec::new();
        let mut to_send: Vec<([u8; 32], VoiceMsg)> = Vec::new();

        if let Some(active) = self.active.as_mut() {
            let key = active.key();
            // Capture locale (la VAD décide de la transmission).
            if !active.muted {
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
                // Lecture des participants (le tampon de gigue cadence).
                for pk in roster.pubkeys() {
                    if pk == me {
                        continue;
                    }
                    if let Ok(Some(_decoded)) = active.room.play(&pk) {
                        #[cfg(feature = "hardware")]
                        if let Some(hw) = &self.hw {
                            hw.play(_decoded);
                        }
                    }
                    // Retour de qualité périodique (SPEC §8).
                    if self.tick_count % PING_PERIOD_TICKS == 0 {
                        if let Some(ping) = active.room.quality_ping(&pk, 0) {
                            to_send.push((pk, ping));
                        }
                    }
                }
                // Vivacité : soi-même n'expire jamais (rafraîchi ici).
                roster.touch(&me, now);
                for event in roster.tick(now, ACTIVE_TIMEOUT_MS, Some(&me)) {
                    if let RosterEvent::Left(pk) = &event {
                        active.room.remove_participant(pk);
                    }
                    events.push((key, event));
                }
            }
            // Rafraîchit la présence passive des membres hors salon.
            if self.tick_count % STATE_PERIOD_TICKS == 0 {
                let (gid, cid, muted, deafened) = (
                    active.group_id,
                    active.channel_id,
                    active.muted,
                    active.deafened,
                );
                self.broadcast_signal(gid, cid, ACTION_STATE, muted, deafened);
            }
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
}
