//! Helpers partagés de la frontière API : parsing des paramètres JSON-RPC et
//! conversions des records applicatifs en JSON (formes gelées côté UI).

use accord_core::db::{Contact, ContactState, DmRecord, GroupMsgRecord};
use accord_core::group::GroupState;
use accord_core::messaging::MAX_ATTACHMENTS;
use accord_crypto::FriendCode;
use accord_proto::core_msg::{ChannelKind, FileRef, MAX_POLL_OPTIONS};
use serde_json::{json, Value};

use crate::error::NodeError;
use crate::hex;

pub(super) fn param_str<'a>(params: &'a Value, key: &str) -> Result<&'a str, NodeError> {
    params
        .get(key)
        .and_then(Value::as_str)
        .ok_or(NodeError::Invalid("paramètre chaîne manquant"))
}

pub(super) fn param_pubkey(params: &Value, key: &str) -> Result<[u8; 32], NodeError> {
    hex::decode::<32>(param_str(params, key)?).ok_or(NodeError::Invalid("clé publique invalide"))
}

pub(super) fn param_id16(params: &Value, key: &str) -> Result<[u8; 16], NodeError> {
    hex::decode::<16>(param_str(params, key)?).ok_or(NodeError::Invalid("identifiant invalide"))
}

/// Champ périphérique de `voice.set_devices` (D-029) : absent = inchangé
/// (`None`), `null` = périphérique par défaut (`Some(None)`), chaîne = nom
/// `cpal` (`Some(Some(nom))`). Tout autre type est refusé à la frontière.
pub(super) fn param_device(params: &Value, key: &str) -> Result<Option<Option<String>>, NodeError> {
    match params.get(key) {
        None => Ok(None),
        Some(Value::Null) => Ok(Some(None)),
        Some(Value::String(name)) => Ok(Some(Some(name.clone()))),
        Some(_) => Err(NodeError::Invalid(
            "nom de périphérique : chaîne ou null requis",
        )),
    }
}

pub(super) fn param_u64(params: &Value, key: &str, default: u64) -> u64 {
    params.get(key).and_then(Value::as_u64).unwrap_or(default)
}

/// Chaîne optionnelle : absente → `None`, présente mais non-chaîne → erreur.
pub(super) fn param_opt_str<'a>(
    params: &'a Value,
    key: &str,
) -> Result<Option<&'a str>, NodeError> {
    match params.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.as_str())),
        Some(_) => Err(NodeError::Invalid("paramètre chaîne attendu")),
    }
}

/// Entier `u32` requis (borné par JSON à `u64`, revalidé ici).
pub(super) fn param_u32(params: &Value, key: &str) -> Result<u32, NodeError> {
    params
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|v| u32::try_from(v).ok())
        .ok_or(NodeError::Invalid(
            "paramètre entier manquant ou hors bornes",
        ))
}

/// Entier `u8` requis (borné par JSON à `u64`, revalidé ici) — utilisé pour
/// `option_index` d'un vote de sondage (D-048).
pub(super) fn param_u8(params: &Value, key: &str) -> Result<u8, NodeError> {
    params
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|v| u8::try_from(v).ok())
        .ok_or(NodeError::Invalid(
            "paramètre entier manquant ou hors bornes (0-255)",
        ))
}

/// Entier `u32` optionnel : absent → `None`, invalide → erreur.
pub(super) fn param_opt_u32(params: &Value, key: &str) -> Result<Option<u32>, NodeError> {
    match params.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_u64()
            .and_then(|v| u32::try_from(v).ok())
            .map(Some)
            .ok_or(NodeError::Invalid("paramètre entier hors bornes")),
    }
}

