//! Canal CORE (0x02) : messagerie directe, groupes, présence (SPEC §6).

use crate::limits;
use crate::wire::{DecodeError, Reader, WireDecode, WireEncode, Writer};

const MAX_NAME: usize = 256;
/// Borne stricte du pseudo d'un message `Profile` : 32 caractères × 4 octets
/// UTF-8 au plus (la validation sémantique 2-32 caractères a lieu à
/// l'ingestion, côté cœur).
const MAX_PROFILE_NAME: usize = 128;
const MAX_BIO: usize = 2048;
/// Borne filaire des pronoms d'un message `Profile` (champ additif) : 40
/// octets UTF-8 au plus (validation sémantique côté cœur à l'ingestion).
const MAX_PRONOUNS: usize = 40;
/// Borne d'une couleur `0xRRGGBB` (`accent_color`/`banner_color`, champs
/// additifs de `Profile`) : tout ce qui dépasse 24 bits est rejeté au
/// décodage.
const MAX_COLOR: u32 = 0xFF_FF_FF;
const MAX_BODY: usize = 64 * 1024;
const MAX_ATTACHMENTS: usize = 10;
const MAX_EMOJI: usize = 64;
/// Borne du nom d'un émoji de serveur (2-32 caractères `[a-z0-9_]` validés à
/// l'ingestion ; 32 octets suffisent, l'alphabet étant ASCII).
const MAX_EMOJI_NAME: usize = 32;
/// Borne filaire d'un pseudo de serveur : 32 caractères × 4 octets UTF-8 au
/// plus (la validation sémantique 1-32 caractères, sans contrôle, a lieu au
/// repli du journal et à la frontière API).
const MAX_NICKNAME: usize = 128;
/// Borne haute de l'expiration d'un ticket d'invitation (ms muraux absolus,
/// ~an 2248, alignée sur `MAX_TIMEOUT_UNTIL_MS` côté cœur) : rejette au
/// décodage une valeur absurde plutôt que de la laisser transiter (D-045).
const MAX_TICKET_EXPIRES_MS: u64 = 1 << 43;
/// Borne filaire d'un titre d'événement planifié : 100 caractères × 4 octets
/// UTF-8 au plus (validation sémantique 2-100 caractères au repli, D-047).
const MAX_EVENT_TITLE: usize = 400;
/// Borne filaire d'une description d'événement : 1024 caractères × 4 octets
/// UTF-8 au plus (validation sémantique au repli, mêmes règles que la bio —
/// voir `accord_core::profile::validate_bio`).
const MAX_EVENT_DESC: usize = 4096;

/// Bitfield de permissions de groupe (SPEC §6.2).
pub mod perms {
    /// Voir le salon.
    pub const VIEW: u32 = 1;
    /// Envoyer des messages.
    pub const SEND: u32 = 2;
    /// Gérer (supprimer/épingler) les messages.
    pub const MANAGE_MESSAGES: u32 = 4;
    /// Gérer les salons et catégories.
    pub const MANAGE_CHANNELS: u32 = 8;
    /// Créer des invitations.
    pub const INVITE: u32 = 16;
    /// Expulser des membres.
    pub const KICK: u32 = 32;
    /// Bannir des membres.
    pub const BAN: u32 = 64;
    /// Gérer les rôles.
    pub const MANAGE_ROLES: u32 = 128;
    /// Administrateur : implique toutes les permissions.
    pub const ADMIN: u32 = 256;
    /// Gérer les émojis de serveur (ajout/suppression).
    pub const MANAGE_EMOJIS: u32 = 512;
    /// Orateur prioritaire : les autres participants d'un salon vocal sont
    /// atténués pendant que ce membre parle. Jamais impliquée par ADMIN ni
    /// par le statut de fondateur (attribution explicite de rôle uniquement).
    pub const PRIORITY_SPEAKER: u32 = 1024;
}

/// Décode un booléen filaire strict (0 ou 1 ; toute autre valeur rejette la
/// structure — entrée attaquant-contrôlée, jamais de tolérance).
fn decode_bool(r: &mut Reader<'_>, what: &'static str) -> Result<bool, DecodeError> {
    match r.u8()? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(DecodeError::InvalidValue(what)),
    }
}

/// Référence à un fichier partagé, embarquée dans un message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRef {
    /// Racine de l'arbre de Merkle (identifiant du fichier).
    pub merkle_root: [u8; 32],
    /// Nom de fichier proposé.
    pub name: String,
    /// Taille totale en octets.
    pub size: u64,
    /// Type MIME déclaré.
    pub mime: String,
}

impl WireEncode for FileRef {
    fn encode(&self, w: &mut Writer) {
        w.put_arr(&self.merkle_root);
        w.put_str(&self.name);
        w.put_u64(self.size);
        w.put_str(&self.mime);
    }
}

impl WireDecode for FileRef {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        let merkle_root = r.arr()?;
        let name = r.str(MAX_NAME, "fileref.name")?;
        let size = r.u64()?;
        if size > limits::MAX_FILE_SIZE {
            return Err(DecodeError::TooLarge("fileref.size"));
        }
        let mime = r.str(MAX_NAME, "fileref.mime")?;
        Ok(FileRef {
            merkle_root,
            name,
            size,
            mime,
        })
    }
}

/// Contenu typé d'un message (direct ou de groupe, une fois déchiffré).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MsgBody {
    /// Message texte, éventuellement en réponse et avec pièces jointes.
    Text {
        /// Texte du message (Markdown côté UI).
        text: String,
        /// `msg_id` du message cité, le cas échéant.
        reply_to: Option<[u8; 16]>,
        /// Pièces jointes (≤ 10).
        attachments: Vec<FileRef>,
    },
    /// Édition d'un message antérieur de l'auteur.
    Edit {
        /// `msg_id` du message modifié.
        target: [u8; 16],
        /// Nouveau texte.
        new_text: String,
    },
    /// Suppression par l'auteur (tombstone).
    Delete {
        /// `msg_id` du message supprimé.
        target: [u8; 16],
    },
    /// Ajout ou retrait d'une réaction emoji.
    Reaction {
        /// `msg_id` du message ciblé.
        target: [u8; 16],
        /// Emoji (grappe de graphèmes UTF-8).
        emoji: String,
        /// 1 = ajout, 0 = retrait.
        add: bool,
    },
    /// Indicateur de saisie en cours (éphémère, non persisté).
    Typing,
    /// Accusé de lecture jusqu'à un message donné.
    ReadReceipt {
        /// Dernier `msg_id` lu.
        up_to: [u8; 16],
    },
    /// Envoi d'un sticker de serveur (D-047) : référence par nom + racine
    /// Merkle de l'image, exactement comme un émoji de serveur. Discriminant
    /// 4 (jamais utilisé jusqu'ici, entre `Reaction` et `Typing`).
    Sticker {
        /// Nom du sticker (2-32 caractères `[a-z0-9_]`, mêmes règles qu'un
        /// nom d'émoji).
        name: String,
        /// Racine Merkle de l'image (publiée dans le magasin de fichiers).
        merkle_root: [u8; 32],
    },
}

impl MsgBody {
    /// Discriminant filaire du corps (SPEC §6, champ `kind`).
    pub fn kind(&self) -> u8 {
        match self {
            MsgBody::Text { .. } => 0,
            MsgBody::Edit { .. } => 1,
            MsgBody::Delete { .. } => 2,
            MsgBody::Reaction { .. } => 3,
            MsgBody::Sticker { .. } => 4,
            MsgBody::Typing => 5,
            MsgBody::ReadReceipt { .. } => 6,
        }
    }

    /// Encode le corps seul (sans le discriminant, porté par l'en-tête).
    pub fn encode_body(&self) -> Vec<u8> {
        let mut w = Writer::new();
        match self {
            MsgBody::Text {
                text,
                reply_to,
                attachments,
            } => {
                w.put_str(text);
                w.put_opt(reply_to.as_ref(), |w, t| w.put_arr(t));
                w.put_list(attachments, |w, a| a.encode(w));
            }
            MsgBody::Edit { target, new_text } => {
                w.put_arr(target);
                w.put_str(new_text);
            }
            MsgBody::Delete { target } => w.put_arr(target),
            MsgBody::Reaction { target, emoji, add } => {
                w.put_arr(target);
                w.put_str(emoji);
                w.put_u8(u8::from(*add));
            }
            MsgBody::Sticker { name, merkle_root } => {
                w.put_str(name);
                w.put_arr(merkle_root);
            }
            MsgBody::Typing => {}
            MsgBody::ReadReceipt { up_to } => w.put_arr(up_to),
        }
        w.into_bytes()
    }

