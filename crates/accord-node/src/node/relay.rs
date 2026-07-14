//! Intégration nœud du NAT traversal (SPEC §11.1 détection cone/symétrique et
//! §11.3 repli relais). Logique pure et testable, sans horloge ni réseau :
//!
//! - [`NatKind`] et [`classify_nat`] déduisent la nature du NAT local à partir
//!   des observations d'adresse agrégées ([`accord_transport::nat::ObservedAddrs`]),
//!   exposée par `network.status` ;
//! - [`pair_key`], [`is_relay_candidate`] et [`select_relays`] réalisent la
//!   sélection DÉTERMINISTE d'un relais partagé par les deux amis (§11.3) : les
//!   deux côtés calculent la même clé de paire et filtrent la table de routage
//!   sur le drapeau relais.
//!
//! Le câblage (agrégation des `ObservedAddr`, déclenchement du repli, ouverture
//! du circuit) vit dans [`crate::runtime`] et [`crate::maintenance`].

use accord_proto::types::{node_flags, NodeId, NodeInfo};
use accord_transport::nat::ObservedAddrs;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::net::SocketAddr;

/// Délai laissé au poinçonnage pour établir une session avant de basculer sur
/// le relais (SPEC §11.3). Constante nommée plutôt que magique : au-delà, on
/// considère le poinçonnage en échec et on tente le repli.
pub const PUNCH_FALLBACK_MS: u64 = 3_000;

/// Nombre de candidats relais les plus proches de la clé de paire examinés lors
/// de la sélection (les suivants servent de repli si le plus proche échoue).
pub const RELAY_SELECT_K: usize = 8;

/// Nombre de relais « domicile » entretenus par un nœud (sessions directes
/// maintenues) et essayés en repli par un expéditeur. Volontairement petit :
/// borne le coût des sessions côté destinataire comme le nombre de tentatives
/// côté expéditeur.
pub const HOME_RELAY_COUNT: usize = 2;

/// Borne dure du nombre total de candidats relais essayés pour joindre un pair
/// (candidats de clé de paire puis relais domicile du pair, sans doublon).
pub const RELAY_TRY_MAX: usize = RELAY_SELECT_K + HOME_RELAY_COUNT;

/// Nature du NAT local déduite par recoupement d'observations d'adresse
/// (SPEC §11.1). Champ additif exposé par `network.status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NatKind {
    /// Indéterminé : trop peu d'observations pour conclure.
    Unknown,
    /// NAT « cone » : les pairs interrogés rapportent la MÊME adresse publique
    /// (consensus) — l'adresse est réutilisable, poinçonnage direct viable.
    Cone,
    /// NAT symétrique : les observations DIVERGENT selon le pair interrogé
    /// (adresse/port différents) — poinçonnage direct non viable, relais requis.
    Symmetric,
}

impl NatKind {
    /// Libellé stable (identique à la sérialisation JSON), pour la journalisation.
    pub fn as_str(&self) -> &'static str {
        match self {
            NatKind::Unknown => "unknown",
            NatKind::Cone => "cone",
            NatKind::Symmetric => "symmetric",
        }
    }
}

/// Déduit la nature du NAT à partir des observations agrégées (SPEC §11.1) :
/// - divergence (`is_symmetric`) ⇒ [`NatKind::Symmetric`] ;
/// - consensus (≥ 2 pairs concordants) ⇒ [`NatKind::Cone`] ;
/// - sinon (0 ou 1 observation) ⇒ [`NatKind::Unknown`].
pub fn classify_nat(observed: &ObservedAddrs) -> NatKind {
    if observed.is_symmetric() {
        NatKind::Symmetric
    } else if observed.consensus().is_some() {
        NatKind::Cone
    } else {
        NatKind::Unknown
    }
}

