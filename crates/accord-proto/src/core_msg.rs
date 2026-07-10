//! Canal CORE (0x02) : messagerie directe, groupes, présence (SPEC §6).

use crate::limits;
use crate::wire::{DecodeError, Reader, WireDecode, WireEncode, Writer};

const MAX_NAME: usize = 256;
/// Borne stricte du pseudo d'un message `Profile` : 32 caractères × 4 octets
/// UTF-8 au plus (la validation sémantique 2-32 caractères a lieu à
/// l'ingestion, côté cœur).
const MAX_PROFILE_NAME: usize = 128;
const MAX_BIO: usize = 2048;
const MAX_BODY: usize = 64 * 1024;
const MAX_ATTACHMENTS: usize = 10;
const MAX_EMOJI: usize = 64;
/// Borne du nom d'un émoji de serveur (2-32 caractères `[a-z0-9_]` validés à
/// l'ingestion ; 32 octets suffisent, l'alphabet étant ASCII).
const MAX_EMOJI_NAME: usize = 32;

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
}

impl MsgBody {
    /// Discriminant filaire du corps (SPEC §6, champ `kind`).
    pub fn kind(&self) -> u8 {
        match self {
            MsgBody::Text { .. } => 0,
            MsgBody::Edit { .. } => 1,
            MsgBody::Delete { .. } => 2,
            MsgBody::Reaction { .. } => 3,
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
    /// 0x02 — Métadonnées (nom, icône).
    SetMeta {
        /// Nouveau nom.
        name: String,
        /// Hash Merkle de l'icône partagée, le cas échéant.
        icon: Option<[u8; 32]>,
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
        }
    }

    /// Encode le corps seul (sans discriminant).
    pub fn encode_body(&self) -> Vec<u8> {
        let mut w = Writer::new();
        match self {
            Self::Create { name } => w.put_str(name),
            Self::SetMeta { name, icon } => {
                w.put_str(name);
                w.put_opt(icon.as_ref(), |w, h| w.put_arr(h));
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
            } => {
                w.put_u8(0x09);
                w.put_str(display_name);
                w.put_str(bio);
                w.put_opt(avatar.as_ref(), |w, h| w.put_arr(h));
                w.put_opt(banner.as_ref(), |w, h| w.put_arr(h));
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
        }
    }
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
            _ => Err(DecodeError::InvalidValue("core kind")),
        }
    }
}
