//! Méthodes `dm.*` : messagerie directe (envoi, historique, fenêtre autour d'un
//! message, épingles, éditions, suppression, réactions, nouvelle tentative).

use std::collections::{BTreeSet, HashMap};

use accord_core::db::DmRecord;
use serde_json::{json, Value};

use crate::error::NodeError;
use crate::hex;
use crate::node::Node;

use super::helpers::{
    dm_json, param_attachments, param_id16, param_limit, param_peer, param_str, param_u64,
};

/// Sérialise une tranche d'historique direct en annotant chaque message de son
/// épingle et de son état de livraison (calculés une fois par appel).
fn dm_messages_json(
    node: &Node,
    msgs: &[DmRecord],
    pinned: &BTreeSet<[u8; 16]>,
    outbox: &HashMap<[u8; 16], (u32, bool)>,
) -> Result<Vec<Value>, NodeError> {
    msgs.iter()
        .map(|m| {
            Ok(dm_json(
                m,
                &node.reactions_of(&m.msg_id)?,
                &node.attachments_of(&m.msg_id)?,
                pinned.contains(&m.msg_id),
                node.dm_delivery(m, outbox),
                node.msg_mentions_me(&m.msg_id)?,
            ))
        })
        .collect()
}

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
            let pinned = node.dm_pinned_set(&peer)?;
            let outbox = node.dm_outbox_states(&peer)?;
            let messages = dm_messages_json(node, &msgs, &pinned, &outbox)?;
            // Peer's read position (read receipts), `null` if unknown.
            Ok(json!({
                "messages": messages,
                "peer_read_lamport": node.dm_peer_read_lamport(&peer)?,
            }))
        }
        "dm.history_around" => {
            let peer = param_peer(params)?;
            let msg_id = param_id16(params, "msg_id")?;
            let (msgs, found) = node.dm_history_around(&peer, &msg_id, param_limit(params))?;
            let pinned = node.dm_pinned_set(&peer)?;
            let outbox = node.dm_outbox_states(&peer)?;
            let messages = dm_messages_json(node, &msgs, &pinned, &outbox)?;
            Ok(json!({
                "messages": messages,
                "found": found,
                "peer_read_lamport": node.dm_peer_read_lamport(&peer)?,
            }))
        }
        "dm.pin" => {
            let peer = param_peer(params)?;
            node.dm_pin(&peer, &param_id16(params, "msg_id")?)?;
            Ok(json!({ "ok": true }))
        }
        "dm.unpin" => {
            let peer = param_peer(params)?;
            node.dm_unpin(&peer, &param_id16(params, "msg_id")?)?;
            Ok(json!({ "ok": true }))
        }
        "dm.pins" => {
            let peer = param_peer(params)?;
            Ok(json!({ "msg_ids": node.dm_pins(&peer)? }))
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
        "dm.retry" => {
            let peer = param_peer(params)?;
            node.dm_retry(&peer, &param_id16(params, "msg_id")?)?;
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
