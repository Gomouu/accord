//! Assemblage du nœud Accord : identité, base, transport, DHT, cœur et API
//! locale câblés en un runtime unique. Consommé par le démon `accord-noded`
//! et par l'hôte Tauri.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod backup;
pub mod device;
pub mod error;
pub mod hex;
pub mod identity;
pub mod maintenance;
pub mod node;
pub mod outbound;
pub mod pairing;
pub mod registry;
pub mod runtime;
pub mod service;
pub mod voice;

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use accord_api::{ApiServer, AuthToken, NotificationHub};
use accord_core::db::Db;
use accord_dht::{DhtConfig, KademliaNode};
use accord_proto::types::{node_flags, NodeInfo, WireAddr};
use accord_transport::{DatagramSocket, Endpoint, EndpointConfig, UdpDatagram};

pub use error::NodeError;
pub use identity::{Paths, Unlocked};
pub use maintenance::MaintenanceConfig;
pub use node::diagnostics::{CountersSnapshot, ProbeResult, Reachability, SelfTestReport};
pub use node::network::{LinkTransport, NetworkStatus, PeerLink};
pub use node::Node;
pub use registry::{AccountEntry, Registry};
pub use service::NodeService;
pub use voice::{
    CallPhase, CallSnapshot, VoiceBackend, VoiceDevices, VoiceHandle, VoiceParticipant, VoiceStatus,
};

use outbound::OutboundSink;
use runtime::{Runtime, TransportDhtRpc};

/// Capacité du canal d'actions sortantes.
const OUTBOUND_CAPACITY: usize = 1024;

/// Configuration de démarrage d'un nœud.
#[derive(Debug, Clone)]
pub struct NodeConfig {
    /// Répertoire de profil (identité + base).
    pub paths: Paths,
    /// Adresse UDP d'écoute P2P. L'IP fixe l'interface (`0.0.0.0` : toutes).
    /// Le port `0` déclenche la stratégie de port stable (B2) : port retenu au
    /// précédent lancement, sinon [`node::network::DEFAULT_P2P_PORT`] puis une
    /// plage de repli puis un port éphémère. Un port explicite non nul est
    /// tenté en priorité.
    pub p2p_addr: SocketAddr,
    /// Port de l'API locale (`0` : éphémère).
    pub api_port: u16,
    /// Difficulté PoW exigée des pairs.
    pub pow_bits: u32,
    /// Mode du sous-système voix (matériel par défaut ; simulé pour les
    /// tests : codec pur et capture injectée, déterministe sans périphérique).
    pub voice_backend: VoiceBackend,
    /// Active le mapping de port automatique (UPnP-IGD puis NAT-PMP/PCP) au
    /// démarrage. Sans effet si l'écoute est en loopback (rien à mapper). En cas
    /// d'échec, dégradation propre : le nœud démarre sans mapping.
    pub nat_enabled: bool,
    /// Active l'annonce et la découverte de pairs Accord sur le réseau local
    /// (mDNS). Sans effet si l'écoute est en loopback.
    pub mdns_enabled: bool,
    /// Le transport présente-t-il la **clé d'appareil** au lieu de la clé de
    /// compte (multi-appareil, jalon 1, lot 1.C phase 2) ?
    ///
    /// **Vrai par défaut depuis la 7.0.** C'est lui qui lève le bloqueur B1 :
    /// l'invariant « au plus une session directe par identité »
    /// devient « par appareil » sans qu'une ligne d'`install_session` ne
    /// change, et deux machines d'un même compte cessent de s'évincer chez
    /// chacun de leurs amis.
    ///
    /// 🔒 Le basculement n'était tenable qu'une fois la moitié « savoir lire »
    /// déployée (6.3.0, phase 1) : `device::account_for_static` rattache une clé
    /// d'appareil listée à son compte, et `DEVICE_FLAG_TRANSPORT_KEY` dit aux
    /// correspondants laquelle des deux clés cette machine présente. Sans
    /// elles, le premier à basculer serait devenu un inconnu pour tous ses
    /// amis. Même discipline que le champ de capacités du handshake : savoir
    /// lire d'abord, écrire ensuite (`docs/MULTI_DEVICE.md` §3.2.1).
    ///
    /// ⚠️ Le champ **reste** configurable, et pas seulement pour les tests : le
    /// remettre à `false` est le retour en arrière d'urgence si le basculement
    /// se passait mal sur le terrain. `reconcile_local_device_flags` gère les
    /// deux sens — la liste d'appareils republie un drapeau retiré aussi bien
    /// qu'un drapeau posé.
    pub device_key_transport: bool,
    /// Nœuds d'amorçage/relais PAR DÉFAUT livrés avec l'application (points
    /// d'entrée du réseau, à la manière des bootstrap nodes d'IPFS/BitTorrent —
    /// ce ne sont PAS des serveurs centraux : ils ne voient que du trafic
    /// chiffré et ne font que router/relayer). Fusionnés avec les pairs
    /// d'amorçage ajoutés par l'utilisateur.
    ///
    /// Indispensable au premier contact « zéro port » entre deux pairs tous
    /// deux derrière un NAT symétrique : sans rendez-vous joignable COMMUN,
    /// deux amis qui ne s'amorcent que l'un sur l'autre ne peuvent jamais se
    /// joindre (aucun des deux n'est joignable). Ces nœuds fournissent ce
    /// rendez-vous partagé automatiquement. L'hôte les peuple depuis une
    /// constante de build ou la variable d'environnement `ACCORD_BOOTSTRAP`.
    pub default_bootstrap: Vec<SocketAddr>,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            paths: Paths::new("."),
            p2p_addr: SocketAddr::new(std::net::Ipv4Addr::UNSPECIFIED.into(), 0),
            api_port: 0,
            pow_bits: accord_proto::limits::IDENTITY_POW_BITS,
            voice_backend: VoiceBackend::default(),
            nat_enabled: true,
            mdns_enabled: true,
            // 🔒 Le basculement. L'invariant « au plus une session directe par
            // identité » devient « par appareil » sans qu'une ligne
            // d'`install_session` ne change, et deux machines d'un même compte
            // cessent de s'évincer chez chacun de leurs amis.
            //
            // ⚠️ Il n'y a pas de chemin de compatibilité : un point d'entrée
            // présente une seule identité statique. Un pair antérieur à la
            // 6.3.0 ne sait pas rattacher une clé d'appareil à un compte et
            // voit un inconnu ; un pair en 6.3.0 le sait pour les comptes avec
            // lesquels il est déjà en relation, mais pas au tout PREMIER
            // contact — il enregistrerait alors l'amitié sous la clé d'une
            // machine. La moitié « savoir recevoir » qui corrige ça est dans la
            // 6.4.0. Les amitiés déjà établies traversent le basculement.
            //
            // Le champ reste réglable, et pas seulement pour les tests : le
            // remettre à `false` est le retour en arrière d'urgence.
            // `reconcile_local_device_flags` gère les deux sens.
            device_key_transport: true,
            default_bootstrap: Vec::new(),
        }
    }
}

