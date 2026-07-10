//! Méthodes `groups.*` : gestion de serveur (métadonnées, salons, rôles,
//! modération, épinglage) et messages de groupe (envoi, édition, suppression,
//! réactions, pièces jointes).
//!
//! Les permissions de l'appelant sont vérifiées avant toute émission d'op
//! (l'op est rejouée sur l'état matérialisé du groupe côté cœur) ; une action
//! refusée rend une erreur applicative « refusé : … » explicite.

use accord_core::group::GroupState;
use accord_proto::core_msg::{ChannelKind, GroupOp, GroupOpBody};
use serde_json::{json, Value};

use crate::error::NodeError;
use crate::hex;
use crate::node::Node;

use super::helpers::{
    b64_decode, group_msg_json, group_state_json, param_attachments, param_channel_kind,
    param_id16, param_limit, param_opt_str, param_opt_u16, param_opt_u32, param_pubkey, param_str,
    param_u32, param_u64,
};

/// Identifiant de salon optionnel (catégorie parente d'un salon).
fn param_opt_id16(params: &Value, key: &str) -> Result<Option<[u8; 16]>, NodeError> {
    match param_opt_str(params, key)? {
        None => Ok(None),
        Some(s) => crate::hex::decode::<16>(s)
            .map(Some)
            .ok_or(NodeError::Invalid("identifiant invalide")),
    }
}

/// Catégorie tri-état de `groups.channel.edit` : champ absent = inchangé
/// (`None`), `null` = sortir de toute catégorie (`Some(None)`), hex 32 =
/// déplacer dans cette catégorie (`Some(Some(id))`).
fn param_category(params: &Value, key: &str) -> Result<Option<Option<[u8; 16]>>, NodeError> {
    match params.get(key) {
        None => Ok(None),
        Some(Value::Null) => Ok(Some(None)),
        Some(Value::String(s)) => crate::hex::decode::<16>(s)
            .map(|id| Some(Some(id)))
            .ok_or(NodeError::Invalid("identifiant invalide")),
        Some(_) => Err(NodeError::Invalid("catégorie : chaîne ou null attendu")),
    }
}

/// Overrides de permissions par salon et rôle, forme API
/// `[{ channel_id, role_id, allow, deny }]` (ordre stable du BTreeMap).
fn overrides_json(state: &GroupState) -> Value {
    Value::Array(
        state
            .overrides
            .iter()
            .map(|((channel_id, role_id), o)| {
                json!({
                    "channel_id": hex::encode(channel_id),
                    "role_id": hex::encode(role_id),
                    "allow": o.allow,
                    "deny": o.deny,
                })
            })
            .collect(),
    )
}

