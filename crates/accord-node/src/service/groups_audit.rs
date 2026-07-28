//! Décodage d'une op de groupe en entrée de journal d'audit (`groups.audit`).
//!
//! Extrait de [`super::groups`] : la table « variante d'op → libellé stable +
//! champs lisibles » est longue par nature (une branche par op du protocole),
//! elle grossit à chaque nouvelle op, et elle n'a rien à voir avec
//! l'aiguillage des méthodes.
//!
//! `kind` est un libellé stable promis aux clients (voir `docs/API.md`) et
//! `params` ne porte que les champs utiles à une description humaine — jamais
//! le filaire brut.

use accord_proto::core_msg::{ChannelKind, GroupOp, GroupOpBody};
use serde_json::{json, Value};

use crate::hex;

/// Entrée du journal d'audit : op décodée en `{ op_id, lamport, wall_ms,
/// author, kind, params }` — `kind` en libellé stable, `params` limités aux
/// champs utiles à une description humaine (jamais le filaire brut).
pub(super) fn audit_entry_json(op: &GroupOp) -> Value {
    let kind_str = |k: ChannelKind| match k {
        ChannelKind::Text => "text",
        ChannelKind::Voice => "voice",
        ChannelKind::Announcement => "announcement",
        ChannelKind::Forum => "forum",
    };
    let (kind, params) = match GroupOpBody::decode_body(op.kind, &op.body) {
        Ok(body) => match body {
            GroupOpBody::Create { name, dm } => {
                ("create", json!({ "name": name, "dm": dm.unwrap_or(false) }))
            }
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
