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
use accord_proto::plaintext::ControlMsg;
use accord_proto::types::WireAddr;
use accord_proto::types::{node_flags, NodeId, NodeInfo};
use accord_transport::nat::{Candidate, ObservedAddrs};
use accord_transport::tcp::{self, TcpLinks};
use accord_transport::{Endpoint, TransportError, TransportEvent};
use rand::RngCore;
use tokio::sync::{mpsc, oneshot, watch};

use crate::error::NodeError;
use crate::maintenance::{self, MaintenanceConfig};
use crate::node::diagnostics::{self, NetCounters, ProbeResult, SelfTestReport};
use crate::node::network::{LinkTransport, NetworkControl, NetworkStatus, PeerLink};
use crate::node::relay::{self, NatKind};
use crate::node::Node;
use crate::node::{discovery, holepunch, nat};
use crate::outbound::Outbound;
use crate::voice::VoiceHandle;

/// Délai d'attente d'une réponse RPC DHT.
const RPC_TIMEOUT: Duration = Duration::from_secs(2);

/// Tentatives d'ouverture d'un circuit relais tant que la session directe avec
/// le relais n'est pas établie (handshake en vol après `connect`).
const RELAY_OPEN_RETRIES: u32 = 5;
/// Attente entre deux tentatives d'ouverture de circuit relais.
const RELAY_OPEN_RETRY_WAIT: Duration = Duration::from_millis(200);

/// Rondes de poinçonnage TCP (ouverture simultanée) après l'échec de la salve
/// UDP (SPEC §11.3) : chaque ronde tente tous les candidats en parallèle.
const TCP_PUNCH_ROUNDS: u32 = 3;
/// Délai d'une tentative de connexion TCP poinçonnée.
const TCP_PUNCH_TIMEOUT: Duration = Duration::from_millis(1_500);
/// Pause entre deux rondes de poinçonnage TCP (laisse les SYN se croiser).
const TCP_PUNCH_ROUND_WAIT: Duration = Duration::from_millis(500);

/// Période de la passe des transferts de fichiers.
const FILES_TICK: Duration = Duration::from_millis(250);
/// Anti-abus du service de fichiers : demandes servies par pair et par
/// seconde au plus (au-delà, silence — le pair relancera).
const FILES_REQS_PAR_S: u32 = 256;
/// Anti-inondation des RÉPONSES entrantes de contrôle (`Have`, `NotFound`,
/// `ManifestMsg`) par pair et par seconde : bien au-dessus du débit légitime
/// (un manifest par téléchargement, quelques `Have`/`NotFound`) mais borne un
/// pair qui inonderait ces bras pour nous faire churner requêtes/vérifs. Les
/// `Block` (données en masse) ne sont PAS bridés ici — leur débit légitime est
/// élevé et ils s'auto-limitent (bloc non demandé ou déjà détenu = rejet bon
/// marché).
const FILES_RESP_PAR_S: u32 = 128;
/// Borne du suivi de débit par pair (au-delà, la table est réinitialisée).
const FILES_DEBIT_MAX_PAIRS: usize = 1024;
/// Annonce `Have` à la complétion d'un blob : pairs ciblés au plus (borne
/// anti-inondation ; les pairs qui téléchargent encore ce fichier y gagnent
/// une source secondaire).
const FILES_HAVE_MAX_PAIRS: usize = 16;
/// Suivi des pairs ayant demandé une racine cette session (sémantique
/// « Have » de BitTorrent : ne l'annoncer qu'à qui est déjà dans l'essaim,
/// donc connaît déjà la racine — sinon on divulguerait la capacité de lecture
/// d'un blob privé à des inconnus). Bornes façon [`FILES_DEBIT_MAX_PAIRS`] :
/// au-delà, la table est vidée.
const FILES_REQUESTERS_MAX_ROOTS: usize = 1024;
/// Demandeurs mémorisés au plus par racine (au-delà, éviction du plus ancien).
const FILES_REQUESTERS_MAX_PAR_ROOT: usize = 16;
/// Cadence minimale, par pair, du repli de joignabilité des transferts
/// (résolution de présence DHT + circuit relais) : au-delà d'un déclenchement
/// par fenêtre, les émissions échouées n'empilent pas de nouvelles tâches.
const FILES_REPLI_MIN_MS: u64 = 10_000;

/// Pairs d'amorçage sondés au plus par un auto-test réseau (borne de durée).
const SELFTEST_BOOTSTRAP_MAX: usize = 8;
/// Interrogations d'une sonde d'auto-test avant verdict d'échec.
const SELFTEST_PROBE_POLLS: u32 = 10;
/// Attente entre deux interrogations d'une sonde d'auto-test.
const SELFTEST_PROBE_POLL_WAIT: Duration = Duration::from_millis(100);

/// Borne du carnet d'adresses (cache best-effort, ré-résolu via la DHT au
/// besoin) : au-delà, une entrée est évincée à l'insertion d'un nouveau pair.
/// Empêche une croissance non bornée sous un flot de handshakes (PoW 16 bits) —
/// l'adresse d'un ami évincée est ré-apprise à son prochain message.
const MAX_BOOK: usize = 8192;