/// Analyse une liste de nœuds d'amorçage par défaut depuis une chaîne
/// `"ip:port,ip:port"` (variable `ACCORD_BOOTSTRAP` ou constante de build).
/// Les entrées invalides sont ignorées. Bornée à
/// [`node::network::MAX_BOOTSTRAP_PEERS`]. Fonction pure — testable.
pub fn parse_default_bootstrap(raw: &str) -> Vec<SocketAddr> {
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse::<SocketAddr>().ok())
        .take(node::network::MAX_BOOTSTRAP_PEERS)
        .collect()
}

/// Nœuds d'amorçage/relais par défaut à câbler dans [`NodeConfig`], résolus
/// depuis l'environnement : la variable d'EXÉCUTION `ACCORD_BOOTSTRAP` (permet
/// d'ajuster un déploiement sans recompiler) prime sur la constante de BUILD
/// `ACCORD_BOOTSTRAP` (`option_env!`, injectée à la compilation du release).
/// Vide si aucune n'est définie — le nœud démarre alors sans rendez-vous par
/// défaut (les amis doivent partager un pair joignable manuellement).
pub fn default_bootstrap_env() -> Vec<SocketAddr> {
    let runtime = std::env::var("ACCORD_BOOTSTRAP").ok();
    match runtime.as_deref().or(option_env!("ACCORD_BOOTSTRAP")) {
        Some(raw) => parse_default_bootstrap(raw),
        None => Vec::new(),
    }
}

/// Nœud en cours d'exécution : réseau + API actifs.
pub struct RunningNode {
    /// État applicatif partagé.
    pub node: Arc<Node>,
    /// Serveur API local.
    pub api: ApiServer,
    /// Jeton d'authentification de l'API (à transmettre à l'UI).
    pub token: AuthToken,
    endpoint: Arc<Endpoint>,
    runtime: Arc<Runtime>,
    voice: VoiceHandle,
}

impl RunningNode {
    /// Adresse P2P effective (port éphémère résolu).
    pub fn p2p_addr(&self) -> SocketAddr {
        self.endpoint.local_addr()
    }

    /// Enregistre l'adresse P2P d'un pair (normalement fournie par la
    /// résolution de présence DHT ; exposé pour l'amorçage et les tests).
    ///
    /// ⚠️ La clé attendue est celle du **transport**, pas celle du compte : le
    /// carnet est indexé par la clé que la machine présente, et depuis le
    /// basculement (lot 1.C phase 2) les deux diffèrent. Inscrire une adresse
    /// sous une clé de compte ne mène nulle part — l'envoi lié à l'identité
    /// refuserait une session ouverte sous une autre clé.
    pub fn register_peer(&self, pubkey: [u8; 32], addr: SocketAddr) {
        self.runtime.register_peer(pubkey, addr);
    }

