//! Nœud Kademlia : traitement des RPC entrants, opérations itératives
//! (bootstrap, lookup, get/put), rafraîchissement et republication (SPEC §4).

use crate::lookup::{find_node, find_value_bounded, ValueResult};
use crate::routing::{InsertOutcome, RoutingTable};
use crate::rpc::{valid_node, DhtRpc};
use crate::store::RecordStore;
use accord_proto::dht_msg::{DhtBody, DhtMessage};
use accord_proto::limits::{DHT_K, IDENTITY_POW_BITS};
use accord_proto::types::{DhtRecord, NodeId, NodeInfo};
use accord_transport::ratelimit::RateLimiter;
use std::sync::Mutex;

/// Configuration d'un nœud DHT.
#[derive(Debug, Clone, Copy)]
pub struct DhtConfig {
    /// Difficulté PoW exigée des pairs.
    pub pow_bits: u32,
    /// Nombre maximal de records stockés localement.
    pub max_records: usize,
    /// Nombre de chemins disjoints pour les FIND_VALUE sensibles.
    pub value_paths: usize,
}

impl Default for DhtConfig {
    fn default() -> Self {
        Self {
            pow_bits: IDENTITY_POW_BITS,
            max_records: 8192,
            value_paths: 2,
        }
    }
}

struct Inner {
    routing: RoutingTable,
    store: RecordStore,
    rpc_rate: RateLimiter,
    store_rate: RateLimiter,
}

/// Nœud Kademlia. Le traitement des RPC entrants est synchrone ; les
/// opérations itératives sont asynchrones via un [`DhtRpc`].
pub struct KademliaNode {
    local: NodeInfo,
    config: DhtConfig,
    inner: Mutex<Inner>,
}

impl KademliaNode {
    /// Crée un nœud pour l'identité locale `local`.
    pub fn new(local: NodeInfo, config: DhtConfig) -> Self {
        let routing = RoutingTable::new(local.node_id);
        Self {
            local,
            config,
            inner: Mutex::new(Inner {
                routing,
                store: RecordStore::new(config.max_records),
                // SPEC §4 : 10 RPC/s régime, rafale 40 ; STORE 2/s, rafale 8.
                rpc_rate: RateLimiter::new(40.0, 10.0),
                store_rate: RateLimiter::new(8.0, 2.0),
            }),
        }
    }

    /// Coordonnées locales.
    pub fn local(&self) -> &NodeInfo {
        &self.local
    }

    /// Identifiant local.
    pub fn node_id(&self) -> NodeId {
        self.local.node_id
    }

    /// Apprend un pair (insertion dans la table, sous réserve de validité).
    pub fn observe(&self, info: NodeInfo, now_ms: u64) -> InsertOutcome {
        if !valid_node(&info, self.config.pow_bits) {
            return InsertOutcome::RejectedSelf;
        }
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .routing
            .insert(info, now_ms)
    }

    /// Nombre de pairs connus.
    pub fn peer_count(&self) -> usize {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .routing
            .len()
    }

    /// Nombre de records stockés.
    pub fn record_count(&self) -> usize {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .store
            .len()
    }

    /// Instantané des pairs connus (cache persistant).
    pub fn peers_snapshot(&self) -> Vec<NodeInfo> {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .routing
            .snapshot()
    }

    /// Traite un RPC entrant de `from`, rend la réponse à renvoyer (même
    /// `rpc_id`). Applique l'anti-abus par IP et apprend l'émetteur.
    pub fn handle_rpc(&self, from: &NodeInfo, msg: DhtMessage, now_ms: u64) -> Option<DhtMessage> {
        let ip = from.addrs.first()?.0.ip();
        let is_store = matches!(msg.body, DhtBody::Store { .. });

        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        // Anti-abus : token bucket par IP ; STORE plus coûteux.
        let allowed = if is_store {
            inner.store_rate.check(ip, 1.0, now_ms) && inner.rpc_rate.check(ip, 4.0, now_ms)
        } else {
            inner.rpc_rate.check(ip, 1.0, now_ms)
        };
        if !allowed {
            return None; // rejet silencieux
        }

        // Apprend l'émetteur (s'il est valide).
        if valid_node(from, self.config.pow_bits) {
            inner.routing.insert(from.clone(), now_ms);
        }

        let response = match msg.body {
            DhtBody::Ping => DhtBody::Pong,
            DhtBody::FindNode { target } => DhtBody::FoundNodes {
                nodes: inner.routing.closest(&NodeId(target), DHT_K),
            },
            DhtBody::FindValue { key } => match inner.store.get(&key, now_ms) {
                Some(record) => DhtBody::FoundValue {
                    value: Some(record),
                    nodes: Vec::new(),
                },
                None => DhtBody::FoundValue {
                    value: None,
                    nodes: inner.routing.closest(&NodeId(key), DHT_K),
                },
            },
            DhtBody::Store { record } => match inner.store.put(record, now_ms) {
                Ok(()) => DhtBody::StoreOk,
                Err(crate::store::StoreError::Full) => DhtBody::Error { code: 0x03 },
                Err(_) => DhtBody::Error { code: 0x04 },
            },
            // Réponses reçues hors contexte de requête : ignorées.
            DhtBody::Pong
            | DhtBody::FoundNodes { .. }
            | DhtBody::FoundValue { .. }
            | DhtBody::StoreOk
            | DhtBody::Error { .. } => return None,
        };
        Some(DhtMessage {
            rpc_id: msg.rpc_id,
            body: response,
        })
    }

