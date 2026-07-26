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
mod dm_sync;
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
pub(crate) mod security;
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
    /// Clé de transport de la machine avec laquelle le canal a été ouvert.
    ///
    /// 🔒 L'appairage ne connaît personne : il n'y a ni amitié ni liste pour
    /// dire d'où doit venir la suite de l'échange. Retenir la machine du canal
    /// est ce qui permet d'exiger que l'entrée scellée, puis la racine du
    /// compte, viennent **de celle-là** — et de savoir où les envoyer.
    pairing_peer: Mutex<Option<[u8; 32]>>,
    /// Racine du compte reçue par la machine qui rejoint, en attente d'adoption.
    ///
    /// 🔒 **En mémoire seulement.** L'écrire ici serait poser le compte sur le
    /// disque sous une clé de base qui n'est pas la sienne. C'est l'hôte qui la
    /// reprend ([`Node::pairing_take_adoption`]) et la scelle dans un coffre
    /// neuf (`identity::adopt_account_seed`).
    pairing_adopted: Mutex<Option<accord_crypto::pairing::AccountSeed>>,
    /// Dernier indicateur de frappe accepté par pair (anti-abus, ms murales).
    typing_seen: Mutex<HashMap<[u8; 32], u64>>,
    /// Cadence des `InviteRedeem` entrants par pair : `(début de fenêtre ms,
    /// compte)`. Anti-abus en mémoire, borné ([`REDEEM_SEEN_MAX_PEERS`]).
    redeem_seen: Mutex<HashMap<[u8; 32], (u64, u32)>>,
    /// Cadence des `SoundboardPlay` entrants par pair : `(début de fenêtre ms,
    /// compte)`. Anti-DoS sonore en mémoire, borné ([`SOUNDBOARD_SEEN_MAX_PEERS`]).
    soundboard_seen: Mutex<HashMap<[u8; 32], (u64, u32)>>,
    /// Identité que le TRANSPORT présente réellement à nos pairs.
    ///
    /// Distincte de [`Node::identity`] dès que cette machine présente sa clé
    /// d'appareil (lot 1.C phase 2). Renseignée au démarrage ; à défaut,
    /// l'identité de compte — ce que présente tout le parc actuel, et ce que
    /// présente un nœud assemblé sans runtime dans les tests.
    ///
    /// 🔒 L'identité complète, pas seulement la clé publique : la présence DHT
    /// et les boîtes aux lettres hors-ligne s'adressent à la machine, donc se
    /// signent et se descellent avec **cette** clé. N'en garder que la partie
    /// publique laisserait ces deux chemins signer au nom du compte des choses
    /// qui n'appartiennent qu'à l'appareil — et deux machines d'un même compte
    /// se réécriraient mutuellement leur adresse dans la DHT.
    transport: OnceLock<Arc<Identity>>,
    /// Rattachements **appareil → compte** prouvés sur une session ouverte,
    /// en mémoire et bornés ([`MAX_DEVICE_OWNERS`]).
    ///
    /// 🔒 C'est la seule façon de rattacher la machine d'un INCONNU. Le chemin
    /// normal ([`crate::device::account_for_static`]) demande « cette clé
    /// est-elle un appareil de tel compte ? » et n'interroge que des comptes
    /// avec lesquels on est en relation — au premier contact, il n'y en a
    /// aucun, et une demande d'ami arriverait sous la clé d'une MACHINE : le
    /// contact serait créé au nom d'un portable, que le code ami du demandeur
    /// ne désignerait jamais, et son second appareil apparaîtrait plus tard
    /// comme une troisième personne.
    ///
    /// 🔒 La preuve tient en deux moitiés indissociables : la racine du compte
    /// a signé une entrée qui porte CETTE clé avec le drapeau de transport, et
    /// la session a prouvé la possession de cette même clé. L'inverse — chercher
    /// à l'aveugle la clé dans toutes les listes connues — serait exploitable :
    /// n'importe qui peut signer une liste qui revendique la clé d'appareil
    /// d'autrui (les nonces de preuve de travail sont publics) et détourner
    /// ainsi le rattachement. Ce qu'il ne peut pas faire, c'est l'annoncer sur
    /// une session scellée avec la clé privée correspondante.
    device_owners: Mutex<HashMap<[u8; 32], [u8; 32]>>,
    /// Listes d'appareils prouvées sur session mais **non persistées**, par
    /// compte. Même borne, même durée de vie que [`Node::device_owners`].
    ///
    /// 🔒 Sans elle, joindre quelqu'un qu'on vient de rencontrer coûterait un
    /// tour de résolution DHT complet — jusqu'à trois minutes — alors que sa
    /// machine vient de nous dire par où la joindre, signature à l'appui. Le
    /// cas est celui de tous les jours : on saisit un code ami, la session
    /// s'ouvre, et la demande d'ami doit partir tout de suite.
    ///
    /// ⚠️ En mémoire seulement, et c'est le compromis : mettre en base la
    /// liste du premier inconnu venu ferait grossir la table d'une ligne par
    /// pair croisé sur la DHT, et cette croissance-là se provoque. Une entrée
    /// perdue coûte un tour de résolution, pas une panne.
    proven_lists: Mutex<HashMap<[u8; 32], accord_proto::device::DeviceList>>,
    profile_frame_migrated: OnceLock<()>,
}

