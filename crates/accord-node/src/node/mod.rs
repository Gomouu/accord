//! État applicatif du nœud : identité déverrouillée + base locale + émission
//! d'événements. Les méthodes de haut niveau sont appelées par le service API
//! ([`crate::service`]) et par les boucles réseau ([`crate::runtime`]).
//!
//! La base `rusqlite` n'est pas `Sync` : elle est protégée par un `Mutex`
//! tenu uniquement pendant des opérations synchrones brèves (jamais à travers
//! un `await`), conformément aux règles du projet.
//!
//! Les méthodes sont réparties par domaine dans les sous-modules ([`dm`],
//! [`groups`], [`friends`], [`voice`], [`profile`], [`files`]) via des blocs
//! `impl Node` séparés ; ce module garde l'état, les constructeurs et le
//! transversal (ingestion réseau, recherche, outbox, boîtes aux lettres).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;

use accord_api::NotificationHub;
use accord_core::db::{ContactState, Db, LocalMembership};
use accord_core::{friends, group, messaging, presence, profile};
use accord_crypto::{derive_search_key, node_id_of, Identity};
use accord_proto::core_msg::CoreMsg;
use serde_json::json;
use zeroize::Zeroizing;

use crate::error::NodeError;
use crate::hex;
use crate::outbound::{Outbound, OutboundSink};

/// Intervalle minimal entre deux indicateurs de frappe acceptés d'un même
/// pair (anti-abus) : en deçà, l'événement est silencieusement ignoré.
const TYPING_MIN_INTERVAL_MS: u64 = 2_000;

/// Fenêtre de cadence des `InviteRedeem` entrants par pair (anti-abus).
const REDEEM_WINDOW_MS: u64 = 60_000;

/// Rachats d'invitation acceptés par pair et par fenêtre : au-delà, le
/// message est silencieusement ignoré (aucun oracle vers l'attaquant).
const REDEEM_MAX_PER_WINDOW: u32 = 5;

/// Borne mémoire du suivi de cadence des rachats : au-delà, les fenêtres
/// expirées sont purgées ; si la table reste pleine, les nouveaux pairs sont
/// ignorés (dégradation sûre plutôt que croissance non bornée).
const REDEEM_SEEN_MAX_PEERS: usize = 1024;

/// Fenêtre de cadence des `SoundboardPlay` entrants par pair (anti-DoS sonore).
const SOUNDBOARD_WINDOW_MS: u64 = 10_000;

/// Lectures de soundboard acceptées par pair et par fenêtre : au-delà, le
/// message est silencieusement ignoré (aucun retour vers l'attaquant).
const SOUNDBOARD_MAX_PER_WINDOW: u32 = 10;

/// Borne mémoire du suivi de cadence des lectures de soundboard (même
/// dégradation sûre que [`REDEEM_SEEN_MAX_PEERS`]).
const SOUNDBOARD_SEEN_MAX_PEERS: usize = 1024;

/// Décode une valeur `u64` big-endian d'une métadonnée (0 si absente ou
/// malformée). Support des marques de lecture DM et de salon.
pub(super) fn read_u64(v: Option<Vec<u8>>) -> u64 {
    v.and_then(|b| b.try_into().ok().map(u64::from_be_bytes))
        .unwrap_or(0)
}

/// Clé de métadonnée de la marque de lecture locale d'une conversation directe.
pub(super) fn dm_mark_key(peer: &[u8; 32]) -> String {
    format!("dmread:{}", hex::encode(peer))
}

/// Clé de métadonnée de la marque de lecture locale d'un salon de groupe.
pub(super) fn group_mark_key(group_id: &[u8; 16], channel_id: &[u8; 16]) -> String {
    format!(
        "grread:{}:{}",
        hex::encode(group_id),
        hex::encode(channel_id)
    )
}

pub(crate) mod backup_schedule;
pub(crate) mod diagnostics;
pub(crate) mod discovery;
mod dm;
mod ephemeral;
mod files;
mod groups;
pub(crate) mod holepunch;
mod mentions;
pub(crate) mod nat;
pub(crate) mod network;
pub(crate) mod privacy;
pub(crate) mod relay;
mod reminders;
mod schedule;
mod voice;

// Les noms `friends` et `profile` sont déjà pris par les imports
// `accord_core::{friends, profile}` utilisés par l'ingestion ci-dessous ;
// `#[path]` garde les fichiers `friends.rs` / `profile.rs` sous un nom de
// module distinct.
#[path = "friends.rs"]
mod node_friends;
#[path = "profile.rs"]
mod node_profile;
// `search` est déjà pris par l'import `accord_core::search` utilisé ci-dessous.
#[path = "search.rs"]
mod node_search;

#[cfg(test)]
mod tests;

pub(crate) use node_friends::verification_state;
pub use node_profile::SelfProfile;

/// Horloge murale en millisecondes (source unique du nœud).
pub fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Rich presence announced by a friend: wire status (0-2) + custom text.
type RichPresence = (u8, Option<String>);

/// Annotations d'un lot de messages, chargées en une passe pour le rendu
/// d'historique ([`Node::annotations_of`]). Un message absent d'une carte n'a
/// simplement pas d'annotation de ce type.
#[derive(Debug, Default)]
pub struct MsgAnnotations {
    /// Réactions par message, ordre `emoji` (identique à [`Node::reactions_of`]).
    pub reactions: HashMap<[u8; 16], Vec<(String, [u8; 32])>>,
    /// Pièces jointes par message, ordre `position` (identique à
    /// [`Node::attachments_of`]).
    pub attachments: HashMap<[u8; 16], Vec<accord_proto::core_msg::FileRef>>,
    /// Messages portant une mention de l'utilisateur local.
    pub mentions: HashSet<[u8; 16]>,
}

