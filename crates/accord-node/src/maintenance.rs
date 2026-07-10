//! Boucles de maintenance réseau du nœud (D-024).
//!
//! Six tâches périodiques, bornées et à intervalles jitterés (aucun réveil
//! intempestif : chaque tâche dort entre deux passes, et s'arrête au signal
//! d'arrêt du runtime) :
//!
//! 1. republication du record d'identité DHT (re-signé à chaque passe) ;
//! 2. publication de la présence (adresses du nœud) sous une clé dérivée de
//!    l'identité, avec demande d'observation d'adresse si besoin ;
//! 3. résolution de la présence des amis, qui alimente le carnet d'adresses
//!    appris du runtime ;
//! 4. vidage de l'outbox : renvoi direct avec backoff (géré par la base) et
//!    dépôt en boîte aux lettres DHT (D-016/D-017) après échecs répétés ;
//! 5. relève des boîtes aux lettres (dépôts hors-ligne des amis) et offres
//!    `GroupSync` périodiques vers les membres joignables des groupes ;
//! 6. ré-annonce du profil local (pseudo, bio, hash d'avatar — D-027) aux
//!    amis joignables, à la cadence de la republication d'identité.
//!
//! Les décisions (jitter, fenêtres de rotation, éligibilité à la file
//! hors-ligne, seuil de dépôt, validation de présence) sont des fonctions
//! pures testables sans horloge ni réseau. La journalisation `tracing` ne
//! porte que des compteurs — jamais de clé, d'adresse ni de contenu.

use std::future::Future;
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use accord_core::offline;
use accord_crypto::{node_id_of, verify_signature};
use accord_proto::core_msg::CoreMsg;
use accord_proto::limits::MAX_NODE_ADDRS;
use accord_proto::plaintext::ChannelMsg;
use accord_proto::types::{DhtRecord, RecordKind, WireAddr};
use accord_proto::{ControlMsg, WireDecode, WireEncode};
use accord_transport::nat::{Candidate, CandidateKind};
use rand::RngCore;
use tokio::sync::watch;

use crate::error::NodeError;
use crate::node::now_ms;
use crate::node::relay::{self, NatKind};
use crate::runtime::Runtime;

/// Durée de vie d'un record de présence (le publieur republie bien avant).
pub const PRESENCE_EXPIRY_S: u32 = 900;
/// Version du format de valeur d'un record de présence.
const PRESENCE_VERSION: u8 = 1;
/// Tolérance d'horloge entre pairs pour juger la fraîcheur d'une présence.
const CLOCK_SKEW_MS: u64 = 5 * 60 * 1000;
/// Délai maximal avant la première passe d'une boucle (borné par l'intervalle).
const STARTUP_DELAY: Duration = Duration::from_secs(2);

/// Intervalles et bornes des boucles de maintenance (défauts sûrs).
#[derive(Debug, Clone)]
pub struct MaintenanceConfig {
    /// Active les boucles (désactivable pour les tests unitaires du runtime).
    pub enabled: bool,
    /// Intervalle de republication du record d'identité.
    pub identity_republish: Duration,
    /// Intervalle de publication de la présence.
    pub presence_publish: Duration,
    /// Intervalle de résolution de la présence des amis.
    pub presence_resolve: Duration,
    /// Intervalle de vidage de l'outbox.
    pub outbox_flush: Duration,
    /// Intervalle de relève des boîtes aux lettres hors-ligne.
    pub mailbox_poll: Duration,
    /// Intervalle d'émission des offres `GroupSync`.
    pub group_sync: Duration,
    /// Intervalle de reconnexion aux pairs d'amorçage (base du backoff, B2).
    pub bootstrap_reconnect: Duration,
    /// Amplitude de jitter relative (0.2 ⇒ ±10 % autour de l'intervalle).
    pub jitter: f64,
    /// Nombre maximal d'éléments d'outbox traités par passe.
    pub outbox_batch: usize,
    /// Nombre maximal de contacts sondés par passe (présence, boîtes).
    pub contacts_per_tick: usize,
    /// Tentatives directes échouées avant dépôt en boîte aux lettres DHT.
    pub mailbox_after_attempts: u32,
}

impl Default for MaintenanceConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            identity_republish: Duration::from_secs(30 * 60),
            presence_publish: Duration::from_secs(5 * 60),
            presence_resolve: Duration::from_secs(3 * 60),
            outbox_flush: Duration::from_secs(30),
            mailbox_poll: Duration::from_secs(10 * 60),
            group_sync: Duration::from_secs(5 * 60),
            bootstrap_reconnect: Duration::from_secs(60),
            jitter: 0.2,
            outbox_batch: 64,
            contacts_per_tick: 32,
            mailbox_after_attempts: 2,
        }
    }
}

// ---- Fonctions de décision pures (testables sans horloge ni réseau) ----

/// Applique un jitter relatif à une durée : `base × (1 ± jitter/2)`, la
/// position dans l'intervalle étant tirée de `salt` (déterministe à salt
/// donné, pour les tests).
pub fn jittered(base: Duration, jitter: f64, salt: u64) -> Duration {
    if base.is_zero() {
        return base;
    }
    let jitter = jitter.clamp(0.0, 1.0);
    let unit = (salt as f64) / (u64::MAX as f64); // ∈ [0, 1]
    base.mul_f64(1.0 + jitter * (unit - 0.5))
}