    /// Apprend d'un autre nœud **les deux moitiés** de ce que la résolution de
    /// présence rend en production : sa liste d'appareils signée, puis
    /// l'adresse de la machine qu'elle désigne.
    ///
    /// **RÉSERVÉ AUX TESTS.** Un test de nœud sans DHT n'a aucun autre moyen
    /// d'apprendre la liste d'un pair, et depuis le basculement l'adresse seule
    /// ne suffit plus : sans la liste, la clé de compte du destinataire reste
    /// la seule cible connue, et plus aucune machine ne l'écoute
    /// (`docs/MULTI_DEVICE.md` §3.2.1). En production, ces deux moitiés
    /// arrivent de la DHT, dans cet ordre, à chaque passe de résolution.
    /// Réapprendre est **idempotent** : la garde de fraîcheur est celle de la
    /// passe de maintenance (`presence_resolve_tick`), et pour la même raison —
    /// réingérer une liste à version identique serait refusé comme un rejeu.
    /// Un test qui rejoue l'amorçage après un redémarrage en dépend.
    #[doc(hidden)]
    pub fn learn_peer(&self, other: &RunningNode) -> Result<(), NodeError> {
        let compte = other.node.public_key();
        if !self.node.has_fresh_device_list(&compte) {
            if let Some(record) = other.node.device_list_record() {
                self.node.learn_device_list_record(&compte, &record)?;
            }
        }
        self.register_peer(other.node.transport_key(), other.p2p_addr());
        Ok(())
    }

    /// Coordonnées DHT locales, à fournir comme graine d'amorçage à d'autres
    /// nœuds (exposé pour l'amorçage et les tests).
    pub fn node_info(&self) -> NodeInfo {
        self.runtime.local_node_info()
    }

    /// Amorce la table de routage DHT avec des nœuds connus (graines).
    pub async fn dht_bootstrap(&self, seeds: Vec<NodeInfo>) {
        self.runtime.dht_bootstrap(seeds).await;
    }

    /// Adresse de l'API locale.
    pub fn api_addr(&self) -> SocketAddr {
        self.api.local_addr()
    }

    /// Ajoute un pair d'amorçage (persisté) et l'ensemence immédiatement
    /// (handshake + DHT). Rend le statut réseau à jour.
    pub async fn add_bootstrap_peer(
        &self,
        addr: SocketAddr,
    ) -> Result<node::network::NetworkStatus, NodeError> {
        use node::network::NetworkControl;
        self.runtime.add_peer(addr).await
    }

    /// Retire un pair d'amorçage persisté. Rend le statut réseau à jour.
    pub async fn remove_bootstrap_peer(
        &self,
        addr: SocketAddr,
    ) -> Result<node::network::NetworkStatus, NodeError> {
        use node::network::NetworkControl;
        self.runtime.remove_peer(addr).await
    }

    /// Statut réseau courant (port, adresses locales, pairs, nœuds DHT).
    pub fn network_status(&self) -> node::network::NetworkStatus {
        use node::network::NetworkControl;
        self.runtime.status()
    }

    /// Lien courant vers chaque ami (diagnostic de connectivité par pair).
    pub fn peer_links(&self) -> Vec<node::network::PeerLink> {
        use node::network::NetworkControl;
        self.runtime.peer_links()
    }

    /// Photographie des compteurs réseau locaux (`diagnostics.counters`).
    pub fn diagnostics_counters(&self) -> node::diagnostics::CountersSnapshot {
        use node::network::NetworkControl;
        self.runtime.counters()
    }

    /// Auto-test réseau borné (`diagnostics.selftest`).
    pub async fn self_test(&self) -> node::diagnostics::SelfTestReport {
        use node::network::NetworkControl;
        self.runtime.self_test().await
    }

    /// Résout un code ami en clé publique via la DHT (repli d'amorçage
    /// inclus). Exposé pour l'hôte et les tests d'intégration.
    pub async fn resolve_friend_code(
        &self,
        code: &accord_crypto::FriendCode,
    ) -> Result<[u8; 32], NodeError> {
        use service::CodeResolver;
        self.runtime.resolve(code).await
    }

    /// Poignée du sous-système voix (salons vocaux ; exposée pour l'hôte et
    /// les tests — l'UI passe par les méthodes `voice.*` de l'API).
    pub fn voice(&self) -> &VoiceHandle {
        &self.voice
    }

    /// Arrête proprement le réseau, la maintenance, la voix et l'API.
    pub fn shutdown(&self) {
        // Prévient les amis joignables que l'on passe hors ligne, tant que le
        // réseau tourne encore (best-effort, jamais mis en file).
        let _ = self.node.broadcast_presence(false);
        self.voice.stop();
        self.runtime.stop();
        self.endpoint.shutdown();
        self.api.shutdown();
        // Casse le cycle d'`Arc` Runtime↔Node (Lot G, cause 3) : sans quoi le
        // runtime — et son socket UDP — fuit à chaque verrouillage.
        self.node.clear_network_control();
    }
}

