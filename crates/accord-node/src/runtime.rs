//! Boucle réseau du nœud : pont DHT ↔ transport, routage des messages CORE,
//! exécution des actions sortantes.
//!
//! Le runtime relie les briques bas niveau (transport chiffré, Kademlia) à
//! l'état applicatif ([`Node`]) :
//! - les RPC DHT sortants sont corrélés par `rpc_id` sur des oneshots ;
//! - les messages CORE entrants sont ingérés puis déclenchent événements et
//!   accusés ;
//! - un carnet d'adresses appris des sessions permet la livraison directe ;
//!   à défaut, le message est mis en file hors-ligne persistante (outbox),
//!   vidée par les boucles de maintenance ([`crate::maintenance`]).

use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::Duration;

use accord_core::files::fetch;
use accord_crypto::node_id_of;
use accord_dht::{DhtRpc, KademliaNode};
use accord_proto::core_msg::CoreMsg;
use accord_proto::dht_msg::{DhtBody, DhtMessage};
use accord_proto::file_msg::FileMsg;
use accord_proto::limits::MAX_NODE_ADDRS;
use accord_proto::plaintext::ChannelMsg;
use accord_proto::types::{NodeId, NodeInfo};
use accord_transport::nat::ObservedAddrs;
use accord_transport::{Endpoint, TransportError, TransportEvent};
use rand::RngCore;
use tokio::sync::{mpsc, oneshot, watch};

use crate::error::NodeError;
use crate::maintenance::{self, MaintenanceConfig};
use crate::node::network::{NetworkControl, NetworkStatus};
use crate::node::relay::{self, NatKind};
use crate::node::Node;
use crate::node::{discovery, nat};
use crate::outbound::Outbound;
use crate::voice::VoiceHandle;

/// Délai d'attente d'une réponse RPC DHT.
const RPC_TIMEOUT: Duration = Duration::from_secs(2);

/// Tentatives d'ouverture d'un circuit relais tant que la session directe avec
/// le relais n'est pas établie (handshake en vol après `connect`).
const RELAY_OPEN_RETRIES: u32 = 5;
/// Attente entre deux tentatives d'ouverture de circuit relais.
const RELAY_OPEN_RETRY_WAIT: Duration = Duration::from_millis(200);

/// Période de la passe des transferts de fichiers.
const FILES_TICK: Duration = Duration::from_millis(250);
/// Anti-abus du service de fichiers : demandes servies par pair et par
/// seconde au plus (au-delà, silence — le pair relancera).
const FILES_REQS_PAR_S: u32 = 256;
/// Borne du suivi de débit par pair (au-delà, la table est réinitialisée).
const FILES_DEBIT_MAX_PAIRS: usize = 1024;

/// Pont [`DhtRpc`] sur l'endpoint transport : envoie un RPC dans une session
/// chiffrée et attend la réponse corrélée par `rpc_id`.
pub struct TransportDhtRpc {
    endpoint: Arc<Endpoint>,
    local_id: NodeId,
    pending: Mutex<HashMap<[u8; 20], oneshot::Sender<DhtBody>>>,
}

impl TransportDhtRpc {
    /// Crée le pont pour un endpoint donné.
    pub fn new(endpoint: Arc<Endpoint>) -> Arc<Self> {
        let local_id = endpoint.node_id();
        Arc::new(Self {
            endpoint,
            local_id,
            pending: Mutex::new(HashMap::new()),
        })
    }

    /// Corrèle une réponse DHT entrante à sa requête en attente.
    pub fn complete(&self, rpc_id: [u8; 20], body: DhtBody) {
        if let Some(tx) = self.pending.lock().expect("pending mutex").remove(&rpc_id) {
            let _ = tx.send(body);
        }
    }

    fn new_rpc_id(&self) -> [u8; 20] {
        let mut id = [0u8; 20];
        rand::rngs::OsRng.fill_bytes(&mut id);
        id
    }
}

#[async_trait::async_trait]
impl DhtRpc for TransportDhtRpc {
    async fn send_rpc(&self, to: &NodeInfo, body: DhtBody) -> Option<DhtBody> {
        let addr = to.addrs.first()?.0;
        let rpc_id = self.new_rpc_id();
        let (tx, rx) = oneshot::channel();
        self.pending
            .lock()
            .expect("pending mutex")
            .insert(rpc_id, tx);
        let msg = ChannelMsg::Dht(DhtMessage { rpc_id, body });
        if self.endpoint.send(addr, &msg).await.is_err() {
            self.pending.lock().expect("pending mutex").remove(&rpc_id);
            return None;
        }
        match tokio::time::timeout(RPC_TIMEOUT, rx).await {
            Ok(Ok(body)) => Some(body),
            _ => {
                self.pending.lock().expect("pending mutex").remove(&rpc_id);
                None
            }
        }
    }

    fn local_id(&self) -> NodeId {
        self.local_id
    }
}

/// Vrai si un corps DHT est une réponse (à corréler), non une requête.
fn is_dht_response(body: &DhtBody) -> bool {
    matches!(
        body,
        DhtBody::Pong
            | DhtBody::FoundNodes { .. }
            | DhtBody::FoundValue { .. }
            | DhtBody::StoreOk
            | DhtBody::Error { .. }
    )
}

/// Carnet d'adresses appris des sessions (clé publique → dernière adresse).
#[derive(Default)]
struct AddressBook {
    by_pubkey: HashMap<[u8; 32], SocketAddr>,
}

