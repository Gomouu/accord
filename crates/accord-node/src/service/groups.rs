//! Méthodes `groups.*` : gestion de serveur (métadonnées, salons, rôles,
//! modération, épinglage) et messages de groupe (envoi, édition, suppression,
//! réactions, pièces jointes).
//!
//! Les permissions de l'appelant sont vérifiées avant toute émission d'op
//! (l'op est rejouée sur l'état matérialisé du groupe côté cœur) ; une action
//! refusée rend une erreur applicative « refusé : … » explicite.

use accord_core::db::IncomingInvite;
use accord_core::group::GroupState;
use accord_proto::core_msg::{ChannelKind, GroupOp, GroupOpBody};
use serde_json::{json, Value};

use crate::error::NodeError;
use crate::hex;
use crate::node::Node;

use super::helpers::{
    b64_decode, group_msg_json, group_state_json, param_attachments, param_channel_kind,
    param_id16, param_limit, param_opt_color, param_opt_str, param_opt_u16, param_opt_u32,
    param_pubkey, param_str, param_u32, param_u64, param_u8,
};

/// Sérialise une tranche d'historique de groupe. Les réactions, pièces
/// jointes et mentions sont chargées en un LOT (trois requêtes par page, au
/// lieu de trois par message).
fn group_messages_json(
    node: &Node,
    msgs: &[accord_core::db::GroupMsgRecord],
) -> Result<Vec<Value>, NodeError> {
    let ids: Vec<[u8; 16]> = msgs.iter().map(|m| m.msg_id).collect();
    let annotations = node.annotations_of(&ids)?;
    msgs.iter()
        .map(|m| {
            Ok(group_msg_json(
                m,
                annotations.reactions_of(&m.msg_id),
                annotations.attachments_of(&m.msg_id),
                annotations.mentions_me(&m.msg_id),
            ))
        })
        .collect()
}

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
        ChannelKind::Forum => "forum",
    };
    let (kind, params) = match GroupOpBody::decode_body(op.kind, &op.body) {
        Ok(body) => match body {
            GroupOpBody::Create { name, dm } => (
                "create",
                json!({ "name": name, "dm": dm.unwrap_or(false) }),
            ),
            GroupOpBody::SetMeta {
                name,
                icon,
                banner_color,
            } => (
                "set_meta",
                json!({
                    "name": name,
                    "icon": icon.map(|h| hex::encode(&h)),
                    "banner_color": banner_color,
                }),
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
            GroupOpBody::AddSound { name, .. } => ("add_sound", json!({ "name": name })),
            GroupOpBody::DelSound { name } => ("del_sound", json!({ "name": name })),
            GroupOpBody::TimeoutMember { member, until_ms } => (
                "timeout",
                json!({ "member": hex::encode(&member), "until_ms": until_ms }),
            ),
            GroupOpBody::SetNickname { member, name } => (
                "set_nickname",
                json!({ "member": hex::encode(&member), "name": name }),
            ),
            GroupOpBody::VoiceModerate {
                member,
                mute,
                deafen,
            } => (
                "voice_moderate",
                json!({
                    "member": hex::encode(&member),
                    "mute": mute,
                    "deafen": deafen,
                }),
            ),
            GroupOpBody::EventCreate {
                event_id, title, ..
            } => (
                "event_create",
                json!({ "event_id": hex::encode(&event_id), "title": title }),
            ),
            GroupOpBody::EventEdit {
                event_id, title, ..
            } => (
                "event_edit",
                json!({ "event_id": hex::encode(&event_id), "title": title }),
            ),
            GroupOpBody::EventDelete { event_id } => (
                "event_delete",
                json!({ "event_id": hex::encode(&event_id) }),
            ),
            GroupOpBody::EventRsvp {
                event_id,
                interested,
            } => (
                "event_rsvp",
                json!({ "event_id": hex::encode(&event_id), "interested": interested }),
            ),
            GroupOpBody::StickerAdd { name, .. } => ("sticker_add", json!({ "name": name })),
            GroupOpBody::StickerRemove { name } => ("sticker_remove", json!({ "name": name })),
            GroupOpBody::SetMemberAvatar { avatar } => (
                "set_member_avatar",
                json!({ "avatar": avatar.map(|h| hex::encode(&h)) }),
            ),
            GroupOpBody::SetBanner { banner } => (
                "set_banner",
                json!({ "banner": banner.map(|h| hex::encode(&h)) }),
            ),
            GroupOpBody::PollCreate {
                poll_id,
                channel_id,
                msg_id,
            } => (
                "poll_create",
                json!({
                    "poll_id": hex::encode(&poll_id),
                    "channel_id": hex::encode(&channel_id),
                    "msg_id": hex::encode(&msg_id),
                }),
            ),
            GroupOpBody::PollVote {
                poll_id,
                option_index,
            } => (
                "poll_vote",
                json!({ "poll_id": hex::encode(&poll_id), "option_index": option_index }),
            ),
            GroupOpBody::PollClose { poll_id } => {
                ("poll_close", json!({ "poll_id": hex::encode(&poll_id) }))
            }
            GroupOpBody::PollDelete { poll_id } => {
                ("poll_delete", json!({ "poll_id": hex::encode(&poll_id) }))
            }
            GroupOpBody::SetAutoModWords { words } => {
                ("automod_set", json!({ "word_count": words.len() }))
            }
            GroupOpBody::SetChannelSlowmode {
                channel_id,
                seconds,
            } => (
                "set_channel_slowmode",
                json!({ "channel_id": hex::encode(&channel_id), "seconds": seconds }),
            ),
            GroupOpBody::CreateThread {
                thread_id,
                parent_channel,
                root_msg,
                name,
            } => (
                "create_thread",
                json!({
                    "thread_id": hex::encode(&thread_id),
                    "parent_channel": hex::encode(&parent_channel),
                    "root_msg": hex::encode(&root_msg),
                    "name": name,
                }),
            ),
            GroupOpBody::SetThreadArchived {
                thread_id,
                archived,
            } => (
                "set_thread_archived",
                json!({ "thread_id": hex::encode(&thread_id), "archived": archived }),
            ),
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

/// Forme API d'une invitation entrante en attente (`groups.invites_list`).
fn incoming_invite_json(inv: &IncomingInvite) -> Value {
    json!({
        "group_id": hex::encode(&inv.group_id),
        "invite_id": hex::encode(&inv.invite_id),
        "group_name": inv.group_name,
        "inviter": hex::encode(&inv.inviter),
        "expires_ms": inv.expires_ms,
        "received_ms": inv.received_ms,
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
            // Purge des appartenances `Accepted` fantômes (lien d'invitation
            // mort/révoqué jamais abouti) avant de lister — balayage léger à la
            // lecture, sans boucle de maintenance dédiée (MEDIUM).
            node.purge_stale_pending_redeems(crate::node::now_ms())?;
            let ids = node.group_ids()?;
            // Non-lus par groupe : `{ group_id: { channel_id: n } }` (seuls les
            // salons portant au moins un non-lu figurent). Mentions non lues
            // par groupe : `{ group_id: n }` (seuls les groupes en portant).
            let mut unread = serde_json::Map::new();
            let mut mentions = serde_json::Map::new();
            let mut channel_mentions = serde_json::Map::new();
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
                let mention_count = node.group_mention_count(&gid)?;
                if mention_count > 0 {
                    mentions.insert(id_hex.clone(), json!(mention_count));
                }
                // Mentions non lues par salon : `{ group_id: { channel_id: n } }`
                // (miroir de `unread`, seuls les salons en portant figurent).
                let per_channel_mentions: serde_json::Map<String, Value> = node
                    .group_channel_mentions(&gid)?
                    .into_iter()
                    .map(|(cid, n)| (crate::hex::encode(&cid), json!(n)))
                    .collect();
                if !per_channel_mentions.is_empty() {
                    channel_mentions.insert(id_hex.clone(), Value::Object(per_channel_mentions));
                }
            }
            Ok(json!({
                "groups": ids,
                "unread": Value::Object(unread),
                "mentions": Value::Object(mentions),
                "channel_mentions": Value::Object(channel_mentions),
            }))
        }
        "groups.state" => {
            let gid = param_id16(params, "group_id")?;
            let state = node.group_state(&gid)?;
            let me = node.public_key();
            let mut value = group_state_json(&gid, &state, &me);
            if let Value::Object(map) = &mut value {
                map.insert("overrides".into(), overrides_json(&state));
                // Our local read marks per channel (`{ channel_id: lamport }`)
                // for the UI "new messages" divider — captured on open before
                // the client advances the mark.
                let read_marks: serde_json::Map<String, Value> = node
                    .group_read_marks(&gid)?
                    .into_iter()
                    .map(|(cid, mark)| (crate::hex::encode(&cid), json!(mark)))
                    .collect();
                map.insert("read_marks".into(), Value::Object(read_marks));
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
        "groups.channel.slowmode" => {
            let gid = param_id16(params, "group_id")?;
            let cid = param_id16(params, "channel_id")?;
            let seconds = param_u32(params, "seconds")?;
            node.group_channel_slowmode(&gid, &cid, seconds)?;
            Ok(json!({ "ok": true }))
        }
        "groups.thread.create" => {
            let gid = param_id16(params, "group_id")?;
            let parent = param_id16(params, "parent_channel")?;
            let root = param_id16(params, "root_msg")?;
            let name = param_str(params, "name")?;
            Ok(json!({
                "thread_id": node.group_thread_create(&gid, &parent, &root, name)?
            }))
        }
        "groups.thread.archive" => {
            let gid = param_id16(params, "group_id")?;
            let tid = param_id16(params, "thread_id")?;
            let archived = params
                .get("archived")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            node.group_thread_archive(&gid, &tid, archived)?;
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
        // Vérification à l'entrée (§9.4) : réglage local du créateur des
        // invitations, et file des demandes prouvées en attente.
        "groups.entry_check.get" => {
            let gid = param_id16(params, "group_id")?;
            Ok(json!({ "enabled": node.group_entry_check(&gid)? }))
        }
        "groups.entry_check.set" => {
            let gid = param_id16(params, "group_id")?;
            let enabled = params
                .get("enabled")
                .and_then(|v| v.as_bool())
                .ok_or(NodeError::Invalid("enabled attendu"))?;
            node.group_set_entry_check(&gid, enabled)?;
            Ok(json!({ "ok": true }))
        }
        "groups.pending.list" => {
            let gid = param_id16(params, "group_id")?;
            let attente = node.group_pending_members(&gid)?;
            Ok(json!({
                "entries": attente
                    .iter()
                    .map(|p| json!({
                        "member": hex::encode(&p.member),
                        "invite_id": hex::encode(&p.invite_id),
                        "at_ms": p.at_ms,
                    }))
                    .collect::<Vec<_>>()
            }))
        }
        "groups.pending.approve" => {
            let gid = param_id16(params, "group_id")?;
            node.group_approve_member(&gid, &param_pubkey(params, "pubkey")?)?;
            Ok(json!({ "ok": true }))
        }
        "groups.pending.refuse" => {
            let gid = param_id16(params, "group_id")?;
            node.group_refuse_member(&gid, &param_pubkey(params, "pubkey")?)?;
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
        "groups.timeout" => {
            let gid = param_id16(params, "group_id")?;
            let member = param_pubkey(params, "pubkey")?;
            let until_ms = param_u64(params, "until_ms", 0);
            node.group_timeout(&gid, &member, until_ms)?;
            Ok(json!({ "ok": true }))
        }
        "groups.timeout_clear" => {
            let gid = param_id16(params, "group_id")?;
            let member = param_pubkey(params, "pubkey")?;
            node.group_timeout_clear(&gid, &member)?;
            Ok(json!({ "ok": true }))
        }
        "groups.voice_moderate" => {
            let gid = param_id16(params, "group_id")?;
            let member = param_pubkey(params, "pubkey")?;
            // Absent = faux : `{ mute: false, deafen: false }` lève la
            // modération vocale du membre.
            let mute = params.get("mute").and_then(Value::as_bool).unwrap_or(false);
            let deafen = params
                .get("deafen")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            node.group_voice_moderate(&gid, &member, mute, deafen)?;
            Ok(json!({ "ok": true }))
        }
        "groups.set_nickname" => {
            let gid = param_id16(params, "group_id")?;
            // `member` absent = self (the local identity).
            let member = match param_opt_str(params, "member")? {
                Some(s) => crate::hex::decode::<32>(s)
                    .ok_or(NodeError::Invalid("clé publique invalide"))?,
                None => node.public_key(),
            };
            let name = param_opt_str(params, "name")?.unwrap_or("");
            node.group_set_nickname(&gid, &member, name)?;
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
            let messages = group_messages_json(node, &msgs)?;
            Ok(json!({ "messages": messages }))
        }
        "groups.history_around" => {
            let gid = param_id16(params, "group_id")?;
            let cid = param_id16(params, "channel_id")?;
            let mid = param_id16(params, "msg_id")?;
            let (msgs, found) = node.group_history_around(&gid, &cid, &mid, param_limit(params))?;
            let messages = group_messages_json(node, &msgs)?;
            Ok(json!({ "messages": messages, "found": found }))
        }
        "groups.send" => {
            let gid = param_id16(params, "group_id")?;
            let cid = param_id16(params, "channel_id")?;
            // A `sticker` param sends a registered server sticker instead of
            // a text message: `text`/`reply_to`/`attachments` are ignored in
            // that case (D-047). Absent `sticker` preserves prior behavior.
            if let Some(sticker_name) = param_opt_str(params, "sticker")? {
                return Ok(json!({
                    "msg_id": node.group_send_sticker(&gid, &cid, sticker_name)?
                }));
            }
            // A `poll` param (`{ question, options: [...] }`) posts a poll
            // instead of a text message (D-048): `text`/`reply_to`/
            // `attachments`/`sticker` are ignored in that case. Absent
            // `poll` preserves prior behavior.
            if let Some(poll) = params.get("poll") {
                let question = poll
                    .get("question")
                    .and_then(Value::as_str)
                    .ok_or(NodeError::Invalid("poll.question manquant"))?;
                let options: Vec<String> = poll
                    .get("options")
                    .and_then(Value::as_array)
                    .ok_or(NodeError::Invalid("poll.options manquant"))?
                    .iter()
                    .map(|v| {
                        v.as_str()
                            .map(str::to_string)
                            .ok_or(NodeError::Invalid("poll.options : chaînes attendues"))
                    })
                    .collect::<Result<_, _>>()?;
                let (msg_id, poll_id) = node.group_send_poll(&gid, &cid, question, options)?;
                return Ok(json!({ "msg_id": msg_id, "poll_id": poll_id }));
            }
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
        "groups.schedule" => {
            // Deferred local send (F1): stored now, routed through the normal
            // send path when due. Zero wire byte at schedule time.
            let gid = param_id16(params, "group_id")?;
            let cid = param_id16(params, "channel_id")?;
            let body = param_str(params, "body")?;
            let fire_at = super::schedule::param_fire_at_ms(params)?;
            Ok(json!({ "id": node.schedule_group(&gid, &cid, body, fire_at)? }))
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
        "groups.purge" => {
            let gid = param_id16(params, "group_id")?;
            let cid = param_id16(params, "channel_id")?;
            let raw = params
                .get("msg_ids")
                .and_then(Value::as_array)
                .ok_or(NodeError::Invalid("msg_ids : tableau attendu"))?;
            let mut ids = Vec::with_capacity(raw.len());
            for v in raw {
                let hex = v
                    .as_str()
                    .ok_or(NodeError::Invalid("msg_ids : identifiants hexadécimaux"))?;
                ids.push(hex::decode::<16>(hex).ok_or(NodeError::Invalid("identifiant invalide"))?);
            }
            let deleted = node.group_purge(&gid, &cid, &ids)?;
            Ok(json!({ "deleted": deleted }))
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
        // Conservée pour compatibilité (l'UI existante l'appelle) mais ne
        // force plus rien : route désormais vers `invite_create`, qui exige
        // un consentement explicite côté invité avant toute matérialisation
        // (D-045). Aucun chemin de force-join n'est plus exposé.
        "groups.invite" => {
            let gid = param_id16(params, "group_id")?;
            let member = param_pubkey(params, "pubkey")?;
            Ok(json!({
                "ok": true,
                "invite_id": node.group_invite_create(&gid, &member)?
            }))
        }
        "groups.invite_create" => {
            let gid = param_id16(params, "group_id")?;
            let member = param_pubkey(params, "pubkey")?;
            Ok(json!({ "invite_id": node.group_invite_create(&gid, &member)? }))
        }
        "groups.invite_link_create" => {
            let gid = param_id16(params, "group_id")?;
            // `max_uses` : 0 (défaut) = illimité ; borné au filaire (u32).
            let max_uses = u32::try_from(param_u64(params, "max_uses", 0)).unwrap_or(u32::MAX);
            // `expires_h` : absent = TTL par défaut, 0 = n'expire jamais.
            let expires_h = params.get("expires_h").and_then(Value::as_u64);
            Ok(json!({
                "code": node.group_invite_link_create(&gid, max_uses, expires_h)?
            }))
        }
        "groups.invite_link_redeem" => {
            let code = param_str(params, "code")?;
            let (group_id, group_name) = node.group_invite_link_redeem(code)?;
            Ok(json!({
                "ok": true,
                "group_id": hex::encode(&group_id),
                "group_name": group_name,
            }))
        }
        // Décode-seul (aucun effet de bord) : prévisualisation riche d'un lien
        // avant adhésion. Les racines/couleur absentes rendent `null`.
        "groups.invite_link_info" => {
            let link = node.group_invite_link_info(param_str(params, "link")?)?;
            Ok(json!({
                "group_id": hex::encode(&link.group_id),
                "invite_id": hex::encode(&link.invite_id),
                "inviter": hex::encode(&link.inviter),
                "group_name": link.group_name,
                "icon": link.icon_root.map(|h| hex::encode(&h)),
                "banner": link.banner_root.map(|h| hex::encode(&h)),
                "banner_color": link.banner_color,
            }))
        }
        "groups.invites_list" => Ok(json!({
            "invites": node
                .group_invites_list()?
                .iter()
                .map(incoming_invite_json)
                .collect::<Vec<_>>()
        })),
        "groups.invite_accept" => {
            let gid = param_id16(params, "group_id")?;
            let iid = param_id16(params, "invite_id")?;
            node.group_invite_accept(&gid, &iid)?;
            Ok(json!({ "ok": true }))
        }
        "groups.invite_decline" => {
            let gid = param_id16(params, "group_id")?;
            let iid = param_id16(params, "invite_id")?;
            node.group_invite_decline(&gid, &iid)?;
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
        "groups.sounds.add" => {
            let gid = param_id16(params, "group_id")?;
            let name = param_str(params, "name")?;
            let mime = param_str(params, "mime")?;
            let data = b64_decode(param_str(params, "data_b64")?)
                .ok_or(NodeError::Invalid("data_b64 : base64 invalide"))?;
            Ok(json!({
                "merkle_root": node.group_sound_add(&gid, name, mime, data)?
            }))
        }
        "groups.sounds.del" => {
            let gid = param_id16(params, "group_id")?;
            node.group_sound_del(&gid, param_str(params, "name")?)?;
            Ok(json!({ "ok": true }))
        }
        "groups.soundboard.play" => {
            let gid = param_id16(params, "group_id")?;
            let cid = param_id16(params, "channel_id")?;
            node.group_soundboard_play(&gid, &cid, param_str(params, "sound_name")?)?;
            Ok(json!({ "ok": true }))
        }
        "groups.stickers.add" => {
            let gid = param_id16(params, "group_id")?;
            let name = param_str(params, "name")?;
            let mime = param_str(params, "mime")?;
            let data = b64_decode(param_str(params, "data_b64")?)
                .ok_or(NodeError::Invalid("data_b64 : base64 invalide"))?;
            Ok(json!({
                "merkle_root": node.group_sticker_add(&gid, name, mime, data)?
            }))
        }
        "groups.stickers.remove" => {
            let gid = param_id16(params, "group_id")?;
            node.group_sticker_remove(&gid, param_str(params, "name")?)?;
            Ok(json!({ "ok": true }))
        }
        "groups.stickers.list" => {
            let gid = param_id16(params, "group_id")?;
            let state = node.group_state(&gid)?;
            Ok(json!({
                "stickers": state.stickers.iter().map(|(name, hash)| json!({
                    "name": name,
                    "merkle_root": hex::encode(hash),
                })).collect::<Vec<_>>()
            }))
        }
        "groups.set_member_avatar" => {
            let gid = param_id16(params, "group_id")?;
            let avatar = match param_opt_str(params, "data_b64")? {
                Some(data_str) => {
                    let mime = param_str(params, "mime")?;
                    let data = b64_decode(data_str)
                        .ok_or(NodeError::Invalid("data_b64 : base64 invalide"))?;
                    node.group_set_member_avatar(&gid, Some((mime, data)))?
                }
                None => node.group_set_member_avatar(&gid, None)?,
            };
            Ok(json!({ "avatar": avatar }))
        }
        "groups.set_banner" => {
            let gid = param_id16(params, "group_id")?;
            let banner = match param_opt_str(params, "data_b64")? {
                Some(data_str) => {
                    let mime = param_str(params, "mime")?;
                    let data = b64_decode(data_str)
                        .ok_or(NodeError::Invalid("data_b64 : base64 invalide"))?;
                    node.group_set_banner(&gid, Some((mime, data)))?
                }
                None => node.group_set_banner(&gid, None)?,
            };
            Ok(json!({ "banner": banner }))
        }
        "groups.set_banner_color" => {
            let gid = param_id16(params, "group_id")?;
            let color = param_opt_color(params, "color")?
                .ok_or(NodeError::Invalid("color requis (entier 0xRRGGBB ou null)"))?;
            node.group_set_banner_color(&gid, color)?;
            Ok(json!({ "ok": true }))
        }
        "groups.automod.set" => {
            let gid = param_id16(params, "group_id")?;
            let words = params
                .get("words")
                .and_then(Value::as_array)
                .ok_or(NodeError::Invalid("words : liste attendue"))?
                .iter()
                .map(|v| {
                    v.as_str()
                        .map(str::to_string)
                        .ok_or(NodeError::Invalid("words : chaînes attendues"))
                })
                .collect::<Result<Vec<String>, NodeError>>()?;
            node.group_automod_set(&gid, words)?;
            Ok(json!({ "ok": true }))
        }
        "groups.automod.get" => {
            let gid = param_id16(params, "group_id")?;
            let state = node.group_state(&gid)?;
            Ok(json!({
                "words": state.automod_words.iter().collect::<Vec<_>>()
            }))
        }
        "groups.events.create" => {
            let gid = param_id16(params, "group_id")?;
            let title = param_str(params, "title")?;
            let description = param_opt_str(params, "description")?.unwrap_or("");
            let start_ms = param_u64(params, "start_ms", 0);
            let channel_id = param_opt_id16(params, "channel_id")?;
            Ok(json!({
                "event_id": node.group_event_create(&gid, title, description, start_ms, channel_id)?
            }))
        }
        "groups.events.edit" => {
            let gid = param_id16(params, "group_id")?;
            let eid = param_id16(params, "event_id")?;
            let title = param_str(params, "title")?;
            let description = param_opt_str(params, "description")?.unwrap_or("");
            let start_ms = param_u64(params, "start_ms", 0);
            let channel_id = param_opt_id16(params, "channel_id")?;
            node.group_event_edit(&gid, &eid, title, description, start_ms, channel_id)?;
            Ok(json!({ "ok": true }))
        }
        "groups.events.delete" => {
            let gid = param_id16(params, "group_id")?;
            let eid = param_id16(params, "event_id")?;
            node.group_event_delete(&gid, &eid)?;
            Ok(json!({ "ok": true }))
        }
        "groups.events.rsvp" => {
            let gid = param_id16(params, "group_id")?;
            let eid = param_id16(params, "event_id")?;
            let interested = params
                .get("interested")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            node.group_event_rsvp(&gid, &eid, interested)?;
            Ok(json!({ "ok": true }))
        }
        "groups.polls.vote" => {
            let gid = param_id16(params, "group_id")?;
            let pid = param_id16(params, "poll_id")?;
            let option_index = param_u8(params, "option_index")?;
            node.group_poll_vote(&gid, &pid, option_index)?;
            Ok(json!({ "ok": true }))
        }
        "groups.polls.close" => {
            let gid = param_id16(params, "group_id")?;
            let pid = param_id16(params, "poll_id")?;
            node.group_poll_close(&gid, &pid)?;
            Ok(json!({ "ok": true }))
        }
        "groups.polls.delete" => {
            let gid = param_id16(params, "group_id")?;
            let pid = param_id16(params, "poll_id")?;
            node.group_poll_delete(&gid, &pid)?;
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
        "groups.set_ephemeral" => {
            // Local-only disappearing-message timer (E2), group scope: same
            // contract as `dm.set_ephemeral`, keyed by group_id.
            let gid = param_id16(params, "group_id")?;
            let ttl = param_opt_u32(params, "ttl_secs")?.map(u64::from);
            node.set_conversation_ephemeral(&gid, ttl)?;
            Ok(json!({ "ok": true }))
        }
        "groups.ephemeral" => {
            let gid = param_id16(params, "group_id")?;
            Ok(json!({ "ttl_secs": node.conversation_ephemeral(&gid)? }))
        }
        _ => Err(NodeError::Invalid("méthode inconnue")),
    }
}
