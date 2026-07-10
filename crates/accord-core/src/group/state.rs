//! État répliqué d'un groupe : repli déterministe de l'op-log signé
//! (SPEC §6.2). Toute op non autorisée par l'état courant est ignorée —
//! tous les pairs honnêtes convergent vers le même état.

use accord_crypto::identity::node_id_of;
use accord_proto::core_msg::{perms, ChannelKind, GroupOp, GroupOpBody};
use std::collections::{BTreeMap, BTreeSet};

/// Permissions implicites de tout membre (D-015) ; les overrides de salon
/// peuvent les retirer (deny > allow).
pub const DEFAULT_MEMBER_PERMS: u32 = perms::VIEW | perms::SEND;

/// Toutes les permissions connues (fondateur / ADMIN).
pub const ALL_PERMS: u32 = perms::VIEW
    | perms::SEND
    | perms::MANAGE_MESSAGES
    | perms::MANAGE_CHANNELS
    | perms::INVITE
    | perms::KICK
    | perms::BAN
    | perms::MANAGE_ROLES
    | perms::ADMIN
    | perms::MANAGE_EMOJIS;

/// Nombre maximal d'émojis de serveur (au-delà, un nouvel ajout est ignoré ;
/// le remplacement d'un émoji existant reste possible).
pub const MAX_EMOJIS: usize = 50;

/// Longueur maximale d'un pseudo de serveur (en caractères, après trim).
pub const MAX_NICKNAME_CHARS: usize = 32;

/// Plafond de l'échéance d'une sourdine (`until_ms`, ms murales). ~an 2248,
/// très en deçà de 2^53 : garde une date exploitable côté UI (JS `number`).
pub const MAX_TIMEOUT_UNTIL_MS: u64 = 1 << 43;

/// Vrai si `c` est un caractère de « format » Unicode exploitable pour
/// l'usurpation visuelle d'identité : override bidirectionnel, marques
/// directionnelles, caractères de largeur nulle, isolats, BOM. `char::is_control`
/// (catégorie Cc) ne les couvre pas ; on les rejette explicitement dans les
/// pseudos de serveur pour éviter qu'un membre affiche un nom trompeur (texte
/// inversé, caractères cachés) dans la liste des membres ou les messages.
fn is_spoofing_char(c: char) -> bool {
    matches!(c,
        '\u{200B}'..='\u{200F}'   // ZWSP, ZWNJ, ZWJ, LRM, RLM
        | '\u{202A}'..='\u{202E}' // LRE, RLE, PDF, LRO, RLO
        | '\u{2066}'..='\u{2069}' // LRI, RLI, FSI, PDI
        | '\u{FEFF}',             // BOM / ZWNBSP
    )
}

/// Vrai si `name` (déjà trimmé) est un pseudo de serveur valide : 1 à 32
/// caractères, sans caractère de contrôle ni caractère de format trompeur
/// ([`is_spoofing_char`]). La chaîne vide efface le pseudo et n'est donc pas
/// « valide » au sens de cette fonction (traitée à part).
fn is_valid_nickname(name: &str) -> bool {
    let len = name.chars().count();
    (1..=MAX_NICKNAME_CHARS).contains(&len)
        && !name.chars().any(|c| c.is_control() || is_spoofing_char(c))
}

/// Vrai si `name` est un nom d'émoji valide : 2 à 32 caractères `[a-z0-9_]`.
fn is_valid_emoji_name(name: &str) -> bool {
    let len = name.chars().count();
    (2..=32).contains(&len)
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// Définition d'un rôle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleDef {
    /// Nom affiché.
    pub name: String,
    /// Couleur RGB (0xRRGGBB).
    pub color: u32,
    /// Position hiérarchique (plus haut = plus fort).
    pub position: u16,
    /// Permissions accordées.
    pub permissions: u32,
}

/// Membre du groupe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    /// Rôles attribués.
    pub roles: BTreeSet<[u8; 16]>,
    /// Lamport de l'op d'admission (ancienneté déterministe).
    pub joined_lamport: u64,
}

/// Salon (textuel ou vocal).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Channel {
    /// Nom affiché.
    pub name: String,
    /// Catégorie parente éventuelle.
    pub category: Option<[u8; 16]>,
    /// Nature du salon.
    pub kind: ChannelKind,
    /// Position de tri.
    pub position: u16,
    /// Sujet affiché.
    pub topic: String,
    /// Messages épinglés.
    pub pins: BTreeSet<[u8; 16]>,
}

/// Catégorie de salons.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Category {
    /// Nom affiché.
    pub name: String,
    /// Position de tri.
    pub position: u16,
}

/// Invitation active.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invite {
    /// SHA-256 du secret d'invitation.
    pub code_hash: [u8; 32],
    /// Utilisations maximales (0 = illimité).
    pub max_uses: u32,
    /// Utilisations consommées.
    pub uses: u32,
    /// Expiration murale ms (0 = jamais).
    pub expires_ms: u64,
    /// Révoquée.
    pub revoked: bool,
}

/// Overrides de permissions d'un rôle sur un salon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PermOverride {
    /// Bits explicitement accordés.
    pub allow: u32,
    /// Bits explicitement refusés (prioritaires).
    pub deny: u32,
}

/// État matérialisé d'un groupe après repli de l'op-log.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GroupState {
    /// Nom du groupe.
    pub name: String,
    /// Icône (racine Merkle) éventuelle.
    pub icon: Option<[u8; 32]>,
    /// Fondateur (clé publique de l'auteur du CREATE).
    pub founder: Option<[u8; 32]>,
    /// Membres actifs, par clé publique.
    pub members: BTreeMap<[u8; 32], Member>,
    /// Clés publiques bannies.
    pub banned: BTreeSet<[u8; 32]>,
    /// Rôles définis.
    pub roles: BTreeMap<[u8; 16], RoleDef>,
    /// Salons.
    pub channels: BTreeMap<[u8; 16], Channel>,
    /// Catégories.
    pub categories: BTreeMap<[u8; 16], Category>,
    /// Invitations.
    pub invites: BTreeMap<[u8; 16], Invite>,
    /// Overrides `(salon, rôle) → allow/deny`.
    pub overrides: BTreeMap<([u8; 16], [u8; 16]), PermOverride>,
    /// Émojis de serveur `nom → racine Merkle de l'image`. L'ordre du
    /// `BTreeMap` (lexicographique par nom) est stable et déterministe.
    pub emojis: BTreeMap<String, [u8; 32]>,
    /// Tombstones de modération à appliquer à l'historique local.
    pub moderated_deletions: BTreeSet<[u8; 16]>,
    /// Sourdines actives `membre → échéance murale (ms)`. Un membre est
    /// réduit au silence tant que `échéance > instant de référence` ; les
    /// entrées expirées sont ignorées à la vérification et effacées
    /// paresseusement (au clear ou au réécrasement).
    pub timeouts: BTreeMap<[u8; 32], u64>,
    /// Pseudos par serveur `membre → pseudo` (remplace le pseudo du profil
    /// global dans ce groupe uniquement).
    pub nicknames: BTreeMap<[u8; 32], String>,
    /// Nombre d'ops appliquées (dont ignorées : non).
    pub applied_ops: u64,
}

