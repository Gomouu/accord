//! Sous-système voix du nœud (SPEC §8, D-020, D-025).
//!
//! Une tâche dédiée cadencée à 20 ms ([`engine`]) possède le salon vocal
//! ([`accord_voice::VoiceRoom`]) et le codec : capture (matérielle avec la
//! feature `hardware`, injectée sinon), encodage, diffusion full mesh des
//! trames aux participants via les sessions chiffrées (canal VOICE), lecture
//! des trames reçues, signalisation `VoiceSignal` (canal CORE) et présence
//! des salons. Le reste du nœud lui parle par un [`VoiceHandle`] clonable.

pub(crate) mod calls;
mod engine;
mod media;
mod roster;

#[cfg(feature = "hardware")]
mod hw;

use std::sync::Arc;

use accord_api::NotificationHub;
use accord_proto::core_msg::CoreMsg;
use accord_proto::plaintext::VoiceMsg;
use tokio::sync::{mpsc, oneshot};

use crate::error::NodeError;
use crate::node::Node;
use crate::outbound::OutboundSink;

pub use calls::{CallPhase, CallSnapshot};

/// Mode d'exécution du sous-système voix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VoiceBackend {
    /// Codec Opus et périphériques audio réels si la feature `hardware` est
    /// compilée ; repli automatique sur le mode simulé sinon.
    #[default]
    Materiel,
    /// Codec PCM 8 bits pur et capture injectée ([`VoiceHandle::inject_pcm`]) :
    /// déterministe, sans matériel ni bibliothèque native (tests, CI).
    Simule,
}

/// Participant d'un salon vocal tel qu'exposé par [`VoiceHandle::status`].
#[derive(Debug, Clone)]
pub struct VoiceParticipant {
    /// Clé publique Ed25519 du participant.
    pub pubkey: [u8; 32],
    /// Vrai si le participant parle (VAD locale pour soi, activité des trames
    /// pour les pairs, hystérésis dans les deux cas).
    pub speaking: bool,
    /// Microphone muted, as broadcast by the participant.
    pub muted: bool,
    /// Output deafened, as broadcast by the participant.
    pub deafened: bool,
    /// Local output volume for this participant in percent (0..=200,
    /// persisted per public key, 100 = unity).
    pub volume: u16,
    /// Force-muted by a group moderator (op 0x1F, group rooms only).
    pub server_muted: bool,
    /// Force-deafened by a group moderator (op 0x1F, group rooms only).
    pub server_deafened: bool,
    /// Holds the `PRIORITY_SPEAKER` permission in this group: while speaking,
    /// the other participants are ducked locally.
    pub priority_speaker: bool,
}

/// État du salon vocal actif.
#[derive(Debug, Clone)]
pub struct VoiceStatus {
    /// Groupe du salon (tout à zéro pour la session audio d'un appel 1-à-1).
    pub group_id: [u8; 16],
    /// Salon vocal (par convention UI, `channel_id == group_id` ; pour un
    /// appel 1-à-1, `channel_id == call_id`).
    pub channel_id: [u8; 16],
    /// Vrai si la session active est celle d'un appel 1-à-1.
    pub is_call: bool,
    /// Micro local coupé.
    pub muted: bool,
    /// Local output deafened (implies `muted`, Discord semantics).
    pub deafened: bool,
    /// Participants du salon, soi-même inclus.
    pub participants: Vec<VoiceParticipant>,
}

impl VoiceStatus {
    /// Vrai si la session active est exactement le salon vocal de groupe
    /// `(group_id, channel_id)` — jamais un appel 1-à-1. Prédicat partagé par
    /// le gate d'ÉMISSION du soundboard (l'appelant doit être connecté à ce
    /// salon) et par le gate de RÉCEPTION (le son ne se joue que pour les
    /// participants de CE salon).
    pub(crate) fn is_room(&self, group_id: &[u8; 16], channel_id: &[u8; 16]) -> bool {
        !self.is_call && &self.group_id == group_id && &self.channel_id == channel_id
    }
}

