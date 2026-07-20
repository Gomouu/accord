//! Endpoint transport : pilote les handshakes, les sessions chiffrées, le
//! keep-alive et l'anti-DoS au-dessus d'un [`DatagramSocket`] (SPEC §1, §2).
//!
//! L'endpoint est orienté adresse : les couches supérieures (DHT, core)
//! associent `NodeId → SocketAddr`. Chaque message applicatif est chiffré dans
//! une session ; les paquets HELLO/WELCOME/COOKIE sont les seuls à en-tête
//! partiellement clair.

use crate::clock::Clock;
use crate::error::TransportError;
use crate::frag::{self, Reassembler};
use crate::nat::Candidate;
use crate::ratelimit::{Bucket, RateLimiter};
use crate::relay::{RelayDecision, RelayTable, DEFAULT_RELAY_CAP_BPS};
use crate::socket::DatagramSocket;
use accord_crypto::handshake::{respond, CookieJar, Initiator, NonceCache};
use accord_crypto::identity::{node_id_of, verify_pow};
use accord_crypto::{Identity, SessionCrypto};
use accord_proto::envelope::{CookiePacket, Hello, Packet};
use accord_proto::plaintext::{ChannelMsg, RelayMsg};
use accord_proto::types::NodeId;
use accord_proto::{ControlMsg, WireDecode, WireEncode};
use rand::rngs::OsRng;
use rand::RngCore;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use subtle::ConstantTimeEq;
use tokio::sync::{mpsc, oneshot};

/// Nombre de salves de HELLO simultanées émises lors d'un poinçonnage
/// coordonné (SPEC §11.2 : « 5 tentatives »).
const PUNCH_ATTEMPTS: u32 = 5;
/// Intervalle entre deux salves de poinçonnage (SPEC §11.2 : « 200 ms
/// d'intervalle »).
const PUNCH_INTERVAL: std::time::Duration = std::time::Duration::from_millis(200);

// --- Codes de refus d'ouverture de circuit relais (canal RELAY, SPEC §10) ---
/// Le nœud sollicité n'assure pas le service de relais (`relay_serving == false`).
const REJECT_NOT_RELAY: u8 = 0x01;
/// Le relais n'a aucune session active avec la cible demandée.
const REJECT_NO_TARGET: u8 = 0x02;
/// La table de circuits du relais est pleine (`MAX_CIRCUITS` atteint).
const REJECT_FULL: u8 = 0x03;

/// Plafond du nombre de circuits relais dont ce nœud est extrémité CLIENTE
/// (miroir de [`crate::relay::MAX_CIRCUITS`] côté serveur). Borne l'empreinte
/// mémoire de `client_circuits` — en particulier les circuits ouverts par des
/// HELLO tunnelés entrants, insérés AVANT tout PoW/rate-limit (FAILLE C).
const MAX_CLIENT_CIRCUITS: usize = crate::relay::MAX_CIRCUITS;

/// Capacité (rafale) du seau de messages de contrôle changeant l'état par
/// session : couvre l'annonce initiale, quelques ré-annonces et les
/// observations d'adresse d'un cycle de présence sans jamais bloquer un pair
/// honnête.
const CTRL_MSG_BURST: f64 = 8.0;
/// Recharge du seau de contrôle (messages/s) : au-delà, les messages
/// excédentaires sont ignorés silencieusement. À 1/s, un pair hostile qui
/// inonde est ramené à un filet négligeable (≈ 1 insertion de table/s),
/// trivialement absorbé, tout en laissant passer le trafic légitime (une
/// poignée de messages par minute au plus).
const CTRL_MSG_REFILL_PER_S: f64 = 1.0;

/// Délai au-delà duquel un circuit client dont le handshake bout-en-bout n'a
/// jamais abouti (`session_id == None`) est balayé par la maintenance (FAILLE C).
/// Large devant la latence d'un handshake tunnelé nominal — pour ne pas
/// interrompre un établissement lent — mais fini, afin de borner l'accumulation
/// d'entrées inachevées créées par un flux de faux HELLO.
const CLIENT_CIRCUIT_HANDSHAKE_TIMEOUT_MS: u64 = 30_000;

/// Délai d'attente de la réponse `Accept`/`Reject` d'un relais après un `Open`
/// (temps réel). Passé ce délai, l'ouverture est abandonnée : un relais
/// silencieux ne peut plus faire pendre l'appelant ni faire fuir l'entrée
/// `pending_relay_open`.
const RELAY_OPEN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// Événement remonté aux couches supérieures.
#[derive(Debug)]
pub enum TransportEvent {
    /// Session établie avec un pair (handshake réussi).
    Connected {
        /// NodeId du pair.
        node: NodeId,
        /// Adresse du pair.
        addr: SocketAddr,
        /// Clé publique Ed25519 du pair.
        static_pub: [u8; 32],
    },
    /// Message applicatif déchiffré reçu d'un pair.
    Message {
        /// NodeId de l'émetteur.
        from: NodeId,
        /// Adresse de l'émetteur.
        addr: SocketAddr,
        /// Clé publique Ed25519 de l'émetteur.
        static_pub: [u8; 32],
        /// Message démultiplexé par canal (boxé : bien plus gros que les
        /// autres variantes de l'énumération).
        msg: Box<ChannelMsg>,
    },
    /// Le pair a demandé une observation d'adresse ; l'endpoint y a répondu.
    ObservedByPeer {
        /// Adresse telle que vue localement pour ce pair.
        addr: SocketAddr,
    },
    /// Un pair nous a communiqué notre adresse publique observée (SPEC §11.1),
    /// sur une session DIRECTE authentifiée. Porte l'identité de l'OBSERVATEUR
    /// pour que l'agrégat de consensus déduplique les votes par pair (un même
    /// pair ne peut pas fabriquer un consensus à lui seul).
    ObservedAddr {
        /// Clé publique Ed25519 du pair qui rapporte l'observation.
        observer: [u8; 32],
        /// Notre adresse vue par ce pair.
        observed: SocketAddr,
    },
    /// Un pair demande un poinçonnage coordonné vers ses candidats
    /// (SPEC §11.2). La POLITIQUE (amitié requise, bornage, cadence, réponse)
    /// appartient aux couches hautes : le transport se contente de remonter.
    PunchRequested {
        /// Clé publique Ed25519 du demandeur (session authentifiée).
        static_pub: [u8; 32],
        /// Jeton à répéter dans la réponse.
        token: u64,
        /// Candidats d'adresse du demandeur (déjà bornés au décodage).
        candidates: Vec<SocketAddr>,
    },
    /// Un pair répond à notre demande de poinçonnage avec ses candidats.
    PunchResponded {
        /// Clé publique Ed25519 du répondeur (session authentifiée).
        static_pub: [u8; 32],
        /// Jeton de la demande d'origine (corrélation côté appelant).
        token: u64,
        /// Candidats d'adresse du répondeur (déjà bornés au décodage).
        candidates: Vec<SocketAddr>,
    },
    /// Un pair s'est auto-annoncé dans une session DIRECTE (SPEC §11.3,
    /// `NODE_ANNOUNCE`) : de quoi construire un `NodeInfo` vérifiable —
    /// identité authentifiée par la session, adresse OBSERVÉE (jamais
    /// déclarée), preuve de travail à re-vérifier par la couche DHT.
    NodeAnnounced {
        /// Clé publique Ed25519 du pair (session authentifiée).
        static_pub: [u8; 32],
        /// Adresse source observée de la session directe.
        addr: SocketAddr,
        /// Nonce de preuve de travail annoncé.
        pow_nonce: u64,
        /// Drapeaux de capacité annoncés.
        flags: u8,
    },
    /// Session fermée (CLOSE reçu ou expiration).
    Disconnected {
        /// Adresse du pair.
        addr: SocketAddr,
    },
    /// Un relais a accepté l'ouverture d'un circuit demandé par ce nœud
    /// (rôle initiateur/client, SPEC §10). Consommé par la logique client,
    /// implémentée dans un incrément ultérieur.
    RelayAccepted {
        /// Identifiant de circuit attribué par le relais.
        circuit: u32,
        /// NodeId du relais qui a accepté.
        relay: NodeId,
    },
    /// Un relais a refusé l'ouverture d'un circuit demandé par ce nœud
    /// (rôle initiateur/client, SPEC §10).
    RelayRejected {
        /// Code de refus (voir `REJECT_*`).
        code: u8,
        /// NodeId du relais qui a refusé.
        relay: NodeId,
    },
}

struct Session {
    crypto: SessionCrypto,
    peer_addr: SocketAddr,
    peer_static: [u8; 32],
    peer_node: NodeId,
    last_recv_ms: u64,
    last_send_ms: u64,
    /// Identifiant du prochain message fragmenté émis dans cette session.
    next_frag_id: u32,
    /// Réassembleur des messages fragmentés reçus (borné anti-DoS).
    reasm: Reassembler,
    /// `Some(circuit)` si cette session bout-en-bout transite par un circuit
    /// relais (SPEC §11.3) plutôt que par un lien direct. Une session relayée a
    /// `peer_addr = relay_addr` (l'adresse du relais) mais n'est PAS indexée dans
    /// `id_by_addr` (cette clé est déjà prise par la session avec le relais) :
    /// elle est retrouvée par `session_id` (paquets DATA) et par `circuit`
    /// (table `client_circuits`). Les ENVOIS vers ce pair sont ré-enveloppés dans
    /// `RelayMsg::Data{circuit, blob}` (voir `send_packet_via_link`).
    relay_circuit: Option<u32>,
    /// Vrai une fois notre `NODE_ANNOUNCE` émis dans cette session : borne
    /// l'échange d'annonces à une par session et par sens (un pair qui
    /// re-demande n'obtient pas de nouvelle réponse). Toujours vrai sur une
    /// session tunnelée (aucune annonce n'y circule : l'adresse observée
    /// serait celle du relais).
    announced: bool,
    /// Seau à jetons des messages de contrôle CHANGEANT L'ÉTAT côté récepteur
    /// (`NODE_ANNOUNCE`, `OBSERVE_ADDR_RESP`) : bornage post-handshake par
    /// session (donc par identité authentifiée). Sans lui, un pair déjà
    /// authentifié pourrait inonder ces messages à plein débit UDP — chaque
    /// remontée déclenchant une insertion de table de routage et un événement
    /// sur un canal non borné (contention de verrous + croissance mémoire).
    /// Le débit légitime est minuscule (une annonce initiale, de rares
    /// ré-annonces, ~3 observations par cycle de présence) : la capacité
    /// couvre les rafales normales, la recharge lente écrête tout flood.
    ctrl_bucket: Bucket,
    /// Dernier PING keep-alive émis en attente de PONG : `(jeton, émis_ms)`.
    /// Sert à la mesure de latence locale ([`Session::last_rtt_ms`]) sans
    /// aucun octet nouveau sur le fil (le PING/PONG keep-alive existe depuis
    /// la 1.0). Un PONG au jeton inattendu (rejoué, forgé, croisé avec un
    /// `connect`) est simplement ignoré.
    ping_pending: Option<(u64, u64)>,
    /// Dernier aller-retour mesuré (ms) sur un PONG corrélé au keep-alive.
    /// Purement local (diagnostic) ; `None` tant qu'aucun cycle n'a abouti.
    last_rtt_ms: Option<u64>,
}

/// Photographie d'une session établie, exposée à la couche nœud pour le
/// diagnostic de connectivité par pair (D4/D35) : lien direct ou tunnelé,
/// fraîcheur du dernier trafic entrant, latence estimée. Aucune donnée
/// applicative ni clé n'y figure.
#[derive(Debug, Clone, Copy)]
pub struct SessionView {
    /// Clé publique Ed25519 du pair (session authentifiée).
    pub peer_static: [u8; 32],
    /// Adresse de transport : celle du pair en direct, celle du RELAIS pour
    /// une session tunnelée.
    pub addr: SocketAddr,
    /// `Some(circuit)` si la session transite par un circuit relais.
    pub relay_circuit: Option<u32>,
    /// Horodatage (ms, horloge du nœud) du dernier trafic entrant.
    pub last_recv_ms: u64,
    /// Dernier aller-retour keep-alive mesuré (ms), si un cycle a abouti.
    pub last_rtt_ms: Option<u64>,
}

/// Préfixe hexadécimal court (4 octets) d'une clé publique, pour les logs.
fn hex4(pubkey: &[u8; 32]) -> String {
    pubkey[..4].iter().map(|b| format!("{b:02x}")).collect()
}