/// Délai avant la première passe d'une boucle : court mais jamais plus long
/// que l'intervalle lui-même (les tests utilisent des intervalles brefs).
pub fn startup_delay(interval: Duration) -> Duration {
    interval.min(STARTUP_DELAY)
}

/// Corps `DirectMsg` éphémères, jamais mis en file hors-ligne : indicateur de
/// frappe (kind 5) et accusé de lecture (kind 6) — voir `MsgBody::kind`.
const EPHEMERAL_DM_KINDS: [u8; 2] = [5, 6];

/// Vrai si un `CoreMsg` non livrable doit être mis en file hors-ligne.
///
/// Les messages porteurs d'état (messages, ops, clés, accusés, demandes
/// d'ami, profils D-027) sont conservés ; les messages éphémères ou
/// d'anti-entropie (présence, retrait d'amitié, frappe, accusés de lecture,
/// offres de synchronisation) sont simplement perdus — ils seront réémis par
/// leurs boucles respectives ou n'ont pas vocation à survivre.
pub fn is_queueable_offline(msg: &CoreMsg) -> bool {
    match msg {
        // Typing indicators and read receipts are ephemeral DirectMsg kinds.
        CoreMsg::DirectMsg { kind, .. } => !EPHEMERAL_DM_KINDS.contains(kind),
        CoreMsg::MsgAck { .. }
        | CoreMsg::FriendRequest { .. }
        | CoreMsg::FriendResponse { .. }
        | CoreMsg::GroupOpMsg { .. }
        | CoreMsg::GroupMsg { .. }
        | CoreMsg::GroupKey { .. }
        | CoreMsg::Profile { .. } => true,
        _ => false,
    }
}

/// Vrai si un élément d'outbox doit (re)déclencher un dépôt en boîte aux
/// lettres DHT : jamais encore déposé et assez de tentatives directes.
pub fn should_deposit(attempts: u32, mailboxed: bool, after_attempts: u32) -> bool {
    !mailboxed && attempts >= after_attempts
}

/// Délai de reconnexion d'un pair d'amorçage après `fails` échecs consécutifs :
/// backoff exponentiel `base × 2^min(fails, PLAFOND)`, borné pour éviter les
/// débordements et les attentes déraisonnables.
pub fn reconnect_backoff(base: Duration, fails: u32) -> Duration {
    /// Plafond d'exposant (32 × base au maximum).
    const PLAFOND: u32 = 5;
    let facteur = 1u32 << fails.min(PLAFOND);
    base.saturating_mul(facteur)
}

/// Fenêtre de rotation sur `len` éléments à partir de `cursor` : rend les
/// indices à visiter (au plus `max`) et le curseur suivant. Garantit
/// l'absence de famine quand `len > max`.
pub fn rotation(len: usize, cursor: usize, max: usize) -> (Vec<usize>, usize) {
    if len == 0 || max == 0 {
        return (Vec::new(), 0);
    }
    let take = max.min(len);
    let start = cursor % len;
    let indices = (0..take).map(|i| (start + i) % len).collect();
    (indices, (start + take) % len)
}

/// Encode un `CoreMsg` pour l'outbox persistante.
pub fn encode_core(msg: &CoreMsg) -> Vec<u8> {
    msg.to_bytes()
}

/// Décode une charge d'outbox ou de boîte aux lettres en `CoreMsg`.
pub fn decode_core(bytes: &[u8]) -> Result<CoreMsg, NodeError> {
    CoreMsg::from_bytes(bytes).map_err(|_| NodeError::Invalid("CoreMsg illisible"))
}

// ---- Présence : clé, valeur, vérification ----

/// Clé DHT de présence d'un nœud : son identifiant Kademlia (le record est
/// donc stocké au voisinage du nœud lui-même, à la Kademlia).
pub fn presence_key(pubkey: &[u8; 32]) -> [u8; 32] {
    node_id_of(pubkey).0
}

/// Encode la valeur d'un record de présence : `version(1) ‖ list<WireAddr>`.
pub fn encode_presence_value(addrs: &[SocketAddr]) -> Vec<u8> {
    let list: Vec<WireAddr> = addrs
        .iter()
        .take(MAX_NODE_ADDRS)
        .map(|a| WireAddr(*a))
        .collect();
    let mut w = accord_proto::Writer::new();
    w.put_u8(PRESENCE_VERSION);
    w.put_list(&list, |w, a| a.encode(w));
    w.into_bytes()
}

/// Décode la valeur d'un record de présence.
pub fn decode_presence_value(bytes: &[u8]) -> Result<Vec<SocketAddr>, NodeError> {
    let mut r = accord_proto::Reader::new(bytes);
    let version = r
        .u8()
        .map_err(|_| NodeError::Invalid("record de présence vide"))?;
    if version != PRESENCE_VERSION {
        return Err(NodeError::Invalid("version de présence inconnue"));
    }
    let list = r
        .list(MAX_NODE_ADDRS, "presence.addrs", WireAddr::decode)
        .map_err(|_| NodeError::Invalid("adresses de présence illisibles"))?;
    r.finish()
        .map_err(|_| NodeError::Invalid("octets excédentaires de présence"))?;
    Ok(list.into_iter().map(|w| w.0).collect())
}

