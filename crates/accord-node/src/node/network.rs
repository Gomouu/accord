//! Configuration réseau persistée et pont de contrôle (B2).
//!
//! Deux clés `meta` portent la configuration réseau du profil, réutilisée à
//! chaque lancement :
//! - `network.port` : port UDP P2P stable retenu (2 octets big-endian) ;
//! - `network.bootstrap` : liste JSON des pairs d'amorçage (`"ip:port"`).
//!
//! Le [`NetworkControl`] est implémenté par le runtime réseau
//! ([`crate::runtime`]) et branché sur le [`Node`] après construction ; il
//! permet aux méthodes `network.*` de l'API (ajout/retrait de pair, statut) de
//! piloter le réseau sans coupler le service au runtime.

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::Weak;

use serde::Serialize;
use serde_json::json;

use crate::error::NodeError;

use super::Node;

/// Clé `meta` du port P2P stable retenu entre deux lancements.
pub(crate) const META_PORT: &str = "network.port";
/// Clé `meta` de la liste JSON des pairs d'amorçage.
pub(crate) const META_BOOTSTRAP: &str = "network.bootstrap";

/// Port UDP P2P par défaut : port stable pour l'amorçage manuel entre amis.
pub const DEFAULT_P2P_PORT: u16 = 48016;
/// Dernier port de la plage de repli avant l'attribution aléatoire (inclus).
pub const P2P_PORT_FALLBACK_END: u16 = 48026;
/// Nombre maximal de pairs d'amorçage persistés (borne anti-abus).
pub const MAX_BOOTSTRAP_PEERS: usize = 64;

/// Statut réseau exposé par `network.status` (sérialisé tel quel côté API).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NetworkStatus {
    /// Port UDP P2P effectivement lié.
    pub p2p_port: u16,
    /// Adresses locales joignables (`ip:port`), sans loopback : l'adresse à
    /// communiquer à un ami. L'adresse publique observée par un pair figure en
    /// tête quand elle est connue.
    pub local_addrs: Vec<String>,
    /// Pairs d'amorçage configurés (`ip:port`).
    pub bootstrap: Vec<String>,
    /// Nombre de pairs dont une session a été apprise (carnet d'adresses).
    pub connected_peers: usize,
    /// Nombre de nœuds dans la table de routage DHT.
    pub dht_nodes: usize,
    /// Adresse externe (IP publique : port) joignable ouverte par le mapping de
    /// port automatique, ou `null` si aucun mapping n'est actif. Champ additif.
    pub external_addr: Option<String>,
    /// Méthode de mapping de port active : `"upnp"`, `"natpmp"` ou `"aucun"`.
    /// Champ additif.
    pub port_mapping: super::nat::PortMappingMethod,
    /// Nombre de pairs Accord découverts sur le réseau local (mDNS). Champ
    /// additif.
    pub lan_peers: usize,
    /// Nature du NAT local déduite par recoupement d'observations d'adresse
    /// (SPEC §11.1) : `"unknown"`, `"cone"` ou `"symmetric"`. Champ additif.
    pub nat_kind: super::relay::NatKind,
}

/// Nature du lien de session courant vers un ami (`network.peers`, D4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LinkTransport {
    /// Session directe (UDP ou lien TCP poinçonné).
    Direct,
    /// Session bout-en-bout tunnelée par un circuit relais (SPEC §11.3).
    Relay,
    /// Aucune session établie en ce moment.
    None,
}

/// Lien courant vers un ami, exposé par `network.peers` pour le diagnostic de
/// connectivité (sérialisé tel quel). Additif : le carnet d'adresses et le
/// suivi des sessions vivantes sont déjà maintenus par le runtime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PeerLink {
    /// Clé publique de l'ami (hex).
    pub pubkey: String,
    /// Vrai si une session (directe ou relayée) est actuellement active.
    pub live: bool,
    /// Dernière adresse directe connue (`ip:port`), ou `null` si jamais apprise.
    pub addr: Option<String>,
    /// Nature du lien de session courant. Champ additif.
    pub transport: LinkTransport,
    /// Adresse du relais qui héberge le tunnel quand `transport` vaut
    /// `"relay"`, `null` sinon. Champ additif.
    pub relay: Option<String>,
    /// Âge (ms) du dernier trafic ENTRANT reçu de ce pair sur la session
    /// courante, ou `null` sans session. Champ additif.
    pub last_recv_age_ms: Option<u64>,
    /// Dernier aller-retour keep-alive mesuré (ms), si un cycle a abouti.
    /// Champ additif.
    pub rtt_ms: Option<u64>,
    /// Horodatage (ms epoch) de la dernière remise RÉUSSIE d'un message à ce
    /// pair (tout canal confondu), ou `null` si aucune depuis le démarrage.
    /// Champ additif.
    pub last_delivery_ms: Option<u64>,
}