/// Boucle principale du runtime réseau.
pub struct Runtime {
    endpoint: Arc<Endpoint>,
    dht: Arc<KademliaNode>,
    rpc: Arc<TransportDhtRpc>,
    node: Arc<Node>,
    book: Mutex<AddressBook>,
    maintenance: MaintenanceConfig,
    /// Notre adresse publique telle qu'observée par un pair (SPEC §11) : le
    /// consensus des observations quand il existe, sinon la dernière reçue.
    /// Alimente l'assemblage des adresses de présence et le statut réseau.
    observed: Mutex<Option<SocketAddr>>,
    /// Agrégat des adresses publiques observées par PLUSIEURS pairs
    /// (SPEC §11.1) : recoupées pour distinguer un NAT cone (consensus) d'un
    /// NAT symétrique (divergence). Voir [`crate::node::relay::classify_nat`].
    observed_addrs: Mutex<ObservedAddrs>,
    /// Pairs avec lesquels une session transport est (ou a été) établie —
    /// directe ou relayée : alimenté par les événements `Connected`/`Message`,
    /// purgé sur `Disconnected`. Sert à l'idempotence du repli relais (ne pas
    /// ouvrir de circuit vers un ami déjà joignable) et à l'observabilité.
    live: Mutex<HashSet<[u8; 32]>>,
    /// Signal d'arrêt des boucles de maintenance.
    stop_tx: watch::Sender<bool>,
    /// Curseurs de rotation des passes de maintenance (anti-famine).
    presence_cursor: AtomicUsize,
    mailbox_cursor: AtomicUsize,
    /// Sous-système voix (câblé après construction, absent dans les tests
    /// sans voix) : reçoit les trames VOICE et les signalisations.
    voice: OnceLock<VoiceHandle>,
    /// Téléchargements de fichiers en cours (coordination pure, E/S ici).
    files: Mutex<fetch::Coordinator>,
    /// Fenêtres de débit du service de fichiers, par pair : `(début_ms, n)`.
    files_debit: Mutex<HashMap<[u8; 32], (u64, u32)>>,
    /// Dernier signal réseau émis (compteurs, mapping, pairs LAN) : évite de
    /// spammer `event.network` quand rien ne change.
    net_last: Mutex<Option<NetSignal>>,
    /// Backoff de reconnexion par pair d'amorçage : `(prochaine_ms, échecs)`.
    boot_backoff: Mutex<HashMap<SocketAddr, (u64, u32)>>,
    /// État du mapping de port automatique (UPnP/NAT-PMP), publié par la tâche
    /// de fond [`nat`] et lu par le statut réseau.
    nat: Arc<nat::NatShared>,
    /// État de la découverte LAN (mDNS), publié par la tâche [`discovery`] et
    /// lu par le statut réseau.
    lan: Arc<discovery::LanShared>,
    /// Référence faible sur soi-même, posée au démarrage ([`Runtime::spawn`]) :
    /// permet aux boucles de maintenance (qui n'ont qu'un `&Runtime`) de
    /// détacher une tâche possédant un `Arc<Runtime>` — typiquement le repli
    /// relais, borné et déporté hors de la boucle de résolution.
    self_ref: OnceLock<Weak<Runtime>>,
}

/// Signal compact de changement réseau : ne déclenche `event.network` que sur
/// transition réelle (les compteurs et l'état de mapping/LAN combinés).
#[derive(PartialEq, Eq, Clone, Copy)]
struct NetSignal {
    connected_peers: usize,
    dht_nodes: usize,
    nat: nat::NatSnapshot,
    nat_kind: NatKind,
    lan_peers: usize,
}

impl Runtime {
    /// Assemble le runtime.
    pub fn new(
        endpoint: Arc<Endpoint>,
        dht: Arc<KademliaNode>,
        rpc: Arc<TransportDhtRpc>,
        node: Arc<Node>,
        maintenance: MaintenanceConfig,
    ) -> Arc<Self> {
        let (stop_tx, _) = watch::channel(false);
        Arc::new(Self {
            endpoint,
            dht,
            rpc,
            node,
            book: Mutex::new(AddressBook::default()),
            maintenance,
            observed: Mutex::new(None),
            observed_addrs: Mutex::new(ObservedAddrs::new()),
            live: Mutex::new(HashSet::new()),
            stop_tx,
            presence_cursor: AtomicUsize::new(0),
            mailbox_cursor: AtomicUsize::new(0),
            voice: OnceLock::new(),
            files: Mutex::new(fetch::Coordinator::new()),
            files_debit: Mutex::new(HashMap::new()),
            net_last: Mutex::new(None),
            boot_backoff: Mutex::new(HashMap::new()),
            nat: Arc::new(nat::NatShared::default()),
            lan: Arc::new(discovery::LanShared::default()),
            self_ref: OnceLock::new(),
        })
    }

    /// Upgrade de la référence faible sur soi-même : `Some` une fois les boucles
    /// démarrées (voir [`Runtime::spawn`]), utilisé pour détacher des tâches
    /// possédant un `Arc<Runtime>` depuis un contexte `&Runtime`.
    pub(crate) fn arc(&self) -> Option<Arc<Runtime>> {
        self.self_ref.get().and_then(Weak::upgrade)
    }

    /// État partagé du mapping de port (écrit par la tâche [`nat`]).
    pub(crate) fn nat_shared(&self) -> Arc<nat::NatShared> {
        Arc::clone(&self.nat)
    }

    /// État partagé de la découverte LAN (écrit par la tâche [`discovery`]).
    pub(crate) fn lan_shared(&self) -> Arc<discovery::LanShared> {
        Arc::clone(&self.lan)
    }

    /// Câble le sous-système voix (une seule fois, après construction).
    pub(crate) fn set_voice(&self, handle: VoiceHandle) {
        let _ = self.voice.set(handle);
    }

    /// Lance les boucles d'événements transport, d'actions sortantes et de
    /// maintenance périodique (si activée).
    pub fn spawn(
        self: &Arc<Self>,
        events: mpsc::UnboundedReceiver<TransportEvent>,
        outbound: mpsc::Receiver<Outbound>,
    ) {
        // Pose la référence faible sur soi (avant les boucles) : les tâches de
        // maintenance peuvent ainsi détacher un repli relais possédant un `Arc`.
        let _ = self.self_ref.set(Arc::downgrade(self));
        let ev = Arc::clone(self);
        tokio::spawn(async move { ev.event_loop(events).await });
        let ob = Arc::clone(self);
        tokio::spawn(async move { ob.outbound_loop(outbound).await });
        let fi = Arc::clone(self);
        tokio::spawn(async move { fi.files_loop().await });
        if self.maintenance.enabled {
            maintenance::spawn_loops(self);
        }
    }

    /// Signale l'arrêt aux boucles de maintenance (idempotent).
    pub fn stop(&self) {
        let _ = self.stop_tx.send(true);
    }

    /// Enregistre l'adresse d'un pair (apprise d'une session ou résolue via
    /// la présence DHT).
    pub fn register_peer(&self, pubkey: [u8; 32], addr: SocketAddr) {
        self.remember(pubkey, addr);
    }

    /// Enregistre une adresse apprise d'un record de présence SANS écraser une
    /// adresse déjà connue (typiquement prouvée joignable par une session
    /// établie, cf. l'événement `Connected`). La présence n'est qu'un indice ;
    /// on l'utilise seulement pour donner une cible de repli à l'outbox quand le
    /// carnet est vide, sans jamais dégrader une adresse joignable connue. Le
    /// poinçonnage ([`Endpoint::punch`]) et l'événement `Connected` fixeront
    /// ensuite l'adresse réellement utilisée.
    pub(crate) fn register_peer_if_absent(&self, pubkey: [u8; 32], addr: SocketAddr) {
        self.book
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .by_pubkey
            .entry(pubkey)
            .or_insert(addr);
    }

    /// Coordonnées DHT locales (amorçage d'autres nœuds, tests).
    pub fn local_node_info(&self) -> NodeInfo {
        self.dht.local().clone()
    }

    /// Amorce la table de routage DHT avec des nœuds connus.
    pub async fn dht_bootstrap(&self, seeds: Vec<NodeInfo>) {
        self.dht
            .bootstrap(&*self.rpc, seeds, crate::node::now_ms())
            .await;
    }

    // ---- Mise en réseau réelle (B2) : amorçage, statut, événements ----