    /// Décode le corps d'après le discriminant de l'en-tête.
    pub fn decode_body(kind: u8, bytes: &[u8]) -> Result<Self, DecodeError> {
        let mut r = Reader::new(bytes);
        let body = match kind {
            0 => MsgBody::Text {
                text: r.str(limits::MAX_TEXT_BYTES, "msg.text")?,
                reply_to: r.opt(|r| r.arr())?,
                attachments: r.list(MAX_ATTACHMENTS, "msg.attachments", FileRef::decode)?,
            },
            1 => MsgBody::Edit {
                target: r.arr()?,
                new_text: r.str(limits::MAX_TEXT_BYTES, "msg.edit")?,
            },
            2 => MsgBody::Delete { target: r.arr()? },
            3 => MsgBody::Reaction {
                target: r.arr()?,
                emoji: r.str(MAX_EMOJI, "msg.emoji")?,
                add: match r.u8()? {
                    0 => false,
                    1 => true,
                    _ => return Err(DecodeError::InvalidValue("reaction.add")),
                },
            },
            4 => MsgBody::Sticker {
                name: r.str(MAX_EMOJI_NAME, "msg.sticker.name")?,
                merkle_root: r.arr()?,
            },
            5 => MsgBody::Typing,
            6 => MsgBody::ReadReceipt { up_to: r.arr()? },
            _ => return Err(DecodeError::InvalidValue("msg kind")),
        };
        r.finish()?;
        Ok(body)
    }
}

/// Opération signée du journal de groupe (SPEC §6.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupOp {
    /// Identifiant unique de l'opération.
    pub op_id: [u8; 16],
    /// Identifiant du groupe.
    pub group_id: [u8; 16],
    /// Horloge de Lamport de l'auteur.
    pub lamport: u64,
    /// Horloge murale (ms) informative.
    pub wall_ms: u64,
    /// Clé publique Ed25519 de l'auteur.
    pub author: [u8; 32],
    /// Discriminant du corps (voir [`GroupOpBody`]).
    pub kind: u8,
    /// Corps encodé (décodé via [`GroupOpBody::decode_body`]).
    pub body: Vec<u8>,
    /// Signature Ed25519 de l'auteur sur [`GroupOp::signable_bytes`].
    pub sig: [u8; 64],
}

impl GroupOp {
    /// Octets couverts par la signature de l'opération.
    pub fn signable_bytes(&self) -> Vec<u8> {
        let mut w = Writer::with_capacity(self.body.len() + 96);
        w.put_raw(b"accord-groupop-v1");
        w.put_arr(&self.op_id);
        w.put_arr(&self.group_id);
        w.put_u64(self.lamport);
        w.put_u64(self.wall_ms);
        w.put_arr(&self.author);
        w.put_u8(self.kind);
        w.put_lbytes(&self.body);
        w.into_bytes()
    }
}

impl WireEncode for GroupOp {
    fn encode(&self, w: &mut Writer) {
        w.put_arr(&self.op_id);
        w.put_arr(&self.group_id);
        w.put_u64(self.lamport);
        w.put_u64(self.wall_ms);
        w.put_arr(&self.author);
        w.put_u8(self.kind);
        w.put_lbytes(&self.body);
        w.put_arr(&self.sig);
    }
}

impl WireDecode for GroupOp {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(GroupOp {
            op_id: r.arr()?,
            group_id: r.arr()?,
            lamport: r.u64()?,
            wall_ms: r.u64()?,
            author: r.arr()?,
            kind: r.u8()?,
            body: r.lbytes(MAX_BODY, "groupop.body")?,
            sig: r.arr()?,
        })
    }
}

/// Nature d'un salon de groupe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ChannelKind {
    /// Salon textuel `#`.
    Text = 0,
    /// Salon vocal.
    Voice = 1,
    /// Salon d'annonces (lecture seule hors rôles autorisés).
    Announcement = 2,
}

impl ChannelKind {
    /// Décode le discriminant filaire.
    pub fn from_u8(v: u8) -> Result<Self, DecodeError> {
        match v {
            0 => Ok(Self::Text),
            1 => Ok(Self::Voice),
            2 => Ok(Self::Announcement),
            _ => Err(DecodeError::InvalidValue("channel kind")),
        }
    }
}