/// Présence d'un salon vocal connu, rejoint ou non ([`VoiceHandle::rooms`]) :
/// permet à l'UI d'afficher les occupants d'un salon avant de le rejoindre.
#[derive(Debug, Clone)]
pub struct VoiceRoomPresence {
    /// Groupe du salon.
    pub group_id: [u8; 16],
    /// Salon vocal (par convention UI, `channel_id == group_id`).
    pub channel_id: [u8; 16],
    /// Occupants connus du salon (présence passive incluse).
    pub participants: Vec<VoiceParticipant>,
}

/// Périphériques audio exposés par `voice.devices` (contrat gelé, D-029).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VoiceDevices {
    /// Noms `cpal` des entrées disponibles (vide sans matériel).
    pub inputs: Vec<String>,
    /// Noms `cpal` des sorties disponibles (vide sans matériel).
    pub outputs: Vec<String>,
    /// Entrée choisie (`None` = périphérique par défaut).
    pub selected_input: Option<String>,
    /// Sortie choisie (`None` = périphérique par défaut).
    pub selected_output: Option<String>,
}

/// Envoi bas niveau d'un message voix à un pair (session chiffrée, canal
/// VOICE). Implémenté par le runtime réseau ; substituable dans les tests.
#[async_trait::async_trait]
pub(crate) trait FrameSender: Send + Sync {
    /// Envoie `msg` au pair `to` ; rend `false` si le pair est injoignable.
    async fn send_voice(&self, to: &[u8; 32], msg: VoiceMsg) -> bool;
}

/// Dépendances du moteur voix.
pub(crate) struct VoiceDeps {
    /// État applicatif (adhésion aux groupes, clé publique locale).
    pub(crate) node: Arc<Node>,
    /// Émission des signalisations `VoiceSignal` (canal CORE).
    pub(crate) outbound: OutboundSink,
    /// Diffusion des événements `event.voice_*` vers l'UI.
    pub(crate) hub: Option<NotificationHub>,
    /// Envoi des trames et pings voix.
    pub(crate) sender: Arc<dyn FrameSender>,
    /// Mode d'exécution (matériel ou simulé).
    pub(crate) backend: VoiceBackend,
}

