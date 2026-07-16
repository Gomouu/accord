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
use accord_core::{friends, group, messaging, presence, profile, search};
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

pub(crate) mod discovery;
mod dm;
mod files;
mod groups;
pub(crate) mod holepunch;
mod mentions;
pub(crate) mod nat;
pub(crate) mod network;
pub(crate) mod relay;
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

/// Nœud Accord déverrouillé.
pub struct Node {
    identity: Arc<Identity>,
    search_key: Zeroizing<[u8; 32]>,
    db: Mutex<Db>,
    outbound: OutboundSink,
    hub: Option<NotificationHub>,
    /// Contrôle réseau (pilotage des méthodes `network.*`), branché après la
    /// construction du runtime ; absent dans les tests sans réseau.
    network: OnceLock<Arc<dyn network::NetworkControl>>,
    /// Amis présumés en ligne (dernier signal reçu). Best-effort, en mémoire :
    /// l'absence d'un pair ne prouve pas qu'il est hors ligne (§6, présence).
    online: Mutex<HashSet<[u8; 32]>>,
    /// Rich presence explicitly announced by friends (`PRESENCE` 0x08):
    /// wire status 0-2 plus optional custom text. Best-effort, in memory,
    /// friends only (anti-abuse); an offline announcement clears the entry.
    peer_status: Mutex<HashMap<[u8; 32], RichPresence>>,
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
            network: OnceLock::new(),
            online: Mutex::new(HashSet::new()),
            peer_status: Mutex::new(HashMap::new()),
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
        let db = self.db.lock().expect("verrou base empoisonné");
        f(&db)
    }

    /// Record d'identité à (re)publier dans la DHT.
    pub fn identity_record(&self) -> accord_proto::types::DhtRecord {
        friends::identity_record(&self.identity, now_ms())
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
            .expect("verrou présence empoisonné")
            .get(peer)
            .cloned()
        {
            return explicit;
        }
        let reachable = self
            .online
            .lock()
            .expect("verrou présence empoisonné")
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
            let mut set = self.online.lock().expect("verrou présence empoisonné");
            if online {
                set.insert(*peer);
            } else {
                set.remove(peer);
            }
        }
        if !online {
            self.peer_status
                .lock()
                .expect("verrou présence empoisonné")
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
            .expect("verrou présence empoisonné")
            .insert(*peer);
        self.peer_status
            .lock()
            .expect("verrou présence empoisonné")
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
            .expect("verrou présence empoisonné")
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
        let mut seen = self.typing_seen.lock().expect("verrou frappe empoisonné");
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
        let mut seen = self.redeem_seen.lock().expect("verrou rachat empoisonné");
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
            .expect("verrou soundboard empoisonné");
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
                if let Some(applied) = updated {
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
                Ok(vec![])
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
                        .expect("verrou présence empoisonné")
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

    /// Recherche locale par intersection de mots.
    pub fn search(&self, query: &str) -> Result<Vec<String>, NodeError> {
        self.with_db(|db| {
            Ok(search::search(db, &self.search_key, query)?
                .iter()
                .map(|id| hex::encode(id))
                .collect())
        })
    }

    /// Réactions stockées pour un message (DM ou groupe) : `(emoji, auteur)`.
    pub fn reactions_of(&self, msg_id: &[u8; 16]) -> Result<Vec<(String, [u8; 32])>, NodeError> {
        self.with_db(|db| Ok(db.reactions(msg_id)?))
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
    pub fn outbox_mark_mailboxed(&self, id: i64) -> Result<(), NodeError> {
        self.with_db(|db| Ok(db.outbox_mark_mailboxed(id)?))
    }

    /// Retire un élément d'outbox livré.
    pub fn outbox_remove(&self, id: i64) -> Result<(), NodeError> {
        self.with_db(|db| Ok(db.outbox_remove(id)?))
    }

    /// Purge les éléments d'outbox expirés ; rend le nombre supprimé.
    pub fn outbox_purge_expired(&self, now_ms: u64) -> Result<usize, NodeError> {
        self.with_db(|db| Ok(db.outbox_purge_expired(now_ms)?))
    }

    /// Solde les `DirectMsg` en file pour `dest` acquittés par `msg_id` ;
    /// rend le nombre d'éléments retirés.
    pub fn outbox_ack(&self, dest: &[u8; 32], msg_id: &[u8; 16]) -> Result<usize, NodeError> {
        self.with_db(|db| {
            let mut removed = 0usize;
            for item in db.outbox_for(dest)? {
                if let Ok(CoreMsg::DirectMsg { msg_id: mid, .. }) =
                    crate::maintenance::decode_core(&item.payload)
                {
                    if mid == *msg_id {
                        db.outbox_remove(item.id)?;
                        removed += 1;
                    }
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