/// Corps typé d'une opération de groupe (SPEC §6.2, kinds 0x01–0x17).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroupOpBody {
    /// 0x01 — Création du groupe par le fondateur.
    Create {
        /// Nom du groupe.
        name: String,
    },
    /// 0x02 — Métadonnées (nom, icône, couleur de bannière).
    SetMeta {
        /// Nouveau nom.
        name: String,
        /// Hash Merkle de l'icône partagée, le cas échéant.
        icon: Option<[u8; 32]>,
        /// Couleur de bannière du serveur `0xRRGGBB`, le cas échéant
        /// (`None` = pas de couleur). Champ **additif** de fin de variant
        /// (D-047) : un émetteur antérieur à son introduction ne l'écrit
        /// pas du tout, et le décodage le rend à `None` dans ce cas
        /// ([`Reader::opt_tail`]) — même schéma de rétrocompatibilité que
        /// [`CoreMsg::Profile`].
        banner_color: Option<u32>,
    },
    /// 0x03 — Ajout d'un salon.
    AddChannel {
        /// Identifiant du salon.
        channel_id: [u8; 16],
        /// Nom affiché.
        name: String,
        /// Catégorie parente éventuelle.
        category: Option<[u8; 16]>,
        /// Nature du salon.
        kind: ChannelKind,
        /// Position de tri.
        position: u16,
    },
    /// 0x04 — Édition d'un salon.
    EditChannel {
        /// Salon visé.
        channel_id: [u8; 16],
        /// Nouveau nom.
        name: String,
        /// Nouvelle position.
        position: u16,
    },
    /// 0x05 — Suppression d'un salon.
    DelChannel {
        /// Salon supprimé.
        channel_id: [u8; 16],
    },
    /// 0x06 — Ajout d'une catégorie.
    AddCategory {
        /// Identifiant de catégorie.
        category_id: [u8; 16],
        /// Nom affiché.
        name: String,
        /// Position de tri.
        position: u16,
    },
    /// 0x07 — Admission d'un membre (après invitation).
    AddMember {
        /// Clé publique du nouveau membre.
        member: [u8; 32],
        /// Invitation utilisée, le cas échéant.
        invite_id: Option<[u8; 16]>,
    },
    /// 0x08 — Expulsion (peut revenir sur invitation).
    Kick {
        /// Membre expulsé.
        member: [u8; 32],
    },
    /// 0x09 — Bannissement définitif.
    Ban {
        /// Membre banni.
        member: [u8; 32],
    },
    /// 0x0A — Levée de bannissement.
    Unban {
        /// Membre réhabilité.
        member: [u8; 32],
    },
    /// 0x0B — Création de rôle.
    AddRole {
        /// Identifiant du rôle.
        role_id: [u8; 16],
        /// Nom affiché.
        name: String,
        /// Couleur RGB (0xRRGGBB).
        color: u32,
        /// Position hiérarchique (plus haut = plus fort).
        position: u16,
        /// Permissions accordées ([`perms`]).
        permissions: u32,
    },
    /// 0x0C — Édition de rôle.
    EditRole {
        /// Rôle visé.
        role_id: [u8; 16],
        /// Nouveau nom.
        name: String,
        /// Nouvelle couleur.
        color: u32,
        /// Nouvelle position.
        position: u16,
        /// Nouvelles permissions.
        permissions: u32,
    },
    /// 0x0D — Suppression de rôle.
    DelRole {
        /// Rôle supprimé.
        role_id: [u8; 16],
    },
    /// 0x0E — Attribution d'un rôle à un membre.
    AssignRole {
        /// Membre visé.
        member: [u8; 32],
        /// Rôle attribué.
        role_id: [u8; 16],
    },
    /// 0x0F — Retrait d'un rôle.
    UnassignRole {
        /// Membre visé.
        member: [u8; 32],
        /// Rôle retiré.
        role_id: [u8; 16],
    },
    /// 0x10 — Overrides de permissions par salon et rôle.
    SetChannelPerms {
        /// Salon visé.
        channel_id: [u8; 16],
        /// Rôle visé.
        role_id: [u8; 16],
        /// Permissions explicitement accordées.
        allow: u32,
        /// Permissions explicitement refusées (deny > allow).
        deny: u32,
    },
    /// 0x11 — Épinglage d'un message.
    Pin {
        /// Salon du message.
        channel_id: [u8; 16],
        /// Message épinglé.
        msg_id: [u8; 16],
    },
    /// 0x12 — Désépinglage.
    Unpin {
        /// Salon du message.
        channel_id: [u8; 16],
        /// Message désépinglé.
        msg_id: [u8; 16],
    },
    /// 0x13 — Suppression de modération (tombstone signée).
    DeleteMsg {
        /// Salon du message.
        channel_id: [u8; 16],
        /// Message supprimé.
        msg_id: [u8; 16],
    },
    /// 0x14 — Sujet du salon.
    SetTopic {
        /// Salon visé.
        channel_id: [u8; 16],
        /// Nouveau sujet.
        topic: String,
    },
    /// 0x15 — Création d'invitation.
    InviteCreate {
        /// Identifiant d'invitation.
        invite_id: [u8; 16],
        /// SHA-256 du secret d'invitation (le secret circule hors op-log).
        code_hash: [u8; 32],
        /// Utilisations maximales (0 = illimité).
        max_uses: u32,
        /// Expiration murale ms (0 = jamais).
        expires_ms: u64,
    },
    /// 0x16 — Révocation d'invitation.
    InviteRevoke {
        /// Invitation révoquée.
        invite_id: [u8; 16],
    },
    /// 0x17 — Départ volontaire de l'auteur.
    Leave,
    /// 0x18 — Ajout ou remplacement d'un émoji de serveur (SPEC §6.2). Un
    /// `AddEmoji` sur un nom existant met à jour l'image associée.
    AddEmoji {
        /// Nom court `[a-z0-9_]` (2-32 caractères, validé au repli).
        name: String,
        /// Racine Merkle de l'image (publiée dans le magasin de fichiers).
        file: [u8; 32],
    },
    /// 0x19 — Suppression d'un émoji de serveur.
    DelEmoji {
        /// Nom de l'émoji retiré.
        name: String,
    },
    /// 0x1A — Category rename/reposition (SPEC §6.2).
    EditCategory {
        /// Target category.
        category_id: [u8; 16],
        /// New display name.
        name: String,
        /// New sort position.
        position: u16,
    },
    /// 0x1B — Category deletion; its channels become uncategorized.
    DelCategory {
        /// Deleted category.
        category_id: [u8; 16],
    },
    /// 0x1C — Move a channel into a category (`None` = uncategorized).
    SetChannelCategory {
        /// Target channel.
        channel_id: [u8; 16],
        /// New parent category, if any.
        category: Option<[u8; 16]>,
    },
    /// 0x1D — Temporary mute: silence `member` until wall time `until_ms`
    /// (`0` clears). Gated on `KICK` and the kick hierarchy at replay; the
    /// member stays in the group but cannot send messages while active.
    TimeoutMember {
        /// Silenced member.
        member: [u8; 32],
        /// Deadline (wall ms); `0` lifts the timeout.
        until_ms: u64,
    },
    /// 0x1E — Per-group display name for `member` (empty clears). A member sets
    /// their own; a `MANAGE_ROLES` moderator sets anyone strictly below them.
    SetNickname {
        /// Target member.
        member: [u8; 32],
        /// Nickname (1-32 characters trimmed; empty clears).
        name: String,
    },
    /// 0x1F — Server-side voice moderation: force-mute and/or force-deafen
    /// `member` in every voice channel of the group (`mute == deafen == false`
    /// clears the entry). Gated on `KICK` and the kick hierarchy at replay,
    /// exactly like [`GroupOpBody::TimeoutMember`]; enforced by the voice
    /// engine of every honest peer (capture gated on the target, frames from
    /// a muted member dropped on receipt).
    VoiceModerate {
        /// Moderated member.
        member: [u8; 32],
        /// Microphone force-muted.
        mute: bool,
        /// Output force-deafened (implies mute, Discord semantics).
        deafen: bool,
    },
    /// 0x20 — Create a scheduled event (D-047). Gated on `MANAGE_CHANNELS` at
    /// replay, like every other server-management op; the author is
    /// recorded so they can always edit/delete their own event afterwards.
    EventCreate {
        /// New event identifier ([`crate::new_id16`]-style random id, minted
        /// by the caller — mirrors channel/category/role id allocation).
        event_id: [u8; 16],
        /// Display title (2-100 characters, anti-spoofing checked).
        title: String,
        /// Optional description (≤1024 characters, control-char checked
        /// like a profile bio).
        description: String,
        /// Wall-clock start time (ms); bounded like
        /// [`TimeoutMember::until_ms`](GroupOpBody::TimeoutMember).
        start_ms: u64,
        /// Optional voice channel where the event takes place.
        channel_id: Option<[u8; 16]>,
    },
    /// 0x21 — Edit an existing event's fields (author or `MANAGE_CHANNELS`
    /// at replay); RSVPs are untouched.
    EventEdit {
        /// Target event.
        event_id: [u8; 16],
        /// New title (same validation as [`GroupOpBody::EventCreate`]).
        title: String,
        /// New description.
        description: String,
        /// New start time (ms).
        start_ms: u64,
        /// New voice channel, if any.
        channel_id: Option<[u8; 16]>,
    },
    /// 0x22 — Delete an event (author or `MANAGE_CHANNELS` at replay).
    EventDelete {
        /// Deleted event.
        event_id: [u8; 16],
    },
    /// 0x23 — RSVP to an event: any member may toggle their own
    /// "interested" status. Deduplicated by `(event_id, member)` at replay
    /// (a later RSVP from the same member simply overwrites the earlier
    /// one — plain map semantics, no explicit bookkeeping needed).
    EventRsvp {
        /// Target event.
        event_id: [u8; 16],
        /// `true` = interested (recorded), `false` = withdrawn (cleared).
        interested: bool,
    },
    /// 0x24 — Add or replace a server sticker (mirrors
    /// [`GroupOpBody::AddEmoji`] exactly, including replace-on-existing-name
    /// semantics and permission gate).
    StickerAdd {
        /// Short name `[a-z0-9_]` (2-32 characters, validated at replay).
        name: String,
        /// Merkle root of the image (published in the file store).
        file: [u8; 32],
    },
    /// 0x25 — Remove a server sticker (mirrors [`GroupOpBody::DelEmoji`]).
    StickerRemove {
        /// Removed sticker's name.
        name: String,
    },
    /// 0x26 — Set or clear the caller's own per-server avatar. Self-service
    /// only — unlike [`GroupOpBody::SetNickname`], there is no moderator
    /// override, so the target is always the author and no `member` field
    /// is needed.
    SetMemberAvatar {
        /// Merkle root of the new avatar image, or `None` to clear it.
        avatar: Option<[u8; 32]>,
    },
}

impl GroupOpBody {
    /// Discriminant filaire de l'opération.
    pub fn kind(&self) -> u8 {
        match self {
            Self::Create { .. } => 0x01,
            Self::SetMeta { .. } => 0x02,
            Self::AddChannel { .. } => 0x03,
            Self::EditChannel { .. } => 0x04,
            Self::DelChannel { .. } => 0x05,
            Self::AddCategory { .. } => 0x06,
            Self::AddMember { .. } => 0x07,
            Self::Kick { .. } => 0x08,
            Self::Ban { .. } => 0x09,
            Self::Unban { .. } => 0x0A,
            Self::AddRole { .. } => 0x0B,
            Self::EditRole { .. } => 0x0C,
            Self::DelRole { .. } => 0x0D,
            Self::AssignRole { .. } => 0x0E,
            Self::UnassignRole { .. } => 0x0F,
            Self::SetChannelPerms { .. } => 0x10,
            Self::Pin { .. } => 0x11,
            Self::Unpin { .. } => 0x12,
            Self::DeleteMsg { .. } => 0x13,
            Self::SetTopic { .. } => 0x14,
            Self::InviteCreate { .. } => 0x15,
            Self::InviteRevoke { .. } => 0x16,
            Self::Leave => 0x17,
            Self::AddEmoji { .. } => 0x18,
            Self::DelEmoji { .. } => 0x19,
            Self::EditCategory { .. } => 0x1A,
            Self::DelCategory { .. } => 0x1B,
            Self::SetChannelCategory { .. } => 0x1C,
            Self::TimeoutMember { .. } => 0x1D,
            Self::SetNickname { .. } => 0x1E,
            Self::VoiceModerate { .. } => 0x1F,
            Self::EventCreate { .. } => 0x20,
            Self::EventEdit { .. } => 0x21,
            Self::EventDelete { .. } => 0x22,
            Self::EventRsvp { .. } => 0x23,
            Self::StickerAdd { .. } => 0x24,
            Self::StickerRemove { .. } => 0x25,
            Self::SetMemberAvatar { .. } => 0x26,
        }
    }

