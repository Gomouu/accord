//! Liste des membres d'un groupe : forme JSON d'un membre, et lecture
//! paginée (`groups.members`).
//!
//! **Pourquoi une méthode séparée.** `groups.state` sérialise la liste des
//! membres en entier à chaque appel : 115,8 Kio de JSON à 500 membres
//! (`docs/PERFORMANCE.md` §3.1), et le nœud émet `event.group_state` à chaque
//! op ingérée, donc l'interface recharge ce bloc très souvent (§3.2).
//! `groups.members` rend une tranche bornée de la même liste.
//!
//! ⚠️ **Ce que la pagination ne fait PAS.** Elle borne la *réponse*, pas le
//! travail : `Node::group_state` replie l'état complet du groupe dans les deux
//! cas (le repli est mémoïsé par `Db`, cf. §3.4). Le gain est en octets
//! sérialisés et en travail côté client, pas en temps de repli.
//!
//! [`member_json`] est l'unique source de la forme d'un membre : `groups.state`
//! et `groups.members` la traversent tous les deux, si bien qu'un client peut
//! passer de l'une à l'autre sans réécrire son décodeur. Les deux listent aussi
//! dans le même ordre — celui du `BTreeMap`, croissant par clé publique — donc
//! la concaténation des pages redonne exactement `groups.state.members`.

use accord_core::group::state::Member;
use accord_core::group::GroupState;
use serde_json::{json, Value};

use crate::error::NodeError;
use crate::hex;
use crate::node::Node;

use super::helpers::{param_id16, param_u64};

/// Plafond d'une page de `groups.members` (promesse publique, citée dans
/// `docs/API.md` et `docs/API_CONTRACT.md`). Même borne que les autres lectures
/// paginées de l'API (`groups.history`, `mentions.inbox`).
pub(super) const MAX_MEMBERS_PAGE: usize = 200;

/// Taille de page par défaut quand l'appelant n'en demande pas.
const DEFAULT_MEMBERS_PAGE: u64 = 50;

/// Forme API d'un membre. **Unique** source de cette forme : `groups.state`
/// (via `helpers::group_state_json`) et `groups.members` l'appellent toutes
/// deux, ce qui rend l'identité des deux formes structurelle plutôt que
/// promise.
pub(super) fn member_json(s: &GroupState, pk: &[u8; 32], m: &Member) -> Value {
    json!({
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
    })
}

/// Liste complète des membres, dans l'ordre du `BTreeMap` (croissant par clé
/// publique) — la forme historique de `groups.state.members`, inchangée.
pub(super) fn members_json(s: &GroupState) -> Vec<Value> {
    s.members
        .iter()
        .map(|(pk, m)| member_json(s, pk, m))
        .collect()
}

/// `groups.members { group_id, offset?, limit? }` → `{ members, total }`.
///
/// `offset` au-delà de `total` rend une page vide et le `total` juste : c'est
/// une fin de liste, pas une erreur — un client qui pagine pendant qu'un
/// membre part ne doit pas recevoir d'échec pour ça.
pub(super) fn members_page(node: &Node, params: &Value) -> Result<Value, NodeError> {
    let gid = param_id16(params, "group_id")?;
    // `usize::try_from` plutôt qu'un `as` : sur une cible 32 bits, un `offset`
    // de 2^40 tronquerait en une page valide au lieu de tomber hors liste.
    let offset = usize::try_from(param_u64(params, "offset", 0)).unwrap_or(usize::MAX);
    // Borne dure : un `limit` absurde ne doit pas pouvoir rappeler la liste
    // entière par la porte de derrière. Le bas de la borne est 1, pas 0 —
    // `limit: 0` est une demande absurde, pas une demande de page vide.
    let limit = usize::try_from(param_u64(params, "limit", DEFAULT_MEMBERS_PAGE))
        .unwrap_or(usize::MAX)
        .clamp(1, MAX_MEMBERS_PAGE);
    let state = node.group_state(&gid)?;
    let members: Vec<Value> = state
        .members
        .iter()
        .skip(offset)
        .take(limit)
        .map(|(pk, m)| member_json(&state, pk, m))
        .collect();
    // `total` = la liste ENTIÈRE, pas la page : c'est ce qui permet à
    // l'appelant de savoir s'il lui reste des pages à demander.
    Ok(json!({ "members": members, "total": state.members.len() }))
}
