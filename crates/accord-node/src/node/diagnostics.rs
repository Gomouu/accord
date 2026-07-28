//! Compteurs réseau locaux et auto-test de connectivité (Lot D — D3/D35/D36).
//!
//! Tout est LOCAL : aucun compteur ne quitte jamais la machine (pas de
//! télémétrie). Les compteurs sont des atomiques sans verrou, incrémentés par
//! le runtime et les boucles de maintenance aux points de décision réseau
//! (poinçonnage, relais, boîtes aux lettres, reconnexions), puis photographiés
//! par la méthode API `diagnostics.counters`. L'auto-test (`diagnostics.selftest`)
//! est assemblé par le runtime ([`crate::runtime`]) — ce module ne porte que
//! les types du contrat JSON et le verdict pur de joignabilité, testables sans
//! réseau.

use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};

use super::nat::PortMappingMethod;
use super::relay::NatKind;

/// Compteurs réseau locaux, cumulés depuis le démarrage du nœud. Atomiques
/// relâchés : chaque compteur est indépendant, seule la monotonie compte.
#[derive(Debug, Default)]
pub struct NetCounters {
    punch_requested: AtomicU64,
    punch_received: AtomicU64,
    punch_ok: AtomicU64,
    punch_fail: AtomicU64,
    relay_open_ok: AtomicU64,
    relay_open_fail: AtomicU64,
    mailbox_deposits: AtomicU64,
    mailbox_pickups: AtomicU64,
    outbox_enqueued: AtomicU64,
    outbox_flushed: AtomicU64,
    reconnect_attempts: AtomicU64,
    reconnect_ok: AtomicU64,
    handshake_hybrid: AtomicU64,
    handshake_classic: AtomicU64,
}

impl NetCounters {
    /// Demande de poinçonnage coordonné ÉMISE (SPEC §11.2).
    pub fn punch_requested(&self) {
        self.punch_requested.fetch_add(1, Ordering::Relaxed);
    }
    /// Demande de poinçonnage entrante ACCEPTÉE (ami, cadence respectée).
    pub fn punch_received(&self) {
        self.punch_received.fetch_add(1, Ordering::Relaxed);
    }
    /// Salve de poinçonnage terminée AVEC session directe.
    pub fn punch_ok(&self) {
        self.punch_ok.fetch_add(1, Ordering::Relaxed);
    }
    /// Salve de poinçonnage terminée SANS session directe (repli relais).
    pub fn punch_fail(&self) {
        self.punch_fail.fetch_add(1, Ordering::Relaxed);
    }
    /// Circuit relais ouvert et handshake tunnelé lancé.
    pub fn relay_open_ok(&self) {
        self.relay_open_ok.fetch_add(1, Ordering::Relaxed);
    }
    /// Repli relais épuisé sans circuit (tous candidats écartés).
    pub fn relay_open_fail(&self) {
        self.relay_open_fail.fetch_add(1, Ordering::Relaxed);
    }
    /// Dépôt en boîte aux lettres DHT réellement répliqué (≥ 1 réplica).
    pub fn mailbox_deposit(&self) {
        self.mailbox_deposits.fetch_add(1, Ordering::Relaxed);
    }
    /// Messages relevés d'une boîte aux lettres DHT et ingérés.
    pub fn mailbox_pickup(&self, messages: u64) {
        self.mailbox_pickups.fetch_add(messages, Ordering::Relaxed);
    }
    /// Message mis en file hors-ligne (destinataire injoignable).
    pub fn outbox_enqueued(&self) {
        self.outbox_enqueued.fetch_add(1, Ordering::Relaxed);
    }
    /// Messages d'outbox partis sur un lien (vidage périodique ou connexion).
    pub fn outbox_flushed(&self, messages: u64) {
        self.outbox_flushed.fetch_add(messages, Ordering::Relaxed);
    }
    /// Tentative de reconnexion à un pair d'amorçage (échéance de backoff).
    pub fn reconnect_attempt(&self) {
        self.reconnect_attempts.fetch_add(1, Ordering::Relaxed);
    }
    /// Reconnexion d'amorçage aboutie (session apprise).
    pub fn reconnect_ok(&self) {
        self.reconnect_ok.fetch_add(1, Ordering::Relaxed);
    }
    /// Session établie, classée selon la provenance de sa clé (jalon 2, lot 2.D).
    ///
    /// 🔒 Purement local, comme tous les compteurs de ce module : rien de ceci
    /// ne part sur le réseau, sous aucune forme et vers aucun pair. C'est ce
    /// qui permet de répondre « quelle part de mes sessions est hybride ? »
    /// sans construire de télémétrie.
    pub fn handshake_done(&self, post_quantum: bool) {
        let compteur = if post_quantum {
            &self.handshake_hybrid
        } else {
            &self.handshake_classic
        };
        compteur.fetch_add(1, Ordering::Relaxed);
    }