impl MsgAnnotations {
    /// Réactions d'un message (vide si aucune).
    pub fn reactions_of(&self, msg_id: &[u8; 16]) -> &[(String, [u8; 32])] {
        self.reactions.get(msg_id).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Pièces jointes d'un message (vide si aucune).
    pub fn attachments_of(&self, msg_id: &[u8; 16]) -> &[accord_proto::core_msg::FileRef] {
        self.attachments
            .get(msg_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Vrai si le message mentionne l'utilisateur local.
    pub fn mentions_me(&self, msg_id: &[u8; 16]) -> bool {
        self.mentions.contains(msg_id)
    }
}

/// Décide si un [`CoreMsg::SoundboardPlay`] entrant est diffusable à l'UI :
/// l'émetteur est membre du groupe, `channel_id` désigne un salon **vocal**
/// existant, et `sound` correspond à la racine Merkle d'un son de serveur
/// **enregistré** (répliqué dans [`GroupState::sounds`]).
///
/// Cette dernière condition est le correctif anti-DoS d'amplification : sans
/// elle, un pair modifié forgerait un `SoundboardPlay` portant une racine
/// arbitraire (jusqu'à 2 Gio, non-audio) que tous les membres en ligne iraient
/// chercher. En n'acceptant que les racines déjà répliquées (bornées par
/// `MAX_SOUNDS`, gate `MANAGE_EMOJIS` à l'ajout), la fenêtre se réduit aux
/// clips audio légitimes du groupe.
///
/// La cadence anti-spam par pair est un effet de bord vérifié séparément par
/// l'appelant (jamais dans ce prédicat pur).
fn soundboard_play_broadcastable(
    state: &group::GroupState,
    peer: &[u8; 32],
    channel_id: &[u8; 16],
    sound: &[u8; 32],
) -> bool {
    let is_voice = matches!(
        state.channels.get(channel_id),
        Some(ch) if ch.kind == accord_proto::core_msg::ChannelKind::Voice
    );
    let is_registered_sound = state.sounds.values().any(|root| root == sound);
    state.is_member(peer) && is_voice && is_registered_sound
}

/// Une offre d'appairage fraîchement ouverte, telle que l'écran l'affiche.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingStarted {
    /// Le code, déjà découpé pour la lecture (`ABCD-EFGH`).
    pub code: String,
    /// Instant d'expiration (ms epoch) — l'écran en fait un compte à rebours.
    pub expires_ms: u64,
}

/// Un appareil du compte, tel que l'API le montre.
///
/// 🔒 Pas de graine, pas de nonce : de quoi reconnaître un appareil, jamais
/// de quoi en usurper un.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountDevice {
    /// Clé publique de l'appareil.
    pub pubkey: [u8; 32],
    /// Nom lisible choisi par l'utilisateur.
    pub name: String,
    /// Date d'ajout (ms epoch), `0` pour l'appareil issu de la migration.
    pub added_ms: u64,
    /// Vrai pour l'appareil sur lequel tourne cette application.
    pub is_current: bool,
}

/// Nœud Accord déverrouillé.
pub struct Node {
    identity: Arc<Identity>,
    search_key: Zeroizing<[u8; 32]>,
    db: Mutex<Db>,
    outbound: OutboundSink,
    hub: Option<NotificationHub>,
    /// Contrôle réseau (pilotage des méthodes `network.*`), branché après la
    /// construction du runtime ; absent dans les tests sans réseau.
    // Strong ref, but CLEARABLE: the runtime holds `Arc<Node>`, so keeping it
    // here forms a Runtime↔Node cycle that never drops by refcount — leaking the
    // endpoint (hence the bound UDP socket) across every lock/unlock (Lot G,
    // cause 3). `RunningNode::shutdown` calls `clear_network_control` to break
    // the cycle explicitly. `Option` (not `OnceLock`) so it can be cleared.
    network: Mutex<Option<Arc<dyn network::NetworkControl>>>,
    /// Amis présumés en ligne (dernier signal reçu). Best-effort, en mémoire :
    /// l'absence d'un pair ne prouve pas qu'il est hors ligne (§6, présence).
    online: Mutex<HashSet<[u8; 32]>>,
    /// Rich presence explicitly announced by friends (`PRESENCE` 0x08):
    /// wire status 0-2 plus optional custom text. Best-effort, in memory,
    /// friends only (anti-abuse); an offline announcement clears the entry.
    peer_status: Mutex<HashMap<[u8; 32], RichPresence>>,
    /// Offre d'appairage en cours sur cet appareil (lot 1.D), s'il y en a une.
    ///
    /// 🔒 Une seule à la fois, et **en mémoire seulement** : un code qui
    /// survivrait à un redémarrage serait un code dont personne ne surveille
    /// plus l'écran.
    pairing_offer: Mutex<Option<crate::pairing::PairingOffer>>,
    /// Canal candidat d'un échange abouti, en attente de confirmation.
    ///
    /// 🔒 « Candidat » est le mot juste : un échange SPAKE2 abouti ne prouve
    /// pas que l'autre connaissait le code (voir `pairing::PairingOffer`).
    /// Seule la comparaison d'empreinte par deux humains le transforme en
    /// appairage.
    pairing_channel: Mutex<Option<accord_crypto::pairing::PairedChannel>>,
    /// Appareil que le pair demande à faire inscrire, une fois le canal ouvert.
    ///
    /// Reçu scellé sous la clé du canal : le détenir prouve que le pair
    /// connaissait le code. Rien n'est inscrit tant que l'empreinte n'est pas
    /// confirmée pour autant — le chiffrement dit « il avait le code », pas
    /// « c'est bien la machine que je voulais ».
    pairing_pending: Mutex<Option<accord_proto::device::DeviceEntry>>,
    /// Dernier indicateur de frappe accepté par pair (anti-abus, ms murales).
    typing_seen: Mutex<HashMap<[u8; 32], u64>>,
    /// Cadence des `InviteRedeem` entrants par pair : `(début de fenêtre ms,
    /// compte)`. Anti-abus en mémoire, borné ([`REDEEM_SEEN_MAX_PEERS`]).
    redeem_seen: Mutex<HashMap<[u8; 32], (u64, u32)>>,
    /// Cadence des `SoundboardPlay` entrants par pair : `(début de fenêtre ms,
    /// compte)`. Anti-DoS sonore en mémoire, borné ([`SOUNDBOARD_SEEN_MAX_PEERS`]).
    soundboard_seen: Mutex<HashMap<[u8; 32], (u64, u32)>>,
    profile_frame_migrated: OnceLock<()>,
}

impl Node {
    /// Assemble un nœud à partir d'une identité et d'une base ouvertes.
    pub fn new(identity: impl Into<Arc<Identity>>, db: Db, outbound: OutboundSink) -> Self {
        Self::with_hub(identity, db, outbound, None)
    }

    /// Assemble un nœud relié à un hub d'événements API.
    pub fn with_hub(
        identity: impl Into<Arc<Identity>>,
        db: Db,
        outbound: OutboundSink,
        hub: Option<NotificationHub>,
    ) -> Self {
        let identity = identity.into();
        let search_key = Zeroizing::new(derive_search_key(identity.seed()));
        Self {
            identity,
            search_key,
            db: Mutex::new(db),
            outbound,
            hub,
            network: Mutex::new(None),
            online: Mutex::new(HashSet::new()),
            peer_status: Mutex::new(HashMap::new()),
            pairing_offer: Mutex::new(None),
            pairing_channel: Mutex::new(None),
            pairing_pending: Mutex::new(None),
            typing_seen: Mutex::new(HashMap::new()),
            redeem_seen: Mutex::new(HashMap::new()),
            soundboard_seen: Mutex::new(HashMap::new()),
            profile_frame_migrated: OnceLock::new(),
        }
    }

    /// Émet un événement temps réel vers l'UI (sans effet si aucun hub).
    fn emit(&self, event: &str, params: serde_json::Value) {
        if let Some(hub) = &self.hub {
            hub.notify(event, params);
        }
    }

    /// Clé publique locale.
    pub fn public_key(&self) -> [u8; 32] {
        self.identity.public_key()
    }

    /// Exécute une opération synchrone sous le verrou de la base.
    fn with_db<T>(&self, f: impl FnOnce(&Db) -> Result<T, NodeError>) -> Result<T, NodeError> {
        let db = self.db.lock().unwrap_or_else(|e| e.into_inner());
        f(&db)
    }

    /// Record d'identité à (re)publier dans la DHT.
    pub fn identity_record(&self) -> accord_proto::types::DhtRecord {
        friends::identity_record(&self.identity, now_ms())
    }

    /// Ingère la liste d'appareils poussée par un pair sur sa session.
    ///
    /// 🔒 La liste doit concerner **l'émetteur lui-même**. Sans ce contrôle,
    /// n'importe quel ami pourrait nous imposer la liste d'appareils d'un
    /// tiers — donc y glisser sa propre clé et se faire passer pour lui.
    /// Ignorée en silence si le pair n'est pas un ami : une liste n'est pas
    /// une raison d'entrer en relation.
    fn ingest_device_list(
        &self,
        peer_pubkey: &[u8; 32],
        list: &accord_proto::device::DeviceList,
    ) -> Result<(), NodeError> {
        if !self.is_friend(peer_pubkey) || list.account != *peer_pubkey {
            return Ok(());
        }
        let connue = self
            .with_db(|db| Ok(db.device_list(peer_pubkey)?))
            .ok()
            .flatten()
            .map(|c| c.version)
            .unwrap_or(0);
        if accord_crypto::verify_device_list(list, peer_pubkey, connue).is_err() {
            return Ok(());
        }
        let mut w = accord_proto::Writer::new();
        accord_proto::WireEncode::encode(list, &mut w);
        self.with_db(|db| {
            db.cache_device_list(&accord_core::db::CachedDeviceList {
                account: *peer_pubkey,
                version: list.version,
                encoded: w.into_bytes(),
                fetched_ms: now_ms(),
            })?;
            Ok(())
        })?;
        Ok(())
    }

    /// Traite le message PAKE d'un appareil qui tente de s'appairer.
    ///
    /// ⚠️ **Le message de réponse est celui de CET échange.** `accept` consomme
    /// l'état SPAKE2 et en repose un neuf pour l'essai suivant : lire
    /// `outgoing()` après coup enverrait au pair le message d'un échange
    /// auquel il ne participe pas, et les deux côtés dériveraient des clés
    /// différentes sans raison apparente.
    ///
    /// Silencieux quand il n'y a pas d'offre, ou quand elle est morte : un
    /// inconnu qui frappe à une porte fermée n'apprend rien de plus que le
    /// silence.
    fn ingest_pairing_hello(&self, peer_msg: &[u8]) -> Vec<CoreMsg> {
        let mut slot = self.pairing_offer.lock().unwrap_or_else(|e| e.into_inner());
        let Some(offer) = slot.as_mut() else {
            return Vec::new();
        };
        // 🔒 Une fois qu'un appareil a prouvé qu'il avait le code — en scellant
        // sa clé sous celle du canal — plus aucun HELLO ne peut remplacer le
        // canal candidat. Sans cette garde, un pair quelconque changerait
        // l'empreinte affichée à l'écran juste avant que l'utilisateur ne la
        // compare, et lui ferait confirmer un appairage qui n'est pas le sien.
        if self
            .pairing_pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some()
        {
            tracing::debug!("appairage : canal déjà établi, HELLO tardif ignoré");
            return Vec::new();
        }
        let reply = offer.outgoing().to_vec();
        match offer.accept(peer_msg, now_ms()) {
            Ok(channel) => {
                // 🔒 Le canal est un CANDIDAT, pas un appairage. Rien n'est
                // signé tant que deux humains n'ont pas comparé l'empreinte —
                // voir `PairingOffer::accept`.
                *self
                    .pairing_channel
                    .lock()
                    .unwrap_or_else(|e| e.into_inner()) = Some(channel);
                tracing::info!("appairage : échange abouti, empreinte à confirmer");
                vec![CoreMsg::PairingHello { msg: reply }]
            }
            Err(refus) => {
                tracing::debug!(?refus, "appairage : tentative refusée");
                Vec::new()
            }
        }
    }