    /// Connecte un pair d'amorçage (handshake transport) puis ensemence la
    /// table de routage DHT en lui demandant ses voisins (FIND_NODE). Best
    /// effort : rend vrai si le handshake a pu être lancé.
    pub(crate) async fn seed_peer(&self, addr: SocketAddr) -> bool {
        if let Err(e) = self.endpoint.connect(addr).await {
            tracing::debug!(erreur = %e, "amorçage : connexion au pair impossible");
            return false;
        }
        // Ensemencement DHT : FIND_NODE(soi) direct vers le pair. La réponse
        // porte des NodeInfo valides (PoW vérifié à la réception) que l'on
        // apprend pour peupler la table de routage.
        let target = self.dht.node_id();
        let synth = direct_target(addr);
        if let Some(DhtBody::FoundNodes { nodes }) = self
            .rpc
            .send_rpc(&synth, DhtBody::FindNode { target: target.0 })
            .await
        {
            let now = crate::node::now_ms();
            for n in nodes {
                self.dht.observe(n, now);
            }
        }
        true
    }

    /// Ensemence tous les pairs d'amorçage persistés (au démarrage).
    pub(crate) async fn bootstrap_all(&self) {
        for addr in self.node.bootstrap_peers().unwrap_or_default() {
            self.seed_peer(addr).await;
        }
        self.emit_network_if_changed();
    }