/// Couleur `0xRRGGBB` optionnelle à trois états (même forme que
/// [`param_device`]) : absente → inchangée (`None`), `null` → effacée
/// (`Some(None)`), entier → définie (`Some(Some(c))`, revalidée côté cœur).
pub(super) fn param_opt_color(params: &Value, key: &str) -> Result<Option<Option<u32>>, NodeError> {
    match params.get(key) {
        None => Ok(None),
        Some(Value::Null) => Ok(Some(None)),
        Some(v) => v
            .as_u64()
            .and_then(|v| u32::try_from(v).ok())
            .map(|c| Some(Some(c)))
            .ok_or(NodeError::Invalid(
                "couleur : entier 0xRRGGBB ou null requis",
            )),
    }
}

/// Entier `u16` optionnel (positions de salons et de rôles).
pub(super) fn param_opt_u16(params: &Value, key: &str) -> Result<Option<u16>, NodeError> {
    match param_opt_u32(params, key)? {
        None => Ok(None),
        Some(v) => u16::try_from(v)
            .map(Some)
            .map_err(|_| NodeError::Invalid("position hors bornes (0-65535)")),
    }
}

/// Nature de salon : `"text"` (défaut), `"voice"` ou `"announcement"`.
pub(super) fn param_channel_kind(params: &Value, key: &str) -> Result<ChannelKind, NodeError> {
    match param_opt_str(params, key)? {
        None | Some("text") => Ok(ChannelKind::Text),
        Some("voice") => Ok(ChannelKind::Voice),
        Some("announcement") => Ok(ChannelKind::Announcement),
        Some(_) => Err(NodeError::Invalid(
            "kind de salon inconnu (text, voice ou announcement)",
        )),
    }
}

/// Pièces jointes optionnelles : `attachments: [{ merkle_root, name, size,
/// mime }]`, 10 au plus, chaque champ validé à la frontière (le cœur revalide
/// les bornes de contenu).
pub(super) fn param_attachments(params: &Value) -> Result<Vec<FileRef>, NodeError> {
    let Some(raw) = params.get("attachments") else {
        return Ok(vec![]);
    };
    let list = raw
        .as_array()
        .ok_or(NodeError::Invalid("attachments : liste attendue"))?;
    if list.len() > MAX_ATTACHMENTS {
        return Err(NodeError::Invalid("trop de pièces jointes (max 10)"));
    }
    list.iter()
        .map(|item| {
            let merkle_root = item
                .get("merkle_root")
                .and_then(Value::as_str)
                .and_then(hex::decode::<32>)
                .ok_or(NodeError::Invalid("pièce jointe : merkle_root invalide"))?;
            let name = item
                .get("name")
                .and_then(Value::as_str)
                .ok_or(NodeError::Invalid("pièce jointe : nom manquant"))?;
            let size = item
                .get("size")
                .and_then(Value::as_u64)
                .ok_or(NodeError::Invalid("pièce jointe : taille manquante"))?;
            let mime = item
                .get("mime")
                .and_then(Value::as_str)
                .ok_or(NodeError::Invalid("pièce jointe : type MIME manquant"))?;
            Ok(FileRef {
                merkle_root,
                name: name.to_string(),
                size,
                mime: mime.to_string(),
            })
        })
        .collect()
}