    /// Ouvre la charge scellée d'un pair et retient l'appareil qu'il propose.
    ///
    /// 🔒 Trois contrôles, dans cet ordre : il faut un canal candidat, la
    /// charge doit s'ouvrir avec sa clé — ce qui prouve que l'émetteur avait
    /// le code — et l'appareil proposé doit porter une preuve de travail
    /// valide. Sans ce dernier point, une clé d'appareil se fabriquerait en
    /// masse pour rien.
    ///
    /// Retenir n'est pas inscrire : c'est la confirmation d'empreinte qui
    /// décide, et elle seule.
    fn ingest_pairing_sealed(&self, sealed: &[u8]) {
        let Some(clear) = self
            .pairing_channel
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .and_then(|c| c.open(sealed).ok())
        else {
            tracing::debug!("appairage : charge scellée illisible, ignorée");
            return;
        };
        let mut r = accord_proto::Reader::new(&clear);
        let Ok(entry) =
            <accord_proto::device::DeviceEntry as accord_proto::WireDecode>::decode(&mut r)
        else {
            tracing::debug!("appairage : appareil proposé illisible");
            return;
        };
        if r.finish().is_err() {
            tracing::debug!("appairage : octets excédentaires après l'appareil");
            return;
        }
        if !accord_crypto::verify_pow(
            &entry.pubkey,
            entry.pow_nonce,
            accord_proto::limits::IDENTITY_POW_BITS,
        ) {
            tracing::debug!("appairage : appareil proposé sans preuve de travail");
            return;
        }
        *self
            .pairing_pending
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(entry);
        tracing::info!("appairage : appareil proposé retenu, en attente de confirmation");
    }

    /// Ouvre une offre d'appairage et rend le code à afficher.
    ///
    /// Remplace une offre en cours : demander un nouveau code annule le
    /// précédent, ce qui est le comportement attendu — l'utilisateur qui
    /// reclique veut repartir de zéro, pas cumuler deux codes valides.
    pub fn pairing_start(&self) -> Result<PairingStarted, NodeError> {
        let offer = crate::pairing::PairingOffer::open(now_ms());
        let started = PairingStarted {
            code: offer.code().display(),
            expires_ms: offer.expires_ms(),
        };
        // Une offre neuve repart d'un état vierge : sans ça, le canal et
        // l'appareil de l'appairage précédent survivraient, et le garde-fou
        // qui fige le canal ferait ignorer le premier HELLO du suivant.
        *self
            .pairing_channel
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = None;
        *self
            .pairing_pending
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = None;
        *self.pairing_offer.lock().unwrap_or_else(|e| e.into_inner()) = Some(offer);
        Ok(started)
    }

    /// Saisit un code d'appairage sur le **nouvel** appareil.
    ///
    /// Symétrique de [`Node::pairing_start`] : là où la machine autorisée
    /// affiche un code, celle-ci le saisit. Les deux côtés se retrouvent
    /// ensuite dans le même état — une offre en cours, en attente du message
    /// PAKE d'en face — parce que SPAKE2 est symétrique et qu'aucun des deux
    /// n'est « le serveur ».
    ///
    /// Rend le message à transmettre : ce module ne connaît pas le transport.
    pub fn pairing_submit(&self, code: &str) -> Result<Vec<u8>, NodeError> {
        let parsed = accord_crypto::pairing::PairingCode::parse(code)
            .map_err(|_| NodeError::Invalid("code d'appairage invalide"))?;
        let offer = crate::pairing::PairingOffer::open_with_code(parsed, now_ms());
        let outgoing = offer.outgoing().to_vec();
        // Une saisie remplace ce qui était en cours : l'utilisateur qui
        // ressaisit veut repartir de zéro, pas cumuler deux appairages.
        *self
            .pairing_channel
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = None;
        *self
            .pairing_pending
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = None;
        *self.pairing_offer.lock().unwrap_or_else(|e| e.into_inner()) = Some(offer);
        Ok(outgoing)
    }

    /// Entrée d'appareil de cette machine, scellée sous la clé du canal.
    ///
    /// Ce que le nouvel appareil envoie une fois l'empreinte confirmée de son
    /// côté. `None` s'il n'y a pas de canal ouvert ou pas d'appareil local.
    pub fn pairing_sealed_self(&self) -> Option<Vec<u8>> {
        let stored = self.with_db(|db| Ok(db.local_device()?)).ok().flatten()?;
        let device = accord_crypto::DeviceIdentity::from_seed(stored.seed);
        let entry = accord_proto::device::DeviceEntry {
            pubkey: device.public_key(),
            pow_nonce: device.pow_nonce(),
            name: stored.name,
            added_ms: now_ms(),
            flags: 0,
        };
        let mut w = accord_proto::Writer::new();
        accord_proto::WireEncode::encode(&entry, &mut w);
        self.pairing_channel
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .and_then(|c| c.seal(&w.into_bytes()).ok())
    }

    /// Annule l'offre en cours, s'il y en a une.
    pub fn pairing_cancel(&self) {
        *self.pairing_offer.lock().unwrap_or_else(|e| e.into_inner()) = None;
        // L'empreinte part avec l'offre : sinon l'écran suivant afficherait
        // celle d'un appairage abandonné.
        *self
            .pairing_channel
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = None;
    }

    /// Empreinte du canal candidat, à afficher pour comparaison humaine.
    ///
    /// `None` tant qu'aucun échange n'a abouti — l'écran affiche alors le code
    /// et attend.
    pub fn pairing_fingerprint(&self) -> Option<String> {
        self.pairing_channel
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|c| c.fingerprint().to_string())
    }

    /// Confirme l'empreinte et scelle l'appairage.
    ///
    /// 🔒 À n'appeler qu'après confirmation **explicite** de l'utilisateur.
    /// Sans canal candidat il n'y a rien à confirmer : refuser, plutôt que
    /// consommer l'offre pour rien — ce serait exactement le trou que la
    /// confirmation existe pour boucher.
    pub fn pairing_confirm(&self) -> Result<(), NodeError> {
        if self.pairing_fingerprint().is_none() {
            return Err(NodeError::Invalid("aucune empreinte à confirmer"));
        }
        // 🔒 Confirmer sans clé d'appareil ne scellerait rien : l'offre serait
        // consommée et l'utilisateur croirait son appareil ajouté. Refuser
        // plutôt que de réussir à vide.
        let entry = self
            .pairing_pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .ok_or(NodeError::Invalid("aucun appareil proposé"))?;
        {
            let mut slot = self.pairing_offer.lock().unwrap_or_else(|e| e.into_inner());
            let offer = slot
                .as_mut()
                .ok_or(NodeError::Invalid("aucun appairage en cours"))?;
            offer
                .confirm(now_ms())
                .map_err(|_| NodeError::Invalid("appairage expiré ou déjà scellé"))?;
        }
        let issue = self.enroll_device(entry);
        // Inscrit ou non, l'appareil proposé a joué son rôle : le garder
        // ferait ignorer le premier HELLO de l'appairage suivant.
        *self
            .pairing_pending
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = None;
        issue
    }

    /// Inscrit un appareil dans la liste du compte et publie la version n+1.
    ///
    /// ⚠️ La version vient de l'horodatage, jamais d'un compteur stocké : après
    /// une restauration depuis la phrase de récupération, un compteur repartant
    /// à 1 ferait ignorer la liste par tous les pairs qui en détiennent une
    /// plus récente, et l'utilisateur resterait enfermé dehors de son compte.
    fn enroll_device(&self, entry: accord_proto::device::DeviceEntry) -> Result<(), NodeError> {
        let mut list = self.current_device_list()?;
        if list.devices.iter().any(|d| d.pubkey == entry.pubkey) {
            // Déjà inscrit : un second appairage du même appareil ne doit pas
            // le faire figurer deux fois, ni faire échouer la confirmation.
            return Ok(());
        }
        if list.devices.len() >= accord_proto::device::MAX_DEVICES {
            return Err(NodeError::Invalid("trop d'appareils sur ce compte"));
        }
        list.devices.push(entry);
        list.version = accord_crypto::version_for(now_ms());
        accord_crypto::sign_device_list_with_root(&self.identity, &mut list);
        self.store_device_list(&list)?;
        self.publish_device_list(&list);
        Ok(())
    }