    /// Reconnecte les pairs d'amorçage injoignables, avec backoff par pair
    /// (appelée périodiquement par la maintenance).
    pub(crate) async fn reconnect_bootstrap(&self, base: Duration) {
        let now = crate::node::now_ms();
        let peers = self.node.bootstrap_peers().unwrap_or_default();
        // Oublie le backoff des pairs retirés de la configuration.
        self.boot_backoff
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|addr, _| peers.contains(addr));
        for addr in peers {
            if self.is_peer_connected(&addr) {
                self.boot_backoff
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .remove(&addr);
                continue;
            }
            let due = {
                let mut bo = self.boot_backoff.lock().unwrap_or_else(|e| e.into_inner());
                let (next, _) = bo.entry(addr).or_insert((0, 0));
                *next <= now
            };
            if !due {
                continue;
            }
            let ok = self.seed_peer(addr).await;
            let connected = ok && self.is_peer_connected(&addr);
            let mut bo = self.boot_backoff.lock().unwrap_or_else(|e| e.into_inner());
            if connected {
                bo.remove(&addr);
            } else {
                let entry = bo.entry(addr).or_insert((0, 0));
                let fails = entry.1.saturating_add(1);
                let delay = maintenance::reconnect_backoff(base, fails);
                *entry = (now.saturating_add(delay.as_millis() as u64), fails);
            }
        }
        self.emit_network_if_changed();
    }

    /// Photographie de l'état réseau (méthode `network.status`).
    pub(crate) fn network_status(&self) -> NetworkStatus {
        let port = self.endpoint.local_addr().port();
        let observed = *self.observed.lock().unwrap_or_else(|e| e.into_inner());
        let nat = self.nat.snapshot();
        NetworkStatus {
            p2p_port: port,
            local_addrs: local_addrs(port, observed),
            bootstrap: self
                .node
                .bootstrap_peers()
                .unwrap_or_default()
                .iter()
                .map(SocketAddr::to_string)
                .collect(),
            connected_peers: self.book_len(),
            dht_nodes: self.dht.peer_count(),
            external_addr: nat.external.map(|a| a.to_string()),
            port_mapping: nat.method,
            lan_peers: self.lan.count(),
            nat_kind: self.nat_kind(),
        }
    }

    /// Signal compact de l'état réseau courant (comparé pour n'émettre qu'aux
    /// transitions, sans recalculer les adresses locales à chaque passe).
    fn net_signal(&self) -> NetSignal {
        NetSignal {
            connected_peers: self.book_len(),
            dht_nodes: self.dht.peer_count(),
            nat: self.nat.snapshot(),
            nat_kind: self.nat_kind(),
            lan_peers: self.lan.count(),
        }
    }

    /// Nature du NAT local déduite des observations d'adresse agrégées
    /// (SPEC §11.1). `Unknown` tant qu'un consensus ou une divergence n'a pas
    /// été observé par plusieurs pairs.
    pub(crate) fn nat_kind(&self) -> NatKind {
        relay::classify_nat(
            &self
                .observed_addrs
                .lock()
                .unwrap_or_else(|e| e.into_inner()),
        )
    }

    /// Nombre de pairs dont une session a été apprise (carnet d'adresses).
    fn book_len(&self) -> usize {
        self.book
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .by_pubkey
            .len()
    }

    /// Vrai si une session est apprise vers cette adresse (carnet d'adresses).
    fn is_peer_connected(&self, addr: &SocketAddr) -> bool {
        self.book
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .by_pubkey
            .values()
            .any(|a| a == addr)
    }

    /// Émet `event.network` uniquement si le signal réseau a changé (compteurs,
    /// mapping de port ou pairs LAN) : pas de spam, la maintenance et les tâches
    /// NAT/mDNS appellent ceci à chaque évolution. Le statut complet n'est
    /// recalculé (adresses locales incluses) que sur transition réelle.
    pub(crate) fn emit_network_if_changed(&self) {
        let current = self.net_signal();
        let changed = {
            let mut last = self.net_last.lock().unwrap_or_else(|e| e.into_inner());
            if *last == Some(current) {
                false
            } else {
                *last = Some(current);
                true
            }
        };
        if changed {
            let status = self.network_status();
            self.node.emit_network_status(&status);
        }
    }

    fn remember(&self, pubkey: [u8; 32], addr: SocketAddr) {
        self.book
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .by_pubkey
            .insert(pubkey, addr);
    }

    pub(crate) fn addr_of(&self, pubkey: &[u8; 32]) -> Option<SocketAddr> {
        self.book
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .by_pubkey
            .get(pubkey)
            .copied()
    }

    // ---- Suivi des sessions vivantes (repli relais idempotent) ----

    /// Note un pair comme joignable (session établie ou message reçu).
    fn mark_live(&self, pubkey: [u8; 32]) {
        self.live
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(pubkey);
    }

    /// Purge du suivi les pairs dont la session vers `addr` s'est fermée. La
    /// correspondance `addr → clé` est retrouvée par le carnet d'adresses ; une
    /// entrée résiduelle (adresse de relais partagée) est sans gravité (le repli
    /// resterait simplement inhibé une passe de plus — best-effort).
    fn forget_live(&self, addr: SocketAddr) {
        let keys: Vec<[u8; 32]> = {
            let book = self.book.lock().unwrap_or_else(|e| e.into_inner());
            book.by_pubkey
                .iter()
                .filter(|(_, a)| **a == addr)
                .map(|(k, _)| *k)
                .collect()
        };
        let mut live = self.live.lock().unwrap_or_else(|e| e.into_inner());
        for k in keys {
            live.remove(&k);
        }
    }

    /// Vrai si une session (directe ou relayée) est active avec ce pair.
    pub(crate) fn is_peer_live(&self, pubkey: &[u8; 32]) -> bool {
        self.live
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(pubkey)
    }

    /// Enregistre une observation d'adresse publique (SPEC §11.1) et met à jour
    /// l'adresse retenue : le consensus s'il existe, sinon la dernière reçue.
    fn observe_addr(&self, observed: SocketAddr) {
        let consensus = {
            let mut agg = self
                .observed_addrs
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            agg.observe(observed);
            agg.consensus()
        };
        *self.observed.lock().unwrap_or_else(|e| e.into_inner()) =
            Some(consensus.unwrap_or(observed));
        // La nature du NAT a pu changer (nouvelle observation) : rafraîchit le
        // statut réseau si le signal a évolué.
        self.emit_network_if_changed();
    }

    /// Jusqu'à `count` adresses distinctes de pairs connus (carnet d'adresses),
    /// pour solliciter plusieurs observations d'adresse (SPEC §11.1).
    pub(crate) fn known_peer_addrs(&self, count: usize) -> Vec<SocketAddr> {
        let mut out: Vec<SocketAddr> = Vec::new();
        for addr in self
            .book
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .by_pubkey
            .values()
        {
            if out.len() >= count {
                break;
            }
            if !out.contains(addr) {
                out.push(*addr);
            }
        }
        out
    }

    // ---- Repli relais (SPEC §11.3) ----

    /// Sélectionne, de façon DÉTERMINISTE, les relais candidats pour joindre
    /// `friend` par un circuit partagé : calcule la clé de paire
    /// `pair_key(moi, ami)` et filtre les [`relay::RELAY_SELECT_K`] nœuds les
    /// plus proches (distance XOR) de cette clé dans la table de routage sur le
    /// drapeau relais. Les DEUX amis calculant la MÊME clé et lisant une table
    /// cohérente convergent vers le même relais (soi-même et l'ami exclus).
    pub(crate) fn select_relay_for(&self, friend: &[u8; 32]) -> Vec<NodeInfo> {
        let me = self.node.public_key();
        let my_node = node_id_of(&me);
        let friend_node = node_id_of(friend);
        let key = relay::pair_key(&my_node, &friend_node);
        let candidates = self.dht.closest_local(&NodeId(key), relay::RELAY_SELECT_K);
        relay::select_relays(candidates, &[me, *friend])
    }

    /// Bascule sur le relais pour joindre `friend` (SPEC §11.3), best-effort et
    /// idempotent : ne fait rien si une session (directe ou relayée) existe déjà
    /// ou si un circuit vers l'ami est déjà ouvert. Sinon, essaie les relais
    /// candidats dans l'ordre déterministe : session directe avec le relais
    /// (établie au besoin), ouverture de circuit, puis handshake A↔B tunnelé
    /// (liaison d'identité D-037 garantie côté transport). À déclencher détaché
    /// (`tokio::spawn`) pour ne pas bloquer la boucle appelante ; aucun verrou
    /// n'est tenu pendant les `await` réseau.
    pub(crate) async fn ensure_relay_to(&self, friend: [u8; 32]) {
        let friend_node = node_id_of(&friend);
        // Idempotence : déjà joignable ou circuit déjà ouvert ⇒ rien à faire.
        if self.is_peer_live(&friend) || self.endpoint.circuit_for_peer(friend_node).is_some() {
            return;
        }
        for relay_info in self.select_relay_for(&friend) {
            let Some(relay_addr) = relay_info.addrs.first().map(|a| a.0) else {
                continue;
            };
            let relay_node = relay_info.node_id;
            // Session DIRECTE avec le relais requise avant d'ouvrir un circuit :
            // `connect` est idempotent (no-op si la session existe déjà).
            if let Err(e) = self.endpoint.connect(relay_addr).await {
                tracing::debug!(erreur = %e, "repli : connexion au relais impossible");
                continue;
            }
            if self.open_and_tunnel(relay_addr, relay_node, friend).await {
                tracing::debug!("repli : circuit relais ouvert vers l'ami");
                return;
            }
        }
    }

    /// Ouvre le circuit relais puis initie le handshake tunnelé A↔B. Réessaie
    /// l'ouverture tant que la session directe avec le relais n'est pas encore
    /// établie ([`TransportError::UnknownPeer`], handshake en vol). Rend vrai si
    /// le handshake tunnelé a pu être lancé.
    async fn open_and_tunnel(
        &self,
        relay_addr: SocketAddr,
        relay_node: NodeId,
        friend: [u8; 32],
    ) -> bool {
        for attempt in 0..RELAY_OPEN_RETRIES {
            match self
                .endpoint
                .open_relay_circuit(relay_addr, relay_node, friend)
                .await
            {
                Ok(circuit) => {
                    if let Err(e) = self.endpoint.connect_via_relay(circuit, friend).await {
                        tracing::debug!(erreur = %e, "repli : handshake tunnelé impossible");
                        return false;
                    }
                    return true;
                }
                // Session avec le relais pas encore prête : laisse le handshake
                // aboutir puis réessaie (borné).
                Err(TransportError::UnknownPeer) if attempt + 1 < RELAY_OPEN_RETRIES => {
                    tokio::time::sleep(RELAY_OPEN_RETRY_WAIT).await;
                }
                Err(e) => {
                    tracing::debug!(erreur = %e, "repli : ouverture de circuit refusée");
                    return false;
                }
            }
        }
        false
    }

    // ---- Accès internes pour les boucles de maintenance ----

    pub(crate) fn node(&self) -> &Node {
        &self.node
    }

    pub(crate) fn dht(&self) -> &KademliaNode {
        &self.dht
    }

    pub(crate) fn dht_rpc(&self) -> &TransportDhtRpc {
        &self.rpc
    }

    pub(crate) fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    /// Endpoint transport partagé (clonable) : permet de déporter un
    /// poinçonnage lent dans une tâche détachée sans emprunter le runtime.
    pub(crate) fn endpoint_arc(&self) -> Arc<Endpoint> {
        Arc::clone(&self.endpoint)
    }

    pub(crate) fn maintenance_config(&self) -> &MaintenanceConfig {
        &self.maintenance
    }

    pub(crate) fn stop_signal(&self) -> watch::Receiver<bool> {
        self.stop_tx.subscribe()
    }

    /// Adresses publiables dans le record de présence (SPEC §11) : l'adresse
    /// publique observée par un pair d'abord (priorité — meilleure joignabilité
    /// inter-réseaux), puis TOUTES les IP des interfaces locales au port P2P.
    /// Publier aussi les IP locales permet à deux amis sur des réseaux
    /// différents comme sur le même LAN de disposer de candidats à poinçonner
    /// (le pair classe et essaie chaque candidat, cf. [`Endpoint::punch`]).
    /// Loopback et adresses non spécifiées exclues, doublons dédupliqués,
    /// nombre borné par [`MAX_NODE_ADDRS`] en gardant l'observée publique en
    /// tête.
    pub(crate) fn presence_addrs(&self) -> Vec<SocketAddr> {
        let observed = *self.observed.lock().unwrap_or_else(|e| e.into_inner());
        let port = self.endpoint.local_addr().port();
        assemble_presence_addrs(observed, &discover_local_ips(), port)
    }

    /// Fenêtre de rotation de la passe de résolution de présence.
    pub(crate) fn presence_window(&self, len: usize, max: usize) -> Vec<usize> {
        let cursor = self.presence_cursor.load(Ordering::Relaxed);
        let (window, next) = maintenance::rotation(len, cursor, max);
        self.presence_cursor.store(next, Ordering::Relaxed);
        window
    }

    /// Fenêtre de rotation de la passe de relève des boîtes aux lettres.
    pub(crate) fn mailbox_window(&self, len: usize, max: usize) -> Vec<usize> {
        let cursor = self.mailbox_cursor.load(Ordering::Relaxed);
        let (window, next) = maintenance::rotation(len, cursor, max);
        self.mailbox_cursor.store(next, Ordering::Relaxed);
        window
    }

    async fn event_loop(self: Arc<Self>, mut events: mpsc::UnboundedReceiver<TransportEvent>) {
        while let Some(event) = events.recv().await {
            match event {
                TransportEvent::Connected {
                    addr, static_pub, ..
                } => {
                    self.remember(static_pub, addr);
                    self.mark_live(static_pub);
                    // Pair joignable : pousse immédiatement sa file d'attente.
                    maintenance::flush_peer(&self, &static_pub, addr).await;
                }
                TransportEvent::Message {
                    addr,
                    static_pub,
                    msg,
                    ..
                } => {
                    self.remember(static_pub, addr);
                    self.mark_live(static_pub);
                    self.route(addr, static_pub, *msg).await;
                }
                TransportEvent::ObservedAddr { observed } => {
                    self.observe_addr(observed);
                }
                TransportEvent::Disconnected { addr } => {
                    self.forget_live(addr);
                }
                _ => {}
            }
        }
    }

    async fn outbound_loop(self: Arc<Self>, mut outbound: mpsc::Receiver<Outbound>) {
        while let Some(action) = outbound.recv().await {
            match action {
                Outbound::Core { to, msg } => self.deliver_core(&to, *msg).await,
                Outbound::GroupOp { op } => {
                    // Diffuse l'op à tous les membres connus joignables.
                    let group_id = op.group_id;
                    let msg = CoreMsg::GroupOpMsg { op: *op };
                    if let Ok(state) = self.node.group_state(&group_id) {
                        for member in state.members.keys() {
                            if *member != self.node.public_key() {
                                self.deliver_core(member, msg.clone()).await;
                            }
                        }
                    }
                }
                Outbound::GroupCast { group_id, msg } => {
                    // Diffuse un message CORE à tous les membres connus.
                    if let Ok(state) = self.node.group_state(&group_id) {
                        for member in state.members.keys() {
                            if *member != self.node.public_key() {
                                self.deliver_core(member, (*msg).clone()).await;
                            }
                        }
                    }
                }
                Outbound::DhtPublish { record } => {
                    let now = crate::node::now_ms();
                    self.dht.put(&*self.rpc, *record, now).await;
                }
            }
        }
    }

    /// Livre un `CoreMsg` à un pair : session directe si l'adresse est connue,
    /// sinon mise en file hors-ligne persistante (vidée par la maintenance :
    /// renvoi direct avec backoff puis dépôt en boîte aux lettres DHT).
    async fn deliver_core(&self, to_pubkey: &[u8; 32], msg: CoreMsg) {
        let channel_msg = ChannelMsg::Core(msg);
        if let Some(addr) = self.addr_of(to_pubkey) {
            // Livraison CORE liée à l'identité du destinataire : la session (ou
            // le handshake) doit émaner de `to_pubkey`, sinon l'envoi échoue
            // (liaison d'identité, SPEC §2.2 — déjoue un MITM on-path) et le
            // message bascule sur le relais ou en file hors-ligne plutôt que
            // d'être scellé sous une session usurpée.
            if self
                .endpoint
                .send_to(addr, Some(*to_pubkey), &channel_msg)
                .await
                .is_ok()
            {
                return;
            }
        }
        // Repli relais (SPEC §11.3) : un circuit vers ce pair existe (session
        // A↔B tunnelée). L'envoi direct a échoué (ou le carnet pointe sur
        // l'adresse du relais, d'où une liaison d'identité rejetée) : on ré-
        // enveloppe le message dans le circuit. Le pair reste identifié par sa
        // clé (liaison D-037 assurée par le handshake tunnelé).
        if let Some(circuit) = self.endpoint.circuit_for_peer(node_id_of(to_pubkey)) {
            if self
                .endpoint
                .send_via_relay(circuit, &channel_msg)
                .await
                .is_ok()
            {
                return;
            }
        }
        // Sans adresse ou échec d'envoi : file hors-ligne si la nature du
        // message le justifie (les messages éphémères sont perdus sans effet).
        let ChannelMsg::Core(msg) = channel_msg else {
            return;
        };
        if !maintenance::is_queueable_offline(&msg) {
            return;
        }
        match self.node.outbox_enqueue(to_pubkey, &msg) {
            Ok(()) => tracing::debug!("core : destinataire injoignable, mis en file hors-ligne"),
            Err(e) => tracing::warn!(erreur = %e, "core : mise en file hors-ligne impossible"),
        }
    }

    async fn route(&self, addr: SocketAddr, static_pub: [u8; 32], msg: ChannelMsg) {
        match msg {
            ChannelMsg::Dht(dht_msg) => self.route_dht(addr, static_pub, dht_msg).await,
            ChannelMsg::Core(core_msg) => self.route_core(&static_pub, core_msg).await,
            // Trames et pings voix : routés vers le moteur voix (D-025).
            ChannelMsg::Voice(voice_msg) => {
                if let Some(voice) = self.voice.get() {
                    voice.peer_frame(static_pub, voice_msg);
                }
            }
            // Fichiers : service des pairs et téléchargements en cours.
            ChannelMsg::File(file_msg) => self.route_file(static_pub, file_msg).await,
            // Relais : hors du périmètre de ce routeur (sous-système dédié).
            _ => {}
        }
    }

    async fn route_dht(&self, addr: SocketAddr, static_pub: [u8; 32], dht_msg: DhtMessage) {
        if is_dht_response(&dht_msg.body) {
            self.rpc.complete(dht_msg.rpc_id, dht_msg.body);
            return;
        }
        // Requête : construit le NodeInfo de l'émetteur et répond.
        let from = NodeInfo {
            node_id: accord_crypto::node_id_of(&static_pub),
            static_pub,
            pow_nonce: 0,
            flags: 0,
            addrs: vec![accord_proto::types::WireAddr(addr)],
        };
        let now = crate::node::now_ms();
        if let Some(response) = self.dht.handle_rpc(&from, dht_msg, now) {
            let _ = self.endpoint.send(addr, &ChannelMsg::Dht(response)).await;
        }
    }

    pub(crate) async fn route_core(&self, static_pub: &[u8; 32], core_msg: CoreMsg) {
        // Signalisation vocale : éphémère, routée vers le moteur voix sans
        // passer par la base (l'adhésion au groupe y est re-validée).
        if let CoreMsg::VoiceSignal {
            group_id,
            channel_id,
            action,
            media_kinds,
            mute,
        } = &core_msg
        {
            if let Some(voice) = self.voice.get() {
                voice.peer_signal(
                    *static_pub,
                    *group_id,
                    *channel_id,
                    *action,
                    *media_kinds,
                    *mute,
                );
            }
            return;
        }
        // Un accusé applicatif solde l'élément d'outbox correspondant.
        if let CoreMsg::MsgAck { msg_id } = &core_msg {
            if let Err(e) = self.node.outbox_ack(static_pub, msg_id) {
                tracing::debug!(erreur = %e, "core : purge d'outbox sur accusé impossible");
            }
        }
        match self.node.ingest_core(static_pub, core_msg) {
            Ok(replies) => {
                for reply in replies {
                    self.deliver_core(static_pub, reply).await;
                }
            }
            Err(e) => tracing::debug!(erreur = %e, "core: ingestion refusée"),
        }
    }

    // ---- Fichiers (canal FILE) : service des pairs et téléchargements ----

    /// Verrou du coordinateur de téléchargements.
    fn files_lock(&self) -> std::sync::MutexGuard<'_, fetch::Coordinator> {
        self.files.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Pairs joignables candidats au sondage de manifest (bornés).
    fn known_peers(&self, cap: usize) -> Vec<[u8; 32]> {
        self.book
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .by_pubkey
            .keys()
            .take(cap)
            .copied()
            .collect()
    }

    /// Anti-abus du service de fichiers : au plus [`FILES_REQS_PAR_S`]
    /// demandes servies par pair et par seconde (fenêtre fixe), silence
    /// au-delà (le pair relancera après son délai de relance).
    fn files_debit_ok(&self, peer: &[u8; 32]) -> bool {
        let now = crate::node::now_ms();
        let mut debit = self.files_debit.lock().unwrap_or_else(|e| e.into_inner());
        if debit.len() > FILES_DEBIT_MAX_PAIRS {
            debit.clear();
        }
        let fenetre = debit.entry(*peer).or_insert((now, 0));
        if now.saturating_sub(fenetre.0) >= 1_000 {
            *fenetre = (now, 0);
        }
        fenetre.1 += 1;
        fenetre.1 <= FILES_REQS_PAR_S
    }

    /// Envoie un message FILE à un pair dont l'adresse est connue (sinon
    /// silence : la relance périodique réessaiera).
    async fn send_file(&self, to: &[u8; 32], msg: FileMsg) {
        let Some(addr) = self.addr_of(to) else {
            return;
        };
        if let Err(e) = self.endpoint.send(addr, &ChannelMsg::File(msg)).await {
            tracing::debug!(erreur = %e, "fichiers : envoi impossible");
        }
    }

    /// Exécute les demandes décidées par le coordinateur.
    async fn send_file_actions(&self, actions: Vec<fetch::Action>) {
        for action in actions {
            let (to, msg) = match action {
                fetch::Action::GetManifest { to, root } => (to, FileMsg::GetManifest { root }),
                fetch::Action::GetBlock { to, root, index } => {
                    (to, FileMsg::GetBlock { root, index })
                }
            };
            self.send_file(&to, msg).await;
        }
    }

    /// Route un message FILE : requêtes servies (manifest, blocs — bornées
    /// par pair) et réponses de nos sources (manifest, blocs, refus, `Have`).
    async fn route_file(&self, static_pub: [u8; 32], msg: FileMsg) {
        match msg {
            // ---- Service des pairs ----
            FileMsg::GetManifest { root } => {
                if !self.files_debit_ok(&static_pub) {
                    return;
                }
                let reply = match self.node.files_serve_manifest(&root) {
                    Ok(Some(manifest)) => FileMsg::ManifestMsg { manifest },
                    _ => FileMsg::NotFound {
                        root,
                        index: fetch::INDEX_MANIFESTE,
                    },
                };
                self.send_file(&static_pub, reply).await;
            }
            FileMsg::GetBlock { root, index } => {
                if !self.files_debit_ok(&static_pub) {
                    return;
                }
                let reply = match self.node.files_serve_block(&root, index) {
                    Ok(Some(data)) => FileMsg::Block { root, index, data },
                    _ => FileMsg::NotFound { root, index },
                };
                self.send_file(&static_pub, reply).await;
            }
            // ---- Réponses de nos sources ----
            FileMsg::ManifestMsg { manifest } => {
                self.on_file_manifest(static_pub, manifest).await;
            }
            FileMsg::Block { root, index, data } => {
                self.on_file_block(static_pub, root, index, data).await;
            }
            FileMsg::NotFound { root, index } => {
                let actions = {
                    let mut c = self.files_lock();
                    c.on_not_found(&static_pub, &root, index);
                    c.requests_for(&root)
                };
                self.send_file_actions(actions).await;
            }
            FileMsg::Have { root, .. } => {
                let actions = {
                    let mut c = self.files_lock();
                    c.add_source(&root, static_pub);
                    c.requests_for(&root)
                };
                self.send_file_actions(actions).await;
            }
        }
    }

    /// Manifest reçu d'un pair sondé : vérification (signature, racine),
    /// indexation locale puis premières demandes de blocs (reprise via la
    /// bitmap persistée, le cas échéant).
    async fn on_file_manifest(&self, from: [u8; 32], manifest: accord_proto::file_msg::Manifest) {
        let root = manifest.merkle_root;
        let now = crate::node::now_ms();
        let entry = self.node.files_entry(&root).ok().flatten();
        let attached = {
            let mut c = self.files_lock();
            if !c.est_actif(&root) {
                return;
            }
            let bitmap = entry.as_ref().map(|e| e.bitmap.as_slice());
            match c.attach_manifest(manifest.clone(), bitmap, Some(from), now) {
                Ok(v) => v,
                Err(e) => {
                    tracing::debug!(erreur = %e, "fichiers : manifest invalide reçu");
                    false
                }
            }
        };
        if !attached {
            return;
        }
        if entry.is_none() {
            if let Err(e) = self.node.files_store_manifest(&manifest) {
                tracing::warn!(erreur = %e, "fichiers : indexation du manifest impossible");
            }
        }
        let actions = self.files_lock().requests_for(&root);
        self.send_file_actions(actions).await;
    }

    /// Bloc reçu d'une source : vérification Merkle (dans le coordinateur),
    /// réparation Reed-Solomon éventuelle, écriture disque, persistance de la
    /// bitmap de reprise, progression et demandes suivantes.
    async fn on_file_block(&self, from: [u8; 32], root: [u8; 32], index: u32, data: Vec<u8>) {
        use accord_core::files::BlockOutcome;
        let now = crate::node::now_ms();
        let node = Arc::clone(&self.node);
        let (ecrits, bitmap, complete, emission, actions) = {
            let mut c = self.files_lock();
            let Some(outcome) = c.on_block(&from, &root, index, data, now) else {
                return;
            };
            if outcome == BlockOutcome::Rejected {
                tracing::debug!("fichiers : bloc corrompu rejeté (source suspecte)");
            }
            // Parité reçue : tente la réparation des groupes incomplets (les
            // blocs déjà drainés sont relus du disque).
            if outcome == BlockOutcome::ParityStored {
                let relecture = |i: u32| node.files_read_block(&root, i).ok().flatten();
                if let Err(e) = c.try_repair(&root, relecture) {
                    tracing::debug!(erreur = %e, "fichiers : réparation impossible");
                }
            }
            let ecrits = c.drain(&root);
            let bitmap = c.bitmap(&root);
            let complete = c.progress(&root).is_some_and(|p| p.complete);
            let emission = c.should_emit(&root);
            let actions = c.requests_for(&root);
            if complete {
                c.finish(&root);
            }
            (ecrits, bitmap, complete, emission, actions)
        };
        if !ecrits.is_empty() {
            // Blocs sur disque d'abord, bitmap ensuite : une coupure entre
            // les deux laisse une reprise honnête (jamais l'inverse).
            for (i, bloc) in &ecrits {
                if let Err(e) = self.node.files_write_block(&root, *i, bloc) {
                    tracing::warn!(erreur = %e, "fichiers : écriture de bloc impossible");
                    return;
                }
            }
            if let Some(bitmap) = &bitmap {
                if let Err(e) = self.node.files_save_progress(&root, bitmap, complete) {
                    tracing::warn!(erreur = %e, "fichiers : persistance de reprise impossible");
                }
            }
            if complete {
                if let Err(e) = self.node.files_fetch_clear(&root) {
                    tracing::debug!(erreur = %e, "fichiers : intention insoldable");
                }
            }
        }
        if let Some(p) = emission {
            self.node
                .emit_file_progress(&root, p.done, p.total, p.complete);
        }
        self.send_file_actions(actions).await;
    }

    /// Boucle périodique des transferts : adoption des intentions
    /// persistées, sondages et relances, abandons après délais bornés.
    async fn files_loop(self: Arc<Self>) {
        let mut stop = self.stop_signal();
        loop {
            tokio::select! {
                _ = tokio::time::sleep(FILES_TICK) => {}
                res = stop.changed() => {
                    if res.is_err() || *stop.borrow() {
                        return;
                    }
                    continue;
                }
            }
            self.files_tick().await;
        }
    }

    /// Une passe des transferts de fichiers.
    async fn files_tick(&self) {
        let now = crate::node::now_ms();
        self.files_adopt_intents(now);
        let pairs = self.known_peers(fetch::MAX_SOURCES);
        let (actions, abandons) = self.files_lock().tick(now, &pairs);
        self.send_file_actions(actions).await;
        for (root, progress) in abandons {
            tracing::debug!("fichiers : téléchargement abandonné (délai dépassé)");
            if let Err(e) = self.node.files_fetch_clear(&root) {
                tracing::debug!(erreur = %e, "fichiers : intention insoldable");
            }
            self.node
                .emit_file_progress(&root, progress.done, progress.total, false);
        }
    }

    /// Adopte les intentions de téléchargement persistées : les fichiers déjà
    /// complets sont soldés, les autres démarrent (bornés par le
    /// coordinateur ; l'excédent attend une place).
    fn files_adopt_intents(&self, now: u64) {
        let intents = match self.node.files_fetch_intents() {
            Ok(v) => v,
            Err(e) => {
                tracing::debug!(erreur = %e, "fichiers : lecture des intentions impossible");
                return;
            }
        };
        for (root, hint) in intents {
            if self.files_lock().est_actif(&root) {
                continue;
            }
            if let Ok(Some(_)) = self.node.files_local_path(&root) {
                let _ = self.node.files_fetch_clear(&root);
                if let Ok(Some(entry)) = self.node.files_entry(&root) {
                    let total = accord_core::files::merkle::block_count(entry.size);
                    self.node.emit_file_progress(&root, total, total, true);
                }
                continue;
            }
            let entry = self.node.files_entry(&root).ok().flatten();
            let manifest = self.node.files_manifest(&root).ok().flatten();
            let mut c = self.files_lock();
            if !c.begin(root, hint, now) {
                continue;
            }
            // Reprise : manifest déjà indexé et bitmap persistée.
            if let Some(m) = manifest {
                let bitmap = entry.map(|e| e.bitmap);
                if let Err(e) = c.attach_manifest(m, bitmap.as_deref(), None, now) {
                    tracing::debug!(erreur = %e, "fichiers : reprise impossible");
                }
            }
        }
    }
}