/// Décode un contenu base64 standard (padding requis, alphabet strict).
/// Implémentation locale minimale : pas de dépendance externe pour un seul
/// point d'entrée (icône de groupe).
pub(super) fn b64_decode(input: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some((c - b'A') as u32),
            b'a'..=b'z' => Some((c - b'a' + 26) as u32),
            b'0'..=b'9' => Some((c - b'0' + 52) as u32),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let bytes = input.as_bytes();
    if bytes.is_empty() || bytes.len() % 4 != 0 {
        return None;
    }
    // Le remplissage `=` n'apparaît qu'en toute fin de contenu.
    if input.trim_end_matches('=').contains('=') {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks(4) {
        let pad = chunk.iter().rev().take_while(|&&c| c == b'=').count();
        if pad > 2 || chunk[..4 - pad].contains(&b'=') {
            return None;
        }
        let mut acc = 0u32;
        for &c in &chunk[..4 - pad] {
            acc = (acc << 6) | val(c)?;
        }
        acc <<= 6 * pad as u32;
        let full = [(acc >> 16) as u8, (acc >> 8) as u8, acc as u8];
        out.extend_from_slice(&full[..3 - pad]);
    }
    Some(out)
}

pub(super) fn param_limit(params: &Value) -> usize {
    (param_u64(params, "limit", 50)).clamp(1, 200) as usize
}

/// Résout un pair désigné soit par `pubkey` (hex), soit par `code` ami. Le
/// code ne fournit que 33 bits : la résolution DHT complète a lieu côté
/// réseau ; ici on n'accepte que la clé publique explicite pour les actions
/// locales.
pub(super) fn param_peer(params: &Value) -> Result<[u8; 32], NodeError> {
    if params.get("pubkey").is_some() {
        return param_pubkey(params, "pubkey");
    }
    Err(NodeError::Invalid("pubkey requis"))
}

fn state_str(state: ContactState) -> &'static str {
    match state {
        ContactState::PendingOut => "pending_out",
        ContactState::PendingIn => "pending_in",
        ContactState::Friend => "friend",
        ContactState::Blocked => "blocked",
    }
}

pub(super) fn contact_json(c: &Contact) -> Value {
    json!({
        "node_id": hex::encode(&c.node_id),
        "pubkey": hex::encode(&c.pubkey),
        "friend_code": FriendCode::of_pubkey(&c.pubkey).display(),
        "display_name": c.display_name,
        "state": state_str(c.state),
        "last_seen_ms": c.last_seen_ms,
    })
}

/// Décode un corps de message en JSON structuré pour l'UI. La frontière API
/// ne transporte jamais d'encodage filaire : un corps indéchiffrable devient
/// `{ "type": "unknown" }` (affiché comme message non pris en charge).
fn body_json(kind: u8, body: &[u8]) -> Value {
    use accord_proto::core_msg::MsgBody;
    match MsgBody::decode_body(kind, body) {
        Ok(MsgBody::Text {
            text,
            reply_to,
            attachments,
        }) => json!({
            "type": "text",
            "text": text,
            "reply_to": reply_to.map(|r| hex::encode(&r)),
            "attachments": attachments.len(),
        }),
        Ok(MsgBody::Edit { target, new_text }) => json!({
            "type": "edit",
            "target": hex::encode(&target),
            "text": new_text,
        }),
        Ok(MsgBody::Delete { target }) => json!({
            "type": "delete",
            "target": hex::encode(&target),
        }),
        Ok(MsgBody::Reaction { target, emoji, add }) => json!({
            "type": "reaction",
            "target": hex::encode(&target),
            "emoji": emoji,
            "add": add,
        }),
        Ok(MsgBody::Sticker { name, merkle_root }) => json!({
            "type": "sticker",
            "name": name,
            "merkle_root": hex::encode(&merkle_root),
        }),
        Ok(MsgBody::Poll {
            poll_id,
            question,
            options,
        }) => json!({
            "type": "poll",
            "poll_id": hex::encode(&poll_id),
            "question": question,
            "options": options,
        }),
        Ok(MsgBody::Typing) | Ok(MsgBody::ReadReceipt { .. }) => json!({ "type": "meta" }),
        Err(_) => json!({ "type": "unknown" }),
    }
}

/// Rend la liste des réactions d'un message : `[{ emoji, author }]`.
fn reactions_json(reactions: &[(String, [u8; 32])]) -> Value {
    Value::Array(
        reactions
            .iter()
            .map(|(emoji, author)| json!({ "emoji": emoji, "author": hex::encode(author) }))
            .collect(),
    )
}