/// Contrôle du réseau depuis l'API : implémenté par le runtime, branché sur le
/// nœud après construction.
#[async_trait::async_trait]
pub trait NetworkControl: Send + Sync {
    /// Ajoute un pair d'amorçage : validation, persistance, connexion immédiate
    /// (handshake transport) et ensemencement DHT. Rend le statut à jour.
    async fn add_peer(&self, addr: SocketAddr) -> Result<NetworkStatus, NodeError>;
    /// Retire un pair d'amorçage persisté. Rend le statut à jour.
    async fn remove_peer(&self, addr: SocketAddr) -> Result<NetworkStatus, NodeError>;
    /// Photographie de l'état réseau courant.
    fn status(&self) -> NetworkStatus;
    /// Lien courant vers chaque ami (diagnostic de connectivité).
    fn peer_links(&self) -> Vec<PeerLink>;
    /// Photographie des compteurs réseau locaux (`diagnostics.counters`).
    fn counters(&self) -> super::diagnostics::CountersSnapshot;
    /// Auto-test réseau borné (`diagnostics.selftest`) : joignabilité, type de
    /// NAT, sondes des pairs d'amorçage et d'un relais candidat.
    async fn self_test(&self) -> super::diagnostics::SelfTestReport;
}

// ---- Encodage/décodage des valeurs `meta` (pur, testable) ----

/// Encode un port en valeur `meta` (2 octets big-endian).
pub(crate) fn encode_port(port: u16) -> Vec<u8> {
    port.to_be_bytes().to_vec()
}

/// Décode une valeur `meta` de port ; `None` si la longueur est inattendue.
pub(crate) fn decode_port(bytes: &[u8]) -> Option<u16> {
    let arr: [u8; 2] = bytes.try_into().ok()?;
    Some(u16::from_be_bytes(arr))
}

/// Encode une liste de pairs d'amorçage en JSON (tableau de chaînes).
pub(crate) fn encode_bootstrap(peers: &[SocketAddr]) -> Vec<u8> {
    let list: Vec<String> = peers.iter().map(SocketAddr::to_string).collect();
    // Sérialisation d'un simple `Vec<String>` : infaillible en pratique.
    serde_json::to_vec(&list).unwrap_or_else(|_| b"[]".to_vec())
}

/// Décode une liste de pairs d'amorçage ; les entrées illisibles sont ignorées
/// (lecture tolérante d'un état persisté potentiellement plus ancien).
pub(crate) fn decode_bootstrap(bytes: &[u8]) -> Vec<SocketAddr> {
    let list: Vec<String> = serde_json::from_slice(bytes).unwrap_or_default();
    list.iter()
        .filter_map(|s| s.parse::<SocketAddr>().ok())
        .collect()
}

/// Valide et normalise une adresse de pair fournie par l'API : `ip:port`
/// routable (IP non spécifiée et port nul refusés). Le loopback est toléré
/// pour l'amorçage local et les tests.
pub(crate) fn parse_peer_addr(raw: &str) -> Result<SocketAddr, NodeError> {
    let addr: SocketAddr = raw
        .trim()
        .parse()
        .map_err(|_| NodeError::Invalid("adresse de pair invalide (attendu ip:port)"))?;
    if addr.ip().is_unspecified() {
        return Err(NodeError::Invalid("adresse de pair non spécifiée"));
    }
    if addr.port() == 0 {
        return Err(NodeError::Invalid("port de pair nul"));
    }
    Ok(addr)
}

// ---- Accès `meta` sur une base ouverte (utilisés par l'assemblage) ----

/// Lit le port P2P stable retenu, s'il existe.
pub(crate) fn read_stored_port(db: &accord_core::db::Db) -> Result<Option<u16>, NodeError> {
    Ok(db.meta(META_PORT)?.as_deref().and_then(decode_port))
}