/// Commande adressée au moteur voix.
pub(crate) enum Cmd {
    /// Rejoindre un salon (quitte l'ancien implicitement).
    Join {
        /// Groupe visé.
        group_id: [u8; 16],
        /// Salon visé.
        channel_id: [u8; 16],
        /// Réponse : participants du salon (soi-même inclus).
        resp: oneshot::Sender<Result<Vec<[u8; 32]>, NodeError>>,
    },
    /// Quitter le salon actif (sans effet si aucun).
    Leave {
        /// Accusé de traitement.
        resp: oneshot::Sender<()>,
    },
    /// Couper/rétablir la capture locale.
    Mute {
        /// Vrai pour couper le micro.
        muted: bool,
        /// Accusé de traitement.
        resp: oneshot::Sender<()>,
    },
    /// Deafen/undeafen the local output (deafen forces mute; undeafen
    /// restores the previous mute state — Discord semantics).
    Deafen {
        /// True to deafen.
        deafened: bool,
        /// Processing acknowledgment.
        resp: oneshot::Sender<()>,
    },
    /// Set an output volume (persisted; applied live to the active room).
    SetVolume {
        /// Target peer (`None` = master output volume).
        peer: Option<[u8; 32]>,
        /// Volume in percent (0..=200, validated).
        volume: u16,
        /// Response: explicit error when out of range or unpersistable.
        resp: oneshot::Sender<Result<(), NodeError>>,
    },
    /// Persisted master output volume in percent.
    MasterVolume {
        /// Response: 0..=200 (default 100).
        resp: oneshot::Sender<u16>,
    },
    /// État du salon actif.
    Status {
        /// Réponse : `None` hors salon.
        resp: oneshot::Sender<Option<VoiceStatus>>,
    },
    /// Présence de tous les salons connus (actif et passifs).
    Rooms {
        /// Réponse : un descripteur par salon connu.
        resp: oneshot::Sender<Vec<VoiceRoomPresence>>,
    },
    /// Périphériques audio disponibles et sélection persistée (D-029).
    Devices {
        /// Réponse : listes vides et sélections `None` sans matériel.
        resp: oneshot::Sender<Result<VoiceDevices, NodeError>>,
    },
    /// Choix des périphériques audio (persisté, appliqué à chaud).
    SetDevices {
        /// Entrée : `None` = inchangée, `Some(None)` = défaut, sinon nom.
        input: Option<Option<String>>,
        /// Sortie : mêmes conventions que l'entrée.
        output: Option<Option<String>>,
        /// Réponse : erreur explicite si un nom est inconnu.
        resp: oneshot::Sender<Result<(), NodeError>>,
    },
    /// Active/désactive le test micro (`event.voice_level` à ~10 Hz).
    MicTest {
        /// Vrai pour démarrer la capture de test.
        enabled: bool,
        /// Réponse : erreur explicite si le matériel audio est indisponible.
        resp: oneshot::Sender<Result<(), NodeError>>,
    },
    /// `calls.start` : lance un appel 1-à-1 vers un ami.
    CallStart {
        /// Appelé (clé publique Ed25519).
        peer: [u8; 32],
        /// Réponse : identifiant d'appel, ou erreur explicite (non-ami,
        /// appel déjà en cours).
        resp: oneshot::Sender<Result<[u8; 16], NodeError>>,
    },
    /// `calls.accept` : accepte la sonnerie entrante.
    CallAccept {
        /// Appel accepté (doit corréler la sonnerie courante).
        call_id: [u8; 16],
        /// Réponse : erreur explicite si aucun appel entrant ne corrèle.
        resp: oneshot::Sender<Result<(), NodeError>>,
    },
    /// `calls.decline` : refuse la sonnerie entrante.
    CallDecline {
        /// Appel refusé (doit corréler la sonnerie courante).
        call_id: [u8; 16],
        /// Réponse : erreur explicite si aucun appel entrant ne corrèle.
        resp: oneshot::Sender<Result<(), NodeError>>,
    },
    /// `calls.hangup` : termine l'appel courant (toute phase, idempotent).
    CallHangup {
        /// Accusé de traitement.
        resp: oneshot::Sender<()>,
    },
    /// `calls.status` : photographie de l'appel courant.
    CallStatus {
        /// Réponse : phase, pair et identifiant le cas échéant.
        resp: oneshot::Sender<CallSnapshot>,
    },
    /// Message d'appel (`CallOffer`/`CallAnswer`/`CallDecline`/`CallHangup`)
    /// reçu d'un pair authentifié (routé par le runtime).
    PeerCall {
        /// Émetteur (clé de session).
        from: [u8; 32],
        /// Message d'appel (les autres variantes sont ignorées).
        msg: CoreMsg,
    },
    /// Réglages DSP de capture (persistés, appliqués à chaud).
    SetDsp {
        /// Suppression de bruit : `None` = inchangée.
        noise_suppression: Option<bool>,
        /// Contrôle automatique de gain : `None` = inchangé.
        agc: Option<bool>,
        /// Annulation d'écho (D-051) : `None` = inchangée.
        echo_cancel: Option<bool>,
        /// Réponse : erreur si la persistance échoue.
        resp: oneshot::Sender<Result<(), NodeError>>,
    },
    /// Réglages DSP courants `(suppression de bruit, AGC, annulation d'écho)`.
    DspConfig {
        /// Réponse : drapeaux courants.
        resp: oneshot::Sender<(bool, bool, bool)>,
    },
    /// L'op-log d'un groupe a changé : rafraîchit la modération vocale et la
    /// priorité d'orateur du salon actif le cas échéant.
    GroupChanged {
        /// Groupe modifié.
        group_id: [u8; 16],
    },
    /// Signalisation `VoiceSignal` reçue d'un pair authentifié.
    PeerSignal {
        /// Émetteur (clé de session).
        from: [u8; 32],
        /// Groupe du salon.
        group_id: [u8; 16],
        /// Salon visé.
        channel_id: [u8; 16],
        /// 0=rejoint, 1=quitte, 2=état.
        action: u8,
        /// Media bitflags (0x01 audio; bit 0x80 carries the deafen state).
        media_kinds: u8,
        /// Microphone muted, as broadcast by the sender.
        mute: bool,
    },
    /// Message du canal VOICE reçu d'un pair authentifié.
    PeerFrame {
        /// Émetteur (clé de session).
        from: [u8; 32],
        /// Trame audio ou ping de qualité.
        msg: VoiceMsg,
    },
    /// Trame PCM de capture injectée (mode simulé).
    InjectPcm {
        /// 960 échantillons mono 48 kHz.
        pcm: Vec<i16>,
    },
    /// Diffuse une trame vidéo encodée (UI → pair d'appel) sur l'un des deux
    /// flux vidéo de l'appel.
    VideoSend {
        /// Flux concerné (partage d'écran ou caméra).
        stream: media::VideoStream,
        /// Vrai si la trame est une keyframe (décodable indépendamment).
        keyframe: bool,
        /// Trame vidéo encodée (WebCodecs), fragmentée par le moteur.
        encoded: Vec<u8>,
    },
    /// Annonce le démarrage (`on == true`) ou l'arrêt d'un flux vidéo au pair
    /// de l'appel actif.
    VideoAnnounce {
        /// Flux concerné.
        stream: media::VideoStream,
        /// Vrai = flux démarré, faux = arrêté.
        on: bool,
    },
    /// Arrêt du moteur (quitte proprement le salon actif).
    Stop,
}