/// Vérifie de bout en bout un record de présence censé venir de `expected` :
/// nature, clé, auto-publication, fraîcheur et signature. Rend les adresses.
pub fn verify_presence_record(
    expected: &[u8; 32],
    record: &DhtRecord,
    now_ms: u64,
) -> Result<Vec<SocketAddr>, NodeError> {
    if record.kind != RecordKind::Presence {
        return Err(NodeError::Invalid(
            "record de présence de nature inattendue",
        ));
    }
    if record.publisher != *expected {
        return Err(NodeError::Invalid("présence non auto-publiée"));
    }
    if record.key != presence_key(expected) {
        return Err(NodeError::Invalid(
            "clé de présence sans rapport avec le pair",
        ));
    }
    let valid_until = record
        .timestamp_ms
        .saturating_add(u64::from(record.expiry_s) * 1000)
        .saturating_add(CLOCK_SKEW_MS);
    if valid_until < now_ms {
        return Err(NodeError::Invalid("record de présence périmé"));
    }
    verify_signature(&record.publisher, &record.signable_bytes(), &record.sig)?;
    decode_presence_value(&record.value)
}

// ---- Classement des candidats de poinçonnage (SPEC §11) ----

/// Vrai si une IP est locale/privée, donc non routable sur l'Internet public :
/// loopback, RFC1918 (IPv4 privé), lien-local IPv4 (169.254/16), ULA IPv6
/// (fc00::/7) ou lien-local IPv6 (fe80::/10). Les méthodes `Ipv6Addr` pour l'ULA
/// et le lien-local étant instables (feature `ip`), on teste les préfixes.
fn est_ip_locale(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_loopback() || v4.is_private() || v4.is_link_local(),
        IpAddr::V6(v6) => {
            let bloc = v6.segments()[0];
            v6.is_loopback()
                || (bloc & 0xfe00) == 0xfc00 // ULA fc00::/7
                || (bloc & 0xffc0) == 0xfe80 // lien-local fe80::/10
        }
    }
}

/// Classe une adresse résolue en candidat de poinçonnage (SPEC §11) : une IP
/// locale/privée est un candidat LAN direct ([`CandidateKind::LocalDirect`]),
/// toute autre un candidat public direct ([`CandidateKind::PublicDirect`]).
/// L'endpoint trie ensuite les candidats par ordre d'essai avant d'émettre les
/// HELLO (cf. [`accord_transport::Endpoint::punch`]). Fonction pure (pas
/// d'E/S) — testable.
pub(crate) fn classer_candidat(addr: SocketAddr) -> Candidate {
    let kind = if est_ip_locale(addr.ip()) {
        CandidateKind::LocalDirect
    } else {
        CandidateKind::PublicDirect
    };
    Candidate { addr, kind }
}

// ---- Boucles périodiques ----

type BoxFut<'a> = Pin<Box<dyn Future<Output = ()> + Send + 'a>>;
type TickFn = for<'a> fn(&'a Runtime, &'a MaintenanceConfig) -> BoxFut<'a>;

fn salt() -> u64 {
    rand::rngs::OsRng.next_u64()
}

/// Dort `d` ou s'interrompt au signal d'arrêt ; vrai s'il faut continuer.
async fn sleep_or_stop(stop: &mut watch::Receiver<bool>, d: Duration) -> bool {
    if *stop.borrow() {
        return false;
    }
    tokio::select! {
        _ = tokio::time::sleep(d) => true,
        _ = stop.changed() => false,
    }
}

fn spawn_periodic(rt: Arc<Runtime>, interval: Duration, tick: TickFn) {
    let mut stop = rt.stop_signal();
    tokio::spawn(async move {
        let jitter = rt.maintenance_config().jitter;
        if !sleep_or_stop(&mut stop, jittered(startup_delay(interval), jitter, salt())).await {
            return;
        }
        loop {
            tick(&rt, rt.maintenance_config()).await;
            if !sleep_or_stop(&mut stop, jittered(interval, jitter, salt())).await {
                return;
            }
        }
    });
}

/// Lance toutes les boucles de maintenance (une tâche tokio par concern).
pub(crate) fn spawn_loops(rt: &Arc<Runtime>) {
    let cfg = rt.maintenance_config();
    spawn_periodic(Arc::clone(rt), cfg.identity_republish, |r, c| {
        Box::pin(identity_tick(r, c))
    });
    spawn_periodic(Arc::clone(rt), cfg.presence_publish, |r, c| {
        Box::pin(presence_publish_tick(r, c))
    });
    spawn_periodic(Arc::clone(rt), cfg.presence_resolve, |r, c| {
        Box::pin(presence_resolve_tick(r, c))
    });
    spawn_periodic(Arc::clone(rt), cfg.outbox_flush, |r, c| {
        Box::pin(outbox_tick(r, c))
    });
    spawn_periodic(Arc::clone(rt), cfg.mailbox_poll, |r, c| {
        Box::pin(mailbox_tick(r, c))
    });
    spawn_periodic(Arc::clone(rt), cfg.group_sync, |r, c| {
        Box::pin(group_sync_tick(r, c))
    });
    // Ré-annonce périodique du profil (D-027) : même cadence que la
    // republication d'identité, pas de bouton de réglage supplémentaire.
    spawn_periodic(Arc::clone(rt), cfg.identity_republish, |r, c| {
        Box::pin(profile_tick(r, c))
    });
    // Reconnexion périodique aux pairs d'amorçage (B2, backoff par pair).
    spawn_periodic(Arc::clone(rt), cfg.bootstrap_reconnect, |r, c| {
        Box::pin(bootstrap_reconnect_tick(r, c))
    });
}