    /// Compte auquel appartient une clé statique de transport.
    ///
    /// La rend telle quelle quand elle est déjà celle d'un compte ami — le cas
    /// de tout le parc aujourd'hui — et remonte au compte propriétaire quand
    /// c'est la clé d'un appareil listé.
    ///
    /// 🔒 C'est **le** point de traduction. Tout ce qui est en aval du routage
    /// travaille sur des comptes : les amitiés, les profils, les op-logs
    /// appartiennent à une personne, pas à une machine. Traduire ailleurs, ou
    /// à moitié, ferait apparaître le même ami sous deux identités selon le
    /// chemin emprunté.
    pub fn account_of_transport_key(&self, static_pub: &[u8; 32]) -> [u8; 32] {
        let friends = self.friend_pubkeys().unwrap_or_default();
        self.with_db(|db| {
            Ok(crate::device::account_for_static(
                db,
                &friends,
                static_pub,
                now_ms(),
            ))
        })
        .ok()
        .flatten()
        .unwrap_or(*static_pub)
    }

    /// Révoque un appareil du compte et publie la version n+1.
    ///
    /// 🔒 **On ne révoque pas l'appareil sur lequel on est.** Ce serait se
    /// couper soi-même, et il ne resterait aucune machine capable de signer la
    /// liste suivante — le compte deviendrait irrécupérable sans la phrase de
    /// récupération.
    ///
    /// ⚠️ La révocation est à **cohérence finale** : un pair qui ne rafraîchit
    /// pas sa liste continue de tenir l'appareil pour valide jusqu'à
    /// expiration (24 h). C'est le prix de l'absence de serveur, et l'écran
    /// doit le dire à qui révoque.
    pub fn revoke_device(&self, pubkey: &[u8; 32]) -> Result<(), NodeError> {
        let local = self
            .with_db(|db| Ok(db.local_device()?))?
            .ok_or(NodeError::Invalid("aucun appareil local"))?;
        if accord_crypto::DeviceIdentity::from_seed(local.seed).public_key() == *pubkey {
            return Err(NodeError::Invalid(
                "impossible de révoquer l'appareil courant",
            ));
        }
        let mut list = self.current_device_list()?;
        let avant = list.devices.len();
        list.devices.retain(|d| d.pubkey != *pubkey);
        if list.devices.len() == avant {
            return Err(NodeError::NotFound("appareil inconnu"));
        }
        let now = now_ms();
        // La révocation est CONSERVÉE, pas seulement l'absence : un pair qui
        // détient une liste ancienne où l'appareil figure encore doit pouvoir
        // constater qu'il a été retiré, et pas seulement ne plus le voir.
        list.revoked.push(accord_proto::device::RevokedEntry {
            pubkey: *pubkey,
            revoked_ms: now,
        });
        if list.revoked.len() > accord_proto::device::MAX_REVOKED {
            // Les plus anciennes sortent : un appareil révoqué depuis
            // longtemps est de toute façon inconnu des pairs récents, et la
            // liste ne doit pas croître sans fin.
            let trop = list.revoked.len() - accord_proto::device::MAX_REVOKED;
            list.revoked.drain(..trop);
        }
        list.version = accord_crypto::version_for(now);
        accord_crypto::sign_device_list_with_root(&self.identity, &mut list);
        self.store_device_list(&list)?;
        self.publish_device_list(&list);
        Ok(())
    }

    /// Liste d'appareils courante du compte : celle qui est persistée, ou une
    /// liste neuve à un seul appareil si aucune ne l'est encore.
    ///
    /// 🔒 Lire le stockage plutôt que reconstruire est ce qui rend
    /// l'appairage durable. Rebâtir depuis le seul appareil local à chaque
    /// appel effacerait silencieusement tous les appareils appairés — la liste
    /// publiée les perdrait, et ils cesseraient d'être joignables.
    fn current_device_list(&self) -> Result<accord_proto::device::DeviceList, NodeError> {
        let account = self.public_key();
        if let Some(cached) = self.with_db(|db| Ok(db.device_list(&account)?))? {
            let mut r = accord_proto::Reader::new(&cached.encoded);
            if let Ok(list) =
                <accord_proto::device::DeviceList as accord_proto::WireDecode>::decode(&mut r)
            {
                return Ok(list);
            }
            tracing::warn!("liste d'appareils locale illisible, reconstruite");
        }
        let stored = self
            .with_db(|db| Ok(db.local_device()?))?
            .ok_or(NodeError::Invalid("aucun appareil local"))?;
        let local = accord_crypto::DeviceIdentity::from_seed(stored.seed);
        Ok(crate::device::build_device_list_with_root(
            &self.identity,
            &local,
            &stored.name,
            now_ms(),
        ))
    }

    /// Persiste la liste d'appareils du compte.
    fn store_device_list(&self, list: &accord_proto::device::DeviceList) -> Result<(), NodeError> {
        let mut w = accord_proto::Writer::new();
        accord_proto::WireEncode::encode(list, &mut w);
        let account = self.public_key();
        self.with_db(|db| {
            db.cache_device_list(&accord_core::db::CachedDeviceList {
                account,
                version: list.version,
                encoded: w.into_bytes(),
                fetched_ms: now_ms(),
            })?;
            Ok(())
        })
    }

    /// Diffuse une liste d'appareils fraîchement signée aux amis connectés.
    ///
    /// Best-effort et volontairement silencieux : la republication DHT
    /// périodique rattrape ce qui n'est pas passé, et faire échouer un
    /// appairage réussi parce qu'un ami est hors ligne n'aurait aucun sens.
    fn publish_device_list(&self, list: &accord_proto::device::DeviceList) {
        let msg = CoreMsg::DeviceListAnnounce { list: list.clone() };
        let friends = self.friend_pubkeys().unwrap_or_default();
        for friend in friends {
            self.outbound.send(Outbound::Core {
                to: friend,
                msg: Box::new(msg.clone()),
            });
        }
    }

    /// Appareils du compte, tels que l'écran « Mes appareils » les montre.
    ///
    /// 🔒 Ne rend jamais la graine — seulement de quoi reconnaître un
    /// appareil dans une liste.
    pub fn account_devices(&self) -> Result<Vec<AccountDevice>, NodeError> {
        let Some(stored) = self.with_db(|db| Ok(db.local_device()?))? else {
            return Ok(Vec::new());
        };
        let local = accord_crypto::DeviceIdentity::from_seed(stored.seed).public_key();
        // 🔒 Lire la liste plutôt que rendre le seul appareil local : sans
        // ça, un appareil appairé n'apparaîtrait jamais à l'écran, et
        // l'utilisateur n'aurait aucun moyen de constater — ni de révoquer —
        // une machine ajoutée à son compte.
        Ok(self
            .current_device_list()?
            .devices
            .into_iter()
            .map(|d| AccountDevice {
                pubkey: d.pubkey,
                name: d.name,
                // Zéro pour l'appareil issu de la migration, qui n'a pas de
                // date d'ajout : l'écran sait l'interpréter, une date inventée
                // induirait en erreur.
                added_ms: d.added_ms,
                is_current: d.pubkey == local,
            })
            .collect())
    }

    /// Renomme l'appareil de cette machine.
    pub fn rename_local_device(&self, name: &str) -> Result<(), NodeError> {
        self.with_db(|db| {
            db.rename_local_device(name)?;
            Ok(())
        })
    }

    /// Liste d'appareils à annoncer à un pair sur sa session (lot 1.C).
    ///
    /// Double emploi assumé avec la DHT : un ami déjà connecté n'a alors aucun
    /// lookup à attendre. Rend `None` tant qu'aucun appareil local n'existe.
    pub fn own_device_list_msg(&self) -> Option<CoreMsg> {
        Some(CoreMsg::DeviceListAnnounce {
            list: self.current_device_list().ok()?,
        })
    }

    /// Record de liste d'appareils à (re)publier dans la DHT (lot 1.C).
    ///
    /// `self.identity` **est** la racine du compte : c'est ce qu'établit la
    /// migration au démarrage (`device::ensure_local_device`), qui conserve la
    /// graine existante comme racine et génère à côté une clé d'appareil
    /// distincte. Rend `None` tant qu'aucun appareil local n'est persisté —
    /// une base ouverte hors du chemin de démarrage normal, dans les tests.
    pub fn device_list_record(&self) -> Option<accord_proto::types::DhtRecord> {
        let list = self.current_device_list().ok()?;
        Some(crate::device::device_list_record_with_root(
            &self.identity,
            &list,
            now_ms(),
        ))
    }

    // ---- Présence des amis (D-034, best-effort) ----