/// Entrée du journal d'audit : op décodée en `{ op_id, lamport, wall_ms,
/// author, kind, params }` — `kind` en libellé stable, `params` limités aux
/// champs utiles à une description humaine (jamais le filaire brut).
fn audit_entry_json(op: &GroupOp) -> Value {
    let kind_str = |k: ChannelKind| match k {
        ChannelKind::Text => "text",
        ChannelKind::Voice => "voice",
        ChannelKind::Announcement => "announcement",
    };
    let (kind, params) = match GroupOpBody::decode_body(op.kind, &op.body) {
        Ok(body) => match body {
            GroupOpBody::Create { name } => ("create", json!({ "name": name })),
            GroupOpBody::SetMeta { name, icon } => (
                "set_meta",
                json!({ "name": name, "icon": icon.map(|h| hex::encode(&h)) }),
            ),
            GroupOpBody::AddChannel {
                channel_id,
                name,
                kind,
                ..
            } => (
                "add_channel",
                json!({
                    "channel_id": hex::encode(&channel_id),
                    "name": name,
                    "kind": kind_str(kind),
                }),
            ),
            GroupOpBody::EditChannel {
                channel_id,
                name,
                position,
            } => (
                "edit_channel",
                json!({
                    "channel_id": hex::encode(&channel_id),
                    "name": name,
                    "position": position,
                }),
            ),
            GroupOpBody::DelChannel { channel_id } => (
                "del_channel",
                json!({ "channel_id": hex::encode(&channel_id) }),
            ),
            GroupOpBody::AddCategory {
                category_id, name, ..
            } => (
                "add_category",
                json!({ "category_id": hex::encode(&category_id), "name": name }),
            ),
            GroupOpBody::EditCategory {
                category_id,
                name,
                position,
            } => (
                "edit_category",
                json!({
                    "category_id": hex::encode(&category_id),
                    "name": name,
                    "position": position,
                }),
            ),
            GroupOpBody::DelCategory { category_id } => (
                "del_category",
                json!({ "category_id": hex::encode(&category_id) }),
            ),
            GroupOpBody::SetChannelCategory {
                channel_id,
                category,
            } => (
                "set_channel_category",
                json!({
                    "channel_id": hex::encode(&channel_id),
                    "category": category.map(|c| hex::encode(&c)),
                }),
            ),
            GroupOpBody::AddMember { member, .. } => {
                ("add_member", json!({ "member": hex::encode(&member) }))
            }
            GroupOpBody::Kick { member } => ("kick", json!({ "member": hex::encode(&member) })),
            GroupOpBody::Ban { member } => ("ban", json!({ "member": hex::encode(&member) })),
            GroupOpBody::Unban { member } => ("unban", json!({ "member": hex::encode(&member) })),
            GroupOpBody::AddRole {
                role_id,
                name,
                permissions,
                ..
            } => (
                "add_role",
                json!({
                    "role_id": hex::encode(&role_id),
                    "name": name,
                    "permissions": permissions,
                }),
            ),
            GroupOpBody::EditRole {
                role_id,
                name,
                position,
                permissions,
                ..
            } => (
                "edit_role",
                json!({
                    "role_id": hex::encode(&role_id),
                    "name": name,
                    "position": position,
                    "permissions": permissions,
                }),
            ),
            GroupOpBody::DelRole { role_id } => {
                ("del_role", json!({ "role_id": hex::encode(&role_id) }))
            }
            GroupOpBody::AssignRole { member, role_id } => (
                "assign_role",
                json!({ "member": hex::encode(&member), "role_id": hex::encode(&role_id) }),
            ),
            GroupOpBody::UnassignRole { member, role_id } => (
                "unassign_role",
                json!({ "member": hex::encode(&member), "role_id": hex::encode(&role_id) }),
            ),
            GroupOpBody::SetChannelPerms {
                channel_id,
                role_id,
                allow,
                deny,
            } => (
                "set_channel_perms",
                json!({
                    "channel_id": hex::encode(&channel_id),
                    "role_id": hex::encode(&role_id),
                    "allow": allow,
                    "deny": deny,
                }),
            ),
            GroupOpBody::Pin { channel_id, msg_id } => (
                "pin",
                json!({ "channel_id": hex::encode(&channel_id), "msg_id": hex::encode(&msg_id) }),
            ),
            GroupOpBody::Unpin { channel_id, msg_id } => (
                "unpin",
                json!({ "channel_id": hex::encode(&channel_id), "msg_id": hex::encode(&msg_id) }),
            ),
            GroupOpBody::DeleteMsg { channel_id, msg_id } => (
                "delete_msg",
                json!({ "channel_id": hex::encode(&channel_id), "msg_id": hex::encode(&msg_id) }),
            ),
            GroupOpBody::SetTopic { channel_id, topic } => (
                "set_topic",
                json!({ "channel_id": hex::encode(&channel_id), "topic": topic }),
            ),
            GroupOpBody::InviteCreate { invite_id, .. } => (
                "invite_create",
                json!({ "invite_id": hex::encode(&invite_id) }),
            ),
            GroupOpBody::InviteRevoke { invite_id } => (
                "invite_revoke",
                json!({ "invite_id": hex::encode(&invite_id) }),
            ),
            GroupOpBody::Leave => ("leave", json!({})),
            GroupOpBody::AddEmoji { name, .. } => ("add_emoji", json!({ "name": name })),
            GroupOpBody::DelEmoji { name } => ("del_emoji", json!({ "name": name })),
        },
        Err(_) => ("unknown", json!({})),
    };
    json!({
        "op_id": hex::encode(&op.op_id),
        "lamport": op.lamport,
        "wall_ms": op.wall_ms,
        "author": hex::encode(&op.author),
        "kind": kind,
        "params": params,
    })
}

