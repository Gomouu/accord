//! Table de routage Kademlia : 256 k-buckets, k=20, éviction LRS avec PING de
//! vérification, diversité de préfixe IP contre les attaques Sybil (SPEC §4).

use accord_proto::limits::DHT_K;
use accord_proto::types::{NodeId, NodeInfo};
use std::collections::HashMap;
use std::net::IpAddr;

use crate::distance::sort_by_distance;

/// Entrée de bucket : coordonnées d'un pair et dernière activité.
#[derive(Debug, Clone)]
pub struct NodeEntry {
    /// Coordonnées complètes du pair.
    pub info: NodeInfo,
    /// Dernière fois où le pair a été vu (ms).
    pub last_seen_ms: u64,
}

/// Résultat d'une tentative d'insertion dans la table.
#[derive(Debug, PartialEq, Eq)]
pub enum InsertOutcome {
    /// Nouveau pair inséré.
    Inserted,
    /// Pair déjà connu, rafraîchi.
    Updated,
    /// Bucket plein : `candidate` (le moins récemment vu) doit être PINGé ;
    /// s'il ne répond pas, rappeler [`RoutingTable::replace`].
    Full {
        /// Candidat à l'éviction (least-recently-seen).
        candidate: NodeId,
    },
    /// Rejeté : trop de pairs du même préfixe IP dans ce bucket (anti-Sybil).
    RejectedDiversity,
    /// Rejeté : c'est notre propre identifiant.
    RejectedSelf,
}

/// Extrait le préfixe de diversité d'une IP : /24 en IPv4, /48 en IPv6.
fn ip_prefix(ip: IpAddr) -> [u8; 16] {
    let mut p = [0u8; 16];
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            p[0..3].copy_from_slice(&o[0..3]);
        }
        IpAddr::V6(v6) => {
            let o = v6.octets();
            p[0..6].copy_from_slice(&o[0..6]);
        }
    }
    p
}

fn primary_prefix(info: &NodeInfo) -> Option<[u8; 16]> {
    info.addrs.first().map(|a| ip_prefix(a.0.ip()))
}

const MAX_PER_PREFIX: usize = 2;

/// Table de routage : un bucket par bit de l'espace 256 bits.
pub struct RoutingTable {
    local: NodeId,
    buckets: Vec<Vec<NodeEntry>>,
}

impl RoutingTable {
    /// Crée une table vide pour l'identité locale `local`.
    pub fn new(local: NodeId) -> Self {
        Self {
            local,
            buckets: (0..256).map(|_| Vec::new()).collect(),
        }
    }

    /// Seau d'un identifiant, ou `None` si c'est le nôtre.
    ///
    /// L'indice vient d'un `NodeId` reçu du réseau : on le résout par
    /// `get_mut` et non par indexation directe. Un indice hors des 256 seaux
    /// est aujourd'hui impossible (`bucket_index` rend `255 - position du
    /// premier bit à 1`), mais l'indexation le convertirait en panique — sur
    /// une donnée pilotée par un pair, et sous le verrou de la table.
    fn bucket_of(&mut self, id: &NodeId) -> Option<&mut Vec<NodeEntry>> {
        let idx = self.local.bucket_index(id)?;
        self.buckets.get_mut(idx)
    }

    /// Insère ou rafraîchit un pair.
    pub fn insert(&mut self, info: NodeInfo, now_ms: u64) -> InsertOutcome {
        let Some(bucket) = self.bucket_of(&info.node_id) else {
            return InsertOutcome::RejectedSelf;
        };

        if let Some(pos) = bucket.iter().position(|e| e.info.node_id == info.node_id) {
            let mut entry = bucket.remove(pos);
            entry.info = info;
            entry.last_seen_ms = now_ms;
            bucket.push(entry); // le plus récemment vu en fin
            return InsertOutcome::Updated;
        }

        // Diversité de préfixe IP au sein du bucket.
        if let Some(prefix) = primary_prefix(&info) {
            let same = bucket
                .iter()
                .filter(|e| primary_prefix(&e.info) == Some(prefix))
                .count();
            if same >= MAX_PER_PREFIX {
                return InsertOutcome::RejectedDiversity;
            }
        }

        if bucket.len() < DHT_K {
            bucket.push(NodeEntry {
                info,
                last_seen_ms: now_ms,
            });
            InsertOutcome::Inserted
        } else {
            InsertOutcome::Full {
                candidate: bucket[0].info.node_id,
            }
        }
    }