/// Issue de l'application d'une op.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applied {
    /// Op appliquée à l'état.
    Ok,
    /// Op ignorée (non autorisée ou incohérente) — convergence préservée.
    Ignored(&'static str),
}

impl GroupState {
    /// Replie un op-log complet dans l'ordre total `(lamport, node_id(author),
    /// op_id)` (SPEC §6.2). Les signatures sont supposées vérifiées à
    /// l'ingestion ([`crate::group::GroupEngine`]).
    pub fn fold(ops: &[GroupOp]) -> Self {
        let mut sorted: Vec<&GroupOp> = ops.iter().collect();
        sorted.sort_by_key(|op| (op.lamport, node_id_of(&op.author), op.op_id));
        let mut state = Self::default();
        for op in sorted {
            let _ = state.apply(op);
        }
        state
    }

    /// Vrai si `who` est membre actif.
    pub fn is_member(&self, who: &[u8; 32]) -> bool {
        self.members.contains_key(who)
    }

    /// Position hiérarchique maximale des rôles de `who` (fondateur = ∞).
    fn top_position(&self, who: &[u8; 32]) -> u32 {
        if self.founder.as_ref() == Some(who) {
            return u32::MAX;
        }
        self.members
            .get(who)
            .map(|m| {
                m.roles
                    .iter()
                    .filter_map(|r| self.roles.get(r))
                    .map(|r| r.position as u32)
                    .max()
                    .unwrap_or(0)
            })
            .unwrap_or(0)
    }

    /// Permissions globales de `who` (hors overrides de salon).
    pub fn base_permissions(&self, who: &[u8; 32]) -> u32 {
        if self.founder.as_ref() == Some(who) {
            return ALL_PERMS;
        }
        let Some(member) = self.members.get(who) else {
            return 0;
        };
        let mut acc = DEFAULT_MEMBER_PERMS;
        for role_id in &member.roles {
            if let Some(role) = self.roles.get(role_id) {
                acc |= role.permissions;
            }
        }
        if acc & perms::ADMIN != 0 {
            ALL_PERMS
        } else {
            acc
        }
    }

    /// Permissions effectives de `who` dans un salon (overrides appliqués,
    /// deny > allow ; ADMIN et fondateur court-circuitent).
    pub fn permissions_in(&self, who: &[u8; 32], channel_id: &[u8; 16]) -> u32 {
        let base = self.base_permissions(who);
        if base & perms::ADMIN != 0 {
            return ALL_PERMS;
        }
        let Some(member) = self.members.get(who) else {
            return 0;
        };
        let mut allow = 0u32;
        let mut deny = 0u32;
        for role_id in &member.roles {
            if let Some(o) = self.overrides.get(&(*channel_id, *role_id)) {
                allow |= o.allow;
                deny |= o.deny;
            }
        }
        (base | allow) & !deny
    }

    /// Vrai si `who` détient `perm` globalement.
    pub fn can(&self, who: &[u8; 32], perm: u32) -> bool {
        self.base_permissions(who) & perm != 0
    }

    /// Vrai si `who` est en sourdine à l'instant mural `when` (les sourdines
    /// expirées — échéance ≤ `when` — sont ignorées).
    pub fn is_timed_out(&self, who: &[u8; 32], when: u64) -> bool {
        self.timeouts.get(who).is_some_and(|&until| until > when)
    }

    /// Vrai si `who` peut publier un message dans `channel_id` à l'instant
    /// mural `when` : VIEW+SEND effectifs (overrides compris), non en sourdine,
    /// et — dans un salon d'annonces — porteur de `MANAGE_CHANNELS`. Reflète
    /// exactement la porte d'émission (`require_send`) pour que composition et
    /// ingestion restent symétriques.
    pub fn can_send_message(&self, who: &[u8; 32], channel_id: &[u8; 16], when: u64) -> bool {
        let Some(channel) = self.channels.get(channel_id) else {
            return false;
        };
        let eff = self.permissions_in(who, channel_id);
        if eff & (perms::VIEW | perms::SEND) != (perms::VIEW | perms::SEND) {
            return false;
        }
        if self.is_timed_out(who, when) {
            return false;
        }
        if channel.kind == ChannelKind::Announcement && eff & perms::MANAGE_CHANNELS == 0 {
            return false;
        }
        true
    }

    /// Membre désigné pour la rotation de clé (SPEC §6.4) : porteur de
    /// MANAGE_ROLES/ADMIN le plus ancien, sinon le fondateur s'il est membre,
    /// sinon le membre le plus ancien — règle totalement déterministe.
    pub fn rotation_responsible(&self) -> Option<[u8; 32]> {
        let by_seniority = |a: &(&[u8; 32], &Member), b: &(&[u8; 32], &Member)| {
            (a.1.joined_lamport, *a.0).cmp(&(b.1.joined_lamport, *b.0))
        };
        let mut managers: Vec<_> = self
            .members
            .iter()
            .filter(|(pk, _)| self.base_permissions(pk) & (perms::MANAGE_ROLES | perms::ADMIN) != 0)
            .collect();
        managers.sort_by(by_seniority);
        if let Some((pk, _)) = managers.first() {
            return Some(**pk);
        }
        if let Some(f) = &self.founder {
            if self.members.contains_key(f) {
                return Some(*f);
            }
        }
        let mut all: Vec<_> = self.members.iter().collect();
        all.sort_by(by_seniority);
        all.first().map(|(pk, _)| **pk)
    }