/// Aiguille les méthodes `groups.*` vers le nœud.
pub(super) fn dispatch(node: &Node, method: &str, params: &Value) -> Result<Value, NodeError> {
    match method {
        "groups.create" => {
            let name = param_str(params, "name")?;
            Ok(json!({ "group_id": node.group_create(name)? }))
        }
        "groups.list" => {
            let ids = node.group_ids()?;
            // Non-lus par groupe : `{ group_id: { channel_id: n } }` (seuls les
            // salons portant au moins un non-lu figurent).
            let mut unread = serde_json::Map::new();
            for id_hex in &ids {
                let gid = crate::hex::decode::<16>(id_hex)
                    .ok_or(NodeError::Invalid("identifiant de groupe invalide"))?;
                let per_channel: serde_json::Map<String, Value> = node
                    .group_unread(&gid)?
                    .into_iter()
                    .map(|(cid, n)| (crate::hex::encode(&cid), json!(n)))
                    .collect();
                if !per_channel.is_empty() {
                    unread.insert(id_hex.clone(), Value::Object(per_channel));
                }
            }
            Ok(json!({ "groups": ids, "unread": Value::Object(unread) }))
        }
        "groups.state" => {
            let gid = param_id16(params, "group_id")?;
            let state = node.group_state(&gid)?;
            let me = node.public_key();
            let mut value = group_state_json(&gid, &state, &me);
            if let Value::Object(map) = &mut value {
                map.insert("overrides".into(), overrides_json(&state));
                // Optional channel scope: my_permissions then folds in the
                // channel overrides (deny > allow).
                if let Some(cid) = param_opt_id16(params, "channel_id")? {
                    map.insert(
                        "my_permissions".into(),
                        json!(state.permissions_in(&me, &cid)),
                    );
                }
            }
            Ok(value)
        }
        "groups.rename" => {
            let gid = param_id16(params, "group_id")?;
            node.group_rename(&gid, param_str(params, "name")?)?;
            Ok(json!({ "ok": true }))
        }
        "groups.set_icon" => {
            let gid = param_id16(params, "group_id")?;
            let mime = param_str(params, "mime")?;
            let data = b64_decode(param_str(params, "data_b64")?)
                .ok_or(NodeError::Invalid("data_b64 : base64 invalide"))?;
            Ok(json!({ "icon": node.group_set_icon(&gid, mime, data)? }))
        }
        "groups.set_topic" => {
            let gid = param_id16(params, "group_id")?;
            let cid = param_id16(params, "channel_id")?;
            node.group_set_topic(&gid, &cid, param_str(params, "topic")?)?;
            Ok(json!({ "ok": true }))
        }
        "groups.channel.add" => {
            let gid = param_id16(params, "group_id")?;
            let name = param_str(params, "name")?;
            let kind = param_channel_kind(params, "kind")?;
            let category = param_opt_id16(params, "category")?;
            Ok(json!({
                "channel_id": node.group_channel_add(&gid, name, kind, category)?
            }))
        }
        "groups.category.add" => {
            let gid = param_id16(params, "group_id")?;
            let name = param_str(params, "name")?;
            let position = param_opt_u16(params, "position")?;
            Ok(json!({
                "category_id": node.group_category_add(&gid, name, position)?
            }))
        }
        "groups.channel.edit" => {
            let gid = param_id16(params, "group_id")?;
            let cid = param_id16(params, "channel_id")?;
            let name = param_opt_str(params, "name")?;
            let position = param_opt_u16(params, "position")?;
            let category = param_category(params, "category")?;
            node.group_channel_edit(&gid, &cid, name, position, category)?;
            Ok(json!({ "ok": true }))
        }
        "groups.channel.perms" => {
            let gid = param_id16(params, "group_id")?;
            let cid = param_id16(params, "channel_id")?;
            let rid = param_id16(params, "role_id")?;
            let allow = param_u32(params, "allow")?;
            let deny = param_u32(params, "deny")?;
            node.group_channel_perms(&gid, &cid, &rid, allow, deny)?;
            Ok(json!({ "ok": true }))
        }
        "groups.category.edit" => {
            let gid = param_id16(params, "group_id")?;
            let cat = param_id16(params, "category_id")?;
            let name = param_opt_str(params, "name")?;
            let position = param_opt_u16(params, "position")?;
            node.group_category_edit(&gid, &cat, name, position)?;
            Ok(json!({ "ok": true }))
        }
        "groups.category.del" => {
            let gid = param_id16(params, "group_id")?;
            let cat = param_id16(params, "category_id")?;
            node.group_category_del(&gid, &cat)?;
            Ok(json!({ "ok": true }))
        }
        "groups.audit" => {
            let gid = param_id16(params, "group_id")?;
            let before = param_opt_id16(params, "before")?;
            let limit = param_u64(params, "limit", 50).clamp(1, 100) as usize;
            let ops = node.group_audit(&gid, before, limit)?;
            Ok(json!({
                "entries": ops.iter().map(audit_entry_json).collect::<Vec<_>>()
            }))
        }
        "groups.channel.del" => {
            let gid = param_id16(params, "group_id")?;
            let cid = param_id16(params, "channel_id")?;
            node.group_channel_del(&gid, &cid)?;
            Ok(json!({ "ok": true }))
        }
        "groups.kick" => {
            let gid = param_id16(params, "group_id")?;
            node.group_kick(&gid, &param_pubkey(params, "pubkey")?)?;
            Ok(json!({ "ok": true }))
        }
        "groups.ban" => {
            let gid = param_id16(params, "group_id")?;
            node.group_ban(&gid, &param_pubkey(params, "pubkey")?)?;
            Ok(json!({ "ok": true }))
        }
        "groups.unban" => {
            let gid = param_id16(params, "group_id")?;
            node.group_unban(&gid, &param_pubkey(params, "pubkey")?)?;
            Ok(json!({ "ok": true }))
        }
        "groups.leave" => {
            let gid = param_id16(params, "group_id")?;
            node.group_leave(&gid)?;
            Ok(json!({ "ok": true }))
        }
        "groups.role.add" => {
            let gid = param_id16(params, "group_id")?;
            let name = param_str(params, "name")?;
            let color = param_u32(params, "color")?;
            let permissions = param_u32(params, "permissions")?;
            let position = param_opt_u16(params, "position")?;
            Ok(json!({
                "role_id": node.group_role_add(&gid, name, color, permissions, position)?
            }))
        }
        "groups.role.edit" => {
            let gid = param_id16(params, "group_id")?;
            let rid = param_id16(params, "role_id")?;
            let name = param_opt_str(params, "name")?;
            let color = param_opt_u32(params, "color")?;
            let position = param_opt_u16(params, "position")?;
            let permissions = param_opt_u32(params, "permissions")?;
            node.group_role_edit(&gid, &rid, name, color, position, permissions)?;
            Ok(json!({ "ok": true }))
        }
        "groups.role.del" => {
            let gid = param_id16(params, "group_id")?;
            let rid = param_id16(params, "role_id")?;
            node.group_role_del(&gid, &rid)?;
            Ok(json!({ "ok": true }))
        }
        "groups.role.assign" => {
            let gid = param_id16(params, "group_id")?;
            let rid = param_id16(params, "role_id")?;
            node.group_role_assign(&gid, &rid, &param_pubkey(params, "pubkey")?)?;
            Ok(json!({ "ok": true }))
        }
        "groups.role.unassign" => {
            let gid = param_id16(params, "group_id")?;
            let rid = param_id16(params, "role_id")?;
            node.group_role_unassign(&gid, &rid, &param_pubkey(params, "pubkey")?)?;
            Ok(json!({ "ok": true }))
        }
        "groups.pin" => {
            let gid = param_id16(params, "group_id")?;
            let cid = param_id16(params, "channel_id")?;
            node.group_pin(&gid, &cid, &param_id16(params, "msg_id")?)?;
            Ok(json!({ "ok": true }))
        }
        "groups.unpin" => {
            let gid = param_id16(params, "group_id")?;
            let cid = param_id16(params, "channel_id")?;
            node.group_unpin(&gid, &cid, &param_id16(params, "msg_id")?)?;
            Ok(json!({ "ok": true }))
        }
        "groups.pins" => {
            let gid = param_id16(params, "group_id")?;
            let cid = param_id16(params, "channel_id")?;
            Ok(json!({ "msg_ids": node.group_pins(&gid, &cid)? }))
        }
        "groups.history" => {
            let gid = param_id16(params, "group_id")?;
            let cid = param_id16(params, "channel_id")?;
            let before = param_u64(params, "before_lamport", u64::MAX);
            let msgs = node.group_history(&gid, &cid, before, param_limit(params))?;
            let messages = msgs
                .iter()
                .map(|m| {
                    Ok(group_msg_json(
                        m,
                        &node.reactions_of(&m.msg_id)?,
                        &node.attachments_of(&m.msg_id)?,
                    ))
                })
                .collect::<Result<Vec<_>, NodeError>>()?;
            Ok(json!({ "messages": messages }))
        }
        "groups.send" => {
            let gid = param_id16(params, "group_id")?;
            let cid = param_id16(params, "channel_id")?;
            let text = param_str(params, "text")?;
            let reply_to = params
                .get("reply_to")
                .and_then(Value::as_str)
                .and_then(crate::hex::decode::<16>);
            let attachments = param_attachments(params)?;
            Ok(json!({
                "msg_id": node.group_send_with_attachments(&gid, &cid, text, reply_to, attachments)?
            }))
        }
        "groups.edit" => {
            let gid = param_id16(params, "group_id")?;
            let cid = param_id16(params, "channel_id")?;
            let mid = param_id16(params, "msg_id")?;
            node.group_edit_msg(&gid, &cid, &mid, param_str(params, "text")?)?;
            Ok(json!({ "ok": true }))
        }
        "groups.delete" => {
            let gid = param_id16(params, "group_id")?;
            let cid = param_id16(params, "channel_id")?;
            let mid = param_id16(params, "msg_id")?;
            node.group_delete_msg(&gid, &cid, &mid)?;
            Ok(json!({ "ok": true }))
        }
        "groups.react" => {
            let gid = param_id16(params, "group_id")?;
            let cid = param_id16(params, "channel_id")?;
            let mid = param_id16(params, "msg_id")?;
            let emoji = param_str(params, "emoji")?;
            let add = params.get("add").and_then(Value::as_bool).unwrap_or(true);
            node.group_react(&gid, &cid, &mid, emoji, add)?;
            Ok(json!({ "ok": true }))
        }
        "groups.invite" => {
            let gid = param_id16(params, "group_id")?;
            let member = param_pubkey(params, "pubkey")?;
            node.group_invite(&gid, &member)?;
            Ok(json!({ "ok": true }))
        }
        "groups.emoji.add" => {
            let gid = param_id16(params, "group_id")?;
            let name = param_str(params, "name")?;
            let mime = param_str(params, "mime")?;
            let data = b64_decode(param_str(params, "data_b64")?)
                .ok_or(NodeError::Invalid("data_b64 : base64 invalide"))?;
            Ok(json!({
                "merkle_root": node.group_emoji_add(&gid, name, mime, data)?
            }))
        }
        "groups.emoji.del" => {
            let gid = param_id16(params, "group_id")?;
            node.group_emoji_del(&gid, param_str(params, "name")?)?;
            Ok(json!({ "ok": true }))
        }
        "groups.typing" => {
            let gid = param_id16(params, "group_id")?;
            let cid = param_id16(params, "channel_id")?;
            node.group_typing(&gid, &cid)?;
            Ok(json!({ "ok": true }))
        }
        "groups.mark_read" => {
            let gid = param_id16(params, "group_id")?;
            let cid = param_id16(params, "channel_id")?;
            let lamport = param_u64(params, "lamport", 0);
            node.group_mark_read(&gid, &cid, lamport)?;
            Ok(json!({ "ok": true }))
        }
        _ => Err(NodeError::Invalid("méthode inconnue")),
    }
}