/// Poignée clonable vers le moteur voix.
#[derive(Clone)]
pub struct VoiceHandle {
    tx: mpsc::UnboundedSender<Cmd>,
}

impl VoiceHandle {
    /// Erreur uniforme quand le moteur est arrêté.
    fn stopped() -> NodeError {
        NodeError::NotFound("sous-système voix arrêté")
    }

    /// Rejoint un salon vocal ; quitte l'ancien salon implicitement. Rend les
    /// participants du salon (soi-même inclus). Erreur explicite si le salon
    /// est plein (10 participants) ou si l'on n'est pas membre du groupe.
    pub async fn join(
        &self,
        group_id: [u8; 16],
        channel_id: [u8; 16],
    ) -> Result<Vec<[u8; 32]>, NodeError> {
        let (resp, rx) = oneshot::channel();
        self.tx
            .send(Cmd::Join {
                group_id,
                channel_id,
                resp,
            })
            .map_err(|_| Self::stopped())?;
        rx.await.map_err(|_| Self::stopped())?
    }

    /// Quitte le salon vocal actif (sans effet si aucun).
    pub async fn leave(&self) -> Result<(), NodeError> {
        let (resp, rx) = oneshot::channel();
        self.tx
            .send(Cmd::Leave { resp })
            .map_err(|_| Self::stopped())?;
        rx.await.map_err(|_| Self::stopped())
    }

    /// Coupe (`true`) ou rétablit (`false`) la capture locale ; on reste dans
    /// le salon.
    pub async fn set_muted(&self, muted: bool) -> Result<(), NodeError> {
        let (resp, rx) = oneshot::channel();
        self.tx
            .send(Cmd::Mute { muted, resp })
            .map_err(|_| Self::stopped())?;
        rx.await.map_err(|_| Self::stopped())
    }

    /// Deafens (`true`) or restores (`false`) the local output. Deafening
    /// stops decoding/playing every incoming voice and forces mute;
    /// undeafening restores the mute state requested before (or during) the
    /// deafen. Idempotent; no effect outside a voice channel.
    pub async fn set_deafened(&self, deafened: bool) -> Result<(), NodeError> {
        let (resp, rx) = oneshot::channel();
        self.tx
            .send(Cmd::Deafen { deafened, resp })
            .map_err(|_| Self::stopped())?;
        rx.await.map_err(|_| Self::stopped())
    }