/// Démarre un nœud complet avec la maintenance réseau par défaut (D-024).
///
/// Voir [`run_with_maintenance`] pour ajuster les intervalles (tests,
/// déploiements particuliers).
pub async fn run(unlocked: Unlocked, config: NodeConfig) -> Result<RunningNode, NodeError> {
    run_with_maintenance(unlocked, config, MaintenanceConfig::default()).await
}

/// Démarre un nœud complet à partir d'une identité déverrouillée.
///
/// Ouvre la base chiffrée, lie le socket UDP, câble le transport, la DHT, le
/// cœur applicatif et le serveur API local, puis lance les boucles (réseau et
/// maintenance périodique selon `maintenance`).
pub async fn run_with_maintenance(
    unlocked: Unlocked,
    config: NodeConfig,
    maintenance: MaintenanceConfig,
) -> Result<RunningNode, NodeError> {
    run_node(unlocked, config, maintenance, None).await
}

/// Démarre un nœud complet sur un socket datagramme FOURNI (mesh simulé des
/// tests d'intégration — ex. NAT symétrique simulé, voir
/// `accord_transport::socket::sim`). Ni liaison UDP réelle, ni écouteur TCP de
/// repli, ni mapping de port/mDNS : le nœud entier (DHT, maintenance, outbox,
/// relais) tourne sur le transport injecté.
///
/// **RÉSERVÉ AUX TESTS (M2).** `#[doc(hidden)]` : ne PAS câbler en production —
/// le socket injecté n'a subi aucun durcissement de liaison (pas de stratégie
/// de port stable, pas de repli TCP, pas de mapping/mDNS). La production passe
/// exclusivement par [`run`] / [`run_with_maintenance`].
#[doc(hidden)]
pub async fn run_with_socket(
    unlocked: Unlocked,
    config: NodeConfig,
    maintenance: MaintenanceConfig,
    socket: Arc<dyn DatagramSocket>,
) -> Result<RunningNode, NodeError> {
    run_node(unlocked, config, maintenance, Some(socket)).await
}