    /// Vrai si `peer` est un ami confirmé.
    fn is_friend(&self, peer: &[u8; 32]) -> bool {
        self.with_db(|db| {
            Ok(db
                .contact(&node_id_of(peer).0)?
                .map(|c| c.state == ContactState::Friend)
                .unwrap_or(false))
        })
        .unwrap_or(false)
    }

    /// Effective presence of a peer: the explicit status announced by the
    /// peer (`PRESENCE` 0x08) when known, else plain reachability mapped to
    /// online (0) / offline (3). Wire status byte + optional custom text.
    fn effective_presence(&self, peer: &[u8; 32]) -> (u8, Option<String>) {
        if let Some(explicit) = self
            .peer_status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(peer)
            .cloned()
        {
            return explicit;
        }
        let reachable = self
            .online
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(peer);
        (if reachable { 0 } else { 3 }, None)
    }

    /// Emits `event.presence` for a friend (rich shape: `online` kept for
    /// backward compatibility, plus `status` and `status_text`).
    fn emit_presence(&self, peer: &[u8; 32], status: u8, custom: &Option<String>) {
        self.emit(
            "event.presence",
            json!({
                "pubkey": hex::encode(peer),
                "online": status != 3,
                "status": presence::status_str(status),
                "status_text": custom,
            }),
        );
    }

    /// Met à jour l'accessibilité présumée d'un pair (tout pair joignable, y
    /// compris un membre de groupe non ami — la frappe s'appuie dessus) et émet
    /// `event.presence` au seul changement d'état effectif, réservé aux amis
    /// (la présence n'est exposée que pour eux). Best-effort, jamais persisté.
    /// A peer going offline also loses its explicit rich status.
    fn set_presence(&self, peer: &[u8; 32], online: bool) {
        let before = self.effective_presence(peer);
        {
            let mut set = self.online.lock().unwrap_or_else(|e| e.into_inner());
            if online {
                set.insert(*peer);
            } else {
                set.remove(peer);
            }
        }
        if !online {
            self.peer_status
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(peer);
        }
        let after = self.effective_presence(peer);
        if before != after && self.is_friend(peer) {
            self.emit_presence(peer, after.0, &after.1);
        }
    }

    /// Applies an explicit presence announcement from a friend: reachability,
    /// rich status (0-2) and custom text; an offline announcement (3) clears
    /// everything. Emits `event.presence` only on effective change.
    fn apply_peer_presence(&self, peer: &[u8; 32], status: u8, custom: Option<String>) {
        if status == 3 {
            self.set_presence(peer, false);
            return;
        }
        // Untrusted peer text: strip control characters (defense in depth —
        // the local path sanitizes, this mirrors it for incoming presence).
        let custom = custom
            .as_deref()
            .and_then(accord_core::presence::sanitize_peer_custom);
        let before = self.effective_presence(peer);
        self.online
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(*peer);
        self.peer_status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(*peer, (status, custom));
        let after = self.effective_presence(peer);
        if before != after && self.is_friend(peer) {
            self.emit_presence(peer, after.0, &after.1);
        }
    }

    /// Rich presence of a peer for the API (`friends.list`): wire status byte
    /// (0-3) plus optional custom text. Best-effort, in memory.
    pub fn peer_presence(&self, peer: &[u8; 32]) -> (u8, Option<String>) {
        self.effective_presence(peer)
    }

    /// Vrai si un pair est présumé joignable. Best-effort : un pair sans
    /// nouvelles récentes n'est pas nécessairement hors ligne (aucune
    /// expiration ici).
    pub fn is_online(&self, peer: &[u8; 32]) -> bool {
        self.online
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(peer)
    }

    /// Presence announcement carrying the local rich status: invisible is
    /// broadcast as plain offline (no custom text leaks), any other status
    /// travels with the persisted custom text.
    pub(crate) fn own_presence_msg(&self) -> Result<CoreMsg, NodeError> {
        let (status, custom) = self.own_presence()?;
        Ok(match status {
            presence::OwnStatus::Invisible => CoreMsg::Presence {
                status: 3,
                custom: None,
            },
            other => CoreMsg::Presence {
                status: other.wire_status(),
                custom,
            },
        })
    }

    /// Diffuse une annonce de présence à tous les amis (au démarrage et
    /// périodiquement : le statut riche persisté ; à l'arrêt propre : hors
    /// ligne). `CoreMsg::Presence` n'est jamais mise en file hors-ligne : les
    /// amis injoignables la perdent sans effet. L'aiguillage effectif
    /// (démarrage/arrêt) relève du runtime.
    pub fn broadcast_presence(&self, online: bool) -> Result<(), NodeError> {
        let msg = if online {
            self.own_presence_msg()?
        } else {
            CoreMsg::Presence {
                status: 3,
                custom: None,
            }
        };
        for friend in self.friend_pubkeys()? {
            self.outbound.send(Outbound::Core {
                to: friend,
                msg: Box::new(msg.clone()),
            });
        }
        Ok(())
    }

    /// Anti-abus des indicateurs de frappe : au plus un événement toutes les
    /// [`TYPING_MIN_INTERVAL_MS`] ms par pair. Rend vrai si l'événement est
    /// accepté (et enregistre l'instant).
    fn typing_allowed(&self, peer: &[u8; 32], now: u64) -> bool {
        let mut seen = self.typing_seen.lock().unwrap_or_else(|e| e.into_inner());
        match seen.get(peer) {
            Some(&last) if now.saturating_sub(last) < TYPING_MIN_INTERVAL_MS => false,
            _ => {
                seen.insert(*peer, now);
                true
            }
        }
    }

    /// Anti-abus des rachats de lien d'invitation : au plus
    /// [`REDEEM_MAX_PER_WINDOW`] `InviteRedeem` acceptés par pair et par
    /// fenêtre de [`REDEEM_WINDOW_MS`] ms. Rend vrai si le message doit être
    /// traité (et crédite la fenêtre). Table bornée : pleine, elle purge les
    /// fenêtres expirées puis, à défaut, ignore les pairs inconnus.
    fn redeem_allowed(&self, peer: &[u8; 32], now: u64) -> bool {
        let mut seen = self.redeem_seen.lock().unwrap_or_else(|e| e.into_inner());
        if seen.len() >= REDEEM_SEEN_MAX_PEERS && !seen.contains_key(peer) {
            seen.retain(|_, (start, _)| now.saturating_sub(*start) < REDEEM_WINDOW_MS);
            if seen.len() >= REDEEM_SEEN_MAX_PEERS {
                return false;
            }
        }
        let entry = seen.entry(*peer).or_insert((now, 0));
        if now.saturating_sub(entry.0) >= REDEEM_WINDOW_MS {
            *entry = (now, 0);
        }
        if entry.1 >= REDEEM_MAX_PER_WINDOW {
            return false;
        }
        entry.1 += 1;
        true
    }

    /// Anti-DoS sonore : au plus [`SOUNDBOARD_MAX_PER_WINDOW`] `SoundboardPlay`
    /// traités par pair et par fenêtre de [`SOUNDBOARD_WINDOW_MS`] ms. Rend
    /// vrai si le message doit être traité (et crédite la fenêtre). Même
    /// dégradation sûre bornée que [`Self::redeem_allowed`].
    fn soundboard_play_allowed(&self, peer: &[u8; 32], now: u64) -> bool {
        let mut seen = self
            .soundboard_seen
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if seen.len() >= SOUNDBOARD_SEEN_MAX_PEERS && !seen.contains_key(peer) {
            seen.retain(|_, (start, _)| now.saturating_sub(*start) < SOUNDBOARD_WINDOW_MS);
            if seen.len() >= SOUNDBOARD_SEEN_MAX_PEERS {
                return false;
            }
        }
        let entry = seen.entry(*peer).or_insert((now, 0));
        if now.saturating_sub(entry.0) >= SOUNDBOARD_WINDOW_MS {
            *entry = (now, 0);
        }
        if entry.1 >= SOUNDBOARD_MAX_PER_WINDOW {
            return false;
        }
        entry.1 += 1;
        true
    }

    // ---- Ingestion des messages réseau ----