#[async_trait::async_trait]
impl crate::voice::FrameSender for Runtime {
    async fn send_voice(&self, to: &[u8; 32], msg: accord_proto::plaintext::VoiceMsg) -> bool {
        let Some(addr) = self.addr_of(to) else {
            return false;
        };
        self.endpoint
            .send(addr, &ChannelMsg::Voice(msg))
            .await
            .is_ok()
    }
}

#[async_trait::async_trait]
impl discovery::LanSink for Runtime {
    async fn on_lan_peer(&self, pubkey: [u8; 32], addr: SocketAddr) {
        // Pair du LAN traité comme un pair d'amorçage : mémorisé pour la
        // livraison directe, puis ensemencement DHT best-effort (handshake +
        // FIND_NODE). Aucune panique ni blocage si le pair n'est pas joignable.
        self.register_peer(pubkey, addr);
        self.seed_peer(addr).await;
    }
}

#[async_trait::async_trait]
impl crate::service::CodeResolver for Runtime {
    async fn resolve(&self, code: &accord_crypto::FriendCode) -> Result<[u8; 32], NodeError> {
        let key = code.dht_key();
        let now = crate::node::now_ms();
        // Voie normale : lookup DHT itératif (efficace dès que la table de
        // routage est peuplée).
        if let Some(record) = self.dht.get(&*self.rpc, key, now).await {
            if let Ok(pubkey) = accord_core::friends::verify_identity_record(code, &record) {
                return Ok(pubkey);
            }
        }
        // Repli d'amorçage : au tout début la table de routage peut être vide.
        // On interroge alors directement les pairs d'amorçage (FIND_VALUE), en
        // réutilisant leur service DHT et la vérification du record d'identité.
        for addr in self.node.bootstrap_peers().unwrap_or_default() {
            let synth = direct_target(addr);
            if let Some(DhtBody::FoundValue { value, nodes }) =
                self.rpc.send_rpc(&synth, DhtBody::FindValue { key }).await
            {
                for n in nodes {
                    self.dht.observe(n, now);
                }
                if let Some(record) = value {
                    if let Ok(pubkey) = accord_core::friends::verify_identity_record(code, &record)
                    {
                        return Ok(pubkey);
                    }
                }
            }
        }
        Err(NodeError::NotFound("code ami introuvable"))
    }
}