async fn run_node(
    unlocked: Unlocked,
    config: NodeConfig,
    maintenance: MaintenanceConfig,
    socket_override: Option<Arc<dyn DatagramSocket>>,
) -> Result<RunningNode, NodeError> {
    let db = Db::open(&config.paths.db(), &unlocked.db_key)?;

    // Migration vers le modèle compte/appareil (jalon 1, lot 1.B) : la graine
    // actuelle devient la racine du compte, et cette machine reçoit une clé
    // d'appareil DISTINCTE. Créée et persistée ici, pas encore utilisée — le
    // transport bascule dessus au lot 1.C. Voir `docs/MULTI_DEVICE.md`.
    let local_device = device::ensure_local_device(&db)?;

    let identity = Arc::new(unlocked.identity);
    // Identité présentée par le TRANSPORT — distincte de celle du compte dès
    // que le drapeau est levé (lot 1.C phase 2). Tout le reste du nœud
    // continue de signer et de s'identifier avec `identity` : le profil, les
    // amitiés, les op-logs appartiennent au compte, pas à la machine.
    let transport_identity: Arc<accord_crypto::Identity> = if config.device_key_transport {
        // La preuve de travail de l'appareil est déjà minée et persistée : la
        // recalculer à chaque démarrage serait du temps brûlé pour retrouver
        // exactement le même nonce.
        Arc::new(accord_crypto::Identity::from_seed_and_pow(
            *local_device.seed(),
            local_device.pow_nonce(),
            config.pow_bits,
        )?)
    } else {
        Arc::clone(&identity)
    };
    // Retenue avant que l'endpoint ne consomme l'`Arc` : c'est cette clé que
    // les correspondants verront, donc celle que la liste d'appareils annonce.
    let transport_pub = transport_identity.public_key();
    let injected = socket_override.is_some();

    // Transport chiffré : socket injecté (tests) ou UDP réel, multiplexé avec
    // les liens TCP de repli (SPEC §11.3) : l'endpoint voit un unique socket
    // datagramme ; un datagramme vers une adresse couverte par un lien TCP
    // poinçonné part encadré sur le flux, tout le reste part en UDP.
    let datagram: Arc<dyn DatagramSocket> = match socket_override {
        Some(socket) => socket,
        None => {
            // Port P2P stable (B2) : port explicite de la config s'il est
            // fourni, sinon le port retenu au précédent lancement, sinon la
            // stratégie par défaut (48016, plage de repli, puis port éphémère).
            let explicit_port = config.p2p_addr.port();
            let preferred_port = if explicit_port != 0 {
                Some(explicit_port)
            } else {
                node::network::read_stored_port(&db)?
            };
            Arc::new(bind_p2p(config.p2p_addr.ip(), preferred_port).await?)
        }
    };
    let p2p_port = datagram.local_addr().port();
    let (socket, tcp_links) = accord_transport::MuxSocket::new(datagram);
    let clock = Arc::new(accord_transport::SystemClock);
    let ep_config = EndpointConfig {
        pow_bits: config.pow_bits,
        // Service de relais activé (SPEC §10) : la logique d'acheminement reste
        // gardée (session avec la cible requise, plafonds 1 Mo/s + 64 circuits,
        // blobs opaques). L'usage réel est cadré par la SÉLECTION côté client,
        // qui ne retient qu'un relais effectivement joignable (voir
        // `Runtime::ensure_relay_to`) : un nœud non joignable, bien qu'annoncé
        // relais, est écarté à l'ouverture. Le raffinement « seuls les nœuds
        // publiquement joignables s'annoncent » (gating par `relay_eligible`)
        // est une optimisation ultérieure.
        relay_serving: true,
        ..EndpointConfig::default()
    };
    let (endpoint, events) =
        Endpoint::new(socket, Arc::clone(&transport_identity), clock, ep_config);
    endpoint.spawn();
    // Retient le port effectivement lié pour les prochains lancements (B2) —
    // sans objet pour un socket injecté (adresse du mesh simulé).
    if !injected {
        node::network::store_port(&db, endpoint.local_addr().port())?;
    }

    // Nœud DHT local.
    //
    // 🔒 Bâti sur l'identité de TRANSPORT, pas sur celle du compte : nos pairs
    // nous placent dans leur table de routage à partir de la clé qu'ils voient
    // dans le handshake (`NodeAnnounce`). Annoncer ici une autre identité nous
    // ferait calculer nos distances XOR, choisir nos relais domicile et placer
    // nos records depuis une position que personne d'autre ne nous attribue.
    let local_info = NodeInfo {
        node_id: transport_identity.node_id(),
        static_pub: transport_pub,
        pow_nonce: transport_identity.pow_nonce(),
        // Capacité relais annoncée (SPEC §10-§11.3) : permet à deux amis en NAT
        // symétrique de nous sélectionner comme relais partagé. La sélection
        // déterministe côté client écarte un relais injoignable, donc annoncer
        // la capacité largement ne nuit pas à la correction (au pire une
        // tentative écartée). Gating par joignabilité réelle = optimisation
        // future.
        flags: node_flags::RELAY,
        addrs: vec![WireAddr(endpoint.local_addr())],
    };
    let dht = Arc::new(KademliaNode::new(
        local_info,
        DhtConfig {
            pow_bits: config.pow_bits,
            ..DhtConfig::default()
        },
    ));

    // Hub d'événements partagé entre le nœud (émission) et l'API (diffusion).
    let hub = NotificationHub::new();
    let (sink, outbound_rx) = OutboundSink::channel(OUTBOUND_CAPACITY);
    let node = Arc::new(Node::with_hub(
        Arc::clone(&identity),
        db,
        sink.clone(),
        Some(hub.clone()),
    ));
    // Clé réellement présentée par le transport, puis réalignement de l'entrée
    // locale dans la liste d'appareils. C'est ce drapeau que les correspondants
    // lisent pour savoir s'ils doivent écrire au compte ou à l'appareil ; le
    // laisser mentir couperait la livraison le jour du basculement.
    node.set_transport_identity(Arc::clone(&transport_identity));
    if let Err(e) = node.reconcile_local_device_flags() {
        tracing::warn!(erreur = %e, "liste d'appareils : réalignement impossible");
    }
    // Migration douce : les invitations de serveur reçues avant cette
    // version (onglet Amis) sont re-présentées comme cartes dans le MP de
    // leur inviteur — idempotent, aucun état perdu.
    node.migrate_incoming_invites_to_dm();

    // Runtime réseau (construit avant l'API : il résout les codes amis).
    let rpc = TransportDhtRpc::new(Arc::clone(&endpoint));
    let runtime = Runtime::new(
        Arc::clone(&endpoint),
        Arc::clone(&dht),
        rpc,
        Arc::clone(&node),
        maintenance,
    );
    // Nœuds d'amorçage/relais par défaut (rendez-vous partagé du premier
    // contact) : fusionnés avec ceux de l'utilisateur pour le seeding, la
    // reconnexion et le repli de résolution de code ami.
    runtime.set_default_bootstrap(config.default_bootstrap.clone());

    // Sous-système voix (D-025) : tâche cadencée à 20 ms, trames via les
    // sessions du runtime, signalisation via le canal d'actions sortantes.
    let voice = voice::spawn(voice::VoiceDeps {
        node: Arc::clone(&node),
        outbound: sink,
        hub: Some(hub.clone()),
        sender: Arc::clone(&runtime) as Arc<dyn voice::FrameSender>,
        backend: config.voice_backend,
    });
    runtime.set_voice(voice.clone());

    // Contrôle réseau (B2) : branche les méthodes `network.*` sur le nœud.
    node.set_network_control(Arc::clone(&runtime) as Arc<dyn node::network::NetworkControl>);

    // API locale.
    let token = AuthToken::generate();
    let api = ApiServer::bind(
        config.api_port,
        token.clone(),
        Arc::new(
            NodeService::with_resolver(
                Arc::clone(&node),
                Arc::clone(&runtime) as Arc<dyn service::CodeResolver>,
            )
            .with_voice(voice.clone()),
        ),
        hub,
    )
    .await?;
    // Repli TCP (SPEC §11.3) : registre des liens câblé au runtime, écouteur
    // sur le MÊME port que l'UDP (partagé avec les `connect()` de poinçonnage
    // via SO_REUSEADDR/SO_REUSEPORT). Best-effort : un échec de liaison ne
    // désactive que les liens TCP entrants, le poinçonnage sortant demeure.
    runtime.set_tcp_links(Arc::clone(&tcp_links));
    match if injected {
        // Socket injecté (mesh simulé) : aucun écouteur TCP réel — le repli
        // TCP entrant est simplement désactivé, comme documenté ci-dessus.
        Err(std::io::Error::other(
            "transport injecté, repli TCP désactivé",
        ))
    } else {
        bind_tcp_listener(config.p2p_addr.ip(), p2p_port)
    } {
        Ok(listener) => {
            let links = Arc::clone(&tcp_links);
            let mut stop = runtime.stop_signal();
            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        res = listener.accept() => match res {
                            Ok((stream, _)) => {
                                // Plafond de liens atteint ou pair déjà parti :
                                // la connexion est simplement refusée.
                                if let Err(e) = links.adopt(stream) {
                                    tracing::debug!(erreur = %e, "tcp : connexion entrante refusée");
                                }
                            }
                            // Erreur d'accept transitoire (descripteurs épuisés…) :
                            // souffle avant de retenter, sans boucler à vide.
                            Err(_) => {
                                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                            }
                        },
                        res = stop.changed() => {
                            if res.is_err() || *stop.borrow() {
                                return;
                            }
                        }
                    }
                }
            });
        }
        Err(e) => tracing::debug!(erreur = %e, "tcp : écouteur de repli indisponible"),
    }

    runtime.spawn(events, outbound_rx);

    // Connexion facile : mapping de port automatique (UPnP-IGD puis NAT-PMP/PCP)
    // et découverte de pairs sur le réseau local (mDNS), en tâches de fond non
    // bloquantes. Ignorés en écoute loopback (rien à exposer ni à annoncer, cas
    // des tests) et désactivables par la configuration. Tout échec dégrade
    // proprement : le nœud reste utilisable via l'amorçage manuel.
    if !injected && !config.p2p_addr.ip().is_loopback() {
        let p2p_port = endpoint.local_addr().port();
        let local_ips = runtime::discover_local_ips();
        if config.nat_enabled {
            let notify: node::nat::OnChange = {
                let rt = Arc::clone(&runtime);
                Arc::new(move || rt.emit_network_if_changed())
            };
            let local_ipv4 = local_ips.iter().copied().find(IpAddr::is_ipv4);
            node::nat::spawn(
                runtime.nat_shared(),
                local_ipv4,
                p2p_port,
                runtime.stop_signal(),
                notify,
            );
        }
        if config.mdns_enabled {
            let notify: node::discovery::OnChange = {
                let rt = Arc::clone(&runtime);
                Arc::new(move || rt.emit_network_if_changed())
            };
            // 🔒 Annoncé sous la clé de TRANSPORT. Le nom d'instance et le nom
            // d'hôte mDNS en dérivent : deux machines d'un même compte sur le
            // même réseau annonceraient sinon exactement le même service —
            // collision de nom — et chacune écarterait l'annonce de l'autre en
            // la prenant pour la sienne. L'adresse apprise indexe de surcroît
            // le carnet, qui doit l'être par la clé effectivement présentée.
            node::discovery::spawn(
                runtime.lan_shared(),
                transport_pub,
                local_ips,
                p2p_port,
                Arc::clone(&runtime) as Arc<dyn node::discovery::LanSink>,
                runtime.stop_signal(),
                notify,
            );
        }
    }

    // Ensemencement immédiat des pairs d'amorçage persistés (la maintenance
    // assure ensuite la reconnexion périodique avec backoff).
    let boot_rt = Arc::clone(&runtime);
    tokio::spawn(async move { boot_rt.bootstrap_all().await });

    // Reconnexion rapide aux amis via leur dernière adresse connue (carnet
    // persistant) : rétablit les sessions sans attendre la résolution DHT.
    let recon_rt = Arc::clone(&runtime);
    tokio::spawn(async move { recon_rt.reconnect_known_friends().await });

    // Annonce de présence « en ligne » aux amis joignables (best-effort :
    // ceux hors ligne la perdent, un futur message les remettra en ligne).
    let _ = node.broadcast_presence(true);

    Ok(RunningNode {
        node,
        api,
        token,
        endpoint,
        runtime,
        voice,
    })
}