    /// Sets an output volume in percent (0..=200; 100 = unity). `peer: None`
    /// targets the master output volume. Persisted (per peer public key for
    /// participants) and applied live to the active room.
    pub async fn set_volume(&self, peer: Option<[u8; 32]>, volume: u16) -> Result<(), NodeError> {
        let (resp, rx) = oneshot::channel();
        self.tx
            .send(Cmd::SetVolume { peer, volume, resp })
            .map_err(|_| Self::stopped())?;
        rx.await.map_err(|_| Self::stopped())?
    }

    /// Persisted master output volume in percent (default 100).
    pub async fn master_volume(&self) -> Result<u16, NodeError> {
        let (resp, rx) = oneshot::channel();
        self.tx
            .send(Cmd::MasterVolume { resp })
            .map_err(|_| Self::stopped())?;
        rx.await.map_err(|_| Self::stopped())
    }

    /// État du salon vocal actif (`None` hors salon).
    pub async fn status(&self) -> Result<Option<VoiceStatus>, NodeError> {
        let (resp, rx) = oneshot::channel();
        self.tx
            .send(Cmd::Status { resp })
            .map_err(|_| Self::stopped())?;
        rx.await.map_err(|_| Self::stopped())
    }

    /// Présence de tous les salons connus (rejoints ou non), pour afficher les
    /// occupants d'un salon avant de le rejoindre.
    pub async fn rooms(&self) -> Result<Vec<VoiceRoomPresence>, NodeError> {
        let (resp, rx) = oneshot::channel();
        self.tx
            .send(Cmd::Rooms { resp })
            .map_err(|_| Self::stopped())?;
        rx.await.map_err(|_| Self::stopped())
    }

    /// Périphériques audio disponibles (noms `cpal`) et sélection persistée.
    /// Sans matériel (mode simulé, feature `hardware` absente) : listes vides
    /// et sélections `None`.
    pub async fn devices(&self) -> Result<VoiceDevices, NodeError> {
        let (resp, rx) = oneshot::channel();
        self.tx
            .send(Cmd::Devices { resp })
            .map_err(|_| Self::stopped())?;
        rx.await.map_err(|_| Self::stopped())?
    }

    /// Choisit les périphériques audio. Champ `None` = inchangé,
    /// `Some(None)` = périphérique par défaut, `Some(Some(nom))` = nom `cpal`
    /// exact (erreur explicite si le nom est inconnu en mode matériel). Le
    /// choix est persisté et appliqué à chaud si un salon est actif.
    pub async fn set_devices(
        &self,
        input: Option<Option<String>>,
        output: Option<Option<String>>,
    ) -> Result<(), NodeError> {
        let (resp, rx) = oneshot::channel();
        self.tx
            .send(Cmd::SetDevices {
                input,
                output,
                resp,
            })
            .map_err(|_| Self::stopped())?;
        rx.await.map_err(|_| Self::stopped())?
    }

    /// Active (`true`) ou coupe (`false`) le test micro : pendant
    /// l'activation, `event.voice_level` est émis à ~10 Hz depuis la capture
    /// réelle. Erreur explicite si le matériel audio est indisponible ; la
    /// désactivation est idempotente.
    pub async fn mic_test(&self, enabled: bool) -> Result<(), NodeError> {
        let (resp, rx) = oneshot::channel();
        self.tx
            .send(Cmd::MicTest { enabled, resp })
            .map_err(|_| Self::stopped())?;
        rx.await.map_err(|_| Self::stopped())?
    }

    /// Lance un appel 1-à-1 vers un ami confirmé ; rend l'identifiant
    /// d'appel. Erreur explicite si l'appelé n'est pas un ami ou si un appel
    /// est déjà en cours (état « occupé »).
    pub async fn call_start(&self, peer: [u8; 32]) -> Result<[u8; 16], NodeError> {
        let (resp, rx) = oneshot::channel();
        self.tx
            .send(Cmd::CallStart { peer, resp })
            .map_err(|_| Self::stopped())?;
        rx.await.map_err(|_| Self::stopped())?
    }