#[async_trait::async_trait]
impl NetworkControl for Runtime {
    async fn add_peer(&self, addr: SocketAddr) -> Result<NetworkStatus, NodeError> {
        self.node.add_bootstrap_peer(addr)?;
        // Tentative immédiate : réinitialise le backoff et connecte.
        self.boot_backoff
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&addr);
        self.seed_peer(addr).await;
        self.emit_network_if_changed();
        Ok(self.network_status())
    }

    async fn remove_peer(&self, addr: SocketAddr) -> Result<NetworkStatus, NodeError> {
        self.node.remove_bootstrap_peer(addr)?;
        self.boot_backoff
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&addr);
        Ok(self.network_status())
    }

    fn status(&self) -> NetworkStatus {
        self.network_status()
    }
}

/// `NodeInfo` synthétique pour un RPC DHT direct vers une adresse : seule
/// `addrs` est utilisée par le transport ; l'identité du pair est inconnue au
/// démarrage (le récepteur vérifie l'authenticité des records qu'il rend).
fn direct_target(addr: SocketAddr) -> NodeInfo {
    NodeInfo {
        node_id: NodeId([0u8; 32]),
        static_pub: [0u8; 32],
        pow_nonce: 0,
        flags: 0,
        addrs: vec![accord_proto::types::WireAddr(addr)],
    }
}