/// Ports candidats à l'écoute P2P, dans l'ordre d'essai : port préféré (non
/// nul) d'abord, puis la plage stable [`node::network::DEFAULT_P2P_PORT`]
/// `..=` [`node::network::P2P_PORT_FALLBACK_END`], puis `0` (éphémère) en
/// dernier recours. Sans doublon.
fn candidate_ports(preferred: Option<u16>) -> Vec<u16> {
    let mut ports: Vec<u16> = Vec::new();
    if let Some(p) = preferred.filter(|p| *p != 0) {
        ports.push(p);
    }
    for p in node::network::DEFAULT_P2P_PORT..=node::network::P2P_PORT_FALLBACK_END {
        if !ports.contains(&p) {
            ports.push(p);
        }
    }
    ports.push(0);
    ports
}

/// Lie l'écouteur TCP de repli sur `ip:port` avec réutilisation d'adresse et
/// de port : le poinçonnage TCP sortant ([`accord_transport::tcp::punch_connect`])
/// se lie au même port local, prérequis de l'ouverture simultanée (SPEC §11.3).
fn bind_tcp_listener(ip: IpAddr, port: u16) -> std::io::Result<tokio::net::TcpListener> {
    let addr = SocketAddr::new(ip, port);
    let socket = if addr.is_ipv4() {
        tokio::net::TcpSocket::new_v4()?
    } else {
        tokio::net::TcpSocket::new_v6()?
    };
    socket.set_reuseaddr(true)?;
    #[cfg(unix)]
    socket.set_reuseport(true)?;
    socket.bind(addr)?;
    socket.listen(64)
}