/// Borne du cache de rattachement appareil → compte prouvé sur session.
///
/// Cache best-effort : une entrée évincée se réapprend à la prochaine annonce
/// du pair, qui part à chaque établissement de session avec une relation.
const MAX_DEVICE_OWNERS: usize = 256;

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
            pairing_peer: Mutex::new(None),
            pairing_adopted: Mutex::new(None),
            typing_seen: Mutex::new(HashMap::new()),
            redeem_seen: Mutex::new(HashMap::new()),
            soundboard_seen: Mutex::new(HashMap::new()),
            transport: OnceLock::new(),
            device_owners: Mutex::new(HashMap::new()),
            proven_lists: Mutex::new(HashMap::new()),
            profile_frame_migrated: OnceLock::new(),
        }
    }

    /// Déclare l'identité que le transport présente réellement.
    ///
    /// Appelée une fois au démarrage, avant toute publication de présence ou de
    /// liste d'appareils. Sans appel, c'est l'identité de compte.
    pub fn set_transport_identity(&self, identity: Arc<Identity>) {
        let _ = self.transport.set(identity);
    }

    /// Identité présentée par le transport de cette machine.
    fn transport_identity(&self) -> &Arc<Identity> {
        self.transport.get_or_init(|| Arc::clone(&self.identity))
    }

    /// Clé publique présentée par le transport de cette machine.
    ///
    /// C'est sous elle que nos pairs nous connaissent au niveau réseau : c'est
    /// donc elle qui indexe la présence DHT, notre boîte aux lettres hors-ligne
    /// et notre annonce mDNS. [`Node::public_key`] reste l'identité **durable**
    /// — amitiés, profil, op-logs — et les deux ne se confondent que tant que
    /// le transport n'a pas basculé.
    pub fn transport_key(&self) -> [u8; 32] {
        self.transport_identity().public_key()
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
    /// Deux portes d'entrée, et une seule des deux suffit :
    ///
    /// - **un ami annonce sa propre liste.** 🔒 Elle doit concerner l'émetteur
    ///   lui-même : sans ce contrôle, n'importe quel ami pourrait nous imposer
    ///   la liste d'un tiers — donc y glisser sa propre clé et se faire passer
    ///   pour lui.
    /// - **la liste s'authentifie elle-même** ([`Node::device_list_proves_owner`]).
    ///   C'est le premier contact : l'émetteur nous est encore inconnu, et sans
    ///   cette porte sa demande d'ami arriverait sous la clé d'une machine.
    ///
    /// Une liste seule ne fait toujours pas entrer en relation : elle dit qui
    /// parle, pas qu'on le connaisse.
    fn ingest_device_list(
        &self,
        device_pubkey: &[u8; 32],
        peer_pubkey: &[u8; 32],
        list: &accord_proto::device::DeviceList,
    ) -> Result<(), NodeError> {
        let dun_ami = self.is_friend(peer_pubkey) && list.account == *peer_pubkey;
        let prouvee = self.device_list_proves_owner(device_pubkey, list);
        if !dun_ami && !prouvee {
            return Ok(());
        }
        // Le rattachement est retenu même quand la liste elle-même est refusée
        // plus bas pour cause de version déjà connue : ce qu'il prouve — quelle
        // personne tient cette machine — ne dépend pas de sa nouveauté, et une
        // ré-annonce à l'identique est le cas normal d'une reconnexion.
        if prouvee {
            self.remember_device_owner(*device_pubkey, list.account);
            self.remember_proven_list(list);
        }
        let compte = list.account;
        // 🔒 Prouvé ne veut pas dire persisté. Notre liste part sur CHAQUE
        // session établie, donc la leur aussi : un nœud qui parle à des
        // centaines de pairs DHT verrait sa table grossir d'une ligne par
        // inconnu croisé, et cette croissance-là, on peut la provoquer. Le
        // cache reste borné à ce qu'on cherche à joindre — exactement la même
        // règle que pour un record relevé dans la DHT. Le rattachement, lui,
        // vit en mémoire et sous une borne dure : il suffit à reconnaître qui
        // nous écrit, ce qui est tout ce que l'inconnu nous demande.
        if !self.is_relation(&compte) && !self.has_queued_for(&compte) {
            return Ok(());
        }
        // 🔒 Prouvé ne veut pas dire persisté. Notre liste part sur CHAQUE
        // session établie, donc la leur aussi : un nœud qui parle à des
        // centaines de pairs DHT verrait sa table grossir d'une ligne par
        // inconnu croisé, et cette croissance-là, on peut la provoquer. Le
        // cache reste borné à ce qu'on cherche à joindre — exactement la même
        // règle que pour un record relevé dans la DHT. Le rattachement, lui,
        // vit en mémoire et sous une borne dure : il suffit à reconnaître qui
        // nous écrit, ce qui est tout ce que l'inconnu nous demande.
        let connue = self
            .with_db(|db| Ok(db.device_list(&compte)?))
            .ok()
            .flatten()
            .map(|c| c.version)
            .unwrap_or(0);
        if accord_crypto::verify_device_list(list, &compte, connue).is_err() {
            return Ok(());
        }
        self.store_peer_device_list(&compte, list)
    }

    /// Vrai si `list` prouve que la machine `device_pubkey` agit pour
    /// `list.account`.
    ///
    /// 🔒 La preuve a deux moitiés, et il les faut toutes les deux : la racine
    /// du compte a **signé** une entrée portant cette clé et le drapeau qui dit
    /// qu'elle la présente au transport ; et la session sur laquelle la liste
    /// arrive a **prouvé la possession** de cette même clé. La première seule
    /// se forge — les nonces de preuve de travail sont publics, donc n'importe
    /// qui peut signer une liste revendiquant l'appareil d'autrui. La seconde
    /// seule ne dit rien du compte. Ensemble, elles ne laissent aucune place :
    /// un usurpateur devrait tenir la clé privée de l'appareil qu'il revendique.
    ///
    /// La fraîcheur est exigée pour la même raison que partout ailleurs : une
    /// liste périmée ferait survivre un appareil révoqué (§3.3).
    fn device_list_proves_owner(
        &self,
        device_pubkey: &[u8; 32],
        list: &accord_proto::device::DeviceList,
    ) -> bool {
        // Version connue à zéro : ce n'est pas la nouveauté de la liste qui est
        // en jeu, c'est ce qu'elle atteste. Une ré-annonce identique prouve
        // exactement autant que la première.
        if accord_crypto::verify_device_list(list, &list.account, 0).is_err() {
            return false;
        }
        if !list.is_fresh(now_ms()) || !list.authorises(device_pubkey) {
            return false;
        }
        list.devices
            .iter()
            .any(|d| d.pubkey == *device_pubkey && d.presents_own_key())
    }

    /// Retient un rattachement appareil → compte prouvé (borné).
    fn remember_device_owner(&self, device: [u8; 32], account: [u8; 32]) {
        let mut map = self.device_owners.lock().unwrap_or_else(|e| e.into_inner());
        if map.len() >= MAX_DEVICE_OWNERS && !map.contains_key(&device) {
            if let Some(victime) = map.keys().next().copied() {
                map.remove(&victime);
            }
        }
        map.insert(device, account);
    }

    /// Compte prouvé pour cette clé de transport, s'il en existe un.
    fn proven_device_owner(&self, device: &[u8; 32]) -> Option<[u8; 32]> {
        self.device_owners
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(device)
            .copied()
    }

    /// Retient la liste prouvée d'un compte (bornée, en mémoire).
    ///
    /// 🔒 Écrite au même endroit et sous la même preuve que le rattachement :
    /// seule une machine du compte, ayant prouvé sa clé sur la session, peut
    /// renseigner la liste de CE compte. Un tiers ne peut donc pas détourner
    /// l'éventail de livraison de quelqu'un d'autre.
    fn remember_proven_list(&self, list: &accord_proto::device::DeviceList) {
        let mut map = self.proven_lists.lock().unwrap_or_else(|e| e.into_inner());
        let connue = map.get(&list.account).map(|l| l.version).unwrap_or(0);
        if list.version < connue {
            return; // jamais reculer : une version antérieure ressusciterait un appareil révoqué
        }
        if map.len() >= MAX_DEVICE_OWNERS && !map.contains_key(&list.account) {
            if let Some(victime) = map.keys().next().copied() {
                map.remove(&victime);
            }
        }
        map.insert(list.account, list.clone());
    }

    /// Liste prouvée d'un compte, si elle est connue et encore fraîche.
    fn proven_list(
        &self,
        account: &[u8; 32],
        now_ms: u64,
    ) -> Option<accord_proto::device::DeviceList> {
        self.proven_lists
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(account)
            .filter(|l| l.is_fresh(now_ms))
            .cloned()
    }

    /// Ingère une liste d'appareils relevée dans la DHT.
    ///
    /// C'est la moitié manquante du lot 1.C : on savait publier la sienne et
    /// accepter celle qu'un ami pousse sur une session ouverte, mais pas
    /// l'apprendre **sans** session. Or c'est précisément ce qu'il faut pour
    /// joindre un pair dont les appareils ont basculé : sa présence n'est plus
    /// publiée sous sa clé de compte, et sans sa liste on ne saurait même pas
    /// quelle autre clé chercher. La poule et l'œuf se dénouent parce que la
    /// clé DHT de la liste se **calcule** depuis la seule clé de compte, qu'un
    /// code ami suffit à obtenir.
    ///
    /// 🔒 Bornée à **ce qu'on cherche à joindre**. Un record est pourtant
    /// auto-ancré et vérifié (publieur, clé, signature, preuve de travail,
    /// version croissante) — mais rien n'oblige à garder en base la liste de
    /// gens avec qui on n'a aucun lien, et un cache que n'importe quelle
    /// réponse DHT peut faire grossir est un cache qu'on peut faire grossir
    /// exprès. Le cache ne grandit donc que de ce que NOUS avons décidé
    /// d'envoyer.
    ///
    /// La borne inclut les destinataires en file, et pas seulement les
    /// relations : le protocole laisse écrire à des gens qui ne sont pas nos
    /// amis — une invitation de groupe, la réponse à une invitation. Bornée
    /// aux seules amitiés, la liste d'un invité ne serait jamais mise en
    /// cache, sa clé de compte resterait la seule cible connue, et depuis le
    /// basculement plus personne n'y écoute : l'invitation ne partirait
    /// jamais, sans une erreur nulle part. C'est exactement l'ensemble que la
    /// passe de résolution appelante se donne déjà comme cibles
    /// (`cibles_de_resolution` : amis, demandes sortantes, destinataires
    /// d'outbox).
    pub fn ingest_device_list_record(
        &self,
        account: &[u8; 32],
        record: &accord_proto::types::DhtRecord,
    ) -> Result<(), NodeError> {
        if !self.is_relation(account) && !self.has_queued_for(account) {
            return Ok(());
        }
        self.learn_device_list_record(account, record)
    }

    /// Vrai si quelque chose attend en file pour ce destinataire.
    fn has_queued_for(&self, dest: &[u8; 32]) -> bool {
        self.with_db(|db| Ok(!db.outbox_for(dest)?.is_empty()))
            .unwrap_or(false)
    }

    /// [`Node::ingest_device_list_record`] **sans** la borne aux relations.
    ///
    /// **RÉSERVÉ AUX TESTS** — même statut que `run_with_socket`. Tient lieu,
    /// dans un test de nœud qui ne câble aucune DHT, de la relève que la
    /// maintenance fait en production (`presence_resolve_tick`). Depuis le
    /// basculement elle est indispensable : la clé de compte d'un pair basculé
    /// ne désigne plus aucune machine qui écoute, donc sans sa liste on ne
    /// saurait même pas quoi composer — un test qui se contentait d'inscrire
    /// une adresse au carnet n'a plus rien qui aboutisse.
    ///
    /// 🔒 Aucune vérification n'est retirée : publieur, ancrage de la clé DHT,
    /// signature racine, preuve de travail par appareil et version monotone
    /// s'appliquent toutes. Seule tombe la borne « on ne met en cache que les
    /// gens avec qui on a un lien », qui est une défense en profondeur du
    /// chemin réseau — et ce chemin, lui, passe toujours par la porte gardée
    /// ci-dessus.
    #[doc(hidden)]
    pub fn learn_device_list_record(
        &self,
        account: &[u8; 32],
        record: &accord_proto::types::DhtRecord,
    ) -> Result<(), NodeError> {
        let connue = self
            .with_db(|db| Ok(db.device_list(account)?))
            .ok()
            .flatten()
            .map(|c| c.version)
            .unwrap_or(0);
        let list = crate::device::verify_device_list_record(account, record, connue)?;
        self.store_peer_device_list(account, &list)
    }

    /// Persiste la liste d'appareils d'un pair déjà vérifiée.
    fn store_peer_device_list(
        &self,
        account: &[u8; 32],
        list: &accord_proto::device::DeviceList,
    ) -> Result<(), NodeError> {
        let mut w = accord_proto::Writer::new();
        accord_proto::WireEncode::encode(list, &mut w);
        self.with_db(|db| {
            db.cache_device_list(&accord_core::db::CachedDeviceList {
                account: *account,
                version: list.version,
                encoded: w.into_bytes(),
                fetched_ms: now_ms(),
            })?;
            // 🔒 Apprendre une révocation, c'est aussi devoir oublier ce qui
            // attendait la machine révoquée. La file hors-ligne est indexée par
            // clé de transport et son vidage à la reconnexion d'un pair ne
            // revérifie rien — c'est ce qui la rend rapide. Sans cette purge,
            // un appareil volé qui se rebranche recevrait tout ce qui lui avait
            // été adressé avant, pendant les sept jours de rétention de la
            // file : bien au-delà des vingt-quatre heures que la révocation
            // promet, et sans que rien nulle part ne le signale.
            for revoked in &list.revoked {
                match db.outbox_purge_dest(&revoked.pubkey)? {
                    0 => {}
                    n => tracing::info!(
                        messages = n,
                        appareil = %crate::hex::encode(&revoked.pubkey[..4]),
                        "révocation : messages en attente retirés de la file"
                    ),
                }
            }
            Ok(())
        })?;
        self.retarget_outbox_after_list_change(account);
        Ok(())
    }

    /// Réadresse ce qui attendait à la clé de COMPTE quand la liste qu'on vient
    /// d'apprendre dit que plus aucune machine n'y écoute.
    ///
    /// 🔒 Le pendant de la purge de révocation ci-dessus, et pour la même
    /// raison : la file est indexée par clé de transport et son vidage ne
    /// revérifie rien. Tant qu'on ignore la liste d'un compte, la seule cible
    /// possible est sa clé de compte — c'est ce que présente tout pair non
    /// basculé. Le jour où sa liste arrive et retire cette clé de l'éventail,
    /// tout ce qui attend dessous devient indélivrable **en silence** : l'envoi
    /// direct échoue en liaison d'identité, le dépôt en boîte va à une clé DHT
    /// que le pair ne sonde plus, et la ligne expire sept jours plus tard sans
    /// une erreur nulle part. C'est ce qu'a coûté le basculement à la toute
    /// première demande d'ami de chaque utilisateur.
    ///
    /// ⚠️ Ce n'est pas un second point d'éventail (`docs/MULTI_DEVICE.md` §5) :
    /// l'éventail a bien eu lieu une fois, dans `deliver_core`. C'est la
    /// **correction** d'une cible choisie faute de mieux, faite à l'endroit
    /// exact où arrive l'information qui l'invalide.
    fn retarget_outbox_after_list_change(&self, account: &[u8; 32]) {
        let now = now_ms();
        // Notre propre clé de transport ne peut jamais devenir une cible : une
        // ligne à notre propre nom ne serait vidée par personne, et les passes
        // de résolution la prendraient pour un pair à chercher dans la DHT.
        let moi = self.transport_key();
        let cibles = self
            .with_db(|db| Ok(crate::device::delivery_targets(db, account, now)))
            .map(|t| crate::device::without_self(t, &moi))
            .unwrap_or_default();
        if cibles.contains(account) {
            return;
        }
        match self.with_db(|db| Ok(db.outbox_retarget(account, &cibles, now)?)) {
            Ok(0) => {}
            Ok(n) => tracing::info!(
                messages = n,
                compte = %crate::hex::encode(&account[..4]),
                "appareils : file réadressée vers les machines du compte"
            ),
            Err(e) => tracing::warn!(erreur = %e, "appareils : réadressage de file impossible"),
        }
    }

    /// Vrai si la liste d'appareils en cache pour `account` est encore fraîche.
    ///
    /// Sert à ne relever dans la DHT que ce qui manque : une liste vaut 24 h,
    /// la resolver à chaque passe serait du trafic pur.
    pub fn has_fresh_device_list(&self, account: &[u8; 32]) -> bool {
        self.with_db(|db| Ok(crate::device::has_fresh_list(db, account, now_ms())))
            .unwrap_or(false)
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
    ///
    /// ⚠️ **Seul le côté déjà autorisé répond.** Le côté qui rejoint a émis le
    /// premier HELLO ; lui faire répondre à la réponse mettrait les deux
    /// machines dans une partie de ping-pong qui épuiserait les trois
    /// tentatives de chacune en trois allers-retours, sans qu'aucun humain
    /// n'ait rien fait de travers.
    fn ingest_pairing_hello(&self, device_pubkey: &[u8; 32], peer_msg: &[u8]) -> Vec<CoreMsg> {
        let mut slot = self.pairing_offer.lock().unwrap_or_else(|e| e.into_inner());
        let Some(offer) = slot.as_mut() else {
            return Vec::new();
        };
        let role = offer.role();
        // 🔒 Le canal candidat se **fige** dès qu'il est acquis, et pour la même
        // raison des deux côtés : sans cette garde, un pair quelconque changerait
        // l'empreinte affichée à l'écran juste avant que l'utilisateur ne la
        // compare, et lui ferait confirmer un appairage qui n'est pas le sien.
        //
        // Ce qui vaut « acquis » diffère pourtant selon le rôle, et c'est
        // délibéré. L'appareil autorisé attend la preuve — l'entrée scellée qui
        // s'ouvre — parce qu'il doit laisser ses trois tentatives à qui recopie
        // mal un code. L'appareil qui rejoint, lui, n'attend rien : il a lancé
        // l'échange, la première réponse bien formée est celle qu'il compare, et
        // toute autre ne peut être qu'un voisin qui répond à un code qui n'est
        // pas le sien.
        let frozen = match role {
            crate::pairing::PairingRole::Authoriser => self
                .pairing_pending
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_some(),
            crate::pairing::PairingRole::Joiner => self
                .pairing_channel
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_some(),
        };
        if frozen {
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
                *self.pairing_peer.lock().unwrap_or_else(|e| e.into_inner()) = Some(*device_pubkey);
                tracing::info!("appairage : échange abouti, empreinte à confirmer");
                match role {
                    crate::pairing::PairingRole::Authoriser => {
                        vec![CoreMsg::PairingHello { msg: reply }]
                    }
                    crate::pairing::PairingRole::Joiner => Vec::new(),
                }
            }
            Err(refus) => {
                tracing::debug!(?refus, "appairage : tentative refusée");
                Vec::new()
            }
        }
    }

    /// Vrai si `peer` est bien la machine avec laquelle le canal a été ouvert.
    ///
    /// 🔒 Rien d'autre ne le dit : à ce stade l'appairage ne s'appuie sur
    /// aucune amitié, aucune liste, aucune signature. Sans ce contrôle, la
    /// charge scellée — puis la racine du compte — pourrait arriver d'une
    /// machine qui n'a pas participé à l'échange.
    fn pairing_peer_is(&self, peer: &[u8; 32]) -> bool {
        self.pairing_peer
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some_and(|p| p == *peer)
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
    ///
    /// ⚠️ Réservée au côté déjà autorisé : celui qui rejoint n'inscrit
    /// personne, et une entrée d'appareil qui lui arriverait n'a aucun sens.
    fn ingest_pairing_sealed(&self, device_pubkey: &[u8; 32], sealed: &[u8]) {
        if self.pairing_role() != Some(crate::pairing::PairingRole::Authoriser) {
            tracing::debug!("appairage : entrée d'appareil hors rôle, ignorée");
            return;
        }
        if !self.pairing_peer_is(device_pubkey) {
            tracing::debug!("appairage : entrée d'appareil d'une autre machine, ignorée");
            return;
        }
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

    /// Reçoit la racine du compte sur le canal d'appairage.
    ///
    /// 🔒 Quatre refus, et chacun ferme une porte différente :
    ///
    /// 1. **Le rôle.** Seule une machine qui a *saisi* un code accepte une
    ///    racine. L'appareil autorisé, lui, en détient déjà une : accepter la
    ///    graine d'en face reviendrait à se laisser remplacer son compte par un
    ///    inconnu qui a deviné un code.
    /// 2. **La confirmation.** Tant que l'utilisateur n'a pas comparé
    ///    l'empreinte de ce côté-ci, la racine qui arrive n'a été demandée par
    ///    personne. C'est le sens de « refuser une graine qu'on n'a pas
    ///    demandée » : un échange PAKE abouti ne prouve rien (§4.2).
    /// 3. **La machine.** La graine doit venir de celle avec qui le canal a été
    ///    ouvert, pas d'une autre qui aurait observé l'échange.
    /// 4. **Le canal.** La charge doit s'ouvrir sous sa clé et porter
    ///    l'étiquette d'une racine — c'est le seul échec cryptographique de
    ///    l'appairage qui prouve quelque chose.
    ///
    /// Une seule adoption : une seconde graine, même valide, est ignorée. Rien
    /// dans le protocole ne justifie qu'un compte en remplace un autre en vol.
    ///
    /// ⚠️ Rien n'est écrit sur le disque ici. La graine attend en mémoire que
    /// l'hôte la reprenne et la scelle dans un coffre neuf : la clé de la base
    /// dérive de la graine, donc la base ouverte sous l'ancienne ne peut pas
    /// simplement resservir (voir `identity::adopt_account_seed`).
    fn ingest_pairing_seed(&self, device_pubkey: &[u8; 32], sealed: &[u8]) {
        let confirme = self
            .pairing_offer
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .is_some_and(|o| o.role() == crate::pairing::PairingRole::Joiner && o.is_confirmed());
        if !confirme {
            tracing::warn!("appairage : racine de compte non sollicitée, refusée");
            return;
        }
        if !self.pairing_peer_is(device_pubkey) {
            tracing::warn!("appairage : racine de compte d'une autre machine, refusée");
            return;
        }
        let Some(seed) = self
            .pairing_channel
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .and_then(|c| c.open_account_seed(sealed).ok())
        else {
            tracing::debug!("appairage : racine de compte illisible, ignorée");
            return;
        };
        {
            let mut slot = self
                .pairing_adopted
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if slot.is_some() {
                tracing::debug!("appairage : une racine est déjà en attente, seconde ignorée");
                return;
            }
            *slot = Some(seed);
        }
        // 🔒 Aucune trace du contenu, ici ni ailleurs : ce sont les octets du
        // compte, et un journal survit au processus qui l'a écrit.
        tracing::info!("appairage : racine de compte reçue, adoption en attente");
        self.emit("event.pairing_adopted", serde_json::json!({}));
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
        self.pairing_reset();
        *self.pairing_offer.lock().unwrap_or_else(|e| e.into_inner()) = Some(offer);
        Ok(started)
    }

    /// Efface tout ce qu'un appairage laisse derrière lui, sauf l'offre.
    ///
    /// 🔒 La racine adoptée part avec le reste : une graine qui survivrait à
    /// l'appairage suivant se ferait adopter par un utilisateur qui a changé
    /// d'avis entre-temps.
    fn pairing_reset(&self) {
        *self
            .pairing_channel
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = None;
        *self
            .pairing_pending
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = None;
        *self.pairing_peer.lock().unwrap_or_else(|e| e.into_inner()) = None;
        *self
            .pairing_adopted
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = None;
    }

    /// De quel côté de l'appairage se trouve cette machine, s'il y en a un.
    pub fn pairing_role(&self) -> Option<crate::pairing::PairingRole> {
        self.pairing_offer
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|o| o.role())
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
    ///
    /// Il part **aussi** de lui-même vers tous les pairs Accord visibles sur le
    /// réseau local. C'est la seule chose que le nouvel appareil sait faire : il
    /// a le code et rien d'autre — ni session, ni adresse, et le code ne porte
    /// ni l'une ni l'autre. Le PAKE échoue en silence chez qui n'a pas le code,
    /// donc saluer tout le monde ne dit rien à personne.
    pub fn pairing_submit(&self, code: &str) -> Result<Vec<u8>, NodeError> {
        let parsed = accord_crypto::pairing::PairingCode::parse(code)
            .map_err(|_| NodeError::Invalid("code d'appairage invalide"))?;
        let offer = crate::pairing::PairingOffer::join(parsed, now_ms());
        let outgoing = offer.outgoing().to_vec();
        // Une saisie remplace ce qui était en cours : l'utilisateur qui
        // ressaisit veut repartir de zéro, pas cumuler deux appairages.
        self.pairing_reset();
        *self.pairing_offer.lock().unwrap_or_else(|e| e.into_inner()) = Some(offer);
        self.outbound.send(Outbound::PairingBroadcast);
        Ok(outgoing)
    }

    /// Le message PAKE à envoyer à `peer`, s'il y a lieu de le saluer.
    ///
    /// Rend `Some` **une fois par pair et par offre**, et seulement du côté qui
    /// rejoint : c'est l'appelant réseau qui balaie le LAN, mais c'est ici que
    /// se tient le compte de qui a déjà été salué.
    ///
    /// 🔒 Ce « une fois » est ce qui rend la diffusion acceptable. Chaque HELLO
    /// consomme une des trois tentatives de l'appareil d'en face : ressaluer le
    /// même voisin brûlerait l'offre qu'un utilisateur regarde à l'écran.
    pub fn pairing_hello_for(&self, peer: &[u8; 32]) -> Option<Vec<u8>> {
        let mut slot = self.pairing_offer.lock().unwrap_or_else(|e| e.into_inner());
        let offer = slot.as_mut()?;
        if offer.role() != crate::pairing::PairingRole::Joiner || offer.is_spent(now_ms()) {
            return None;
        }
        // Le canal est déjà ouvert : l'appareil autorisé a répondu, saluer plus
        // loin ne ferait que consommer les tentatives des voisins.
        if self
            .pairing_channel
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some()
        {
            return None;
        }
        offer.greet(peer).then(|| offer.outgoing().to_vec())
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
            // 🔒 L'entrée dit par où joindre CETTE machine, donc elle doit dire
            // laquelle de ses deux clés son transport présente. Un zéro en dur
            // ferait inscrire tout appareil appairé comme joignable par la clé
            // de compte : il n'aurait jamais reçu un seul message, et rien à
            // l'écran n'aurait suggéré pourquoi — il figurerait bien dans
            // « Mes appareils ». C'est l'appareil qui rejoint qui sait, et lui
            // seul : celui qui autorise ne peut que le croire sur parole.
            flags: crate::device::local_device_flags(&self.transport_key(), &device.public_key()),
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
        self.pairing_reset();
    }

    /// Vrai si une racine de compte attend d'être adoptée par cette machine.
    ///
    /// 🔒 Le booléen, et jamais la graine : cette réponse remonte jusqu'à
    /// l'API locale, et rien de ce qui passe par là ne doit contenir le compte.
    pub fn pairing_adopted_ready(&self) -> bool {
        self.pairing_adopted
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some()
    }

    /// Reprend ce qu'il faut pour installer le compte, **une seule fois**.
    ///
    /// Réservée à l'hôte, qui le scelle dans un profil neuf
    /// (`identity::adopt_account_seed`) et redémarre le nœud dessus. Rien de
    /// tout cela ne transite par l'API JSON locale.
    ///
    /// 🔒 La racine part avec la clé d'appareil, jamais seule : l'appareil
    /// autorisé vient d'inscrire CETTE clé-là dans la liste signée du compte.
    /// Un profil adopté qui en régénérerait une autre serait listé sous une
    /// identité qu'il ne détient plus.
    pub fn pairing_take_adoption(&self) -> Option<crate::pairing::AccountAdoption> {
        // L'appareil local d'abord : sans lui il n'y a rien à adopter, et la
        // racine doit rester en attente plutôt que de se consommer pour rien.
        let device = self.with_db(|db| Ok(db.local_device()?)).ok().flatten()?;
        let seed = self
            .pairing_adopted
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()?;
        Some(crate::pairing::AccountAdoption::new(seed, device))
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
        let role = self
            .pairing_role()
            .ok_or(NodeError::Invalid("aucun appairage en cours"))?;
        match role {
            crate::pairing::PairingRole::Authoriser => self.pairing_confirm_authoriser(),
            crate::pairing::PairingRole::Joiner => self.pairing_confirm_joiner(),
        }
    }

    /// Confirmation côté **déjà autorisé** : inscrit l'appareil, publie la
    /// liste, puis lui remet la racine du compte.
    ///
    /// 🔒 L'ordre est l'invariant. La racine ne part qu'après l'inscription
    /// réussie : si la liste est pleine, ou si la signature ou l'écriture
    /// échouent, l'appareil d'en face n'a rien à faire du compte — et une
    /// racine remise à une machine qui n'est même pas dans la liste serait un
    /// accès qu'aucun écran ne montre et qu'aucune révocation n'atteint.
    fn pairing_confirm_authoriser(&self) -> Result<(), NodeError> {
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
        if issue.is_ok() {
            self.send_account_seed();
        }
        // Inscrit ou non, l'appareil proposé a joué son rôle : le garder
        // ferait ignorer le premier HELLO de l'appairage suivant.
        *self
            .pairing_pending
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = None;
        issue
    }

    /// Confirmation côté **qui rejoint** : scelle l'entrée de cette machine et
    /// l'envoie à l'appareil autorisé, qui décidera de l'inscrire.
    ///
    /// 🔒 C'est aussi ce geste qui autorise cette machine à *accepter* une
    /// racine ensuite : sans lui, la graine qui arriverait n'aurait été
    /// demandée par personne (voir [`Node::ingest_pairing_seed`]).
    fn pairing_confirm_joiner(&self) -> Result<(), NodeError> {
        let peer = self
            .pairing_peer
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .ok_or(NodeError::Invalid("aucun appareil en face"))?;
        let sealed = self
            .pairing_sealed_self()
            .ok_or(NodeError::Invalid("aucun appareil local à proposer"))?;
        {
            let mut slot = self.pairing_offer.lock().unwrap_or_else(|e| e.into_inner());
            let offer = slot
                .as_mut()
                .ok_or(NodeError::Invalid("aucun appairage en cours"))?;
            offer
                .confirm(now_ms())
                .map_err(|_| NodeError::Invalid("appairage expiré ou déjà scellé"))?;
        }
        self.outbound.send(Outbound::Core {
            to: peer,
            msg: Box::new(CoreMsg::PairingSealed { sealed }),
        });
        Ok(())
    }

    /// Scelle la racine du compte sous la clé du canal et l'envoie à
    /// l'appareil qui vient d'être inscrit.
    ///
    /// 🔒 Appelée **uniquement** depuis [`Node::pairing_confirm_authoriser`],
    /// après confirmation humaine et inscription réussie. Un échec de
    /// scellement ou l'absence de canal se journalise sans détail : mieux vaut
    /// un appareil inscrit mais non promu — l'utilisateur relancera un
    /// appairage — qu'une racine remise sur un chemin qu'on n'a pas su vérifier.
    fn send_account_seed(&self) {
        let Some(peer) = *self.pairing_peer.lock().unwrap_or_else(|e| e.into_inner()) else {
            tracing::warn!("appairage : aucune machine en face, racine non transmise");
            return;
        };
        let seed = accord_crypto::pairing::AccountSeed::new(*self.identity.seed());
        let Some(sealed) = self
            .pairing_channel
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .and_then(|c| c.seal_account_seed(&seed).ok())
        else {
            tracing::warn!("appairage : canal indisponible, racine non transmise");
            return;
        };
        self.outbound.send(Outbound::Core {
            to: peer,
            msg: Box::new(CoreMsg::PairingSeed { sealed }),
        });
        tracing::info!("appairage : racine de compte transmise à l'appareil inscrit");
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
    ///
    /// 🔒 Les comptes éligibles sont nos amis **et le nôtre**. On n'est pas son
    /// propre ami : une liste bornée aux amitiés ferait de notre propre
    /// portable un inconnu, avec lequel rien ne se rattache — ni l'adresse au
    /// carnet, ni la session vivante, ni le rattrapage d'historique. Le
    /// paramètre de `account_for_static` s'appelle `accounts`, et non « amis »,
    /// exactement pour ce cas.
    ///
    /// 🔒 Le repli sur [`Node::proven_device_owner`] couvre le PREMIER CONTACT,
    /// et lui seul : un demandeur d'ami n'est encore ni ami ni nous-même, donc
    /// aucun compte éligible ne peut revendiquer sa machine. Le repli n'élargit
    /// pas la liste des comptes interrogés — ce serait ouvrir la porte à
    /// quiconque signe une liste revendiquant la clé d'appareil d'autrui : il
    /// n'accepte que des rattachements que le pair a **prouvés** sur une
    /// session (voir le champ `device_owners`).
    pub fn account_of_transport_key(&self, static_pub: &[u8; 32]) -> [u8; 32] {
        let mut accounts = self.friend_pubkeys().unwrap_or_default();
        accounts.push(self.public_key());
        self.with_db(|db| {
            Ok(crate::device::account_for_static(
                db,
                &accounts,
                static_pub,
                now_ms(),
            ))
        })
        .ok()
        .flatten()
        .or_else(|| self.proven_device_owner(static_pub))
        .unwrap_or(*static_pub)
    }

    /// Clés de transport par lesquelles joindre `account` (lot 1.E).
    ///
    /// Rend toujours au moins une cible : un compte dont on ne sait rien reste
    /// joignable par sa clé, qui est ce que présente tout pair non basculé.
    ///
    /// Deux sources pour la liste, une seule règle pour en déduire les cibles
    /// ([`crate::device::targets_from_list`]) : le cache en base d'abord —
    /// celui d'une relation, refraîchi par la DHT —, puis la liste **prouvée
    /// sur session** d'un pair qu'on ne connaît pas encore. Sans ce second
    /// recours, écrire à quelqu'un dont on vient de saisir le code ami
    /// attendrait la prochaine passe de résolution, jusqu'à trois minutes,
    /// alors que sa machine vient de nous dire par où la joindre.
    pub fn delivery_targets(&self, account: &[u8; 32]) -> Vec<[u8; 32]> {
        let now = now_ms();
        let liste = self
            .with_db(|db| Ok(crate::device::cached_list_for(db, account, now)))
            .ok()
            .flatten()
            .or_else(|| self.proven_list(account, now));
        liste.map_or_else(
            || vec![*account],
            |list| crate::device::targets_from_list(&list, account),
        )
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
            crate::device::local_device_flags(&self.transport_key(), &local.public_key()),
        ))
    }

    /// Aligne l'entrée de l'appareil local sur la clé que le transport présente
    /// vraiment, et republie si elle avait bougé.
    ///
    /// 🔒 Appelée une fois au démarrage. Sans elle, la liste persistée garderait
    /// éternellement le drapeau du jour où elle a été signée : la mise à jour
    /// qui bascule le transport sur la clé d'appareil laisserait les
    /// correspondants continuer d'écrire à la clé de compte — que plus personne
    /// n'écoute. Le retour en arrière (drapeau à retirer) compte tout autant, et
    /// c'est pour ça que la comparaison se fait dans les deux sens.
    pub fn reconcile_local_device_flags(&self) -> Result<(), NodeError> {
        let Some(stored) = self.with_db(|db| Ok(db.local_device()?))? else {
            return Ok(());
        };
        let local = accord_crypto::DeviceIdentity::from_seed(stored.seed).public_key();
        let voulu = crate::device::local_device_flags(&self.transport_key(), &local);
        let mut list = self.current_device_list()?;
        let Some(entry) = list.devices.iter_mut().find(|d| d.pubkey == local) else {
            return Ok(());
        };
        if entry.flags == voulu {
            return Ok(());
        }
        entry.flags = voulu;
        list.version = accord_crypto::version_for(now_ms());
        accord_crypto::sign_device_list_with_root(&self.identity, &mut list);
        self.store_device_list(&list)?;
        self.publish_device_list(&list);
        tracing::info!(
            presente_sa_cle = voulu != 0,
            "liste d'appareils : entrée locale réalignée sur la clé de transport"
        );
        Ok(())
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

    /// Décode et authentifie une liste d'appareils de NOTRE PROPRE compte,
    /// relevée dans la DHT, en vue d'une fusion.
    ///
    /// 🔒 La contrainte de version monotone est délibérément relâchée ici, et
    /// c'est sans danger : elle existe pour empêcher un TIERS de rejouer une
    /// liste ancienne, or ce record est signé par notre propre racine, que nul
    /// autre ne détient. Et fusionner une vue périmée ne peut rien perdre —
    /// l'union ne retire jamais, et une révocation garde la priorité.
    /// L'exiger reviendrait à refuser exactement le cas qu'on cherche à
    /// rattraper : une copie publiée à la même version que la nôtre.
    pub fn decode_own_device_list(
        &self,
        record: &accord_proto::types::DhtRecord,
    ) -> Option<accord_proto::device::DeviceList> {
        crate::device::verify_device_list_record(&self.public_key(), record, 0).ok()
    }

    /// Réémet la liste d'appareils du compte, fusionnée avec `autre`.
    ///
    /// 🔒 C'est ce qui remplace le « dernier écrivain gagne » par une union
    /// (voir `device::merge_device_lists`). Sans elle, une machine à la vue
    /// incomplète — fraîchement appairée, ou éteinte pendant une inscription —
    /// effaçait les autres appareils du compte chez tous les correspondants.
    ///
    /// Réémet aussi la date : `issued_ms` n'était écrit qu'à la construction,
    /// de sorte que la liste se périmait vingt-quatre heures après le premier
    /// démarrage, et qu'une republication à version égale était refusée par les
    /// pairs — qui restaient bloqués sur le repli, sans issue.
    pub fn reissue_device_list(
        &self,
        autre: Option<&accord_proto::device::DeviceList>,
    ) -> Result<accord_proto::device::DeviceList, NodeError> {
        let ours = self.current_device_list()?;
        let source = autre.unwrap_or(&ours);
        let mut list = crate::device::merge_device_lists(&ours, source, now_ms());
        accord_crypto::sign_device_list_with_root(&self.identity, &mut list);
        self.store_device_list(&list)?;
        Ok(list)
    }

    /// Annonce une liste fraîchement réémise aux amis connectés.
    pub fn announce_device_list(&self, list: &accord_proto::device::DeviceList) {
        self.publish_device_list(list);
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
    ///
    /// Forme à une clé : la machine et la personne s'y confondent. Le routeur,
    /// lui, connaît les deux et passe par [`Node::ingest_core_from`].
    pub fn ingest_core(
        &self,
        peer_pubkey: &[u8; 32],
        msg: CoreMsg,
    ) -> Result<Vec<CoreMsg>, NodeError> {
        self.ingest_core_from(peer_pubkey, peer_pubkey, msg)
    }

    /// [`Node::ingest_core`] en distinguant la **machine** émettrice de la
    /// **personne** à laquelle elle appartient.
    ///
    /// 🔒 Presque tout raisonne sur `peer_pubkey`, la personne : une amitié, un
    /// profil, un op-log appartiennent à quelqu'un, pas à une machine. Le
    /// rattrapage entre nos propres appareils est l'exception, et il n'a pas le
    /// choix : le routeur traduit appareil → compte à son bord
    /// (`docs/MULTI_DEVICE.md` §5.1), si bien que nos trois machines arrivent
    /// ici sous une seule et même clé de compte. Un curseur par appareil frère
    /// serait impossible à tenir sans savoir laquelle a parlé.
    pub fn ingest_core_from(
        &self,
        device_pubkey: &[u8; 32],
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
                self.ingest_device_list(device_pubkey, peer_pubkey, &list)?;
                Ok(vec![])
            }
            CoreMsg::SelfReadMark { scope, conv, up_to } => {
                // Autorisé sur la clé de la MACHINE, pas sur la personne : que
                // le routeur ait su ou non remonter au compte, c'est la clé que
                // la session a authentifiée qui décide, et `is_own_device`
                // accepte les deux formes du parc mixte.
                self.ingest_self_read_mark(device_pubkey, scope, &conv, &up_to)?;
                Ok(vec![])
            }
            CoreMsg::SelfSyncOffer {
                conv,
                count,
                max_lamport,
                digest,
            } => self.ingest_self_sync_offer(
                device_pubkey,
                accord_core::dm_sync::SyncOffer {
                    conv,
                    count,
                    max_lamport,
                    digest,
                },
            ),
            CoreMsg::SelfSyncPull {
                conv,
                since_lamport,
                max_items,
            } => self.ingest_self_sync_pull(device_pubkey, &conv, since_lamport, max_items),
            CoreMsg::SelfSyncItem {
                conv,
                msg_id,
                author,
                lamport,
                sent_ms,
                kind,
                body,
                acked,
                deleted,
                edited,
            } => {
                // ⚠️ `sent_ms` est repris tel quel : c'est l'heure d'ENVOI du
                // message, pas celle de sa recopie. La recalculer ferait
                // apparaître, sur la machine qui rattrape, une conversation
                // entière datée d'aujourd'hui.
                self.ingest_self_sync_item(
                    device_pubkey,
                    &accord_core::db::DmRecord {
                        msg_id,
                        peer: conv,
                        author,
                        lamport,
                        sent_ms,
                        kind,
                        body,
                        acked,
                        deleted,
                        edited,
                    },
                )?;
                Ok(vec![])
            }
            // 🔒 L'appairage raisonne sur la MACHINE, pas sur la personne : les
            // deux appareils ne se connaissent pas encore, il n'y a ni amitié
            // ni liste pour les rattacher à un compte. `device_pubkey` est la
            // seule identité qui ait un sens ici.
            CoreMsg::PairingHello { msg } => Ok(self.ingest_pairing_hello(device_pubkey, &msg)),
            CoreMsg::PairingSealed { sealed } => {
                self.ingest_pairing_sealed(device_pubkey, &sealed);
                Ok(vec![])
            }
            CoreMsg::PairingSeed { sealed } => {
                self.ingest_pairing_seed(device_pubkey, &sealed);
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
    ///
    /// 🔒 Ancré sur la clé de **transport**, pas sur celle du compte. Une
    /// présence par compte ne peut porter qu'un jeu d'adresses : deux machines
    /// d'un même compte publieraient à la même clé DHT, chacune avec une
    /// signature valide, et le dernier écrivain gagnerait. Les adresses du
    /// compte se mettraient à osciller entre les deux machines au rythme de la
    /// republication, et un correspondant sur deux joindrait la mauvaise. La
    /// liste d'appareils sert d'indirection : compte → appareils → présences.
    pub fn presence_record(
        &self,
        addrs: &[std::net::SocketAddr],
    ) -> accord_proto::types::DhtRecord {
        let identity = self.transport_identity();
        let mut record = accord_proto::types::DhtRecord {
            key: crate::maintenance::presence_key(&identity.public_key()),
            kind: accord_proto::types::RecordKind::Presence,
            value: crate::maintenance::encode_presence_value(addrs),
            publisher: identity.public_key(),
            timestamp_ms: now_ms(),
            expiry_s: crate::maintenance::PRESENCE_EXPIRY_S,
            sig: [0u8; 64],
        };
        record.sig = identity.sign(&record.signable_bytes());
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
            // 🔒 Déposé au nom de l'APPAREIL. La clé de boîte mêle le nœud du
            // destinataire, le jour et le nœud de l'expéditeur : deux machines
            // d'un même compte déposant pour la même personne le même jour
            // écriraient à la même clé DHT, et le dernier écrivain effacerait
            // le dépôt de l'autre. Le destinataire sonde une clé par appareil
            // de l'expéditeur, ce qui redonne à chacune sa case.
            let records = accord_core::offline::deposit_records(
                self.transport_identity(),
                dest,
                &payloads,
                now_ms,
            )?;
            Ok((records, items.iter().map(|i| i.id).collect()))
        })
    }

    /// Ouvre un dépôt de boîte aux lettres relevé dans la DHT et authentifie
    /// son expéditeur (`expected_sender_node` : node_id du contact sondé).
    ///
    /// 🔒 Descellé avec l'identité de **transport** : un dépôt est scellé pour
    /// la clé à laquelle l'expéditeur a essayé de livrer, c'est-à-dire un
    /// appareil. L'ouvrir avec la clé de compte échouerait dès que les deux
    /// diffèrent — et la boîte deviendrait silencieusement muette, sans erreur
    /// visible ailleurs qu'ici. L'expéditeur, lui, reste identifié par son
    /// compte : c'est sous ce nom qu'on le connaît.
    pub fn open_mailbox_deposit(
        &self,
        expected_sender_node: &[u8; 32],
        fragment_values: &[Vec<u8>],
    ) -> Result<Vec<Vec<u8>>, NodeError> {
        Ok(accord_core::offline::open_deposit(
            self.transport_identity(),
            expected_sender_node,
            fragment_values,
        )?)
    }
}