    /// Photographie sérialisable des compteurs (contrat `diagnostics.counters`).
    pub fn snapshot(&self) -> CountersSnapshot {
        CountersSnapshot {
            punch: PunchCounters {
                requested: self.punch_requested.load(Ordering::Relaxed),
                received: self.punch_received.load(Ordering::Relaxed),
                ok: self.punch_ok.load(Ordering::Relaxed),
                fail: self.punch_fail.load(Ordering::Relaxed),
            },
            relay: RelayCounters {
                open_ok: self.relay_open_ok.load(Ordering::Relaxed),
                open_fail: self.relay_open_fail.load(Ordering::Relaxed),
            },
            mailbox: MailboxCounters {
                deposits: self.mailbox_deposits.load(Ordering::Relaxed),
                pickups: self.mailbox_pickups.load(Ordering::Relaxed),
            },
            outbox: OutboxCounters {
                enqueued: self.outbox_enqueued.load(Ordering::Relaxed),
                flushed: self.outbox_flushed.load(Ordering::Relaxed),
            },
            reconnect: ReconnectCounters {
                attempts: self.reconnect_attempts.load(Ordering::Relaxed),
                ok: self.reconnect_ok.load(Ordering::Relaxed),
            },
            handshake: HandshakeCounters {
                hybrid: self.handshake_hybrid.load(Ordering::Relaxed),
                classic: self.handshake_classic.load(Ordering::Relaxed),
            },
        }
    }
}

/// Compteurs de poinçonnage coordonné (SPEC §11.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PunchCounters {
    /// Demandes émises.
    pub requested: u64,
    /// Demandes entrantes acceptées.
    pub received: u64,
    /// Salves ayant abouti à une session directe.
    pub ok: u64,
    /// Salves terminées sans session directe.
    pub fail: u64,
}

/// Compteurs du repli relais (SPEC §11.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct RelayCounters {
    /// Circuits ouverts (handshake tunnelé lancé).
    pub open_ok: u64,
    /// Replis épuisés sans circuit.
    pub open_fail: u64,
}

/// Compteurs des boîtes aux lettres DHT (remise hors-ligne, D-017).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct MailboxCounters {
    /// Dépôts répliqués (≥ 1 réplica DHT).
    pub deposits: u64,
    /// Messages relevés et ingérés.
    pub pickups: u64,
}

/// Compteurs de la file hors-ligne persistante.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct OutboxCounters {
    /// Messages mis en file (destinataire injoignable).
    pub enqueued: u64,
    /// Messages partis sur un lien (vidage).
    pub flushed: u64,
}

/// Compteurs de reconnexion aux pairs d'amorçage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ReconnectCounters {
    /// Tentatives (échéances de backoff).
    pub attempts: u64,
    /// Reconnexions abouties.
    pub ok: u64,
}

/// Contrat JSON de `diagnostics.counters` : groupes de compteurs cumulés
/// depuis le démarrage. Champs additifs uniquement à l'avenir.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CountersSnapshot {
    /// Poinçonnage coordonné.
    pub punch: PunchCounters,
    /// Repli relais.
    pub relay: RelayCounters,
    /// Boîtes aux lettres DHT.
    pub mailbox: MailboxCounters,
    /// File hors-ligne.
    pub outbox: OutboxCounters,
    /// Reconnexion d'amorçage.
    pub reconnect: ReconnectCounters,
    /// Provenance des clés de session établies. Champ additif.
    pub handshake: HandshakeCounters,
}

/// Sessions établies depuis le démarrage, réparties selon la provenance de leur
/// clé (jalon 2, lot 2.D). Le rapport des deux est la proportion de sessions
/// hybrides ; il n'est calculé qu'à l'affichage, pour éviter de figer une
/// division par zéro au démarrage.
///
/// 🔒 Jamais transmis. Ces deux nombres ne sortent que par l'API LOCALE.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct HandshakeCounters {
    /// Sessions dont la clé dérive de X25519 **et** de ML-KEM.
    pub hybrid: u64,
    /// Sessions dont la clé dérive de X25519 seul.
    pub classic: u64,
}