/// Reconnecte les pairs d'amorçage injoignables (backoff par pair géré par le
/// runtime) et rafraîchit l'événement `event.network` si les compteurs ont
/// changé.
async fn bootstrap_reconnect_tick(rt: &Runtime, cfg: &MaintenanceConfig) {
    rt.reconnect_bootstrap(cfg.bootstrap_reconnect).await;
}

/// Republie le record d'identité (re-signé, horodatage frais) et purge les
/// records locaux expirés.
async fn identity_tick(rt: &Runtime, _cfg: &MaintenanceConfig) {
    let now = now_ms();
    let record = rt.node().identity_record();
    let replicas = rt.dht().put(rt.dht_rpc(), record, now).await;
    let expires = rt.dht().expire_records(now);
    tracing::debug!(replicas, expires, "identité : record republié");
}

/// Nombre de pairs sollicités pour une observation d'adresse (SPEC §11.1 :
/// « réponses croisées de 3 nœuds » pour distinguer cone de symétrique).
const OBSERVE_PEERS: usize = 3;

/// Publie la présence (adresses du nœud) après avoir sollicité, si nécessaire,
/// plusieurs observations d'adresse pour classer le NAT local (SPEC §11.1).
async fn presence_publish_tick(rt: &Runtime, _cfg: &MaintenanceConfig) {
    let now = now_ms();
    // Periodic presence announcement to friends (rich status + custom text,
    // invisible broadcast as offline): ephemeral, never queued — unreachable
    // friends simply miss it until the next pass.
    if let Err(e) = rt.node().broadcast_presence(true) {
        tracing::debug!(erreur = %e, "présence : annonce aux amis impossible");
    }
    // SPEC §11.1 : interroge PLUSIEURS pairs (≥3 si connus) pour recouper les
    // adresses publiques observées ; les réponses `ObservedAddr` sont agrégées
    // par le runtime et déduisent cone (consensus) vs symétrique (divergence).
    request_observations(rt).await;
    let addrs = rt.presence_addrs();
    if addrs.is_empty() {
        tracing::debug!("présence : aucune adresse publiable");
        return;
    }
    let record = rt.node().presence_record(&addrs);
    let replicas = rt.dht().put(rt.dht_rpc(), record, now).await;
    tracing::debug!(replicas, "présence : record publié");
}

/// Sollicite une observation d'adresse auprès de plusieurs pairs connus tant
/// qu'un consensus (NAT cone) n'est pas établi (SPEC §11.1). Sans effet si aucun
/// pair n'est connu ; borné à [`OBSERVE_PEERS`] messages de contrôle par passe.
async fn request_observations(rt: &Runtime) {
    // Consensus déjà atteint : inutile de re-solliciter à chaque passe.
    if rt.nat_kind() == NatKind::Cone {
        return;
    }
    for peer in rt.known_peer_addrs(OBSERVE_PEERS) {
        let req = ChannelMsg::Control(ControlMsg::ObserveAddrReq);
        if rt.endpoint().send(peer, &req).await.is_err() {
            tracing::debug!("présence : demande d'observation échouée");
        }
    }
}

/// Résout la présence d'une fenêtre d'amis, alimente le carnet d'adresses et
/// déclenche un poinçonnage NAT vers chaque ami résolu (SPEC §11).
///
/// Pour chaque ami dont la présence est vérifiée :
/// - une adresse de repli est posée au carnet *sans écraser* une adresse déjà
///   connue (cf. [`Runtime::register_peer_if_absent`]) : l'outbox a une cible
///   même avant l'établissement d'une session, sans jamais dégrader une adresse
///   prouvée joignable ;
/// - toutes les adresses publiées sont classées en candidats
///   ([`classer_candidat`]) et un poinçonnage [`accord_transport::Endpoint::punch`]
///   est lancé. `punch` est idempotent (no-op si une session avec l'ami existe
///   déjà), donc le relancer à chaque passe est sûr et borné par la fenêtre
///   `contacts_per_tick`.
///
/// Chaque poinçonnage (jusqu'à 5×200 ms ≈ 1 s) est **détaché** dans une tâche
/// `tokio` sur un clone de l'`Arc<Endpoint>` : la boucle de résolution ne se
/// bloque pas derrière un poinçonnage lent lorsqu'elle en enchaîne plusieurs.
async fn presence_resolve_tick(rt: &Runtime, cfg: &MaintenanceConfig) {
    let now = now_ms();
    let friends = match rt.node().friend_pubkeys() {
        Ok(f) => f,
        Err(e) => {
            tracing::warn!(erreur = %e, "présence : liste d'amis illisible");
            return;
        }
    };
    let window = rt.presence_window(friends.len(), cfg.contacts_per_tick);
    let mut resolved = 0usize;
    let mut punched = 0usize;
    for i in window {
        let friend = friends[i];
        let Some(record) = rt.dht().get(rt.dht_rpc(), presence_key(&friend), now).await else {
            continue;
        };
        let addrs = match verify_presence_record(&friend, &record, now) {
            Ok(addrs) if !addrs.is_empty() => addrs,
            Ok(_) => continue, // record valide mais sans adresse : rien à essayer
            Err(e) => {
                tracing::debug!(erreur = %e, "présence : record rejeté");
                continue;
            }
        };
        resolved += 1;
        // Cible de repli pour l'outbox, sans écraser une adresse déjà prouvée.
        rt.register_peer_if_absent(friend, addrs[0]);
        // Poinçonnage best-effort vers TOUS les candidats classés, détaché pour
        // ne pas bloquer la résolution des amis suivants.
        let candidats: Vec<Candidate> = addrs.into_iter().map(classer_candidat).collect();
        let endpoint = rt.endpoint_arc();
        tokio::spawn(async move {
            if let Err(e) = endpoint.punch(&candidats, friend).await {
                tracing::debug!(erreur = %e, "présence : poinçonnage échoué");
            }
        });
        punched += 1;

        // Repli relais (SPEC §11.3) : si le poinçonnage n'établit pas de session
        // dans le délai imparti — NAT symétrique où le punch ne peut pas passer,
        // ou NAT cone dont le punch a échoué — on bascule sur un relais partagé.
        // Détaché et idempotent : `ensure_relay_to` ne fait rien si l'ami est
        // déjà joignable (session directe/relayée) ou si un circuit existe déjà.
        if let Some(rt_arc) = rt.arc() {
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(relay::PUNCH_FALLBACK_MS)).await;
                if !rt_arc.is_peer_live(&friend) {
                    rt_arc.ensure_relay_to(friend).await;
                }
            });
        }
    }
    tracing::debug!(
        amis = friends.len(),
        resolus = resolved,
        poinconnes = punched,
        "présence : passe de résolution"
    );
}