/// Persiste le port P2P retenu pour les prochains lancements.
pub(crate) fn store_port(db: &accord_core::db::Db, port: u16) -> Result<(), NodeError> {
    Ok(db.set_meta(META_PORT, &encode_port(port))?)
}

impl Node {
    /// Pairs d'amorçage persistés.
    pub fn bootstrap_peers(&self) -> Result<Vec<SocketAddr>, NodeError> {
        self.with_db(|db| {
            Ok(db
                .meta(META_BOOTSTRAP)?
                .map(|b| decode_bootstrap(&b))
                .unwrap_or_default())
        })
    }

    /// Ajoute un pair d'amorçage (idempotent, borné). Rend vrai s'il a été
    /// ajouté (faux s'il était déjà présent).
    pub fn add_bootstrap_peer(&self, addr: SocketAddr) -> Result<bool, NodeError> {
        self.with_db(|db| {
            let mut peers = db
                .meta(META_BOOTSTRAP)?
                .map(|b| decode_bootstrap(&b))
                .unwrap_or_default();
            if peers.contains(&addr) {
                return Ok(false);
            }
            if peers.len() >= MAX_BOOTSTRAP_PEERS {
                return Err(NodeError::Invalid("trop de pairs d'amorçage"));
            }
            peers.push(addr);
            db.set_meta(META_BOOTSTRAP, &encode_bootstrap(&peers))?;
            Ok(true)
        })
    }

    /// Retire un pair d'amorçage. Rend vrai s'il était présent.
    pub fn remove_bootstrap_peer(&self, addr: SocketAddr) -> Result<bool, NodeError> {
        self.with_db(|db| {
            let mut peers = db
                .meta(META_BOOTSTRAP)?
                .map(|b| decode_bootstrap(&b))
                .unwrap_or_default();
            let before = peers.len();
            peers.retain(|a| a != &addr);
            if peers.len() == before {
                return Ok(false);
            }
            db.set_meta(META_BOOTSTRAP, &encode_bootstrap(&peers))?;
            Ok(true)
        })
    }

    /// Branche le contrôle réseau (une seule fois, après construction du
    /// runtime).
    pub(crate) fn set_network_control(&self, ctrl: Arc<dyn NetworkControl>) {
        // Référence FAIBLE : évite le cycle Runtime↔Node (Lot G, cause 3). Le
        // runtime, ses boucles et le `RunningNode` gardent le strong-count vivant
        // tant que le nœud tourne ; l'upgrade échoue après l'arrêt.
        let _ = self.network.set(Arc::downgrade(&ctrl));
    }

    /// Contrôle réseau branché et encore vivant, s'il l'est (absent dans les
    /// tests sans réseau, ou après l'arrêt du runtime).
    pub(crate) fn network_control(&self) -> Option<Arc<dyn NetworkControl>> {
        self.network.get().and_then(Weak::upgrade)
    }