    /// Accepte la sonnerie entrante identifiée par `call_id` : la session
    /// audio démarre (le salon vocal actif éventuel est quitté).
    pub async fn call_accept(&self, call_id: [u8; 16]) -> Result<(), NodeError> {
        let (resp, rx) = oneshot::channel();
        self.tx
            .send(Cmd::CallAccept { call_id, resp })
            .map_err(|_| Self::stopped())?;
        rx.await.map_err(|_| Self::stopped())?
    }

    /// Refuse la sonnerie entrante identifiée par `call_id`.
    pub async fn call_decline(&self, call_id: [u8; 16]) -> Result<(), NodeError> {
        let (resp, rx) = oneshot::channel();
        self.tx
            .send(Cmd::CallDecline { call_id, resp })
            .map_err(|_| Self::stopped())?;
        rx.await.map_err(|_| Self::stopped())?
    }

    /// Termine l'appel courant, quelle que soit sa phase (annule une
    /// sonnerie sortante, refuse une entrante, raccroche un appel actif).
    /// Idempotent au repos.
    pub async fn call_hangup(&self) -> Result<(), NodeError> {
        let (resp, rx) = oneshot::channel();
        self.tx
            .send(Cmd::CallHangup { resp })
            .map_err(|_| Self::stopped())?;
        rx.await.map_err(|_| Self::stopped())
    }

    /// Photographie de l'appel courant (`idle` au repos).
    pub async fn call_status(&self) -> Result<CallSnapshot, NodeError> {
        let (resp, rx) = oneshot::channel();
        self.tx
            .send(Cmd::CallStatus { resp })
            .map_err(|_| Self::stopped())?;
        rx.await.map_err(|_| Self::stopped())
    }

    /// Transmet un message d'appel reçu d'un pair authentifié (point
    /// d'entrée du routeur réseau ; les variantes non-appel sont ignorées).
    pub fn peer_call(&self, from: [u8; 32], msg: CoreMsg) {
        let _ = self.tx.send(Cmd::PeerCall { from, msg });
    }

    /// Règle la chaîne DSP de capture (champ `None` = inchangé) ; persisté
    /// et appliqué à chaud à la session active.
    pub async fn set_dsp(
        &self,
        noise_suppression: Option<bool>,
        agc: Option<bool>,
        echo_cancel: Option<bool>,
    ) -> Result<(), NodeError> {
        let (resp, rx) = oneshot::channel();
        self.tx
            .send(Cmd::SetDsp {
                noise_suppression,
                agc,
                echo_cancel,
                resp,
            })
            .map_err(|_| Self::stopped())?;
        rx.await.map_err(|_| Self::stopped())?
    }

    /// Réglages DSP courants `(suppression de bruit, AGC, annulation d'écho)`.
    pub async fn dsp_config(&self) -> Result<(bool, bool, bool), NodeError> {
        let (resp, rx) = oneshot::channel();
        self.tx
            .send(Cmd::DspConfig { resp })
            .map_err(|_| Self::stopped())?;
        rx.await.map_err(|_| Self::stopped())
    }

    /// Signale un changement d'op-log de groupe (modération vocale,
    /// priorités) ; sans effet hors du salon actif de ce groupe.
    pub fn group_changed(&self, group_id: [u8; 16]) {
        let _ = self.tx.send(Cmd::GroupChanged { group_id });
    }

    /// Injecte une trame PCM (960 échantillons mono 48 kHz) comme capture
    /// locale — source de substitution du mode simulé (tests, machines sans
    /// matériel audio).
    pub fn inject_pcm(&self, pcm: Vec<i16>) {
        let _ = self.tx.send(Cmd::InjectPcm { pcm });
    }