/// Nombre de tentatives sur le premier port candidat (préféré, ou par défaut)
/// avant de passer au port de repli suivant.
///
/// Absorbe la course bénigne où le port d'une précédente instance du même
/// profil vient tout juste d'être libéré par le système (fin de processus en
/// cours de réclamation par l'OS lors d'un relancement rapide) : la toute
/// première tentative peut échouer de quelques dizaines de millisecondes.
/// Sans cette retenue, un seul échec transitoire faisait immédiatement
/// dériver le port stable vers le suivant de la plage (48016 → 48017) au lieu
/// de converger vers une valeur unique — d'où la dérive observée d'un
/// lancement à l'autre.
///
/// Volontairement *sans* `SO_REUSEADDR`/`SO_REUSEPORT` ici : ces options
/// permettraient à deux instances **réellement distinctes et actives** de
/// partager silencieusement le même port UDP (le noyau se met alors à
/// répartir les datagrammes entre elles), au lieu de faire échouer
/// proprement la seconde et de la faire replier sur le port suivant. C'est
/// exactement le cas de deux nœuds de test sur `127.0.0.1` (voir
/// `tests/fichiers_e2e.rs`) ou de deux profils lancés volontairement en
/// parallèle sur la même machine : la réutilisation y romprait le transport
/// au lieu de le stabiliser.
const PREFERRED_PORT_BIND_ATTEMPTS: u32 = 5;
/// Délai entre deux tentatives sur le premier port candidat.
const PREFERRED_PORT_BIND_RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(30);

/// Essaie chaque port de `ports` dans l'ordre via `try_bind`, avec
/// `attempts_first` tentatives (séparées de `delay`) sur le premier port de
/// la liste avant de passer au suivant (un seul essai chacun, vrai repli).
///
/// Fonction pure paramétrée par `try_bind` pour rester testable sans socket
/// réel (voir les tests ci-dessous).
async fn bind_candidates<F, Fut, T>(
    ports: &[u16],
    attempts_first: u32,
    delay: std::time::Duration,
    mut try_bind: F,
) -> Result<T, std::io::Error>
where
    F: FnMut(u16) -> Fut,
    Fut: std::future::Future<Output = std::io::Result<T>>,
{
    let mut last_err: Option<std::io::Error> = None;
    for (idx, port) in ports.iter().enumerate() {
        let attempts = if idx == 0 { attempts_first.max(1) } else { 1 };
        for attempt in 0..attempts {
            match try_bind(*port).await {
                Ok(v) => return Ok(v),
                Err(e) => last_err = Some(e),
            }
            if attempt + 1 < attempts {
                tokio::time::sleep(delay).await;
            }
        }
    }
    Err(last_err.unwrap_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::AddrNotAvailable,
            "aucun port disponible",
        )
    }))
}

