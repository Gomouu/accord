//! État répliqué d'un groupe : repli déterministe de l'op-log signé
//! (SPEC §6.2). Toute op non autorisée par l'état courant est ignorée —
//! tous les pairs honnêtes convergent vers le même état.

use accord_crypto::identity::node_id_of;
use accord_proto::core_msg::{
    perms, ChannelKind, GroupOp, GroupOpBody, MAX_AUTOMOD_WORDS, MAX_CHANNEL_SLOWMODE_SECS,
    MAX_POLL_OPTIONS,
};
use std::collections::{BTreeMap, BTreeSet};

/// Permissions implicites de tout membre (D-015) ; les overrides de salon
/// peuvent les retirer (deny > allow).
pub const DEFAULT_MEMBER_PERMS: u32 = perms::VIEW | perms::SEND;

/// Toutes les permissions connues (fondateur / ADMIN). `PRIORITY_SPEAKER`
/// n'en fait volontairement PAS partie : l'atténuation des autres pendant
/// qu'on parle est une attribution explicite de rôle, pas un privilège
/// administratif implicite (sinon tout fondateur atténuerait son salon en
/// permanence) — voir [`GroupState::is_priority_speaker`].
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

/// Nombre maximal de stickers de serveur (D-047 ; même politique que les
/// émojis : au-delà, un nouvel ajout est ignoré, le remplacement reste
/// possible).
pub const MAX_STICKERS: usize = 30;

/// Nombre maximal d'événements planifiés par groupe (D-047 ; au-delà, un
/// `EventCreate` supplémentaire est ignoré — les événements existants
/// restent modifiables/supprimables sans limite).
pub const MAX_EVENTS: usize = 25;

/// Nombre maximal de sondages par groupe (D-048 ; même politique que
/// [`MAX_EVENTS`] : au-delà, un nouveau `PollCreate` est ignoré, les sondages
/// existants restent votables/clôturables sans limite). Nécessaire car,
/// contrairement à un vote qui est naturellement borné par l'effectif du
/// groupe (une entrée par membre dans `Poll::votes`), le *nombre de sondages*
/// eux-mêmes n'a aucune borne organique — un membre malveillant pourrait sinon
/// gonfler l'état répliqué sans limite via des `PollCreate` répétés.
pub const MAX_POLLS: usize = 25;

/// Bornes du titre d'un événement (caractères, après trim).
const MIN_EVENT_TITLE_CHARS: usize = 2;
/// Borne haute du titre d'un événement (caractères, après trim).
pub const MAX_EVENT_TITLE_CHARS: usize = 100;
/// Borne haute de la description d'un événement (caractères, après trim).
pub const MAX_EVENT_DESC_CHARS: usize = 1024;

/// Longueur maximale d'un pseudo de serveur (en caractères, après trim).
pub const MAX_NICKNAME_CHARS: usize = 32;

/// Longueur maximale d'un mot AutoMod (en caractères, après normalisation
/// en minuscules) — même politique que [`MAX_NICKNAME_CHARS`]. Le nombre de
/// mots lui-même est borné par
/// [`accord_proto::core_msg::MAX_AUTOMOD_WORDS`] (revérifié ici au repli en
/// défense en profondeur — le décodage filaire la fait déjà respecter, mais
/// le repli ne doit jamais dépendre de cette seule garde amont).
pub const MAX_AUTOMOD_WORD_CHARS: usize = 32;

/// Plafond de l'échéance d'une sourdine (`until_ms`, ms murales). ~an 2248,
/// très en deçà de 2^53 : garde une date exploitable côté UI (JS `number`).
pub const MAX_TIMEOUT_UNTIL_MS: u64 = 1 << 43;

/// Vrai si `c` est un caractère de « format » Unicode exploitable pour
/// l'usurpation visuelle d'identité : override bidirectionnel, marques
/// directionnelles, caractères de largeur nulle, isolats, BOM. `char::is_control`
/// (catégorie Cc) ne les couvre pas ; on les rejette explicitement dans les
/// pseudos de serveur pour éviter qu'un membre affiche un nom trompeur (texte
/// inversé, caractères cachés) dans la liste des membres ou les messages.
pub(crate) fn is_spoofing_char(c: char) -> bool {
    matches!(c,
        '\u{200B}'..='\u{200F}'   // ZWSP, ZWNJ, ZWJ, LRM, RLM
        | '\u{202A}'..='\u{202E}' // LRE, RLE, PDF, LRO, RLO
        | '\u{2066}'..='\u{2069}' // LRI, RLI, FSI, PDI
        | '\u{FEFF}',             // BOM / ZWNBSP
    )
}

/// Retire les caractères de format Unicode trompeurs ([`is_spoofing_char`])
/// d'un texte annoncé par un **pair** (miroir de
/// [`crate::presence::sanitize_peer_custom`]) : meilleur effort plutôt que
/// rejet total — un seul caractère indésirable ne doit pas faire échouer
/// l'ingestion de tout un profil ou libellé. La validation stricte locale
/// (rejet complet) reste [`is_valid_display_label`] / la validation dédiée de
/// chaque champ.
pub(crate) fn strip_spoofing_chars(text: &str) -> String {
    text.chars().filter(|c| !is_spoofing_char(*c)).collect()
}

/// Vrai si `name` est un libellé affiché valide : `1..=max_chars` caractères,
/// sans caractère de contrôle ni caractère de format trompeur
/// ([`is_spoofing_char`]). Base commune à [`is_valid_nickname`] (pseudos de
/// serveur) et réutilisée par `accord-node` pour tout autre libellé montré à
/// l'utilisateur avant qu'il ait pu vérifier son origine — notamment le
/// `group_name` d'un ticket d'invitation, intégralement contrôlé par
/// l'émetteur et affiché à l'invité avant toute décision de rejoindre.
pub fn is_valid_display_label(name: &str, max_chars: usize) -> bool {
    let len = name.chars().count();
    (1..=max_chars).contains(&len) && !name.chars().any(|c| c.is_control() || is_spoofing_char(c))
}

/// Vrai si `name` (déjà trimmé) est un pseudo de serveur valide : 1 à 32
/// caractères, sans caractère de contrôle ni caractère de format trompeur
/// ([`is_spoofing_char`]). La chaîne vide efface le pseudo et n'est donc pas
/// « valide » au sens de cette fonction (traitée à part).
fn is_valid_nickname(name: &str) -> bool {
    is_valid_display_label(name, MAX_NICKNAME_CHARS)
}

/// Vrai si `name` est un nom d'émoji valide : 2 à 32 caractères `[a-z0-9_]`.
fn is_valid_emoji_name(name: &str) -> bool {
    let len = name.chars().count();
    (2..=32).contains(&len)
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// Vrai si `name` est un nom de sticker valide : mêmes règles qu'un nom
/// d'émoji ([`is_valid_emoji_name`], D-047).
fn is_valid_sticker_name(name: &str) -> bool {
    is_valid_emoji_name(name)
}

/// Vrai si `word` est un mot AutoMod valide : 1 à [`MAX_AUTOMOD_WORD_CHARS`]
/// caractères, sans caractère de contrôle ni caractère de format trompeur
/// ([`is_spoofing_char`]) — même garde anti-usurpation qu'un pseudo
/// ([`is_valid_display_label`]). L'appelant doit passer un mot **déjà**
/// normalisé en minuscules (`str::to_lowercase`) : ce validateur ne
/// normalise pas la casse lui-même, il ne fait que rejeter les entrées
/// hostiles — la forme canonique stockée dans
/// [`GroupState::automod_words`] est toujours en minuscules puisque la
/// comparaison à la composition/au rendu est insensible à la casse.
fn is_valid_automod_word(word: &str) -> bool {
    is_valid_display_label(word, MAX_AUTOMOD_WORD_CHARS)
}

/// Vrai si `title` est un titre d'événement valide : 2 à 100 caractères,
/// sans caractère de contrôle ni caractère de format trompeur
/// ([`is_spoofing_char`]) — même garde anti-usurpation que
/// [`is_valid_display_label`], avec un plancher de 2 caractères au lieu de 1
/// (un événement a besoin d'un vrai nom, D-047).
fn is_valid_event_title(title: &str) -> bool {
    let len = title.chars().count();
    (MIN_EVENT_TITLE_CHARS..=MAX_EVENT_TITLE_CHARS).contains(&len)
        && !title.chars().any(|c| c.is_control() || is_spoofing_char(c))
}

/// Vrai si `desc` est une description d'événement valide : au plus 1024
/// caractères, caractères de contrôle refusés hormis `\n`/`\r`/`\t` (mêmes
/// règles qu'une bio de profil, `accord_core::profile::validate_bio`). Une
/// description vide est valide (aucune description).
fn is_valid_event_description(desc: &str) -> bool {
    desc.chars().count() <= MAX_EVENT_DESC_CHARS
        && !desc
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
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
    /// Mode lent : délai minimal (secondes) entre deux messages d'un même
    /// auteur non exempté dans ce salon (`0` = désactivé, plafond
    /// [`MAX_CHANNEL_SLOWMODE_SECS`]). Config répliquée
    /// (`GroupOpBody::SetChannelSlowmode`) ; l'application du cooldown
    /// lui-même n'est PAS repliée ici (les messages ne font pas partie de
    /// l'op-log signé — voir `accord_core::group::msg::check_slowmode`) :
    /// chaque pair honnête la réévalue localement à la composition/
    /// ingestion. Effacé gratuitement à la suppression du salon (le champ
    /// disparaît avec l'entrée `Channel`, comme les overrides).
    pub slowmode_secs: u32,
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

/// Modération vocale serveur d'un membre (op 0x1F) : sourdine et/ou surdité
/// forcées dans tous les salons vocaux du groupe. Une entrée entièrement
/// fausse n'est jamais stockée (elle vaut absence).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VoiceModeration {
    /// Micro coupé par un modérateur.
    pub mute: bool,
    /// Sortie coupée par un modérateur (implique la sourdine à l'application).
    pub deafen: bool,
}

/// Événement planifié (op 0x20-0x23, D-047). `channel_id`, s'il est fourni,
/// doit être un salon vocal existant du groupe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    /// Titre affiché (2-100 caractères).
    pub title: String,
    /// Description éventuelle (≤1024 caractères).
    pub description: String,
    /// Échéance murale de démarrage (ms), bornée comme
    /// [`GroupState::timeouts`].
    pub start_ms: u64,
    /// Salon vocal où l'événement se déroule, le cas échéant.
    pub channel_id: Option<[u8; 16]>,
    /// Auteur (peut toujours éditer/supprimer son propre événement).
    pub author: [u8; 32],
    /// Membres ayant indiqué être intéressés.
    pub rsvps: BTreeSet<[u8; 32]>,
}

