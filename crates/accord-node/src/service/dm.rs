//! Méthodes `dm.*` : messagerie directe (envoi, historique, édition,
//! suppression, réactions).

use serde_json::{json, Value};

use crate::error::NodeError;
use crate::hex;
use crate::node::Node;

use super::helpers::{
    dm_json, param_attachments, param_id16, param_limit, param_peer, param_str, param_u64,
};

/// Aiguille les méthodes `dm.*` vers le nœud.
pub(super) fn dispatch(node: &Node, method: &str, params: &Value) -> Result<Value, NodeError> {
    match method {
        "dm.send" => {
            let peer = param_peer(params)?;
            let text = param_str(params, "text")?;
            let reply_to = params
                .get("reply_to")
                .and_then(Value::as_str)
                .and_then(hex::decode::<16>);
            let attachments = param_attachments(params)?;
            let msg_id = node.dm_send_with_attachments(&peer, text, reply_to, attachments)?;
            Ok(json!({ "msg_id": msg_id }))
        }
        "dm.history" => {
            let peer = param_peer(params)?;
            let before = param_u64(params, "before_lamport", u64::MAX);
            let msgs = node.dm_history(&peer, before, param_limit(params))?;
            let messages = msgs
                .iter()
                .map(|m| {
                    Ok(dm_json(
                        m,
                        &node.reactions_of(&m.msg_id)?,
                        &node.attachments_of(&m.msg_id)?,
                    ))
                })
                .collect::<Result<Vec<_>, NodeError>>()?;
            // Peer's read position (read receipts), `null` if unknown.
            Ok(json!({
                "messages": messages,
                "peer_read_lamport": node.dm_peer_read_lamport(&peer)?,
            }))
        }
        "dm.edit" => {
            let peer = param_peer(params)?;
            let msg_id = param_id16(params, "msg_id")?;
            let text = param_str(params, "text")?;
            node.dm_edit(&peer, &msg_id, text)?;
            Ok(json!({ "ok": true }))
        }
        "dm.delete" => {
            let peer = param_peer(params)?;
            let msg_id = param_id16(params, "msg_id")?;
            node.dm_delete(&peer, &msg_id)?;
            Ok(json!({ "ok": true }))
        }
        "dm.react" => {
            let peer = param_peer(params)?;
            let msg_id = param_id16(params, "msg_id")?;
            let emoji = param_str(params, "emoji")?;
            let remove = params
                .get("remove")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            node.dm_react(&peer, &msg_id, emoji, !remove)?;
            Ok(json!({ "ok": true }))
        }
        "dm.typing" => {
            node.dm_typing(&param_peer(params)?)?;
            Ok(json!({ "ok": true }))
        }
        "dm.mark_read" => {
            let peer = param_peer(params)?;
            let lamport = param_u64(params, "lamport", 0);
            node.dm_mark_read(&peer, lamport)?;
            Ok(json!({ "ok": true }))
        }
        "dm.set_read_receipts" => {
            let enabled = params
                .get("enabled")
                .and_then(Value::as_bool)
                .ok_or(NodeError::Invalid("enabled booléen requis"))?;
            node.set_read_receipts(enabled)?;
            Ok(json!({ "ok": true }))
        }
        "dm.get_read_receipts" => Ok(json!({ "enabled": node.read_receipts_enabled()? })),
        _ => Err(NodeError::Invalid("méthode inconnue")),
    }
}