    /// Encode le corps seul (sans discriminant).
    pub fn encode_body(&self) -> Vec<u8> {
        let mut w = Writer::new();
        match self {
            Self::Create { name } => w.put_str(name),
            Self::SetMeta {
                name,
                icon,
                banner_color,
            } => {
                w.put_str(name);
                w.put_opt(icon.as_ref(), |w, h| w.put_arr(h));
                w.put_opt(banner_color.as_ref(), |w, c| w.put_u32(*c));
            }
            Self::AddChannel {
                channel_id,
                name,
                category,
                kind,
                position,
            } => {
                w.put_arr(channel_id);
                w.put_str(name);
                w.put_opt(category.as_ref(), |w, c| w.put_arr(c));
                w.put_u8(*kind as u8);
                w.put_u16(*position);
            }
            Self::EditChannel {
                channel_id,
                name,
                position,
            } => {
                w.put_arr(channel_id);
                w.put_str(name);
                w.put_u16(*position);
            }
            Self::DelChannel { channel_id } => w.put_arr(channel_id),
            Self::AddCategory {
                category_id,
                name,
                position,
            } => {
                w.put_arr(category_id);
                w.put_str(name);
                w.put_u16(*position);
            }
            Self::AddMember { member, invite_id } => {
                w.put_arr(member);
                w.put_opt(invite_id.as_ref(), |w, i| w.put_arr(i));
            }
            Self::Kick { member } | Self::Ban { member } | Self::Unban { member } => {
                w.put_arr(member)
            }
            Self::AddRole {
                role_id,
                name,
                color,
                position,
                permissions,
            }
            | Self::EditRole {
                role_id,
                name,
                color,
                position,
                permissions,
            } => {
                w.put_arr(role_id);
                w.put_str(name);
                w.put_u32(*color);
                w.put_u16(*position);
                w.put_u32(*permissions);
            }
            Self::DelRole { role_id } => w.put_arr(role_id),
            Self::AssignRole { member, role_id } | Self::UnassignRole { member, role_id } => {
                w.put_arr(member);
                w.put_arr(role_id);
            }
            Self::SetChannelPerms {
                channel_id,
                role_id,
                allow,
                deny,
            } => {
                w.put_arr(channel_id);
                w.put_arr(role_id);
                w.put_u32(*allow);
                w.put_u32(*deny);
            }
            Self::Pin { channel_id, msg_id }
            | Self::Unpin { channel_id, msg_id }
            | Self::DeleteMsg { channel_id, msg_id } => {
                w.put_arr(channel_id);
                w.put_arr(msg_id);
            }
            Self::SetTopic { channel_id, topic } => {
                w.put_arr(channel_id);
                w.put_str(topic);
            }
            Self::InviteCreate {
                invite_id,
                code_hash,
                max_uses,
                expires_ms,
            } => {
                w.put_arr(invite_id);
                w.put_arr(code_hash);
                w.put_u32(*max_uses);
                w.put_u64(*expires_ms);
            }
            Self::InviteRevoke { invite_id } => w.put_arr(invite_id),
            Self::Leave => {}
            Self::AddEmoji { name, file } => {
                w.put_str(name);
                w.put_arr(file);
            }
            Self::DelEmoji { name } => w.put_str(name),
            Self::EditCategory {
                category_id,
                name,
                position,
            } => {
                w.put_arr(category_id);
                w.put_str(name);
                w.put_u16(*position);
            }
            Self::DelCategory { category_id } => w.put_arr(category_id),
            Self::SetChannelCategory {
                channel_id,
                category,
            } => {
                w.put_arr(channel_id);
                w.put_opt(category.as_ref(), |w, c| w.put_arr(c));
            }
            Self::TimeoutMember { member, until_ms } => {
                w.put_arr(member);
                w.put_u64(*until_ms);
            }
            Self::SetNickname { member, name } => {
                w.put_arr(member);
                w.put_str(name);
            }
            Self::VoiceModerate {
                member,
                mute,
                deafen,
            } => {
                w.put_arr(member);
                w.put_u8(u8::from(*mute));
                w.put_u8(u8::from(*deafen));
            }
            Self::EventCreate {
                event_id,
                title,
                description,
                start_ms,
                channel_id,
            }
            | Self::EventEdit {
                event_id,
                title,
                description,
                start_ms,
                channel_id,
            } => {
                w.put_arr(event_id);
                w.put_str(title);
                w.put_str(description);
                w.put_u64(*start_ms);
                w.put_opt(channel_id.as_ref(), |w, c| w.put_arr(c));
            }
            Self::EventDelete { event_id } => w.put_arr(event_id),
            Self::EventRsvp {
                event_id,
                interested,
            } => {
                w.put_arr(event_id);
                w.put_u8(u8::from(*interested));
            }
            Self::StickerAdd { name, file } => {
                w.put_str(name);
                w.put_arr(file);
            }
            Self::StickerRemove { name } => w.put_str(name),
            Self::SetMemberAvatar { avatar } => {
                w.put_opt(avatar.as_ref(), |w, h| w.put_arr(h));
            }
        }
        w.into_bytes()
    }

    /// Décode le corps d'après le discriminant.
    pub fn decode_body(kind: u8, bytes: &[u8]) -> Result<Self, DecodeError> {
        let mut r = Reader::new(bytes);
        let body = match kind {
            0x01 => Self::Create {
                name: r.str(MAX_NAME, "op.name")?,
            },
            0x02 => Self::SetMeta {
                name: r.str(MAX_NAME, "op.name")?,
                icon: r.opt(|r| r.arr())?,
                banner_color: r
                    .opt_tail(|r| decode_profile_color(r, "op.set_meta.banner_color"))?,
            },
            0x03 => Self::AddChannel {
                channel_id: r.arr()?,
                name: r.str(MAX_NAME, "op.name")?,
                category: r.opt(|r| r.arr())?,
                kind: ChannelKind::from_u8(r.u8()?)?,
                position: r.u16()?,
            },
            0x04 => Self::EditChannel {
                channel_id: r.arr()?,
                name: r.str(MAX_NAME, "op.name")?,
                position: r.u16()?,
            },
            0x05 => Self::DelChannel {
                channel_id: r.arr()?,
            },
            0x06 => Self::AddCategory {
                category_id: r.arr()?,
                name: r.str(MAX_NAME, "op.name")?,
                position: r.u16()?,
            },
            0x07 => Self::AddMember {
                member: r.arr()?,
                invite_id: r.opt(|r| r.arr())?,
            },
            0x08 => Self::Kick { member: r.arr()? },
            0x09 => Self::Ban { member: r.arr()? },
            0x0A => Self::Unban { member: r.arr()? },
            0x0B => Self::AddRole {
                role_id: r.arr()?,
                name: r.str(MAX_NAME, "op.name")?,
                color: r.u32()?,
                position: r.u16()?,
                permissions: r.u32()?,
            },
            0x0C => Self::EditRole {
                role_id: r.arr()?,
                name: r.str(MAX_NAME, "op.name")?,
                color: r.u32()?,
                position: r.u16()?,
                permissions: r.u32()?,
            },
            0x0D => Self::DelRole { role_id: r.arr()? },
            0x0E => Self::AssignRole {
                member: r.arr()?,
                role_id: r.arr()?,
            },
            0x0F => Self::UnassignRole {
                member: r.arr()?,
                role_id: r.arr()?,
            },
            0x10 => Self::SetChannelPerms {
                channel_id: r.arr()?,
                role_id: r.arr()?,
                allow: r.u32()?,
                deny: r.u32()?,
            },
            0x11 => Self::Pin {
                channel_id: r.arr()?,
                msg_id: r.arr()?,
            },
            0x12 => Self::Unpin {
                channel_id: r.arr()?,
                msg_id: r.arr()?,
            },
            0x13 => Self::DeleteMsg {
                channel_id: r.arr()?,
                msg_id: r.arr()?,
            },
            0x14 => Self::SetTopic {
                channel_id: r.arr()?,
                topic: r.str(MAX_BIO, "op.topic")?,
            },
            0x15 => Self::InviteCreate {
                invite_id: r.arr()?,
                code_hash: r.arr()?,
                max_uses: r.u32()?,
                expires_ms: r.u64()?,
            },
            0x16 => Self::InviteRevoke {
                invite_id: r.arr()?,
            },
            0x17 => Self::Leave,
            0x18 => Self::AddEmoji {
                name: r.str(MAX_EMOJI_NAME, "op.emoji.name")?,
                file: r.arr()?,
            },
            0x19 => Self::DelEmoji {
                name: r.str(MAX_EMOJI_NAME, "op.emoji.name")?,
            },
            0x1A => Self::EditCategory {
                category_id: r.arr()?,
                name: r.str(MAX_NAME, "op.name")?,
                position: r.u16()?,
            },
            0x1B => Self::DelCategory {
                category_id: r.arr()?,
            },
            0x1C => Self::SetChannelCategory {
                channel_id: r.arr()?,
                category: r.opt(|r| r.arr())?,
            },
            0x1D => Self::TimeoutMember {
                member: r.arr()?,
                until_ms: r.u64()?,
            },
            0x1E => Self::SetNickname {
                member: r.arr()?,
                name: r.str(MAX_NICKNAME, "op.nickname")?,
            },
            0x1F => Self::VoiceModerate {
                member: r.arr()?,
                mute: decode_bool(&mut r, "op.voice_moderate.mute")?,
                deafen: decode_bool(&mut r, "op.voice_moderate.deafen")?,
            },
            0x20 => Self::EventCreate {
                event_id: r.arr()?,
                title: r.str(MAX_EVENT_TITLE, "op.event.title")?,
                description: r.str(MAX_EVENT_DESC, "op.event.description")?,
                start_ms: r.u64()?,
                channel_id: r.opt(|r| r.arr())?,
            },
            0x21 => Self::EventEdit {
                event_id: r.arr()?,
                title: r.str(MAX_EVENT_TITLE, "op.event.title")?,
                description: r.str(MAX_EVENT_DESC, "op.event.description")?,
                start_ms: r.u64()?,
                channel_id: r.opt(|r| r.arr())?,
            },
            0x22 => Self::EventDelete { event_id: r.arr()? },
            0x23 => Self::EventRsvp {
                event_id: r.arr()?,
                interested: decode_bool(&mut r, "op.event_rsvp.interested")?,
            },
            0x24 => Self::StickerAdd {
                name: r.str(MAX_EMOJI_NAME, "op.sticker.name")?,
                file: r.arr()?,
            },
            0x25 => Self::StickerRemove {
                name: r.str(MAX_EMOJI_NAME, "op.sticker.name")?,
            },
            0x26 => Self::SetMemberAvatar {
                avatar: r.opt(|r| r.arr())?,
            },
            _ => return Err(DecodeError::InvalidValue("groupop kind")),
        };
        r.finish()?;
        Ok(body)
    }
}