/// Dépouillement d'un sondage (op 0x27-0x29, D-048). La question et les
/// options n'y figurent volontairement pas : elles vivent dans le message
/// `MsgBody::Poll` content-adressé à `poll_id`, jamais dans l'op-log — le
/// repli n'a besoin que du décompte pour que tous les pairs convergent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Poll {
    /// Auteur (membre qui a créé le sondage ; peut toujours le clôturer,
    /// comme [`Event::author`]).
    pub author: [u8; 32],
    /// Salon où le sondage a été posté — l'auteur devait y détenir
    /// `VIEW`+`SEND` effectifs au moment de la création (vérifié une seule
    /// fois, à la création, comme l'écriture d'un message ordinaire).
    pub channel_id: [u8; 16],
    /// Message `MsgBody::Poll` canonique auquel ce `poll_id` est lié : seul
    /// un message reçu de `author` portant exactement cet identifiant est
    /// affiché comme la question/les options de ce sondage (anti-usurpation,
    /// D-048 fix HIGH-2).
    pub msg_id: [u8; 16],
    /// Clos : les votes ultérieurs sont ignorés au repli.
    pub closed: bool,
    /// Vote courant par membre `membre → index d'option` (choix unique : un
    /// nouveau vote du même membre remplace le précédent, dédoublonnage par
    /// clé de `BTreeMap` — mêmes sémantiques que [`Event::rsvps`]).
    /// `option_index` n'est borné qu'structurellement
    /// ([`accord_proto::core_msg::MAX_POLL_OPTIONS`]) : le repli ne connaît
    /// pas le nombre *réel* d'options du sondage (qui vit dans le message),
    /// donc un index au-delà du nombre réel d'options est accepté ici et
    /// simplement jamais affiché par une UI honnête (dégradation gracieuse).
    pub votes: BTreeMap<[u8; 32], u8>,
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
    /// Modérations vocales actives `membre → sourdine/surdité forcées`
    /// (op 0x1F ; une entrée entièrement fausse est retirée).
    pub voice_moderation: BTreeMap<[u8; 32], VoiceModeration>,
    /// Stickers de serveur `nom → racine Merkle de l'image` (D-047, mêmes
    /// règles que [`GroupState::emojis`]). Ordre du `BTreeMap` stable.
    pub stickers: BTreeMap<String, [u8; 32]>,
    /// Événements planifiés `id → détails` (D-047, bornés à [`MAX_EVENTS`]
    /// par groupe à la création).
    pub events: BTreeMap<[u8; 16], Event>,
    /// Sondages `id → dépouillement` (D-048, bornés à [`MAX_POLLS`] par
    /// groupe à la création via `PollCreate`). La question/les options
    /// vivent dans le message, jamais ici.
    pub polls: BTreeMap<[u8; 16], Poll>,
    /// Avatars par serveur `membre → racine Merkle` (op 0x26, self-service
    /// uniquement — aucun retrait par un modérateur).
    pub member_avatars: BTreeMap<[u8; 32], [u8; 32]>,
    /// Couleur de bannière du serveur `0xRRGGBB`, le cas échéant (D-047,
    /// champ additif de [`GroupOpBody::SetMeta`]).
    pub banner_color: Option<u32>,
    /// Liste des mots bloqués AutoMod du groupe, normalisés en minuscules
    /// (`GroupOpBody::SetAutoModWords`, remplacement intégral à chaque op —
    /// jamais d'accumulation). `BTreeSet` : ordre déterministe et
    /// dédoublonnage gratuit, cohérent avec le fait qu'il n'y a aucune
    /// notion d'ordre d'ajout à préserver (c'est une config, pas un
    /// historique).
    ///
    /// **Portée strictement backend** : Accord est serverless, donc rien
    /// ici n'empêche un pair modifié d'envoyer n'importe quel texte — cette
    /// liste ne fait que stocker/répliquer la règle signée. L'application
    /// (avertir/bloquer l'expéditeur à la composition, masquer les mots
    /// reçus au rendu) est un choix du client honnête, jamais garanti
    /// contre un pair modifié — voir `docs/COMMUNITY.md`.
    pub automod_words: BTreeSet<String>,
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

    /// Modération vocale active de `who` (les deux drapeaux faux si aucune).
    pub fn voice_moderation_of(&self, who: &[u8; 32]) -> VoiceModeration {
        self.voice_moderation.get(who).copied().unwrap_or_default()
    }

    /// Vrai si `who` tient EXPLICITEMENT la permission d'orateur prioritaire
    /// par l'un de ses rôles. Jamais impliquée par ADMIN ni par le statut de
    /// fondateur : atténuer les autres participants est un choix
    /// d'attribution, pas un privilège administratif.
    pub fn is_priority_speaker(&self, who: &[u8; 32]) -> bool {
        self.members.get(who).is_some_and(|m| {
            m.roles
                .iter()
                .filter_map(|r| self.roles.get(r))
                .any(|r| r.permissions & perms::PRIORITY_SPEAKER != 0)
        })
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
            GroupOpBody::SetMeta {
                name,
                icon,
                banner_color,
            } => {
                if !has(perms::MANAGE_CHANNELS) {
                    return self.ignore("SET_META refusé");
                }
                self.name = name;
                self.icon = icon;
                self.banner_color = banner_color;
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
                        slowmode_secs: 0,
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
                // Un événement pointant sur ce salon devient « sans salon »,
                // exactement comme un salon devient « sans catégorie » à la
                // suppression de sa catégorie (D-047) : l'événement lui-même
                // survit.
                for event in self.events.values_mut() {
                    if event.channel_id == Some(channel_id) {
                        event.channel_id = None;
                    }
                }
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
                self.voice_moderation.remove(&member);
                self.member_avatars.remove(&member);
                // Ses RSVP sont retirés de tous les événements (hygiène : pas
                // de clé publique orpheline dans l'état répliqué). Ses
                // événements AUTORÉS restent : gérables par le fondateur ou
                // tout MANAGE_CHANNELS (même découplage que DelChannel).
                for ev in self.events.values_mut() {
                    ev.rsvps.remove(&member);
                }
                // Même hygiène pour ses votes de sondage (D-048) ; les
                // sondages qu'il a AUTORÉS restent, clôturables par le
                // fondateur ou tout MANAGE_CHANNELS (même découplage).
                for poll in self.polls.values_mut() {
                    poll.votes.remove(&member);
                }
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
                self.voice_moderation.remove(&author);
                self.member_avatars.remove(&author);
                // Même hygiène qu'au kick/ban : RSVP retirés, événements
                // autorés conservés (gérables par fondateur/MANAGE_CHANNELS).
                for ev in self.events.values_mut() {
                    ev.rsvps.remove(&author);
                }
                // Idem pour les votes de sondage (D-048).
                for poll in self.polls.values_mut() {
                    poll.votes.remove(&author);
                }
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
                    self.timeouts
                        .insert(member, until_ms.min(MAX_TIMEOUT_UNTIL_MS));
                }
            }
            GroupOpBody::VoiceModerate {
                member,
                mute,
                deafen,
            } => {
                // Same gate as TimeoutMember (0x1D): KICK with the kick
                // hierarchy — no moderating the founder, nor a member of
                // higher/equal role (D-015).
                if !has(perms::KICK) {
                    return self.ignore("VOICE_MODERATE refusé");
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
                if !mute && !deafen {
                    self.voice_moderation.remove(&member);
                } else {
                    self.voice_moderation
                        .insert(member, VoiceModeration { mute, deafen });
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
            GroupOpBody::EventCreate {
                event_id,
                title,
                description,
                start_ms,
                channel_id,
            } => {
                if !has(perms::MANAGE_CHANNELS) {
                    return self.ignore("EVENT_CREATE refusé");
                }
                if !is_valid_event_title(&title) {
                    return self.ignore("titre d'événement invalide");
                }
                if !is_valid_event_description(&description) {
                    return self.ignore("description d'événement invalide");
                }
                // Same wall-clock ceiling as a timeout's `until_ms` (D-047) :
                // an absurd start time is rejected rather than silently
                // capped — unlike a mute, there is no graceful degrade for a
                // garbled date.
                if start_ms > MAX_TIMEOUT_UNTIL_MS {
                    return self.ignore("date d'événement hors bornes");
                }
                if let Some(cid) = &channel_id {
                    match self.channels.get(cid) {
                        Some(ch) if ch.kind == ChannelKind::Voice => {}
                        _ => return self.ignore("salon vocal inconnu"),
                    }
                }
                if self.events.contains_key(&event_id) {
                    return self.ignore("identifiant d'événement déjà utilisé");
                }
                if self.events.len() >= MAX_EVENTS {
                    return self.ignore("trop d'événements (25 max)");
                }
                self.events.insert(
                    event_id,
                    Event {
                        title,
                        description,
                        start_ms,
                        channel_id,
                        author,
                        rsvps: BTreeSet::new(),
                    },
                );
            }
            GroupOpBody::EventEdit {
                event_id,
                title,
                description,
                start_ms,
                channel_id,
            } => {
                let Some(current) = self.events.get(&event_id) else {
                    return self.ignore("événement inconnu");
                };
                // Author-owns-or-manager-overrides, like most moderation ops
                // — no role-hierarchy comparison needed beyond that (an
                // event is not a member, cf. TimeoutMember/VoiceModerate).
                if !has(perms::MANAGE_CHANNELS) && current.author != author {
                    return self.ignore("EVENT_EDIT refusé");
                }
                if !is_valid_event_title(&title) {
                    return self.ignore("titre d'événement invalide");
                }
                if !is_valid_event_description(&description) {
                    return self.ignore("description d'événement invalide");
                }
                if start_ms > MAX_TIMEOUT_UNTIL_MS {
                    return self.ignore("date d'événement hors bornes");
                }
                if let Some(cid) = &channel_id {
                    match self.channels.get(cid) {
                        Some(ch) if ch.kind == ChannelKind::Voice => {}
                        _ => return self.ignore("salon vocal inconnu"),
                    }
                }
                let rsvps = current.rsvps.clone();
                let ev_author = current.author;
                self.events.insert(
                    event_id,
                    Event {
                        title,
                        description,
                        start_ms,
                        channel_id,
                        author: ev_author,
                        rsvps,
                    },
                );
            }
            GroupOpBody::EventDelete { event_id } => {
                let Some(current) = self.events.get(&event_id) else {
                    return self.ignore("événement inconnu");
                };
                if !has(perms::MANAGE_CHANNELS) && current.author != author {
                    return self.ignore("EVENT_DELETE refusé");
                }
                self.events.remove(&event_id);
            }
            GroupOpBody::EventRsvp {
                event_id,
                interested,
            } => {
                // Any member may RSVP to any known event; deduplicated by
                // `(event_id, member)` — a later RSVP simply overwrites the
                // earlier one (plain map/set semantics).
                let Some(ev) = self.events.get_mut(&event_id) else {
                    return self.ignore("événement inconnu");
                };
                if interested {
                    ev.rsvps.insert(author);
                } else {
                    ev.rsvps.remove(&author);
                }
            }
            GroupOpBody::StickerAdd { name, file } => {
                if !has(perms::MANAGE_EMOJIS) {
                    return self.ignore("STICKER_ADD refusé");
                }
                if !is_valid_sticker_name(&name) {
                    return self.ignore("nom de sticker invalide");
                }
                // Mirrors AddEmoji exactly: replacing an existing name is
                // always allowed, only a *new* name beyond the cap is not.
                if !self.stickers.contains_key(&name) && self.stickers.len() >= MAX_STICKERS {
                    return self.ignore("trop de stickers (30 max)");
                }
                self.stickers.insert(name, file);
            }
            GroupOpBody::StickerRemove { name } => {
                if !has(perms::MANAGE_EMOJIS) {
                    return self.ignore("STICKER_REMOVE refusé");
                }
                if self.stickers.remove(&name).is_none() {
                    return self.ignore("sticker inconnu");
                }
            }
            GroupOpBody::SetMemberAvatar { avatar } => {
                // Self-service only: no moderator override, unlike
                // SetNickname — the target is always the author.
                match avatar {
                    Some(hash) => {
                        self.member_avatars.insert(author, hash);
                    }
                    None => {
                        self.member_avatars.remove(&author);
                    }
                }
            }
            GroupOpBody::PollCreate {
                poll_id,
                channel_id,
                msg_id,
            } => {
                // Gated on effective VIEW+SEND in `channel_id`, like a plain
                // message send (`GroupState::can_send_message` /
                // `accord_core::group::msg::require_send`) — bare
                // membership alone is *not* sufficient (D-048 fix HIGH-1):
                // without this gate, any member could squat all `MAX_POLLS`
                // slots via a channel they cannot even write to, permanently
                // denying polls to the rest of the group. `PollDelete` is
                // the recovery path once a slot is legitimately spent.
                // Unlike `require_send`, the timeout check is intentionally
                // NOT replicated here: no op-fold gate anywhere in this file
                // checks `is_timed_out` (an op's own `wall_ms` is informative
                // only, never trusted for security decisions) — timeout
                // enforcement is a message-layer concern, checked against the
                // receiver's local clock at compose/ingest time.
                let Some(channel) = self.channels.get(&channel_id) else {
                    return self.ignore("salon inconnu");
                };
                let eff = self.permissions_in(&author, &channel_id);
                if eff & (perms::VIEW | perms::SEND) != (perms::VIEW | perms::SEND) {
                    return self.ignore("POLL_CREATE refusé (droit d'écriture)");
                }
                if channel.kind == ChannelKind::Announcement && eff & perms::MANAGE_CHANNELS == 0 {
                    return self.ignore("salon d'annonces en lecture seule");
                }
                // Dedup by id (like every other random-id-keyed create op)
                // makes the *first* PollCreate for a given `poll_id` in
                // Lamport order the one that wins, so a racing forgery from
                // another member can never hijack an already-registered
                // poll's authorship.
                if self.polls.contains_key(&poll_id) {
                    return self.ignore("identifiant de sondage déjà utilisé");
                }
                if self.polls.len() >= MAX_POLLS {
                    return self.ignore("trop de sondages (25 max)");
                }
                self.polls.insert(
                    poll_id,
                    Poll {
                        author,
                        channel_id,
                        msg_id,
                        closed: false,
                        votes: BTreeMap::new(),
                    },
                );
            }
            GroupOpBody::PollVote {
                poll_id,
                option_index,
            } => {
                // Any member may vote on any known poll (mirrors EventRsvp);
                // structural bound only, see `Poll::votes` doc.
                if option_index as usize >= MAX_POLL_OPTIONS {
                    return self.ignore("option de sondage hors bornes");
                }
                let Some(poll) = self.polls.get_mut(&poll_id) else {
                    return self.ignore("sondage inconnu");
                };
                if poll.closed {
                    return self.ignore("sondage clos");
                }
                poll.votes.insert(author, option_index);
            }
            GroupOpBody::PollClose { poll_id } => {
                let Some(poll) = self.polls.get_mut(&poll_id) else {
                    return self.ignore("sondage inconnu");
                };
                // Author-owns-or-manager-overrides, exactly like
                // EventEdit/EventDelete.
                if !has(perms::MANAGE_CHANNELS) && poll.author != author {
                    return self.ignore("POLL_CLOSE refusé");
                }
                poll.closed = true;
            }
            GroupOpBody::PollDelete { poll_id } => {
                let Some(current) = self.polls.get(&poll_id) else {
                    return self.ignore("sondage inconnu");
                };
                // Author-owns-or-manager-overrides, exactly like
                // EventDelete — mirrors it so the same 25-slot cap can be
                // recovered once a poll is no longer needed (D-048 fix
                // HIGH-1).
                if !has(perms::MANAGE_CHANNELS) && current.author != author {
                    return self.ignore("POLL_DELETE refusé");
                }
                self.polls.remove(&poll_id);
            }
            GroupOpBody::SetAutoModWords { words } => {
                // Same permission family as SetMeta/channels/categories —
                // AutoMod's rule list is server config, not per-member
                // state (see GroupState::automod_words doc + honest-P2P
                // caveat).
                if !has(perms::MANAGE_CHANNELS) {
                    return self.ignore("AUTOMOD_SET refusé");
                }
                // Defense in depth: the wire decode already caps the list
                // at MAX_AUTOMOD_WORDS, but the fold must never rely on
                // that alone (see MAX_AUTOMOD_WORD_CHARS doc).
                if words.len() > MAX_AUTOMOD_WORDS {
                    return self.ignore("trop de mots AutoMod (50 max)");
                }
                // Build the normalized replacement set in a local first —
                // any single malformed/spoofed word rejects the *whole* op
                // atomically, never a partial replacement of the list.
                let mut normalized = BTreeSet::new();
                for word in &words {
                    let lower = word.to_lowercase();
                    if !is_valid_automod_word(&lower) {
                        return self.ignore("mot AutoMod invalide");
                    }
                    normalized.insert(lower);
                }
                self.automod_words = normalized;
            }
            GroupOpBody::SetChannelSlowmode {
                channel_id,
                seconds,
            } => {
                if !has(perms::MANAGE_CHANNELS) {
                    return self.ignore("SET_CHANNEL_SLOWMODE refusé");
                }
                // Defense in depth: the wire decode already rejects
                // `seconds` beyond the ceiling, but the fold must never
                // rely on that alone (same policy as `MAX_AUTOMOD_WORDS`).
                if seconds > MAX_CHANNEL_SLOWMODE_SECS {
                    return self.ignore("mode lent hors bornes (6h max)");
                }
                let Some(ch) = self.channels.get_mut(&channel_id) else {
                    return self.ignore("salon inconnu");
                };
                ch.slowmode_secs = seconds;
            }
        }
        self.applied_ops += 1;
        Applied::Ok
    }

    /// Vrai si `who` est exempté du mode lent d'un salon : porteur de
    /// `MANAGE_CHANNELS` ou `MANAGE_MESSAGES` effectif dans ce salon
    /// (comportement Discord — un modérateur n'est jamais bridé par sa
    /// propre configuration). Overrides de salon compris, comme
    /// [`Self::permissions_in`].
    pub fn slowmode_exempt(&self, who: &[u8; 32], channel_id: &[u8; 16]) -> bool {
        self.permissions_in(who, channel_id) & (perms::MANAGE_CHANNELS | perms::MANAGE_MESSAGES)
            != 0
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
    /// Salon textuel utilisé par les tests de sondage (D-048) : tout membre
    /// y a VIEW+SEND par défaut (`DEFAULT_MEMBER_PERMS`), sauf override
    /// explicite dans un test donné.
    const POLL_CHAN: [u8; 16] = [0x70; 16];

    /// Ajoute [`POLL_CHAN`] (salon textuel) aux ops fournies, à `lamport`.
    fn add_poll_channel(ops: &mut Vec<GroupOp>, lamport: u64) {
        ops.push(signed(
            GroupOpBody::AddChannel {
                channel_id: POLL_CHAN,
                name: "général".into(),
                category: None,
                kind: ChannelKind::Text,
                position: 0,
            },
            FOUNDER,
            lamport,
        ));
    }

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
    fn voice_moderate_requires_kick_and_respects_hierarchy() {
        let mut ops = base_ops();
        // Alice (plain member) cannot voice-moderate Bob.
        ops.push(signed(
            GroupOpBody::VoiceModerate {
                member: BOB,
                mute: true,
                deafen: false,
            },
            ALICE,
            4,
        ));
        assert_eq!(
            GroupState::fold(&ops).voice_moderation_of(&BOB),
            VoiceModeration::default(),
            "plain member cannot voice-moderate"
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
        // Alice mutes+deafens Bob (below her) but not the founder.
        ops.push(signed(
            GroupOpBody::VoiceModerate {
                member: BOB,
                mute: true,
                deafen: true,
            },
            ALICE,
            7,
        ));
        ops.push(signed(
            GroupOpBody::VoiceModerate {
                member: FOUNDER,
                mute: true,
                deafen: true,
            },
            ALICE,
            8,
        ));
        let st = GroupState::fold(&ops);
        assert_eq!(
            st.voice_moderation_of(&BOB),
            VoiceModeration {
                mute: true,
                deafen: true,
            },
        );
        assert_eq!(
            st.voice_moderation_of(&FOUNDER),
            VoiceModeration::default(),
            "founder untouchable"
        );

        // Clearing both flags removes the entry entirely.
        let mut cleared = ops.clone();
        cleared.push(signed(
            GroupOpBody::VoiceModerate {
                member: BOB,
                mute: false,
                deafen: false,
            },
            FOUNDER,
            9,
        ));
        assert!(GroupState::fold(&cleared).voice_moderation.is_empty());

        // A non-member target is ignored.
        let mut stranger = ops.clone();
        stranger.push(signed(
            GroupOpBody::VoiceModerate {
                member: [0xEE; 32],
                mute: true,
                deafen: false,
            },
            FOUNDER,
            9,
        ));
        assert!(!GroupState::fold(&stranger)
            .voice_moderation
            .contains_key(&[0xEE; 32]));
    }

    #[test]
    fn priority_speaker_is_explicit_never_implied_by_admin() {
        let mut ops = base_ops();
        let st = GroupState::fold(&ops);
        // Le fondateur a ALL_PERMS mais n'est PAS orateur prioritaire.
        assert!(!st.is_priority_speaker(&FOUNDER));
        assert!(!st.is_priority_speaker(&ALICE));

        // Un rôle accordant explicitement la permission la confère.
        ops.push(signed(
            GroupOpBody::AddRole {
                role_id: [4; 16],
                name: "Orateur".into(),
                color: 0,
                position: 1,
                permissions: perms::PRIORITY_SPEAKER,
            },
            FOUNDER,
            4,
        ));
        ops.push(signed(
            GroupOpBody::AssignRole {
                member: ALICE,
                role_id: [4; 16],
            },
            FOUNDER,
            5,
        ));
        let st = GroupState::fold(&ops);
        assert!(st.is_priority_speaker(&ALICE));
        assert!(!st.is_priority_speaker(&FOUNDER));
        // Un non-membre n'est jamais prioritaire.
        assert!(!st.is_priority_speaker(&[0xEE; 32]));
    }

    #[test]
    fn voice_moderation_cleared_when_member_removed() {
        let mut ops = base_ops();
        ops.push(signed(
            GroupOpBody::VoiceModerate {
                member: BOB,
                mute: true,
                deafen: false,
            },
            FOUNDER,
            4,
        ));
        ops.push(signed(GroupOpBody::Kick { member: BOB }, FOUNDER, 5));
        let st = GroupState::fold(&ops);
        assert!(!st.is_member(&BOB));
        assert!(
            st.voice_moderation.is_empty(),
            "a kick clears the member's voice moderation"
        );
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
                GroupState::fold(&sp)
                    .nicknames
                    .get(&BOB)
                    .map(String::as_str),
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
    fn is_valid_display_label_rejects_control_spoofing_empty_and_overlong() {
        assert!(is_valid_display_label("Guilde", 100));
        assert!(!is_valid_display_label("", 100), "empty is not valid");
        assert!(
            !is_valid_display_label(&"x".repeat(101), 100),
            "over the max_chars bound is rejected"
        );
        assert!(is_valid_display_label(&"x".repeat(100), 100));
        for bad in [
            "bad\u{7}name",
            "\u{202E}pirate",
            "z\u{200B}ero",
            "\u{FEFF}bom",
        ] {
            assert!(
                !is_valid_display_label(bad, 100),
                "{bad:?} must be rejected"
            );
        }
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

    // ---- D-047 : événements, stickers, avatar de serveur, banner_color ----

    #[test]
    fn set_meta_banner_color_preserved_and_permission_gated() {
        let mut ops = base_ops();
        ops.push(signed(
            GroupOpBody::SetMeta {
                name: "Salon".into(),
                icon: None,
                banner_color: Some(0x5865F2),
            },
            FOUNDER,
            4,
        ));
        let st = GroupState::fold(&ops);
        assert_eq!(st.banner_color, Some(0x5865F2));

        // A plain member cannot change server metadata.
        let mut denied = ops.clone();
        denied.push(signed(
            GroupOpBody::SetMeta {
                name: "Piraté".into(),
                icon: None,
                banner_color: Some(0),
            },
            ALICE,
            5,
        ));
        let st2 = GroupState::fold(&denied);
        assert_eq!(st2.banner_color, Some(0x5865F2), "denied, unchanged");
        assert_eq!(st2.name, "Salon");
    }

    #[test]
    fn event_create_requires_manage_channels_and_validates_fields() {
        let mut ops = base_ops();
        // Plain member cannot create an event.
        ops.push(signed(
            GroupOpBody::EventCreate {
                event_id: [1; 16],
                title: "Soirée".into(),
                description: String::new(),
                start_ms: 1_000,
                channel_id: None,
            },
            ALICE,
            4,
        ));
        assert!(GroupState::fold(&ops).events.is_empty());

        // Founder (MANAGE_CHANNELS via ALL_PERMS) can.
        ops.push(signed(
            GroupOpBody::EventCreate {
                event_id: [2; 16],
                title: "Soirée jeux".into(),
                description: "Amenez vos manettes.".into(),
                start_ms: 1_000,
                channel_id: None,
            },
            FOUNDER,
            5,
        ));
        let st = GroupState::fold(&ops);
        assert_eq!(st.events.len(), 1);
        let ev = st.events.get(&[2; 16]).unwrap();
        assert_eq!(ev.title, "Soirée jeux");
        assert_eq!(ev.author, FOUNDER);
        assert!(ev.rsvps.is_empty());

        // Too-short title (1 char) is rejected.
        let mut bad_title = ops.clone();
        bad_title.push(signed(
            GroupOpBody::EventCreate {
                event_id: [3; 16],
                title: "X".into(),
                description: String::new(),
                start_ms: 1_000,
                channel_id: None,
            },
            FOUNDER,
            6,
        ));
        assert!(!GroupState::fold(&bad_title).events.contains_key(&[3; 16]));

        // Over-long description is rejected.
        let mut bad_desc = ops.clone();
        bad_desc.push(signed(
            GroupOpBody::EventCreate {
                event_id: [4; 16],
                title: "Valide".into(),
                description: "x".repeat(MAX_EVENT_DESC_CHARS + 1),
                start_ms: 1_000,
                channel_id: None,
            },
            FOUNDER,
            6,
        ));
        assert!(!GroupState::fold(&bad_desc).events.contains_key(&[4; 16]));

        // Spoofing characters in the title are rejected, like nicknames.
        let mut spoof = ops.clone();
        spoof.push(signed(
            GroupOpBody::EventCreate {
                event_id: [5; 16],
                title: "\u{202E}soirée".into(),
                description: String::new(),
                start_ms: 1_000,
                channel_id: None,
            },
            FOUNDER,
            6,
        ));
        assert!(!GroupState::fold(&spoof).events.contains_key(&[5; 16]));

        // start_ms beyond the timeout ceiling is rejected outright (no
        // silent clamp — unlike a timeout, a garbled event date has no
        // graceful degrade).
        let mut bad_date = ops.clone();
        bad_date.push(signed(
            GroupOpBody::EventCreate {
                event_id: [6; 16],
                title: "Valide".into(),
                description: String::new(),
                start_ms: MAX_TIMEOUT_UNTIL_MS + 1,
                channel_id: None,
            },
            FOUNDER,
            6,
        ));
        assert!(!GroupState::fold(&bad_date).events.contains_key(&[6; 16]));

        // An unknown channel_id is rejected.
        let mut bad_chan = ops.clone();
        bad_chan.push(signed(
            GroupOpBody::EventCreate {
                event_id: [7; 16],
                title: "Valide".into(),
                description: String::new(),
                start_ms: 1_000,
                channel_id: Some([0xEE; 16]),
            },
            FOUNDER,
            6,
        ));
        assert!(!GroupState::fold(&bad_chan).events.contains_key(&[7; 16]));

        // A text channel (not voice) is rejected as the event's channel.
        let mut with_text_channel = ops.clone();
        with_text_channel.push(signed(
            GroupOpBody::AddChannel {
                channel_id: [9; 16],
                name: "texte".into(),
                category: None,
                kind: ChannelKind::Text,
                position: 0,
            },
            FOUNDER,
            6,
        ));
        with_text_channel.push(signed(
            GroupOpBody::EventCreate {
                event_id: [8; 16],
                title: "Valide".into(),
                description: String::new(),
                start_ms: 1_000,
                channel_id: Some([9; 16]),
            },
            FOUNDER,
            7,
        ));
        assert!(!GroupState::fold(&with_text_channel)
            .events
            .contains_key(&[8; 16]));

        // A voice channel is accepted.
        let mut with_voice_channel = ops.clone();
        with_voice_channel.push(signed(
            GroupOpBody::AddChannel {
                channel_id: [10; 16],
                name: "vocal".into(),
                category: None,
                kind: ChannelKind::Voice,
                position: 0,
            },
            FOUNDER,
            6,
        ));
        with_voice_channel.push(signed(
            GroupOpBody::EventCreate {
                event_id: [11; 16],
                title: "Valide".into(),
                description: String::new(),
                start_ms: 1_000,
                channel_id: Some([10; 16]),
            },
            FOUNDER,
            7,
        ));
        assert_eq!(
            GroupState::fold(&with_voice_channel)
                .events
                .get(&[11; 16])
                .and_then(|e| e.channel_id),
            Some([10; 16])
        );
    }

    #[test]
    fn event_cap_blocks_new_but_edit_and_delete_still_work() {
        let mut ops = base_ops();
        for i in 0..(MAX_EVENTS as u64) {
            let mut id = [0u8; 16];
            id[0..8].copy_from_slice(&i.to_be_bytes());
            ops.push(signed(
                GroupOpBody::EventCreate {
                    event_id: id,
                    title: format!("Événement {i}"),
                    description: String::new(),
                    start_ms: 1_000,
                    channel_id: None,
                },
                FOUNDER,
                10 + i,
            ));
        }
        let mut one_too_many = [0xFFu8; 16];
        one_too_many[0] = 0xAA;
        ops.push(signed(
            GroupOpBody::EventCreate {
                event_id: one_too_many,
                title: "Un de trop".into(),
                description: String::new(),
                start_ms: 1_000,
                channel_id: None,
            },
            FOUNDER,
            200,
        ));
        let st = GroupState::fold(&ops);
        assert_eq!(st.events.len(), MAX_EVENTS);
        assert!(!st.events.contains_key(&one_too_many));

        // Editing and deleting an existing event still works despite the cap.
        let mut first_id = [0u8; 16];
        first_id[0..8].copy_from_slice(&0u64.to_be_bytes());
        let mut edited = ops.clone();
        edited.push(signed(
            GroupOpBody::EventEdit {
                event_id: first_id,
                title: "Renommé".into(),
                description: String::new(),
                start_ms: 2_000,
                channel_id: None,
            },
            FOUNDER,
            201,
        ));
        assert_eq!(
            GroupState::fold(&edited)
                .events
                .get(&first_id)
                .unwrap()
                .title,
            "Renommé"
        );
        let mut deleted = ops.clone();
        deleted.push(signed(
            GroupOpBody::EventDelete { event_id: first_id },
            FOUNDER,
            201,
        ));
        assert_eq!(GroupState::fold(&deleted).events.len(), MAX_EVENTS - 1);
    }

    #[test]
    fn event_edit_delete_allowed_for_author_or_manager_only() {
        let mut ops = base_ops();
        // Bob (plain member) creates nothing directly — the founder creates
        // on his behalf isn't possible; instead exercise: Alice authors via
        // a manager role, Bob (no role) cannot touch Alice's event.
        ops.push(signed(
            GroupOpBody::AddRole {
                role_id: [2; 16],
                name: "Orga".into(),
                color: 0,
                position: 5,
                permissions: perms::MANAGE_CHANNELS,
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
        ops.push(signed(
            GroupOpBody::EventCreate {
                event_id: [1; 16],
                title: "Événement d'Alice".into(),
                description: String::new(),
                start_ms: 1_000,
                channel_id: None,
            },
            ALICE,
            6,
        ));

        // Bob (plain member, not the author) cannot edit or delete it.
        let mut bob_edit = ops.clone();
        bob_edit.push(signed(
            GroupOpBody::EventEdit {
                event_id: [1; 16],
                title: "Piraté".into(),
                description: String::new(),
                start_ms: 1_000,
                channel_id: None,
            },
            BOB,
            7,
        ));
        assert_eq!(
            GroupState::fold(&bob_edit)
                .events
                .get(&[1; 16])
                .unwrap()
                .title,
            "Événement d'Alice"
        );
        let mut bob_delete = ops.clone();
        bob_delete.push(signed(
            GroupOpBody::EventDelete { event_id: [1; 16] },
            BOB,
            7,
        ));
        assert!(GroupState::fold(&bob_delete).events.contains_key(&[1; 16]));

        // Alice (the author) can edit her own event without MANAGE_CHANNELS
        // being re-checked (she holds it here, but the author path is what's
        // under test — remove her role and confirm author-only still works).
        let mut alice_edits_own = ops.clone();
        alice_edits_own.push(signed(
            GroupOpBody::UnassignRole {
                member: ALICE,
                role_id: [2; 16],
            },
            FOUNDER,
            7,
        ));
        alice_edits_own.push(signed(
            GroupOpBody::EventEdit {
                event_id: [1; 16],
                title: "Renommé par l'autrice".into(),
                description: String::new(),
                start_ms: 1_000,
                channel_id: None,
            },
            ALICE,
            8,
        ));
        assert_eq!(
            GroupState::fold(&alice_edits_own)
                .events
                .get(&[1; 16])
                .unwrap()
                .title,
            "Renommé par l'autrice"
        );

        // The founder (MANAGE_CHANNELS via ALL_PERMS) can delete anyone's
        // event, even without being the author.
        let mut founder_deletes = ops.clone();
        founder_deletes.push(signed(
            GroupOpBody::EventDelete { event_id: [1; 16] },
            FOUNDER,
            7,
        ));
        assert!(!GroupState::fold(&founder_deletes)
            .events
            .contains_key(&[1; 16]));
    }

    #[test]
    fn event_rsvp_dedups_by_member_and_is_order_independent() {
        let mut ops = base_ops();
        ops.push(signed(
            GroupOpBody::EventCreate {
                event_id: [1; 16],
                title: "Soirée".into(),
                description: String::new(),
                start_ms: 1_000,
                channel_id: None,
            },
            FOUNDER,
            4,
        ));
        ops.push(signed(
            GroupOpBody::EventRsvp {
                event_id: [1; 16],
                interested: true,
            },
            ALICE,
            5,
        ));
        ops.push(signed(
            GroupOpBody::EventRsvp {
                event_id: [1; 16],
                interested: true,
            },
            BOB,
            6,
        ));
        let st = GroupState::fold(&ops);
        let ev = st.events.get(&[1; 16]).unwrap();
        assert_eq!(ev.rsvps.len(), 2);
        assert!(ev.rsvps.contains(&ALICE) && ev.rsvps.contains(&BOB));

        // Bob withdraws: dedup keeps a single entry per member, this clears
        // it regardless of how many times he RSVP'd before.
        let mut withdrawn = ops.clone();
        withdrawn.push(signed(
            GroupOpBody::EventRsvp {
                event_id: [1; 16],
                interested: false,
            },
            BOB,
            7,
        ));
        let st2 = GroupState::fold(&withdrawn);
        let ev2 = st2.events.get(&[1; 16]).unwrap();
        assert_eq!(ev2.rsvps.len(), 1);
        assert!(ev2.rsvps.contains(&ALICE) && !ev2.rsvps.contains(&BOB));

        // Order-independence: shuffled ops converge to the same state.
        let mut shuffled = withdrawn.clone();
        shuffled.reverse();
        assert_eq!(GroupState::fold(&withdrawn), GroupState::fold(&shuffled));

        // RSVP to an unknown event is ignored (no panic, no phantom entry).
        let mut unknown = ops.clone();
        unknown.push(signed(
            GroupOpBody::EventRsvp {
                event_id: [0xEE; 16],
                interested: true,
            },
            ALICE,
            7,
        ));
        assert!(!GroupState::fold(&unknown).events.contains_key(&[0xEE; 16]));
    }

    /// Hygiène au départ d'un membre (revue D-047) : ses RSVP sont retirés de
    /// tous les événements — aucune clé publique orpheline dans l'état
    /// répliqué — tandis que les événements qu'il a AUTORÉS restent, gérables
    /// par le fondateur ou tout MANAGE_CHANNELS (découplage type DelChannel).
    #[test]
    fn kick_and_leave_clear_event_rsvps_but_keep_authored_events() {
        let mut ops = base_ops();
        ops.push(signed(
            GroupOpBody::EventCreate {
                event_id: [1; 16],
                title: "Soirée".into(),
                description: String::new(),
                start_ms: 1_000,
                channel_id: None,
            },
            FOUNDER,
            4,
        ));
        ops.push(signed(
            GroupOpBody::EventRsvp {
                event_id: [1; 16],
                interested: true,
            },
            BOB,
            5,
        ));

        // Kick : le RSVP de Bob disparaît, l'événement du fondateur reste.
        let mut kicked = ops.clone();
        kicked.push(signed(GroupOpBody::Kick { member: BOB }, FOUNDER, 6));
        let st = GroupState::fold(&kicked);
        let ev = st.events.get(&[1; 16]).expect("événement conservé");
        assert!(!ev.rsvps.contains(&BOB));

        // Leave : même hygiène quand le membre part de lui-même.
        let mut left = ops.clone();
        left.push(signed(GroupOpBody::Leave, BOB, 6));
        let st2 = GroupState::fold(&left);
        assert!(!st2.events.get(&[1; 16]).unwrap().rsvps.contains(&BOB));

        // Les événements autorés par le partant restent (gérables ensuite).
        let mut authored = base_ops();
        authored.push(signed(
            GroupOpBody::AddRole {
                role_id: [9; 16],
                name: "Orga".into(),
                color: 0x00FF00,
                position: 10,
                permissions: perms::MANAGE_CHANNELS,
            },
            FOUNDER,
            4,
        ));
        authored.push(signed(
            GroupOpBody::AssignRole {
                member: ALICE,
                role_id: [9; 16],
            },
            FOUNDER,
            5,
        ));
        authored.push(signed(
            GroupOpBody::EventCreate {
                event_id: [2; 16],
                title: "Raid".into(),
                description: String::new(),
                start_ms: 2_000,
                channel_id: None,
            },
            ALICE,
            6,
        ));
        authored.push(signed(GroupOpBody::Kick { member: ALICE }, FOUNDER, 7));
        let st3 = GroupState::fold(&authored);
        assert!(st3.events.contains_key(&[2; 16]));
        // …et restent supprimables par le fondateur après le départ.
        authored.push(signed(
            GroupOpBody::EventDelete { event_id: [2; 16] },
            FOUNDER,
            8,
        ));
        assert!(!GroupState::fold(&authored).events.contains_key(&[2; 16]));
    }

    #[test]
    fn del_channel_clears_dangling_event_reference() {
        let mut ops = base_ops();
        ops.push(signed(
            GroupOpBody::AddChannel {
                channel_id: [7; 16],
                name: "vocal".into(),
                category: None,
                kind: ChannelKind::Voice,
                position: 0,
            },
            FOUNDER,
            4,
        ));
        ops.push(signed(
            GroupOpBody::EventCreate {
                event_id: [1; 16],
                title: "Soirée vocale".into(),
                description: String::new(),
                start_ms: 1_000,
                channel_id: Some([7; 16]),
            },
            FOUNDER,
            5,
        ));
        ops.push(signed(
            GroupOpBody::DelChannel {
                channel_id: [7; 16],
            },
            FOUNDER,
            6,
        ));
        let st = GroupState::fold(&ops);
        assert_eq!(
            st.events.get(&[1; 16]).unwrap().channel_id,
            None,
            "event survives, uncoupled from the deleted channel"
        );
    }

    #[test]
    fn sticker_add_replace_delete_permission_and_cap() {
        let mut ops = base_ops();
        // Plain member cannot add.
        ops.push(signed(
            GroupOpBody::StickerAdd {
                name: "wave".into(),
                file: [1; 32],
            },
            ALICE,
            4,
        ));
        assert!(GroupState::fold(&ops).stickers.is_empty());

        // Founder (MANAGE_EMOJIS via ALL_PERMS) can add and replace.
        ops.push(signed(
            GroupOpBody::StickerAdd {
                name: "wave".into(),
                file: [1; 32],
            },
            FOUNDER,
            5,
        ));
        ops.push(signed(
            GroupOpBody::StickerAdd {
                name: "wave".into(),
                file: [2; 32],
            },
            FOUNDER,
            6,
        ));
        let st = GroupState::fold(&ops);
        assert_eq!(st.stickers.get("wave"), Some(&[2; 32]));
        assert_eq!(st.stickers.len(), 1);

        // Invalid name (uppercase) is rejected.
        let mut bad_name = ops.clone();
        bad_name.push(signed(
            GroupOpBody::StickerAdd {
                name: "Bad".into(),
                file: [3; 32],
            },
            FOUNDER,
            7,
        ));
        assert!(!GroupState::fold(&bad_name).stickers.contains_key("Bad"));

        // Removal by the founder.
        let mut removed = ops.clone();
        removed.push(signed(
            GroupOpBody::StickerRemove {
                name: "wave".into(),
            },
            FOUNDER,
            7,
        ));
        assert!(GroupState::fold(&removed).stickers.is_empty());

        // Cap: MAX_STICKERS distinct names allowed, one more is ignored,
        // but replacing an existing name still works past the cap.
        let mut capped = base_ops();
        for i in 0..(MAX_STICKERS as u64) {
            capped.push(signed(
                GroupOpBody::StickerAdd {
                    name: format!("s{i}"),
                    file: [i as u8; 32],
                },
                FOUNDER,
                10 + i,
            ));
        }
        capped.push(signed(
            GroupOpBody::StickerAdd {
                name: "one_too_many".into(),
                file: [9; 32],
            },
            FOUNDER,
            100,
        ));
        capped.push(signed(
            GroupOpBody::StickerAdd {
                name: "s0".into(),
                file: [42; 32],
            },
            FOUNDER,
            101,
        ));
        let st_capped = GroupState::fold(&capped);
        assert_eq!(st_capped.stickers.len(), MAX_STICKERS);
        assert!(!st_capped.stickers.contains_key("one_too_many"));
        assert_eq!(st_capped.stickers.get("s0"), Some(&[42; 32]));
    }

    #[test]
    fn set_member_avatar_is_self_service_only_and_cleared_on_departure() {
        let mut ops = base_ops();
        // Bob sets his own avatar.
        ops.push(signed(
            GroupOpBody::SetMemberAvatar {
                avatar: Some([1; 32]),
            },
            BOB,
            4,
        ));
        let st = GroupState::fold(&ops);
        assert_eq!(st.member_avatars.get(&BOB), Some(&[1; 32]));

        // Alice cannot set Bob's avatar on his behalf — the op has no
        // `member` field at all, so this is structurally impossible; the
        // closest adversarial equivalent is Alice authoring her own
        // `SetMemberAvatar`, which only ever affects Alice.
        let mut alice_sets_own = ops.clone();
        alice_sets_own.push(signed(
            GroupOpBody::SetMemberAvatar {
                avatar: Some([2; 32]),
            },
            ALICE,
            5,
        ));
        let st2 = GroupState::fold(&alice_sets_own);
        assert_eq!(st2.member_avatars.get(&BOB), Some(&[1; 32]), "unaffected");
        assert_eq!(st2.member_avatars.get(&ALICE), Some(&[2; 32]));

        // Clearing with None removes the entry.
        let mut cleared = ops.clone();
        cleared.push(signed(
            GroupOpBody::SetMemberAvatar { avatar: None },
            BOB,
            5,
        ));
        assert!(GroupState::fold(&cleared).member_avatars.is_empty());

        // A kick clears the departed member's avatar.
        let mut kicked = ops.clone();
        kicked.push(signed(GroupOpBody::Kick { member: BOB }, FOUNDER, 5));
        assert!(GroupState::fold(&kicked).member_avatars.is_empty());
    }

    /// D-048 : n'importe quel membre peut créer un sondage (mirrors sending
    /// a message — no elevated permission, unlike events/stickers) **du
    /// moment qu'il a VIEW+SEND effectifs dans le salon visé**. Dedup par
    /// `poll_id` (id déjà pris = ignoré) et plafond `MAX_POLLS`.
    #[test]
    fn poll_create_is_self_service_and_enforces_dedup_and_cap() {
        let mut ops = base_ops();
        add_poll_channel(&mut ops, 4);
        // Bob (simple membre) crée un sondage : aucune permission élevée
        // requise au-delà de VIEW+SEND (défaut de tout membre), contrairement
        // à un événement ou un sticker qui exigent MANAGE_CHANNELS/
        // MANAGE_EMOJIS.
        ops.push(signed(
            GroupOpBody::PollCreate {
                poll_id: [1; 16],
                channel_id: POLL_CHAN,
                msg_id: [100; 16],
            },
            BOB,
            5,
        ));
        let st = GroupState::fold(&ops);
        let poll = st.polls.get(&[1; 16]).expect("sondage créé");
        assert_eq!(poll.author, BOB);
        assert_eq!(poll.channel_id, POLL_CHAN);
        assert_eq!(poll.msg_id, [100; 16]);
        assert!(!poll.closed);
        assert!(poll.votes.is_empty());

        // A second PollCreate with the same id is ignored — the first one
        // in Lamport order wins deterministically, so a racing forgery from
        // another member can never steal an already-registered poll_id.
        let mut hijack = ops.clone();
        hijack.push(signed(
            GroupOpBody::PollCreate {
                poll_id: [1; 16],
                channel_id: POLL_CHAN,
                msg_id: [101; 16],
            },
            ALICE,
            6,
        ));
        let st2 = GroupState::fold(&hijack);
        assert_eq!(st2.polls.get(&[1; 16]).unwrap().author, BOB, "unhijacked");
        assert_eq!(st2.polls.len(), 1);

        // Cap: MAX_POLLS distinct ids allowed, one more is ignored.
        let mut capped = base_ops();
        add_poll_channel(&mut capped, 4);
        for i in 0..(MAX_POLLS as u64) {
            let mut id = [0u8; 16];
            id[0..8].copy_from_slice(&i.to_be_bytes());
            capped.push(signed(
                GroupOpBody::PollCreate {
                    poll_id: id,
                    channel_id: POLL_CHAN,
                    msg_id: id,
                },
                FOUNDER,
                10 + i,
            ));
        }
        let mut one_too_many = [0xFFu8; 16];
        one_too_many[15] = 1;
        capped.push(signed(
            GroupOpBody::PollCreate {
                poll_id: one_too_many,
                channel_id: POLL_CHAN,
                msg_id: one_too_many,
            },
            FOUNDER,
            100,
        ));
        let st_capped = GroupState::fold(&capped);
        assert_eq!(st_capped.polls.len(), MAX_POLLS);
        assert!(!st_capped.polls.contains_key(&one_too_many));
    }

    /// D-048 fix HIGH-1 : `PollCreate` sans VIEW+SEND effectifs dans
    /// `channel_id` est ignoré au repli, exactement comme un message qui
    /// serait refusé par `require_send`/`can_send_message` — la simple
    /// appartenance au groupe ne suffit plus (avant ce correctif, n'importe
    /// quel membre pouvait squatter les 25 emplacements de `MAX_POLLS` via
    /// un salon où il n'avait même pas le droit d'écrire, DoS permanent sur
    /// les sondages du reste du groupe).
    #[test]
    fn poll_create_requires_send_permission_in_channel() {
        // Salon inconnu : ignoré, jamais de panique.
        let mut unknown_channel = base_ops();
        unknown_channel.push(signed(
            GroupOpBody::PollCreate {
                poll_id: [1; 16],
                channel_id: [0xEE; 16],
                msg_id: [2; 16],
            },
            BOB,
            4,
        ));
        assert!(GroupState::fold(&unknown_channel).polls.is_empty());

        // Salon connu mais SEND explicitement refusé par override de rôle :
        // Alice (porteuse du rôle "Muet") ne peut pas y créer de sondage.
        let mut denied = base_ops();
        add_poll_channel(&mut denied, 4);
        denied.push(signed(
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
        denied.push(signed(
            GroupOpBody::AssignRole {
                member: ALICE,
                role_id: [3; 16],
            },
            FOUNDER,
            6,
        ));
        denied.push(signed(
            GroupOpBody::SetChannelPerms {
                channel_id: POLL_CHAN,
                role_id: [3; 16],
                allow: 0,
                deny: perms::SEND,
            },
            FOUNDER,
            7,
        ));
        denied.push(signed(
            GroupOpBody::PollCreate {
                poll_id: [1; 16],
                channel_id: POLL_CHAN,
                msg_id: [2; 16],
            },
            ALICE,
            8,
        ));
        let st_denied = GroupState::fold(&denied);
        assert!(
            st_denied.polls.is_empty(),
            "SEND refusé : aucun sondage créé, aucun emplacement consommé"
        );

        // Same channel, same op, but a member with the default VIEW+SEND
        // (Bob, untouched by the override) succeeds — proves the gate is on
        // capability, not on the channel/op shape itself.
        let mut allowed = denied.clone();
        allowed.push(signed(
            GroupOpBody::PollCreate {
                poll_id: [9; 16],
                channel_id: POLL_CHAN,
                msg_id: [10; 16],
            },
            BOB,
            9,
        ));
        assert!(GroupState::fold(&allowed).polls.contains_key(&[9; 16]));
    }

    /// D-048 : choix unique dédoublonné par membre, order-independent au
    /// repli, vote sur sondage inconnu ignoré, `option_index` hors bornes
    /// (>= MAX_POLL_OPTIONS) ignoré sans jamais paniquer (mirrors
    /// `event_rsvp_dedups_by_member_and_is_order_independent`).
    #[test]
    fn poll_vote_dedups_is_order_independent_and_rejects_unknown_or_out_of_range() {
        let mut ops = base_ops();
        add_poll_channel(&mut ops, 4);
        ops.push(signed(
            GroupOpBody::PollCreate {
                poll_id: [1; 16],
                channel_id: POLL_CHAN,
                msg_id: [50; 16],
            },
            FOUNDER,
            5,
        ));
        ops.push(signed(
            GroupOpBody::PollVote {
                poll_id: [1; 16],
                option_index: 2,
            },
            ALICE,
            6,
        ));
        ops.push(signed(
            GroupOpBody::PollVote {
                poll_id: [1; 16],
                option_index: 0,
            },
            BOB,
            7,
        ));
        let st = GroupState::fold(&ops);
        let poll = st.polls.get(&[1; 16]).unwrap();
        assert_eq!(poll.votes.len(), 2);
        assert_eq!(poll.votes.get(&ALICE), Some(&2));
        assert_eq!(poll.votes.get(&BOB), Some(&0));

        // Alice changes her mind: single choice replaces the earlier vote,
        // never accumulates.
        let mut changed = ops.clone();
        changed.push(signed(
            GroupOpBody::PollVote {
                poll_id: [1; 16],
                option_index: 1,
            },
            ALICE,
            8,
        ));
        let st2 = GroupState::fold(&changed);
        let poll2 = st2.polls.get(&[1; 16]).unwrap();
        assert_eq!(poll2.votes.len(), 2);
        assert_eq!(poll2.votes.get(&ALICE), Some(&1));

        // Order-independence: shuffled ops converge to the same state.
        let mut shuffled = changed.clone();
        shuffled.reverse();
        assert_eq!(GroupState::fold(&changed), GroupState::fold(&shuffled));

        // Vote on an unknown poll is ignored (no panic, no phantom entry).
        let mut unknown = ops.clone();
        unknown.push(signed(
            GroupOpBody::PollVote {
                poll_id: [0xEE; 16],
                option_index: 0,
            },
            ALICE,
            8,
        ));
        assert!(!GroupState::fold(&unknown).polls.contains_key(&[0xEE; 16]));

        // Out-of-range option_index (structural bound, MAX_POLL_OPTIONS=10)
        // is ignored: no vote recorded, no panic on the adversarial byte.
        let mut forged = ops.clone();
        forged.push(signed(
            GroupOpBody::PollVote {
                poll_id: [1; 16],
                option_index: 250,
            },
            ALICE,
            8,
        ));
        let st_forged = GroupState::fold(&forged);
        // Alice's earlier, valid vote (option 2) is untouched by the forgery.
        assert_eq!(
            st_forged.polls.get(&[1; 16]).unwrap().votes.get(&ALICE),
            Some(&2)
        );
    }

    /// D-048 : clôture réservée à l'auteur du sondage ou `MANAGE_CHANNELS`
    /// (comme `EventEdit`/`EventDelete`) ; une fois clos, les votes
    /// ultérieurs sont ignorés — dépouillement figé.
    #[test]
    fn poll_close_requires_author_or_manage_channels_and_freezes_votes() {
        let mut ops = base_ops();
        add_poll_channel(&mut ops, 4);
        // Bob (simple membre) crée et vote sur son propre sondage.
        ops.push(signed(
            GroupOpBody::PollCreate {
                poll_id: [1; 16],
                channel_id: POLL_CHAN,
                msg_id: [50; 16],
            },
            BOB,
            5,
        ));

        // Alice (simple membre, ni auteur ni MANAGE_CHANNELS) ne peut pas
        // clore le sondage de Bob.
        let mut denied = ops.clone();
        denied.push(signed(
            GroupOpBody::PollClose { poll_id: [1; 16] },
            ALICE,
            6,
        ));
        assert!(
            !GroupState::fold(&denied)
                .polls
                .get(&[1; 16])
                .unwrap()
                .closed
        );

        // Bob (auteur) peut clore son propre sondage.
        let mut author_closes = ops.clone();
        author_closes.push(signed(GroupOpBody::PollClose { poll_id: [1; 16] }, BOB, 6));
        assert!(
            GroupState::fold(&author_closes)
                .polls
                .get(&[1; 16])
                .unwrap()
                .closed
        );

        // Founder (MANAGE_CHANNELS via ALL_PERMS) can close someone else's
        // poll too.
        let mut manager_closes = ops.clone();
        manager_closes.push(signed(
            GroupOpBody::PollClose { poll_id: [1; 16] },
            FOUNDER,
            6,
        ));
        assert!(
            GroupState::fold(&manager_closes)
                .polls
                .get(&[1; 16])
                .unwrap()
                .closed
        );

        // Closing an unknown poll is ignored without panic.
        let mut unknown = base_ops();
        unknown.push(signed(
            GroupOpBody::PollClose {
                poll_id: [0xEE; 16],
            },
            FOUNDER,
            4,
        ));
        assert!(GroupState::fold(&unknown).polls.is_empty());

        // Once closed, a subsequent vote (even from the poll's own author)
        // is ignored — the tally stays frozen.
        let mut then_vote = author_closes.clone();
        then_vote.push(signed(
            GroupOpBody::PollVote {
                poll_id: [1; 16],
                option_index: 3,
            },
            BOB,
            7,
        ));
        let st = GroupState::fold(&then_vote);
        assert!(st.polls.get(&[1; 16]).unwrap().votes.is_empty());

        // Close is idempotent (closing twice stays closed, no panic).
        let mut twice = author_closes.clone();
        twice.push(signed(
            GroupOpBody::PollClose { poll_id: [1; 16] },
            FOUNDER,
            7,
        ));
        assert!(GroupState::fold(&twice).polls.get(&[1; 16]).unwrap().closed);
    }

    /// D-048 fix HIGH-1 : `PollDelete` (0x2A) mirrors `EventDelete` exactly
    /// (auteur ou `MANAGE_CHANNELS`), et sa suppression libère l'emplacement
    /// compté par `MAX_POLLS` — seul moyen de récupérer un emplacement une
    /// fois un sondage créé.
    #[test]
    fn poll_delete_requires_author_or_manage_channels_and_recovers_cap() {
        let mut ops = base_ops();
        add_poll_channel(&mut ops, 4);
        ops.push(signed(
            GroupOpBody::PollCreate {
                poll_id: [1; 16],
                channel_id: POLL_CHAN,
                msg_id: [50; 16],
            },
            BOB,
            5,
        ));

        // Alice (ni auteur ni MANAGE_CHANNELS) ne peut pas supprimer le
        // sondage de Bob.
        let mut denied = ops.clone();
        denied.push(signed(
            GroupOpBody::PollDelete { poll_id: [1; 16] },
            ALICE,
            6,
        ));
        assert!(
            GroupState::fold(&denied).polls.contains_key(&[1; 16]),
            "refusé : le sondage doit survivre"
        );

        // Bob (auteur) peut supprimer son propre sondage.
        let mut author_deletes = ops.clone();
        author_deletes.push(signed(GroupOpBody::PollDelete { poll_id: [1; 16] }, BOB, 6));
        assert!(!GroupState::fold(&author_deletes)
            .polls
            .contains_key(&[1; 16]));

        // Founder (MANAGE_CHANNELS via ALL_PERMS) can delete someone else's
        // poll too.
        let mut manager_deletes = ops.clone();
        manager_deletes.push(signed(
            GroupOpBody::PollDelete { poll_id: [1; 16] },
            FOUNDER,
            6,
        ));
        assert!(!GroupState::fold(&manager_deletes)
            .polls
            .contains_key(&[1; 16]));

        // Deleting an unknown poll is ignored without panic.
        let mut unknown = base_ops();
        unknown.push(signed(
            GroupOpBody::PollDelete {
                poll_id: [0xEE; 16],
            },
            FOUNDER,
            4,
        ));
        assert!(GroupState::fold(&unknown).polls.is_empty());

        // Cap recovery: fill MAX_POLLS, deleting one frees exactly one slot
        // for a fresh PollCreate that would otherwise be ignored.
        let mut capped = base_ops();
        add_poll_channel(&mut capped, 4);
        for i in 0..(MAX_POLLS as u64) {
            let mut id = [0u8; 16];
            id[0..8].copy_from_slice(&i.to_be_bytes());
            capped.push(signed(
                GroupOpBody::PollCreate {
                    poll_id: id,
                    channel_id: POLL_CHAN,
                    msg_id: id,
                },
                FOUNDER,
                10 + i,
            ));
        }
        let st_full = GroupState::fold(&capped);
        assert_eq!(st_full.polls.len(), MAX_POLLS);

        // At cap: one more PollCreate is ignored.
        let mut still_full = capped.clone();
        let fresh_id = [0xAAu8; 16];
        still_full.push(signed(
            GroupOpBody::PollCreate {
                poll_id: fresh_id,
                channel_id: POLL_CHAN,
                msg_id: fresh_id,
            },
            FOUNDER,
            200,
        ));
        assert_eq!(GroupState::fold(&still_full).polls.len(), MAX_POLLS);
        assert!(!GroupState::fold(&still_full).polls.contains_key(&fresh_id));

        // Delete one existing poll, then the same fresh PollCreate succeeds
        // — the cap recovered exactly one slot, no more, no less.
        let mut recovered = capped.clone();
        let first_id = [0u8; 16];
        recovered.push(signed(
            GroupOpBody::PollDelete { poll_id: first_id },
            FOUNDER,
            150,
        ));
        recovered.push(signed(
            GroupOpBody::PollCreate {
                poll_id: fresh_id,
                channel_id: POLL_CHAN,
                msg_id: fresh_id,
            },
            FOUNDER,
            200,
        ));
        let st_recovered = GroupState::fold(&recovered);
        assert_eq!(st_recovered.polls.len(), MAX_POLLS);
        assert!(!st_recovered.polls.contains_key(&first_id));
        assert!(st_recovered.polls.contains_key(&fresh_id));
    }

    /// Hygiène au départ d'un membre (D-048, mirrors
    /// `kick_and_leave_clear_event_rsvps_but_keep_authored_events`) : ses
    /// votes sont retirés de tous les sondages, tandis que les sondages
    /// qu'il a AUTORÉS restent, clôturables par le fondateur ou tout
    /// MANAGE_CHANNELS.
    #[test]
    fn kick_and_leave_clear_poll_votes_but_keep_authored_polls() {
        let mut ops = base_ops();
        add_poll_channel(&mut ops, 4);
        ops.push(signed(
            GroupOpBody::PollCreate {
                poll_id: [1; 16],
                channel_id: POLL_CHAN,
                msg_id: [50; 16],
            },
            FOUNDER,
            5,
        ));
        ops.push(signed(
            GroupOpBody::PollVote {
                poll_id: [1; 16],
                option_index: 1,
            },
            BOB,
            6,
        ));

        // Kick: Bob's vote disappears, the founder's poll survives.
        let mut kicked = ops.clone();
        kicked.push(signed(GroupOpBody::Kick { member: BOB }, FOUNDER, 7));
        let st = GroupState::fold(&kicked);
        let poll = st.polls.get(&[1; 16]).expect("sondage conservé");
        assert!(!poll.votes.contains_key(&BOB));

        // Leave: same hygiene when the member departs voluntarily.
        let mut left = ops.clone();
        left.push(signed(GroupOpBody::Leave, BOB, 7));
        let st2 = GroupState::fold(&left);
        assert!(!st2.polls.get(&[1; 16]).unwrap().votes.contains_key(&BOB));

        // Polls authored by the departed member survive, still closable by
        // the founder/MANAGE_CHANNELS afterwards.
        let mut authored = base_ops();
        add_poll_channel(&mut authored, 4);
        authored.push(signed(
            GroupOpBody::AddRole {
                role_id: [9; 16],
                name: "Orga".into(),
                color: 0x00FF00,
                position: 10,
                permissions: perms::MANAGE_CHANNELS,
            },
            FOUNDER,
            5,
        ));
        authored.push(signed(
            GroupOpBody::AssignRole {
                member: ALICE,
                role_id: [9; 16],
            },
            FOUNDER,
            6,
        ));
        authored.push(signed(
            GroupOpBody::PollCreate {
                poll_id: [2; 16],
                channel_id: POLL_CHAN,
                msg_id: [51; 16],
            },
            ALICE,
            7,
        ));
        authored.push(signed(GroupOpBody::Kick { member: ALICE }, FOUNDER, 8));
        let st3 = GroupState::fold(&authored);
        assert!(st3.polls.contains_key(&[2; 16]));
        assert!(!st3.polls.get(&[2; 16]).unwrap().closed);
        // …and the (now-departed) author's own poll is still closable by a
        // MANAGE_CHANNELS holder.
        authored.push(signed(
            GroupOpBody::PollClose { poll_id: [2; 16] },
            FOUNDER,
            9,
        ));
        assert!(
            GroupState::fold(&authored)
                .polls
                .get(&[2; 16])
                .unwrap()
                .closed
        );
    }

    #[test]
    fn automod_set_words_replaces_wholesale_normalizes_case_and_requires_manage_channels() {
        let mut ops = base_ops();
        // A plain member (no MANAGE_CHANNELS) cannot set the list.
        ops.push(signed(
            GroupOpBody::SetAutoModWords {
                words: vec!["spam".into()],
            },
            ALICE,
            4,
        ));
        assert!(GroupState::fold(&ops).automod_words.is_empty());

        // The founder (MANAGE_CHANNELS via ALL_PERMS) can set it — words
        // are normalized to lowercase and deduplicated case-insensitively.
        ops.push(signed(
            GroupOpBody::SetAutoModWords {
                words: vec!["Spam".into(), "SCAM".into(), "scam".into()],
            },
            FOUNDER,
            5,
        ));
        let st = GroupState::fold(&ops);
        assert_eq!(st.automod_words.len(), 2);
        assert!(st.automod_words.contains("spam"));
        assert!(st.automod_words.contains("scam"));

        // A later SetAutoModWords wholesale REPLACES the list, it does not
        // merge with the previous one.
        ops.push(signed(
            GroupOpBody::SetAutoModWords {
                words: vec!["only-this".into()],
            },
            FOUNDER,
            6,
        ));
        let st2 = GroupState::fold(&ops);
        assert_eq!(st2.automod_words.len(), 1);
        assert!(st2.automod_words.contains("only-this"));

        // The empty list is a valid replacement: it clears the filter.
        ops.push(signed(
            GroupOpBody::SetAutoModWords { words: vec![] },
            FOUNDER,
            7,
        ));
        assert!(GroupState::fold(&ops).automod_words.is_empty());
    }

    /// Adversarial pass (server config, hostile bytes): oversized list
    /// rejected wholesale, oversized/spoofed word rejected wholesale (no
    /// partial application), order-independent fold.
    #[test]
    fn automod_set_words_rejects_oversized_list_and_spoofed_words_atomically() {
        // Oversized list (structural bound, defense in depth — the wire
        // decoder already caps this, but the fold must not rely on it
        // alone): entirely ignored, not truncated to the first 50.
        let mut oversized = base_ops();
        oversized.push(signed(
            GroupOpBody::SetAutoModWords {
                words: (0..(MAX_AUTOMOD_WORDS + 1))
                    .map(|i| format!("w{i}"))
                    .collect(),
            },
            FOUNDER,
            4,
        ));
        assert!(GroupState::fold(&oversized).automod_words.is_empty());

        // Exactly MAX_AUTOMOD_WORDS is accepted.
        let mut at_cap = base_ops();
        at_cap.push(signed(
            GroupOpBody::SetAutoModWords {
                words: (0..MAX_AUTOMOD_WORDS).map(|i| format!("w{i}")).collect(),
            },
            FOUNDER,
            4,
        ));
        assert_eq!(
            GroupState::fold(&at_cap).automod_words.len(),
            MAX_AUTOMOD_WORDS
        );

        // A single overlong word (33 characters) poisons the whole op: the
        // list stays exactly as it was before (atomic rejection, no
        // partial replacement with the other, valid words).
        let mut overlong = base_ops();
        overlong.push(signed(
            GroupOpBody::SetAutoModWords {
                words: vec!["ok".into()],
            },
            FOUNDER,
            4,
        ));
        overlong.push(signed(
            GroupOpBody::SetAutoModWords {
                words: vec!["fine".into(), "x".repeat(33)],
            },
            FOUNDER,
            5,
        ));
        let st_overlong = GroupState::fold(&overlong);
        assert_eq!(st_overlong.automod_words.len(), 1);
        assert!(st_overlong.automod_words.contains("ok"));

        // A spoofed word (zero-width space, anti-spoofing) also poisons
        // the whole op, same as a control character.
        let mut spoofed = base_ops();
        spoofed.push(signed(
            GroupOpBody::SetAutoModWords {
                words: vec!["ok".into()],
            },
            FOUNDER,
            4,
        ));
        spoofed.push(signed(
            GroupOpBody::SetAutoModWords {
                words: vec!["fine".into(), "spa\u{200B}m".into()],
            },
            FOUNDER,
            5,
        ));
        let st_spoofed = GroupState::fold(&spoofed);
        assert_eq!(st_spoofed.automod_words.len(), 1);
        assert!(st_spoofed.automod_words.contains("ok"));

        // An empty-string word (0 characters) is also rejected — the
        // 1-char floor of is_valid_display_label applies here too.
        let mut empty_word = base_ops();
        empty_word.push(signed(
            GroupOpBody::SetAutoModWords {
                words: vec!["ok".into()],
            },
            FOUNDER,
            4,
        ));
        empty_word.push(signed(
            GroupOpBody::SetAutoModWords {
                words: vec![String::new()],
            },
            FOUNDER,
            5,
        ));
        let st_empty = GroupState::fold(&empty_word);
        assert_eq!(st_empty.automod_words.len(), 1);
        assert!(st_empty.automod_words.contains("ok"));

        // Order-independence: shuffled ops converge to the same state
        // (mirrors `fold_is_order_independent`).
        let mut shuffled = at_cap.clone();
        shuffled.reverse();
        assert_eq!(GroupState::fold(&at_cap), GroupState::fold(&shuffled));
    }

    #[test]
    fn channel_slowmode_requires_manage_channels_and_known_channel() {
        let mut ops = base_ops();
        add_poll_channel(&mut ops, 3);

        // A plain member (no MANAGE_CHANNELS) cannot set it.
        ops.push(signed(
            GroupOpBody::SetChannelSlowmode {
                channel_id: POLL_CHAN,
                seconds: 10,
            },
            ALICE,
            4,
        ));
        assert_eq!(
            GroupState::fold(&ops)
                .channels
                .get(&POLL_CHAN)
                .unwrap()
                .slowmode_secs,
            0
        );

        // The founder (MANAGE_CHANNELS via ALL_PERMS) can set it.
        ops.push(signed(
            GroupOpBody::SetChannelSlowmode {
                channel_id: POLL_CHAN,
                seconds: 30,
            },
            FOUNDER,
            5,
        ));
        assert_eq!(
            GroupState::fold(&ops)
                .channels
                .get(&POLL_CHAN)
                .unwrap()
                .slowmode_secs,
            30
        );

        // An unknown channel is ignored rather than creating a dangling entry.
        let mut unknown = base_ops();
        unknown.push(signed(
            GroupOpBody::SetChannelSlowmode {
                channel_id: [0xEE; 16],
                seconds: 10,
            },
            FOUNDER,
            4,
        ));
        assert!(GroupState::fold(&unknown).channels.is_empty());

        // 0 turns it back off.
        ops.push(signed(
            GroupOpBody::SetChannelSlowmode {
                channel_id: POLL_CHAN,
                seconds: 0,
            },
            FOUNDER,
            6,
        ));
        assert_eq!(
            GroupState::fold(&ops)
                .channels
                .get(&POLL_CHAN)
                .unwrap()
                .slowmode_secs,
            0
        );
    }

    /// Adversarial pass + convergence: out-of-range `seconds` ignored
    /// (defense in depth — the wire decoder already rejects it, the fold
    /// must not rely on that alone), slow mode cleared for free when the
    /// channel is deleted (the whole `Channel` entry disappears), and the
    /// fold is order-independent.
    #[test]
    fn channel_slowmode_rejects_out_of_range_and_clears_on_channel_delete() {
        let mut oversized = base_ops();
        add_poll_channel(&mut oversized, 3);
        oversized.push(signed(
            GroupOpBody::SetChannelSlowmode {
                channel_id: POLL_CHAN,
                seconds: MAX_CHANNEL_SLOWMODE_SECS + 1,
            },
            FOUNDER,
            4,
        ));
        assert_eq!(
            GroupState::fold(&oversized)
                .channels
                .get(&POLL_CHAN)
                .unwrap()
                .slowmode_secs,
            0
        );

        // Exactly the ceiling is accepted.
        let mut at_cap = base_ops();
        add_poll_channel(&mut at_cap, 3);
        at_cap.push(signed(
            GroupOpBody::SetChannelSlowmode {
                channel_id: POLL_CHAN,
                seconds: MAX_CHANNEL_SLOWMODE_SECS,
            },
            FOUNDER,
            4,
        ));
        assert_eq!(
            GroupState::fold(&at_cap)
                .channels
                .get(&POLL_CHAN)
                .unwrap()
                .slowmode_secs,
            MAX_CHANNEL_SLOWMODE_SECS
        );

        // Deleting the channel drops the whole `Channel` entry, slow mode
        // included — mirrors how per-channel overrides are cleared on
        // `DelChannel`. Re-adding a channel under the SAME id afterwards
        // starts fresh at `slowmode_secs = 0`.
        let mut deleted = at_cap.clone();
        deleted.push(signed(
            GroupOpBody::DelChannel {
                channel_id: POLL_CHAN,
            },
            FOUNDER,
            5,
        ));
        assert!(!GroupState::fold(&deleted).channels.contains_key(&POLL_CHAN));
        add_poll_channel(&mut deleted, 6);
        assert_eq!(
            GroupState::fold(&deleted)
                .channels
                .get(&POLL_CHAN)
                .unwrap()
                .slowmode_secs,
            0
        );

        // Order-independence: shuffled ops converge to the same state.
        let mut shuffled = at_cap.clone();
        shuffled.reverse();
        assert_eq!(GroupState::fold(&at_cap), GroupState::fold(&shuffled));
    }
}