/// Verdict de joignabilité de l'auto-test réseau, dérivé de l'éligibilité
/// relais (mapping actif ou consensus au port local) et de la nature du NAT.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Reachability {
    /// Joignable directement (mapping actif ou port public confirmé).
    Direct,
    /// NAT cone : le poinçonnage direct est viable.
    Punch,
    /// NAT symétrique : un relais est requis.
    Relay,
    /// Trop peu d'observations pour conclure.
    Unknown,
}

/// Déduit le verdict de joignabilité — fonction pure, testable.
pub fn reachability(relay_eligible: bool, nat: NatKind) -> Reachability {
    if relay_eligible {
        return Reachability::Direct;
    }
    match nat {
        NatKind::Cone => Reachability::Punch,
        NatKind::Symmetric => Reachability::Relay,
        NatKind::Unknown => Reachability::Unknown,
    }
}

/// Résultat d'une sonde de connectivité (pair d'amorçage ou relais).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProbeResult {
    /// Adresse sondée (`ip:port`).
    pub addr: String,
    /// Vrai si une session a été établie dans le délai imparti.
    pub ok: bool,
}

/// Contrat JSON de `diagnostics.selftest` : auto-test réseau déclenchable,
/// borné (sondes courtes), données backend uniquement — l'UI le met en forme.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SelfTestReport {
    /// Port UDP P2P lié.
    pub p2p_port: u16,
    /// Nature du NAT local (SPEC §11.1).
    pub nat_kind: NatKind,
    /// Méthode de mapping de port active.
    pub port_mapping: PortMappingMethod,
    /// Adresse externe du mapping, si actif.
    pub external_addr: Option<String>,
    /// Consensus d'adresse publique observée (≥ 2 pairs), si établi.
    pub observed_consensus: Option<String>,
    /// Nœuds dans la table de routage DHT.
    pub dht_nodes: usize,
    /// Pairs dont une session a été apprise.
    pub connected_peers: usize,
    /// Vrai si ce nœud remplit les critères d'annonce relais (joignable).
    pub relay_eligible: bool,
    /// Sondes des pairs d'amorçage effectifs (bornées).
    pub bootstrap: Vec<ProbeResult>,
    /// Sonde d'un relais candidat (le plus proche), s'il en existe un.
    pub relay_probe: Option<ProbeResult>,
    /// Verdict de joignabilité.
    pub reachability: Reachability,
}

/// Un lien vers un ami, **débarrassé de tout ce qui désigne cet ami**.
///
/// Voir [`bug_report`] pour la règle et pourquoi elle existe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RedactedLink {
    /// Numéro d'ordre dans la liste, seule façon de distinguer deux liens.
    /// Il ne vaut que pour ce rapport : rien ne le rattache à un ami.
    pub peer: usize,
    /// Session active.
    pub live: bool,
    /// Nature du lien (direct, relayé, aucun).
    pub transport: super::network::LinkTransport,
    /// Relais qui héberge le tunnel, s'il y en a un. **Conservé** : c'est
    /// l'adresse d'un nœud relais, pas celle de l'ami.
    pub relay: Option<String>,
    /// Âge du dernier trafic entrant (ms).
    pub last_recv_age_ms: Option<u64>,
    /// Dernier aller-retour keep-alive mesuré (ms).
    pub rtt_ms: Option<u64>,
    /// Capacités annoncées par le pair (bitmask).
    pub capabilities: u32,
}

/// Auto-test réseau dont l'adresse publique locale est masquée.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RedactedSelfTest {
    /// Port UDP P2P lié.
    pub p2p_port: u16,
    /// Nature du NAT local.
    pub nat_kind: NatKind,
    /// Méthode de mapping de port active.
    pub port_mapping: PortMappingMethod,
    /// Adresse externe du mapping, hôte masqué, port conservé.
    pub external_addr: Option<String>,
    /// Consensus d'adresse publique, hôte masqué, port conservé.
    pub observed_consensus: Option<String>,
    /// Nœuds dans la table de routage DHT.
    pub dht_nodes: usize,
    /// Pairs dont une session a été apprise.
    pub connected_peers: usize,
    /// Éligibilité relais.
    pub relay_eligible: bool,
    /// Sondes des pairs d'amorçage. **Conservées telles quelles** : ce sont
    /// des adresses d'infrastructure, que l'utilisateur a lui-même saisies.
    pub bootstrap: Vec<ProbeResult>,
    /// Sonde du relais candidat, même raison.
    pub relay_probe: Option<ProbeResult>,
    /// Verdict de joignabilité.
    pub reachability: Reachability,
}