/// Lie le socket UDP P2P en essayant les ports candidats dans l'ordre (B2).
///
/// Le premier port (préféré — retenu au précédent lancement — ou
/// [`node::network::DEFAULT_P2P_PORT`] à défaut) est retenté plusieurs fois
/// avec un court délai avant tout repli (voir
/// [`PREFERRED_PORT_BIND_ATTEMPTS`]) : un échec transitoire (port tout juste
/// libéré par l'OS) ne le fait donc plus dériver immédiatement vers le
/// suivant de la plage. Un port réellement occupé de façon durable continue
/// d'échouer et déclenche normalement le repli sur la plage stable puis, en
/// dernier recours, un port éphémère (`0`), qui garantit qu'une adresse est
/// toujours obtenue (sauf échec d'E/S système, propagé tel quel).
async fn bind_p2p(ip: IpAddr, preferred: Option<u16>) -> Result<UdpDatagram, NodeError> {
    let ports = candidate_ports(preferred);
    bind_candidates(
        &ports,
        PREFERRED_PORT_BIND_ATTEMPTS,
        PREFERRED_PORT_BIND_RETRY_DELAY,
        move |port| UdpDatagram::bind(SocketAddr::new(ip, port)),
    )
    .await
    .map_err(NodeError::Io)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::network::{DEFAULT_P2P_PORT, P2P_PORT_FALLBACK_END};

    #[test]
    fn parse_default_bootstrap_filtre_et_borne() {
        // Adresses valides, séparateurs et espaces tolérés.
        let v = parse_default_bootstrap(" 203.0.113.7:48016, [2001:db8::1]:5000 ");
        assert_eq!(v.len(), 2);
        assert_eq!(v[0], "203.0.113.7:48016".parse().unwrap());
        // Entrées invalides et vides ignorées, valides conservées.
        let v = parse_default_bootstrap("pasunehote,,10.0.0.1:9000,x:y");
        assert_eq!(v, vec!["10.0.0.1:9000".parse().unwrap()]);
        // Chaîne vide ⇒ aucun nœud.
        assert!(parse_default_bootstrap("").is_empty());
        assert!(parse_default_bootstrap("   ").is_empty());
        // Bornée à MAX_BOOTSTRAP_PEERS.
        let many = (0..node::network::MAX_BOOTSTRAP_PEERS + 10)
            .map(|i| format!("10.0.0.1:{}", 1000 + i))
            .collect::<Vec<_>>()
            .join(",");
        assert_eq!(
            parse_default_bootstrap(&many).len(),
            node::network::MAX_BOOTSTRAP_PEERS
        );
    }

    #[test]
    fn ports_candidats_ordre_et_repli() {
        // Sans préférence : plage stable puis éphémère.
        let sans = candidate_ports(None);
        assert_eq!(sans.first(), Some(&DEFAULT_P2P_PORT));
        assert_eq!(sans.last(), Some(&0));
        assert_eq!(
            sans.len(),
            (P2P_PORT_FALLBACK_END - DEFAULT_P2P_PORT + 1) as usize + 1
        );

        // Préférence dans la plage : en tête, sans doublon.
        let pref = candidate_ports(Some(48020));
        assert_eq!(pref.first(), Some(&48020));
        assert_eq!(pref.iter().filter(|p| **p == 48020).count(), 1);
        assert_eq!(pref.last(), Some(&0));

        // Préférence hors plage : ajoutée en tête.
        let hors = candidate_ports(Some(50000));
        assert_eq!(hors.first(), Some(&50000));
        assert!(hors.contains(&DEFAULT_P2P_PORT));

        // Préférence nulle : ignorée (équivalent à None).
        assert_eq!(candidate_ports(Some(0)), sans);
    }

    /// Documente le cœur du correctif B2 : un échec transitoire sur le
    /// premier port (port stable) est retenté sur le MÊME port plusieurs
    /// fois avant tout repli — il ne dérive pas immédiatement vers le port
    /// suivant de la plage.
    #[tokio::test]
    async fn retente_le_port_prefere_avant_de_deriver() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Mutex;

        let ports = [48016u16, 48017, 48018];
        let tried: Mutex<Vec<u16>> = Mutex::new(Vec::new());
        let echecs_restants = AtomicUsize::new(2); // Réussit à la 3e tentative.
        let echecs_restants = &echecs_restants; // Référence `Copy` : capturable par l'`async move`.

        let resultat: Result<u16, std::io::Error> =
            bind_candidates(&ports, 3, std::time::Duration::from_millis(1), |port| {
                tried.lock().unwrap().push(port);
                async move {
                    if port == 48016
                        && echecs_restants
                            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
                            .is_ok()
                    {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::AddrInUse,
                            "port temporairement occupé",
                        ));
                    }
                    Ok(port)
                }
            })
            .await;

        assert_eq!(resultat.unwrap(), 48016, "converge sur le port stable");
        assert_eq!(
            *tried.lock().unwrap(),
            vec![48016, 48016, 48016],
            "3 tentatives sur le même port, aucun incrément vers 48017"
        );
    }

    /// Une fois les tentatives sur le port préféré épuisées, le repli
    /// avance bien vers le port candidat suivant (dernier recours).
    #[tokio::test]
    async fn replie_sur_le_port_suivant_apres_epuisement() {
        use std::sync::Mutex;

        let ports = [48016u16, 48017, 48018];
        let tried: Mutex<Vec<u16>> = Mutex::new(Vec::new());

        let resultat: Result<u16, std::io::Error> =
            bind_candidates(&ports, 2, std::time::Duration::from_millis(1), |port| {
                tried.lock().unwrap().push(port);
                async move {
                    if port == 48016 {
                        Err(std::io::Error::new(
                            std::io::ErrorKind::AddrInUse,
                            "toujours occupé",
                        ))
                    } else {
                        Ok(port)
                    }
                }
            })
            .await;

        assert_eq!(resultat.unwrap(), 48017);
        assert_eq!(
            *tried.lock().unwrap(),
            vec![48016, 48016, 48017],
            "2 tentatives sur le port préféré, puis repli unique sur le suivant"
        );
    }
}