/// Décide si ce nœud peut s'annoncer RELAIS (drapeau `NODE_ANNOUNCE`,
/// SPEC §11.3) : il doit être plausiblement joignable de l'extérieur, sinon
/// les « relais domicile » élus par dérivation déterministe seraient
/// injoignables et le rendez-vous du premier contact échouerait. Critères :
/// - un mapping de port automatique (UPnP/NAT-PMP) est actif ; ou
/// - le consensus d'adresse observée (≥ 2 pairs concordants) porte le MÊME
///   port que le port local — nœud public, redirection manuelle, ou NAT
///   préservant le port (joignable tant que ses sessions entretiennent le
///   mapping).
///
/// Fonction pure — testable sans horloge ni réseau.
pub fn relay_eligible(
    observed_consensus: Option<SocketAddr>,
    local_port: u16,
    mapping_external: Option<SocketAddr>,
) -> bool {
    if mapping_external.is_some() {
        return true;
    }
    matches!(observed_consensus, Some(a) if a.port() == local_port)
}

/// Drapeaux de capacité à annoncer selon l'éligibilité relais.
pub fn announce_flags(eligible: bool) -> u8 {
    if eligible {
        node_flags::RELAY
    } else {
        0
    }
}

/// Clé de paire DÉTERMINISTE et symétrique pour deux nœuds (SPEC §11.3) :
/// `sha256(min(a, b) ‖ max(a, b))`. L'ordre canonique (min/max sur l'identifiant
/// Kademlia) garantit `pair_key(a, b) == pair_key(b, a)` : les DEUX amis
/// calculent la MÊME clé indépendamment du côté, donc convergent vers le même
/// relais après filtrage de la table de routage.
pub fn pair_key(a: &NodeId, b: &NodeId) -> [u8; 32] {
    let (lo, hi) = if a.0 <= b.0 { (a, b) } else { (b, a) };
    let mut h = Sha256::new();
    h.update(lo.0);
    h.update(hi.0);
    h.finalize().into()
}

/// Vrai si `info` est un candidat relais retenu : il annonce le drapeau relais
/// ([`node_flags::RELAY`]), dispose d'au moins une adresse, et n'est pas exclu
/// (soi-même ou l'un des deux amis). Le caractère joignable est attesté par
/// l'annonceur via le drapeau plutôt que ré-inspecté ici : la sélection fait
/// confiance au drapeau, et un relais injoignable est de toute façon écarté à
/// l'ouverture du circuit (cf. [`crate::runtime::Runtime::ensure_relay_to`]).
pub fn is_relay_candidate(info: &NodeInfo, exclude: &[[u8; 32]]) -> bool {
    info.flags & node_flags::RELAY != 0
        && !info.addrs.is_empty()
        && !exclude.contains(&info.static_pub)
}

/// Filtre une liste de candidats (déjà triés par distance XOR à la clé de paire,
/// tels que rendus par `closest_local`) sur le drapeau relais, en préservant
/// l'ordre : le premier élément est le relais le plus proche, les suivants
/// servent de repli. `exclude` écarte soi-même et les deux amis.
pub fn select_relays(candidates: Vec<NodeInfo>, exclude: &[[u8; 32]]) -> Vec<NodeInfo> {
    candidates
        .into_iter()
        .filter(|info| is_relay_candidate(info, exclude))
        .collect()
}

/// Relais « domicile » d'un nœud : les candidats (déjà triés par distance XOR
/// à `node_id_of(pubkey)` du propriétaire, tels que rendus par `closest_local`)
/// filtrés sur le drapeau relais, bornés à [`HOME_RELAY_COUNT`]. Dérivation
/// DÉTERMINISTE et symétrique : le propriétaire (qui entretient une session
/// vers chacun) et un expéditeur inconnu (premier contact, SPEC §11.3) qui lit
/// une table cohérente convergent vers les MÊMES relais — c'est le point de
/// rendez-vous qui rend le premier contact possible sans port ouvert.
pub fn select_home_relays(candidates: Vec<NodeInfo>, exclude: &[[u8; 32]]) -> Vec<NodeInfo> {
    let mut relays = select_relays(candidates, exclude);
    relays.truncate(HOME_RELAY_COUNT);
    relays
}