/// Vide l'outbox : purge des expirés, tentative directe par destinataire
/// (backoff persistant en cas d'échec), dépôt en boîte aux lettres DHT après
/// `mailbox_after_attempts` tentatives infructueuses.
async fn outbox_tick(rt: &Runtime, cfg: &MaintenanceConfig) {
    let now = now_ms();
    let node = rt.node();
    match node.outbox_purge_expired(now) {
        Ok(purged) if purged > 0 => tracing::info!(purged, "outbox : éléments expirés purgés"),
        Ok(_) => {}
        Err(e) => tracing::warn!(erreur = %e, "outbox : purge impossible"),
    }
    let due = match node.outbox_due(now, cfg.outbox_batch) {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!(erreur = %e, "outbox : lecture impossible");
            return;
        }
    };
    if due.is_empty() {
        return;
    }
    let mut by_dest: std::collections::BTreeMap<[u8; 32], Vec<accord_core::db::OutboxItem>> =
        std::collections::BTreeMap::new();
    for item in due {
        by_dest.entry(item.dest).or_default().push(item);
    }
    for (dest, items) in by_dest {
        let addr = rt.addr_of(&dest);
        let mut want_deposit = false;
        for item in items {
            let msg = match decode_core(&item.payload) {
                Ok(m) => m,
                Err(_) => {
                    tracing::warn!("outbox : charge illisible retirée");
                    let _ = node.outbox_remove(item.id);
                    continue;
                }
            };
            // Un DirectMsg n'est retiré que sur accusé applicatif (MsgAck) ;
            // les autres natures sont retirées dès l'envoi transport réussi.
            let await_ack = matches!(msg, CoreMsg::DirectMsg { .. });
            let sent = match addr {
                Some(a) => rt
                    .endpoint()
                    .send_to(a, Some(dest), &ChannelMsg::Core(msg))
                    .await
                    .is_ok(),
                None => false,
            };
            if sent && !await_ack {
                let _ = node.outbox_remove(item.id);
            } else {
                if let Err(e) = node.outbox_reschedule(item.id, now) {
                    tracing::debug!(erreur = %e, "outbox : replanification impossible");
                }
                if should_deposit(item.attempts, item.mailboxed, cfg.mailbox_after_attempts) {
                    want_deposit = true;
                }
            }
        }
        if !want_deposit {
            continue;
        }
        // D-017 : le dépôt regroupe TOUTE la file pour ce destinataire.
        match node.mailbox_deposit_records(&dest, now) {
            Ok((records, ids)) => {
                let fragments = records.len();
                let mut replicas = 0usize;
                for record in records {
                    replicas += rt.dht().put(rt.dht_rpc(), record, now).await;
                }
                for id in ids {
                    let _ = node.outbox_mark_mailboxed(id);
                }
                tracing::debug!(fragments, replicas, "outbox : dépôt hors-ligne publié");
            }
            Err(e) => tracing::warn!(erreur = %e, "outbox : dépôt hors-ligne impossible"),
        }
    }
}