    /// Traite un `CoreMsg` reçu d'un pair authentifié (clé de session).
    /// Rend les `CoreMsg` de réponse à renvoyer au pair (zéro, un ou
    /// plusieurs — la synchronisation d'op-log peut en produire un lot), et
    /// émet les événements API correspondants.
    pub fn ingest_core(
        &self,
        peer_pubkey: &[u8; 32],
        msg: CoreMsg,
    ) -> Result<Vec<CoreMsg>, NodeError> {
        // Tout message reçu d'un ami atteste qu'il est joignable : on le note
        // en ligne (les annonces de présence, traitées explicitement, gèrent
        // aussi le passage hors ligne).
        if !matches!(msg, CoreMsg::Presence { .. }) {
            self.set_presence(peer_pubkey, true);
        }
        match msg {
            CoreMsg::DirectMsg {
                msg_id,
                lamport,
                sent_ms,
                kind,
                body,
            } => {
                let event = self.with_db(|db| {
                    Ok(messaging::ingest_dm(
                        db,
                        &self.search_key,
                        peer_pubkey,
                        &msg_id,
                        lamport,
                        sent_ms,
                        kind,
                        &body,
                    )?)
                })?;
                if event == messaging::DmEvent::Typing {
                    // Frappe éphémère : événement dédié, borné par l'anti-abus.
                    if self.typing_allowed(peer_pubkey, now_ms()) {
                        self.emit(
                            "event.dm_typing",
                            json!({ "peer": hex::encode(peer_pubkey) }),
                        );
                    }
                } else if event == messaging::DmEvent::Read {
                    // Read receipt: the peer's read position was persisted by
                    // the ingestion; expose it as a lamport for the UI.
                    if let Some(read_lamport) = self.dm_peer_read_lamport(peer_pubkey)? {
                        self.emit(
                            "event.dm_read",
                            json!({
                                "peer": hex::encode(peer_pubkey),
                                "lamport": read_lamport,
                            }),
                        );
                    }
                } else if matches!(event, messaging::DmEvent::Pin { .. }) {
                    // Replicated (un)pin: never rendered as a chat message; the
                    // UI reloads this peer's pin set.
                    self.emit("event.dm_pins", json!({ "peer": hex::encode(peer_pubkey) }));
                } else if !matches!(
                    event,
                    messaging::DmEvent::Ignored | messaging::DmEvent::Noop
                ) {
                    // Pièces jointes du message stocké (vide hors kind Text).
                    let attachments = self.with_db(|db| Ok(db.msg_attachments(&msg_id)?))?;
                    self.emit(
                        "event.dm",
                        json!({
                            "peer": hex::encode(peer_pubkey),
                            "msg_id": hex::encode(&msg_id),
                            "attachments": dm::attachments_json(&attachments),
                        }),
                    );
                    // Détection de mention (meilleur effort) : une nouvelle
                    // entrée de boîte donne lieu à `event.mention`.
                    if event == messaging::DmEvent::Stored
                        && self
                            .record_dm_mention(peer_pubkey, &msg_id, sent_ms, lamport, kind, &body)
                            .unwrap_or(false)
                    {
                        self.emit(
                            "event.mention",
                            json!({
                                "peer": hex::encode(peer_pubkey),
                                "msg_id": hex::encode(&msg_id),
                            }),
                        );
                    }
                }
                Ok(event
                    .should_ack()
                    .then_some(CoreMsg::MsgAck { msg_id })
                    .into_iter()
                    .collect())
            }
            CoreMsg::MsgAck { msg_id } => {
                self.with_db(|db| Ok(messaging::ingest_ack(db, &msg_id)?))?;
                self.emit(
                    "event.dm_ack",
                    json!({
                        "peer": hex::encode(peer_pubkey),
                        "msg_id": hex::encode(&msg_id),
                    }),
                );
                Ok(vec![])
            }
            CoreMsg::FriendRequest { display_name, .. } => {
                let outcome = self.with_db(|db| {
                    Ok(friends::ingest_friend_request(
                        db,
                        peer_pubkey,
                        &display_name,
                        now_ms(),
                    )?)
                })?;
                // Rien à signaler pour une demande silencieusement écartée
                // (pair bloqué ou débit d'ingestion saturé) : ne pas laisser un
                // flot d'inconnus inonder l'UI d'événements.
                if outcome != friends::IncomingOutcome::Ignored {
                    self.emit(
                        "event.friend_request",
                        json!({ "peer": hex::encode(peer_pubkey) }),
                    );
                }
                Ok(match outcome {
                    friends::IncomingOutcome::AutoAccepted
                    | friends::IncomingOutcome::AlreadyFriend => {
                        // Amitié (re)confirmée : accepter et annoncer notre
                        // pseudo au pair (D-027).
                        let mut replies = vec![CoreMsg::FriendResponse { accepted: true }];
                        replies.extend(self.own_profile_msg()?);
                        replies
                    }
                    _ => vec![],
                })
            }
            CoreMsg::FriendResponse { accepted } => {
                let established = self.with_db(|db| {
                    Ok(friends::ingest_friend_response(
                        db,
                        peer_pubkey,
                        accepted,
                        now_ms(),
                    )?)
                })?;
                self.emit(
                    "event.friend_response",
                    json!({ "peer": hex::encode(peer_pubkey), "accepted": accepted }),
                );
                // Nouvel ami : annoncer notre pseudo en retour (l'accepteur
                // fait de même de son côté, D-027).
                if established {
                    return Ok(self.own_profile_msg()?.into_iter().collect());
                }
                Ok(vec![])
            }
            CoreMsg::DeviceListAnnounce { list } => {
                self.ingest_device_list(peer_pubkey, &list)?;
                Ok(vec![])
            }
            CoreMsg::PairingHello { msg } => Ok(self.ingest_pairing_hello(&msg)),
            CoreMsg::PairingSealed { sealed } => {
                self.ingest_pairing_sealed(&sealed);
                Ok(vec![])
            }
            CoreMsg::Profile {
                display_name,
                bio,
                avatar,
                banner,
                pronouns,
                accent_color,
                banner_color,
                avatar_decoration,
                profile_effect,
                profile_frame,
            } => {
                // Anti-abus : seuls les amis sont pris en compte (ignoré
                // silencieusement sinon) ; pseudo validé (2-32 caractères,
                // meilleur effort sur les caractères de format trompeur), bio
                // bornée (2048 caractères), hashes d'avatar et de bannière
                // persistés. Pronoms et couleurs : champs annexes, toujours
                // en meilleur effort (jamais de rejet du profil entier).
                let updated = self.with_db(|db| {
                    Ok(profile::ingest_peer_profile(
                        db,
                        peer_pubkey,
                        &display_name,
                        &bio,
                        avatar,
                        banner,
                        pronouns.as_deref(),
                        accent_color,
                        banner_color,
                        avatar_decoration.as_deref(),
                        profile_effect.as_deref(),
                        profile_frame.as_deref(),
                        now_ms(),
                    )?)
                })?;
                tracing::debug!(
                    moi = %hex::encode(&self.public_key()[..4]),
                    pair = %hex::encode(&peer_pubkey[..4]),
                    appliquee = updated.is_some(),
                    "profil : annonce de pair reçue"
                );
                if let Some(applied) = updated {
                    tracing::debug!(
                        pair = %hex::encode(&peer_pubkey[..4]),
                        banniere = applied.banner.is_some(),
                        banniere_changee = applied.banner_changed,
                        "profil : annonce de pair appliquée"
                    );
                    // Octets d'avatar et de bannière absents en local :
                    // récupération en arrière-plan auprès de l'émetteur
                    // (meilleur effort — le sous-système fichiers peut être
                    // indisponible, l'annonce reste appliquée). On ne récupère
                    // QUE les hashes qui ont changé (anti-DoS) : une ré-annonce
                    // du même profil ou un spam de hashes ne crée aucune
                    // nouvelle intention de téléchargement.
                    for hash in applied
                        .avatar
                        .iter()
                        .filter(|_| applied.avatar_changed)
                        .chain(applied.banner.iter().filter(|_| applied.banner_changed))
                    {
                        if let Ok(None) = self.files_local_path(hash) {
                            // Média auto-récupéré : plafonné (anti-DoS taille).
                            let _ = self.files_fetch_media(hash, Some(*peer_pubkey));
                        }
                    }
                    self.emit(
                        "event.profile",
                        json!({
                            "pubkey": hex::encode(peer_pubkey),
                            "name": applied.name,
                            "bio": applied.bio,
                            "avatar": applied.avatar.map(|h| hex::encode(&h)),
                            "banner": applied.banner.map(|h| hex::encode(&h)),
                            "pronouns": applied.pronouns,
                            "accent_color": applied.accent_color,
                            "banner_color": applied.banner_color,
                            "avatar_decoration": applied.avatar_decoration,
                            "profile_effect": applied.profile_effect,
                            "profile_frame": applied.profile_frame,
                        }),
                    );
                }
                Ok(vec![])
            }
            CoreMsg::GroupOpMsg { op } => {
                let group_id = op.group_id;
                // Porte de consentement (D-045) : un op-log poussé sans
                // intention locale de rejoindre (ni fondateur, ni invitation
                // acceptée) est ignoré en silence — un pair malveillant ne
                // peut plus forcer l'affichage d'un groupe (ex force-join).
                let membership = self.with_db(|db| Ok(db.group_membership(&group_id)?))?;
                if membership == LocalMembership::None {
                    return Ok(vec![]);
                }
                let outcome = self.with_db(|db| Ok(group::ingest_op(db, &op)?))?;
                self.emit(
                    "event.group_op",
                    json!({ "group_id": hex::encode(&group_id) }),
                );
                // Op nouvelle appliquée : l'UI recharge `groups.state`
                // (rejouer un doublon ne change pas l'état).
                if outcome == group::IngestOutcome::Inserted {
                    // Première op reçue après acceptation : le groupe est
                    // désormais matérialisé et visible (`groups.list`).
                    if membership == LocalMembership::Accepted {
                        self.with_db(|db| {
                            Ok(db.set_group_membership(&group_id, LocalMembership::Joined)?)
                        })?;
                    }
                    self.emit_group_state(&group_id);
                }
                Ok(vec![])
            }
            CoreMsg::GroupMsg {
                group_id,
                channel_id,
                msg_id,
                lamport,
                sent_ms,
                key_epoch,
                body_enc,
            } => {
                let event = self.with_db(|db| {
                    Ok(group::ingest_group_message(
                        db,
                        &self.search_key,
                        peer_pubkey,
                        &group_id,
                        &channel_id,
                        &msg_id,
                        lamport,
                        sent_ms,
                        now_ms(),
                        key_epoch,
                        &body_enc,
                    )?)
                })?;
                if event == group::GroupMsgEvent::Typing {
                    // Frappe éphémère dans un salon : événement dédié, borné
                    // par l'anti-abus (émetteur crédité comme auteur).
                    if self.typing_allowed(peer_pubkey, now_ms()) {
                        self.emit(
                            "event.group_typing",
                            json!({
                                "group_id": hex::encode(&group_id),
                                "channel_id": hex::encode(&channel_id),
                                "pubkey": hex::encode(peer_pubkey),
                            }),
                        );
                    }
                } else if matches!(
                    event,
                    group::GroupMsgEvent::Stored
                        | group::GroupMsgEvent::Edited
                        | group::GroupMsgEvent::Deleted
                        | group::GroupMsgEvent::Reacted
                ) {
                    // Pièces jointes du message stocké (vide hors kind Text).
                    let attachments = self.with_db(|db| Ok(db.msg_attachments(&msg_id)?))?;
                    self.emit(
                        "event.group_msg",
                        json!({
                            "group_id": hex::encode(&group_id),
                            "channel_id": hex::encode(&channel_id),
                            "msg_id": hex::encode(&msg_id),
                            "attachments": dm::attachments_json(&attachments),
                        }),
                    );
                    // Détection de mention (meilleur effort) : une nouvelle
                    // entrée de boîte donne lieu à `event.mention`.
                    if event == group::GroupMsgEvent::Stored
                        && self
                            .record_group_mention(
                                &group_id,
                                &channel_id,
                                &msg_id,
                                peer_pubkey,
                                sent_ms,
                                lamport,
                            )
                            .unwrap_or(false)
                    {
                        self.emit(
                            "event.mention",
                            json!({
                                "group_id": hex::encode(&group_id),
                                "channel_id": hex::encode(&channel_id),
                                "msg_id": hex::encode(&msg_id),
                            }),
                        );
                    }
                }
                // Accusé applicatif (même mécanique que les MP) : le transport
                // ne retransmet pas les trames DATA, donc sans accusé une
                // seule perte UDP creusait un trou PERMANENT d'historique chez
                // ce membre (l'anti-entropie GroupSync ne couvre que l'op-log
                // administratif, pas le contenu). Les doublons sont acquittés
                // aussi (l'émetteur doit cesser de réémettre) ; `Typing` est
                // éphémère et `Ignored` (clé d'epoch absente, droit manquant)
                // ne l'est PAS : la réémission avec backoff laisse à la
                // `GroupKey` en route le temps d'arriver.
                let ack = matches!(
                    event,
                    group::GroupMsgEvent::Stored
                        | group::GroupMsgEvent::Edited
                        | group::GroupMsgEvent::Deleted
                        | group::GroupMsgEvent::Reacted
                        | group::GroupMsgEvent::Duplicate
                );
                Ok(ack
                    .then_some(CoreMsg::MsgAck { msg_id })
                    .into_iter()
                    .collect())
            }
            CoreMsg::GroupKey {
                group_id,
                key_epoch,
                sealed_key,
            } => {
                // Même porte de consentement que `GroupOpMsg` : une clé
                // poussée pour un groupe sans intention locale de rejoindre
                // est ignorée (ni stockage inutile, ni signal exploitable).
                let membership = self.with_db(|db| Ok(db.group_membership(&group_id)?))?;
                if membership == LocalMembership::None {
                    return Ok(vec![]);
                }
                // La clé n'est acceptée que si elle s'ouvre avec notre clé
                // privée ; un tiers ne peut pas nous en imposer une fausse.
                self.with_db(|db| {
                    Ok(group::accept_sealed_key(
                        db,
                        &self.identity,
                        &group_id,
                        key_epoch,
                        &sealed_key,
                    )?)
                })?;
                self.emit(
                    "event.group_key",
                    json!({ "group_id": hex::encode(&group_id) }),
                );
                Ok(vec![])
            }
            CoreMsg::GroupSync {
                group_id,
                max_lamport,
                op_count,
                digest,
            } => {
                let offer = group::SyncOffer {
                    group_id,
                    max_lamport,
                    op_count: op_count as u64,
                    digest,
                };
                let pull = self.with_db(|db| Ok(group::should_pull(db, &offer)?))?;
                Ok(pull
                    .map(|since_lamport| CoreMsg::GroupSyncPull {
                        group_id,
                        since_lamport,
                    })
                    .into_iter()
                    .collect())
            }
            CoreMsg::GroupSyncPull {
                group_id,
                since_lamport,
            } => {
                // Seuls les membres du groupe peuvent tirer l'op-log.
                let ops = self.with_db(|db| {
                    let state = group::group_state(db, &group_id)?;
                    if !state.is_member(peer_pubkey) {
                        return Ok(vec![]);
                    }
                    Ok(group::ops_for_pull(db, &group_id, since_lamport)?)
                })?;
                Ok(ops
                    .into_iter()
                    .map(|op| CoreMsg::GroupOpMsg { op })
                    .collect())
            }
            CoreMsg::Presence { status, custom } => {
                // Presence announcement: rich status (0-2) and custom text
                // are tracked for friends only (anti-abuse); a non-friend
                // only updates plain reachability. Never persisted
                // (best-effort, in memory). Older nodes sending bare
                // online/offline keep working (custom stays `None`).
                if self.is_friend(peer_pubkey) {
                    self.apply_peer_presence(peer_pubkey, status, custom);
                } else {
                    self.set_presence(peer_pubkey, status != 3);
                }
                Ok(vec![])
            }
            CoreMsg::FriendRemove => {
                // The peer removed the friendship on their side: mirror it
                // locally (DM history kept) and refresh both UIs. A stranger
                // or a blocked peer cannot mutate our contact list.
                let removed =
                    self.with_db(|db| Ok(friends::ingest_friend_remove(db, peer_pubkey)?))?;
                if removed {
                    self.peer_status
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .remove(peer_pubkey);
                    self.emit(
                        "event.friend_removed",
                        json!({ "peer": hex::encode(peer_pubkey) }),
                    );
                }
                Ok(vec![])
            }
            CoreMsg::InviteTicket {
                group_id,
                invite_id,
                group_name,
                inviter,
                secret,
                expires_ms,
                sig,
            } => {
                self.ingest_invite_ticket(
                    group_id, invite_id, group_name, inviter, secret, expires_ms, sig,
                );
                Ok(vec![])
            }
            CoreMsg::InviteAccept {
                group_id,
                invite_id,
                secret,
            } => {
                self.ingest_invite_accept(*peer_pubkey, group_id, invite_id, secret);
                Ok(vec![])
            }
            CoreMsg::InviteDecline { .. } => {
                // Best-effort : aucun suivi local des invitations sortantes
                // aujourd'hui, rien à effacer côté inviteur.
                Ok(vec![])
            }
            CoreMsg::InviteRedeem {
                group_id,
                invite_id,
                secret,
            } => {
                // Anti-abus : cadence par pair, silencieusement ignoré
                // au-delà (entrée attaquant-contrôlée, aucun oracle).
                if self.redeem_allowed(peer_pubkey, now_ms()) {
                    self.ingest_invite_redeem(*peer_pubkey, group_id, invite_id, secret);
                }
                Ok(vec![])
            }
            CoreMsg::SoundboardPlay {
                group_id,
                channel_id,
                sound,
            } => {
                // Purement éphémère : jamais rejoué comme une op, jamais mis en
                // file. Entrée attaquant-contrôlée ⇒ toute validation qui
                // échoue est ignorée en silence (aucun oracle).
                //
                // Gate à la réception : l'émetteur doit être membre du groupe,
                // `channel_id` doit être un salon vocal existant, et `sound`
                // doit correspondre à un son de serveur ENREGISTRÉE (racine
                // répliquée dans `state.sounds`) — voir
                // `soundboard_play_broadcastable` : sans ce dernier point, un
                // pair modifié forgerait une racine arbitraire (jusqu'à 2 Gio,
                // non-audio) que tous les membres iraient chercher
                // (amplification DoS). Cadence par pair en dernier pour
                // empêcher le spam sonore.
                //
                // La présence vocale du RÉCEPTEUR n'est délibérément PAS
                // vérifiée ici : le statut du salon actif vit dans l'acteur
                // voix (tâche séparée), injoignable de façon synchrone depuis
                // `Node`. Ce contrôle est appliqué en amont par le routeur
                // (`Runtime::route_core`), seul détenteur de la poignée voix.
                if let Ok(state) = self.group_state(&group_id) {
                    if soundboard_play_broadcastable(&state, peer_pubkey, &channel_id, &sound)
                        && self.soundboard_play_allowed(peer_pubkey, now_ms())
                    {
                        self.emit(
                            "event.soundboard_play",
                            json!({
                                "group_id": hex::encode(&group_id),
                                "channel_id": hex::encode(&channel_id),
                                "sound": hex::encode(&sound),
                                "from": hex::encode(peer_pubkey),
                            }),
                        );
                    }
                }
                Ok(vec![])
            }
            // Signalisation vocale et autres éphémères : non persistées ici.
            _ => Ok(vec![]),
        }
    }