/// Message du canal CORE.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreMsg {
    /// 0x01 — Message direct (corps en clair dans la session E2E).
    DirectMsg {
        /// Identifiant unique du message.
        msg_id: [u8; 16],
        /// Horloge de Lamport de l'expéditeur.
        lamport: u64,
        /// Horloge murale d'envoi (ms).
        sent_ms: u64,
        /// Discriminant du corps ([`MsgBody::kind`]).
        kind: u8,
        /// Corps encodé ([`MsgBody`]).
        body: Vec<u8>,
    },
    /// 0x02 — Accusé de réception applicatif.
    MsgAck {
        /// Message acquitté.
        msg_id: [u8; 16],
    },
    /// 0x03 — Demande d'ami.
    FriendRequest {
        /// Pseudo affiché du demandeur.
        display_name: String,
        /// Message d'accompagnement.
        message: String,
        /// Phrase de vérification hors bande optionnelle.
        verify_phrase: Option<String>,
    },
    /// 0x04 — Réponse à une demande d'ami.
    FriendResponse {
        /// Acceptée ou refusée.
        accepted: bool,
    },
    /// 0x05 — Opération de groupe répliquée.
    GroupOpMsg {
        /// Opération signée.
        op: GroupOp,
    },
    /// 0x06 — Message de groupe (chiffré par la clé de groupe).
    GroupMsg {
        /// Groupe visé.
        group_id: [u8; 16],
        /// Salon visé.
        channel_id: [u8; 16],
        /// Identifiant unique du message.
        msg_id: [u8; 16],
        /// Horloge de Lamport.
        lamport: u64,
        /// Horloge murale d'envoi (ms).
        sent_ms: u64,
        /// Epoch de clé de groupe utilisé.
        key_epoch: u32,
        /// `nonce(24) ‖ AEAD(corps)` sous la clé de groupe.
        body_enc: Vec<u8>,
    },
    /// 0x07 — Distribution de clé de groupe scellée (SPEC §6.4).
    GroupKey {
        /// Groupe visé.
        group_id: [u8; 16],
        /// Epoch distribué.
        key_epoch: u32,
        /// `eph_pub(32) ‖ box(48)` scellé pour le destinataire.
        sealed_key: [u8; 80],
    },
    /// 0x08 — Présence/statut.
    Presence {
        /// 0=en ligne, 1=inactif, 2=ne pas déranger, 3=hors ligne.
        status: u8,
        /// Statut personnalisé.
        custom: Option<String>,
    },
    /// 0x09 — Profil public (D-027 : seul le pseudo est exploité aujourd'hui,
    /// annoncé aux amis à chaque changement et à l'établissement d'une
    /// amitié ; bio/avatar/bannière réservés, versionnables).
    ///
    /// `pronouns`/`accent_color`/`banner_color` sont des champs **additifs**
    /// ajoutés en fin de variant : un émetteur plus ancien ne les écrit pas
    /// du tout, et le décodage les rend à `None` dans ce cas
    /// ([`Reader::opt_tail`]) — rétrocompatibilité filaire garantie.
    Profile {
        /// Pseudo affiché (≤ 128 octets UTF-8 au décodage ; 2 à 32
        /// caractères après trim exigés à l'ingestion).
        display_name: String,
        /// Biographie.
        bio: String,
        /// Avatar (racine Merkle) le cas échéant.
        avatar: Option<[u8; 32]>,
        /// Bannière (racine Merkle) le cas échéant.
        banner: Option<[u8; 32]>,
        /// Pronoms affichés (≤ 40 octets UTF-8 au décodage), champ additif ;
        /// absent chez un émetteur plus ancien (décodé à `None`).
        pronouns: Option<String>,
        /// Couleur d'accent `0xRRGGBB`, champ additif ; absent chez un
        /// émetteur plus ancien (décodé à `None`), rejetée au décodage
        /// au-delà de 24 bits.
        accent_color: Option<u32>,
        /// Couleur de bannière `0xRRGGBB` (l'image de bannière prime sur
        /// cette couleur à l'affichage, règle côté client), champ additif ;
        /// absent chez un émetteur plus ancien (décodé à `None`), rejetée au
        /// décodage au-delà de 24 bits.
        banner_color: Option<u32>,
    },
    /// 0x0A — Signalisation de salon vocal.
    VoiceSignal {
        /// Groupe du salon.
        group_id: [u8; 16],
        /// Salon vocal visé.
        channel_id: [u8; 16],
        /// 0=rejoint, 1=quitte, 2=état.
        action: u8,
        /// Bitflags des médias actifs (0x01 audio ; vidéo/écran réservés).
        media_kinds: u8,
        /// Micro coupé.
        mute: bool,
    },
    /// 0x0B — Synchronisation d'op-log : état connu de l'émetteur.
    GroupSync {
        /// Groupe visé.
        group_id: [u8; 16],
        /// Lamport max connu.
        max_lamport: u64,
        /// Nombre d'ops connues.
        op_count: u32,
        /// Digest de l'op-log ordonné (SHA-256 des op_id concaténés).
        digest: [u8; 32],
    },
    /// 0x0C — Demande des ops manquantes après GroupSync.
    GroupSyncPull {
        /// Groupe visé.
        group_id: [u8; 16],
        /// Renvoyer les ops de lamport > ce seuil.
        since_lamport: u64,
    },
    /// 0x0D — Friendship removal. The sender (authenticated by the encrypted
    /// session) removed the friendship on their side; the receiver drops it
    /// too and keeps the DM history. Best-effort: never queued offline.
    FriendRemove,
    /// 0x0E — Invitation signée envoyée par l'inviteur à UN invité précis
    /// (consentement explicite requis avant toute matérialisation, D-045).
    /// Transporte la préimage de l'op `InviteCreate` répliquée : seul le
    /// destinataire de ce message peut ensuite prouver son consentement via
    /// `InviteAccept`.
    InviteTicket {
        /// Groupe concerné.
        group_id: [u8; 16],
        /// Invitation correspondant à l'op `InviteCreate` répliquée.
        invite_id: [u8; 16],
        /// Nom du groupe au moment de l'invitation (affichage avant adhésion,
        /// l'invité n'a pas encore l'état matérialisé pour le lire lui-même).
        group_name: String,
        /// Clé publique Ed25519 de l'inviteur (signataire).
        inviter: [u8; 32],
        /// Secret d'invitation (préimage de `code_hash` porté par l'op
        /// `InviteCreate` correspondante).
        secret: [u8; 32],
        /// Expiration murale ms du ticket (0 = jamais).
        expires_ms: u64,
        /// Signature Ed25519 de l'inviteur sur les autres champs
        /// ([`invite_ticket_signable_bytes`]).
        sig: [u8; 64],
    },
    /// 0x0F — Acceptation d'une invitation par l'invité : preuve de
    /// consentement explicite envoyée à l'inviteur, qui peut alors admettre
    /// le membre (op `AddMember`) et lui pousser l'op-log et la clé de
    /// groupe. Ne matérialise rien à elle seule côté réseau.
    InviteAccept {
        /// Groupe concerné.
        group_id: [u8; 16],
        /// Invitation acceptée.
        invite_id: [u8; 16],
        /// Secret reçu dans le `InviteTicket` correspondant.
        secret: [u8; 32],
    },
    /// 0x10 — Refus d'une invitation par l'invité (best-effort, informatif ;
    /// l'inviteur efface son suivi local le cas échéant).
    InviteDecline {
        /// Groupe concerné.
        group_id: [u8; 16],
        /// Invitation refusée.
        invite_id: [u8; 16],
    },
    /// 0x11 — Offre d'appel vocal 1-à-1 (sonnerie). Éphémère : jamais mise en
    /// file hors-ligne. L'appelé n'honore l'offre que d'un AMI confirmé et
    /// sous cadence par pair (anti sonnerie-spam) ; l'appelant réémet l'offre
    /// périodiquement tant que ça sonne (transport UDP avec pertes), le
    /// destinataire déduplique par `call_id`.
    CallOffer {
        /// Identifiant d'appel, tiré aléatoirement par l'appelant. Sert aussi
        /// de `room` aux trames du canal VOICE une fois l'appel accepté.
        call_id: [u8; 16],
    },
    /// 0x12 — Acceptation d'une offre d'appel : la session audio démarre des
    /// deux côtés (trames VOICE, `room == call_id`). Ignorée si elle ne
    /// corrèle pas une offre sortante fraîche (anti-rejeu).
    CallAnswer {
        /// Appel accepté.
        call_id: [u8; 16],
    },
    /// 0x13 — Refus d'une offre d'appel. `reason` : 0 = refusé par
    /// l'utilisateur, 1 = occupé (déjà en appel). Ignoré s'il ne corrèle pas
    /// une offre sortante fraîche.
    CallDecline {
        /// Appel refusé.
        call_id: [u8; 16],
        /// 0 = refusé, 1 = occupé.
        reason: u8,
    },
    /// 0x14 — Fin d'appel : raccrochage d'un appel actif ou annulation d'une
    /// sonnerie par l'appelant. Ignoré s'il ne corrèle pas l'appel courant.
    CallHangup {
        /// Appel terminé.
        call_id: [u8; 16],
    },
}

