//! Méthodes `security.*` (jalon 2, lots 2.C et 2.D) : état du chiffrement et
//! réglage avancé d'exigence post-quantique.

use serde_json::{json, Value};

use crate::error::NodeError;
use crate::node::Node;

/// Route les méthodes `security.*` vers le nœud.
pub(super) fn dispatch(node: &Node, method: &str, params: &Value) -> Result<Value, NodeError> {
    match method {
        "security.state" => {
            let state = node.security_state()?;
            Ok(serde_json::to_value(state).unwrap_or_else(|_| json!({})))
        }
        "security.set_require_hybrid" => {
            // Paramètre obligatoire et strictement booléen : un appel sans
            // `require` ne doit pas se lire comme « lève l'exigence ». Se
            // tromper de sens ici baisserait silencieusement la protection.
            let require = params
                .get("require")
                .and_then(Value::as_bool)
                .ok_or(NodeError::Invalid("require booléen requis"))?;
            let state = node.set_require_hybrid(require)?;
            Ok(serde_json::to_value(state).unwrap_or_else(|_| json!({})))
        }
        _ => Err(NodeError::Invalid("méthode inconnue")),
    }
}