    /// Applique une op à l'état courant. Rend [`Applied::Ignored`] plutôt
    /// qu'une erreur : l'ignorance est déterministe et partagée.
    pub fn apply(&mut self, op: &GroupOp) -> Applied {
        let body = match GroupOpBody::decode_body(op.kind, &op.body) {
            Ok(b) => b,
            Err(_) => return self.ignore("corps indécodable"),
        };
        let author = op.author;

        // CREATE : uniquement comme toute première op.
        if let GroupOpBody::Create { name } = &body {
            if self.founder.is_some() {
                return self.ignore("groupe déjà créé");
            }
            self.founder = Some(author);
            self.name = name.clone();
            self.members.insert(
                author,
                Member {
                    roles: BTreeSet::new(),
                    joined_lamport: op.lamport,
                },
            );
            self.applied_ops += 1;
            return Applied::Ok;
        }
        if self.founder.is_none() {
            return self.ignore("aucun CREATE");
        }
        if !self.is_member(&author) {
            return self.ignore("auteur non membre");
        }

        let perms_of_author = self.base_permissions(&author);
        let has = |p: u32| {
            perms_of_author & (p | perms::ADMIN) != 0 || self.founder.as_ref() == Some(&author)
        };

        match body {
            GroupOpBody::Create { .. } => unreachable!("traité plus haut"),
            GroupOpBody::SetMeta { name, icon } => {
                if !has(perms::MANAGE_CHANNELS) {
                    return self.ignore("SET_META refusé");
                }
                self.name = name;
                self.icon = icon;
            }
            GroupOpBody::AddChannel {
                channel_id,
                name,
                category,
                kind,
                position,
            } => {
                if !has(perms::MANAGE_CHANNELS) {
                    return self.ignore("ADD_CHANNEL refusé");
                }
                if self.channels.contains_key(&channel_id) {
                    return self.ignore("salon existant");
                }
                self.channels.insert(
                    channel_id,
                    Channel {
                        name,
                        category,
                        kind,
                        position,
                        topic: String::new(),
                        pins: BTreeSet::new(),
                    },
                );
            }
            GroupOpBody::EditChannel {
                channel_id,
                name,
                position,
            } => {
                if !has(perms::MANAGE_CHANNELS) {
                    return self.ignore("EDIT_CHANNEL refusé");
                }
                let Some(ch) = self.channels.get_mut(&channel_id) else {
                    return self.ignore("salon inconnu");
                };
                ch.name = name;
                ch.position = position;
            }
            GroupOpBody::DelChannel { channel_id } => {
                if !has(perms::MANAGE_CHANNELS) {
                    return self.ignore("DEL_CHANNEL refusé");
                }
                if self.channels.remove(&channel_id).is_none() {
                    return self.ignore("salon inconnu");
                }
                self.overrides.retain(|(ch, _), _| ch != &channel_id);
            }
            GroupOpBody::AddCategory {
                category_id,
                name,
                position,
            } => {
                if !has(perms::MANAGE_CHANNELS) {
                    return self.ignore("ADD_CATEGORY refusé");
                }
                if self.categories.contains_key(&category_id) {
                    return self.ignore("catégorie existante");
                }
                self.categories
                    .insert(category_id, Category { name, position });
            }
            GroupOpBody::EditCategory {
                category_id,
                name,
                position,
            } => {
                if !has(perms::MANAGE_CHANNELS) {
                    return self.ignore("EDIT_CATEGORY refusé");
                }
                let Some(cat) = self.categories.get_mut(&category_id) else {
                    return self.ignore("catégorie inconnue");
                };
                cat.name = name;
                cat.position = position;
            }
            GroupOpBody::DelCategory { category_id } => {
                if !has(perms::MANAGE_CHANNELS) {
                    return self.ignore("DEL_CATEGORY refusé");
                }
                if self.categories.remove(&category_id).is_none() {
                    return self.ignore("catégorie inconnue");
                }
                // Channels of the deleted category survive, uncategorized.
                for channel in self.channels.values_mut() {
                    if channel.category == Some(category_id) {
                        channel.category = None;
                    }
                }
            }
            GroupOpBody::SetChannelCategory {
                channel_id,
                category,
            } => {
                if !has(perms::MANAGE_CHANNELS) {
                    return self.ignore("SET_CHANNEL_CATEGORY refusé");
                }
                if let Some(cat) = &category {
                    if !self.categories.contains_key(cat) {
                        return self.ignore("catégorie inconnue");
                    }
                }
                let Some(ch) = self.channels.get_mut(&channel_id) else {
                    return self.ignore("salon inconnu");
                };
                ch.category = category;
            }
            GroupOpBody::AddMember { member, invite_id } => {
                if !has(perms::INVITE) {
                    return self.ignore("ADD_MEMBER refusé");
                }
                if self.banned.contains(&member) {
                    return self.ignore("membre banni");
                }
                if self.members.contains_key(&member) {
                    return self.ignore("déjà membre");
                }
                if let Some(inv_id) = invite_id {
                    let Some(inv) = self.invites.get_mut(&inv_id) else {
                        return self.ignore("invitation inconnue");
                    };
                    if inv.revoked
                        || (inv.max_uses > 0 && inv.uses >= inv.max_uses)
                        || (inv.expires_ms > 0 && op.wall_ms > inv.expires_ms)
                    {
                        return self.ignore("invitation invalide");
                    }
                    inv.uses += 1;
                }
                self.members.insert(
                    member,
                    Member {
                        roles: BTreeSet::new(),
                        joined_lamport: op.lamport,
                    },
                );
            }
            GroupOpBody::Kick { member } | GroupOpBody::Ban { member } => {
                let (needed, label) = match op.kind {
                    0x08 => (perms::KICK, "KICK refusé"),
                    _ => (perms::BAN, "BAN refusé"),
                };
                if !has(needed) {
                    return self.ignore(label);
                }
                if self.founder.as_ref() == Some(&member) {
                    return self.ignore("le fondateur est intouchable");
                }
                if self.top_position(&author) <= self.top_position(&member)
                    && self.founder.as_ref() != Some(&author)
                {
                    return self.ignore("hiérarchie insuffisante");
                }
                if self.members.remove(&member).is_none() {
                    return self.ignore("cible non membre");
                }
                // A departed member keeps no per-group moderation state.
                self.timeouts.remove(&member);
                self.nicknames.remove(&member);
                if op.kind == 0x09 {
                    self.banned.insert(member);
                }
            }
            GroupOpBody::Unban { member } => {
                if !has(perms::BAN) {
                    return self.ignore("UNBAN refusé");
                }
                if !self.banned.remove(&member) {
                    return self.ignore("non banni");
                }
            }
            GroupOpBody::AddRole {
                role_id,
                name,
                color,
                position,
                permissions,
            } => {
                if !has(perms::MANAGE_ROLES) {
                    return self.ignore("ADD_ROLE refusé");
                }
                if self.roles.contains_key(&role_id) {
                    return self.ignore("rôle existant");
                }
                if !self.may_manage_position(&author, position) {
                    return self.ignore("rôle au-dessus de l'auteur");
                }
                self.roles.insert(
                    role_id,
                    RoleDef {
                        name,
                        color,
                        position,
                        permissions,
                    },
                );
            }
            GroupOpBody::EditRole {
                role_id,
                name,
                color,
                position,
                permissions,
            } => {
                if !has(perms::MANAGE_ROLES) {
                    return self.ignore("EDIT_ROLE refusé");
                }
                let Some(current) = self.roles.get(&role_id) else {
                    return self.ignore("rôle inconnu");
                };
                if !self.may_manage_position(&author, current.position)
                    || !self.may_manage_position(&author, position)
                {
                    return self.ignore("rôle au-dessus de l'auteur");
                }
                self.roles.insert(
                    role_id,
                    RoleDef {
                        name,
                        color,
                        position,
                        permissions,
                    },
                );
            }
            GroupOpBody::DelRole { role_id } => {
                if !has(perms::MANAGE_ROLES) {
                    return self.ignore("DEL_ROLE refusé");
                }
                let Some(current) = self.roles.get(&role_id) else {
                    return self.ignore("rôle inconnu");
                };
                if !self.may_manage_position(&author, current.position) {
                    return self.ignore("rôle au-dessus de l'auteur");
                }
                self.roles.remove(&role_id);
                for member in self.members.values_mut() {
                    member.roles.remove(&role_id);
                }
                self.overrides.retain(|(_, r), _| r != &role_id);
            }
            GroupOpBody::AssignRole { member, role_id }
            | GroupOpBody::UnassignRole { member, role_id } => {
                if !has(perms::MANAGE_ROLES) {
                    return self.ignore("ASSIGN_ROLE refusé");
                }
                let Some(role) = self.roles.get(&role_id) else {
                    return self.ignore("rôle inconnu");
                };
                if !self.may_manage_position(&author, role.position) {
                    return self.ignore("rôle au-dessus de l'auteur");
                }
                let Some(m) = self.members.get_mut(&member) else {
                    return self.ignore("cible non membre");
                };
                if op.kind == 0x0E {
                    m.roles.insert(role_id);
                } else {
                    m.roles.remove(&role_id);
                }
            }
            GroupOpBody::SetChannelPerms {
                channel_id,
                role_id,
                allow,
                deny,
            } => {
                if !has(perms::MANAGE_ROLES) {
                    return self.ignore("SET_CHANNEL_PERMS refusé");
                }
                if !self.channels.contains_key(&channel_id) || !self.roles.contains_key(&role_id) {
                    return self.ignore("salon ou rôle inconnu");
                }
                if allow == 0 && deny == 0 {
                    // Empty override = inherit everything: drop the entry so
                    // the materialized state stays minimal and the UI reads
                    // "no override" back.
                    self.overrides.remove(&(channel_id, role_id));
                } else {
                    self.overrides
                        .insert((channel_id, role_id), PermOverride { allow, deny });
                }
            }
            GroupOpBody::Pin { channel_id, msg_id } | GroupOpBody::Unpin { channel_id, msg_id } => {
                if !has(perms::MANAGE_MESSAGES) {
                    return self.ignore("PIN refusé");
                }
                let Some(ch) = self.channels.get_mut(&channel_id) else {
                    return self.ignore("salon inconnu");
                };
                if op.kind == 0x11 {
                    ch.pins.insert(msg_id);
                } else {
                    ch.pins.remove(&msg_id);
                }
            }
            GroupOpBody::DeleteMsg { channel_id, msg_id } => {
                if !has(perms::MANAGE_MESSAGES) {
                    return self.ignore("DELETE_MSG refusé");
                }
                if !self.channels.contains_key(&channel_id) {
                    return self.ignore("salon inconnu");
                }
                self.moderated_deletions.insert(msg_id);
            }
            GroupOpBody::SetTopic { channel_id, topic } => {
                if !has(perms::MANAGE_CHANNELS) {
                    return self.ignore("SET_TOPIC refusé");
                }
                let Some(ch) = self.channels.get_mut(&channel_id) else {
                    return self.ignore("salon inconnu");
                };
                ch.topic = topic;
            }
            GroupOpBody::InviteCreate {
                invite_id,
                code_hash,
                max_uses,
                expires_ms,
            } => {
                if !has(perms::INVITE) {
                    return self.ignore("INVITE_CREATE refusé");
                }
                if self.invites.contains_key(&invite_id) {
                    return self.ignore("invitation existante");
                }
                self.invites.insert(
                    invite_id,
                    Invite {
                        code_hash,
                        max_uses,
                        uses: 0,
                        expires_ms,
                        revoked: false,
                    },
                );
            }
            GroupOpBody::InviteRevoke { invite_id } => {
                if !has(perms::INVITE) {
                    return self.ignore("INVITE_REVOKE refusé");
                }
                let Some(inv) = self.invites.get_mut(&invite_id) else {
                    return self.ignore("invitation inconnue");
                };
                inv.revoked = true;
            }
            GroupOpBody::Leave => {
                if self.founder.as_ref() == Some(&author) && self.members.len() > 1 {
                    return self.ignore("le fondateur ne part pas en dernier");
                }
                self.members.remove(&author);
                self.timeouts.remove(&author);
                self.nicknames.remove(&author);
            }
            GroupOpBody::AddEmoji { name, file } => {
                if !has(perms::MANAGE_EMOJIS) {
                    return self.ignore("ADD_EMOJI refusé");
                }
                if !is_valid_emoji_name(&name) {
                    return self.ignore("nom d'émoji invalide");
                }
                // Le remplacement d'un émoji existant est toujours permis ;
                // seul un ajout portant le total au-delà de la borne est ignoré.
                if !self.emojis.contains_key(&name) && self.emojis.len() >= MAX_EMOJIS {
                    return self.ignore("trop d'émojis (50 max)");
                }
                self.emojis.insert(name, file);
            }
            GroupOpBody::DelEmoji { name } => {
                if !has(perms::MANAGE_EMOJIS) {
                    return self.ignore("DEL_EMOJI refusé");
                }
                if self.emojis.remove(&name).is_none() {
                    return self.ignore("émoji inconnu");
                }
            }
            GroupOpBody::TimeoutMember { member, until_ms } => {
                // Gated on KICK with the kick hierarchy: no timing out the
                // founder, nor a member of higher/equal role (D-015).
                if !has(perms::KICK) {
                    return self.ignore("TIMEOUT refusé");
                }
                if self.founder.as_ref() == Some(&member) {
                    return self.ignore("le fondateur est intouchable");
                }
                if self.top_position(&author) <= self.top_position(&member)
                    && self.founder.as_ref() != Some(&author)
                {
                    return self.ignore("hiérarchie insuffisante");
                }
                if !self.members.contains_key(&member) {
                    return self.ignore("cible non membre");
                }
                if until_ms == 0 {
                    self.timeouts.remove(&member);
                } else {
                    // Borne déterministe (tous les pairs plafonnent pareil) :
                    // évite qu'un `until_ms` absurde (p. ex. u64::MAX) perde de
                    // la précision côté UI (JS `number` > 2^53 → date invalide).
                    // Le porteur de KICK peut déjà exclure définitivement via
                    // KICK/BAN, donc plafonner n'ôte aucun pouvoir.
                    self.timeouts.insert(member, until_ms.min(MAX_TIMEOUT_UNTIL_MS));
                }
            }
            GroupOpBody::SetNickname { member, name } => {
                // Self-service, or a MANAGE_ROLES moderator strictly above the
                // target (kick-style hierarchy; the founder is untouchable).
                let is_self = author == member;
                if !is_self {
                    if !has(perms::MANAGE_ROLES) {
                        return self.ignore("SET_NICKNAME refusé");
                    }
                    if self.founder.as_ref() == Some(&member) {
                        return self.ignore("le fondateur est intouchable");
                    }
                    if self.top_position(&author) <= self.top_position(&member)
                        && self.founder.as_ref() != Some(&author)
                    {
                        return self.ignore("hiérarchie insuffisante");
                    }
                }
                if !self.members.contains_key(&member) {
                    return self.ignore("cible non membre");
                }
                let trimmed = name.trim();
                if trimmed.is_empty() {
                    self.nicknames.remove(&member);
                } else if is_valid_nickname(trimmed) {
                    self.nicknames.insert(member, trimmed.to_string());
                } else {
                    return self.ignore("pseudo invalide");
                }
            }
        }
        self.applied_ops += 1;
        Applied::Ok
    }