/// Raison d'un [`CoreMsg::CallDecline`] : refus explicite de l'utilisateur.
pub const CALL_DECLINE_REJECTED: u8 = 0;
/// Raison d'un [`CoreMsg::CallDecline`] : destinataire déjà en appel.
pub const CALL_DECLINE_BUSY: u8 = 1;

/// Octets couverts par la signature d'un `CoreMsg::InviteTicket` (hors
/// `sig`) : domaine séparé du reste du protocole, encodage canonique stable.
pub fn invite_ticket_signable_bytes(
    group_id: &[u8; 16],
    invite_id: &[u8; 16],
    group_name: &str,
    inviter: &[u8; 32],
    secret: &[u8; 32],
    expires_ms: u64,
) -> Vec<u8> {
    let mut w = Writer::with_capacity(16 + 16 + group_name.len() + 32 + 32 + 8 + 24);
    w.put_raw(b"accord-invite-ticket-v1");
    w.put_arr(group_id);
    w.put_arr(invite_id);
    w.put_str(group_name);
    w.put_arr(inviter);
    w.put_arr(secret);
    w.put_u64(expires_ms);
    w.into_bytes()
}

impl WireEncode for CoreMsg {
    fn encode(&self, w: &mut Writer) {
        match self {
            CoreMsg::DirectMsg {
                msg_id,
                lamport,
                sent_ms,
                kind,
                body,
            } => {
                w.put_u8(0x01);
                w.put_arr(msg_id);
                w.put_u64(*lamport);
                w.put_u64(*sent_ms);
                w.put_u8(*kind);
                w.put_lbytes(body);
            }
            CoreMsg::MsgAck { msg_id } => {
                w.put_u8(0x02);
                w.put_arr(msg_id);
            }
            CoreMsg::FriendRequest {
                display_name,
                message,
                verify_phrase,
            } => {
                w.put_u8(0x03);
                w.put_str(display_name);
                w.put_str(message);
                w.put_opt(verify_phrase.as_ref(), |w, p| w.put_str(p));
            }
            CoreMsg::FriendResponse { accepted } => {
                w.put_u8(0x04);
                w.put_u8(u8::from(*accepted));
            }
            CoreMsg::GroupOpMsg { op } => {
                w.put_u8(0x05);
                op.encode(w);
            }
            CoreMsg::GroupMsg {
                group_id,
                channel_id,
                msg_id,
                lamport,
                sent_ms,
                key_epoch,
                body_enc,
            } => {
                w.put_u8(0x06);
                w.put_arr(group_id);
                w.put_arr(channel_id);
                w.put_arr(msg_id);
                w.put_u64(*lamport);
                w.put_u64(*sent_ms);
                w.put_u32(*key_epoch);
                w.put_lbytes(body_enc);
            }
            CoreMsg::GroupKey {
                group_id,
                key_epoch,
                sealed_key,
            } => {
                w.put_u8(0x07);
                w.put_arr(group_id);
                w.put_u32(*key_epoch);
                w.put_arr(sealed_key);
            }
            CoreMsg::Presence { status, custom } => {
                w.put_u8(0x08);
                w.put_u8(*status);
                w.put_opt(custom.as_ref(), |w, c| w.put_str(c));
            }
            CoreMsg::Profile {
                display_name,
                bio,
                avatar,
                banner,
                pronouns,
                accent_color,
                banner_color,
            } => {
                w.put_u8(0x09);
                w.put_str(display_name);
                w.put_str(bio);
                w.put_opt(avatar.as_ref(), |w, h| w.put_arr(h));
                w.put_opt(banner.as_ref(), |w, h| w.put_arr(h));
                w.put_opt(pronouns.as_ref(), |w, p| w.put_str(p));
                w.put_opt(accent_color.as_ref(), |w, c| w.put_u32(*c));
                w.put_opt(banner_color.as_ref(), |w, c| w.put_u32(*c));
            }
            CoreMsg::VoiceSignal {
                group_id,
                channel_id,
                action,
                media_kinds,
                mute,
            } => {
                w.put_u8(0x0A);
                w.put_arr(group_id);
                w.put_arr(channel_id);
                w.put_u8(*action);
                w.put_u8(*media_kinds);
                w.put_u8(u8::from(*mute));
            }
            CoreMsg::GroupSync {
                group_id,
                max_lamport,
                op_count,
                digest,
            } => {
                w.put_u8(0x0B);
                w.put_arr(group_id);
                w.put_u64(*max_lamport);
                w.put_u32(*op_count);
                w.put_arr(digest);
            }
            CoreMsg::GroupSyncPull {
                group_id,
                since_lamport,
            } => {
                w.put_u8(0x0C);
                w.put_arr(group_id);
                w.put_u64(*since_lamport);
            }
            CoreMsg::FriendRemove => {
                w.put_u8(0x0D);
            }
            CoreMsg::InviteTicket {
                group_id,
                invite_id,
                group_name,
                inviter,
                secret,
                expires_ms,
                sig,
            } => {
                w.put_u8(0x0E);
                w.put_arr(group_id);
                w.put_arr(invite_id);
                w.put_str(group_name);
                w.put_arr(inviter);
                w.put_arr(secret);
                w.put_u64(*expires_ms);
                w.put_arr(sig);
            }
            CoreMsg::InviteAccept {
                group_id,
                invite_id,
                secret,
            } => {
                w.put_u8(0x0F);
                w.put_arr(group_id);
                w.put_arr(invite_id);
                w.put_arr(secret);
            }
            CoreMsg::InviteDecline {
                group_id,
                invite_id,
            } => {
                w.put_u8(0x10);
                w.put_arr(group_id);
                w.put_arr(invite_id);
            }
            CoreMsg::CallOffer { call_id } => {
                w.put_u8(0x11);
                w.put_arr(call_id);
            }
            CoreMsg::CallAnswer { call_id } => {
                w.put_u8(0x12);
                w.put_arr(call_id);
            }
            CoreMsg::CallDecline { call_id, reason } => {
                w.put_u8(0x13);
                w.put_arr(call_id);
                w.put_u8(*reason);
            }
            CoreMsg::CallHangup { call_id } => {
                w.put_u8(0x14);
                w.put_arr(call_id);
            }
        }
    }
}

/// Décode une couleur `0xRRGGBB` : rejette strictement tout ce qui dépasse 24
/// bits. Partagée par `profile.accent_color`/`profile.banner_color`
/// ([`CoreMsg::Profile`]) et `op.set_meta.banner_color`
/// ([`GroupOpBody::SetMeta`], D-047) — même format, même borne.
fn decode_profile_color(r: &mut Reader<'_>, what: &'static str) -> Result<u32, DecodeError> {
    let v = r.u32()?;
    if v > MAX_COLOR {
        return Err(DecodeError::InvalidValue(what));
    }
    Ok(v)
}