/// Liste ordonnée des candidats relais à essayer pour joindre un pair : les
/// candidats de clé de paire d'abord (chemin historique, convergent entre
/// amis), puis les relais domicile du pair (repli premier contact), sans
/// doublon (par identifiant de nœud) et bornée à [`RELAY_TRY_MAX`].
pub fn merge_relay_candidates(pair: Vec<NodeInfo>, home: Vec<NodeInfo>) -> Vec<NodeInfo> {
    let mut out = pair;
    for info in home {
        if !out.iter().any(|c| c.node_id == info.node_id) {
            out.push(info);
        }
    }
    out.truncate(RELAY_TRY_MAX);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use accord_proto::types::WireAddr;

    fn addr(s: &str) -> SocketAddr {
        s.parse().unwrap()
    }

    #[test]
    fn eligibilite_relais_mapping_ou_port_preserve() {
        // Mapping UPnP/NAT-PMP actif : éligible quoi qu'il en soit.
        assert!(relay_eligible(None, 48016, Some(addr("203.0.113.7:48016"))));
        // Consensus observé au MÊME port que le port local : éligible
        // (public, redirection, ou NAT préservant le port).
        assert!(relay_eligible(Some(addr("203.0.113.7:48016")), 48016, None));
        // Consensus à un port DIFFÉRENT (NAT réécrivant le port) : refusé.
        assert!(!relay_eligible(
            Some(addr("203.0.113.7:50001")),
            48016,
            None
        ));
        // Sans consensus ni mapping : refusé (joignabilité non établie).
        assert!(!relay_eligible(None, 48016, None));
        // Traduction en drapeaux d'annonce.
        assert_eq!(announce_flags(true), node_flags::RELAY);
        assert_eq!(announce_flags(false), 0);
    }

    fn node(id: u8, flags: u8, addrs: &[&str]) -> NodeInfo {
        NodeInfo {
            node_id: NodeId([id; 32]),
            static_pub: [id; 32],
            pow_nonce: 0,
            flags,
            addrs: addrs.iter().map(|a| WireAddr(addr(a))).collect(),
        }
    }

    #[test]
    fn nat_kind_consensus_donne_cone() {
        let mut o = ObservedAddrs::new();
        o.observe(addr("203.0.113.7:5000"));
        o.observe(addr("203.0.113.7:5000"));
        assert_eq!(classify_nat(&o), NatKind::Cone);
        assert_eq!(o.consensus(), Some(addr("203.0.113.7:5000")));
    }

    #[test]
    fn nat_kind_divergence_donne_symmetric() {
        let mut o = ObservedAddrs::new();
        o.observe(addr("203.0.113.7:5000"));
        o.observe(addr("203.0.113.7:6001")); // port différent selon le pair
        assert_eq!(classify_nat(&o), NatKind::Symmetric);
    }

    #[test]
    fn nat_kind_trop_peu_observations_reste_unknown() {
        let vide = ObservedAddrs::new();
        assert_eq!(classify_nat(&vide), NatKind::Unknown);
        let mut une = ObservedAddrs::new();
        une.observe(addr("203.0.113.7:5000"));
        assert_eq!(classify_nat(&une), NatKind::Unknown);
    }

    #[test]
    fn pair_key_est_symetrique() {
        let a = NodeId([1; 32]);
        let b = NodeId([2; 32]);
        assert_eq!(
            pair_key(&a, &b),
            pair_key(&b, &a),
            "clé indépendante du côté"
        );
        // Une autre paire donne une clé différente (pas de collision triviale).
        let c = NodeId([3; 32]);
        assert_ne!(pair_key(&a, &b), pair_key(&a, &c));
    }

    #[test]
    fn selection_filtre_non_relais_et_exclus() {
        let relay_pub = node(1, node_flags::RELAY, &["203.0.113.7:5000"]);
        let non_relay = node(2, 0, &["203.0.113.8:5000"]); // pas de drapeau
        let relay_exclu = node(3, node_flags::RELAY, &["203.0.113.9:5000"]);
        let relay_sans_addr = node(4, node_flags::RELAY, &[]); // aucune adresse

        let candidates = vec![
            relay_pub.clone(),
            non_relay,
            relay_exclu.clone(),
            relay_sans_addr,
        ];
        let retenus = select_relays(candidates.clone(), &[relay_exclu.static_pub]);
        assert_eq!(retenus.len(), 1, "un seul candidat valide");
        assert_eq!(retenus[0].static_pub, relay_pub.static_pub);

        // Filtre unitaire cohérent.
        assert!(is_relay_candidate(&relay_pub, &[]));
        assert!(!is_relay_candidate(&relay_pub, &[relay_pub.static_pub]));
    }

    #[test]
    fn selection_relais_domicile_deterministe_filtre_et_bornee() {
        // Quatre relais valides + un non-relais + le propriétaire lui-même :
        // seuls les relais (drapeau + adresse), hors exclus, sont retenus,
        // dans l'ordre d'entrée (distance XOR), bornés à HOME_RELAY_COUNT.
        let moi = node(9, node_flags::RELAY, &["203.0.113.99:5000"]);
        let candidates = vec![
            moi.clone(), // soi-même : exclu
            node(1, node_flags::RELAY, &["203.0.113.1:5000"]),
            node(2, 0, &["203.0.113.2:5000"]), // pas relais : filtré
            node(3, node_flags::RELAY, &["203.0.113.3:5000"]),
            node(4, node_flags::RELAY, &["203.0.113.4:5000"]),
        ];
        let domicile = select_home_relays(candidates.clone(), &[moi.static_pub]);
        assert_eq!(domicile.len(), HOME_RELAY_COUNT, "borné à HOME_RELAY_COUNT");
        assert_eq!(domicile[0].node_id, NodeId([1; 32]), "ordre préservé");
        assert_eq!(domicile[1].node_id, NodeId([3; 32]));
        // Déterministe : même entrée ⇒ même sortie.
        assert_eq!(
            select_home_relays(candidates.clone(), &[moi.static_pub]),
            domicile
        );
        // Moins de candidats que la borne : tous rendus, sans panique.
        let peu = vec![node(5, node_flags::RELAY, &["203.0.113.5:5000"])];
        assert_eq!(select_home_relays(peu, &[]).len(), 1);
    }

    #[test]
    fn fusion_candidats_pair_puis_domicile_sans_doublon_et_bornee() {
        let r = |id: u8| node(id, node_flags::RELAY, &["203.0.113.7:5000"]);
        // Le relais 2 apparaît des deux côtés : une seule occurrence, côté paire.
        let pair = vec![r(1), r(2)];
        let home = vec![r(2), r(3)];
        let merged = merge_relay_candidates(pair, home);
        let ids: Vec<u8> = merged.iter().map(|i| i.node_id.0[0]).collect();
        assert_eq!(
            ids,
            vec![1, 2, 3],
            "paire d'abord, domicile ensuite, sans doublon"
        );

        // Bornée à RELAY_TRY_MAX même sur des entrées trop longues.
        let pair: Vec<NodeInfo> = (1..=RELAY_SELECT_K as u8 + 2).map(r).collect();
        let home: Vec<NodeInfo> = (100..=100 + HOME_RELAY_COUNT as u8 + 2).map(r).collect();
        let merged = merge_relay_candidates(pair, home);
        assert!(merged.len() <= RELAY_TRY_MAX, "borne dure des tentatives");

        // Sans candidat de paire (premier contact pur) : le domicile suffit.
        let merged = merge_relay_candidates(Vec::new(), vec![r(7)]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].node_id, NodeId([7; 32]));
    }
}