/// Désigne le lien par lequel joindre un pair : soit un datagramme direct, soit
/// un circuit relais. C'est l'abstraction d'envoi commune (point 4 de
/// l'architecture §11.3) : un paquet transport brut (HELLO/WELCOME/DATA) est émis
/// tel quel en direct, ou ré-enveloppé dans `RelayMsg::Data` et scellé sous la
/// session avec le relais pour un tunnel. Le même lien sert à router les réponses
/// (WELCOME, PONG…) sur le chemin d'où le paquet est arrivé.
#[derive(Clone, Copy)]
enum PeerLink {
    /// Lien direct : adresse UDP du pair.
    Direct(SocketAddr),
    /// Tunnel relais : `relay_addr` héberge le circuit `circuit`.
    Tunnel {
        relay_addr: SocketAddr,
        circuit: u32,
    },
}

impl PeerLink {
    /// Adresse de transport associée : celle du pair en direct, celle du relais
    /// en tunnel (utilisée comme `peer_addr` et remontée dans les événements).
    fn addr(self) -> SocketAddr {
        match self {
            PeerLink::Direct(addr) => addr,
            PeerLink::Tunnel { relay_addr, .. } => relay_addr,
        }
    }

    /// Circuit relais du lien, le cas échéant.
    fn circuit(self) -> Option<u32> {
        match self {
            PeerLink::Direct(_) => None,
            PeerLink::Tunnel { circuit, .. } => Some(circuit),
        }
    }
}

/// Circuit relais dont CE nœud est une extrémité CLIENTE (SPEC §11.3) : la
/// session bout-en-bout avec `peer_*` passe à travers le relais `relay_*`. Table
/// distincte de `State.relay` (circuits qu'on héberge en tant que relais) : c'est
/// cette séparation qui lève la limitation « un nœud servant ne peut être
/// extrémité d'un circuit » — les deux rôles vivent dans des tables disjointes.
struct ClientCircuit {
    /// Adresse du relais qui héberge le circuit.
    relay_addr: SocketAddr,
    /// NodeId du relais.
    relay_node: NodeId,
    /// Clé statique Ed25519 du pair distant (intention d'ouverture).
    peer_static: [u8; 32],
    /// NodeId du pair distant (dérivé de `peer_static`).
    peer_node: NodeId,
    /// `session_id` de la session bout-en-bout une fois le handshake terminé.
    session_id: Option<[u8; 8]>,
    /// Horodatage de création (ms). Sert au balayage anti-DoS : une entrée qui
    /// reste `session_id: None` au-delà de [`CLIENT_CIRCUIT_HANDSHAKE_TIMEOUT_MS`]
    /// (handshake jamais abouti) est purgée par la maintenance (FAILLE C).
    created_ms: u64,
}

/// Ouverture de circuit en attente de la réponse `Accept`/`Reject` du relais.
/// Indexée par `(relay_node, peer_node)` dans `pending_relay_open`.
struct PendingOpen {
    /// Clé statique du pair visé (pour construire le `ClientCircuit` à l'Accept).
    peer_static: [u8; 32],
    /// Canal résolu par `handle_relay` : `Ok(circuit)` sur Accept, `Err(code)`
    /// sur Reject.
    tx: oneshot::Sender<Result<u32, u8>>,
}

struct Pending {
    initiator: Initiator,
    /// Identité Ed25519 attendue à cette adresse, si l'ouverture vise un pair
    /// précis (livraison CORE liée). `None` pour un pair quelconque (DHT) :
    /// aucune liaison n'est alors imposée. Le WELCOME établi est refusé si sa
    /// clé statique diffère de cette cible (voir `on_welcome`).
    expected_static: Option<[u8; 32]>,
    queued: Vec<Vec<u8>>,
    attempts: u32,
    last_send_ms: u64,
}

struct State {
    nonce_cache: NonceCache,
    cookie_jar: CookieJar,
    sessions_by_id: HashMap<[u8; 8], Session>,
    id_by_addr: HashMap<SocketAddr, [u8; 8]>,
    pending: HashMap<SocketAddr, Pending>,
    hs_rate: RateLimiter,
    hs_seen_in_window: u32,
    window_start_ms: u64,
    /// Circuits de relais hébergés par ce nœud (SPEC §10). Placée dans `State`
    /// (déjà sous `Mutex`) plutôt que dans un verrou dédié : un unique verrou
    /// garde atomiquement circuits ET sessions, ce qui supprime tout risque
    /// d'ordre de verrouillage lorsqu'un acheminement doit à la fois décider via
    /// la table et résoudre l'adresse de session de la cible. Le verrou n'est
    /// jamais tenu pendant un `await` réseau : on calcule sous verrou, on relâche,
    /// puis on émet (même discipline que le reste de l'endpoint).
    relay: RelayTable,
    /// Circuits dont ce nœud est une extrémité CLIENTE (SPEC §11.3), indexés par
    /// identifiant de circuit. Séparé de `relay` (circuits hébergés). Sert à la
    /// réinjection entrante (un `RelayMsg::Data` sur un circuit connu ici est un
    /// paquet A↔B tunnelé, pas du trafic à héberger) et au routage des envois.
    client_circuits: HashMap<u32, ClientCircuit>,
    /// Ouvertures de circuit en attente d'`Accept`/`Reject`, indexées par
    /// `(relay_node, peer_node)`. Résolues dans `handle_relay`.
    pending_relay_open: HashMap<(NodeId, NodeId), PendingOpen>,
    /// Handshakes initiateur en cours À TRAVERS un tunnel, indexés par circuit.
    /// Le `pending` classique (indexé par adresse) ne convient pas : `relay_addr`
    /// y est déjà occupé par la session avec le relais.
    relay_pending: HashMap<u32, Pending>,
}

/// Configuration de l'endpoint.
#[derive(Debug, Clone, Copy)]
pub struct EndpointConfig {
    /// Difficulté PoW exigée des pairs.
    pub pow_bits: u32,
    /// Intervalle de keep-alive UDP (ms).
    pub keepalive_ms: u64,
    /// Inactivité avant fermeture de session (ms).
    pub idle_timeout_ms: u64,
    /// Seuil de HELLO/s au-delà duquel les cookies anti-DoS sont exigés.
    pub cookie_pressure_per_s: u32,
    /// Active le service de relais (SPEC §10) : ce nœud n'accepte d'acheminer du
    /// trafic pour des tiers que si ce drapeau est vrai. Le nœud ne l'active que
    /// lorsqu'il se sait publiquement joignable (hors périmètre ici). Faux par
    /// défaut : un nœud n'est jamais relais à son insu (limitation de la surface
    /// d'abus).
    pub relay_serving: bool,
}

impl Default for EndpointConfig {
    fn default() -> Self {
        Self {
            pow_bits: accord_proto::limits::IDENTITY_POW_BITS,
            keepalive_ms: 25_000,
            idle_timeout_ms: 120_000,
            cookie_pressure_per_s: 64,
            relay_serving: false,
        }
    }
}

/// Endpoint transport partagé (clonable via `Arc`).
pub struct Endpoint {
    socket: Arc<dyn DatagramSocket>,
    identity: Arc<Identity>,
    clock: Arc<dyn Clock>,
    config: EndpointConfig,
    state: Mutex<State>,
    events: mpsc::UnboundedSender<TransportEvent>,
    shutdown: AtomicBool,
    /// Drapeaux de capacité annoncés dans `NODE_ANNOUNCE` (SPEC §11.3). Mis à
    /// jour par la couche nœud quand l'éligibilité relais change (voir
    /// `accord-node::node::relay::relay_eligible`) ; 0 au démarrage — un nœud
    /// ne s'annonce jamais relais tant que sa joignabilité publique n'est pas
    /// établie.
    local_flags: AtomicU8,
}

impl Endpoint {
    /// Verrou de l'état, robuste à l'empoisonnement : la boucle réseau est le
    /// seul chemin de livraison de TOUS les pairs — un panic ponctuel d'un
    /// autre thread (bug isolé) ne doit jamais condamner le transport entier
    /// en propageant l'empoisonnement (D23, zéro panic en production). Les
    /// invariants de l'état sont re-vérifiés à l'usage (session retrouvée par
    /// identifiant, tables nettoyées par la maintenance).
    fn state_lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Crée un endpoint et le canal d'événements associé.
    pub fn new(
        socket: Arc<dyn DatagramSocket>,
        identity: Arc<Identity>,
        clock: Arc<dyn Clock>,
        config: EndpointConfig,
    ) -> (Arc<Self>, mpsc::UnboundedReceiver<TransportEvent>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let now = clock.now_ms();
        let ep = Arc::new(Self {
            socket,
            identity,
            clock,
            config,
            state: Mutex::new(State {
                nonce_cache: NonceCache::new(),
                cookie_jar: CookieJar::new(now),
                sessions_by_id: HashMap::new(),
                id_by_addr: HashMap::new(),
                pending: HashMap::new(),
                // SPEC §4 : 10 RPC/s régime, rafale 40 (réutilisé ici pour
                // borner les handshakes par IP).
                hs_rate: RateLimiter::new(8.0, 2.0),
                hs_seen_in_window: 0,
                window_start_ms: now,
                relay: RelayTable::new(DEFAULT_RELAY_CAP_BPS),
                client_circuits: HashMap::new(),
                pending_relay_open: HashMap::new(),
                relay_pending: HashMap::new(),
            }),
            events: tx,
            shutdown: AtomicBool::new(false),
            local_flags: AtomicU8::new(0),
        });
        (ep, rx)
    }

    /// Drapeaux de capacité annoncés dans `NODE_ANNOUNCE`. Rend vrai si la
    /// valeur a changé (l'appelant ré-annonce alors aux sessions directes).
    pub fn set_local_flags(&self, flags: u8) -> bool {
        self.local_flags.swap(flags, Ordering::SeqCst) != flags
    }

    /// Drapeaux de capacité couramment annoncés.
    pub fn local_flags(&self) -> u8 {
        self.local_flags.load(Ordering::SeqCst)
    }

    /// Message d'auto-annonce courant (preuve de travail de l'identité +
    /// drapeaux de capacité).
    fn announce_msg(&self) -> ChannelMsg {
        ChannelMsg::Control(ControlMsg::NodeAnnounce {
            pow_nonce: self.identity.pow_nonce(),
            flags: self.local_flags(),
        })
    }

    /// Ré-émet l'auto-annonce vers toutes les sessions DIRECTES établies
    /// (après un changement de drapeaux — ex. éligibilité relais acquise).
    /// Borné par le nombre de sessions ; best-effort.
    pub async fn reannounce_direct_sessions(&self) {
        let addrs: Vec<SocketAddr> = {
            let st = self.state_lock();
            st.sessions_by_id
                .values()
                .filter(|s| s.relay_circuit.is_none())
                .map(|s| s.peer_addr)
                .collect()
        };
        let msg = self.announce_msg();
        for addr in addrs {
            let _ = self.send(addr, &msg).await;
        }
    }

    /// Adresse locale liée.
    pub fn local_addr(&self) -> SocketAddr {
        self.socket.local_addr()
    }

    /// NodeId local.
    pub fn node_id(&self) -> NodeId {
        self.identity.node_id()
    }

    /// Démarre les boucles de réception et de maintenance en tâches tokio.
    pub fn spawn(self: &Arc<Self>) {
        let recv = Arc::clone(self);
        tokio::spawn(async move { recv.recv_loop().await });
        let maint = Arc::clone(self);
        tokio::spawn(async move { maint.maintenance_loop().await });
    }