    /// Remplace le candidat évincé (mort au PING) par un nouveau pair.
    /// Sans effet si le candidat a bougé entre-temps.
    pub fn replace(&mut self, dead: &NodeId, fresh: NodeInfo, now_ms: u64) -> InsertOutcome {
        let Some(bucket) = self.bucket_of(&fresh.node_id) else {
            return InsertOutcome::RejectedSelf;
        };
        if let Some(pos) = bucket.iter().position(|e| e.info.node_id == *dead) {
            bucket.remove(pos);
            bucket.push(NodeEntry {
                info: fresh,
                last_seen_ms: now_ms,
            });
            InsertOutcome::Inserted
        } else {
            InsertOutcome::Updated
        }
    }

    /// Marque un pair vivant (répond au PING) : le déplace en fin de bucket.
    pub fn mark_alive(&mut self, id: &NodeId, now_ms: u64) {
        if let Some(bucket) = self.bucket_of(id) {
            if let Some(pos) = bucket.iter().position(|e| e.info.node_id == *id) {
                let mut entry = bucket.remove(pos);
                entry.last_seen_ms = now_ms;
                bucket.push(entry);
            }
        }
    }

    /// Retire un pair (mort confirmé).
    pub fn remove(&mut self, id: &NodeId) {
        if let Some(bucket) = self.bucket_of(id) {
            bucket.retain(|e| e.info.node_id != *id);
        }
    }

    /// Les `count` pairs les plus proches de `target`, tous buckets confondus.
    pub fn closest(&self, target: &NodeId, count: usize) -> Vec<NodeInfo> {
        let mut all: Vec<NodeInfo> = self
            .buckets
            .iter()
            .flat_map(|b| b.iter().map(|e| e.info.clone()))
            .collect();
        sort_by_distance(&mut all, target, |i| i.node_id);
        all.truncate(count);
        all
    }

    /// Nombre total de pairs connus.
    pub fn len(&self) -> usize {
        self.buckets.iter().map(|b| b.len()).sum()
    }

    /// Vrai si la table est vide.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Indices des buckets non vides (pour le rafraîchissement ciblé).
    pub fn nonempty_buckets(&self) -> Vec<usize> {
        self.buckets
            .iter()
            .enumerate()
            .filter(|(_, b)| !b.is_empty())
            .map(|(i, _)| i)
            .collect()
    }

    /// Least-recently-seen d'un bucket (candidat à vérifier au rafraîchissement).
    pub fn bucket_lrs(&self, idx: usize) -> Option<NodeId> {
        self.buckets
            .get(idx)
            .and_then(|b| b.first())
            .map(|e| e.info.node_id)
    }

    /// Instantané de tous les pairs (cache persistant de bons pairs — SPEC §2).
    pub fn snapshot(&self) -> Vec<NodeInfo> {
        self.buckets
            .iter()
            .flat_map(|b| b.iter().map(|e| e.info.clone()))
            .collect()
    }