/// Délai maximal d'une interrogation du moteur voix DEPUIS la boucle
/// d'événements. Celle-ci est l'unique consommatrice du trafic de TOUS les
/// pairs : elle ne doit jamais se bloquer indéfiniment sur une autre tâche
/// (le moteur voix peut être momentanément occupé, p. ex. ouverture d'un
/// périphérique). Au-delà du délai, on considère « pas dans le salon ».
const VOICE_QUERY_TIMEOUT: Duration = Duration::from_secs(2);

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
        if let Some(tx) = self
            .pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&rpc_id)
        {
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
            .unwrap_or_else(|e| e.into_inner())
            .insert(rpc_id, tx);
        let msg = ChannelMsg::Dht(DhtMessage { rpc_id, body });
        if self.endpoint.send(addr, &msg).await.is_err() {
            self.pending
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&rpc_id);
            return None;
        }
        match tokio::time::timeout(RPC_TIMEOUT, rx).await {
            Ok(Ok(body)) => Some(body),
            _ => {
                self.pending
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .remove(&rpc_id);
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
    /// Annonces authentifiées reçues (`NODE_ANNOUNCE`) : clé publique →
    /// `(pow_nonce, flags)`. Source de vérité pour reconstruire le `NodeInfo`
    /// d'un émetteur de RPC entrant ([`Runtime::route_dht`]) : sans elle, un
    /// `pow_nonce`/`flags` synthétisés à zéro écraseraient l'entrée de table
    /// (drapeau relais perdu). Bornée comme le carnet ([`MAX_BOOK`]).
    announces: Mutex<HashMap<[u8; 32], (u64, u8)>>,
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
    /// Relais dont la joignabilité a été ACTIVEMENT vérifiée (M1a) : identités
    /// ayant annoncé le drapeau RELAY dans une session DIRECTE établie (handshake
    /// mutuellement authentifié = preuve de joignabilité, contrairement au
    /// drapeau seul, auto-déclaré et gratuit). Purgé sur `Disconnected`. Sert à
    /// prioriser ces relais dans la sélection ([`relay::prioritize_reachable`]),
    /// pour qu'un flot de faux relais injoignables ne les évince pas de la
    /// fenêtre bornée d'essais.
    verified_relays: Mutex<HashSet<[u8; 32]>>,
    /// Dernière adresse d'ami écrite dans le carnet PERSISTANT (cf.
    /// [`accord_core::peer_addr`]) : évite une écriture base par message. Une
    /// entrée présente et égale court-circuite la persistance (voir
    /// [`Self::maybe_persist_friend_addr`]).
    addr_persisted: Mutex<HashMap<[u8; 32], SocketAddr>>,
    /// Amis auxquels notre profil a été RE-annoncé sur premier message entrant
    /// de l'épisode de session courant (purgé sur `Disconnected`, comme
    /// [`Self::live`]). Filet de l'annonce à la connexion (D-052) : quand des
    /// sessions se croisent à la reconnexion (dial + poinçonnage vers un pair
    /// qui relie le même port), l'annonce unique peut partir sur un lien
    /// cadavre et se perdre sans trace — un message ENTRANT de l'ami prouve un
    /// chemin vivant, on rejoue l'annonce dessus (une fois par épisode).
    profile_reannounced: Mutex<HashSet<[u8; 32]>>,
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
    /// Fenêtres de débit des RÉPONSES entrantes de contrôle (`Have`,
    /// `NotFound`, `ManifestMsg`), par pair : anti-inondation, borné.
    files_resp_debit: Mutex<HashMap<[u8; 32], (u64, u32)>>,
    /// Pairs ayant demandé une racine (`GetManifest`/`GetBlock`) cette session :
    /// seuls destinataires légitimes d'une annonce `Have` de cette racine (ils
    /// la connaissent déjà — aucune divulgation). Auto-borné.
    files_requesters: Mutex<HashMap<[u8; 32], Vec<[u8; 32]>>>,
    /// Dernier repli de joignabilité déclenché par pair pour les transferts
    /// (présence DHT + relais), cadencé par [`FILES_REPLI_MIN_MS`].
    files_repli: Mutex<HashMap<[u8; 32], u64>>,
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
    /// Nœuds d'amorçage/relais PAR DÉFAUT (points d'entrée du réseau livrés avec
    /// l'app), fusionnés avec les pairs ajoutés par l'utilisateur pour le
    /// seeding, la reconnexion et le repli de résolution de code ami. Câblés une
    /// seule fois au démarrage ; jamais persistés (une valeur périmée d'une
    /// version antérieure ne s'accumule pas en base).
    default_bootstrap: OnceLock<Vec<SocketAddr>>,
    /// Référence faible sur soi-même, posée au démarrage ([`Runtime::spawn`]) :
    /// permet aux boucles de maintenance (qui n'ont qu'un `&Runtime`) de
    /// détacher une tâche possédant un `Arc<Runtime>` — typiquement le repli
    /// relais, borné et déporté hors de la boucle de résolution.
    self_ref: OnceLock<Weak<Runtime>>,
    /// Cadence et corrélation du poinçonnage coordonné (SPEC §11.2) : état
    /// borné, politique dans [`crate::node::holepunch`].
    punch: holepunch::PunchCoordinator,
    /// Registre des liens TCP (repli SPEC §11.3), câblé après construction par
    /// l'assemblage ([`crate::run_with_maintenance`]) ; absent dans les tests
    /// sans réseau réel — le repli TCP est alors simplement désactivé.
    tcp_links: OnceLock<Arc<TcpLinks>>,
    /// Compteurs réseau locaux (D3/D35) : poinçonnage, relais, boîtes aux
    /// lettres, outbox, reconnexions. Jamais transmis — diagnostic local.
    counters: NetCounters,
    /// Dernière remise RÉUSSIE d'un message par pair (ms epoch), tout canal
    /// confondu (direct, relais, vidage d'outbox). Alimente `network.peers`
    /// (D4). Borné façon [`MAX_BOOK`] par éviction du plus ancien.
    last_delivery: Mutex<HashMap<[u8; 32], u64>>,
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
            announces: Mutex::new(HashMap::new()),
            maintenance,
            observed: Mutex::new(None),
            observed_addrs: Mutex::new(ObservedAddrs::new()),
            live: Mutex::new(HashSet::new()),
            verified_relays: Mutex::new(HashSet::new()),
            addr_persisted: Mutex::new(HashMap::new()),
            profile_reannounced: Mutex::new(HashSet::new()),
            stop_tx,
            presence_cursor: AtomicUsize::new(0),
            mailbox_cursor: AtomicUsize::new(0),
            voice: OnceLock::new(),
            files: Mutex::new(fetch::Coordinator::new()),
            files_debit: Mutex::new(HashMap::new()),
            files_resp_debit: Mutex::new(HashMap::new()),
            files_requesters: Mutex::new(HashMap::new()),
            files_repli: Mutex::new(HashMap::new()),
            net_last: Mutex::new(None),
            boot_backoff: Mutex::new(HashMap::new()),
            default_bootstrap: OnceLock::new(),
            nat: Arc::new(nat::NatShared::default()),
            lan: Arc::new(discovery::LanShared::default()),
            self_ref: OnceLock::new(),
            punch: holepunch::PunchCoordinator::default(),
            tcp_links: OnceLock::new(),
            counters: NetCounters::default(),
            last_delivery: Mutex::new(HashMap::new()),
        })
    }

    /// Câble le registre des liens TCP (une seule fois, à l'assemblage).
    pub fn set_tcp_links(&self, links: Arc<TcpLinks>) {
        let _ = self.tcp_links.set(links);
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

    /// Câble les nœuds d'amorçage par défaut (une seule fois, au démarrage).
    pub fn set_default_bootstrap(&self, peers: Vec<SocketAddr>) {
        let _ = self.default_bootstrap.set(peers);
    }

    /// Liste EFFECTIVE des pairs d'amorçage : pairs par défaut (points d'entrée
    /// livrés avec l'app) fusionnés avec ceux ajoutés par l'utilisateur, sans
    /// doublon, bornée. C'est ce rendez-vous partagé qui rend le premier contact
    /// possible entre deux pairs tous deux NATés (aucun n'étant joignable, ils
    /// ne peuvent se joindre qu'à travers un nœud commun).
    pub(crate) fn all_bootstrap_peers(&self) -> Vec<SocketAddr> {
        let mut out: Vec<SocketAddr> = self.default_bootstrap.get().cloned().unwrap_or_default();
        for addr in self.node.bootstrap_peers().unwrap_or_default() {
            if !out.contains(&addr) {
                out.push(addr);
            }
        }
        out.truncate(crate::node::network::MAX_BOOTSTRAP_PEERS);
        out
    }

    /// Ensemence tous les pairs d'amorçage effectifs (au démarrage).
    pub(crate) async fn bootstrap_all(&self) {
        for addr in self.all_bootstrap_peers() {
            self.seed_peer(addr).await;
        }
        self.emit_network_if_changed();
    }

    /// Reconnexion RAPIDE aux amis via leur dernière adresse directe mémorisée
    /// (carnet persistant, cf. [`peer_addr`]). Appelée au démarrage EN PLUS de
    /// l'amorçage DHT : une adresse encore valide rétablit la session en un
    /// aller-retour, sans attendre la résolution de présence. Best-effort et
    /// borné (nombre d'amis) ; un dial qui échoue est sans conséquence — la
    /// DHT et la maintenance périodique prennent le relais.
    pub(crate) async fn reconnect_known_friends(&self) {
        let known = match self.node.known_friend_addrs() {
            Ok(k) if !k.is_empty() => k,
            _ => {
                tracing::debug!("reconnexion : aucune adresse d'ami mémorisée");
                return;
            }
        };
        tracing::debug!(
            amis = known.len(),
            "reconnexion : dial des adresses mémorisées"
        );
        for (pubkey, addr) in known {
            // Le carnet mémoire connaît déjà l'adresse : la session à venir
            // (événement `Connected`) déclenchera flush + convergence profil.
            self.register_peer_if_absent(pubkey, addr);
            if let Err(e) = self.endpoint.connect(addr).await {
                tracing::debug!(erreur = %e, "reconnexion ami : dial impossible");
            }
        }
        self.emit_network_if_changed();
    }

    /// Reconnecte les pairs d'amorçage injoignables, avec backoff par pair
    /// (appelée périodiquement par la maintenance).
    pub(crate) async fn reconnect_bootstrap(&self, base: Duration) {
        let now = crate::node::now_ms();
        let peers = self.all_bootstrap_peers();
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
            self.counters.reconnect_attempt();
            let ok = self.seed_peer(addr).await;
            let connected = ok && self.is_peer_connected(&addr);
            let mut bo = self.boot_backoff.lock().unwrap_or_else(|e| e.into_inner());
            if connected {
                self.counters.reconnect_ok();
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

    /// Mémorise l'annonce authentifiée d'un pair (borné façon [`MAX_BOOK`] :
    /// au-delà, une entrée arbitraire est évincée — cache best-effort,
    /// ré-alimenté à la prochaine annonce du pair concerné).
    fn remember_announce(&self, pubkey: [u8; 32], pow_nonce: u64, flags: u8) {
        let mut cache = self.announces.lock().unwrap_or_else(|e| e.into_inner());
        if cache.len() >= MAX_BOOK && !cache.contains_key(&pubkey) {
            if let Some(victim) = cache.keys().next().copied() {
                cache.remove(&victim);
            }
        }
        cache.insert(pubkey, (pow_nonce, flags));
    }

    /// Annonce authentifiée connue d'un pair, le cas échéant.
    fn announce_of(&self, pubkey: &[u8; 32]) -> Option<(u64, u8)> {
        self.announces
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(pubkey)
            .copied()
    }

    fn remember(&self, pubkey: [u8; 32], addr: SocketAddr) {
        let mut book = self.book.lock().unwrap_or_else(|e| e.into_inner());
        // Borne mémoire (anti-croissance non bornée) : au-delà de [`MAX_BOOK`],
        // l'insertion d'un pair INCONNU évince une entrée. Best-effort — le
        // carnet est un cache d'adresses, ré-résolu via la DHT si besoin.
        if book.by_pubkey.len() >= MAX_BOOK && !book.by_pubkey.contains_key(&pubkey) {
            if let Some(victim) = book.by_pubkey.keys().next().copied() {
                book.by_pubkey.remove(&victim);
            }
        }
        book.by_pubkey.insert(pubkey, addr);
    }

    pub(crate) fn addr_of(&self, pubkey: &[u8; 32]) -> Option<SocketAddr> {
        self.book
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .by_pubkey
            .get(pubkey)
            .copied()
    }

    /// Persiste l'adresse directe d'une RELATION (ami ou demande en cours)
    /// dans le carnet durable, pour une reconnexion rapide au prochain
    /// démarrage. Court-circuit peu coûteux : si `(pubkey, addr)` est déjà
    /// l'entrée persistée connue, aucune lecture base — la persistance ne
    /// coûte qu'un accès par changement d'adresse (nouvelle session), pas par
    /// message. Le périmètre RELATION (et non ami strict) est décisif : la
    /// session s'établit souvent AVANT la conclusion de l'amitié ; ne
    /// persister que les amis raterait cette fenêtre et laisserait le cache
    /// vide au premier redémarrage — le pair redémarré ne pourrait plus
    /// joindre personne quand l'autre côté garde une session périmée vers son
    /// ancienne incarnation (poinçonnage no-op « déjà connecté »).
    fn maybe_persist_friend_addr(&self, pubkey: [u8; 32], addr: SocketAddr) {
        {
            let map = self
                .addr_persisted
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if map.get(&pubkey) == Some(&addr) {
                return;
            }
        }
        if !self.node.is_relation(&pubkey) {
            return;
        }
        if self.node.remember_peer_addr(pubkey, addr).is_ok() {
            self.addr_persisted
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(pubkey, addr);
        }
    }

    /// Compteurs réseau locaux (incrémentés par le runtime et la maintenance).
    /// Nom distinct du `NetworkControl::counters` (photographie) pour lever
    /// l'ambiguïté inhérent/trait.
    pub(crate) fn net_counters(&self) -> &NetCounters {
        &self.counters
    }

    /// Note une remise RÉUSSIE vers `to` (alimente `network.peers`, D4).
    /// Bornée : éviction du plus ancien au-delà de [`MAX_BOOK`] entrées.
    pub(crate) fn note_delivery(&self, to: &[u8; 32]) {
        let now = crate::node::now_ms();
        let mut map = self.last_delivery.lock().unwrap_or_else(|e| e.into_inner());
        evict_oldest_if_full(&mut map, MAX_BOOK, to);
        map.insert(*to, now);
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
        let mut verified = self
            .verified_relays
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let mut reannounced = self
            .profile_reannounced
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        for k in &keys {
            live.remove(k);
            // La joignabilité vérifiée expire avec la session (M1a) : on ne
            // priorise plus un relais dont on n'a plus de session directe.
            verified.remove(k);
            // Le filet de ré-annonce de profil se réarme pour le prochain
            // épisode de session (voir `profile_reannounced`).
            reannounced.remove(k);
        }
    }

    /// Note un relais comme ACTIVEMENT vérifié joignable (M1a) : appelé quand un
    /// pair annonce le drapeau RELAY sur une session DIRECTE établie (le
    /// handshake mutuellement authentifié atteste la joignabilité).
    fn mark_relay_verified(&self, pubkey: [u8; 32]) {
        self.verified_relays
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(pubkey);
    }

    /// Instantané des relais vérifiés joignables (M1a), pour la priorisation de
    /// sélection.
    fn verified_relays_snapshot(&self) -> HashSet<[u8; 32]> {
        self.verified_relays
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Vrai si une session (directe ou relayée) est active avec ce pair.
    pub(crate) fn is_peer_live(&self, pubkey: &[u8; 32]) -> bool {
        self.live
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(pubkey)
    }

    /// Enregistre une observation d'adresse publique rapportée par le pair
    /// `observer` (SPEC §11.1) et met à jour l'adresse retenue : le consensus
    /// s'il existe, sinon la dernière reçue. La déduplication par identité
    /// d'observateur ([`ObservedAddrs::observe`]) empêche un pair unique de
    /// fabriquer un consensus en votant plusieurs fois (M1b).
    fn observe_addr(&self, observer: [u8; 32], observed: SocketAddr) {
        let consensus = {
            let mut agg = self
                .observed_addrs
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            agg.observe(observer, observed);
            agg.consensus()
        };
        *self.observed.lock().unwrap_or_else(|e| e.into_inner()) =
            Some(consensus.unwrap_or(observed));
        // La nature du NAT a pu changer (nouvelle observation) : rafraîchit le
        // statut réseau si le signal a évolué.
        self.emit_network_if_changed();
    }

    /// Consensus d'adresse publique observée (≥ 2 pairs concordants), s'il
    /// existe (SPEC §11.1). Alimente l'éligibilité relais.
    pub(crate) fn observed_consensus(&self) -> Option<SocketAddr> {
        self.observed_addrs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .consensus()
    }

    /// Adresse externe du mapping de port automatique (UPnP/NAT-PMP), si actif.
    pub(crate) fn nat_mapping_external(&self) -> Option<SocketAddr> {
        self.nat.snapshot().external
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

    /// Relais « domicile » d'un pair (ou de soi-même) : les relais annoncés les
    /// plus proches (distance XOR) de `node_id_of(owner)` dans la table de
    /// routage, bornés à [`relay::HOME_RELAY_COUNT`]. Dérivation déterministe,
    /// calculable par N'IMPORTE QUI (elle ne dépend que de la clé publique du
    /// propriétaire) : le propriétaire y entretient une session
    /// ([`crate::maintenance`], passe domicile) et un expéditeur inconnu les
    /// essaie en repli — rendez-vous du premier contact sans port ouvert.
    pub(crate) fn home_relays_of(&self, owner: &[u8; 32]) -> Vec<NodeInfo> {
        let me = self.node.public_key();
        let candidates = self
            .dht
            .closest_local(&node_id_of(owner), relay::RELAY_SELECT_K);
        relay::select_home_relays(candidates, &[me, *owner])
    }

    /// Bascule sur le relais pour joindre `friend` (SPEC §11.3), best-effort et
    /// idempotent : ne fait rien si une session (directe ou relayée) existe déjà
    /// ou si un circuit vers l'ami est déjà ouvert. Sinon, essaie les relais
    /// candidats dans l'ordre déterministe — clé de paire d'abord, puis relais
    /// domicile du pair (repli premier contact, où le pair entretient une
    /// session ; borné à [`relay::RELAY_TRY_MAX`] au total) : session directe
    /// avec le relais (établie au besoin), ouverture de circuit, puis handshake
    /// A↔B tunnelé (liaison d'identité D-037 garantie côté transport). À
    /// déclencher détaché (`tokio::spawn`) pour ne pas bloquer la boucle
    /// appelante ; aucun verrou n'est tenu pendant les `await` réseau.
    pub(crate) async fn ensure_relay_to(&self, friend: [u8; 32]) {
        let friend_node = node_id_of(&friend);
        // Idempotence : déjà joignable ou circuit déjà ouvert ⇒ rien à faire.
        if self.is_peer_live(&friend) || self.endpoint.circuit_for_peer(friend_node).is_some() {
            return;
        }
        // Aligne la vue locale sur la vue GLOBALE autour de l'identifiant du
        // pair avant la sélection : les relais domicile sont un rendez-vous —
        // les deux côtés doivent dériver le MÊME ensemble, ce qu'une table
        // locale clairsemée ne garantit pas. Lookup itératif borné (α, k) qui
        // peuple la table via `observe`.
        let _ = self.dht.lookup_node(&*self.rpc, friend_node).await;
        let candidats = relay::merge_relay_candidates(
            self.select_relay_for(&friend),
            self.home_relays_of(&friend),
            &self.verified_relays_snapshot(),
        );
        for relay_info in candidats {
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
                self.counters.relay_open_ok();
                tracing::debug!("repli : circuit relais ouvert vers l'ami");
                return;
            }
        }
        self.counters.relay_open_fail();
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

    // ---- Poinçonnage coordonné (SPEC §11.2) ----

    /// Demande à `friend` un poinçonnage coordonné en lui transmettant nos
    /// candidats frais, par n'importe quel lien déjà établi — typiquement la
    /// session bout-en-bout tunnelée par un relais (rendez-vous sans serveur).
    /// Best-effort, cadencé par le coordinateur ; sans effet si une session
    /// directe existe déjà.
    pub(crate) async fn request_punch(&self, friend: [u8; 32]) {
        if self.endpoint.has_direct_session_with(&friend) {
            return; // déjà en direct : rien à upgrader
        }
        let candidates = self.presence_addrs();
        if candidates.is_empty() {
            return; // rien à proposer au pair (aucune adresse joignable)
        }
        let token = rand::rngs::OsRng.next_u64();
        if !self
            .punch
            .begin_request(friend, token, crate::node::now_ms())
        {
            return; // demande trop récente (cadence) ou état plein
        }
        let msg = ChannelMsg::Control(ControlMsg::PunchRequest {
            token,
            candidates: candidates.into_iter().map(WireAddr).collect(),
        });
        if self.send_via_best_link(&friend, &msg).await {
            self.counters.punch_requested();
            tracing::debug!("poinçonnage : demande coordonnée émise");
        }
    }

    /// Achemine un message vers un pair par le meilleur lien existant :
    /// session liée à l'identité d'abord, circuit relais sinon (SPEC §11.3 —
    /// une session relayée a pour adresse celle du relais et n'est pas
    /// indexée par adresse : l'envoi direct y échoue en liaison d'identité et
    /// DOIT retomber sur le circuit). Rend vrai si un envoi est parti (sans
    /// garantie de livraison). Utilisé par la signalisation de poinçonnage et
    /// par les boucles de maintenance (outbox, profil, anti-entropie).
    pub(crate) async fn send_via_best_link(&self, to: &[u8; 32], msg: &ChannelMsg) -> bool {
        // Session directe ÉTABLIE seulement (adresse tenue par l'endpoint, pas
        // par le carnet) : un `send_to` vers une adresse sans session mettrait
        // le message en file d'un handshake spéculatif et rendrait Ok — trou
        // noir si l'adresse (issue d'un record de présence) est injoignable,
        // alors que l'appelant retire le message de l'outbox sur ce
        // « succès ». Ici : pas de session ⇒ on tente le circuit relais,
        // sinon échec franc (le message reste en file, relivré au prochain
        // lien — `flush_peer` à la connexion, ou la passe d'outbox suivante).
        if let Some(addr) = self.endpoint.direct_session_addr(to) {
            match self.endpoint.send_to(addr, Some(*to), msg).await {
                Ok(()) => {
                    self.note_delivery(to);
                    return true;
                }
                Err(e) => tracing::debug!(
                    ami = %crate::hex::encode(&to[..4]),
                    %addr,
                    erreur = %e,
                    "envoi direct impossible"
                ),
            }
        } else {
            tracing::debug!(
                ami = %crate::hex::encode(&to[..4]),
                "envoi : aucune session directe"
            );
        }
        if let Some(circuit) = self.endpoint.circuit_for_peer(node_id_of(to)) {
            match self.endpoint.send_via_relay(circuit, msg).await {
                Ok(()) => {
                    self.note_delivery(to);
                    return true;
                }
                Err(e) => tracing::debug!(
                    ami = %crate::hex::encode(&to[..4]),
                    erreur = %e,
                    "envoi via relais impossible"
                ),
            }
        }
        false
    }

    /// Vrai si `pubkey` est un ami confirmé (les demandes de poinçonnage des
    /// simples pairs de session — nœuds DHT, inconnus — sont ignorées : elles
    /// feraient émettre des HELLO vers des adresses arbitraires).
    fn is_friend(&self, pubkey: &[u8; 32]) -> bool {
        self.node
            .friend_pubkeys()
            .map(|friends| friends.contains(pubkey))
            .unwrap_or(false)
    }

    /// Demande de poinçonnage ENTRANTE : amitié requise, cadence par pair,
    /// candidats filtrés et bornés ; puis réponse avec NOS candidats frais et
    /// salve immédiate vers les siens — les deux salves se croisent.
    async fn on_punch_requested(&self, from: [u8; 32], token: u64, candidates: Vec<SocketAddr>) {
        if !self.is_friend(&from) {
            tracing::debug!("poinçonnage : demande d'un non-ami ignorée");
            return;
        }
        if !self.punch.accept_inbound(from, crate::node::now_ms()) {
            tracing::debug!("poinçonnage : demande trop fréquente ignorée");
            return;
        }
        let candidates = holepunch::sanitize_candidates(&candidates);
        if candidates.is_empty() {
            return;
        }
        self.counters.punch_received();
        let ours: Vec<WireAddr> = self.presence_addrs().into_iter().map(WireAddr).collect();
        let resp = ChannelMsg::Control(ControlMsg::PunchResponse {
            token,
            candidates: ours,
        });
        let _ = self.send_via_best_link(&from, &resp).await;
        self.spawn_punch(from, candidates);
    }

    /// Réponse de poinçonnage : uniquement si elle corrèle une demande
    /// sortante fraîche (jeton), sinon ignorée (forgée, rejouée, périmée).
    async fn on_punch_responded(&self, from: [u8; 32], token: u64, candidates: Vec<SocketAddr>) {
        if !self.punch.take_response(from, token, crate::node::now_ms()) {
            tracing::debug!("poinçonnage : réponse non sollicitée ignorée");
            return;
        }
        let candidates = holepunch::sanitize_candidates(&candidates);
        if candidates.is_empty() {
            return;
        }
        self.spawn_punch(from, candidates);
    }

    /// Salve UDP vers `candidates` (liée à l'identité `friend`), puis repli
    /// TCP par ouverture simultanée si aucune session directe n'est apparue
    /// (SPEC §11.3 : UDP d'abord, TCP ensuite, relais en dernier). Détaché
    /// pour ne jamais bloquer la boucle d'événements.
    fn spawn_punch(&self, friend: [u8; 32], candidates: Vec<SocketAddr>) {
        let endpoint = self.endpoint_arc();
        let tcp = self.tcp_links.get().cloned();
        // Comptage du résultat (D3) : `None` si les boucles ne sont pas
        // démarrées (tests sans réseau) — on ne compte alors rien.
        let rt = self.arc();
        tokio::spawn(async move {
            let cands: Vec<Candidate> = candidates
                .iter()
                .copied()
                .map(maintenance::classer_candidat)
                .collect();
            if let Err(e) = endpoint.punch(&cands, friend).await {
                tracing::debug!(erreur = %e, "poinçonnage coordonné : salve UDP échouée");
            }
            if !endpoint.has_direct_session_with(&friend) {
                if let Some(links) = tcp {
                    tcp_punch_toward(Arc::clone(&endpoint), links, friend, &candidates).await;
                }
            }
            if let Some(rt) = rt {
                if endpoint.has_direct_session_with(&friend) {
                    rt.counters.punch_ok();
                } else {
                    rt.counters.punch_fail();
                }
            }
        });
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
        // Honore le signal d'arrêt (Lot G) : `events.recv()` ne rend jamais
        // `None` — le sender vit dans le cycle d'`Arc` que cette tâche retient
        // (tâche → Runtime → Endpoint → sender). Sans arrêt explicite, la tâche
        // ne sort jamais et le runtime entier (avec son socket UDP) fuit à
        // chaque verrouillage. Le `select` casse le cycle.
        let mut stop = self.stop_signal();
        loop {
            let event = tokio::select! {
                maybe = events.recv() => match maybe {
                    Some(ev) => ev,
                    None => break,
                },
                res = stop.changed() => {
                    if res.is_err() || *stop.borrow() {
                        break;
                    }
                    continue;
                }
            };
            match event {
                TransportEvent::Connected {
                    addr, static_pub, ..
                } => {
                    self.remember(static_pub, addr);
                    self.mark_live(static_pub);
                    // Pair joignable : pousse immédiatement sa file d'attente.
                    maintenance::flush_peer(&self, &static_pub, addr).await;
                    // … et relance les téléchargements qui l'attendaient
                    // (intentions en backoff dont il est l'indice,
                    // avatar/bannière annoncés mais manquants en local).
                    self.files_on_peer_connected(&static_pub);
                    // Convergence de profil DÉTERMINISTE (D-052) : notre
                    // profil courant part à CHAQUE établissement de session
                    // avec un ami. Les autres canaux (annonce au changement
                    // via l'outbox, dépôt en boîte DHT, ré-annonce
                    // périodique) sont tous best-effort et peuvent tous
                    // rater ensemble sur le terrain (outbox purgée après 7 j,
                    // DHT injoignable, sessions plus courtes que la période
                    // de ré-annonce) — laissant un pair bloqué sur un profil
                    // périmé (« je ne vois jamais sa bannière »). Un petit
                    // message par connexion suffit à fermer cette classe de
                    // pannes : le pair compare les hashes et ne télécharge
                    // que ce qui a changé (anti-DoS déjà en place).
                    // Carnet d'adresses PERSISTANT (reconnexion rapide au
                    // prochain démarrage, avant la DHT). Voir aussi l'appel
                    // jumeau sur `Message` pour les sessions antérieures à
                    // l'amitié.
                    self.maybe_persist_friend_addr(static_pub, addr);
                    // Nouvel épisode de session : réarme le filet de
                    // ré-annonce (voir `profile_reannounced`). Indispensable
                    // ici et pas seulement sur `Disconnected` : l'extinction
                    // brutale d'un pair (UDP) n'émet RIEN — au retour du pair,
                    // le drapeau de l'épisode précédent inhiberait le filet.
                    self.profile_reannounced
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .remove(&static_pub);
                    if self.is_friend(&static_pub) {
                        match self.node.own_profile_msg() {
                            Ok(Some(msg)) => {
                                let banniere = matches!(
                                    &msg,
                                    accord_proto::core_msg::CoreMsg::Profile {
                                        banner: Some(_),
                                        ..
                                    }
                                );
                                let envoye = self
                                    .send_via_best_link(&static_pub, &ChannelMsg::Core(msg))
                                    .await;
                                tracing::debug!(
                                    moi = %crate::hex::encode(&self.node.public_key()[..4]),
                                    ami = %crate::hex::encode(&static_pub[..4]),
                                    envoye,
                                    banniere,
                                    "profil : annonce à l'établissement de session"
                                );
                            }
                            Ok(None) => {
                                tracing::debug!("profil : rien à annoncer à la connexion");
                            }
                            Err(e) => {
                                tracing::debug!(erreur = %e, "profil : annonce à la connexion impossible");
                            }
                        }
                    }
                }
                TransportEvent::Message {
                    addr,
                    static_pub,
                    msg,
                    ..
                } => {
                    self.remember(static_pub, addr);
                    self.mark_live(static_pub);
                    self.maybe_persist_friend_addr(static_pub, addr);
                    // La relation peut NAÎTRE pendant le routage (FriendRequest/
                    // FriendResponse ingérés) : re-tente la persistance APRÈS,
                    // sinon un nœud qui s'éteint juste après avoir accepté une
                    // amitié redémarre avec un carnet vide. Borné aux messages
                    // Core (seuls à changer l'état des contacts) — le
                    // court-circuit mémoire rend l'appel gratuit ensuite.
                    let est_core = matches!(&*msg, ChannelMsg::Core(_));
                    self.route(addr, static_pub, *msg).await;
                    if est_core {
                        self.maybe_persist_friend_addr(static_pub, addr);
                    }
                    // Filet D-052 (voir `profile_reannounced`) : premier
                    // message ENTRANT d'un ami sur cet épisode de session —
                    // le chemin est prouvé vivant, on rejoue notre annonce de
                    // profil dessus. L'annonce à la connexion seule peut se
                    // perdre sans trace quand dial et poinçonnage se croisent
                    // à la reconnexion (envoi sur une session cadavre).
                    let premier_message = self
                        .profile_reannounced
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .insert(static_pub);
                    if premier_message && self.is_friend(&static_pub) {
                        if let Ok(Some(msg)) = self.node.own_profile_msg() {
                            let envoye = self
                                .send_via_best_link(&static_pub, &ChannelMsg::Core(msg))
                                .await;
                            tracing::debug!(
                                ami = %crate::hex::encode(&static_pub[..4]),
                                envoye,
                                "profil : ré-annonce sur premier message entrant"
                            );
                        }
                    }
                }
                TransportEvent::ObservedAddr { observer, observed } => {
                    self.observe_addr(observer, observed);
                }
                TransportEvent::NodeAnnounced {
                    static_pub,
                    addr,
                    pow_nonce,
                    flags,
                } => {
                    // Apprentissage DHT organique (SPEC §11.3) : l'identité est
                    // celle, AUTHENTIFIÉE, de la session ; l'adresse est celle
                    // OBSERVÉE (un pair ne peut pas faire pointer une entrée
                    // vers un tiers) ; la preuve de travail est re-vérifiée par
                    // `KademliaNode::observe` (via `valid_node`).
                    self.remember_announce(static_pub, pow_nonce, flags);
                    // M1a : le drapeau RELAY reçu SUR CETTE SESSION DIRECTE est
                    // une preuve de joignabilité (l'événement n'est émis que sur
                    // un lien direct authentifié) — on le note vérifié pour la
                    // priorisation de sélection. Le drapeau seul, dans un
                    // `NodeInfo` glané par gossip, ne le serait pas.
                    if flags & node_flags::RELAY != 0 {
                        self.mark_relay_verified(static_pub);
                    }
                    let info = NodeInfo {
                        node_id: node_id_of(&static_pub),
                        static_pub,
                        pow_nonce,
                        flags,
                        addrs: vec![WireAddr(addr)],
                    };
                    self.dht.observe(info, crate::node::now_ms());
                    self.emit_network_if_changed();
                }
                TransportEvent::PunchRequested {
                    static_pub,
                    token,
                    candidates,
                } => {
                    self.on_punch_requested(static_pub, token, candidates).await;
                }
                TransportEvent::PunchResponded {
                    static_pub,
                    token,
                    candidates,
                } => {
                    self.on_punch_responded(static_pub, token, candidates).await;
                }
                TransportEvent::Disconnected { addr } => {
                    self.forget_live(addr);
                }
                _ => {}
            }
        }
    }

    async fn outbound_loop(self: Arc<Self>, mut outbound: mpsc::Receiver<Outbound>) {
        // Même cycle d'`Arc` que `event_loop` (Lot G) : honore l'arrêt pour que
        // la tâche relâche son `Arc<Runtime>` au shutdown.
        let mut stop = self.stop_signal();
        loop {
            let action = tokio::select! {
                maybe = outbound.recv() => match maybe {
                    Some(a) => a,
                    None => break,
                },
                res = stop.changed() => {
                    if res.is_err() || *stop.borrow() {
                        break;
                    }
                    continue;
                }
            };
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
                self.note_delivery(to_pubkey);
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
                self.note_delivery(to_pubkey);
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
            Ok(()) => {
                self.counters.outbox_enqueued();
                tracing::debug!("core : destinataire injoignable, mis en file hors-ligne");
            }
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
        // Requête : construit le NodeInfo de l'émetteur et répond. La preuve
        // de travail et les drapeaux proviennent de l'annonce AUTHENTIFIÉE du
        // pair (`NODE_ANNOUNCE`) quand elle est connue : c'est ce qui permet à
        // `handle_rpc` d'apprendre l'émetteur (`valid_node`) sans jamais
        // écraser son drapeau relais par des valeurs synthétisées à zéro.
        let (pow_nonce, flags) = self.announce_of(&static_pub).unwrap_or((0, 0));
        let from = NodeInfo {
            node_id: accord_crypto::node_id_of(&static_pub),
            static_pub,
            pow_nonce,
            flags,
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
        // Signalisation d'appel 1-à-1 : éphémère, routée vers le moteur voix
        // sans passer par la base (amitié, cadence et corrélation stricte y
        // sont vérifiées ; jamais mise en file hors-ligne).
        if matches!(
            core_msg,
            CoreMsg::CallOffer { .. }
                | CoreMsg::CallAnswer { .. }
                | CoreMsg::CallDecline { .. }
                | CoreMsg::CallHangup { .. }
        ) {
            if let Some(voice) = self.voice.get() {
                voice.peer_call(*static_pub, core_msg);
            }
            return;
        }
        // Soundboard : un son ne doit se jouer que chez les participants du
        // salon vocal ciblé. La présence vocale locale vit dans l'acteur voix
        // (injoignable de façon synchrone depuis `Node::ingest_core`) : c'est
        // ici, au routeur — seul détenteur de la poignée voix — qu'on la
        // vérifie. Hors de CE salon (ou sans salon actif) le message n'est
        // jamais ingéré, donc `event.soundboard_play` n'est jamais émis.
        if let CoreMsg::SoundboardPlay {
            group_id,
            channel_id,
            ..
        } = &core_msg
        {
            // Borné par un timeout : le moteur voix vit sur une AUTRE tâche.
            // S'il est bloqué (ouverture de périphérique qui pend, dialogue de
            // permission macOS sans réponse…), la boucle d'événements — seule
            // à router le trafic de TOUS les pairs — ne doit pas geler avec lui
            // (sinon plus aucun message/DHT/fichier ne passe jusqu'au
            // redémarrage). Au pire on n'annonce pas ce son : dégradation bénigne.
            let in_room = match self.voice.get() {
                Some(voice) => matches!(
                    tokio::time::timeout(VOICE_QUERY_TIMEOUT, voice.status()).await,
                    Ok(Ok(Some(s))) if s.is_room(group_id, channel_id)
                ),
                None => false,
            };
            if !in_room {
                return;
            }
        }
        // Un accusé applicatif solde l'élément d'outbox correspondant.
        if let CoreMsg::MsgAck { msg_id } = &core_msg {
            if let Err(e) = self.node.outbox_ack(static_pub, msg_id) {
                tracing::debug!(erreur = %e, "core : purge d'outbox sur accusé impossible");
            }
        }
        // Une op de groupe ingérée peut changer la modération vocale ou les
        // priorités d'orateur : le moteur voix rafraîchit son cache.
        let voice_refresh = match &core_msg {
            CoreMsg::GroupOpMsg { op } => Some(op.group_id),
            _ => None,
        };
        match self.node.ingest_core(static_pub, core_msg) {
            Ok(replies) => {
                if let Some(group_id) = voice_refresh {
                    if let Some(voice) = self.voice.get() {
                        voice.group_changed(group_id);
                    }
                }
                for reply in replies {
                    self.deliver_core(static_pub, reply).await;
                }
            }
            Err(e) => tracing::debug!(erreur = %e, "core: ingestion refusée"),
        }
    }

    // ---- Auto-test réseau (D36, `diagnostics.selftest`) ----

    /// Auto-test réseau borné : photographie de l'état (NAT, mapping,
    /// consensus, DHT), sonde des pairs d'amorçage effectifs et d'UN relais
    /// candidat. Chaque sonde est un `connect` idempotent suivi d'une courte
    /// attente de session — au pire quelques secondes au total, jamais
    /// bloquant pour le reste du nœud (appelé depuis le service API).
    pub(crate) async fn run_self_test(&self) -> SelfTestReport {
        let port = self.endpoint.local_addr().port();
        let nat = self.nat.snapshot();
        let consensus = self.observed_consensus();
        let eligible = relay::relay_eligible(consensus, port, nat.external);

        let mut bootstrap = Vec::new();
        for addr in self
            .all_bootstrap_peers()
            .into_iter()
            .take(SELFTEST_BOOTSTRAP_MAX)
        {
            let ok = self.probe_session(addr).await;
            bootstrap.push(ProbeResult {
                addr: addr.to_string(),
                ok,
            });
        }

        // Relais candidat : le premier relais annoncé joignable en principe
        // (relais domicile de soi-même — même dérivation que le rendez-vous
        // de premier contact). Aucun candidat : rien à sonder.
        let me = self.node.public_key();
        let relay_probe = match self
            .home_relays_of(&me)
            .into_iter()
            .find_map(|info| info.addrs.first().map(|a| a.0))
        {
            Some(addr) => Some(ProbeResult {
                addr: addr.to_string(),
                ok: self.probe_session(addr).await,
            }),
            None => None,
        };

        SelfTestReport {
            p2p_port: port,
            nat_kind: self.nat_kind(),
            port_mapping: nat.method,
            external_addr: nat.external.map(|a| a.to_string()),
            observed_consensus: consensus.map(|a| a.to_string()),
            dht_nodes: self.dht.peer_count(),
            connected_peers: self.book_len(),
            relay_eligible: eligible,
            bootstrap,
            relay_probe,
            reachability: diagnostics::reachability(eligible, self.nat_kind()),
        }
    }

    /// Sonde une adresse : `connect` (idempotent — no-op si la session existe)
    /// puis attente courte que la session soit apprise au carnet. Vrai si une
    /// session est en place avant l'échéance.
    async fn probe_session(&self, addr: SocketAddr) -> bool {
        if self.is_peer_connected(&addr) {
            return true;
        }
        if self.endpoint.connect(addr).await.is_err() {
            return false;
        }
        for _ in 0..SELFTEST_PROBE_POLLS {
            if self.is_peer_connected(&addr) {
                return true;
            }
            tokio::time::sleep(SELFTEST_PROBE_POLL_WAIT).await;
        }
        self.is_peer_connected(&addr)
    }

    // ---- Fichiers (canal FILE) : service des pairs et téléchargements ----

    /// Verrou du coordinateur de téléchargements.
    fn files_lock(&self) -> std::sync::MutexGuard<'_, fetch::Coordinator> {
        self.files.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Vivier de sondage d'un téléchargement : pairs du CARNET uniquement
    /// (relations établies), borné. Confidentialité : ne JAMAIS sonder
    /// `GetManifest{racine privée}` auprès d'inconnus — une session vive avec
    /// un étranger (premier contact via relais domicile, membre de groupe non
    /// ami) n'est pas un candidat. L'indice (auteur) et les sources ayant
    /// annoncé `Have` pour CETTE racine sont ajoutés par le coordinateur,
    /// indépendamment de ce vivier.
    fn fetch_probe_peers(&self, cap: usize) -> Vec<[u8; 32]> {
        let book = self.book.lock().unwrap_or_else(|e| e.into_inner());
        probe_pool_from_book(book.by_pubkey.keys().copied(), cap)
    }

    /// Mémorise qu'un pair a demandé une racine cette session (destinataire
    /// légitime d'une annonce `Have` de cette racine). Auto-borné façon
    /// [`FILES_DEBIT_MAX_PAIRS`] : au-delà de [`FILES_REQUESTERS_MAX_ROOTS`]
    /// racines la table est vidée ; au-delà de
    /// [`FILES_REQUESTERS_MAX_PAR_ROOT`] demandeurs le plus ancien est évincé.
    fn note_file_requester(&self, root: [u8; 32], peer: [u8; 32]) {
        let mut map = self
            .files_requesters
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if map.len() > FILES_REQUESTERS_MAX_ROOTS {
            map.clear();
        }
        let peers = map.entry(root).or_default();
        if !peers.contains(&peer) {
            if peers.len() >= FILES_REQUESTERS_MAX_PAR_ROOT {
                peers.remove(0);
            }
            peers.push(peer);
        }
    }

    /// Pairs ayant demandé cette racine cette session (essaim connu).
    fn file_requesters_of(&self, root: &[u8; 32]) -> Vec<[u8; 32]> {
        self.files_requesters
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(root)
            .cloned()
            .unwrap_or_default()
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
        fixed_window_ok(fenetre, now, 1_000, FILES_REQS_PAR_S)
    }

    /// Anti-inondation des réponses entrantes de contrôle (`Have`, `NotFound`,
    /// `ManifestMsg`) : au plus [`FILES_RESP_PAR_S`] par pair et par seconde,
    /// silence au-delà. Distinct de [`Self::files_debit_ok`] (requêtes
    /// SERVIES) : les deux comptent des flux différents.
    fn files_resp_ok(&self, peer: &[u8; 32]) -> bool {
        let now = crate::node::now_ms();
        let mut debit = self
            .files_resp_debit
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if debit.len() > FILES_DEBIT_MAX_PAIRS {
            debit.clear();
        }
        let fenetre = debit.entry(*peer).or_insert((now, 0));
        fixed_window_ok(fenetre, now, 1_000, FILES_RESP_PAR_S)
    }

    /// Envoie un message FILE à un pair : session directe (liaison d'identité
    /// comme `deliver_core` — si le carnet pointe sur l'adresse d'un relais,
    /// l'envoi direct échoue proprement au lieu de sceller la requête sous la
    /// session du relais), puis circuit relais existant. Sans l'un ni l'autre,
    /// déclenche le repli de joignabilité détaché ([`Self::spawn_files_reach`])
    /// pour que les relances suivantes aient un chemin. Rend vrai si l'envoi
    /// est parti (sans garantie de livraison).
    async fn send_file(&self, to: &[u8; 32], msg: FileMsg) -> bool {
        let channel_msg = ChannelMsg::File(msg);
        if let Some(addr) = self.addr_of(to) {
            if self
                .endpoint
                .send_to(addr, Some(*to), &channel_msg)
                .await
                .is_ok()
            {
                return true;
            }
        }
        // Repli relais (SPEC §11.3) : sans ce chemin, aucun transfert de
        // fichier (émojis, stickers, avatars, pièces jointes) n'aboutit entre
        // deux pairs joignables uniquement via un circuit tunnelé (NAT).
        if let Some(circuit) = self.endpoint.circuit_for_peer(node_id_of(to)) {
            match self.endpoint.send_via_relay(circuit, &channel_msg).await {
                Ok(()) => return true,
                Err(e) => tracing::debug!(erreur = %e, "fichiers : envoi via relais impossible"),
            }
        }
        // Ni adresse ni circuit : l'envoi n'est PAS parti. On tente d'ouvrir
        // un chemin (présence DHT + circuit) en tâche détachée et cadencée —
        // le coordinateur, prévenu par l'appelant ([`fetch::Coordinator::
        // note_emission`]), libérera vite la place si rien ne part.
        tracing::debug!("fichiers : pair injoignable (ni adresse ni circuit)");
        self.spawn_files_reach(*to);
        false
    }

    /// Déclenche, détaché et cadencé par pair ([`FILES_REPLI_MIN_MS`]), le
    /// repli de joignabilité des transferts : résolution de présence DHT
    /// (adresse de repli au carnet, comme la passe de maintenance) puis
    /// repli relais idempotent ([`Self::ensure_relay_to`]). Miroir du chemin
    /// complet de `deliver_core`/maintenance pour le canal FILE.
    fn spawn_files_reach(&self, peer: [u8; 32]) {
        let now = crate::node::now_ms();
        {
            let mut repli = self.files_repli.lock().unwrap_or_else(|e| e.into_inner());
            match repli.get(&peer) {
                Some(&last) if now.saturating_sub(last) < FILES_REPLI_MIN_MS => return,
                _ => {
                    // Éviction LRU (plus ancien repli) au lieu de tout vider :
                    // préserve la cadence des autres pairs sous pression.
                    evict_oldest_if_full(&mut repli, FILES_DEBIT_MAX_PAIRS, &peer);
                    repli.insert(peer, now);
                }
            }
        }
        let Some(rt) = self.arc() else {
            return; // boucles non démarrées (tests sans réseau)
        };
        tokio::spawn(async move {
            let now = crate::node::now_ms();
            if let Some(record) = rt
                .dht
                .get(&*rt.rpc, maintenance::presence_key(&peer), now)
                .await
            {
                if let Ok(addrs) = maintenance::verify_presence_record(&peer, &record, now) {
                    // Cible de repli sans écraser une adresse déjà prouvée.
                    if let Some(addr) = addrs.first() {
                        rt.register_peer_if_absent(peer, *addr);
                    }
                }
            }
            rt.ensure_relay_to(peer).await;
        });
    }

    /// Exécute les demandes décidées par le coordinateur, puis lui signale,
    /// racine par racine, combien d'émissions ont réellement pu partir (un
    /// pair injoignable fait échouer vite le téléchargement au lieu de le
    /// laisser attendre des réponses jamais émises).
    async fn send_file_actions(&self, actions: Vec<fetch::Action>) {
        if actions.is_empty() {
            return;
        }
        let mut issues: HashMap<[u8; 32], (usize, usize)> = HashMap::new();
        for action in actions {
            let (to, root, msg) = match action {
                fetch::Action::GetManifest { to, root } => {
                    (to, root, FileMsg::GetManifest { root })
                }
                fetch::Action::GetBlock { to, root, index } => {
                    (to, root, FileMsg::GetBlock { root, index })
                }
            };
            let parti = self.send_file(&to, msg).await;
            let (envoyees, tentees) = issues.entry(root).or_insert((0, 0));
            *tentees += 1;
            if parti {
                *envoyees += 1;
            }
        }
        let now = crate::node::now_ms();
        let mut c = self.files_lock();
        for (root, (envoyees, tentees)) in issues {
            tracing::debug!(
                racine = %crate::hex::encode(&root[..4]),
                envoyees,
                tentees,
                "fichiers : requêtes émises"
            );
            c.note_emission(&root, envoyees, tentees, now);
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
                // Ce pair connaît la racine (il la demande) : seul destinataire
                // légitime d'une future annonce `Have` de cette racine.
                self.note_file_requester(root, static_pub);
                // Lecture + déchiffrement du manifest = travail bloquant : hors
                // du thread asynchrone (`spawn_blocking`) pour ne pas figer la
                // boucle réseau, awaitée en ligne (voir `event_loop`).
                let node = Arc::clone(&self.node);
                let served =
                    tokio::task::spawn_blocking(move || node.files_serve_manifest(&root)).await;
                let reply = match served {
                    Ok(Ok(Some(manifest))) => FileMsg::ManifestMsg { manifest },
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
                self.note_file_requester(root, static_pub);
                // Lecture disque + parité Reed-Solomon éventuelle = travail
                // bloquant : déporté hors du thread asynchrone (voir ci-dessus).
                let node = Arc::clone(&self.node);
                let served =
                    tokio::task::spawn_blocking(move || node.files_serve_block(&root, index)).await;
                let reply = match served {
                    Ok(Ok(Some(data))) => FileMsg::Block { root, index, data },
                    _ => FileMsg::NotFound { root, index },
                };
                self.send_file(&static_pub, reply).await;
            }
            // ---- Réponses de nos sources ----
            // `Have` et `ManifestMsg` (contrôle, débit légitime très faible —
            // une annonce par source, un manifest par téléchargement, ré-
            // sollicitation cadencée) sont anti-inondés par pair
            // ([`Self::files_resp_ok`]). `NotFound` ne l'est PAS : il libère un
            // créneau et réémet aussitôt une demande (non cadencé par tick),
            // donc son débit légitime peut être élevé face à une source
            // clairsemée — le brider throttlerait de vrais téléchargements ; il
            // s'auto-limite déjà (la source sans le bloc cesse vite d'être
            // sollicitée). `Block` (données en masse) n'est pas bridé non plus.
            FileMsg::ManifestMsg { manifest } => {
                if !self.files_resp_ok(&static_pub) {
                    return;
                }
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
                if !self.files_resp_ok(&static_pub) {
                    return;
                }
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
        // Plafond des médias auto-récupérés (avatar/bannière de profil) : un
        // manifest annonçant une taille supérieure au plafond persisté sur
        // l'intention est refusé et l'intention abandonnée (base ET
        // coordinateur). Empêche un pair malveillant de nous faire télécharger
        // un blob massif sous couvert de média de profil. Le plafond ne
        // s'applique qu'aux intentions posées via `files_fetch_media` : une
        // pièce jointe cliquée par l'utilisateur n'en a pas (bornée à
        // `MAX_FILE_SIZE`) et n'est jamais annulée par ce chemin.
        if let Some(max) = self.node.media_cap_of(&root) {
            if manifest.size > max {
                tracing::warn!(
                    taille = manifest.size,
                    plafond = max,
                    "fichiers : média auto-récupéré trop volumineux, abandon"
                );
                if let Err(e) = self.node.files_fetch_clear(&root) {
                    tracing::debug!(erreur = %e, "fichiers : intention média insoldable");
                }
                self.files_lock().finish(&root);
                return;
            }
        }
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
                // Blob complet : annonce `Have` aux seuls pairs qui ont
                // demandé cette racine (essaim connu) — ils y gagnent une
                // source secondaire, sans divulguer la racine à des tiers.
                self.announce_have(root, bitmap.clone().unwrap_or_default())
                    .await;
            }
        }
        if let Some(p) = emission {
            self.node
                .emit_file_progress(&root, p.done, p.total, p.complete);
        }
        self.send_file_actions(actions).await;
    }

    /// Annonce `Have{root}` à la complétion, UNIQUEMENT aux pairs qui ont
    /// demandé cette racine cette session ET encore en session vive, borné à
    /// [`FILES_HAVE_MAX_PAIRS`]. Confidentialité (sémantique « Have » de
    /// BitTorrent) : ces pairs connaissent déjà la racine, donc rien n'est
    /// divulgué ; un blob privé (photo de DM, avatar) n'est jamais annoncé à
    /// un tiers. Sans demandeur enregistré : on n'annonce à PERSONNE.
    /// `FileMsg::Have` est déjà décodé par les nœuds existants : aucun nouveau
    /// type filaire.
    async fn announce_have(&self, root: [u8; 32], bitmap: Vec<u8>) {
        let cibles = {
            let requesters = self.file_requesters_of(&root);
            if requesters.is_empty() {
                return;
            }
            let live = self.live.lock().unwrap_or_else(|e| e.into_inner());
            have_targets(&requesters, &live, FILES_HAVE_MAX_PAIRS)
        };
        for pair in cibles {
            let msg = FileMsg::Have {
                root,
                bitmap: bitmap.clone(),
            };
            self.send_file(&pair, msg).await;
        }
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
        // Un vivier plus large que la borne par téléchargement : quand des
        // pairs refusent (`NotFound`), le coordinateur pioche des remplaçants
        // — il borne lui-même à [`fetch::MAX_SOURCES`] par racine. Restreint
        // au carnet (relations établies) : ne pas divulguer une racine privée
        // à des inconnus en session vive.
        let pairs = self.fetch_probe_peers(fetch::MAX_SOURCES * 2);
        let (actions, abandons) = self.files_lock().tick(now, &pairs);
        self.send_file_actions(actions).await;
        for (root, progress) in abandons {
            // Abandon ≠ renoncement : l'intention persistée est REPORTÉE
            // (backoff par racine) et sera ré-adoptée — elle n'est soldée
            // qu'à la complétion.
            tracing::debug!("fichiers : téléchargement abandonné (retenté après backoff)");
            if let Err(e) = self.node.files_fetch_defer(&root, now) {
                tracing::debug!(erreur = %e, "fichiers : report de l'intention impossible");
            }
            self.node
                .emit_file_progress(&root, progress.done, progress.total, false);
        }
    }

    /// Adopte les intentions de téléchargement persistées : les fichiers déjà
    /// complets sont soldés, les autres démarrent (bornés par le
    /// coordinateur ; l'excédent attend une place). Les intentions en backoff
    /// (abandonnées récemment) attendent leur échéance.
    fn files_adopt_intents(&self, now: u64) {
        let intents = match self.node.files_fetch_intents() {
            Ok(v) => v,
            Err(e) => {
                tracing::debug!(erreur = %e, "fichiers : lecture des intentions impossible");
                return;
            }
        };
        for intent in intents {
            // Le backoff d'abandon existe pour ne pas marteler un pair
            // INJOIGNABLE. Si l'indice (le pair qui détient le média) a une
            // session VIVANTE en ce moment, l'attendre n'a pas de sens : on
            // court-circuite le report. Corrige le « profil/bannière jamais
            // reçus » à la reconnexion — le fetch, abandonné pendant la brève
            // fenêtre où la session n'était pas encore établie, était reporté
            // de 60 s (première échéance) bien au-delà de la fenêtre de
            // co-présence, alors que l'ami est là.
            let indice_vivant = intent.hint.is_some_and(|h| self.is_peer_live(&h));
            if intent.next_attempt_ms > now && !indice_vivant {
                continue;
            }
            let (root, hint) = (intent.merkle_root, intent.hint);
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

    /// À la connexion d'un pair : (a) réarme immédiatement les intentions en
    /// backoff dont il est l'indice (la boucle des transferts les ré-adopte à
    /// la passe suivante), et (b) pour un ami, relance la récupération de
    /// l'avatar et de la bannière de son profil stockés mais absents en local
    /// — sans quoi un raté ne serait retenté qu'à sa prochaine ré-annonce de
    /// profil (30 minutes).
    fn files_on_peer_connected(&self, peer: &[u8; 32]) {
        if let Err(e) = self.node.files_retry_hinted(peer) {
            tracing::debug!(erreur = %e, "fichiers : relance des intentions impossible");
        }
        if !self.is_friend(peer) {
            return;
        }
        let node_id = node_id_of(peer).0;
        let Ok(profil) = self.node.peer_public_profile(&node_id) else {
            return;
        };
        for racine in profil.avatar.iter().chain(profil.banner.iter()) {
            if let Ok(None) = self.node.files_local_path(racine) {
                // Média auto-récupéré : plafonné (anti-DoS taille). Repeuple
                // aussi le plafond en mémoire après un redémarrage.
                let _ = self.node.files_fetch_media(racine, Some(*peer));
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
        // On interroge alors directement les pairs d'amorçage EFFECTIFS (défaut
        // + utilisateur) via FIND_VALUE, en réutilisant leur service DHT et la
        // vérification du record d'identité. Inclure les nœuds par défaut est ce
        // qui permet de résoudre le code d'un invité NATé sans rendez-vous
        // configuré manuellement.
        for addr in self.all_bootstrap_peers() {
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

    fn peer_links(&self) -> Vec<PeerLink> {
        let friends = self.node.friend_pubkeys().unwrap_or_default();
        let now = crate::node::now_ms();
        // Vue des sessions par pair : la session DIRECTE la plus fraîche prime
        // (même règle que `direct_session_addr` — après un redémarrage
        // silencieux, la session cadavre coexiste 2 min avec la fraîche),
        // une session relayée sert de repli d'affichage.
        let mut sessions: HashMap<[u8; 32], accord_transport::SessionView> = HashMap::new();
        for view in self.endpoint.session_views() {
            match sessions.get(&view.peer_static) {
                Some(cur) => {
                    let cur_direct = cur.relay_circuit.is_none();
                    let new_direct = view.relay_circuit.is_none();
                    let remplace = match (cur_direct, new_direct) {
                        (false, true) => true,
                        (true, false) => false,
                        _ => view.last_recv_ms > cur.last_recv_ms,
                    };
                    if remplace {
                        sessions.insert(view.peer_static, view);
                    }
                }
                None => {
                    sessions.insert(view.peer_static, view);
                }
            }
        }
        let delivered = self
            .last_delivery
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        friends
            .into_iter()
            .map(|pk| {
                let session = sessions.get(&pk);
                let (transport, relay) = match session {
                    Some(s) if s.relay_circuit.is_none() => (LinkTransport::Direct, None),
                    Some(s) => (LinkTransport::Relay, Some(s.addr.to_string())),
                    None => (LinkTransport::None, None),
                };
                PeerLink {
                    pubkey: crate::hex::encode(&pk),
                    live: self.is_peer_live(&pk),
                    addr: self.addr_of(&pk).map(|a| a.to_string()),
                    transport,
                    relay,
                    last_recv_age_ms: session.map(|s| now.saturating_sub(s.last_recv_ms)),
                    rtt_ms: session.and_then(|s| s.last_rtt_ms),
                    last_delivery_ms: delivered.get(&pk).copied(),
                }
            })
            .collect()
    }

    fn counters(&self) -> diagnostics::CountersSnapshot {
        self.counters.snapshot()
    }

    async fn self_test(&self) -> SelfTestReport {
        self.run_self_test().await
    }
}

/// `NodeInfo` synthétique pour un RPC DHT direct vers une adresse : seule
/// `addrs` est utilisée par le transport ; l'identité du pair est inconnue au
/// démarrage (le récepteur vérifie l'authenticité des records qu'il rend).
/// Poinçonnage TCP par ouverture simultanée (SPEC §11.3, best-effort). À
/// chaque ronde, tente `connect()` vers TOUS les candidats en parallèle depuis
/// le port P2P local (partagé avec l'écouteur TCP via `SO_REUSEPORT`) : les
/// SYN sortants ouvrent les mappings NAT pendant que ceux du pair — qui
/// exécute la même procédure au même moment (coordination §11.2) — tentent de
/// les traverser. La première connexion établie est adoptée comme lien
/// datagramme, puis le handshake de session chiffrée est rejoué à travers elle
/// (mêmes validations, PoW et liaison d'identité que sur UDP).
///
/// Limite documentée : sans observation du port TCP public du pair, chaque
/// côté vise le port ANNONCÉ (celui de l'écouteur) — la traversée réussit
/// surtout avec des NAT préservant le port, une redirection, ou quand seul
/// UDP est filtré. Sinon, le relais (SPEC §11.3) reste en place.
async fn tcp_punch_toward(
    endpoint: Arc<Endpoint>,
    links: Arc<TcpLinks>,
    friend: [u8; 32],
    candidates: &[SocketAddr],
) {
    let local_port = endpoint.local_addr().port();
    if local_port == 0 {
        return;
    }
    for _ in 0..TCP_PUNCH_ROUNDS {
        if endpoint.has_direct_session_with(&friend) {
            return; // l'UDP (ou le lien TCP entrant du pair) a fini par passer
        }
        let mut set = tokio::task::JoinSet::new();
        for cand in candidates {
            let cand = *cand;
            set.spawn(async move { tcp::punch_connect(local_port, cand, TCP_PUNCH_TIMEOUT).await });
        }
        while let Some(res) = set.join_next().await {
            let Ok(Ok(stream)) = res else { continue };
            let Ok(peer) = links.adopt(stream) else {
                continue; // plafond de liens atteint : on n'insiste pas
            };
            set.abort_all();
            let cand = Candidate {
                addr: peer,
                kind: accord_transport::nat::CandidateKind::HolePunch,
            };
            if let Err(e) = endpoint.punch(&[cand], friend).await {
                tracing::debug!(erreur = %e, "poinçonnage TCP : handshake échoué");
            }
            return;
        }
        tokio::time::sleep(TCP_PUNCH_ROUND_WAIT).await;
    }
}

/// Cibles d'une annonce `Have` à la complétion d'un blob : intersection des
/// DEMANDEURS de cette racine (essaim connu) et des sessions vives, bornée
/// (anti-inondation) et en ordre stable. Un pair qui n'a pas demandé la
/// racine — inconnu ou non — n'est jamais ciblé. Pure, testable.
/// Vivier de sondage à partir des seuls pairs du carnet, borné. Isolé pour le
/// test : la propriété de sécurité est qu'un pair en session vive mais absent
/// du carnet (inconnu) ne peut PAS y figurer (l'entrée ne provient que du
/// carnet). Pure, testable.
fn probe_pool_from_book(book_peers: impl Iterator<Item = [u8; 32]>, cap: usize) -> Vec<[u8; 32]> {
    book_peers.take(cap).collect()
}

fn have_targets(requesters: &[[u8; 32]], live: &HashSet<[u8; 32]>, cap: usize) -> Vec<[u8; 32]> {
    let mut cibles: Vec<[u8; 32]> = requesters
        .iter()
        .copied()
        .filter(|p| live.contains(p))
        .collect();
    cibles.sort_unstable();
    cibles.dedup();
    cibles.truncate(cap);
    cibles
}

/// Débit à fenêtre fixe : réarme la fenêtre si `window_ms` est écoulé, puis
/// incrémente le compteur ; rend `true` tant que le compte reste dans `max`.
/// Pure, testable — partagée par les débits « requêtes servies » et
/// « réponses entrantes ».
fn fixed_window_ok(win: &mut (u64, u32), now: u64, window_ms: u64, max: u32) -> bool {
    if now.saturating_sub(win.0) >= window_ms {
        *win = (now, 0);
    }
    win.1 += 1;
    win.1 <= max
}

/// Éviction LRU d'une table `pair -> horodatage` pleine : si `key` n'y est pas
/// déjà et que la table a atteint `cap`, retire l'entrée la PLUS ANCIENNE
/// (plus petit horodatage) pour faire de la place — au lieu de tout vider, ce
/// qui effacerait d'un coup la cadence de tous les pairs (un attaquant
/// pourrait alors la remettre à zéro à volonté). Pure, testable.
fn evict_oldest_if_full(map: &mut HashMap<[u8; 32], u64>, cap: usize, key: &[u8; 32]) {
    if map.len() < cap || map.contains_key(key) {
        return;
    }
    if let Some((&oldest, _)) = map.iter().min_by_key(|(_, &t)| t) {
        map.remove(&oldest);
    }
}

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

    #[test]
    fn annonce_have_ciblee_demandeurs_intersection_vives_en_ordre_stable() {
        let demandeurs: Vec<[u8; 32]> = (0..5u8).map(|i| [i; 32]).collect();
        let live: HashSet<[u8; 32]> = (0..5u8).map(|i| [i; 32]).collect();
        // Ordre stable (trié) malgré l'itération non déterministe du set.
        assert_eq!(
            have_targets(&demandeurs, &live, 16),
            (0..5u8).map(|i| [i; 32]).collect::<Vec<_>>()
        );
        // Borne anti-inondation respectée.
        assert_eq!(have_targets(&demandeurs, &live, 2).len(), 2);
        // Confidentialité : un pair vif qui n'a PAS demandé la racine n'est
        // jamais ciblé (inconnu via relais domicile, membre hors audience).
        let live_avec_inconnu: HashSet<[u8; 32]> = (0..8u8).map(|i| [i; 32]).collect();
        assert_eq!(
            have_targets(&demandeurs, &live_avec_inconnu, 16),
            (0..5u8).map(|i| [i; 32]).collect::<Vec<_>>(),
            "seuls les demandeurs sont ciblés, pas les autres sessions vives"
        );
        // Un demandeur hors ligne (absent des vives) n'est pas ciblé.
        assert!(have_targets(&demandeurs, &HashSet::new(), 16).is_empty());
        // Aucun demandeur : aucune annonce.
        assert!(have_targets(&[], &live, 16).is_empty());
    }

    #[test]
    fn debit_fenetre_fixe_refuse_la_rafale_puis_reprend_a_la_fenetre_suivante() {
        // Quota de 3 par fenêtre de 1 000 ms.
        let mut win = (0u64, 0u32);
        assert!(fixed_window_ok(&mut win, 0, 1_000, 3));
        assert!(fixed_window_ok(&mut win, 100, 1_000, 3));
        assert!(fixed_window_ok(&mut win, 200, 1_000, 3));
        // 4e dans la même fenêtre : refusée.
        assert!(!fixed_window_ok(&mut win, 300, 1_000, 3));
        assert!(!fixed_window_ok(&mut win, 999, 1_000, 3));
        // Fenêtre suivante : le quota repart.
        assert!(fixed_window_ok(&mut win, 1_000, 1_000, 3));
        assert!(fixed_window_ok(&mut win, 1_100, 1_000, 3));
    }

    #[test]
    fn eviction_lru_retire_le_plus_ancien_et_preserve_les_autres() {
        let mut map: HashMap<[u8; 32], u64> = HashMap::new();
        map.insert([1; 32], 10); // le plus ancien
        map.insert([2; 32], 20);
        map.insert([3; 32], 30);
        // Table pleine (cap 3), insertion d'une NOUVELLE clé : évince [1] (10).
        evict_oldest_if_full(&mut map, 3, &[9; 32]);
        assert_eq!(map.len(), 2);
        assert!(!map.contains_key(&[1; 32]), "le plus ancien est évincé");
        assert!(map.contains_key(&[2; 32]) && map.contains_key(&[3; 32]));
    }

    #[test]
    fn eviction_lru_nop_si_cle_deja_presente_ou_table_non_pleine() {
        let mut map: HashMap<[u8; 32], u64> = HashMap::new();
        map.insert([1; 32], 10);
        map.insert([2; 32], 20);
        // Table non pleine (cap 3) : aucune éviction.
        evict_oldest_if_full(&mut map, 3, &[9; 32]);
        assert_eq!(map.len(), 2);
        // Table pleine mais la clé y est déjà (rafraîchissement) : pas d'éviction.
        map.insert([3; 32], 30);
        evict_oldest_if_full(&mut map, 3, &[1; 32]);
        assert_eq!(map.len(), 3, "un pair déjà connu n'évince personne");
    }
}