    /// Transmet une signalisation `VoiceSignal` reçue d'un pair authentifié.
    /// Point d'entrée du routeur réseau ; exposé aussi pour les tests
    /// d'intégration (l'adhésion au groupe est re-vérifiée par le moteur).
    /// `media_kinds`/`mute` carry the sender's broadcast state (bit 0x80 of
    /// `media_kinds` = deafened; unknown bits are ignored).
    pub fn peer_signal(
        &self,
        from: [u8; 32],
        group_id: [u8; 16],
        channel_id: [u8; 16],
        action: u8,
        media_kinds: u8,
        mute: bool,
    ) {
        let _ = self.tx.send(Cmd::PeerSignal {
            from,
            group_id,
            channel_id,
            action,
            media_kinds,
            mute,
        });
    }

    /// Transmet un message du canal VOICE reçu d'un pair authentifié (point
    /// d'entrée du routeur réseau).
    pub fn peer_frame(&self, from: [u8; 32], msg: VoiceMsg) {
        let _ = self.tx.send(Cmd::PeerFrame { from, msg });
    }

    /// Diffuse une trame vidéo de partage d'écran encodée au pair de l'appel
    /// actif (fragmentée sur le canal VOICE). Sans effet hors appel.
    /// Fire-and-forget (haute fréquence), comme [`Self::inject_pcm`].
    pub fn screen_send(&self, keyframe: bool, encoded: Vec<u8>) {
        let _ = self.tx.send(Cmd::VideoSend {
            stream: media::VideoStream::Screen,
            keyframe,
            encoded,
        });
    }

    /// Annonce au pair de l'appel actif le démarrage (`on == true`) ou l'arrêt
    /// d'un partage d'écran. Sans effet hors appel.
    pub fn screen_announce(&self, on: bool) {
        let _ = self.tx.send(Cmd::VideoAnnounce {
            stream: media::VideoStream::Screen,
            on,
        });
    }

    /// Diffuse une trame vidéo de CAMÉRA encodée au pair de l'appel actif
    /// (flux indépendant du partage d'écran). Sans effet hors appel.
    pub fn camera_send(&self, keyframe: bool, encoded: Vec<u8>) {
        let _ = self.tx.send(Cmd::VideoSend {
            stream: media::VideoStream::Camera,
            keyframe,
            encoded,
        });
    }

    /// Annonce au pair de l'appel actif l'allumage (`on == true`) ou
    /// l'extinction de la caméra. Sans effet hors appel.
    pub fn camera_announce(&self, on: bool) {
        let _ = self.tx.send(Cmd::VideoAnnounce {
            stream: media::VideoStream::Camera,
            on,
        });
    }

    /// Arrête le moteur voix (idempotent).
    pub fn stop(&self) {
        let _ = self.tx.send(Cmd::Stop);
    }
}

/// Lance le moteur voix en tâche tokio et rend sa poignée.
pub(crate) fn spawn(deps: VoiceDeps) -> VoiceHandle {
    let (tx, rx) = mpsc::unbounded_channel();
    let engine = engine::Engine::new(deps, rx);
    tokio::spawn(engine.run());
    VoiceHandle { tx }
}

#[cfg(test)]
mod tests {
    use super::VoiceStatus;

    fn status(group_id: [u8; 16], channel_id: [u8; 16], is_call: bool) -> VoiceStatus {
        VoiceStatus {
            group_id,
            channel_id,
            is_call,
            muted: false,
            deafened: false,
            participants: Vec::new(),
        }
    }

    #[test]
    fn is_room_matches_exact_group_and_channel_only() {
        let g = [1u8; 16];
        let c = [2u8; 16];
        // Salon vocal de groupe exact → vrai (émission et réception permises).
        assert!(status(g, c, false).is_room(&g, &c));
        // Mauvais salon ou mauvais groupe → faux.
        assert!(!status(g, c, false).is_room(&g, &[9u8; 16]));
        assert!(!status(g, c, false).is_room(&[9u8; 16], &c));
        // Un appel 1-à-1 n'est jamais un salon de groupe, même identifiants
        // égaux : le soundboard y est exclu.
        assert!(!status(g, c, true).is_room(&g, &c));
    }
}