    /// Statistiques d'occupation par bucket (observabilité).
    pub fn occupancy(&self) -> HashMap<usize, usize> {
        self.buckets
            .iter()
            .enumerate()
            .filter(|(_, b)| !b.is_empty())
            .map(|(i, b)| (i, b.len()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use accord_proto::types::WireAddr;
    use std::net::SocketAddr;

    fn info(id: u8, ip: &str) -> NodeInfo {
        NodeInfo {
            node_id: NodeId([id; 32]),
            static_pub: [id; 32],
            pow_nonce: 0,
            flags: 0,
            addrs: vec![WireAddr(ip.parse::<SocketAddr>().unwrap())],
        }
    }

    fn local() -> NodeId {
        NodeId([0; 32])
    }

    #[test]
    fn insert_and_closest() {
        let mut rt = RoutingTable::new(local());
        assert_eq!(rt.insert(info(1, "1.0.0.1:1"), 0), InsertOutcome::Inserted);
        assert_eq!(rt.insert(info(2, "2.0.0.1:1"), 0), InsertOutcome::Inserted);
        assert_eq!(rt.insert(info(1, "1.0.0.1:1"), 1), InsertOutcome::Updated);
        assert_eq!(rt.len(), 2);
        let closest = rt.closest(&NodeId([1; 32]), 1);
        assert_eq!(closest[0].node_id, NodeId([1; 32]));
    }

    #[test]
    fn self_rejected() {
        let mut rt = RoutingTable::new(local());
        assert_eq!(
            rt.insert(info(0, "1.0.0.1:1"), 0),
            InsertOutcome::RejectedSelf
        );
    }

    #[test]
    fn bucket_full_returns_lrs_candidate() {
        // Force tous les nœuds dans le même bucket : ids qui partagent le bit
        // de poids fort avec la distance. Avec local=0, le bucket d'un id est
        // déterminé par son bit de poids fort ; on choisit des ids dans
        // [0x80.., 0xFF..] mais avec le même octet de tête pour rester groupés.
        let mut rt = RoutingTable::new(local());
        // Tous ces ids ont 0x80 comme octet de tête ⇒ bucket 255.
        for i in 0..DHT_K {
            let mut id = [0u8; 32];
            id[0] = 0x80;
            id[31] = i as u8;
            let ni = NodeInfo {
                node_id: NodeId(id),
                static_pub: id,
                pow_nonce: 0,
                flags: 0,
                addrs: vec![WireAddr(format!("10.{i}.0.1:1").parse().unwrap())],
            };
            assert_eq!(rt.insert(ni, i as u64), InsertOutcome::Inserted);
        }
        // Le (k+1)e déclenche l'éviction : candidat = premier inséré.
        let mut id = [0u8; 32];
        id[0] = 0x80;
        id[31] = 200;
        let extra = NodeInfo {
            node_id: NodeId(id),
            static_pub: id,
            pow_nonce: 0,
            flags: 0,
            addrs: vec![WireAddr("10.200.0.1:1".parse().unwrap())],
        };
        match rt.insert(extra.clone(), 100) {
            InsertOutcome::Full { candidate } => {
                // Le candidat est le LRS (premier inséré).
                let mut first = [0u8; 32];
                first[0] = 0x80;
                assert_eq!(candidate, NodeId(first));
                // S'il est mort, on remplace.
                assert_eq!(rt.replace(&candidate, extra, 101), InsertOutcome::Inserted);
            }
            other => panic!("attendu Full, obtenu {other:?}"),
        }
        assert_eq!(rt.len(), DHT_K);
    }

    #[test]
    fn ip_diversity_enforced() {
        let mut rt = RoutingTable::new(local());
        // Trois nœuds du même /24 ⇒ le 3e est rejeté (max 2).
        let mk = |last: u8, host: u8| NodeInfo {
            node_id: NodeId([host; 32]),
            static_pub: [host; 32],
            pow_nonce: 0,
            flags: 0,
            addrs: vec![WireAddr(format!("192.168.1.{last}:1").parse().unwrap())],
        };
        assert_eq!(rt.insert(mk(1, 10), 0), InsertOutcome::Inserted);
        assert_eq!(rt.insert(mk(2, 11), 0), InsertOutcome::Inserted);
        // Note : ces ids peuvent tomber dans des buckets différents ; on force
        // le même bucket en gardant l'octet de tête identique.
        let same_bucket = |last: u8, tail: u8| NodeInfo {
            node_id: NodeId({
                let mut id = [0u8; 32];
                id[0] = 0x40;
                id[31] = tail;
                id
            }),
            static_pub: [tail; 32],
            pow_nonce: 0,
            flags: 0,
            addrs: vec![WireAddr(format!("172.16.5.{last}:1").parse().unwrap())],
        };
        assert_eq!(rt.insert(same_bucket(1, 1), 0), InsertOutcome::Inserted);
        assert_eq!(rt.insert(same_bucket(2, 2), 0), InsertOutcome::Inserted);
        assert_eq!(
            rt.insert(same_bucket(3, 3), 0),
            InsertOutcome::RejectedDiversity
        );
    }

    #[test]
    fn mark_alive_and_remove() {
        let mut rt = RoutingTable::new(local());
        rt.insert(info(5, "5.0.0.1:1"), 0);
        rt.mark_alive(&NodeId([5; 32]), 10);
        assert_eq!(rt.len(), 1);
        rt.remove(&NodeId([5; 32]));
        assert!(rt.is_empty());
    }
}
