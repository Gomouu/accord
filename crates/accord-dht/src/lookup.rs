//! Lookup itératif Kademlia (SPEC §4) : FIND_NODE et FIND_VALUE avec
//! parallélisme α=3, et croisement de chemins disjoints pour les valeurs
//! sensibles (résistance à l'empoisonnement).

use crate::distance::Distance;
use crate::rpc::{filter_valid, DhtRpc};
use crate::store::{RecordStore, MAX_CLOCK_SKEW_MS};
use accord_proto::dht_msg::DhtBody;
use accord_proto::limits::{DHT_ALPHA, DHT_K};
use accord_proto::types::{DhtRecord, NodeId, NodeInfo};
use futures_util::future::join_all;
use std::collections::{HashMap, HashSet};

/// Envoie le même type de requête aux `α` pairs du lot EN PARALLÈLE et rend
/// `(node_id, réponse)` dans l'ordre du lot. Sérialiser ces envois faisait
/// payer les timeouts en cascade (un pair mort ≈ 2 s bloquait les suivants) ;
/// ici la latence d'un tour vaut le max du lot, pas la somme. La shortlist
/// n'est mutée qu'APRÈS le tour complet — même sémantique itérative qu'avant
/// (`next_to_query` n'était de toute façon rappelé qu'après le lot entier).
async fn query_batch<R: DhtRpc>(
    rpc: &R,
    batch: &[NodeInfo],
    body: impl Fn() -> DhtBody,
) -> Vec<(NodeId, Option<DhtBody>)> {
    join_all(batch.iter().map(|peer| {
        let body = body();
        async move { (peer.node_id, rpc.send_rpc(peer, body).await) }
    }))
    .await
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PeerState {
    Fresh,
    Queried,
    Failed,
}

/// Shortlist ordonnée par distance à la cible.
struct Shortlist {
    target: NodeId,
    peers: HashMap<NodeId, NodeInfo>,
    state: HashMap<NodeId, PeerState>,
}

impl Shortlist {
    fn new(target: NodeId) -> Self {
        Self {
            target,
            peers: HashMap::new(),
            state: HashMap::new(),
        }
    }

    fn add(&mut self, info: NodeInfo) {
        // La cible elle-même est un pair valide de la recherche.
        self.state.entry(info.node_id).or_insert(PeerState::Fresh);
        self.peers.entry(info.node_id).or_insert(info);
    }

    fn mark(&mut self, id: NodeId, s: PeerState) {
        self.state.insert(id, s);
    }

    /// Les `α` pairs `Fresh` les plus proches non encore interrogés.
    fn next_to_query(&self, alpha: usize, exclude: &HashSet<NodeId>) -> Vec<NodeInfo> {
        let mut fresh: Vec<&NodeInfo> = self
            .peers
            .values()
            .filter(|p| {
                self.state.get(&p.node_id) == Some(&PeerState::Fresh)
                    && !exclude.contains(&p.node_id)
            })
            .collect();
        fresh.sort_by_key(|p| Distance::between(&p.node_id, &self.target).0);
        fresh.into_iter().take(alpha).cloned().collect()
    }

    /// Les `k` pairs les plus proches connus (tous états confondus, non-failed).
    fn closest_k(&self, k: usize) -> Vec<NodeInfo> {
        let mut alive: Vec<&NodeInfo> = self
            .peers
            .values()
            .filter(|p| self.state.get(&p.node_id) != Some(&PeerState::Failed))
            .collect();
        alive.sort_by_key(|p| Distance::between(&p.node_id, &self.target).0);
        alive.into_iter().take(k).cloned().collect()
    }

    /// Vrai si tous les `k` plus proches ont été interrogés (convergence).
    fn converged(&self, k: usize) -> bool {
        self.closest_k(k)
            .iter()
            .all(|p| matches!(self.state.get(&p.node_id), Some(PeerState::Queried)))
    }
}

/// Lookup FIND_NODE : rend les k nœuds les plus proches de `target`.
pub async fn find_node<R: DhtRpc>(
    rpc: &R,
    target: NodeId,
    seeds: Vec<NodeInfo>,
    pow_bits: u32,
) -> Vec<NodeInfo> {
    let mut sl = Shortlist::new(target);
    for s in filter_valid(seeds, pow_bits) {
        sl.add(s);
    }
    let local = rpc.local_id();
    let mut excluded = HashSet::new();
    excluded.insert(local);

    loop {
        let batch = sl.next_to_query(DHT_ALPHA, &excluded);
        if batch.is_empty() || sl.converged(DHT_K) {
            break;
        }
        for (peer_id, resp) in
            query_batch(rpc, &batch, || DhtBody::FindNode { target: target.0 }).await
        {
            match resp {
                Some(DhtBody::FoundNodes { nodes }) => {
                    sl.mark(peer_id, PeerState::Queried);
                    for n in filter_valid(nodes, pow_bits) {
                        if n.node_id != local {
                            sl.add(n);
                        }
                    }
                }
                _ => sl.mark(peer_id, PeerState::Failed),
            }
        }
    }
    sl.closest_k(DHT_K)
}

/// Résultat d'un FIND_VALUE.
pub enum ValueResult {
    /// Valeur trouvée et validée.
    Found(DhtRecord),
    /// Non trouvée ; nœuds les plus proches (pour un STORE ultérieur).
    NotFound(Vec<NodeInfo>),
}

async fn find_value_path<R: DhtRpc>(
    rpc: &R,
    key: [u8; 32],
    seeds: Vec<NodeInfo>,
    pow_bits: u32,
    max_ts: u64,
) -> (Option<DhtRecord>, Vec<NodeInfo>) {
    let target = NodeId(key);
    let mut sl = Shortlist::new(target);
    for s in seeds {
        sl.add(s);
    }
    let local = rpc.local_id();
    let mut excluded = HashSet::new();
    excluded.insert(local);
    let mut best: Option<DhtRecord> = None;

    loop {
        let batch = sl.next_to_query(DHT_ALPHA, &excluded);
        if batch.is_empty() || sl.converged(DHT_K) {
            break;
        }
        for (peer_id, resp) in query_batch(rpc, &batch, || DhtBody::FindValue { key }).await {
            match resp {
                Some(DhtBody::FoundValue { value, nodes }) => {
                    sl.mark(peer_id, PeerState::Queried);
                    if let Some(rec) = value {
                        // Ignore un horodatage au-delà de la borne (record du
                        // futur, potentiellement gonflé par un pair malveillant).
                        if rec.key == key
                            && rec.timestamp_ms <= max_ts
                            && RecordStore::validate(&rec).is_ok()
                        {
                            // Garde la valeur signée la plus récente du chemin.
                            let take = match &best {
                                Some(b) => rec.timestamp_ms > b.timestamp_ms,
                                None => true,
                            };
                            if take {
                                best = Some(rec);
                            }
                        }
                    }
                    for n in filter_valid(nodes, pow_bits) {
                        if n.node_id != local {
                            sl.add(n);
                        }
                    }
                }
                _ => sl.mark(peer_id, PeerState::Failed),
            }
        }
    }
    (best, sl.closest_k(DHT_K))
}

/// Lookup FIND_VALUE avec croisement de `paths` chemins disjoints (SPEC §4 :
/// 2 chemins pour les valeurs sensibles). Les seeds sont répartis en ensembles
/// de premiers sauts disjoints.
///
/// Conserve la voie sûre par défaut : sans horloge à ce niveau, aucune borne
/// d'horodatage n'est appliquée. L'appelant qui dispose d'une horloge (p. ex.
/// `KademliaNode::get`) devrait préférer [`find_value_bounded`] afin de rejeter
/// les records datés du futur au moment de la sélection.
pub async fn find_value<R: DhtRpc>(
    rpc: &R,
    key: [u8; 32],
    seeds: Vec<NodeInfo>,
    pow_bits: u32,
    paths: usize,
) -> ValueResult {
    // `u64::MAX` : borne d'horodatage neutre (comportement historique préservé).
    find_value_bounded(rpc, key, seeds, pow_bits, paths, u64::MAX).await
}

/// Variante de [`find_value`] bornant l'horodatage des candidats à
/// `now_ms + MAX_CLOCK_SKEW_MS`. Un record daté au-delà (p. ex. `u64::MAX`) est
/// écarté à la sélection : la défense par chemins disjoints ne peut plus être
/// contournée par un simple `timestamp_ms` gonflé.
///
/// Politique de sélection robuste : on retient le record confirmé par le plus
/// grand nombre de chemins disjoints (consensus), les égalités étant tranchées
/// par l'horodatage (borné) le plus récent. Un unique chemin compromis ne peut
/// donc pas imposer sa valeur face à un consensus de chemins. La validation
/// sémantique finale du contenu reste à la charge du lecteur (accord-core).
pub async fn find_value_bounded<R: DhtRpc>(
    rpc: &R,
    key: [u8; 32],
    seeds: Vec<NodeInfo>,
    pow_bits: u32,
    paths: usize,
    now_ms: u64,
) -> ValueResult {
    let seeds = filter_valid(seeds, pow_bits);
    // Bornage haut autant que bas : `paths` dimensionne une allocation, et un
    // réglage aberrant (ou un `usize` mal converti) ne doit pas se traduire par
    // une réservation démesurée. Au-delà de `DHT_K` chemins, les seeds sont de
    // toute façon en nombre insuffisant pour les remplir.
    let paths = paths.clamp(1, DHT_K);
    let max_ts = now_ms.saturating_add(MAX_CLOCK_SKEW_MS);

    // Répartition round-robin des seeds en chemins disjoints.
    let mut buckets: Vec<Vec<NodeInfo>> = vec![Vec::new(); paths];
    for (i, seed) in seeds.into_iter().enumerate() {
        buckets[i % paths].push(seed);
    }

    let mut candidates: Vec<DhtRecord> = Vec::new();
    let mut closest: Vec<NodeInfo> = Vec::new();
    for bucket in buckets {
        if bucket.is_empty() {
            continue;
        }
        let (value, near) = find_value_path(rpc, key, bucket, pow_bits, max_ts).await;
        if let Some(rec) = value {
            candidates.push(rec);
        }
        // Fusionne les nœuds proches, en dédupliquant.
        for n in near {
            if !closest.iter().any(|c| c.node_id == n.node_id) {
                closest.push(n);
            }
        }
    }
    match select_consensus(candidates) {
        Some(rec) => ValueResult::Found(rec),
        None => {
            closest.sort_by_key(|p| Distance::between(&p.node_id, &NodeId(key)).0);
            closest.truncate(DHT_K);
            ValueResult::NotFound(closest)
        }
    }
}

/// Sélectionne le record final parmi les meilleurs candidats de chaque chemin :
/// celui confirmé par le plus de chemins disjoints, les égalités étant tranchées
/// par l'horodatage le plus récent. Empêche un timestamp élevé isolé d'écraser
/// un consensus de chemins.
fn select_consensus(candidates: Vec<DhtRecord>) -> Option<DhtRecord> {
    let mut tally: Vec<(DhtRecord, usize)> = Vec::new();
    for rec in candidates {
        match tally.iter_mut().find(|(r, _)| *r == rec) {
            Some(entry) => entry.1 += 1,
            None => tally.push((rec, 1)),
        }
    }
    tally
        .into_iter()
        .max_by(|(ra, ca), (rb, cb)| ca.cmp(cb).then(ra.timestamp_ms.cmp(&rb.timestamp_ms)))
        .map(|(rec, _)| rec)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testnet::TestNet;
    use accord_crypto::Identity;
    use accord_proto::types::{RecordKind, WireAddr};
    use async_trait::async_trait;

    /// RPC simulé : à chaque pair est associée la réponse `FoundValue` qu'il
    /// renvoie. Aucune propagation de nœuds ; chaque chemin converge sur son
    /// unique seed, ce qui rend les tests déterministes.
    struct MockRpc {
        local: NodeId,
        values: HashMap<NodeId, Option<DhtRecord>>,
    }

    #[async_trait]
    impl DhtRpc for MockRpc {
        async fn send_rpc(&self, to: &NodeInfo, body: DhtBody) -> Option<DhtBody> {
            match body {
                DhtBody::FindValue { .. } => Some(DhtBody::FoundValue {
                    value: self.values.get(&to.node_id).cloned().flatten(),
                    nodes: Vec::new(),
                }),
                _ => Some(DhtBody::FoundNodes { nodes: Vec::new() }),
            }
        }

        fn local_id(&self) -> NodeId {
            self.local
        }
    }

    fn node_info(id: &Identity, tag: u8) -> NodeInfo {
        NodeInfo {
            node_id: id.node_id(),
            static_pub: id.public_key(),
            pow_nonce: id.pow_nonce(),
            flags: 0,
            addrs: vec![WireAddr(format!("10.0.0.{tag}:4433").parse().unwrap())],
        }
    }

    fn presence_at(publisher: &Identity, key: [u8; 32], ts: u64) -> DhtRecord {
        let mut rec = DhtRecord {
            key,
            kind: RecordKind::Presence,
            value: vec![1, 2, 3],
            publisher: publisher.public_key(),
            timestamp_ms: ts,
            expiry_s: 3600,
            sig: [0; 64],
        };
        rec.sig = publisher.sign(&rec.signable_bytes());
        rec
    }

    #[tokio::test]
    async fn find_value_bounded_rejects_future_timestamp() {
        // Faille #3 : un record au timestamp gonflé (u64::MAX) sur un chemin ne
        // doit pas être choisi ; le record honnête (borné) est retenu.
        let honnete = Identity::generate_with_pow_bits(1);
        let attaquant = Identity::generate_with_pow_bits(1);
        let seed_h = Identity::generate_with_pow_bits(1);
        let seed_a = Identity::generate_with_pow_bits(1);
        let local = Identity::generate_with_pow_bits(1);
        let key = [7u8; 32];
        let now = 1_000_000;

        let bon = presence_at(&honnete, key, now);
        let poison = presence_at(&attaquant, key, u64::MAX);

        let mut values = HashMap::new();
        values.insert(seed_h.node_id(), Some(bon.clone()));
        values.insert(seed_a.node_id(), Some(poison));
        let rpc = MockRpc {
            local: local.node_id(),
            values,
        };
        let seeds = vec![node_info(&seed_h, 1), node_info(&seed_a, 2)];

        match find_value_bounded(&rpc, key, seeds, 1, 2, now).await {
            ValueResult::Found(rec) => assert_eq!(rec, bon, "le record borné doit gagner"),
            ValueResult::NotFound(_) => panic!("record honnête introuvable"),
        }
    }

    #[tokio::test]
    async fn find_value_prefers_path_consensus() {
        // Faille #3 : un record confirmé par deux chemins l'emporte sur un
        // record isolé au timestamp plus élevé (mais dans la borne d'horloge).
        let honnete = Identity::generate_with_pow_bits(1);
        let attaquant = Identity::generate_with_pow_bits(1);
        let s0 = Identity::generate_with_pow_bits(1);
        let s1 = Identity::generate_with_pow_bits(1);
        let s2 = Identity::generate_with_pow_bits(1);
        let local = Identity::generate_with_pow_bits(1);
        let key = [9u8; 32];
        let now = 1_000_000;

        let bon = presence_at(&honnete, key, now - 5_000);
        let poison = presence_at(&attaquant, key, now); // plus récent mais isolé

        let mut values = HashMap::new();
        values.insert(s0.node_id(), Some(bon.clone()));
        values.insert(s1.node_id(), Some(bon.clone()));
        values.insert(s2.node_id(), Some(poison));
        let rpc = MockRpc {
            local: local.node_id(),
            values,
        };
        let seeds = vec![node_info(&s0, 1), node_info(&s1, 2), node_info(&s2, 3)];

        match find_value_bounded(&rpc, key, seeds, 1, 3, now).await {
            ValueResult::Found(rec) => assert_eq!(rec, bon, "le consensus de chemins doit primer"),
            ValueResult::NotFound(_) => panic!("record introuvable"),
        }
    }

    /// RPC simulé où chaque pair met `delay_ms` à répondre (pair mort dont le
    /// timeout court, p. ex.).
    struct SlowRpc {
        local: NodeId,
        delay_ms: u64,
    }

    #[async_trait]
    impl DhtRpc for SlowRpc {
        async fn send_rpc(&self, _to: &NodeInfo, _body: DhtBody) -> Option<DhtBody> {
            tokio::time::sleep(std::time::Duration::from_millis(self.delay_ms)).await;
            Some(DhtBody::FoundNodes { nodes: Vec::new() })
        }

        fn local_id(&self) -> NodeId {
            self.local
        }
    }

    #[tokio::test(start_paused = true)]
    async fn un_pair_lent_ne_serialise_pas_le_lot() {
        // Trois seeds répondant chacun en 2 s (l'ordre de grandeur d'un
        // timeout de pair mort) : le lot α=3 doit partir en parallèle, donc
        // le tour coûte ~2 s — la version sérialisée coûtait ~6 s. L'horloge
        // tokio pausée rend la mesure déterministe.
        let local = Identity::generate_with_pow_bits(1);
        let s0 = Identity::generate_with_pow_bits(1);
        let s1 = Identity::generate_with_pow_bits(1);
        let s2 = Identity::generate_with_pow_bits(1);
        let rpc = SlowRpc {
            local: local.node_id(),
            delay_ms: 2_000,
        };
        let seeds = vec![node_info(&s0, 1), node_info(&s1, 2), node_info(&s2, 3)];

        let debut = tokio::time::Instant::now();
        find_node(&rpc, NodeId([9u8; 32]), seeds, 1).await;
        let ecoule = debut.elapsed();
        assert!(
            ecoule < std::time::Duration::from_millis(4_000),
            "lot sérialisé : {ecoule:?} (attendu ~2 s)"
        );
    }

    #[tokio::test]
    async fn find_node_converges_on_target() {
        let net = TestNet::new(60, 8).await;
        // Un nœud arbitraire cherche un autre via ses seeds bootstrap.
        let (rpc, seeds) = net.client(0);
        let target = net.node_id(40);
        let result = find_node(&rpc, target, seeds, 8).await;
        assert!(
            result.iter().any(|n| n.node_id == target),
            "la cible doit figurer parmi les k plus proches"
        );
    }

    #[tokio::test]
    async fn find_value_retrieves_stored_record() {
        let net = TestNet::new(60, 8).await;
        let publisher = Identity::generate_with_pow_bits(8);
        let (key, record) = net.publish_identity(&publisher, 30).await;
        let (rpc, seeds) = net.client(5);
        match find_value(&rpc, key, seeds, 8, 2).await {
            ValueResult::Found(rec) => assert_eq!(rec, record),
            ValueResult::NotFound(_) => panic!("record introuvable"),
        }
    }
}