    // ---- Recherche ----

    /// Recherche locale par intersection de mots, les plus récents d'abord.
    /// Bornée comme [`Node::search_filtered`], dont elle est le cas sans filtre.
    pub fn search(&self, query: &str) -> Result<Vec<String>, NodeError> {
        Ok(self
            .search_filtered(query)?
            .iter()
            .filter_map(|hit| hit.get("msg_id").and_then(serde_json::Value::as_str))
            .map(str::to_string)
            .collect())
    }

    /// Réactions stockées pour un message (DM ou groupe) : `(emoji, auteur)`.
    pub fn reactions_of(&self, msg_id: &[u8; 16]) -> Result<Vec<(String, [u8; 32])>, NodeError> {
        self.with_db(|db| Ok(db.reactions(msg_id)?))
    }

    /// Annotations d'un LOT de messages (réactions, pièces jointes, mentions)
    /// en trois requêtes et une seule prise du verrou base, au lieu de trois
    /// requêtes PAR message : le rendu d'une page d'historique ne dépend plus
    /// de `limit`. Contenu identique aux accès unitaires ([`Self::reactions_of`],
    /// [`Self::attachments_of`], [`Self::msg_mentions_me`]).
    pub fn annotations_of(&self, msg_ids: &[[u8; 16]]) -> Result<MsgAnnotations, NodeError> {
        self.with_db(|db| {
            Ok(MsgAnnotations {
                reactions: db.reactions_for(msg_ids)?,
                attachments: db.msg_attachments_for(msg_ids)?,
                mentions: db.mentions_recorded_for(msg_ids)?,
            })
        })
    }

