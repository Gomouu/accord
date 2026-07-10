//! Méthodes `mentions.*` : boîte de mentions locale (liste paginée,
//! marquage lu). La détection est passive à l'ingestion ; ces méthodes ne font
//! que lire et marquer l'état local (aucun effet réseau).

use accord_core::db::{MentionEntry, MentionScope};
use serde_json::{json, Value};

use crate::error::NodeError;
use crate::hex;
use crate::node::Node;

use super::helpers::param_limit;

/// Sérialise une entrée de mention (référence de conversation typée).
fn mention_entry_json(e: &MentionEntry) -> Value {
    let conversation = match e.scope {
        MentionScope::Dm => json!({ "kind": "dm", "peer": hex::encode(&e.conv_a) }),
        MentionScope::Group => json!({
            "kind": "group",
            "group_id": hex::encode(&e.conv_a),
            "channel_id": e.conv_b.as_ref().map(|c| hex::encode(c)),
        }),
    };
    json!({
        "msg_id": hex::encode(&e.msg_id),
        "conversation": conversation,
        "author": hex::encode(&e.author),
        "ts_ms": e.ts_ms,
        "lamport": e.lamport,
        "snippet": e.snippet,
        "read": e.read,
    })
}

/// Aiguille les méthodes `mentions.*` vers le nœud.
pub(super) fn dispatch(node: &Node, method: &str, params: &Value) -> Result<Value, NodeError> {
    match method {
        "mentions.inbox" => {
            // `before` : horloge murale (ms) de pagination ; absent = présent.
            let before = params.get("before").and_then(Value::as_u64);
            let entries = node.mention_inbox(before, param_limit(params))?;
            Ok(json!({
                "entries": entries.iter().map(mention_entry_json).collect::<Vec<_>>()
            }))
        }
        "mentions.mark_read" => {
            // `msg_ids` optionnel : liste hex 16 à marquer ; absent = tout.
            let ids = match params.get("msg_ids") {
                None | Some(Value::Null) => None,
                Some(Value::Array(a)) => {
                    let mut v = Vec::with_capacity(a.len());
                    for item in a {
                        let id = item
                            .as_str()
                            .and_then(hex::decode::<16>)
                            .ok_or(NodeError::Invalid("msg_id invalide"))?;
                        v.push(id);
                    }
                    Some(v)
                }
                Some(_) => return Err(NodeError::Invalid("msg_ids : liste attendue")),
            };
            let marked = node.mentions_mark_read(ids.as_deref())?;
            Ok(json!({ "ok": true, "marked": marked }))
        }
        _ => Err(NodeError::Invalid("méthode inconnue")),
    }
}
