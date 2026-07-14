//! Aide au NAT traversal (SPEC §11) : agrégation de candidats d'adresses et
//! détection de NAT symétrique par recoupement d'observations.

use std::collections::HashMap;
use std::net::SocketAddr;

/// Classe d'un candidat d'adresse, par ordre de préférence d'essai.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CandidateKind {
    /// Adresse locale directe (LAN).
    LocalDirect = 0,
    /// Adresse publique directe (port mappé stable).
    PublicDirect = 1,
    /// Candidat de hole punching.
    HolePunch = 2,
    /// Repli par relais.
    Relay = 3,
}

/// Candidat d'adresse pour l'établissement de session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Candidate {
    /// Adresse à essayer.
    pub addr: SocketAddr,
    /// Classe du candidat (ordre d'essai).
    pub kind: CandidateKind,
}

/// Agrège les observations d'adresse publique depuis plusieurs pairs pour
/// détecter un NAT symétrique (SPEC §11 : réponses croisées de 3 nœuds).
///
/// Les votes sont indexés par IDENTITÉ d'observateur (clé publique), pas par
/// message : un pair unique ne compte que pour UN vote, quelle que soit la
/// quantité d'`ObserveAddrResp` qu'il envoie, et son vote le plus récent
/// remplace le précédent. Sans cette déduplication, un seul pair connecté
/// pourrait fabriquer un « consensus » (≥ 2 votes concordants) en envoyant
/// deux réponses, et faire basculer l'éligibilité relais d'une victime NATée
/// (M1b).
#[derive(Default)]
pub struct ObservedAddrs {
    /// clé publique de l'observateur → dernière adresse qu'il a rapportée.
    by_observer: HashMap<[u8; 32], SocketAddr>,
}

impl ObservedAddrs {
    /// Crée un agrégateur vide.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enregistre l'adresse que `observer` rapporte avoir vue pour nous. Un
    /// même observateur ne détient qu'un seul vote (le plus récent).
    pub fn observe(&mut self, observer: [u8; 32], addr: SocketAddr) {
        self.by_observer.insert(observer, addr);
    }

    /// Compte des votes par adresse (un par observateur distinct).
    fn tally(&self) -> HashMap<SocketAddr, u32> {
        let mut counts: HashMap<SocketAddr, u32> = HashMap::new();
        for addr in self.by_observer.values() {
            *counts.entry(*addr).or_insert(0) += 1;
        }
        counts
    }

    /// Adresse publique consensuelle si ≥ 2 pairs DISTINCTS concordent.
    pub fn consensus(&self) -> Option<SocketAddr> {
        self.tally()
            .into_iter()
            .filter(|(_, v)| *v >= 2)
            .max_by_key(|(_, v)| *v)
            .map(|(addr, _)| addr)
    }

    /// Vrai si les observations de pairs DISTINCTS divergent : NAT symétrique
    /// probable (adresses/ports différents selon le pair interrogé). Exige au
    /// moins deux adresses distinctes rapportées et aucun consensus.
    pub fn is_symmetric(&self) -> bool {
        let counts = self.tally();
        counts.len() >= 2 && !counts.values().any(|&v| v >= 2)
    }

    /// Nombre d'adresses distinctes rapportées (tous observateurs confondus).
    pub fn distinct(&self) -> usize {
        self.tally().len()
    }
}

/// Construit la liste ordonnée des candidats à essayer (SPEC §11 étape 3).
pub fn ordered_candidates(
    local: &[SocketAddr],
    public: Option<SocketAddr>,
    relays: &[SocketAddr],
    symmetric: bool,
) -> Vec<Candidate> {
    let mut out = Vec::new();
    for a in local {
        out.push(Candidate {
            addr: *a,
            kind: CandidateKind::LocalDirect,
        });
    }
    if let Some(p) = public {
        // Sous NAT symétrique, l'adresse publique n'est pas réutilisable en
        // direct : on la classe comme hole punch plutôt que direct.
        out.push(Candidate {
            addr: p,
            kind: if symmetric {
                CandidateKind::HolePunch
            } else {
                CandidateKind::PublicDirect
            },
        });
    }
    for r in relays {
        out.push(Candidate {
            addr: *r,
            kind: CandidateKind::Relay,
        });
    }
    out.sort_by_key(|c| c.kind);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(s: &str) -> SocketAddr {
        s.parse().unwrap()
    }

    fn peer(n: u8) -> [u8; 32] {
        [n; 32]
    }

    #[test]
    fn consensus_needs_two_distinct_peers() {
        let mut o = ObservedAddrs::new();
        o.observe(peer(1), addr("1.2.3.4:5"));
        assert_eq!(o.consensus(), None);
        // Deux pairs DISTINCTS concordent : consensus.
        o.observe(peer(2), addr("1.2.3.4:5"));
        assert_eq!(o.consensus(), Some(addr("1.2.3.4:5")));
    }

    #[test]
    fn un_seul_pair_ne_fabrique_pas_de_consensus() {
        // M1b : un pair unique qui vote deux fois (ou change de vote) ne peut
        // pas fabriquer un consensus — son vote le plus récent remplace le
        // précédent, il ne compte jamais que pour un.
        let mut o = ObservedAddrs::new();
        o.observe(peer(1), addr("9.9.9.9:5"));
        o.observe(peer(1), addr("9.9.9.9:5"));
        assert_eq!(o.consensus(), None, "un pair = un seul vote");
        assert_eq!(o.distinct(), 1);
        // Même en changeant d'adresse, un pair seul ne franchit pas le seuil.
        o.observe(peer(1), addr("8.8.8.8:5"));
        assert_eq!(o.consensus(), None);
        assert_eq!(o.distinct(), 1, "dernier vote remplace le précédent");
        // Il faut un DEUXIÈME pair concordant pour un consensus.
        o.observe(peer(2), addr("8.8.8.8:5"));
        assert_eq!(o.consensus(), Some(addr("8.8.8.8:5")));
    }

    #[test]
    fn divergent_observations_flag_symmetric() {
        let mut o = ObservedAddrs::new();
        o.observe(peer(1), addr("1.2.3.4:5"));
        o.observe(peer(2), addr("1.2.3.4:6"));
        assert!(o.is_symmetric());
        assert_eq!(o.consensus(), None);
    }

    #[test]
    fn un_seul_pair_divergent_ne_flag_pas_symmetric() {
        // Un pair unique qui rapporte deux adresses successives ne suffit pas à
        // conclure au NAT symétrique (son vote est dédupliqué).
        let mut o = ObservedAddrs::new();
        o.observe(peer(1), addr("1.2.3.4:5"));
        o.observe(peer(1), addr("1.2.3.4:6"));
        assert!(!o.is_symmetric());
    }

    #[test]
    fn candidate_ordering() {
        let cands = ordered_candidates(
            &[addr("192.168.0.2:4000")],
            Some(addr("9.9.9.9:5000")),
            &[addr("50.50.50.50:443")],
            false,
        );
        assert_eq!(cands[0].kind, CandidateKind::LocalDirect);
        assert_eq!(cands[1].kind, CandidateKind::PublicDirect);
        assert_eq!(cands[2].kind, CandidateKind::Relay);
    }

    #[test]
    fn symmetric_downgrades_public_to_holepunch() {
        let cands = ordered_candidates(&[], Some(addr("9.9.9.9:5000")), &[], true);
        assert_eq!(cands[0].kind, CandidateKind::HolePunch);
    }
}