    // ---- Points d'accès des boucles de maintenance (D-024) ----

    /// Record DHT de présence auto-signé portant les adresses du nœud.
    pub fn presence_record(
        &self,
        addrs: &[std::net::SocketAddr],
    ) -> accord_proto::types::DhtRecord {
        let mut record = accord_proto::types::DhtRecord {
            key: crate::maintenance::presence_key(&self.identity.public_key()),
            kind: accord_proto::types::RecordKind::Presence,
            value: crate::maintenance::encode_presence_value(addrs),
            publisher: self.identity.public_key(),
            timestamp_ms: now_ms(),
            expiry_s: crate::maintenance::PRESENCE_EXPIRY_S,
            sig: [0u8; 64],
        };
        record.sig = self.identity.sign(&record.signable_bytes());
        record
    }

    /// Met un `CoreMsg` en file hors-ligne pour un destinataire (clé publique).
    pub fn outbox_enqueue(&self, dest: &[u8; 32], msg: &CoreMsg) -> Result<(), NodeError> {
        let payload = crate::maintenance::encode_core(msg);
        self.with_db(|db| Ok(db.enqueue(dest, &payload, now_ms()).map(|_| ())?))
    }

    /// Éléments d'outbox dus (prochaine tentative atteinte), bornés.
    pub fn outbox_due(
        &self,
        now_ms: u64,
        limit: usize,
    ) -> Result<Vec<accord_core::db::OutboxItem>, NodeError> {
        self.with_db(|db| Ok(db.outbox_due(now_ms, limit)?))
    }

    /// Toute la file d'attente d'un destinataire (reconnexion, dépôt complet).
    pub fn outbox_for(
        &self,
        dest: &[u8; 32],
    ) -> Result<Vec<accord_core::db::OutboxItem>, NodeError> {
        self.with_db(|db| Ok(db.outbox_for(dest)?))
    }

    /// Destinataires distincts ayant des messages en file, bornés (cibles
    /// supplémentaires de la résolution de présence).
    pub fn outbox_dests(&self, limit: usize) -> Result<Vec<[u8; 32]>, NodeError> {
        self.with_db(|db| Ok(db.outbox_dests(limit)?))
    }

    /// Replanifie un élément d'outbox après échec (backoff exponentiel).
    pub fn outbox_reschedule(&self, id: i64, now_ms: u64) -> Result<(), NodeError> {
        self.with_db(|db| Ok(db.outbox_reschedule(id, now_ms)?))
    }

    /// Marque un élément d'outbox comme déposé en boîte aux lettres DHT.
    pub fn outbox_mark_mailboxed(&self, id: i64, day: u64) -> Result<(), NodeError> {
        self.with_db(|db| Ok(db.outbox_mark_mailboxed(id, day)?))
    }

    /// Retire un élément d'outbox livré.
    pub fn outbox_remove(&self, id: i64) -> Result<(), NodeError> {
        self.with_db(|db| Ok(db.outbox_remove(id)?))
    }

    /// Purge les éléments d'outbox expirés ; rend le nombre supprimé.
    pub fn outbox_purge_expired(&self, now_ms: u64) -> Result<usize, NodeError> {
        self.with_db(|db| Ok(db.outbox_purge_expired(now_ms)?))
    }

    /// Solde les messages en file pour `dest` (MP comme salons) acquittés
    /// par `msg_id` ; rend le nombre d'éléments retirés.
    pub fn outbox_ack(&self, dest: &[u8; 32], msg_id: &[u8; 16]) -> Result<usize, NodeError> {
        self.with_db(|db| {
            let mut removed = 0usize;
            for item in db.outbox_for(dest)? {
                let acked = match crate::maintenance::decode_core(&item.payload) {
                    Ok(CoreMsg::DirectMsg { msg_id: mid, .. })
                    | Ok(CoreMsg::GroupMsg { msg_id: mid, .. }) => mid == *msg_id,
                    _ => false,
                };
                if acked {
                    db.outbox_remove(item.id)?;
                    removed += 1;
                }
            }
            Ok(removed)
        })
    }

    /// Records DHT du dépôt hors-ligne complet pour `dest` (D-016/D-017 : la
    /// totalité de la file, signée puis scellée, fragmentée) et identifiants
    /// d'outbox couverts (à marquer déposés après publication).
    pub fn mailbox_deposit_records(
        &self,
        dest: &[u8; 32],
        now_ms: u64,
    ) -> Result<(Vec<accord_proto::types::DhtRecord>, Vec<i64>), NodeError> {
        self.with_db(|db| {
            let items = db.outbox_for(dest)?;
            if items.is_empty() {
                return Ok((Vec::new(), Vec::new()));
            }
            let payloads: Vec<Vec<u8>> = items.iter().map(|i| i.payload.clone()).collect();
            let records =
                accord_core::offline::deposit_records(&self.identity, dest, &payloads, now_ms)?;
            Ok((records, items.iter().map(|i| i.id).collect()))
        })
    }

    /// Ouvre un dépôt de boîte aux lettres relevé dans la DHT et authentifie
    /// son expéditeur (`expected_sender_node` : node_id du contact sondé).
    pub fn open_mailbox_deposit(
        &self,
        expected_sender_node: &[u8; 32],
        fragment_values: &[Vec<u8>],
    ) -> Result<Vec<Vec<u8>>, NodeError> {
        Ok(accord_core::offline::open_deposit(
            &self.identity,
            expected_sender_node,
            fragment_values,
        )?)
    }
}
