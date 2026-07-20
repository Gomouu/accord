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