    /// Émet `event.network` vers l'UI avec le statut réseau complet (compteurs
    /// de pairs et de nœuds DHT, adresse externe, méthode de mapping, pairs
    /// LAN). La forme est celle de `network.status` (champs additifs inclus).
    pub fn emit_network_status(&self, status: &NetworkStatus) {
        // Sérialisation d'une structure simple : infaillible en pratique ;
        // repli défensif sur objet vide.
        let value = serde_json::to_value(status).unwrap_or_else(|_| json!({}));
        self.emit("event.network", value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outbound::OutboundSink;
    use accord_core::db::Db;
    use accord_crypto::Identity;

    fn node() -> Node {
        let id = Identity::generate_with_pow_bits(1);
        let db = Db::open_in_memory(&[3u8; 32]).unwrap();
        Node::new(id, db, OutboundSink::null())
    }

    #[test]
    fn port_roundtrip_et_longueur_stricte() {
        assert_eq!(decode_port(&encode_port(48016)), Some(48016));
        assert_eq!(decode_port(&encode_port(0)), Some(0));
        assert_eq!(decode_port(&[]), None);
        assert_eq!(decode_port(&[1, 2, 3]), None);
    }

    #[test]
    fn bootstrap_roundtrip_et_tolerance() {
        let peers: Vec<SocketAddr> = vec![
            "203.0.113.7:48016".parse().unwrap(),
            "[2001:db8::1]:9000".parse().unwrap(),
        ];
        assert_eq!(decode_bootstrap(&encode_bootstrap(&peers)), peers);
        // Octets illisibles : liste vide, pas de panique.
        assert!(decode_bootstrap(b"pas du json").is_empty());
        // Entrées invalides ignorées, valides conservées.
        let mixed = br#"["10.0.0.1:5", "pas-une-adresse", "10.0.0.2:6"]"#;
        assert_eq!(decode_bootstrap(mixed).len(), 2);
    }

    #[test]
    fn validation_adresse_de_pair() {
        assert!(parse_peer_addr(" 198.51.100.4:48016 ").is_ok());
        assert!(parse_peer_addr("127.0.0.1:48016").is_ok());
        assert!(parse_peer_addr("0.0.0.0:48016").is_err());
        assert!(parse_peer_addr("198.51.100.4:0").is_err());
        assert!(parse_peer_addr("198.51.100.4").is_err());
        assert!(parse_peer_addr("bonjour").is_err());
    }

    #[test]
    fn pairs_amorcage_persistes_et_idempotents() {
        let n = node();
        let a: SocketAddr = "203.0.113.7:48016".parse().unwrap();
        let b: SocketAddr = "203.0.113.8:48016".parse().unwrap();
        assert!(n.bootstrap_peers().unwrap().is_empty());
        assert!(n.add_bootstrap_peer(a).unwrap());
        assert!(!n.add_bootstrap_peer(a).unwrap(), "doublon ignoré");
        assert!(n.add_bootstrap_peer(b).unwrap());
        assert_eq!(n.bootstrap_peers().unwrap(), vec![a, b]);
        assert!(n.remove_bootstrap_peer(a).unwrap());
        assert!(!n.remove_bootstrap_peer(a).unwrap(), "déjà retiré");
        assert_eq!(n.bootstrap_peers().unwrap(), vec![b]);
    }

    #[test]
    fn borne_du_nombre_de_pairs() {
        let n = node();
        for i in 0..MAX_BOOTSTRAP_PEERS {
            let addr: SocketAddr = format!("203.0.113.7:{}", 40000 + i).parse().unwrap();
            assert!(n.add_bootstrap_peer(addr).unwrap());
        }
        let over: SocketAddr = "203.0.113.9:1".parse().unwrap();
        assert!(
            n.add_bootstrap_peer(over).is_err(),
            "borne dépassée refusée"
        );
    }

    #[test]
    fn statut_reseau_serialise_champs_additifs() {
        use super::super::nat::PortMappingMethod;
        use super::super::relay::NatKind;

        // Mapping actif : adresse externe présente, méthode "upnp".
        let status = NetworkStatus {
            p2p_port: 48016,
            local_addrs: vec!["203.0.113.7:48016".into()],
            bootstrap: vec![],
            connected_peers: 2,
            dht_nodes: 5,
            external_addr: Some("203.0.113.7:48016".into()),
            port_mapping: PortMappingMethod::Upnp,
            lan_peers: 1,
            nat_kind: NatKind::Cone,
        };
        let v = serde_json::to_value(&status).unwrap();
        // Champs historiques préservés (forme non cassée).
        assert_eq!(v["p2p_port"], 48016);
        assert_eq!(v["local_addrs"], serde_json::json!(["203.0.113.7:48016"]));
        assert_eq!(v["connected_peers"], 2);
        assert_eq!(v["dht_nodes"], 5);
        // Champs additifs.
        assert_eq!(v["external_addr"], "203.0.113.7:48016");
        assert_eq!(v["port_mapping"], "upnp");
        assert_eq!(v["lan_peers"], 1);
        assert_eq!(v["nat_kind"], "cone");

        // Aucun mapping : external_addr null, port_mapping "aucun" (repli
        // honnête exposé tel quel à l'UI).
        let none = NetworkStatus {
            external_addr: None,
            port_mapping: PortMappingMethod::Aucun,
            lan_peers: 0,
            nat_kind: NatKind::Unknown,
            ..status
        };
        let v = serde_json::to_value(&none).unwrap();
        assert!(v["external_addr"].is_null());
        assert_eq!(v["port_mapping"], "aucun");
        assert_eq!(v["lan_peers"], 0);
        assert_eq!(v["nat_kind"], "unknown");
    }
}