/// Vide immédiatement la file d'un pair qui vient de se connecter.
pub(crate) async fn flush_peer(rt: &Runtime, peer: &[u8; 32], addr: SocketAddr) {
    let now = now_ms();
    let items = match rt.node().outbox_for(peer) {
        Ok(items) => items,
        Err(e) => {
            tracing::debug!(erreur = %e, "outbox : lecture à la connexion impossible");
            return;
        }
    };
    if items.is_empty() {
        return;
    }
    let batch = rt.maintenance_config().outbox_batch;
    let total = items.len();
    let mut pushed = 0usize;
    for item in items.into_iter().take(batch) {
        let Ok(msg) = decode_core(&item.payload) else {
            let _ = rt.node().outbox_remove(item.id);
            continue;
        };
        let await_ack = matches!(msg, CoreMsg::DirectMsg { .. });
        if rt
            .endpoint()
            .send_to(addr, Some(*peer), &ChannelMsg::Core(msg))
            .await
            .is_ok()
        {
            pushed += 1;
            if await_ack {
                let _ = rt.node().outbox_reschedule(item.id, now);
            } else {
                let _ = rt.node().outbox_remove(item.id);
            }
        } else {
            let _ = rt.node().outbox_reschedule(item.id, now);
        }
    }
    tracing::debug!(total, pushed, "outbox : vidage à la connexion d'un pair");
}

/// Relève les boîtes aux lettres DHT : pour chaque ami de la fenêtre, sonde
/// les jours {courant, veille}, ré-assemble les fragments et ingère le dépôt.
async fn mailbox_tick(rt: &Runtime, cfg: &MaintenanceConfig) {
    let now = now_ms();
    let my_node = node_id_of(&rt.node().public_key()).0;
    let friends = match rt.node().friend_pubkeys() {
        Ok(f) => f,
        Err(e) => {
            tracing::warn!(erreur = %e, "boîtes : liste d'amis illisible");
            return;
        }
    };
    let window = rt.mailbox_window(friends.len(), cfg.contacts_per_tick);
    for i in window {
        let friend = friends[i];
        let sender_node = node_id_of(&friend).0;
        for day in offline::poll_days(now) {
            let key0 = offline::mailbox_key(&my_node, day, &sender_node, 0);
            let Some(rec0) = rt.dht().get(rt.dht_rpc(), key0, now).await else {
                continue;
            };
            if rec0.publisher != friend {
                tracing::debug!("boîtes : fragment d'un publieur inattendu, ignoré");
                continue;
            }
            let total = match offline::fragment_total(&rec0.value) {
                Ok(t) if (1..=offline::MAX_FRAGMENTS).contains(&t) => t as usize,
                _ => {
                    tracing::debug!("boîtes : en-tête de fragment invalide");
                    continue;
                }
            };
            let mut values = Vec::with_capacity(total);
            values.push(rec0.value.clone());
            for frag in 1..total {
                let key = offline::mailbox_key(&my_node, day, &sender_node, frag as u32);
                match rt.dht().get(rt.dht_rpc(), key, now).await {
                    Some(rec) if rec.publisher == friend => values.push(rec.value.clone()),
                    _ => break,
                }
            }
            if values.len() != total {
                tracing::debug!("boîtes : dépôt incomplet, nouvelle tentative plus tard");
                continue;
            }
            match rt.node().open_mailbox_deposit(&sender_node, &values) {
                Ok(payloads) => {
                    let messages = payloads.len();
                    for payload in payloads {
                        match decode_core(&payload) {
                            Ok(msg) => rt.route_core(&friend, msg).await,
                            Err(_) => tracing::debug!("boîtes : message illisible ignoré"),
                        }
                    }
                    tracing::debug!(messages, "boîtes : dépôt hors-ligne relevé");
                }
                Err(e) => tracing::debug!(erreur = %e, "boîtes : dépôt rejeté"),
            }
        }
    }
}

/// Ré-annonce le profil local (pseudo, bio, hash d'avatar — D-027) aux amis
/// joignables : filet de sécurité pour les pairs ayant manqué l'annonce
/// émise au changement (l'outbox couvre déjà les amis connus hors ligne au
/// moment du changement, cette passe couvre les reconnexions silencieuses).
async fn profile_tick(rt: &Runtime, _cfg: &MaintenanceConfig) {
    let msg = match rt.node().own_profile_msg() {
        Ok(Some(msg)) => msg,
        Ok(None) => return, // Aucun pseudo défini : rien à annoncer.
        Err(e) => {
            tracing::warn!(erreur = %e, "profil : annonce illisible");
            return;
        }
    };
    let friends = match rt.node().friend_pubkeys() {
        Ok(f) => f,
        Err(e) => {
            tracing::warn!(erreur = %e, "profil : liste d'amis illisible");
            return;
        }
    };
    let mut annonces = 0usize;
    for friend in friends {
        let Some(addr) = rt.addr_of(&friend) else {
            continue;
        };
        if rt
            .endpoint()
            .send_to(addr, Some(friend), &ChannelMsg::Core(msg.clone()))
            .await
            .is_ok()
        {
            annonces += 1;
        }
    }
    if annonces > 0 {
        tracing::debug!(annonces, "profil : ré-annonce périodique");
    }
}

