//! Méthodes `network.*` (B2) et `diagnostics.*` (Lot D) : port P2P stable,
//! pairs d'amorçage, statut réseau, compteurs locaux et auto-test. Le
//! pilotage passe par le [`NetworkControl`] branché sur le nœud (implémenté
//! par le runtime réseau), sans coupler le service au runtime.

use serde_json::{json, Value};

use crate::error::NodeError;
use crate::node::network::parse_peer_addr;

use super::helpers::param_str;
use super::NodeService;

impl NodeService {
    /// Aiguille les méthodes `network.*` et `diagnostics.*` vers le contrôle
    /// réseau du nœud.
    pub(super) async fn call_network(
        &self,
        method: &str,
        params: &Value,
    ) -> Result<Value, NodeError> {
        let ctrl = self
            .node
            .network_control()
            .ok_or(NodeError::NotFound("sous-système réseau indisponible"))?;
        if method == "network.peers" {
            let links = ctrl.peer_links();
            return Ok(serde_json::to_value(links).unwrap_or_else(|_| json!([])));
        }
        // Diagnostics (Lot D) : compteurs locaux photographiés, auto-test
        // borné. Contrats JSON documentés dans docs/API.md — sérialisation
        // d'objets simples, repli défensif sur objet vide.
        if method == "diagnostics.counters" {
            let snapshot = ctrl.counters();
            return Ok(serde_json::to_value(snapshot).unwrap_or_else(|_| json!({})));
        }
        if method == "diagnostics.selftest" {
            let report = ctrl.self_test().await;
            return Ok(serde_json::to_value(report).unwrap_or_else(|_| json!({})));
        }
        let status = match method {
            "network.status" => ctrl.status(),
            "network.add_peer" => {
                let addr = parse_peer_addr(param_str(params, "addr")?)?;
                ctrl.add_peer(addr).await?
            }
            "network.remove_peer" => {
                let addr = parse_peer_addr(param_str(params, "addr")?)?;
                ctrl.remove_peer(addr).await?
            }
            _ => return Err(NodeError::Invalid("méthode réseau inconnue")),
        };
        // Le statut réseau se sérialise en objet JSON simple (jamais d'échec en
        // pratique) ; repli défensif sur objet vide.
        Ok(serde_json::to_value(status).unwrap_or_else(|_| json!({})))
    }
}