impl WireDecode for CoreMsg {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        match r.u8()? {
            0x01 => Ok(CoreMsg::DirectMsg {
                msg_id: r.arr()?,
                lamport: r.u64()?,
                sent_ms: r.u64()?,
                kind: r.u8()?,
                body: r.lbytes(MAX_BODY, "direct.body")?,
            }),
            0x02 => Ok(CoreMsg::MsgAck { msg_id: r.arr()? }),
            0x03 => Ok(CoreMsg::FriendRequest {
                display_name: r.str(MAX_NAME, "friend.name")?,
                message: r.str(MAX_BIO, "friend.msg")?,
                verify_phrase: r.opt(|r| r.str(MAX_NAME, "friend.verify"))?,
            }),
            0x04 => Ok(CoreMsg::FriendResponse {
                accepted: match r.u8()? {
                    0 => false,
                    1 => true,
                    _ => return Err(DecodeError::InvalidValue("friend.accepted")),
                },
            }),
            0x05 => Ok(CoreMsg::GroupOpMsg {
                op: GroupOp::decode(r)?,
            }),
            0x06 => Ok(CoreMsg::GroupMsg {
                group_id: r.arr()?,
                channel_id: r.arr()?,
                msg_id: r.arr()?,
                lamport: r.u64()?,
                sent_ms: r.u64()?,
                key_epoch: r.u32()?,
                body_enc: r.lbytes(MAX_BODY, "group.body")?,
            }),
            0x07 => Ok(CoreMsg::GroupKey {
                group_id: r.arr()?,
                key_epoch: r.u32()?,
                sealed_key: r.arr()?,
            }),
            0x08 => Ok(CoreMsg::Presence {
                status: {
                    let s = r.u8()?;
                    if s > 3 {
                        return Err(DecodeError::InvalidValue("presence.status"));
                    }
                    s
                },
                custom: r.opt(|r| r.str(MAX_NAME, "presence.custom"))?,
            }),
            0x09 => Ok(CoreMsg::Profile {
                display_name: r.str(MAX_PROFILE_NAME, "profile.name")?,
                bio: r.str(MAX_BIO, "profile.bio")?,
                avatar: r.opt(|r| r.arr())?,
                banner: r.opt(|r| r.arr())?,
                // Champs additifs de fin de variant : absents chez un
                // émetteur plus ancien → `None` (rétrocompatibilité).
                pronouns: r.opt_tail(|r| r.str(MAX_PRONOUNS, "profile.pronouns"))?,
                accent_color: r.opt_tail(|r| decode_profile_color(r, "profile.accent_color"))?,
                banner_color: r.opt_tail(|r| decode_profile_color(r, "profile.banner_color"))?,
            }),
            0x0A => Ok(CoreMsg::VoiceSignal {
                group_id: r.arr()?,
                channel_id: r.arr()?,
                action: {
                    let a = r.u8()?;
                    if a > 2 {
                        return Err(DecodeError::InvalidValue("voice.action"));
                    }
                    a
                },
                media_kinds: r.u8()?,
                mute: match r.u8()? {
                    0 => false,
                    1 => true,
                    _ => return Err(DecodeError::InvalidValue("voice.mute")),
                },
            }),
            0x0B => Ok(CoreMsg::GroupSync {
                group_id: r.arr()?,
                max_lamport: r.u64()?,
                op_count: r.u32()?,
                digest: r.arr()?,
            }),
            0x0C => Ok(CoreMsg::GroupSyncPull {
                group_id: r.arr()?,
                since_lamport: r.u64()?,
            }),
            0x0D => Ok(CoreMsg::FriendRemove),
            0x0E => {
                let group_id = r.arr()?;
                let invite_id = r.arr()?;
                let group_name = r.str(MAX_NAME, "invite.group_name")?;
                let inviter = r.arr()?;
                let secret = r.arr()?;
                let expires_ms = r.u64()?;
                if expires_ms > MAX_TICKET_EXPIRES_MS {
                    return Err(DecodeError::InvalidValue("invite.expires_ms"));
                }
                let sig = r.arr()?;
                Ok(CoreMsg::InviteTicket {
                    group_id,
                    invite_id,
                    group_name,
                    inviter,
                    secret,
                    expires_ms,
                    sig,
                })
            }
            0x0F => Ok(CoreMsg::InviteAccept {
                group_id: r.arr()?,
                invite_id: r.arr()?,
                secret: r.arr()?,
            }),
            0x10 => Ok(CoreMsg::InviteDecline {
                group_id: r.arr()?,
                invite_id: r.arr()?,
            }),
            0x11 => Ok(CoreMsg::CallOffer { call_id: r.arr()? }),
            0x12 => Ok(CoreMsg::CallAnswer { call_id: r.arr()? }),
            0x13 => Ok(CoreMsg::CallDecline {
                call_id: r.arr()?,
                reason: {
                    let reason = r.u8()?;
                    if reason > CALL_DECLINE_BUSY {
                        return Err(DecodeError::InvalidValue("call.decline.reason"));
                    }
                    reason
                },
            }),
            0x14 => Ok(CoreMsg::CallHangup { call_id: r.arr()? }),
            _ => Err(DecodeError::InvalidValue("core kind")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-trips a group op body through its wire discriminant + encoding.
    fn roundtrip(body: &GroupOpBody) -> GroupOpBody {
        GroupOpBody::decode_body(body.kind(), &body.encode_body()).expect("decode")
    }

    #[test]
    fn timeout_member_roundtrips() {
        let body = GroupOpBody::TimeoutMember {
            member: [0xAB; 32],
            until_ms: 1_700_000_000_000,
        };
        assert_eq!(body.kind(), 0x1D);
        assert_eq!(roundtrip(&body), body);
        // Clear form (until_ms = 0) survives the round-trip too.
        let clear = GroupOpBody::TimeoutMember {
            member: [0xAB; 32],
            until_ms: 0,
        };
        assert_eq!(roundtrip(&clear), clear);
    }

    #[test]
    fn set_nickname_roundtrips() {
        let body = GroupOpBody::SetNickname {
            member: [0x11; 32],
            name: "Capitaine".into(),
        };
        assert_eq!(body.kind(), 0x1E);
        assert_eq!(roundtrip(&body), body);
        // Empty name (the clear form) is a valid wire value.
        let clear = GroupOpBody::SetNickname {
            member: [0x11; 32],
            name: String::new(),
        };
        assert_eq!(roundtrip(&clear), clear);
    }

    #[test]
    fn invite_ticket_expires_ms_bound_is_enforced_at_decode() {
        let msg = CoreMsg::InviteTicket {
            group_id: [1; 16],
            invite_id: [2; 16],
            group_name: "Guilde".into(),
            inviter: [3; 32],
            secret: [4; 32],
            expires_ms: MAX_TICKET_EXPIRES_MS,
            sig: [5; 64],
        };
        let mut w = Writer::new();
        msg.encode(&mut w);
        let bytes = w.into_bytes();
        let mut r = Reader::new(&bytes);
        assert_eq!(CoreMsg::decode(&mut r).unwrap(), msg);

        // Une valeur au-delà de la borne est rejetée au décodage (jamais de
        // panique, même sur une entrée entièrement attaquant-contrôlée).
        let too_far = CoreMsg::InviteTicket {
            group_id: [1; 16],
            invite_id: [2; 16],
            group_name: "Guilde".into(),
            inviter: [3; 32],
            secret: [4; 32],
            expires_ms: MAX_TICKET_EXPIRES_MS + 1,
            sig: [5; 64],
        };
        let mut w2 = Writer::new();
        too_far.encode(&mut w2);
        let bytes2 = w2.into_bytes();
        let mut r2 = Reader::new(&bytes2);
        assert!(CoreMsg::decode(&mut r2).is_err());
    }

    #[test]
    fn invite_ticket_signable_bytes_are_stable_and_domain_separated() {
        let a = invite_ticket_signable_bytes(&[1; 16], &[2; 16], "Guilde", &[3; 32], &[4; 32], 5);
        let b = invite_ticket_signable_bytes(&[1; 16], &[2; 16], "Guilde", &[3; 32], &[4; 32], 5);
        assert_eq!(a, b);
        assert!(a.starts_with(b"accord-invite-ticket-v1"));
        // Un champ différent change les octets signables (pas de collision
        // triviale entre un nom de groupe et un secret par ex.).
        let c = invite_ticket_signable_bytes(&[1; 16], &[2; 16], "Autre", &[3; 32], &[4; 32], 5);
        assert_ne!(a, c);
    }

    #[test]
    fn malformed_new_ops_are_rejected() {
        // Truncated TimeoutMember body (member present, missing until_ms).
        assert!(GroupOpBody::decode_body(0x1D, &[0u8; 32]).is_err());
        // Truncated SetNickname body (missing the length-prefixed name).
        assert!(GroupOpBody::decode_body(0x1E, &[0u8; 32]).is_err());
        // Trailing garbage after a complete body is rejected by `finish`.
        let mut over = GroupOpBody::TimeoutMember {
            member: [7; 32],
            until_ms: 5,
        }
        .encode_body();
        over.push(0xFF);
        assert!(GroupOpBody::decode_body(0x1D, &over).is_err());
    }

    #[test]
    fn voice_moderate_roundtrips_and_rejects_forged_bytes() {
        for (mute, deafen) in [(false, false), (true, false), (false, true), (true, true)] {
            let body = GroupOpBody::VoiceModerate {
                member: [0xCD; 32],
                mute,
                deafen,
            };
            assert_eq!(body.kind(), 0x1F);
            assert_eq!(roundtrip(&body), body);
        }
        // Truncated: member only, missing the two flag bytes.
        assert!(GroupOpBody::decode_body(0x1F, &[0u8; 32]).is_err());
        // Forged flag bytes outside {0, 1} are rejected, never coerced.
        let mut forged = [0u8; 34];
        forged[32] = 2;
        assert!(GroupOpBody::decode_body(0x1F, &forged).is_err());
        let mut forged = [0u8; 34];
        forged[33] = 0xFF;
        assert!(GroupOpBody::decode_body(0x1F, &forged).is_err());
        // Trailing garbage after a complete body is rejected.
        let mut over = GroupOpBody::VoiceModerate {
            member: [1; 32],
            mute: true,
            deafen: false,
        }
        .encode_body();
        over.push(0);
        assert!(GroupOpBody::decode_body(0x1F, &over).is_err());
    }

    /// Round-trips a CoreMsg through the full wire encoding.
    fn core_roundtrip(msg: &CoreMsg) -> CoreMsg {
        let mut w = Writer::new();
        msg.encode(&mut w);
        let bytes = w.into_bytes();
        let mut r = Reader::new(&bytes);
        let decoded = CoreMsg::decode(&mut r).expect("decode");
        r.finish().expect("no trailing bytes");
        decoded
    }

    #[test]
    fn call_messages_roundtrip() {
        for msg in [
            CoreMsg::CallOffer { call_id: [9; 16] },
            CoreMsg::CallAnswer { call_id: [9; 16] },
            CoreMsg::CallDecline {
                call_id: [9; 16],
                reason: CALL_DECLINE_REJECTED,
            },
            CoreMsg::CallDecline {
                call_id: [9; 16],
                reason: CALL_DECLINE_BUSY,
            },
            CoreMsg::CallHangup { call_id: [9; 16] },
        ] {
            assert_eq!(core_roundtrip(&msg), msg);
        }
    }

    #[test]
    fn forged_call_messages_are_rejected_without_panic() {
        // Truncated call_id on each call kind: decode fails cleanly.
        for kind in [0x11u8, 0x12, 0x13, 0x14] {
            let mut bytes = vec![kind];
            bytes.extend_from_slice(&[0u8; 8]); // half a call_id
            let mut r = Reader::new(&bytes);
            assert!(CoreMsg::decode(&mut r).is_err(), "kind {kind:#x}");
        }
        // Decline with an out-of-domain reason byte is rejected.
        let mut bytes = vec![0x13];
        bytes.extend_from_slice(&[0u8; 16]);
        bytes.push(2);
        let mut r = Reader::new(&bytes);
        assert!(CoreMsg::decode(&mut r).is_err());
        // Trailing bytes after a complete CallOffer are rejected by finish().
        let mut w = Writer::new();
        CoreMsg::CallOffer { call_id: [1; 16] }.encode(&mut w);
        let mut bytes = w.into_bytes();
        bytes.push(0xAB);
        let mut r = Reader::new(&bytes);
        let _ = CoreMsg::decode(&mut r).expect("prefix decodes");
        assert!(r.finish().is_err());
    }

    #[test]
    fn set_meta_banner_color_roundtrips_and_is_additive() {
        let with_color = GroupOpBody::SetMeta {
            name: "Salon".into(),
            icon: Some([1; 32]),
            banner_color: Some(0x00_FF_AA),
        };
        assert_eq!(with_color.kind(), 0x02);
        assert_eq!(roundtrip(&with_color), with_color);

        // A legacy encoder that never learned about `banner_color` simply
        // never writes the trailing `opt` tag: strip the final byte our
        // current encoder always writes for `None` and confirm it still
        // decodes, to `None` (rétrocompatibilité filaire, `Reader::opt_tail`).
        let no_color = GroupOpBody::SetMeta {
            name: "Salon".into(),
            icon: Some([1; 32]),
            banner_color: None,
        };
        let mut encoded = no_color.encode_body();
        assert_eq!(encoded.pop(), Some(0), "trailing None opt tag");
        let decoded = GroupOpBody::decode_body(0x02, &encoded).expect("legacy decode");
        assert_eq!(decoded, no_color);

        // Out-of-range colour (> 24 bits) is rejected at decode.
        let mut forged = with_color.encode_body();
        let len = forged.len();
        forged[len - 4..].copy_from_slice(&0x0100_0000u32.to_be_bytes());
        assert!(GroupOpBody::decode_body(0x02, &forged).is_err());
    }

    #[test]
    fn event_op_bodies_roundtrip() {
        for body in [
            GroupOpBody::EventCreate {
                event_id: [1; 16],
                title: "Soirée jeux".into(),
                description: "Amenez vos manettes.".into(),
                start_ms: 1_700_000_000_000,
                channel_id: Some([2; 16]),
            },
            GroupOpBody::EventCreate {
                event_id: [1; 16],
                title: "Ti".into(),
                description: String::new(),
                start_ms: 0,
                channel_id: None,
            },
            GroupOpBody::EventEdit {
                event_id: [1; 16],
                title: "Soirée jeux (reportée)".into(),
                description: "Nouvelle date.".into(),
                start_ms: 1_700_100_000_000,
                channel_id: None,
            },
            GroupOpBody::EventDelete { event_id: [1; 16] },
            GroupOpBody::EventRsvp {
                event_id: [1; 16],
                interested: true,
            },
            GroupOpBody::EventRsvp {
                event_id: [1; 16],
                interested: false,
            },
        ] {
            assert_eq!(roundtrip(&body), body);
        }
        assert_eq!(
            GroupOpBody::EventCreate {
                event_id: [0; 16],
                title: String::new(),
                description: String::new(),
                start_ms: 0,
                channel_id: None,
            }
            .kind(),
            0x20
        );
        assert_eq!(
            GroupOpBody::EventEdit {
                event_id: [0; 16],
                title: String::new(),
                description: String::new(),
                start_ms: 0,
                channel_id: None,
            }
            .kind(),
            0x21
        );
        assert_eq!(GroupOpBody::EventDelete { event_id: [0; 16] }.kind(), 0x22);
        assert_eq!(
            GroupOpBody::EventRsvp {
                event_id: [0; 16],
                interested: true,
            }
            .kind(),
            0x23
        );
    }

    #[test]
    fn event_rsvp_rejects_forged_interested_byte() {
        // `interested` must be a strict {0,1} boolean tag; anything else is
        // rejected, never coerced (adversarial byte).
        let mut bytes = [0u8; 17];
        bytes[16] = 2;
        assert!(GroupOpBody::decode_body(0x23, &bytes).is_err());
        // Truncated (missing the flag byte entirely).
        assert!(GroupOpBody::decode_body(0x23, &[0u8; 16]).is_err());
    }

    #[test]
    fn event_title_and_description_bounds_are_strict_at_decode() {
        // A title beyond MAX_EVENT_TITLE (400 UTF-8 bytes) is rejected.
        let mut w = Writer::new();
        w.put_arr(&[0u8; 16]);
        w.put_str(&"x".repeat(MAX_EVENT_TITLE + 1));
        assert!(GroupOpBody::decode_body(0x20, &w.into_bytes()).is_err());

        // A description beyond MAX_EVENT_DESC is rejected too, even with a
        // valid title ahead of it.
        let mut w2 = Writer::new();
        w2.put_arr(&[0u8; 16]);
        w2.put_str("Titre");
        w2.put_str(&"x".repeat(MAX_EVENT_DESC + 1));
        w2.put_u64(0);
        w2.put_u8(0); // channel_id opt tag = None
        assert!(GroupOpBody::decode_body(0x20, &w2.into_bytes()).is_err());
    }

    #[test]
    fn sticker_op_bodies_roundtrip_and_share_emoji_name_bound() {
        let add = GroupOpBody::StickerAdd {
            name: "wave".into(),
            file: [9; 32],
        };
        assert_eq!(add.kind(), 0x24);
        assert_eq!(roundtrip(&add), add);
        let remove = GroupOpBody::StickerRemove {
            name: "wave".into(),
        };
        assert_eq!(remove.kind(), 0x25);
        assert_eq!(roundtrip(&remove), remove);

        // A name beyond 32 bytes is rejected at decode (shared bound with
        // AddEmoji/DelEmoji — MAX_EMOJI_NAME).
        let mut w = Writer::new();
        w.put_str(&"x".repeat(33));
        w.put_arr(&[0u8; 32]);
        assert!(GroupOpBody::decode_body(0x24, &w.into_bytes()).is_err());
    }

    #[test]
    fn set_member_avatar_roundtrips_and_rejects_trailing_bytes() {
        let set = GroupOpBody::SetMemberAvatar {
            avatar: Some([3; 32]),
        };
        assert_eq!(set.kind(), 0x26);
        assert_eq!(roundtrip(&set), set);
        let clear = GroupOpBody::SetMemberAvatar { avatar: None };
        assert_eq!(roundtrip(&clear), clear);

        let mut over = set.encode_body();
        over.push(0xFF);
        assert!(GroupOpBody::decode_body(0x26, &over).is_err());
    }

    #[test]
    fn sticker_msg_body_roundtrips_and_rejects_truncation() {
        let body = MsgBody::Sticker {
            name: "wave".into(),
            merkle_root: [7; 32],
        };
        assert_eq!(body.kind(), 4);
        assert_eq!(roundtrip_msg_body(&body), body);

        let encoded = body.encode_body();
        for cut in 0..encoded.len() {
            assert!(
                MsgBody::decode_body(4, &encoded[..cut]).is_err(),
                "cut={cut}"
            );
        }
    }

    /// Round-trips a `MsgBody` through its wire discriminant + encoding.
    fn roundtrip_msg_body(body: &MsgBody) -> MsgBody {
        MsgBody::decode_body(body.kind(), &body.encode_body()).expect("decode")
    }
}