/// Rapport de diagnostic destiné à être **joint à un rapport de bug**.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BugReport {
    /// Version de l'application qui a produit le rapport.
    pub version: &'static str,
    /// Système et architecture (`macos/aarch64`).
    pub platform: String,
    /// Compteurs réseau depuis le démarrage.
    pub counters: CountersSnapshot,
    /// Auto-test, caviardé.
    pub selftest: RedactedSelfTest,
    /// Liens vers les amis, caviardés.
    pub links: Vec<RedactedLink>,
}

/// Masque l'hôte d'une adresse `hôte:port` en gardant le port.
///
/// Le port est ce qui sert au diagnostic NAT — savoir que le mapping externe
/// tombe sur un port différent du port local est précisément l'information
/// utile. L'hôte, lui, est l'adresse IP publique de la machine : dans un
/// fichier que l'utilisateur envoie à un inconnu, elle n'a rien à faire.
fn masquer_hote(addr: &str) -> String {
    // IPv6 littéral : `[::1]:443`. Le dernier `:` sépare toujours le port.
    match addr.rfind(':') {
        Some(i) => format!("masqué:{}", &addr[i + 1..]),
        None => "masqué".to_string(),
    }
}

/// Assemble le rapport de bug à partir de l'état brut — **fonction pure**,
/// donc éprouvable sans monter le moindre nœud.
///
/// 🔒 **Ce qui ne doit jamais y entrer, et pourquoi.**
///
/// Ce rapport a une seule raison d'exister : être envoyé à quelqu'un d'autre.
/// C'est le seul endroit de l'application où des données locales partent
/// délibérément — tout le reste du produit est construit pour que rien ne
/// sorte. Il doit donc être sûr à partager par construction, pas par la
/// prudence de celui qui l'envoie.
///
/// Deux champs de [`super::network::PeerLink`] sont écartés sans discussion :
///
/// - `pubkey` est la clé publique de l'ami, c'est-à-dire **son code ami**.
///   Un rapport qui la porte livre le carnet d'adresses de l'utilisateur à qui
///   le reçoit, et permet de recouper deux rapports pour établir que deux
///   personnes se connaissent.
/// - `addr` est l'**adresse IP de l'ami**. Ce n'est pas la donnée de
///   l'utilisateur : c'est celle d'un tiers, qui n'a rien demandé et à qui on
///   ne peut pas poser la question.
///
/// Ce qui reste est conservé délibérément : les adresses d'amorçage et de
/// relais sont de l'infrastructure publique, saisie par l'utilisateur
/// lui-même, et sans elles un problème de relais n'est pas diagnosticable.
/// L'adresse publique de la machine locale, elle, est masquée jusqu'au port.
///
/// ⚠️ [`CountersSnapshot`] traverse **en bloc**, sans liste blanche — à la
/// différence des liens, filtrés champ par champ. Ajouter un compteur, c'est
/// donc l'ajouter à ce rapport, en silence. Ce n'est tenable que parce que tout
/// compteur est un agrégat local sans rattachement à un pair ; le jour où l'un
/// d'eux porterait un identifiant, un horodatage exploitable ou une valeur par
/// ami, il faudrait filtrer ici. Le contrôle de forme de
/// `diagnostics_report_ne_sort_ni_cle_ni_adresse_d_ami` fige la liste des
/// groupes pour que l'ajout suivant se voie.
pub fn bug_report(
    counters: CountersSnapshot,
    selftest: SelfTestReport,
    links: &[super::network::PeerLink],
) -> BugReport {
    BugReport {
        version: env!("CARGO_PKG_VERSION"),
        platform: format!("{}/{}", std::env::consts::OS, std::env::consts::ARCH),
        counters,
        selftest: RedactedSelfTest {
            p2p_port: selftest.p2p_port,
            nat_kind: selftest.nat_kind,
            port_mapping: selftest.port_mapping,
            external_addr: selftest.external_addr.as_deref().map(masquer_hote),
            observed_consensus: selftest.observed_consensus.as_deref().map(masquer_hote),
            dht_nodes: selftest.dht_nodes,
            connected_peers: selftest.connected_peers,
            relay_eligible: selftest.relay_eligible,
            bootstrap: selftest.bootstrap,
            relay_probe: selftest.relay_probe,
            reachability: selftest.reachability,
        },
        links: links
            .iter()
            .enumerate()
            .map(|(i, l)| RedactedLink {
                peer: i + 1,
                live: l.live,
                transport: l.transport,
                relay: l.relay.clone(),
                last_recv_age_ms: l.last_recv_age_ms,
                rtt_ms: l.rtt_ms,
                capabilities: l.capabilities,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests_rapport {
    use super::*;
    use crate::node::network::{LinkTransport, PeerLink};

    /// Clé publique d'un ami, en hexadécimal, telle que `PeerLink` la porte.
    const CLE_AMI: &str = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";
    /// Adresse directe d'un ami : la donnée d'un tiers.
    const IP_AMI: &str = "198.51.100.42:51820";
    /// Adresse publique de la machine locale.
    const IP_LOCALE: &str = "203.0.113.7:41234";

    fn lien() -> PeerLink {
        PeerLink {
            pubkey: CLE_AMI.to_string(),
            live: true,
            addr: Some(IP_AMI.to_string()),
            transport: LinkTransport::Relay,
            relay: Some("192.0.2.10:4242".to_string()),
            last_recv_age_ms: Some(1_200),
            rtt_ms: Some(38),
            last_delivery_ms: Some(1_700_000_000_000),
            capabilities: 0b101,
            post_quantum: true,
        }
    }

    fn autotest() -> SelfTestReport {
        SelfTestReport {
            p2p_port: 40_000,
            nat_kind: NatKind::Cone,
            port_mapping: PortMappingMethod::Upnp,
            external_addr: Some(IP_LOCALE.to_string()),
            observed_consensus: Some(IP_LOCALE.to_string()),
            dht_nodes: 12,
            connected_peers: 3,
            relay_eligible: false,
            bootstrap: vec![ProbeResult {
                addr: "45.77.223.0:48016".to_string(),
                ok: true,
            }],
            relay_probe: Some(ProbeResult {
                addr: "192.0.2.10:4242".to_string(),
                ok: true,
            }),
            reachability: Reachability::Punch,
        }
    }

    #[test]
    fn le_rapport_ne_porte_ni_cle_ni_adresse_d_ami() {
        // 🔒 Le test qui justifie la fonction. Il ne lit pas les champs un par
        // un — il cherche les valeurs interdites dans le JSON complet, donc un
        // champ ajouté plus tard à `PeerLink` et recopié par inadvertance le
        // fait tomber, sans que personne ait à y penser.
        let rapport = bug_report(NetCounters::default().snapshot(), autotest(), &[lien()]);
        let json = serde_json::to_string(&rapport).expect("rapport sérialisable");

        assert!(
            !json.contains(CLE_AMI),
            "la clé publique de l'ami — son code ami — est dans le rapport"
        );
        assert!(
            !json.contains("198.51.100.42"),
            "l'adresse IP de l'ami est dans le rapport"
        );
        assert!(
            !json.contains("203.0.113.7"),
            "l'adresse publique de la machine est dans le rapport"
        );
    }

    #[test]
    fn le_rapport_garde_de_quoi_diagnostiquer() {
        // L'autre moitié : un rapport vide serait parfaitement privé et
        // parfaitement inutile.
        let rapport = bug_report(NetCounters::default().snapshot(), autotest(), &[lien()]);

        assert_eq!(rapport.links.len(), 1);
        let l = &rapport.links[0];
        assert_eq!(l.peer, 1, "les liens restent distinguables entre eux");
        assert!(l.live);
        assert_eq!(l.transport, LinkTransport::Relay);
        assert_eq!(l.rtt_ms, Some(38));
        assert_eq!(l.last_recv_age_ms, Some(1_200));
        assert_eq!(
            l.relay.as_deref(),
            Some("192.0.2.10:4242"),
            "l'adresse du relais est de l'infrastructure, elle reste"
        );

        // Le port du mapping externe est ce qui sert au diagnostic NAT.
        assert_eq!(
            rapport.selftest.external_addr.as_deref(),
            Some("masqué:41234")
        );
        assert_eq!(rapport.selftest.p2p_port, 40_000);
        assert_eq!(
            rapport.selftest.bootstrap[0].addr, "45.77.223.0:48016",
            "les pairs d'amorçage sont ceux que l'utilisateur a saisis"
        );
    }

    #[test]
    fn deux_amis_restent_deux_lignes_distinctes() {
        // Caviarder ne doit pas fusionner : sans numéro d'ordre, deux liens
        // au même état deviendraient indiscernables et le rapport mentirait
        // sur le nombre d'amis connectés.
        let rapport = bug_report(
            NetCounters::default().snapshot(),
            autotest(),
            &[lien(), lien()],
        );
        assert_eq!(rapport.links.len(), 2);
        assert_eq!(rapport.links[0].peer, 1);
        assert_eq!(rapport.links[1].peer, 2);
    }

    #[test]
    fn masquer_garde_le_port_meme_en_ipv6() {
        assert_eq!(masquer_hote("203.0.113.7:41234"), "masqué:41234");
        // IPv6 littéral : le dernier `:` est celui du port, pas ceux de
        // l'adresse. Une découpe sur le PREMIER `:` rendrait « masqué:: »
        // et laisserait l'adresse entière dans le rapport.
        assert_eq!(masquer_hote("[2001:db8::1]:443"), "masqué:443");
        // Rien qui ressemble à un port : on ne garde rien plutôt que de
        // laisser passer la chaîne telle quelle.
        assert_eq!(masquer_hote("hote-sans-port"), "masqué");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compteurs_cumulent_et_se_photographient() {
        let c = NetCounters::default();
        c.punch_requested();
        c.punch_requested();
        c.punch_received();
        c.punch_ok();
        c.punch_fail();
        c.relay_open_ok();
        c.relay_open_fail();
        c.mailbox_deposit();
        c.mailbox_pickup(3);
        c.outbox_enqueued();
        c.outbox_flushed(5);
        c.reconnect_attempt();
        c.reconnect_ok();

        let s = c.snapshot();
        assert_eq!(s.punch.requested, 2);
        assert_eq!(s.punch.received, 1);
        assert_eq!(s.punch.ok, 1);
        assert_eq!(s.punch.fail, 1);
        assert_eq!(s.relay.open_ok, 1);
        assert_eq!(s.relay.open_fail, 1);
        assert_eq!(s.mailbox.deposits, 1);
        assert_eq!(s.mailbox.pickups, 3);
        assert_eq!(s.outbox.enqueued, 1);
        assert_eq!(s.outbox.flushed, 5);
        assert_eq!(s.reconnect.attempts, 1);
        assert_eq!(s.reconnect.ok, 1);
    }

    #[test]
    fn contrat_json_des_compteurs_stable() {
        let c = NetCounters::default();
        c.punch_requested();
        let v = serde_json::to_value(c.snapshot()).unwrap();
        assert_eq!(v["punch"]["requested"], 1);
        assert_eq!(v["punch"]["ok"], 0);
        assert_eq!(v["relay"]["open_ok"], 0);
        assert_eq!(v["mailbox"]["deposits"], 0);
        assert_eq!(v["outbox"]["flushed"], 0);
        assert_eq!(v["reconnect"]["attempts"], 0);
    }

    #[test]
    fn verdict_de_joignabilite() {
        assert_eq!(reachability(true, NatKind::Unknown), Reachability::Direct);
        assert_eq!(reachability(true, NatKind::Symmetric), Reachability::Direct);
        assert_eq!(reachability(false, NatKind::Cone), Reachability::Punch);
        assert_eq!(reachability(false, NatKind::Symmetric), Reachability::Relay);
        assert_eq!(reachability(false, NatKind::Unknown), Reachability::Unknown);
    }

    #[test]
    fn contrat_json_du_rapport_selftest() {
        let report = SelfTestReport {
            p2p_port: 48016,
            nat_kind: NatKind::Cone,
            port_mapping: PortMappingMethod::Aucun,
            external_addr: None,
            observed_consensus: Some("203.0.113.7:48016".into()),
            dht_nodes: 4,
            connected_peers: 2,
            relay_eligible: true,
            bootstrap: vec![ProbeResult {
                addr: "203.0.113.9:48016".into(),
                ok: true,
            }],
            relay_probe: None,
            reachability: Reachability::Direct,
        };
        let v = serde_json::to_value(&report).unwrap();
        assert_eq!(v["p2p_port"], 48016);
        assert_eq!(v["nat_kind"], "cone");
        assert_eq!(v["reachability"], "direct");
        assert_eq!(v["bootstrap"][0]["ok"], true);
        assert!(v["relay_probe"].is_null());
        assert!(v["external_addr"].is_null());
    }
}
