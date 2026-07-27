//! Méthodes `portable.*` (jalon 7, §19.4.2) : export lisible des conversations
//! et réimport de ce même fichier.
//!
//! 🔒 Le document produit n'est **pas chiffré**. C'est ce qu'on lui demande —
//! un utilisateur qui ne peut pas partir avec ses données est captif — et c'est
//! aussi ce qui en fait le fichier le plus dangereux que l'application sache
//! écrire. L'interface le dit à l'utilisateur au moment de l'export ; le
//! document le rappelle lui-même dans son en-tête, pour rester explicite une
//! fois séparé de l'application.

use serde_json::{json, Value};

use crate::error::NodeError;
use crate::node::Node;

/// Route les méthodes `portable.*`.
pub(super) fn dispatch(node: &Node, method: &str, params: &Value) -> Result<Value, NodeError> {
    match method {
        "portable.export" => node.export_document(),
        "portable.import" => {
            let doc = params
                .get("document")
                .ok_or(NodeError::Invalid("document manquant"))?;
            let bilan = node.import_document(doc)?;
            Ok(json!({
                "inserted": bilan.inserted,
                "skipped": bilan.skipped,
                "rejected": bilan.rejected,
            }))
        }
        _ => Err(NodeError::Invalid("méthode inconnue")),
    }
}