/// Adresses locales joignables (`ip:port`) sans loopback, pour l'UI (« ton
/// adresse à donner à un ami ») : l'adresse publique observée par un pair
/// d'abord (si connue), puis l'IP de l'interface de sortie découverte.
fn local_addrs(port: u16, observed: Option<SocketAddr>) -> Vec<String> {
    let mut candidates: Vec<SocketAddr> = Vec::new();
    if let Some(obs) = observed {
        candidates.push(obs);
    }
    for ip in discover_local_ips() {
        candidates.push(SocketAddr::new(ip, port));
    }
    let mut out: Vec<String> = Vec::new();
    for addr in candidates {
        if addr.ip().is_loopback() || addr.ip().is_unspecified() {
            continue;
        }
        let s = addr.to_string();
        if !out.contains(&s) {
            out.push(s);
        }
    }
    out
}

/// Assemble la liste bornée d'adresses de présence (SPEC §11) à partir de
/// l'adresse publique observée (prioritaire) et des IP d'interface locales (au
/// port P2P). Filtre loopback et adresses non spécifiées, déduplique et borne à
/// [`MAX_NODE_ADDRS`] en conservant l'observée publique en tête. Pure (sans
/// socket), testable.
fn assemble_presence_addrs(
    observed: Option<SocketAddr>,
    local_ips: &[IpAddr],
    port: u16,
) -> Vec<SocketAddr> {
    let locales = local_ips.iter().map(|ip| SocketAddr::new(*ip, port));
    let mut addrs: Vec<SocketAddr> = Vec::new();
    for addr in observed.into_iter().chain(locales) {
        if addrs.len() >= MAX_NODE_ADDRS {
            break;
        }
        if addr.ip().is_loopback() || addr.ip().is_unspecified() {
            continue;
        }
        if !addrs.contains(&addr) {
            addrs.push(addr);
        }
    }
    addrs
}