/// Rend une liste de pièces jointes : `[{ merkle_root, name, size, mime }]`.
pub(super) fn attachments_json(attachments: &[FileRef]) -> Value {
    Value::Array(
        attachments
            .iter()
            .map(|a| {
                json!({
                    "merkle_root": hex::encode(&a.merkle_root),
                    "name": a.name,
                    "size": a.size,
                    "mime": a.mime,
                })
            })
            .collect(),
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn dm_json(
    m: &DmRecord,
    reactions: &[(String, [u8; 32])],
    attachments: &[FileRef],
    pinned: bool,
    delivery: &str,
    mentions_me: bool,
) -> Value {
    json!({
        "msg_id": hex::encode(&m.msg_id),
        "author": hex::encode(&m.author),
        "lamport": m.lamport,
        "sent_ms": m.sent_ms,
        "acked": m.acked,
        "deleted": m.deleted,
        "pinned": pinned,
        // Delivery state of our outgoing message: sent∣pending∣failed.
        "delivery": delivery,
        // True when this message mentions the local user (local detection).
        "mentions_me": mentions_me,
        "body": body_json(m.kind, &m.body),
        // Dernière édition : texte brut UTF-8, ou null.
        "edited": m.edited.as_ref().map(|b| String::from_utf8_lossy(b)),
        "reactions": reactions_json(reactions),
        "attachments": attachments_json(attachments),
    })
}

pub(super) fn group_msg_json(
    m: &GroupMsgRecord,
    reactions: &[(String, [u8; 32])],
    attachments: &[FileRef],
    mentions_me: bool,
) -> Value {
    json!({
        "msg_id": hex::encode(&m.msg_id),
        "channel_id": hex::encode(&m.channel_id),
        "author": hex::encode(&m.author),
        "lamport": m.lamport,
        "sent_ms": m.sent_ms,
        "deleted": m.deleted,
        // True when this message mentions the local user (local detection).
        "mentions_me": mentions_me,
        "body": body_json(m.kind, &m.body),
        "edited": m.edited.as_ref().map(|b| String::from_utf8_lossy(b)),
        "reactions": reactions_json(reactions),
        "attachments": attachments_json(attachments),
    })
}

fn channel_kind_str(kind: ChannelKind) -> &'static str {
    match kind {
        ChannelKind::Text => "text",
        ChannelKind::Voice => "voice",
        ChannelKind::Announcement => "announcement",
    }
}

/// État complet d'un groupe pour l'UI. `me` : clé publique locale, pour
/// calculer `my_permissions` (bitfield `perms::*`, voir API.md).
pub(super) fn group_state_json(group_id: &[u8; 16], s: &GroupState, me: &[u8; 32]) -> Value {
    json!({
        "group_id": hex::encode(group_id),
        "name": s.name,
        "icon": s.icon.as_ref().map(|h| hex::encode(h)),
        // Server banner color `0xRRGGBB` (D-047), or null when unset.
        "banner_color": s.banner_color,
        "founder": s.founder.as_ref().map(|f| hex::encode(f)),
        "members": s.members.iter().map(|(pk, m)| json!({
            "pubkey": hex::encode(pk),
            "roles": m.roles.iter().map(|r| hex::encode(r)).collect::<Vec<_>>(),
            // Per-group display name (overrides the global profile name), or null.
            "nickname": s.nicknames.get(pk),
            // Per-group avatar (op 0x26, self-service only), or null.
            "avatar": s.member_avatars.get(pk).map(|h| hex::encode(h)),
            // Active timeout deadline (wall ms), or 0 when not muted. The UI
            // compares it against the current time (expired timeouts are moot).
            "timeout_until_ms": s.timeouts.get(pk).copied().unwrap_or(0),
            // Server-side voice moderation (op 0x1F): forced mute/deafen in
            // every voice channel of the group (both false when unmoderated).
            "voice_muted": s.voice_moderation_of(pk).mute,
            "voice_deafened": s.voice_moderation_of(pk).deafen,
        })).collect::<Vec<_>>(),
        "bans": s.banned.iter().map(|pk| hex::encode(pk)).collect::<Vec<_>>(),
        "channels": s.channels.iter().map(|(id, ch)| json!({
            "channel_id": hex::encode(id),
            "name": ch.name,
            "kind": channel_kind_str(ch.kind),
            "category": ch.category.as_ref().map(|c| hex::encode(c)),
            "position": ch.position,
            "topic": ch.topic,
            // Slow mode cooldown in seconds (0/absent semantics: 0 = off).
            // Enforcement is NOT part of this replicated state — every
            // honest peer re-applies it locally at message compose/ingest
            // (see docs/COMMUNITY.md).
            "slowmode_secs": ch.slowmode_secs,
        })).collect::<Vec<_>>(),
        "categories": s.categories.iter().map(|(id, c)| json!({
            "category_id": hex::encode(id),
            "name": c.name,
            "position": c.position,
        })).collect::<Vec<_>>(),
        "roles": s.roles.iter().map(|(id, r)| json!({
            "role_id": hex::encode(id),
            "name": r.name,
            "color": r.color,
            "position": r.position,
            "permissions": r.permissions,
        })).collect::<Vec<_>>(),
        "invites": s.invites.iter().map(|(id, inv)| json!({
            "invite_id": hex::encode(id),
            "max_uses": inv.max_uses,
            "uses": inv.uses,
            "expires_ms": inv.expires_ms,
            "revoked": inv.revoked,
        })).collect::<Vec<_>>(),
        "emojis": s.emojis.iter().map(|(name, hash)| json!({
            "name": name,
            "merkle_root": hex::encode(hash),
        })).collect::<Vec<_>>(),
        "stickers": s.stickers.iter().map(|(name, hash)| json!({
            "name": name,
            "merkle_root": hex::encode(hash),
        })).collect::<Vec<_>>(),
        // AutoMod blocked-word list (server config, replicated, lowercased
        // — see docs/COMMUNITY.md for the honest-P2P enforcement caveat:
        // this backend only stores/replicates the list, an honest client
        // is responsible for warning the sender / masking received words).
        "automod_words": s.automod_words.iter().collect::<Vec<_>>(),
        "events": s.events.iter().map(|(id, ev)| json!({
            "event_id": hex::encode(id),
            "title": ev.title,
            "description": ev.description,
            "start_ms": ev.start_ms,
            "channel_id": ev.channel_id.as_ref().map(|c| hex::encode(c)),
            "author": hex::encode(&ev.author),
            "rsvp_count": ev.rsvps.len(),
            // True when the local user is in the RSVP list.
            "rsvped": ev.rsvps.contains(me),
        })).collect::<Vec<_>>(),
        // Poll tallies (D-048). The question/options themselves are NOT
        // here — they live in the originating `MsgBody::Poll` message
        // (content-addressed to `poll_id`, fetched via `groups.history`);
        // this is only the live, replicated vote count. `counts[i]` is the
        // number of votes for option `i`; entries beyond a poll's *real*
        // option count are always 0 in practice (an honest UI never lets a
        // caller vote out of range) but the array is always
        // `MAX_POLL_OPTIONS` wide regardless of the real option count.
        "polls": s.polls.iter().map(|(id, p)| {
            let mut counts = [0u64; MAX_POLL_OPTIONS];
            for &opt in p.votes.values() {
                if let Some(slot) = counts.get_mut(opt as usize) {
                    *slot += 1;
                }
            }
            json!({
                "poll_id": hex::encode(id),
                "author": hex::encode(&p.author),
                "channel_id": hex::encode(&p.channel_id),
                "msg_id": hex::encode(&p.msg_id),
                "closed": p.closed,
                "counts": counts,
                "total_votes": p.votes.len(),
                // The caller's own chosen option index, or null if they
                // haven't voted.
                "my_vote": p.votes.get(me),
            })
        }).collect::<Vec<_>>(),
        "my_permissions": s.base_permissions(me),
    })
}