    /// Purge les records expirés ; rend le nombre supprimé.
    pub fn expire_records(&self, now_ms: u64) -> usize {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .store
            .expire(now_ms)
    }

    /// Records valides détenus (pour republication périodique).
    pub fn records_to_republish(&self, now_ms: u64) -> Vec<DhtRecord> {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .store
            .all_valid(now_ms)
    }

    /// Les k pairs les plus proches d'une clé, connus localement (seeds).
    pub fn closest_local(&self, key: &NodeId, count: usize) -> Vec<NodeInfo> {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .routing
            .closest(key, count)
    }

    // ---- Opérations itératives (asynchrones) ----

    /// Rejoint le réseau via des nœuds d'amorçage, puis se localise soi-même
    /// pour peupler la table (SPEC §2 bootstrap).
    pub async fn bootstrap<R: DhtRpc>(&self, rpc: &R, seeds: Vec<NodeInfo>, now_ms: u64) {
        for s in &seeds {
            self.observe(s.clone(), now_ms);
        }
        let self_id = self.node_id();
        let found = find_node(rpc, self_id, seeds, self.config.pow_bits).await;
        for n in found {
            self.observe(n, now_ms);
        }
    }

    /// Lookup itératif des k plus proches d'une cible.
    pub async fn lookup_node<R: DhtRpc>(&self, rpc: &R, target: NodeId) -> Vec<NodeInfo> {
        let seeds = self.closest_local(&target, DHT_K);
        let found = find_node(rpc, target, seeds, self.config.pow_bits).await;
        let now = 0; // l'appelant réapprend avec l'horloge réelle via observe
        for n in &found {
            self.observe(n.clone(), now);
        }
        found
    }

    /// Récupère une valeur par clé (croisement de chemins disjoints).
    pub async fn get<R: DhtRpc>(&self, rpc: &R, key: [u8; 32], now_ms: u64) -> Option<DhtRecord> {
        // Tentative locale d'abord.
        if let Some(rec) = self
            .inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .store
            .get(&key, now_ms)
        {
            return Some(rec);
        }
        let seeds = self.closest_local(&NodeId(key), DHT_K);
        // Recherche bornée par l'horloge : un record au `timestamp_ms` gonflé
        // ne peut pas détourner la sélection par consensus de chemins (faille
        // de sécurité corrigée dans le lookup).
        match find_value_bounded(
            rpc,
            key,
            seeds,
            self.config.pow_bits,
            self.config.value_paths,
            now_ms,
        )
        .await
        {
            ValueResult::Found(rec) => {
                let _ = self
                    .inner
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .store
                    .put(rec.clone(), now_ms);
                Some(rec)
            }
            ValueResult::NotFound(_) => None,
        }
    }

    /// Publie un record sur les k nœuds les plus proches de sa clé.
    pub async fn put<R: DhtRpc>(&self, rpc: &R, record: DhtRecord, now_ms: u64) -> usize {
        // Stocke localement si l'on est proche.
        let _ = self
            .inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .store
            .put(record.clone(), now_ms);
        let target = NodeId(record.key);
        let closest = self.lookup_node(rpc, target).await;
        let mut stored = 0;
        for peer in closest.into_iter().take(DHT_K) {
            if let Some(DhtBody::StoreOk) = rpc
                .send_rpc(
                    &peer,
                    DhtBody::Store {
                        record: record.clone(),
                    },
                )
                .await
            {
                stored += 1;
            }
        }
        stored
    }

    /// Republie tous les records détenus (à appeler toutes les 60 min).
    pub async fn republish<R: DhtRpc>(&self, rpc: &R, now_ms: u64) -> usize {
        let records = self.records_to_republish(now_ms);
        let mut total = 0;
        for record in records {
            total += self.put(rpc, record, now_ms).await;
        }
        total
    }
}