/// IP des interfaces de sortie (IPv4 puis IPv6), par sondage sans émission.
/// Réutilisé par l'assemblage pour alimenter le mapping de port et l'annonce
/// mDNS.
pub(crate) fn discover_local_ips() -> Vec<std::net::IpAddr> {
    let mut ips = Vec::new();
    for target in ["8.8.8.8:80", "[2001:4860:4860::8888]:80"] {
        if let Some(ip) = probe_local_ip(target) {
            if !ips.contains(&ip) {
                ips.push(ip);
            }
        }
    }
    ips
}

/// Découvre l'IP de l'interface qui routerait vers `target` sans envoyer de
/// paquet (`connect` UDP ne fait que fixer la route). `None` hors ligne.
fn probe_local_ip(target: &str) -> Option<std::net::IpAddr> {
    let bind = if target.starts_with('[') {
        "[::]:0"
    } else {
        "0.0.0.0:0"
    };
    let sock = std::net::UdpSocket::bind(bind).ok()?;
    sock.connect(target).ok()?;
    let ip = sock.local_addr().ok()?.ip();
    if ip.is_loopback() || ip.is_unspecified() {
        None
    } else {
        Some(ip)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dht_response_classification() {
        assert!(is_dht_response(&DhtBody::Pong));
        assert!(is_dht_response(&DhtBody::StoreOk));
        assert!(is_dht_response(&DhtBody::FoundNodes { nodes: vec![] }));
        assert!(!is_dht_response(&DhtBody::Ping));
        assert!(!is_dht_response(&DhtBody::FindNode { target: [0; 32] }));
    }

    #[test]
    fn presence_addrs_inclut_locales_et_observee_sans_loopback() {
        use std::net::Ipv4Addr;
        let observed: SocketAddr = "203.0.113.7:5000".parse().unwrap();
        let local_ips = vec![
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 20)), // privé : gardé au port P2P
            IpAddr::V4(Ipv4Addr::LOCALHOST),            // loopback : exclu
            IpAddr::V4(Ipv4Addr::UNSPECIFIED),          // 0.0.0.0 : exclu
        ];
        let out = assemble_presence_addrs(Some(observed), &local_ips, 4433);
        // L'observée publique est en tête (priorité de joignabilité).
        assert_eq!(out.first(), Some(&observed));
        // L'IP privée est publiée au port P2P, pas au sien propre.
        assert!(out.contains(&"192.168.1.20:4433".parse().unwrap()));
        // Ni loopback ni adresse non spécifiée ne subsistent.
        assert!(out
            .iter()
            .all(|a| !a.ip().is_loopback() && !a.ip().is_unspecified()));
        assert_eq!(out.len(), 2, "observée + une seule locale routable");
    }

    #[test]
    fn presence_addrs_borne_a_max_et_priorise_observee() {
        use std::net::Ipv4Addr;
        let observed: SocketAddr = "203.0.113.7:5000".parse().unwrap();
        // Bien plus de MAX_NODE_ADDRS candidats : troncature en gardant l'observée.
        let local_ips: Vec<IpAddr> = (0..10)
            .map(|i| IpAddr::V4(Ipv4Addr::new(10, 0, 0, i)))
            .collect();
        let out = assemble_presence_addrs(Some(observed), &local_ips, 4433);
        assert_eq!(out.len(), MAX_NODE_ADDRS);
        assert_eq!(out.first(), Some(&observed), "observée conservée en tête");
    }

    #[test]
    fn presence_addrs_deduplique_et_tolere_sans_observee() {
        use std::net::Ipv4Addr;
        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 0, 5));
        let out = assemble_presence_addrs(None, &[ip, ip], 4433);
        assert_eq!(out, vec!["192.168.0.5:4433".parse::<SocketAddr>().unwrap()]);
    }
}