/// Émet une offre `GroupSync` par groupe vers les membres joignables.
/// L'anti-entropie répondante (pull) est déjà câblée à l'ingestion.
async fn group_sync_tick(rt: &Runtime, _cfg: &MaintenanceConfig) {
    let ids = match rt.node().group_ids() {
        Ok(ids) => ids,
        Err(e) => {
            tracing::warn!(erreur = %e, "anti-entropie : groupes illisibles");
            return;
        }
    };
    let me = rt.node().public_key();
    let mut offers = 0usize;
    for gid_hex in ids {
        let Some(group_id) = crate::hex::decode::<16>(&gid_hex) else {
            continue;
        };
        let (offer, members) = match (
            rt.node().group_sync_offer(&group_id),
            rt.node().group_state(&group_id),
        ) {
            (Ok(offer), Ok(state)) => (offer, state.members),
            _ => continue,
        };
        let msg = CoreMsg::GroupSync {
            group_id: offer.group_id,
            max_lamport: offer.max_lamport,
            op_count: u32::try_from(offer.op_count).unwrap_or(u32::MAX),
            digest: offer.digest,
        };
        for member in members.keys().filter(|m| **m != me) {
            let Some(addr) = rt.addr_of(member) else {
                continue;
            };
            if rt
                .endpoint()
                .send_to(addr, Some(*member), &ChannelMsg::Core(msg.clone()))
                .await
                .is_ok()
            {
                offers += 1;
            }
        }
    }
    if offers > 0 {
        tracing::debug!(offres = offers, "anti-entropie : offres GroupSync émises");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use accord_crypto::Identity;

    #[test]
    fn jitter_reste_borne_et_deterministe() {
        let base = Duration::from_secs(100);
        for salt in [0, 1, u64::MAX / 2, u64::MAX] {
            let d = jittered(base, 0.2, salt);
            assert!(d >= Duration::from_secs(90), "trop court : {d:?}");
            assert!(d <= Duration::from_secs(110), "trop long : {d:?}");
            assert_eq!(d, jittered(base, 0.2, salt), "non déterministe");
        }
        assert_eq!(jittered(Duration::ZERO, 0.2, 42), Duration::ZERO);
        // Jitter hors bornes : clampe sans paniquer.
        let d = jittered(base, 9.0, u64::MAX);
        assert!(d <= Duration::from_secs(150));
    }

    #[test]
    fn delai_de_demarrage_borne_par_l_intervalle() {
        assert_eq!(
            startup_delay(Duration::from_millis(200)),
            Duration::from_millis(200)
        );
        assert_eq!(startup_delay(Duration::from_secs(600)), STARTUP_DELAY);
    }

    #[test]
    fn classer_candidat_local_vs_public() {
        // IPv4 privée (RFC1918) ⇒ candidat LAN direct ; adresse préservée.
        let prive: SocketAddr = "192.168.1.10:4433".parse().unwrap();
        let cand = classer_candidat(prive);
        assert_eq!(cand.kind, CandidateKind::LocalDirect);
        assert_eq!(cand.addr, prive);
        // IPv4 privée 10/8 et lien-local 169.254/16 ⇒ LAN direct.
        assert_eq!(
            classer_candidat("10.0.0.4:4433".parse().unwrap()).kind,
            CandidateKind::LocalDirect
        );
        assert_eq!(
            classer_candidat("169.254.5.6:4433".parse().unwrap()).kind,
            CandidateKind::LocalDirect
        );
        // IPv4 publique ⇒ candidat public direct.
        assert_eq!(
            classer_candidat("93.184.216.34:4433".parse().unwrap()).kind,
            CandidateKind::PublicDirect
        );
        // IPv6 lien-local (fe80::/10) et ULA (fc00::/7) ⇒ LAN direct.
        assert_eq!(
            classer_candidat("[fe80::1]:4433".parse().unwrap()).kind,
            CandidateKind::LocalDirect
        );
        assert_eq!(
            classer_candidat("[fd12::1]:4433".parse().unwrap()).kind,
            CandidateKind::LocalDirect
        );
        // IPv6 publique (DNS Google) ⇒ public direct.
        assert_eq!(
            classer_candidat("[2001:4860:4860::8888]:4433".parse().unwrap()).kind,
            CandidateKind::PublicDirect
        );
    }

    #[test]
    fn candidats_resolus_trient_local_avant_public() {
        // Reproduit ce que construit la passe de résolution : la liste classée
        // et triée (comme dans `punch`) essaie le LAN direct avant le public.
        let addrs: Vec<SocketAddr> = vec![
            "93.184.216.34:4433".parse().unwrap(), // publique
            "192.168.1.5:4433".parse().unwrap(),   // privée (même LAN)
        ];
        let mut cands: Vec<Candidate> = addrs.into_iter().map(classer_candidat).collect();
        cands.sort_by_key(|c| c.kind);
        assert_eq!(cands[0].kind, CandidateKind::LocalDirect);
        assert_eq!(cands[0].addr, "192.168.1.5:4433".parse().unwrap());
        assert_eq!(cands[1].kind, CandidateKind::PublicDirect);
    }

    #[test]
    fn rotation_couvre_tout_sans_famine() {
        let (w, c) = rotation(5, 0, 2);
        assert_eq!(w, vec![0, 1]);
        let (w, c) = rotation(5, c, 2);
        assert_eq!(w, vec![2, 3]);
        let (w, c) = rotation(5, c, 2);
        assert_eq!(w, vec![4, 0]);
        assert_eq!(c, 1);
        assert_eq!(rotation(0, 3, 2).0, Vec::<usize>::new());
        assert_eq!(rotation(3, 0, 10).0, vec![0, 1, 2]);
    }

    #[test]
    fn eligibilite_file_hors_ligne() {
        assert!(is_queueable_offline(&CoreMsg::MsgAck { msg_id: [0; 16] }));
        assert!(is_queueable_offline(&CoreMsg::FriendResponse {
            accepted: true
        }));
        // Text bodies are queued; typing (5) and read receipts (6) are not.
        let dm = |kind: u8| CoreMsg::DirectMsg {
            msg_id: [0; 16],
            lamport: 1,
            sent_ms: 1,
            kind,
            body: vec![],
        };
        assert!(is_queueable_offline(&dm(0)));
        assert!(!is_queueable_offline(&dm(5)));
        assert!(!is_queueable_offline(&dm(6)));
        // Friendship removal is best-effort: never queued.
        assert!(!is_queueable_offline(&CoreMsg::FriendRemove));
        assert!(is_queueable_offline(&CoreMsg::Profile {
            display_name: "Anna".into(),
            bio: String::new(),
            avatar: None,
            banner: None,
        }));
        assert!(!is_queueable_offline(&CoreMsg::Presence {
            status: 0,
            custom: None
        }));
        assert!(!is_queueable_offline(&CoreMsg::GroupSync {
            group_id: [0; 16],
            max_lamport: 0,
            op_count: 0,
            digest: [0; 32],
        }));
        assert!(!is_queueable_offline(&CoreMsg::GroupSyncPull {
            group_id: [0; 16],
            since_lamport: 0,
        }));
    }

    #[test]
    fn seuil_de_depot_en_boite() {
        assert!(!should_deposit(0, false, 2));
        assert!(!should_deposit(1, false, 2));
        assert!(should_deposit(2, false, 2));
        assert!(!should_deposit(5, true, 2), "déjà déposé");
        assert!(should_deposit(0, false, 0), "dépôt immédiat configurable");
    }

    #[test]
    fn backoff_reconnexion_croissant_et_borne() {
        let base = Duration::from_secs(60);
        assert_eq!(reconnect_backoff(base, 0), base);
        assert_eq!(reconnect_backoff(base, 1), Duration::from_secs(120));
        assert_eq!(reconnect_backoff(base, 3), Duration::from_secs(480));
        // Plafonné à 2^5 = 32 × base, sans débordement au-delà.
        let plafond = Duration::from_secs(60 * 32);
        assert_eq!(reconnect_backoff(base, 5), plafond);
        assert_eq!(reconnect_backoff(base, 99), plafond);
    }

    #[test]
    fn coremsg_roundtrip_outbox() {
        let msg = CoreMsg::MsgAck { msg_id: [7; 16] };
        let bytes = encode_core(&msg);
        assert_eq!(decode_core(&bytes).unwrap(), msg);
        assert!(decode_core(&[0xFF, 0xFF]).is_err());
    }

    #[test]
    fn presence_valeur_roundtrip() {
        let addrs: Vec<SocketAddr> = vec![
            "127.0.0.1:4433".parse().unwrap(),
            "[2001:db8::1]:9000".parse().unwrap(),
        ];
        let bytes = encode_presence_value(&addrs);
        assert_eq!(decode_presence_value(&bytes).unwrap(), addrs);
        // Version inconnue et résidu : refusés.
        let mut bad = bytes.clone();
        bad[0] = 99;
        assert!(decode_presence_value(&bad).is_err());
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(decode_presence_value(&trailing).is_err());
        // Le nombre d'adresses publiées est borné.
        let many: Vec<SocketAddr> = (0..10)
            .map(|i| format!("127.0.0.1:{}", 1000 + i).parse().unwrap())
            .collect();
        assert_eq!(
            decode_presence_value(&encode_presence_value(&many))
                .unwrap()
                .len(),
            MAX_NODE_ADDRS
        );
    }

    fn presence_record_of(id: &Identity, addrs: &[SocketAddr], now: u64) -> DhtRecord {
        let mut record = DhtRecord {
            key: presence_key(&id.public_key()),
            kind: RecordKind::Presence,
            value: encode_presence_value(addrs),
            publisher: id.public_key(),
            timestamp_ms: now,
            expiry_s: PRESENCE_EXPIRY_S,
            sig: [0u8; 64],
        };
        record.sig = id.sign(&record.signable_bytes());
        record
    }

    #[test]
    fn presence_verifiee_de_bout_en_bout() {
        let alice = Identity::generate_with_pow_bits(1);
        let mallory = Identity::generate_with_pow_bits(1);
        let addr: SocketAddr = "10.0.0.1:4433".parse().unwrap();
        let now = 10_000_000;
        let record = presence_record_of(&alice, &[addr], now);
        assert_eq!(
            verify_presence_record(&alice.public_key(), &record, now).unwrap(),
            vec![addr]
        );

        // Publieur inattendu (record d'Alice vérifié contre Mallory).
        assert!(verify_presence_record(&mallory.public_key(), &record, now).is_err());

        // Record re-signé par Mallory sous la clé d'Alice : publieur ≠ attendu.
        let mut forged = record.clone();
        forged.publisher = mallory.public_key();
        forged.sig = mallory.sign(&forged.signable_bytes());
        assert!(verify_presence_record(&alice.public_key(), &forged, now).is_err());

        // Signature altérée.
        let mut bad_sig = record.clone();
        bad_sig.sig[0] ^= 1;
        assert!(verify_presence_record(&alice.public_key(), &bad_sig, now).is_err());

        // Record périmé (au-delà de l'expiration + tolérance d'horloge).
        let stale = now + u64::from(PRESENCE_EXPIRY_S) * 1000 + CLOCK_SKEW_MS + 1;
        assert!(verify_presence_record(&alice.public_key(), &record, stale).is_err());
    }
}