    /// Signale l'arrêt (les boucles s'arrêtent au prochain tour).
    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
    }

    fn is_shutdown(&self) -> bool {
        self.shutdown.load(Ordering::SeqCst)
    }

    /// Envoie un message applicatif à un pair par adresse, sans lier la session
    /// à une identité (pair quelconque : trafic DHT vers des nœuds sans amitié
    /// établie). Établit la session si nécessaire (message mis en file jusqu'au
    /// handshake). Pour une livraison liée à une identité précise, voir
    /// [`Endpoint::send_to`].
    pub async fn send(&self, addr: SocketAddr, msg: &ChannelMsg) -> Result<(), TransportError> {
        self.send_to(addr, None, msg).await
    }

    /// Envoie un message applicatif en liant la session à l'identité Ed25519
    /// attendue (`expected`). Si un WELCOME (ou une session existante) émane
    /// d'une autre identité que `expected`, l'envoi échoue avec
    /// [`TransportError::PeerIdentityMismatch`] et la file n'est jamais scellée
    /// sous une session usurpée (liaison d'identité, SPEC §2.2). `expected` à
    /// `None` équivaut à [`Endpoint::send`] (aucune liaison imposée).
    pub async fn send_to(
        &self,
        addr: SocketAddr,
        expected: Option<[u8; 32]>,
        msg: &ChannelMsg,
    ) -> Result<(), TransportError> {
        let plaintext = msg.to_bytes();
        self.send_plaintext(addr, expected, plaintext).await
    }

    async fn send_plaintext(
        &self,
        addr: SocketAddr,
        expected: Option<[u8; 32]>,
        plaintext: Vec<u8>,
    ) -> Result<(), TransportError> {
        // Au-delà de la borne de réassemblage du pair (1 MiB), le message serait
        // rejeté à l'arrivée : refus franc à l'émission.
        if plaintext.len() > frag::MAX_MESSAGE_LEN {
            return Err(TransportError::TooLarge);
        }
        let now = self.clock.now_ms();
        let outgoing: Vec<Vec<u8>> = {
            let mut st = self.state_lock();
            // Session retrouvée par identifiant (jamais supposée : une
            // désynchronisation `id_by_addr`/`sessions_by_id` retombe sur le
            // handshake plutôt que de paniquer, D23).
            let sid = st.id_by_addr.get(&addr).copied();
            if let Some(session) = sid.and_then(|sid| st.sessions_by_id.get_mut(&sid)) {
                // Défense en profondeur : un envoi lié ne doit jamais être scellé
                // sous une session dont l'identité du pair diffère de la cible
                // (ex. session usurpée déjà en place à cette adresse). Comparaison
                // temps constant.
                if let Some(k) = expected {
                    if !bool::from(session.peer_static.ct_eq(&k)) {
                        return Err(TransportError::PeerIdentityMismatch);
                    }
                }
                Self::seal_frames(session, &plaintext, now)?
            } else {
                // Pas de session : ouvrir un handshake et mettre en file.
                let entry = st.pending.entry(addr).or_insert_with(|| {
                    let initiator = Initiator::start(
                        &self.identity,
                        now,
                        Vec::new(),
                        self.config.pow_bits,
                        expected,
                    );
                    Pending {
                        initiator,
                        expected_static: expected,
                        queued: Vec::new(),
                        attempts: 0,
                        last_send_ms: 0,
                    }
                });
                // Renforce la liaison si un envoi lié rejoint un pending
                // jusque-là non lié ; ne desserre jamais une liaison existante.
                // On ne touche pas à l'initiateur en vol (transcript déjà émis) :
                // la vérification transport de `on_welcome` s'appuie sur ce champ.
                if entry.expected_static.is_none() {
                    entry.expected_static = expected;
                }
                entry.queued.push(plaintext);
                if entry.attempts == 0 {
                    entry.attempts = 1;
                    entry.last_send_ms = now;
                    vec![Packet::Hello(entry.initiator.hello().clone()).to_bytes()]
                } else {
                    Vec::new()
                }
            }
        };
        for bytes in outgoing {
            self.socket.send_to(&bytes, addr).await?;
        }
        Ok(())
    }

    /// Scelle un plaintext applicatif en un ou plusieurs paquets DATA prêts à
    /// émettre, en fragmentant de façon transparente au-delà de la MTU
    /// (SPEC §13.1). Applique au passage le re-keying périodique.
    fn seal_frames(
        session: &mut Session,
        plaintext: &[u8],
        now: u64,
    ) -> Result<Vec<Vec<u8>>, TransportError> {
        // Re-keying périodique (forward secrecy) avant de sceller.
        if session.crypto.needs_rekey(now) {
            let _ = session.crypto.advance_send_epoch(now);
        }
        let frames = frag::frame(plaintext, &mut session.next_frag_id);
        let mut out = Vec::with_capacity(frames.len());
        for cadre in &frames {
            let packet = session.crypto.seal(cadre)?;
            out.push(Packet::Data(packet).to_bytes());
        }
        session.last_send_ms = now;
        Ok(out)
    }

    /// Ouvre proactivement une session vers `addr` (sans message applicatif).
    pub async fn connect(&self, addr: SocketAddr) -> Result<(), TransportError> {
        // Un PING de contrôle sert de porteur ; s'il n'y a pas encore de
        // session, il déclenche le handshake et sera livré ensuite.
        let mut token = [0u8; 8];
        OsRng.fill_bytes(&mut token);
        let ping = ChannelMsg::Control(ControlMsg::Ping {
            token: u64::from_be_bytes(token),
        });
        self.send(addr, &ping).await
    }

    /// Poinçonnage UDP coordonné vers un pair connu par une LISTE de candidats
    /// (SPEC §11, points 2-3).
    ///
    /// Les candidats sont d'abord triés par ordre d'essai du SPEC (local direct
    /// → public direct → hole punch → relais, via l'ordre de [`Candidate::kind`]).
    /// L'endpoint émet ensuite des HELLO **simultanés** vers TOUS les candidats,
    /// réémis à [`PUNCH_INTERVAL`] sur [`PUNCH_ATTEMPTS`] salves, jusqu'à ce
    /// qu'une session soit établie avec `expected_static` — **première session
    /// gagnante**, les salves restantes s'arrêtent aussitôt.
    ///
    /// Chaque tentative réutilise le mécanisme HELLO existant et la liaison
    /// d'identité `expected_static` (anti-MITM, D-037 : le WELCOME devra émaner
    /// de cette clé, cf. [`Endpoint::on_welcome`]). L'état partagé (`pending`,
    /// sessions) est réutilisé tel quel : un candidat déjà couvert par une
    /// session établie ou un `Pending` en cours n'est pas dupliqué.
    ///
    /// **Best-effort** : aucune erreur dure si le poinçonnage échoue (le repli
    /// relais interviendra plus tard, SPEC §11.3). L'ouverture simultanée d'un
    /// HELLO de part et d'autre est résolue en une **unique** session par
    /// l'arbitrage de [`Endpoint::on_hello`].
    pub async fn punch(
        self: &Arc<Self>,
        candidates: &[Candidate],
        expected_static: [u8; 32],
    ) -> Result<(), TransportError> {
        // Ordre d'essai du SPEC : LocalDirect < PublicDirect < HolePunch < Relay.
        let mut ordered: Vec<Candidate> = candidates.to_vec();
        ordered.sort_by_key(|c| c.kind);

        for _ in 0..PUNCH_ATTEMPTS {
            // Première session DIRECTE gagnante : dès qu'un lien direct avec le
            // pair attendu existe, on cesse d'émettre (pas de HELLO superflu).
            // Une session seulement RELAYÉE ne suffit pas : le poinçonnage sert
            // précisément à la remplacer par un lien direct (SPEC §11.3).
            if self.has_direct_session_with(&expected_static) {
                return Ok(());
            }
            for cand in &ordered {
                self.emit_punch_hello(cand.addr, expected_static).await?;
            }
            // On laisse une salve le temps d'éliciter un WELCOME avant la
            // suivante (y compris après la dernière : le WELCOME peut arriver
            // pendant cette attente et sera capté par le prochain `punch` ou la
            // maintenance).
            tokio::time::sleep(PUNCH_INTERVAL).await;
        }
        Ok(())
    }

    /// Émet un HELLO de poinçonnage vers `addr` en réutilisant l'état existant :
    /// - session déjà établie vers `addr` ⇒ ne fait rien ;
    /// - `Pending` déjà en cours ⇒ réémet SON HELLO (même nonce : la
    ///   retransmission reste idempotente côté répondeur grâce au cache de
    ///   nonces) sans créer de doublon ;
    /// - sinon ⇒ crée un `Pending` lié à `expected_static` et émet le HELLO.
    ///
    /// Le verrou d'état n'est jamais tenu pendant l'envoi réseau (async).
    async fn emit_punch_hello(
        &self,
        addr: SocketAddr,
        expected_static: [u8; 32],
    ) -> Result<(), TransportError> {
        let now = self.clock.now_ms();
        let hello_bytes: Option<Vec<u8>> = {
            let mut st = self.state_lock();
            if st.id_by_addr.contains_key(&addr) {
                None // session déjà en place vers ce candidat
            } else if let Some(pending) = st.pending.get_mut(&addr) {
                // Ouverture déjà en cours : on réémet, sans dupliquer le pending.
                pending.attempts += 1;
                pending.last_send_ms = now;
                Some(Packet::Hello(pending.initiator.hello().clone()).to_bytes())
            } else {
                let initiator = Initiator::start(
                    &self.identity,
                    now,
                    Vec::new(),
                    self.config.pow_bits,
                    Some(expected_static),
                );
                let hello = Packet::Hello(initiator.hello().clone()).to_bytes();
                st.pending.insert(
                    addr,
                    Pending {
                        initiator,
                        expected_static: Some(expected_static),
                        queued: Vec::new(),
                        attempts: 1,
                        last_send_ms: now,
                    },
                );
                Some(hello)
            }
        };
        if let Some(bytes) = hello_bytes {
            self.socket.send_to(&bytes, addr).await?;
        }
        Ok(())
    }

    /// Vrai si une session établie DIRECTE (non relayée) existe avec le pair
    /// d'identité `static_pub` (comparaison temps constant, cohérente avec le
    /// reste de l'endpoint). Publique : le runtime s'en sert pour décider s'il
    /// faut tenter un poinçonnage d'upgrade quand seul un relais est en place.
    pub fn has_direct_session_with(&self, static_pub: &[u8; 32]) -> bool {
        self.direct_session_addr(static_pub).is_some()
    }

    /// Adresse de la session établie DIRECTE (non relayée) avec le pair
    /// d'identité `static_pub`, le cas échéant. Contrairement au carnet
    /// d'adresses de la couche nœud, ne rend JAMAIS une adresse sans session :
    /// un envoi vers cette adresse scelle réellement (aucune mise en file de
    /// handshake spéculatif) — c'est ce qui permet aux boucles de maintenance
    /// de ne consommer l'outbox que sur une livraison effective.
    pub fn direct_session_addr(&self, static_pub: &[u8; 32]) -> Option<SocketAddr> {
        let st = self.state_lock();
        // Après un redémarrage SILENCIEUX du pair (extinction UDP sans adieu),
        // deux sessions directes coexistent pour la même identité jusqu'à
        // l'expiration d'inactivité (2 min) : la morte (ancienne adresse) et la
        // fraîche (nouvelle). Un choix arbitraire (ordre de HashMap, stable pour
        // tout le processus) enverrait TOUTES les annonces de profil dans la
        // session cadavre — trou noir UDP sans erreur, aucune relance avant la
        // ré-annonce périodique (30 min). On préfère donc la session au dernier
        // TRAFIC ENTRANT le plus récent : seule une session vivante en reçoit.
        st.sessions_by_id
            .values()
            .filter(|s| s.relay_circuit.is_none() && bool::from(s.peer_static.ct_eq(static_pub)))
            .max_by_key(|s| s.last_recv_ms)
            .map(|s| s.peer_addr)
    }

    /// Corrèle un PONG entrant au dernier PING keep-alive émis sur cette
    /// session et enregistre l'aller-retour mesuré. La session est retrouvée
    /// par le lien d'arrivée : adresse pour un lien direct, circuit pour un
    /// tunnel. Jeton inattendu (rejoué, croisé avec un `connect`) : ignoré —
    /// un pair ne peut pas fabriquer une latence sans connaître le jeton
    /// aléatoire du PING scellé sous la session.
    fn note_pong(&self, link: PeerLink, token: u64) {
        let now = self.clock.now_ms();
        let mut st = self.state_lock();
        let sid = match link {
            PeerLink::Direct(addr) => st.id_by_addr.get(&addr).copied(),
            PeerLink::Tunnel { circuit, .. } => {
                st.client_circuits.get(&circuit).and_then(|c| c.session_id)
            }
        };
        let Some(session) = sid.and_then(|sid| st.sessions_by_id.get_mut(&sid)) else {
            return;
        };
        if let Some((attendu, emis_ms)) = session.ping_pending {
            if attendu == token {
                session.ping_pending = None;
                session.last_rtt_ms = Some(now.saturating_sub(emis_ms));
            }
        }
    }

    /// Photographie des sessions établies (directes et tunnelées), pour le
    /// diagnostic par pair de la couche nœud ([`SessionView`]). Instantané
    /// sous verrou court, sans E/S.
    pub fn session_views(&self) -> Vec<SessionView> {
        let st = self.state_lock();
        st.sessions_by_id
            .values()
            .map(|s| SessionView {
                peer_static: s.peer_static,
                addr: s.peer_addr,
                relay_circuit: s.relay_circuit,
                last_recv_ms: s.last_recv_ms,
                last_rtt_ms: s.last_rtt_ms,
            })
            .collect()
    }

    /// Consomme un jeton du seau de contrôle de la session DIRECTE à `addr`
    /// (bornage post-handshake des messages de contrôle changeant l'état,
    /// H1/M1b). Rend `false` si aucune session directe n'existe à cette adresse
    /// ou si le quota par session est épuisé — l'appelant ignore alors le
    /// message. Le verrou n'est jamais tenu pendant un `await`.
    fn take_ctrl_token(&self, addr: SocketAddr, now: u64) -> bool {
        let mut st = self.state_lock();
        match st
            .id_by_addr
            .get(&addr)
            .copied()
            .and_then(|sid| st.sessions_by_id.get_mut(&sid))
        {
            Some(session) => session.ctrl_bucket.try_take(now),
            None => false,
        }
    }

    /// Sous UN verrou (H1) : consomme un jeton de contrôle pour la session
    /// DIRECTE à `addr` et rend s'il faut RÉPONDRE (première annonce de la
    /// session). `None` = pas de session directe à cette adresse ou quota de
    /// contrôle épuisé (l'annonce est alors ignorée sans aucune remontée).
    fn accept_announce(&self, addr: SocketAddr, now: u64) -> Option<bool> {
        let mut st = self.state_lock();
        let sid = st.id_by_addr.get(&addr).copied()?;
        let session = st.sessions_by_id.get_mut(&sid)?;
        if !session.ctrl_bucket.try_take(now) {
            return None;
        }
        let reply = !session.announced;
        session.announced = true;
        Some(reply)
    }

    async fn recv_loop(self: Arc<Self>) {
        while !self.is_shutdown() {
            let (buf, from) = match self.socket.recv_from().await {
                Ok(v) => v,
                Err(_) => {
                    if self.is_shutdown() {
                        break;
                    }
                    continue;
                }
            };
            if let Err(e) = self.handle_datagram(&buf, from).await {
                tracing::debug!(?from, error = %e, "datagramme rejeté");
            }
        }
    }

    async fn handle_datagram(&self, buf: &[u8], from: SocketAddr) -> Result<(), TransportError> {
        let packet = Packet::from_bytes(buf)?;
        match packet {
            Packet::Hello(hello) => self.on_hello(hello, PeerLink::Direct(from)).await,
            Packet::Welcome(welcome) => self.on_welcome(welcome, PeerLink::Direct(from)).await,
            Packet::Data(data) => self.on_data(data, from).await,
            Packet::Cookie(cookie) => self.on_cookie(cookie, PeerLink::Direct(from)).await,
        }
    }

    async fn on_hello(&self, hello: Hello, link: PeerLink) -> Result<(), TransportError> {
        let now = self.clock.now_ms();
        // Adresse de transport du pair : directe, ou celle du relais en tunnel.
        let peer_addr = link.addr();

        // --- Résolution du simultaneous-open (hole punching, SPEC §11.2-3) ---
        //
        // Si l'on a déjà un HELLO sortant en vol vers `from` (un `Pending`) et
        // que le pair nous renvoie SON HELLO en même temps, les deux extrémités
        // ouvrent chacune un handshake. Sans arbitrage, DEUX sessions distinctes
        // s'établiraient : chaque répondeur tire un `session_id` aléatoire
        // (`respond`), et le trafic finirait scellé sous des identifiants
        // divergents — chaque bord ne connaîtrait pas le `session_id` de l'autre.
        //
        // On tranche de façon déterministe par comparaison des clés statiques
        // publiques (que les deux bords connaissent une fois les HELLO échangés) :
        //   - clé locale > clé du pair  ⇒ on joue le RÉPONDEUR : on traite ce
        //     HELLO ci-dessous ; `install_session` retirera notre `Pending`,
        //     abandonnant proprement notre propre tentative d'initiateur ;
        //   - clé locale < clé du pair  ⇒ on joue l'INITIATEUR : on IGNORE ce
        //     HELLO, on conserve notre `Pending`, et l'on conclura le handshake
        //     à réception du WELCOME du pair.
        // Les deux bords calculent le même arbitrage ⇒ une seule session,
        // portée par le `session_id` de l'unique répondeur. La liaison d'identité
        // du `Pending` (anti-MITM, D-037) reste intacte côté initiateur puisqu'on
        // ne le retire jamais. La comparaison porte sur des clés publiques : le
        // temps constant n'est pas requis ici.
        //
        // L'arbitrage ne concerne que les liens DIRECTS (pendings indexés par
        // adresse). Sur un tunnel, les rôles sont fixés : l'initiateur est le
        // seul à appeler `connect_via_relay`, l'autre bord est toujours répondeur.
        if let PeerLink::Direct(from) = link {
            let st = self.state_lock();
            if st.pending.contains_key(&from) && self.identity.public_key() < hello.static_pub {
                drop(st);
                tracing::trace!(
                    ?from,
                    "simultaneous-open : rôle initiateur retenu, HELLO du pair ignoré"
                );
                return Ok(());
            }
        }

        // --- Liaison d'identité au niveau du tunnel (FAILLE A) ----------------
        //
        // Un HELLO réinjecté depuis un circuit relais ne doit être traité que
        // s'il émane de l'identité à laquelle le circuit est LIÉ. La règle est la
        // même pour les deux rôles, mais protège de deux façons :
        //   - Circuit INITIATEUR (ouvert par `open_relay_circuit` /
        //     `connect_via_relay`) : `client_circuits[circuit].peer_static` est la
        //     cible VOULUE (B). Ce circuit n'attend qu'un WELCOME. Sans ce
        //     contrôle, un relais malveillant qui réinjecterait `Data{blob:
        //     Hello(Z)}` ferait installer à la victime une session avec Z tout en
        //     croyant parler à B (`peer_static` inchangé), en ÉCRASANT le vrai
        //     handshake `relay_pending[circuit]` → MITM complet. On exige donc
        //     `hello.static_pub == cc.peer_static` (Z ≠ B ⇒ HELLO ignoré).
        //   - Circuit RÉPONDEUR (créé par `accept_inbound_circuit`) :
        //     `peer_static` a été fixé à l'identité du HELLO initial ; les
        //     retransmissions du MÊME HELLO passent, mais le circuit ne peut plus
        //     être RÉ-IDENTIFIÉ ensuite par une autre clé.
        //
        // Comparaison temps constant. C'est la liaison D-037 (§2.2) portée jusqu'à
        // l'étage de réinjection : ni `respond` ni le handshake ne connaissent la
        // cible attendue d'un circuit, seul ce point de contrôle la connaît.
        if let PeerLink::Tunnel { circuit, .. } = link {
            let st = self.state_lock();
            if let Some(cc) = st.client_circuits.get(&circuit) {
                if !bool::from(hello.static_pub.ct_eq(&cc.peer_static)) {
                    drop(st);
                    tracing::warn!(
                        circuit,
                        "HELLO tunnelé d'une identité non liée au circuit : ignoré (liaison D-037)"
                    );
                    return Ok(());
                }
            }
        }

        let reply: Option<Vec<u8>> = {
            let mut st = self.state_lock();

            // Comptage de pression + rate limit par IP (celle du relais en tunnel :
            // tous les HELLO tunnelés d'un relais partagent son seau, ce qui borne
            // l'amplification).
            if now.saturating_sub(st.window_start_ms) >= 1000 {
                st.hs_seen_in_window = 0;
                st.window_start_ms = now;
            }
            st.hs_seen_in_window += 1;
            let under_pressure = st.hs_seen_in_window > self.config.cookie_pressure_per_s;
            if !st.hs_rate.check(peer_addr.ip(), 1.0, now) {
                return Ok(()); // rejet silencieux
            }

            // Sous pression : exiger un cookie valide avant tout état.
            if under_pressure {
                let addr_key = peer_addr.to_string();
                let ok = st
                    .cookie_jar
                    .verify(&addr_key, &hello.static_pub, &hello.cookie);
                if !ok {
                    let cookie = st.cookie_jar.issue(&addr_key, &hello.static_pub, now);
                    Some(
                        Packet::Cookie(CookiePacket {
                            cookie: cookie.to_vec(),
                        })
                        .to_bytes(),
                    )
                } else {
                    None
                }
            } else {
                None
            }
        };
        if let Some(bytes) = reply {
            // Le défi COOKIE repart par le même lien (ré-enveloppé en tunnel).
            self.send_packet_via_link(bytes, link).await?;
            return Ok(());
        }

        // Traiter le HELLO : produire le WELCOME et établir la session.
        let welcome_bytes;
        // Trames scellées de la file d'un pending initiateur évincé par ce
        // répondeur (simultaneous-open) : envoyées APRÈS le WELCOME, sur le
        // même lien direct (un croisement ne se produit pas en tunnel).
        let mut file_rescellee: Vec<Vec<u8>> = Vec::new();
        {
            let mut st = self.state_lock();
            let (welcome, established) = respond(
                &self.identity,
                &hello,
                now,
                &mut st.nonce_cache,
                self.config.pow_bits,
            )?;
            welcome_bytes = Packet::Welcome(welcome).to_bytes();
            let peer_node = node_id_of(&established.peer_static);
            let crypto = SessionCrypto::new(&established.keys, established.session_id, false, now);
            let sid = established.session_id;
            let en_attente = Self::install_session(
                &mut st,
                Session {
                    crypto,
                    peer_addr,
                    peer_static: established.peer_static,
                    peer_node,
                    last_recv_ms: now,
                    last_send_ms: now,
                    next_frag_id: 0,
                    reasm: Reassembler::new(),
                    relay_circuit: link.circuit(),
                    // Répondeur : l'annonce partira en réponse à celle de
                    // l'initiateur (jamais sur un tunnel).
                    announced: link.circuit().is_some(),
                    ctrl_bucket: Bucket::new(CTRL_MSG_BURST, CTRL_MSG_REFILL_PER_S, now),
                    ping_pending: None,
                    last_rtt_ms: None,
                },
                sid,
            );
            // Re-scelle la file du pending évincé sous la session tout juste
            // établie (sinon ces messages — dont l'annonce de profil — sont
            // perdus au simultaneous-open, cf. `install_session`).
            if !en_attente.is_empty() {
                if let Some(session) = st.sessions_by_id.get_mut(&sid) {
                    for plaintext in en_attente {
                        if let Ok(frames) = Self::seal_frames(session, &plaintext, now) {
                            file_rescellee.extend(frames);
                        }
                    }
                }
            }
            tracing::debug!(
                pair = %hex4(&established.peer_static),
                %peer_addr,
                tunnel = link.circuit().is_some(),
                "session établie (répondeur)"
            );
            let _ = self.events.send(TransportEvent::Connected {
                node: peer_node,
                addr: peer_addr,
                static_pub: established.peer_static,
            });
        }
        // Le WELCOME repart par le même lien (direct ou ré-enveloppé en tunnel).
        self.send_packet_via_link(welcome_bytes, link).await?;
        // Puis la file re-scellée (lien direct uniquement — jamais en tunnel).
        for bytes in file_rescellee {
            self.send_packet_via_link(bytes, link).await?;
        }
        Ok(())
    }

    async fn on_welcome(
        &self,
        welcome: accord_proto::Welcome,
        link: PeerLink,
    ) -> Result<(), TransportError> {
        let now = self.clock.now_ms();
        let peer_addr = link.addr();
        let mut to_send: Vec<Vec<u8>> = Vec::new();
        {
            let mut st = self.state_lock();
            // Le pending initiateur vit dans `pending` (lien direct, indexé par
            // adresse) ou dans `relay_pending` (tunnel, indexé par circuit).
            let pending = match link {
                PeerLink::Direct(addr) => st.pending.remove(&addr),
                PeerLink::Tunnel { circuit, .. } => st.relay_pending.remove(&circuit),
            };
            let Some(pending) = pending else {
                return Ok(()); // WELCOME non sollicité
            };
            let expected_static = pending.expected_static;
            let established = match pending.initiator.finish(&welcome, now) {
                Ok(e) => e,
                Err(e) => {
                    // Handshake invalide : on abandonne ce pending.
                    return Err(e.into());
                }
            };
            // Liaison d'identité (SPEC §2.2), défense en profondeur en plus du
            // contrôle crypto de `finish` : si un pair précis était attendu, la
            // clé statique authentifiée doit lui correspondre (temps constant).
            // Sinon on abandonne le pending SANS sceller ni émettre sa file : un
            // MITM on-path — ou un RELAIS qui routerait le circuit vers un autre
            // nœud — ne peut pas se substituer à la cible. Le pending est déjà
            // retiré. C'est ce contrôle qui garantit que le relais ne peut jamais
            // usurper B au bout d'un tunnel (D-037).
            if let Some(expected) = expected_static {
                if !bool::from(established.peer_static.ct_eq(&expected)) {
                    tracing::warn!(
                        ?peer_addr,
                        "WELCOME d'une identité inattendue : liaison refusée, file abandonnée"
                    );
                    return Err(TransportError::PeerIdentityMismatch);
                }
            }
            let peer_node = node_id_of(&established.peer_static);
            let crypto = SessionCrypto::new(&established.keys, established.session_id, true, now);
            let mut session = Session {
                crypto,
                peer_addr,
                peer_static: established.peer_static,
                peer_node,
                last_recv_ms: now,
                last_send_ms: now,
                next_frag_id: 0,
                reasm: Reassembler::new(),
                relay_circuit: link.circuit(),
                // Initiateur : notre annonce part immédiatement après (lien
                // direct seulement), le pair y répondra avec la sienne.
                announced: true,
                ctrl_bucket: Bucket::new(CTRL_MSG_BURST, CTRL_MSG_REFILL_PER_S, now),
                ping_pending: None,
                last_rtt_ms: None,
            };
            // Vider la file d'attente sous la nouvelle session (fragmentée au
            // besoin).
            for plaintext in &pending.queued {
                if let Ok(frames) = Self::seal_frames(&mut session, plaintext, now) {
                    to_send.extend(frames);
                }
            }
            let sid = established.session_id;
            // Le pending initiateur a déjà été retiré en tête de `on_welcome`
            // (sa file est scellée dans `to_send` ci-dessus) : `install_session`
            // ne trouve donc rien à re-sceller ici. ATTENTION : l'appel doit
            // rester HORS de `debug_assert!` — son argument n'est pas évalué en
            // release, et la session ne serait alors JAMAIS installée (régression
            // 3.0.0→3.3.0 : aucune session initiateur en production, plus aucun
            // message échangé dès que les deux pairs doivent se recomposer).
            let file_residuelle = Self::install_session(&mut st, session, sid);
            debug_assert!(file_residuelle.is_empty());
            tracing::debug!(
                pair = %hex4(&established.peer_static),
                %peer_addr,
                tunnel = link.circuit().is_some(),
                "session établie (initiateur)"
            );
            let _ = self.events.send(TransportEvent::Connected {
                node: peer_node,
                addr: peer_addr,
                static_pub: established.peer_static,
            });
        }
        for bytes in to_send {
            self.send_packet_via_link(bytes, link).await?;
        }
        // Auto-annonce DHT (SPEC §11.3) : l'initiateur d'une session DIRECTE
        // s'annonce dès l'établissement ; le répondeur — dont la session existe
        // déjà (il a émis le WELCOME), aucun risque de course — répondra avec
        // la sienne. Jamais sur un tunnel (l'adresse observée serait celle du
        // relais et empoisonnerait la table du pair).
        if let PeerLink::Direct(addr) = link {
            let _ = self.send(addr, &self.announce_msg()).await;
        }
        Ok(())
    }

    async fn on_cookie(&self, cookie: CookiePacket, link: PeerLink) -> Result<(), TransportError> {
        let now = self.clock.now_ms();
        let hello_bytes: Option<Vec<u8>> = {
            let mut st = self.state_lock();
            // Même dualité que `on_welcome` : pending direct vs pending tunnelé.
            let pending = match link {
                PeerLink::Direct(addr) => st.pending.get_mut(&addr),
                PeerLink::Tunnel { circuit, .. } => st.relay_pending.get_mut(&circuit),
            };
            if let Some(pending) = pending {
                // Relance le handshake avec le cookie fourni, en conservant la
                // liaison d'identité attendue (le nouveau HELLO reste lié).
                let initiator = Initiator::start(
                    &self.identity,
                    now,
                    cookie.cookie.clone(),
                    self.config.pow_bits,
                    pending.expected_static,
                );
                pending.initiator = initiator;
                pending.attempts += 1;
                pending.last_send_ms = now;
                Some(Packet::Hello(pending.initiator.hello().clone()).to_bytes())
            } else {
                None
            }
        };
        if let Some(bytes) = hello_bytes {
            self.send_packet_via_link(bytes, link).await?;
        }
        Ok(())
    }

    async fn on_data(
        &self,
        data: accord_proto::DataPacket,
        from: SocketAddr,
    ) -> Result<(), TransportError> {
        let now = self.clock.now_ms();
        let ready: Option<(Vec<u8>, NodeId, [u8; 32], PeerLink)> = {
            let mut st = self.state_lock();
            let Some(session) = st.sessions_by_id.get_mut(&data.session_id) else {
                return Ok(()); // session inconnue
            };
            let cadre = session.crypto.open(&data)?;
            session.last_recv_ms = now;
            // Déframe/réassemble : `None` si fragment partiel ou doublon.
            let assembled = session.reasm.accept(&cadre, now)?;
            let relay_circuit = session.relay_circuit;
            // Mobilité : le pair a peut-être changé d'adresse. Ne s'applique QU'AUX
            // sessions directes : une session relayée garde `peer_addr = relay_addr`
            // et n'est pas indexée par adresse (sinon on écraserait la session avec
            // le relais, qui partage cette adresse).
            if relay_circuit.is_none() && session.peer_addr != from {
                let old = session.peer_addr;
                st.id_by_addr.remove(&old);
                if let Some(s) = st.sessions_by_id.get_mut(&data.session_id) {
                    s.peer_addr = from;
                }
                st.id_by_addr.insert(from, data.session_id);
            }
            // Ré-obtention après la mise à jour de mobilité : disparition
            // impossible sous ce même verrou — repli silencieux (D23).
            let Some(session) = st.sessions_by_id.get(&data.session_id) else {
                return Ok(());
            };
            // Le lien de réponse suit le chemin d'arrivée : tunnel pour une session
            // relayée, direct sinon.
            let link = match relay_circuit {
                Some(circuit) => PeerLink::Tunnel {
                    relay_addr: session.peer_addr,
                    circuit,
                },
                None => PeerLink::Direct(from),
            };
            assembled.map(|plaintext| (plaintext, session.peer_node, session.peer_static, link))
        };

        let Some((plaintext, peer_node, peer_static, link)) = ready else {
            return Ok(()); // rien de complet à remonter pour l'instant
        };
        let msg = ChannelMsg::from_bytes(&plaintext)?;
        self.dispatch(msg, link, peer_node, peer_static).await
    }

    async fn dispatch(
        &self,
        msg: ChannelMsg,
        link: PeerLink,
        peer_node: NodeId,
        peer_static: [u8; 32],
    ) -> Result<(), TransportError> {
        // Adresse remontée aux couches hautes : celle du relais pour un tunnel.
        let from = link.addr();
        // Les messages de contrôle sont traités par le transport lui-même. Les
        // réponses repartent par le MÊME lien (direct ou ré-enveloppé en tunnel).
        if let ChannelMsg::Control(ctrl) = &msg {
            match ctrl {
                ControlMsg::Ping { token } => {
                    let pong = ChannelMsg::Control(ControlMsg::Pong { token: *token });
                    self.send_msg_via_link(link, &pong).await?;
                    return Ok(());
                }
                ControlMsg::Pong { token } => {
                    self.note_pong(link, *token);
                    return Ok(());
                }
                ControlMsg::Close { .. } => {
                    self.close_link(link);
                    let _ = self
                        .events
                        .send(TransportEvent::Disconnected { addr: from });
                    return Ok(());
                }
                ControlMsg::Rekey { .. } => {
                    // La dérivation d'epoch en réception est automatique ; rien
                    // à faire de plus.
                    return Ok(());
                }
                ControlMsg::ObserveAddrReq => {
                    let resp = ChannelMsg::Control(ControlMsg::ObserveAddrResp {
                        addr: accord_proto::WireAddr(from),
                    });
                    self.send_msg_via_link(link, &resp).await?;
                    let _ = self
                        .events
                        .send(TransportEvent::ObservedByPeer { addr: from });
                    return Ok(());
                }
                ControlMsg::ObserveAddrResp { addr } => {
                    // Observation d'adresse : UNIQUEMENT sur session directe (sur
                    // un tunnel, le pair observerait l'adresse du relais, pas la
                    // nôtre) et bornée par le seau de contrôle de la session
                    // (M1b : anti-flood). Portée par l'identité du pair pour que
                    // le consensus dédoublonne les votes par pair (M1b).
                    let PeerLink::Direct(addr_from) = link else {
                        return Ok(());
                    };
                    if !self.take_ctrl_token(addr_from, self.clock.now_ms()) {
                        return Ok(()); // pas de session / quota épuisé : ignoré
                    }
                    let _ = self.events.send(TransportEvent::ObservedAddr {
                        observer: peer_static,
                        observed: addr.0,
                    });
                    return Ok(());
                }
                ControlMsg::PunchRequest { token, candidates } => {
                    // Remontée brute : la politique (amitié, cadence, filtrage
                    // des candidats, réponse) vit dans le runtime applicatif.
                    let _ = self.events.send(TransportEvent::PunchRequested {
                        static_pub: peer_static,
                        token: *token,
                        candidates: candidates.iter().map(|a| a.0).collect(),
                    });
                    return Ok(());
                }
                ControlMsg::PunchResponse { token, candidates } => {
                    let _ = self.events.send(TransportEvent::PunchResponded {
                        static_pub: peer_static,
                        token: *token,
                        candidates: candidates.iter().map(|a| a.0).collect(),
                    });
                    return Ok(());
                }
                ControlMsg::NodeAnnounce { pow_nonce, flags } => {
                    // Uniquement sur session DIRECTE : sur un tunnel, l'adresse
                    // observée serait celle du relais (empoisonnement de table).
                    let PeerLink::Direct(addr) = link else {
                        return Ok(());
                    };
                    // H1 : consomme un jeton du seau de contrôle de la session ET
                    // décide de la réponse (première annonce) sous UN verrou. Au-
                    // delà du quota par session, l'annonce est ignorée AVANT toute
                    // remontée d'événement — un pair authentifié ne peut plus
                    // inonder le canal d'événements ni marteler la table de
                    // routage à plein débit UDP.
                    let Some(reply) = self.accept_announce(addr, self.clock.now_ms()) else {
                        return Ok(()); // pas de session directe / quota épuisé
                    };
                    let _ = self.events.send(TransportEvent::NodeAnnounced {
                        static_pub: peer_static,
                        addr,
                        pow_nonce: *pow_nonce,
                        flags: *flags,
                    });
                    if reply {
                        let _ = self.send(addr, &self.announce_msg()).await;
                    }
                    return Ok(());
                }
            }
        }
        // Canal RELAY (0x05, SPEC §10) : intercepté par le transport AVANT toute
        // remontée générique. Le NodeId de l'émetteur est `peer_node`.
        match msg {
            ChannelMsg::Relay(relay) => {
                self.handle_relay(relay, from, peer_node, peer_static).await
            }
            other => {
                let _ = self.events.send(TransportEvent::Message {
                    from: peer_node,
                    addr: from,
                    static_pub: peer_static,
                    msg: Box::new(other),
                });
                Ok(())
            }
        }
    }

    /// Traite un message du canal RELAY reçu de `peer_node` (à l'adresse `from`).
    ///
    /// Trois rôles se croisent ici, distingués sans ambiguïté :
    /// - **Extrémité CLIENTE** (SPEC §11.3) : le circuit figure dans
    ///   `client_circuits` ⇒ le `Data` est un paquet A↔B tunnelé, RÉINJECTÉ dans
    ///   le pipeline de réception (`on_hello`/`on_welcome`/`on_data`) ; les
    ///   `Accept`/`Reject` résolvent une ouverture en attente.
    /// - **Relais SERVEUR** (`relay_serving == true`) : `Open`/`Data`/`Close`
    ///   acheminent du trafic opaque. Le relais ne déchiffre JAMAIS les blobs.
    /// - **Cible PASSIVE** : un `Data` contenant un HELLO sur un circuit inconnu
    ///   ouvre un tunnel entrant dont ce nœud est l'extrémité.
    ///
    /// L'ordre d'essai (client → serveur → cible passive) est ce qui autorise un
    /// nœud servant à être AUSSI extrémité d'un autre circuit (limitation levée).
    async fn handle_relay(
        &self,
        m: RelayMsg,
        from: SocketAddr,
        peer_node: NodeId,
        peer_static: [u8; 32],
    ) -> Result<(), TransportError> {
        let now = self.clock.now_ms();
        match m {
            RelayMsg::Open { target } => {
                // Décision synchrone sous verrou, réponse émise après relâche.
                let reply = self.relay_open(NodeId(target), peer_node, now);
                self.send(from, &reply).await
            }
            RelayMsg::Data { circuit, blob } => {
                self.on_relay_data(circuit, blob, from, peer_node, peer_static, now)
                    .await
            }
            RelayMsg::Close { circuit } => {
                // Fermeture liée à la PROVENANCE (`peer_node`), pour interdire
                // qu'un tiers ferme un circuit qui ne le concerne pas (FAILLE D) :
                // les identifiants sont petits et devinables.
                let disconnected = {
                    let mut st = self.state_lock();
                    // Copie des champs utiles sans conserver l'emprunt de `st`.
                    let client_hit = st
                        .client_circuits
                        .get(&circuit)
                        .map(|cc| (cc.relay_node, cc.relay_addr, cc.session_id));
                    match client_hit {
                        // Extrémité cliente : n'accepter la fermeture QUE du relais
                        // qui héberge ce circuit.
                        Some((relay_node, relay_addr, sid)) if relay_node == peer_node => {
                            st.client_circuits.remove(&circuit);
                            st.relay_pending.remove(&circuit);
                            if let Some(sid) = sid {
                                st.sessions_by_id.remove(&sid);
                            }
                            Some(relay_addr)
                        }
                        // Circuit client existant, mais le CLOSE n'émane pas de
                        // notre relais : provenance invalide, on ignore.
                        Some(_) => None,
                        // Pas un circuit client : rôle serveur. Le circuit hébergé
                        // n'est fermé que si `peer_node` en est une extrémité.
                        None => {
                            st.relay.close_by(circuit, peer_node);
                            None
                        }
                    }
                };
                if let Some(addr) = disconnected {
                    let _ = self.events.send(TransportEvent::Disconnected { addr });
                }
                Ok(())
            }
            // Rôle CLIENT/initiateur : résout l'ouverture en attente (API
            // `open_relay_circuit`) ET émet l'événement historique.
            RelayMsg::Accept { circuit } => {
                self.resolve_relay_open(peer_node, from, Ok(circuit));
                let _ = self.events.send(TransportEvent::RelayAccepted {
                    circuit,
                    relay: peer_node,
                });
                Ok(())
            }
            RelayMsg::Reject { code } => {
                self.resolve_relay_open(peer_node, from, Err(code));
                let _ = self.events.send(TransportEvent::RelayRejected {
                    code,
                    relay: peer_node,
                });
                Ok(())
            }
        }
    }

    /// Traite un `RelayMsg::Data{circuit, blob}` reçu du relais `relay_node`
    /// (adresse `relay_addr`). Voir [`Endpoint::handle_relay`] pour l'ordre des
    /// rôles ; `relay_static` sert au repli verbatim historique.
    async fn on_relay_data(
        &self,
        circuit: u32,
        blob: Vec<u8>,
        relay_addr: SocketAddr,
        relay_node: NodeId,
        relay_static: [u8; 32],
        now: u64,
    ) -> Result<(), TransportError> {
        // 1. Extrémité cliente : le blob est un paquet A↔B tunnelé → réinjection.
        //    On EXIGE que le circuit nous vienne de SON relais (`relay_node`) : les
        //    identifiants de circuit sont propres à chaque relais, donc un même id
        //    peut coexister comme circuit CLIENT (via ce relais) et comme circuit
        //    SERVI (id de notre propre table). La provenance tranche sans ambiguïté
        //    et lève ainsi la limitation « un nœud servant ne peut être extrémité ».
        let is_client = {
            let st = self.state_lock();
            st.client_circuits
                .get(&circuit)
                .is_some_and(|cc| cc.relay_node == relay_node)
        };
        if is_client {
            // `Box::pin` : la réinjection ré-entre dans le pipeline (on_data →
            // dispatch → handle_relay → on_relay_data), un cycle d'`async fn` qui
            // exige une indirection pour rester de taille finie.
            return Box::pin(self.reinject_tunnel_packet(blob, relay_addr, circuit)).await;
        }

        // 2. Rôle serveur : acheminer si l'on héberge ce circuit.
        if self.config.relay_serving {
            let decision = {
                let mut st = self.state_lock();
                st.relay.forward(circuit, relay_node, blob.len(), now)
            };
            match decision {
                RelayDecision::Forward(other_node) => {
                    // Recopie octet pour octet : blob opaque, aucune inspection.
                    let msg = ChannelMsg::Relay(RelayMsg::Data { circuit, blob });
                    return self.send_to_node(other_node, &msg).await;
                }
                // Plafond de débit atteint : on abandonne ce blob en silence.
                RelayDecision::Throttled => return Ok(()),
                // Pas un circuit hébergé : on tente l'ouverture entrante (3).
                RelayDecision::Unknown => {}
            }
        }

        // 3. Cible passive : un HELLO tunnelé ouvre un circuit dont ce nœud est
        //    l'extrémité. Le HELLO est validé par le handshake standard (PoW,
        //    signature, fraîcheur) ; aucune identité n'est présumée.
        if let Ok(Packet::Hello(hello)) = Packet::from_bytes(&blob) {
            return self
                .accept_inbound_circuit(circuit, relay_addr, relay_node, hello)
                .await;
        }

        // Repli historique : pour un nœud NON servant, un blob opaque sans
        // handshake reconnaissable est remonté verbatim (compat rôle relais). Un
        // nœud servant, lui, laisse tomber un circuit inconnu (déjà décidé en 2).
        if !self.config.relay_serving {
            let _ = self.events.send(TransportEvent::Message {
                from: relay_node,
                addr: relay_addr,
                static_pub: relay_static,
                msg: Box::new(ChannelMsg::Relay(RelayMsg::Data { circuit, blob })),
            });
        }
        Ok(())
    }

    /// Réinjecte un paquet transport tunnelé (`blob`) reçu sur `circuit` du relais
    /// `relay_addr` : on le décode et on l'aiguille par le MÊME chemin que
    /// [`Endpoint::handle_datagram`], mais en identifiant le lien par le CIRCUIT
    /// (le pair n'a pas d'adresse directe joignable). Le tunnel est ainsi
    /// transparent pour tout l'étage handshake/session existant.
    async fn reinject_tunnel_packet(
        &self,
        blob: Vec<u8>,
        relay_addr: SocketAddr,
        circuit: u32,
    ) -> Result<(), TransportError> {
        let packet = Packet::from_bytes(&blob)?;
        let link = PeerLink::Tunnel {
            relay_addr,
            circuit,
        };
        match packet {
            Packet::Hello(hello) => self.on_hello(hello, link).await,
            Packet::Welcome(welcome) => self.on_welcome(welcome, link).await,
            // `on_data` retrouve la session par `session_id` et dérive le lien
            // tunnel via `relay_circuit` : l'adresse passée est ignorée pour une
            // session relayée (pas de mobilité).
            Packet::Data(data) => self.on_data(data, relay_addr).await,
            Packet::Cookie(cookie) => self.on_cookie(cookie, link).await,
        }
    }

    /// Enregistre un circuit entrant dont ce nœud est la cible passive, puis
    /// traite le HELLO tunnelé en répondeur (le WELCOME repartira dans le même
    /// circuit). Idempotent sur retransmission (le cache de nonces du handshake
    /// écarte les rejeux).
    async fn accept_inbound_circuit(
        &self,
        circuit: u32,
        relay_addr: SocketAddr,
        relay_node: NodeId,
        hello: Hello,
    ) -> Result<(), TransportError> {
        let peer_static = hello.static_pub;
        let peer_node = node_id_of(&peer_static);
        let now = self.clock.now_ms();

        // FAILLE C-bis (slot réservé AVANT validation) : ne réserver un slot du
        // plafond global qu'après avoir vérifié la PREUVE DE TRAVAIL du HELLO.
        // C'est le seul contrôle de validité disponible SANS état à ce point (la
        // signature, la fraîcheur et le cache de nonces restent vérifiés par
        // `respond` en aval, via `on_hello`). Un HELLO au `pow_nonce` invalide —
        // exactement le flux de garbage à coût crypto nul décrit par la faille —
        // est écarté AVANT toute insertion : il ne consomme plus aucun slot, et le
        // rate-limiter par IP (interne à `on_hello`) n'est même pas sollicité.
        //
        // On applique la MÊME difficulté (`config.pow_bits`) que `respond` : aucun
        // HELLO légitime n'est donc rejeté ici qui aurait passé le handshake.
        //
        // Remarque de conception : réserver seulement après le `respond()` COMPLET
        // serait plus strict, mais casserait l'invariant vérifié par
        // `faille_c_client_circuits_plafonnes_contre_faux_hello_tunneles` (un pair
        // unique remplit le plafond global de circuits) — le rate-limit par IP
        // bornerait alors les établissements bien en deçà du plafond. Le garde PoW
        // + le rollback ci-dessous sont l'équilibre retenu (voir revue de sécurité).
        if !verify_pow(&peer_static, hello.pow_nonce, self.config.pow_bits) {
            tracing::debug!(circuit, "HELLO tunnelé à PoW invalide : aucun slot réservé");
            return Ok(());
        }

        let reserved = {
            let mut st = self.state_lock();
            match st.client_circuits.get(&circuit) {
                // Collision d'identifiant entre deux relais distincts : on ne peut
                // pas héberger deux circuits clients de même id. On ignore ce HELLO
                // plutôt que de détourner le circuit existant.
                Some(cc) if cc.relay_node != relay_node => return Ok(()),
                // Même relais : retransmission d'un HELLO (le cache de nonces du
                // handshake écartera le rejeu dans `on_hello`). L'entrée existe déjà,
                // ce traitement n'en réserve pas : pas de rollback à sa charge.
                Some(_) => false,
                None => {
                    // Plafond anti-DoS (FAILLE C) : un flux de faux HELLO tunnelés
                    // (circuits frais) ne doit pas faire croître `client_circuits`
                    // sans borne, y compris sur un nœud non-relais. Au-delà du
                    // plafond, on ignore ce HELLO : le circuit n'est pas ouvert et
                    // le handshake n'est pas traité.
                    if st.client_circuits.len() >= MAX_CLIENT_CIRCUITS {
                        return Ok(());
                    }
                    st.client_circuits.insert(
                        circuit,
                        ClientCircuit {
                            relay_addr,
                            relay_node,
                            peer_static,
                            peer_node,
                            session_id: None,
                            created_ms: now,
                        },
                    );
                    true
                }
            }
        };

        // Validation complète + réponse répondeur. `respond` (via `on_hello`)
        // vérifie signature, fraîcheur et rejeu de nonce, et n'établit la session
        // qu'en cas de succès.
        let outcome = self
            .on_hello(
                hello,
                PeerLink::Tunnel {
                    relay_addr,
                    circuit,
                },
            )
            .await;

        // Rollback (FAILLE C-bis) : si le handshake ÉCHOUE (`Err` de `respond` —
        // signature invalide, rejeu, fraîcheur), on retire l'entrée que CE
        // traitement vient de réserver, pour qu'un HELLO à PoW valide mais
        // signature invalide ne laisse pas de slot `session_id: None` en suspens
        // jusqu'au balayage. On ne retire que NOTRE réservation (`reserved`), et
        // uniquement si aucune session ne s'y est installée entre-temps.
        if reserved && outcome.is_err() {
            let mut st = self.state_lock();
            if let Some(cc) = st.client_circuits.get(&circuit) {
                if cc.relay_node == relay_node && cc.session_id.is_none() {
                    st.client_circuits.remove(&circuit);
                    st.relay_pending.remove(&circuit);
                }
            }
        }
        outcome
    }

    /// Résout une ouverture de circuit en attente sur réception d'`Accept`/`Reject`
    /// du relais `relay_node` (adresse `relay_addr`).
    ///
    /// Le protocole `Accept`/`Reject` ne rappelle PAS la cible : la résolution
    /// s'appuie donc sur `relay_node` seul. On sélectionne la première ouverture
    /// en attente vers ce relais. Dans le cas nominal (une ouverture en vol par
    /// relais, ce que garantit l'usage `open_relay_circuit` puis attente), c'est
    /// exact ; des ouvertures concurrentes vers un MÊME relais se résoudraient
    /// dans un ordre non spécifié (limitation inhérente au format `accord-proto`,
    /// non modifiable ici).
    fn resolve_relay_open(
        &self,
        relay_node: NodeId,
        relay_addr: SocketAddr,
        result: Result<u32, u8>,
    ) {
        let now = self.clock.now_ms();
        let resolved = {
            let mut st = self.state_lock();
            let key = st
                .pending_relay_open
                .keys()
                .find(|k| k.0 == relay_node)
                .copied();
            let Some(key) = key else {
                return; // aucune ouverture en attente (ex. Accept déjà consommé)
            };
            // La clé vient d'être trouvée sous CE MÊME verrou : l'absence est
            // impossible en pratique — repli silencieux plutôt que panic (D23).
            let Some(po) = st.pending_relay_open.remove(&key) else {
                return;
            };
            if let Ok(circuit) = result {
                let (_, peer_node) = key;
                // NOTE LOW : ne JAMAIS écraser une entrée existante ni dépasser le
                // plafond sur cette voie d'insertion. Un relais qui refermerait puis
                // réattribuerait le même id de circuit à une AUTRE identité ferait
                // sinon pointer un même numéro vers un pair différent (l'AEAD et la
                // liaison D-037 tiennent, mais une couche appelante pourrait s'y
                // méprendre). En cas de collision ou de plafond atteint, on ignore
                // l'`Accept` (l'entrée en place reste intacte) ; l'appelant obtient
                // le circuit annoncé mais un `connect_via_relay` ultérieur échouera
                // proprement (`UnknownPeer`) plutôt que de corrompre un circuit vif.
                if st.client_circuits.contains_key(&circuit) {
                    tracing::warn!(
                        circuit,
                        "Accept relais réattribuant un circuit client déjà en service : ignoré"
                    );
                } else if st.client_circuits.len() >= MAX_CLIENT_CIRCUITS {
                    tracing::warn!(
                        circuit,
                        "Accept relais au-delà du plafond de circuits clients : ignoré"
                    );
                } else {
                    st.client_circuits.insert(
                        circuit,
                        ClientCircuit {
                            relay_addr,
                            relay_node,
                            peer_static: po.peer_static,
                            peer_node,
                            session_id: None,
                            created_ms: now,
                        },
                    );
                }
            }
            (po.tx, result)
        };
        // `oneshot::send` est synchrone ; l'erreur (récepteur abandonné) est
        // bénigne — l'appelant n'attend plus.
        let _ = resolved.0.send(resolved.1);
    }

    /// Décide de la réponse à un `Open` : `Accept` ou `Reject` (synchrone).
    ///
    /// Sécurité (SPEC §10) : on ne relaie JAMAIS si le service est désactivé, et
    /// l'on n'ouvre un circuit que si une session avec la cible existe déjà. Les
    /// plafonds de la [`RelayTable`] (débit par circuit, nombre de circuits) sont
    /// l'unique barrière restante et ne sont pas contournés.
    fn relay_open(&self, target: NodeId, initiator: NodeId, now: u64) -> ChannelMsg {
        if !self.config.relay_serving {
            return ChannelMsg::Relay(RelayMsg::Reject {
                code: REJECT_NOT_RELAY,
            });
        }
        let mut st = self.state_lock();
        if Self::addr_of_node(&st, &target).is_none() {
            return ChannelMsg::Relay(RelayMsg::Reject {
                code: REJECT_NO_TARGET,
            });
        }
        match st.relay.open(initiator, target, now) {
            Some(circuit) => ChannelMsg::Relay(RelayMsg::Accept { circuit }),
            None => ChannelMsg::Relay(RelayMsg::Reject { code: REJECT_FULL }),
        }
    }

    /// Adresse de la session établie avec `node`, si elle existe.
    fn addr_of_node(st: &State, node: &NodeId) -> Option<SocketAddr> {
        st.sessions_by_id
            .values()
            .find(|s| s.peer_node == *node)
            .map(|s| s.peer_addr)
    }

    /// Envoie un message applicatif au pair identifié par `node`, via l'adresse
    /// de sa session établie. No-op si aucune session n'existe avec ce nœud (on
    /// n'ouvre jamais de handshake ici : l'acheminement relais suppose la session
    /// déjà en place). Le verrou d'état n'est pas tenu pendant l'envoi réseau.
    async fn send_to_node(&self, node: NodeId, msg: &ChannelMsg) -> Result<(), TransportError> {
        let addr = {
            let st = self.state_lock();
            Self::addr_of_node(&st, &node)
        };
        match addr {
            Some(addr) => self.send(addr, msg).await,
            None => Ok(()),
        }
    }

    // --- Tunnel relais côté CLIENT (SPEC §10-§11.3) ----------------------------

    /// Émet un paquet transport BRUT (`bytes` : un HELLO/WELCOME/DATA/COOKIE déjà
    /// sérialisé) vers un pair via `link`. C'est l'abstraction d'envoi retenue
    /// (point 4 §11.3) :
    /// - `Direct` : envoi UDP direct ;
    /// - `Tunnel` : ré-enveloppe les octets dans `RelayMsg::Data{circuit, blob}` et
    ///   les scelle sous la session avec le relais. Le relais ne voit qu'un blob
    ///   opaque (paquet d'une session bout-en-bout dont il n'a pas les clés).
    ///
    /// Le verrou d'état n'est jamais tenu pendant l'envoi réseau.
    async fn send_packet_via_link(
        &self,
        bytes: Vec<u8>,
        link: PeerLink,
    ) -> Result<(), TransportError> {
        match link {
            PeerLink::Direct(addr) => {
                self.socket.send_to(&bytes, addr).await?;
                Ok(())
            }
            PeerLink::Tunnel {
                relay_addr,
                circuit,
            } => {
                let wrapped = ChannelMsg::Relay(RelayMsg::Data {
                    circuit,
                    blob: bytes,
                });
                // Scellé sous la session A↔R existante (chemin `send` standard).
                self.send(relay_addr, &wrapped).await
            }
        }
    }

    /// Envoie un message applicatif via `link` : chemin direct habituel, ou tunnel
    /// (chaque cadre DATA de la session A↔B est ré-enveloppé). Utilisé pour les
    /// réponses de contrôle (PONG, ObserveAddrResp) sur le chemin d'arrivée.
    async fn send_msg_via_link(
        &self,
        link: PeerLink,
        msg: &ChannelMsg,
    ) -> Result<(), TransportError> {
        match link {
            PeerLink::Direct(addr) => self.send(addr, msg).await,
            PeerLink::Tunnel { circuit, .. } => self.send_via_relay(circuit, msg).await,
        }
    }

    /// Ouvre un circuit relais vers `peer_static` À TRAVERS le relais `relay_addr`
    /// (SPEC §11.3). Prérequis : une session DIRECTE avec le relais doit déjà
    /// exister (sinon [`TransportError::UnknownPeer`]). Envoie `RelayMsg::Open` au
    /// relais et attend sa réponse : rend le `circuit` attribué sur `Accept`, ou
    /// [`TransportError::RelayOpenRejected`] sur `Reject`.
    ///
    /// N'initie PAS encore le handshake A↔B : appeler ensuite
    /// [`Endpoint::connect_via_relay`] avec le circuit rendu. La SÉLECTION du
    /// relais et le DÉCLENCHEMENT du repli sont hors périmètre (intégration nœud).
    pub async fn open_relay_circuit(
        self: &Arc<Self>,
        relay_addr: SocketAddr,
        relay_node: NodeId,
        peer_static: [u8; 32],
    ) -> Result<u32, TransportError> {
        let peer_node = node_id_of(&peer_static);
        // Prérequis : session DIRECTE avec le relais (une session relayée ne peut
        // pas elle-même porter un `Open`).
        {
            let st = self.state_lock();
            let has_direct = st
                .id_by_addr
                .get(&relay_addr)
                .and_then(|sid| st.sessions_by_id.get(sid))
                .is_some_and(|s| s.relay_circuit.is_none());
            if !has_direct {
                return Err(TransportError::UnknownPeer);
            }
        }
        let (tx, rx) = oneshot::channel();
        {
            let mut st = self.state_lock();
            st.pending_relay_open
                .insert((relay_node, peer_node), PendingOpen { peer_static, tx });
        }
        // Envoie l'Open via la session A↔R ; la réponse est résolue dans
        // `handle_relay` (Accept/Reject) → `resolve_relay_open`.
        let open = ChannelMsg::Relay(RelayMsg::Open {
            target: peer_node.0,
        });
        if let Err(e) = self.send(relay_addr, &open).await {
            let mut st = self.state_lock();
            st.pending_relay_open.remove(&(relay_node, peer_node));
            return Err(e);
        }
        // Attente BORNÉE : un relais silencieux (ni Accept ni Reject) ne doit pas
        // faire pendre l'appelant ni laisser fuir l'entrée `pending_relay_open`.
        match tokio::time::timeout(RELAY_OPEN_TIMEOUT, rx).await {
            Ok(Ok(Ok(circuit))) => Ok(circuit),
            Ok(Ok(Err(code))) => Err(TransportError::RelayOpenRejected(code)),
            Ok(Err(_)) => {
                // Émetteur abandonné (endpoint arrêté) : nettoyer l'entrée.
                let mut st = self.state_lock();
                st.pending_relay_open.remove(&(relay_node, peer_node));
                Err(TransportError::Shutdown)
            }
            Err(_) => {
                // Délai dépassé : abandon de l'ouverture et purge de l'attente.
                let mut st = self.state_lock();
                st.pending_relay_open.remove(&(relay_node, peer_node));
                Err(TransportError::RelayOpenTimeout)
            }
        }
    }

    /// Initie la session bout-en-bout A↔B À TRAVERS le circuit `circuit` déjà
    /// ouvert (voir [`Endpoint::open_relay_circuit`]). Lance un HELLO lié à
    /// l'identité `peer_static` (anti-MITM D-037) enveloppé dans le circuit. Le
    /// WELCOME du pair revient par le même circuit, est réinjecté, et la session
    /// s'installe (`peer_addr = relay_addr`, `relay_circuit = Some(circuit)`) —
    /// signalée par un [`TransportEvent::Connected`]. Ensuite, `send_via_relay`
    /// et les [`TransportEvent::Message`] fonctionnent normalement.
    ///
    /// `peer_static` est l'identité que l'appelant EXIGE au bout du circuit : si le
    /// WELCOME émane d'une autre clé (relais malveillant qui routerait ailleurs),
    /// la session est refusée dans `on_welcome`.
    pub async fn connect_via_relay(
        self: &Arc<Self>,
        circuit: u32,
        peer_static: [u8; 32],
    ) -> Result<(), TransportError> {
        let now = self.clock.now_ms();
        let (relay_addr, hello_bytes) = {
            let mut st = self.state_lock();
            let Some(cc) = st.client_circuits.get(&circuit) else {
                return Err(TransportError::UnknownPeer);
            };
            let relay_addr = cc.relay_addr;
            // Handshake initiateur lié à `peer_static` (le WELCOME devra en émaner).
            let initiator = Initiator::start(
                &self.identity,
                now,
                Vec::new(),
                self.config.pow_bits,
                Some(peer_static),
            );
            let hello = Packet::Hello(initiator.hello().clone()).to_bytes();
            st.relay_pending.insert(
                circuit,
                Pending {
                    initiator,
                    expected_static: Some(peer_static),
                    queued: Vec::new(),
                    attempts: 1,
                    last_send_ms: now,
                },
            );
            (relay_addr, hello)
        };
        // HELLO enveloppé dans le circuit (scellé sous la session A↔R).
        self.send_packet_via_link(
            hello_bytes,
            PeerLink::Tunnel {
                relay_addr,
                circuit,
            },
        )
        .await
    }

    /// Envoie un message applicatif au pair joignable par le `circuit` relais.
    /// Chaque cadre DATA de la session bout-en-bout est ré-enveloppé dans
    /// `RelayMsg::Data{circuit, blob}` et scellé sous la session avec le relais.
    /// Erreur si le circuit est inconnu ou si la session A↔B n'est pas encore
    /// établie ([`TransportError::UnknownPeer`]).
    pub async fn send_via_relay(
        &self,
        circuit: u32,
        msg: &ChannelMsg,
    ) -> Result<(), TransportError> {
        let plaintext = msg.to_bytes();
        if plaintext.len() > frag::MAX_MESSAGE_LEN {
            return Err(TransportError::TooLarge);
        }
        let now = self.clock.now_ms();
        let (relay_addr, frames) = {
            let mut st = self.state_lock();
            let (relay_addr, sid) = {
                let cc = st
                    .client_circuits
                    .get(&circuit)
                    .ok_or(TransportError::UnknownPeer)?;
                (
                    cc.relay_addr,
                    cc.session_id.ok_or(TransportError::UnknownPeer)?,
                )
            };
            // Jamais d'`expect` infaillible sur un invariant CROISÉ (client_circuits
            // ↔ sessions_by_id) sous un verrou à large rayon : une incohérence
            // transitoire (session expirée par la maintenance) rendrait une erreur
            // gérée, pas une panic qui empoisonnerait le mutex (FAILLE B-3).
            let session = st
                .sessions_by_id
                .get_mut(&sid)
                .ok_or(TransportError::UnknownPeer)?;
            let frames = Self::seal_frames(session, &plaintext, now)?;
            (relay_addr, frames)
        };
        // Chaque cadre est ré-enveloppé et scellé sous la session A↔R.
        for frame in frames {
            let wrapped = ChannelMsg::Relay(RelayMsg::Data {
                circuit,
                blob: frame,
            });
            self.send(relay_addr, &wrapped).await?;
        }
        Ok(())
    }

    /// Circuit relais dont ce nœud est extrémité et qui joint le pair `peer_node`,
    /// le cas échéant. Permet à une couche haute de répondre à un pair reçu via un
    /// tunnel ([`TransportEvent::Message`] porte le NodeId de l'émetteur).
    pub fn circuit_for_peer(&self, peer_node: NodeId) -> Option<u32> {
        let st = self.state_lock();
        st.client_circuits
            .iter()
            .find(|(_, cc)| cc.peer_node == peer_node)
            .map(|(circuit, _)| *circuit)
    }

    /// Descripteur d'un circuit relais client : `(relay_addr, relay_node,
    /// peer_static)`, le cas échéant. Exposé pour la couche nœud (observabilité,
    /// et sélection/ré-ouverture du relais, incrément suivant hors périmètre ici).
    pub fn relay_circuit_descriptor(&self, circuit: u32) -> Option<(SocketAddr, NodeId, [u8; 32])> {
        let st = self.state_lock();
        st.client_circuits
            .get(&circuit)
            .map(|cc| (cc.relay_addr, cc.relay_node, cc.peer_static))
    }

    /// Installe une session et RETOURNE les messages applicatifs qui étaient
    /// en file sur le `Pending` évincé par cette installation — à re-sceller
    /// sous la nouvelle session par l'appelant. Décisif au simultaneous-open
    /// (SPEC §11.2) : le côté qui devient RÉPONDEUR (arbitrage `on_hello`) voit
    /// son propre `Pending` initiateur retiré ici ; sans récupérer sa file,
    /// tous les messages qu'il avait mis en attente (typiquement l'annonce de
    /// profil/bannière à la reconnexion) seraient jetés en silence — le bug
    /// « la bannière de mon ami n'arrive jamais ».
    #[must_use]
    fn install_session(st: &mut State, session: Session, sid: [u8; 8]) -> Vec<Vec<u8>> {
        match session.relay_circuit {
            Some(circuit) => {
                // Session relayée : indexée par circuit, JAMAIS par adresse
                // (`relay_addr` est déjà pris dans `id_by_addr` par la session avec
                // le relais — l'y insérer détournerait le trafic A↔R). Le lien
                // circuit → session_id vit dans `client_circuits`.
                if let Some(cc) = st.client_circuits.get_mut(&circuit) {
                    if let Some(old_sid) = cc.session_id.replace(sid) {
                        if old_sid != sid {
                            st.sessions_by_id.remove(&old_sid);
                        }
                    }
                }
                st.sessions_by_id.insert(sid, session);
                // Les rôles d'un tunnel sont fixes (seul l'initiateur ouvre le
                // circuit) : le pending relais retiré n'a pas de file croisée.
                st.relay_pending.remove(&circuit);
                Vec::new()
            }
            None => {
                let addr = session.peer_addr;
                // Remplace une éventuelle session antérieure vers cette adresse.
                if let Some(old_sid) = st.id_by_addr.insert(addr, sid) {
                    if old_sid != sid {
                        st.sessions_by_id.remove(&old_sid);
                    }
                }
                st.sessions_by_id.insert(sid, session);
                st.pending
                    .remove(&addr)
                    .map(|p| p.queued)
                    .unwrap_or_default()
            }
        }
    }

    /// Ferme la session jointe par `link` : par adresse (direct) ou par circuit
    /// (tunnel). Symétrique de [`Endpoint::install_session`].
    fn close_link(&self, link: PeerLink) {
        let mut st = self.state_lock();
        match link {
            PeerLink::Direct(addr) => {
                if let Some(sid) = st.id_by_addr.remove(&addr) {
                    st.sessions_by_id.remove(&sid);
                }
                st.pending.remove(&addr);
            }
            PeerLink::Tunnel { circuit, .. } => {
                if let Some(cc) = st.client_circuits.remove(&circuit) {
                    if let Some(sid) = cc.session_id {
                        st.sessions_by_id.remove(&sid);
                    }
                }
                st.relay_pending.remove(&circuit);
            }
        }
    }

    /// Nombre de sessions établies (observabilité/tests).
    pub fn session_count(&self) -> usize {
        self.state_lock().sessions_by_id.len()
    }

    /// Nombre de circuits relais dont ce nœud est extrémité CLIENTE
    /// (observabilité/tests). Distinct de [`Endpoint::session_count`] : borne
    /// surveillée par le plafond anti-DoS [`MAX_CLIENT_CIRCUITS`].
    pub fn client_circuit_count(&self) -> usize {
        self.state_lock().client_circuits.len()
    }

    /// Déclenche immédiatement un tour de maintenance (retransmissions de
    /// handshake, keep-alive, expiration des sessions et balayage anti-DoS des
    /// circuits inachevés). En production, la boucle interne
    /// [`Endpoint::maintenance_loop`] l'appelle périodiquement ; exposé pour un
    /// pilotage déterministe (embarqueur à ordonnancement propre, tests).
    pub async fn run_maintenance(&self) {
        self.maintenance_tick().await;
    }

    async fn maintenance_loop(self: Arc<Self>) {
        loop {
            if self.is_shutdown() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            self.maintenance_tick().await;
        }
    }

    async fn maintenance_tick(&self) {
        let now = self.clock.now_ms();
        let mut sends: Vec<(Vec<u8>, SocketAddr)> = Vec::new();
        let mut disconnected: Vec<SocketAddr> = Vec::new();
        // Keep-alives à router À TRAVERS un tunnel (FAILLE B-1) :
        // `(relay_addr, circuit, cadre_chiffré)`. Ré-enveloppés APRÈS relâche du
        // verrou, jamais poussés bruts vers le relais (qui les rejetterait, faute
        // de connaître le `session_id` bout-en-bout).
        let mut tunnel_keepalives: Vec<(SocketAddr, u32, Vec<u8>)> = Vec::new();
        {
            let mut st = self.state_lock();

            // Retransmission des HELLO (timeout 2 s, 2 retransmissions).
            let mut abandon: Vec<SocketAddr> = Vec::new();
            for (addr, pending) in st.pending.iter_mut() {
                if now.saturating_sub(pending.last_send_ms)
                    >= accord_proto::limits::DHT_RPC_TIMEOUT_MS
                {
                    if pending.attempts > accord_proto::limits::DHT_RPC_RETRIES {
                        abandon.push(*addr);
                    } else {
                        pending.attempts += 1;
                        pending.last_send_ms = now;
                        sends.push((
                            Packet::Hello(pending.initiator.hello().clone()).to_bytes(),
                            *addr,
                        ));
                    }
                }
            }
            for addr in abandon {
                st.pending.remove(&addr);
            }

            // Keep-alive et expiration de session.
            let keepalive = self.config.keepalive_ms;
            let idle = self.config.idle_timeout_ms;
            let mut expire: Vec<[u8; 8]> = Vec::new();
            for (sid, session) in st.sessions_by_id.iter_mut() {
                if now.saturating_sub(session.last_recv_ms) >= idle {
                    expire.push(*sid);
                    // Une session relayée n'est pas indexée par adresse : son
                    // événement Disconnected est émis à la purge ci-dessous (où
                    // l'on tient `relay_addr`).
                    if session.relay_circuit.is_none() {
                        disconnected.push(session.peer_addr);
                    }
                    continue;
                }
                // Libère les réassemblages partiels expirés (timeout 30 s).
                session.reasm.sweep(now);
                if now.saturating_sub(session.last_send_ms) >= keepalive {
                    let mut token = [0u8; 8];
                    OsRng.fill_bytes(&mut token);
                    let token = u64::from_be_bytes(token);
                    let ping = ChannelMsg::Control(ControlMsg::Ping { token });
                    // Mesure de latence : le PONG corrélé à CE jeton donnera le
                    // dernier aller-retour (voir `note_pong`). Un cycle non
                    // soldé est simplement remplacé au keep-alive suivant.
                    session.ping_pending = Some((token, now));
                    // Un PING tient dans un cadre unique (framing SPEC §13.1).
                    let addr = session.peer_addr;
                    let relay_circuit = session.relay_circuit;
                    if let Ok(frames) = Self::seal_frames(session, &ping.to_bytes(), now) {
                        for bytes in frames {
                            match relay_circuit {
                                // Session relayée : le keep-alive doit voyager DANS
                                // le tunnel (ré-enveloppé + scellé sous A↔R), sinon
                                // le relais le rejette et la session tunnelée finit
                                // par expirer à tort (FAILLE B-1).
                                Some(circuit) => tunnel_keepalives.push((addr, circuit, bytes)),
                                None => sends.push((bytes, addr)),
                            }
                        }
                    }
                }
            }
            for sid in expire {
                if let Some(s) = st.sessions_by_id.remove(&sid) {
                    match s.relay_circuit {
                        // Session relayée : elle n'est PAS indexée par adresse
                        // (`relay_addr` appartient à la session A↔R). On nettoie les
                        // tables de circuit de façon atomique — sinon
                        // `client_circuits[c].session_id` pointerait un sid mort
                        // (FAILLE B-2) et un `send_via_relay` ultérieur paniquerait.
                        Some(circuit) => {
                            if let Some(cc) = st.client_circuits.remove(&circuit) {
                                disconnected.push(cc.relay_addr);
                            }
                            st.relay_pending.remove(&circuit);
                        }
                        None => {
                            st.id_by_addr.remove(&s.peer_addr);
                        }
                    }
                }
            }

            // --- Balayage anti-DoS des circuits clients inachevés (FAILLE C) ---
            // Une entrée `client_circuits` restée `session_id: None` au-delà du
            // délai de handshake (WELCOME jamais reçu — p.ex. relais silencieux, ou
            // faux HELLO tunnelé) est purgée avec son éventuel `relay_pending`.
            // Le plafond `MAX_CLIENT_CIRCUITS` borne l'instantané ; ce balayage
            // borne la DURÉE de vie des entrées inachevées (et abandonne du même
            // coup les `relay_pending` jamais retentés).
            let mut stale_circuits: Vec<u32> = Vec::new();
            for (circuit, cc) in st.client_circuits.iter() {
                if cc.session_id.is_none()
                    && now.saturating_sub(cc.created_ms) >= CLIENT_CIRCUIT_HANDSHAKE_TIMEOUT_MS
                {
                    stale_circuits.push(*circuit);
                }
            }
            for circuit in stale_circuits {
                st.client_circuits.remove(&circuit);
                st.relay_pending.remove(&circuit);
            }
        }
        for (bytes, addr) in sends {
            let _ = self.socket.send_to(&bytes, addr).await;
        }
        // Keep-alives tunnelés : ré-enveloppés dans `RelayMsg::Data` et scellés
        // sous la session avec le relais (chemin `send` standard), hors verrou.
        for (relay_addr, circuit, blob) in tunnel_keepalives {
            let _ = self
                .send_packet_via_link(
                    blob,
                    PeerLink::Tunnel {
                        relay_addr,
                        circuit,
                    },
                )
                .await;
        }
        for addr in disconnected {
            let _ = self.events.send(TransportEvent::Disconnected { addr });
        }
    }
}