    /// Un auteur ne gère que des rôles strictement sous sa position
    /// (fondateur et ADMIN exemptés).
    fn may_manage_position(&self, author: &[u8; 32], position: u16) -> bool {
        if self.founder.as_ref() == Some(author)
            || self.base_permissions(author) & perms::ADMIN != 0
        {
            return true;
        }
        self.top_position(author) > position as u32
    }

    fn ignore(&self, reason: &'static str) -> Applied {
        tracing::debug!(reason, "op de groupe ignorée");
        Applied::Ignored(reason)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use accord_proto::core_msg::perms;

    fn signed(kind_body: GroupOpBody, author: [u8; 32], lamport: u64) -> GroupOp {
        GroupOp {
            op_id: {
                let mut id = [0u8; 16];
                id[0] = lamport as u8;
                id[1] = author[0];
                id
            },
            group_id: [1; 16],
            lamport,
            wall_ms: 0,
            author,
            kind: kind_body.kind(),
            body: kind_body.encode_body(),
            sig: [0; 64],
        }
    }

    const FOUNDER: [u8; 32] = [0xF0; 32];
    const ALICE: [u8; 32] = [0xA1; 32];
    const BOB: [u8; 32] = [0xB0; 32];

    fn base_ops() -> Vec<GroupOp> {
        vec![
            signed(
                GroupOpBody::Create {
                    name: "Salon".into(),
                },
                FOUNDER,
                1,
            ),
            signed(
                GroupOpBody::AddMember {
                    member: ALICE,
                    invite_id: None,
                },
                FOUNDER,
                2,
            ),
            signed(
                GroupOpBody::AddMember {
                    member: BOB,
                    invite_id: None,
                },
                FOUNDER,
                3,
            ),
        ]
    }

    #[test]
    fn create_establishes_founder_and_membership() {
        let st = GroupState::fold(&base_ops());
        assert_eq!(st.founder, Some(FOUNDER));
        assert_eq!(st.members.len(), 3);
        assert_eq!(st.name, "Salon");
        assert_eq!(st.base_permissions(&FOUNDER), ALL_PERMS);
        assert_eq!(st.base_permissions(&ALICE), DEFAULT_MEMBER_PERMS);
    }

    #[test]
    fn second_create_is_ignored() {
        let mut ops = base_ops();
        ops.push(signed(
            GroupOpBody::Create {
                name: "Usurpé".into(),
            },
            ALICE,
            10,
        ));
        let st = GroupState::fold(&ops);
        assert_eq!(st.founder, Some(FOUNDER));
        assert_eq!(st.name, "Salon");
    }

    #[test]
    fn unauthorized_ops_are_ignored_deterministically() {
        let mut ops = base_ops();
        // Alice (membre simple) tente de bannir Bob : ignoré.
        ops.push(signed(GroupOpBody::Ban { member: BOB }, ALICE, 4));
        let st = GroupState::fold(&ops);
        assert!(st.is_member(&BOB));
        assert!(st.banned.is_empty());
    }

    #[test]
    fn kick_and_ban_respect_hierarchy_and_founder() {
        let mut ops = base_ops();
        ops.push(signed(
            GroupOpBody::AddRole {
                role_id: [2; 16],
                name: "Modo".into(),
                color: 0xFF0000,
                position: 10,
                permissions: perms::KICK | perms::BAN,
            },
            FOUNDER,
            4,
        ));
        ops.push(signed(
            GroupOpBody::AssignRole {
                member: ALICE,
                role_id: [2; 16],
            },
            FOUNDER,
            5,
        ));
        // Alice (modo) peut expulser Bob.
        ops.push(signed(GroupOpBody::Kick { member: BOB }, ALICE, 6));
        // Mais pas le fondateur.
        ops.push(signed(GroupOpBody::Ban { member: FOUNDER }, ALICE, 7));
        let st = GroupState::fold(&ops);
        assert!(!st.is_member(&BOB));
        assert!(st.is_member(&FOUNDER));
        assert!(st.banned.is_empty());
    }

    #[test]
    fn banned_member_cannot_rejoin_until_unban() {
        let mut ops = base_ops();
        ops.push(signed(GroupOpBody::Ban { member: BOB }, FOUNDER, 4));
        ops.push(signed(
            GroupOpBody::AddMember {
                member: BOB,
                invite_id: None,
            },
            FOUNDER,
            5,
        ));
        let st = GroupState::fold(&ops);
        assert!(!st.is_member(&BOB), "réadmission d'un banni");
        let mut ops2 = ops.clone();
        ops2.push(signed(GroupOpBody::Unban { member: BOB }, FOUNDER, 6));
        ops2.push(signed(
            GroupOpBody::AddMember {
                member: BOB,
                invite_id: None,
            },
            FOUNDER,
            7,
        ));
        let st2 = GroupState::fold(&ops2);
        assert!(st2.is_member(&BOB));
    }

    #[test]
    fn invites_enforce_uses_and_expiry() {
        let mut ops = vec![
            signed(GroupOpBody::Create { name: "G".into() }, FOUNDER, 1),
            signed(
                GroupOpBody::InviteCreate {
                    invite_id: [9; 16],
                    code_hash: [0; 32],
                    max_uses: 1,
                    expires_ms: 0,
                },
                FOUNDER,
                2,
            ),
        ];
        ops.push(signed(
            GroupOpBody::AddMember {
                member: ALICE,
                invite_id: Some([9; 16]),
            },
            FOUNDER,
            3,
        ));
        // Deuxième usage de la même invitation à usage unique : ignoré.
        ops.push(signed(
            GroupOpBody::AddMember {
                member: BOB,
                invite_id: Some([9; 16]),
            },
            FOUNDER,
            4,
        ));
        let st = GroupState::fold(&ops);
        assert!(st.is_member(&ALICE));
        assert!(!st.is_member(&BOB));
    }

    #[test]
    fn channel_overrides_deny_beats_allow() {
        let mut ops = base_ops();
        ops.push(signed(
            GroupOpBody::AddChannel {
                channel_id: [7; 16],
                name: "général".into(),
                category: None,
                kind: ChannelKind::Text,
                position: 0,
            },
            FOUNDER,
            4,
        ));
        ops.push(signed(
            GroupOpBody::AddRole {
                role_id: [3; 16],
                name: "Muet".into(),
                color: 0,
                position: 1,
                permissions: 0,
            },
            FOUNDER,
            5,
        ));
        ops.push(signed(
            GroupOpBody::AssignRole {
                member: ALICE,
                role_id: [3; 16],
            },
            FOUNDER,
            6,
        ));
        ops.push(signed(
            GroupOpBody::SetChannelPerms {
                channel_id: [7; 16],
                role_id: [3; 16],
                allow: 0,
                deny: perms::SEND,
            },
            FOUNDER,
            7,
        ));
        let st = GroupState::fold(&ops);
        // Alice voit le salon mais ne peut pas y écrire.
        assert_ne!(st.permissions_in(&ALICE, &[7; 16]) & perms::VIEW, 0);
        assert_eq!(st.permissions_in(&ALICE, &[7; 16]) & perms::SEND, 0);
        // Bob (sans le rôle Muet) écrit normalement.
        assert_ne!(st.permissions_in(&BOB, &[7; 16]) & perms::SEND, 0);
    }

    #[test]
    fn fold_is_order_independent() {
        let mut ops = base_ops();
        ops.push(signed(
            GroupOpBody::AddChannel {
                channel_id: [7; 16],
                name: "général".into(),
                category: None,
                kind: ChannelKind::Text,
                position: 0,
            },
            FOUNDER,
            4,
        ));
        ops.push(signed(GroupOpBody::Kick { member: BOB }, FOUNDER, 5));
        let a = GroupState::fold(&ops);
        let mut shuffled = ops.clone();
        shuffled.reverse();
        let b = GroupState::fold(&shuffled);
        assert_eq!(a, b, "le repli doit être indépendant de l'ordre d'arrivée");
    }

    #[test]
    fn rotation_responsible_is_deterministic() {
        let mut ops = base_ops();
        let st = GroupState::fold(&ops);
        assert_eq!(st.rotation_responsible(), Some(FOUNDER));
        // Un modo MANAGE_ROLES devient responsable devant le fondateur.
        ops.push(signed(
            GroupOpBody::AddRole {
                role_id: [2; 16],
                name: "Gestionnaire".into(),
                color: 0,
                position: 5,
                permissions: perms::MANAGE_ROLES,
            },
            FOUNDER,
            4,
        ));
        ops.push(signed(
            GroupOpBody::AssignRole {
                member: ALICE,
                role_id: [2; 16],
            },
            FOUNDER,
            5,
        ));
        let st = GroupState::fold(&ops);
        // Le fondateur a ADMIN implicite (ALL_PERMS) et est plus ancien.
        assert_eq!(st.rotation_responsible(), Some(FOUNDER));
        // Fondateur parti : Alice (MANAGE_ROLES) prend la relève.
        // (KICK impossible sur le fondateur ⇒ départ volontaire… interdit
        // tant qu'il reste des membres ; on simule un groupe sans fondateur.)
        let mut st2 = st.clone();
        st2.members.remove(&FOUNDER);
        assert_eq!(st2.rotation_responsible(), Some(ALICE));
    }

    #[test]
    fn emojis_add_replace_delete_and_permission() {
        let mut ops = base_ops();
        // Le fondateur (ADMIN implicite) ajoute un émoji.
        ops.push(signed(
            GroupOpBody::AddEmoji {
                name: "parrot".into(),
                file: [1; 32],
            },
            FOUNDER,
            4,
        ));
        // Remplacement du même nom : mise à jour de l'image, pas de doublon.
        ops.push(signed(
            GroupOpBody::AddEmoji {
                name: "parrot".into(),
                file: [2; 32],
            },
            FOUNDER,
            5,
        ));
        // Alice, simple membre, ne peut ni ajouter ni supprimer.
        ops.push(signed(
            GroupOpBody::AddEmoji {
                name: "boom".into(),
                file: [3; 32],
            },
            ALICE,
            6,
        ));
        // Nom invalide (majuscule) : ignoré.
        ops.push(signed(
            GroupOpBody::AddEmoji {
                name: "Bad".into(),
                file: [4; 32],
            },
            FOUNDER,
            7,
        ));
        let st = GroupState::fold(&ops);
        assert_eq!(st.emojis.get("parrot"), Some(&[2; 32]));
        assert_eq!(st.emojis.len(), 1, "ni Alice ni le nom invalide n'ont pris");

        // Suppression par le fondateur.
        let mut ops2 = ops.clone();
        ops2.push(signed(
            GroupOpBody::DelEmoji {
                name: "parrot".into(),
            },
            FOUNDER,
            8,
        ));
        assert!(GroupState::fold(&ops2).emojis.is_empty());
    }

    #[test]
    fn emoji_cap_blocks_new_but_allows_replace() {
        let mut ops = base_ops();
        for i in 0..(MAX_EMOJIS as u64) {
            ops.push(signed(
                GroupOpBody::AddEmoji {
                    name: format!("e{i}"),
                    file: [i as u8; 32],
                },
                FOUNDER,
                10 + i,
            ));
        }
        // 51e nom distinct : ignoré (borne atteinte).
        ops.push(signed(
            GroupOpBody::AddEmoji {
                name: "one_too_many".into(),
                file: [9; 32],
            },
            FOUNDER,
            100,
        ));
        // Remplacement d'un nom existant : accepté malgré la borne.
        ops.push(signed(
            GroupOpBody::AddEmoji {
                name: "e0".into(),
                file: [42; 32],
            },
            FOUNDER,
            101,
        ));
        let st = GroupState::fold(&ops);
        assert_eq!(st.emojis.len(), MAX_EMOJIS);
        assert!(!st.emojis.contains_key("one_too_many"));
        assert_eq!(st.emojis.get("e0"), Some(&[42; 32]));
    }

    /// Base ops + one category `[C; 16]` and one channel `[7; 16]` inside it.
    fn ops_with_categorized_channel() -> Vec<GroupOp> {
        let mut ops = base_ops();
        ops.push(signed(
            GroupOpBody::AddCategory {
                category_id: [0xC; 16],
                name: "Vocaux".into(),
                position: 0,
            },
            FOUNDER,
            4,
        ));
        ops.push(signed(
            GroupOpBody::AddChannel {
                channel_id: [7; 16],
                name: "général".into(),
                category: Some([0xC; 16]),
                kind: ChannelKind::Text,
                position: 0,
            },
            FOUNDER,
            5,
        ));
        ops
    }

    #[test]
    fn edit_category_renames_and_requires_manage_channels() {
        let mut ops = ops_with_categorized_channel();
        // Alice (plain member) cannot rename; the founder can.
        ops.push(signed(
            GroupOpBody::EditCategory {
                category_id: [0xC; 16],
                name: "Piraté".into(),
                position: 3,
            },
            ALICE,
            6,
        ));
        ops.push(signed(
            GroupOpBody::EditCategory {
                category_id: [0xC; 16],
                name: "Textuels".into(),
                position: 2,
            },
            FOUNDER,
            7,
        ));
        // Unknown category: ignored.
        ops.push(signed(
            GroupOpBody::EditCategory {
                category_id: [0xE; 16],
                name: "Fantôme".into(),
                position: 0,
            },
            FOUNDER,
            8,
        ));
        let st = GroupState::fold(&ops);
        let cat = st.categories.get(&[0xC; 16]).unwrap();
        assert_eq!(cat.name, "Textuels");
        assert_eq!(cat.position, 2);
        assert_eq!(st.categories.len(), 1);
    }

    #[test]
    fn del_category_keeps_channels_uncategorized() {
        let mut ops = ops_with_categorized_channel();
        ops.push(signed(
            GroupOpBody::DelCategory {
                category_id: [0xC; 16],
            },
            FOUNDER,
            6,
        ));
        let st = GroupState::fold(&ops);
        assert!(st.categories.is_empty());
        let ch = st.channels.get(&[7; 16]).unwrap();
        assert_eq!(ch.category, None, "channel survives, uncategorized");
        assert_eq!(ch.name, "général");
    }

    #[test]
    fn del_category_denied_to_plain_member() {
        let mut ops = ops_with_categorized_channel();
        ops.push(signed(
            GroupOpBody::DelCategory {
                category_id: [0xC; 16],
            },
            ALICE,
            6,
        ));
        let st = GroupState::fold(&ops);
        assert!(st.categories.contains_key(&[0xC; 16]));
    }

    #[test]
    fn set_channel_category_moves_and_clears() {
        let mut ops = ops_with_categorized_channel();
        // Move out of any category…
        ops.push(signed(
            GroupOpBody::SetChannelCategory {
                channel_id: [7; 16],
                category: None,
            },
            FOUNDER,
            6,
        ));
        let st = GroupState::fold(&ops);
        assert_eq!(st.channels.get(&[7; 16]).unwrap().category, None);
        // …then back in; an unknown target category is ignored.
        let mut ops2 = ops.clone();
        ops2.push(signed(
            GroupOpBody::SetChannelCategory {
                channel_id: [7; 16],
                category: Some([0xC; 16]),
            },
            FOUNDER,
            7,
        ));
        ops2.push(signed(
            GroupOpBody::SetChannelCategory {
                channel_id: [7; 16],
                category: Some([0xE; 16]),
            },
            FOUNDER,
            8,
        ));
        // Alice cannot move channels at all.
        ops2.push(signed(
            GroupOpBody::SetChannelCategory {
                channel_id: [7; 16],
                category: None,
            },
            ALICE,
            9,
        ));
        let st2 = GroupState::fold(&ops2);
        assert_eq!(
            st2.channels.get(&[7; 16]).unwrap().category,
            Some([0xC; 16])
        );
    }

    #[test]
    fn empty_channel_override_clears_the_entry() {
        let mut ops = base_ops();
        ops.push(signed(
            GroupOpBody::AddChannel {
                channel_id: [7; 16],
                name: "général".into(),
                category: None,
                kind: ChannelKind::Text,
                position: 0,
            },
            FOUNDER,
            4,
        ));
        ops.push(signed(
            GroupOpBody::AddRole {
                role_id: [3; 16],
                name: "Muet".into(),
                color: 0,
                position: 1,
                permissions: 0,
            },
            FOUNDER,
            5,
        ));
        ops.push(signed(
            GroupOpBody::AssignRole {
                member: ALICE,
                role_id: [3; 16],
            },
            FOUNDER,
            6,
        ));
        ops.push(signed(
            GroupOpBody::SetChannelPerms {
                channel_id: [7; 16],
                role_id: [3; 16],
                allow: 0,
                deny: perms::SEND,
            },
            FOUNDER,
            7,
        ));
        let st = GroupState::fold(&ops);
        assert_eq!(st.overrides.len(), 1);
        assert_eq!(st.permissions_in(&ALICE, &[7; 16]) & perms::SEND, 0);

        // allow = deny = 0 removes the override: permissions are inherited.
        let mut ops2 = ops.clone();
        ops2.push(signed(
            GroupOpBody::SetChannelPerms {
                channel_id: [7; 16],
                role_id: [3; 16],
                allow: 0,
                deny: 0,
            },
            FOUNDER,
            8,
        ));
        let st2 = GroupState::fold(&ops2);
        assert!(st2.overrides.is_empty());
        assert_ne!(st2.permissions_in(&ALICE, &[7; 16]) & perms::SEND, 0);
    }

    #[test]
    fn moderation_deletion_recorded() {
        let mut ops = base_ops();
        ops.push(signed(
            GroupOpBody::AddChannel {
                channel_id: [7; 16],
                name: "g".into(),
                category: None,
                kind: ChannelKind::Text,
                position: 0,
            },
            FOUNDER,
            4,
        ));
        ops.push(signed(
            GroupOpBody::DeleteMsg {
                channel_id: [7; 16],
                msg_id: [0xDD; 16],
            },
            FOUNDER,
            5,
        ));
        let st = GroupState::fold(&ops);
        assert!(st.moderated_deletions.contains(&[0xDD; 16]));
    }

    #[test]
    fn timeout_requires_kick_and_respects_hierarchy() {
        let mut ops = base_ops();
        // Alice (plain member) cannot time out Bob.
        ops.push(signed(
            GroupOpBody::TimeoutMember {
                member: BOB,
                until_ms: 5_000,
            },
            ALICE,
            4,
        ));
        assert!(
            !GroupState::fold(&ops).is_timed_out(&BOB, 0),
            "plain member cannot time out"
        );

        // Give Alice a Modo role with KICK.
        ops.push(signed(
            GroupOpBody::AddRole {
                role_id: [2; 16],
                name: "Modo".into(),
                color: 0,
                position: 10,
                permissions: perms::KICK,
            },
            FOUNDER,
            5,
        ));
        ops.push(signed(
            GroupOpBody::AssignRole {
                member: ALICE,
                role_id: [2; 16],
            },
            FOUNDER,
            6,
        ));
        // Alice times out Bob (below her) but not the founder.
        ops.push(signed(
            GroupOpBody::TimeoutMember {
                member: BOB,
                until_ms: 5_000,
            },
            ALICE,
            7,
        ));
        ops.push(signed(
            GroupOpBody::TimeoutMember {
                member: FOUNDER,
                until_ms: 5_000,
            },
            ALICE,
            8,
        ));
        let st = GroupState::fold(&ops);
        assert!(st.is_timed_out(&BOB, 4_999));
        assert!(!st.is_timed_out(&BOB, 5_000), "expired at the deadline");
        assert!(!st.is_timed_out(&FOUNDER, 0), "founder untouchable");

        // Clearing with until_ms = 0 removes the entry.
        let mut cleared = ops.clone();
        cleared.push(signed(
            GroupOpBody::TimeoutMember {
                member: BOB,
                until_ms: 0,
            },
            FOUNDER,
            9,
        ));
        assert!(!GroupState::fold(&cleared).timeouts.contains_key(&BOB));
    }

    #[test]
    fn timeout_cleared_when_member_removed() {
        let mut ops = base_ops();
        ops.push(signed(
            GroupOpBody::TimeoutMember {
                member: BOB,
                until_ms: 9_000,
            },
            FOUNDER,
            4,
        ));
        ops.push(signed(GroupOpBody::Kick { member: BOB }, FOUNDER, 5));
        let st = GroupState::fold(&ops);
        assert!(!st.is_member(&BOB));
        assert!(
            !st.timeouts.contains_key(&BOB),
            "a kick clears the member's timeout"
        );
    }

    #[test]
    fn nickname_self_service_moderation_and_validation() {
        let mut ops = base_ops();
        // Bob sets his own nickname (plain member, trimmed).
        ops.push(signed(
            GroupOpBody::SetNickname {
                member: BOB,
                name: "  Bobby  ".into(),
            },
            BOB,
            4,
        ));
        assert_eq!(
            GroupState::fold(&ops)
                .nicknames
                .get(&BOB)
                .map(String::as_str),
            Some("Bobby"),
        );

        // Alice (plain member, no MANAGE_ROLES) cannot rename Bob.
        let mut denied = ops.clone();
        denied.push(signed(
            GroupOpBody::SetNickname {
                member: BOB,
                name: "Pirate".into(),
            },
            ALICE,
            5,
        ));
        assert_eq!(
            GroupState::fold(&denied)
                .nicknames
                .get(&BOB)
                .map(String::as_str),
            Some("Bobby"),
        );

        // A MANAGE_ROLES moderator above Bob renames him.
        ops.push(signed(
            GroupOpBody::AddRole {
                role_id: [2; 16],
                name: "Modo".into(),
                color: 0,
                position: 10,
                permissions: perms::MANAGE_ROLES,
            },
            FOUNDER,
            5,
        ));
        ops.push(signed(
            GroupOpBody::AssignRole {
                member: ALICE,
                role_id: [2; 16],
            },
            FOUNDER,
            6,
        ));
        ops.push(signed(
            GroupOpBody::SetNickname {
                member: BOB,
                name: "Renommé".into(),
            },
            ALICE,
            7,
        ));

        // A control character is rejected (op ignored, prior value kept).
        let mut bad = ops.clone();
        bad.push(signed(
            GroupOpBody::SetNickname {
                member: BOB,
                name: "bad\u{7}name".into(),
            },
            BOB,
            8,
        ));
        assert_eq!(
            GroupState::fold(&bad)
                .nicknames
                .get(&BOB)
                .map(String::as_str),
            Some("Renommé"),
        );

        // Visual-spoofing format characters (bidi override, zero-width) are
        // rejected too, even though they are not `char::is_control`.
        for spoof in ["\u{202E}pirate", "z\u{200B}ero", "\u{FEFF}bom"] {
            let mut sp = ops.clone();
            sp.push(signed(
                GroupOpBody::SetNickname {
                    member: BOB,
                    name: spoof.into(),
                },
                BOB,
                8,
            ));
            assert_eq!(
                GroupState::fold(&sp).nicknames.get(&BOB).map(String::as_str),
                Some("Renommé"),
                "spoofing nickname {spoof:?} must be rejected",
            );
        }

        // A whitespace-only name clears the nickname.
        ops.push(signed(
            GroupOpBody::SetNickname {
                member: BOB,
                name: "   ".into(),
            },
            BOB,
            8,
        ));
        assert!(!GroupState::fold(&ops).nicknames.contains_key(&BOB));
    }

    #[test]
    fn can_send_message_enforces_announcement_and_timeout() {
        let mut ops = base_ops();
        ops.push(signed(
            GroupOpBody::AddChannel {
                channel_id: [7; 16],
                name: "général".into(),
                category: None,
                kind: ChannelKind::Text,
                position: 0,
            },
            FOUNDER,
            4,
        ));
        ops.push(signed(
            GroupOpBody::AddChannel {
                channel_id: [8; 16],
                name: "annonces".into(),
                category: None,
                kind: ChannelKind::Announcement,
                position: 1,
            },
            FOUNDER,
            5,
        ));
        let st = GroupState::fold(&ops);
        // Text channel: any member may send.
        assert!(st.can_send_message(&BOB, &[7; 16], 0));
        // Announcement: only MANAGE_CHANNELS holders (founder here) may post.
        assert!(st.can_send_message(&FOUNDER, &[8; 16], 0));
        assert!(
            !st.can_send_message(&BOB, &[8; 16], 0),
            "announcements are read-only for plain members"
        );
        // Unknown channel: never sendable.
        assert!(!st.can_send_message(&BOB, &[9; 16], 0));

        // An active timeout blocks sending even in a text channel.
        ops.push(signed(
            GroupOpBody::TimeoutMember {
                member: BOB,
                until_ms: 1_000,
            },
            FOUNDER,
            6,
        ));
        let st = GroupState::fold(&ops);
        assert!(!st.can_send_message(&BOB, &[7; 16], 500), "timed out");
        assert!(
            st.can_send_message(&BOB, &[7; 16], 1_000),
            "timeout expired at the deadline"
        );
    }
}
