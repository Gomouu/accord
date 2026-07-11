//! Messages de salon de groupe : composition et ingestion (SPEC §6.5).
//!
//! Un message de salon voyage dans `CoreMsg::GroupMsg`, chiffré par la clé de
//! groupe de l'epoch courant avec un AAD liant groupe, salon, message et
//! epoch. Le clair est encadré `kind(1) ‖ corps` (le filaire `GroupMsg` ne
//! porte pas de discriminant, contrairement à `DirectMsg`). L'authenticité de
//! l'auteur vient de la session E2E avec lui : l'ingestion attribue le
//! message à la clé de session émettrice, un relais deviendrait donc l'auteur
//! et ne peut rien usurper.

use accord_crypto::Identity;
use accord_proto::core_msg::{
    ChannelKind, CoreMsg, FileRef, MsgBody, MAX_POLL_OPTIONS, MAX_POLL_OPTION_BYTES,
    MAX_POLL_QUESTION_BYTES, MIN_POLL_OPTIONS,
};
use accord_proto::limits::MAX_TEXT_BYTES;

use crate::db::{Db, GroupMsgRecord};
use crate::error::CoreError;
use crate::messaging::validate_attachments;
use crate::search;

use accord_proto::core_msg::perms;

use super::state::GroupState;
use super::{crypt, group_state, new_id16};

/// Borne d'un emoji de réaction (grappe de graphèmes UTF-8, en octets).
const MAX_EMOJI_BYTES: usize = 64;

/// Vrai si `s` ne contient aucun caractère de contrôle hormis
/// tabulation/retour/saut de ligne — même politique que
/// [`super::state`]'s `is_valid_event_description` (D-048 : pas
/// d'anti-usurpation par caractères de format, contrairement aux pseudos —
/// un texte de sondage n'apparaît jamais dans une liste de membres).
fn poll_text_ok(s: &str) -> bool {
    !s.chars()
        .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
}

/// Issue de l'ingestion d'un message de groupe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupMsgEvent {
    /// Message stocké : à signaler à l'UI.
    Stored,
    /// Message existant édité par son auteur.
    Edited,
    /// Message existant supprimé par son auteur.
    Deleted,
    /// Réaction ajoutée ou retirée.
    Reacted,
    /// Indicateur de frappe éphémère d'un membre (jamais persisté).
    Typing,
    /// Déjà connu (rejeu ou double livraison) : rien à faire.
    Duplicate,
    /// Ignoré silencieusement (groupe/salon inconnu, émetteur sans droit,
    /// clé d'epoch absente, cible inéditable).
    Ignored,
}

/// Vérifie le contexte d'émission (salon connu, droit d'écriture effectif dans
/// ce salon, non en sourdine, salon d'annonces réservé aux gestionnaires) et
/// rend l'état matérialisé. `now_ms` = horloge murale locale, comparée à
/// l'échéance de sourdine (les sourdines expirées sont ignorées). Reflète
/// exactement [`GroupState::can_send_message`], utilisé à l'ingestion.
fn require_send(
    db: &Db,
    identity: &Identity,
    group_id: &[u8; 16],
    channel_id: &[u8; 16],
    now_ms: u64,
) -> Result<GroupState, CoreError> {
    let state = group_state(db, group_id)?;
    let me = identity.public_key();
    let Some(channel) = state.channels.get(channel_id) else {
        return Err(CoreError::Invalid("salon inconnu"));
    };
    // Writing requires both VIEW and SEND once channel overrides are folded
    // in: a channel hidden from a role cannot be written to either.
    let needed = perms::VIEW | perms::SEND;
    let eff = state.permissions_in(&me, channel_id);
    if eff & needed != needed {
        return Err(CoreError::OpRejected("droit d'écriture refusé"));
    }
    // A timed-out member stays in the group but cannot send while active.
    if state.is_timed_out(&me, now_ms) {
        return Err(CoreError::OpRejected("membre en sourdine (timeout)"));
    }
    // Announcement channels are read-only for anyone without MANAGE_CHANNELS.
    if channel.kind == ChannelKind::Announcement && eff & perms::MANAGE_CHANNELS == 0 {
        return Err(CoreError::OpRejected("salon d'annonces en lecture seule"));
    }
    Ok(state)
}

/// Vérifie le mode lent d'un salon pour un NOUVEAU message (`Text`/
/// `Sticker`/`Poll` — jamais une édition, une réaction, une suppression ou
/// un indicateur de frappe, qui ne consomment pas de « tour » de mode lent)
/// et, si autorisé, enregistre `at_ms` comme dernier envoi accepté pour
/// (salon, auteur). Rend `false` sans rien enregistrer si le cooldown n'est
/// pas écoulé — l'appelant décide de la traduction en erreur (composition)
/// ou en silence (`GroupMsgEvent::Ignored`, ingestion).
///
/// `at_ms` doit être une horloge NON falsifiable par l'auteur du message :
/// l'horloge locale de l'ÉMETTEUR en composition (`now_ms` de l'appelant),
/// l'horloge locale du RÉCEPTEUR en ingestion (`local_now_ms`) — jamais
/// `sent_ms` auto-déclaré (hors AAD, non authentifié). Même défense que le
/// contournement de sourdine anti-forge plus bas dans
/// [`ingest_group_message`] : un pair hostile qui mentirait sur `sent_ms`
/// ou ignorerait son propre cooldown local ne gagne rien, puisque chaque
/// pair HONNÊTE applique indépendamment la même règle en fonction de SA
/// PROPRE horloge de réception — c'est ce qui rend cette limite robuste
/// contre un client modifié, alors même que les messages de salon ne font
/// pas partie de l'op-log signé replié dans `GroupState` (voir
/// [`GroupState::slowmode_exempt`] / `GroupOpBody::SetChannelSlowmode`).
///
/// Les porteurs de `MANAGE_CHANNELS`/`MANAGE_MESSAGES` sont exemptés
/// (comportement Discord). Un salon inconnu ou sans mode lent actif
/// n'entrave jamais rien (`true`, aucun enregistrement).
fn check_slowmode(
    db: &Db,
    state: &GroupState,
    group_id: &[u8; 16],
    channel_id: &[u8; 16],
    author: &[u8; 32],
    at_ms: u64,
) -> Result<bool, CoreError> {
    let Some(channel) = state.channels.get(channel_id) else {
        return Ok(true);
    };
    if channel.slowmode_secs == 0 {
        return Ok(true);
    }
    if !state.slowmode_exempt(author, channel_id) {
        if let Some(last) = db.slowmode_last_ms(group_id, channel_id, author)? {
            let cooldown_ms = u64::from(channel.slowmode_secs) * 1000;
            if at_ms.saturating_sub(last) < cooldown_ms {
                return Ok(false);
            }
        }
    }
    db.bump_slowmode_last_ms(group_id, channel_id, author, at_ms)?;
    Ok(true)
}

/// Chiffre un corps par la clé de groupe courante et rend le `CoreMsg` à
/// diffuser (l'horloge de Lamport locale est avancée). `msg_id` frais généré
/// localement ([`new_id16`]).
fn seal_body(
    db: &Db,
    group_id: &[u8; 16],
    channel_id: &[u8; 16],
    body: &MsgBody,
    now_ms: u64,
) -> Result<CoreMsg, CoreError> {
    seal_body_with_id(db, group_id, channel_id, new_id16(), body, now_ms)
}

/// Comme [`seal_body`], mais avec un `msg_id` fourni par l'appelant plutôt
/// que généré ici. Utilisé quand le `msg_id` doit être connu *avant* le
/// scellement — par exemple pour qu'un sondage (D-048 fix HIGH-2) puisse
/// enregistrer son `GroupOpBody::PollCreate { msg_id, .. }` en amont de la
/// composition du message, afin de lier `poll_id` au message canonique qui
/// l'accompagne.
fn seal_body_with_id(
    db: &Db,
    group_id: &[u8; 16],
    channel_id: &[u8; 16],
    msg_id: [u8; 16],
    body: &MsgBody,
    now_ms: u64,
) -> Result<CoreMsg, CoreError> {
    let stored = db
        .latest_group_key(group_id)?
        .ok_or(CoreError::Invalid("clé de groupe absente"))?;
    let lamport = db.bump_lamport(0)?;
    let encoded = body.encode_body();
    let mut plain = Vec::with_capacity(1 + encoded.len());
    plain.push(body.kind());
    plain.extend_from_slice(&encoded);
    let body_enc = crypt::encrypt_group_msg(
        &stored.key,
        group_id,
        channel_id,
        &msg_id,
        stored.key_epoch,
        &plain,
    )?;
    Ok(CoreMsg::GroupMsg {
        group_id: *group_id,
        channel_id: *channel_id,
        msg_id,
        lamport,
        sent_ms: now_ms,
        key_epoch: stored.key_epoch,
        body_enc,
    })
}

/// Compose un message texte pour un salon de groupe : persiste le clair et
/// ses pièces jointes, l'indexe pour la recherche et rend le `CoreMsg`
/// chiffré à diffuser aux membres.
#[allow(clippy::too_many_arguments)]
pub fn compose_group_message(
    db: &Db,
    identity: &Identity,
    search_key: &[u8; 32],
    group_id: &[u8; 16],
    channel_id: &[u8; 16],
    text: &str,
    reply_to: Option<[u8; 16]>,
    attachments: Vec<FileRef>,
    now_ms: u64,
) -> Result<CoreMsg, CoreError> {
    if text.trim().is_empty() && attachments.is_empty() {
        return Err(CoreError::Invalid("message vide"));
    }
    if text.len() > MAX_TEXT_BYTES {
        return Err(CoreError::Invalid("texte trop long"));
    }
    validate_attachments(&attachments)?;
    let state = require_send(db, identity, group_id, channel_id, now_ms)?;
    if !check_slowmode(
        db,
        &state,
        group_id,
        channel_id,
        &identity.public_key(),
        now_ms,
    )? {
        return Err(CoreError::OpRejected(
            "mode lent actif : patiente encore un instant",
        ));
    }

    let body = MsgBody::Text {
        text: text.to_string(),
        reply_to,
        attachments,
    };
    let msg = seal_body(db, group_id, channel_id, &body, now_ms)?;
    let CoreMsg::GroupMsg {
        msg_id, lamport, ..
    } = &msg
    else {
        return Err(CoreError::Invalid("enveloppe de groupe inattendue"));
    };

    db.insert_group_msg(&GroupMsgRecord {
        msg_id: *msg_id,
        group_id: *group_id,
        channel_id: *channel_id,
        author: identity.public_key(),
        lamport: *lamport,
        sent_ms: now_ms,
        kind: body.kind(),
        body: body.encode_body(),
        deleted: false,
        edited: None,
    })?;
    if let MsgBody::Text { attachments, .. } = &body {
        db.put_msg_attachments(msg_id, attachments)?;
    }
    search::index_message(db, search_key, msg_id, text)?;
    Ok(msg)
}

/// Compose l'édition d'un de nos messages de groupe (auteur seul, refusé
/// sinon). L'édition est appliquée localement puis chiffrée à diffuser.
#[allow(clippy::too_many_arguments)]
pub fn compose_group_edit(
    db: &Db,
    identity: &Identity,
    search_key: &[u8; 32],
    group_id: &[u8; 16],
    channel_id: &[u8; 16],
    target: &[u8; 16],
    new_text: &str,
    now_ms: u64,
) -> Result<CoreMsg, CoreError> {
    if new_text.trim().is_empty() || new_text.len() > MAX_TEXT_BYTES {
        return Err(CoreError::Invalid("texte d'édition vide ou trop long"));
    }
    require_send(db, identity, group_id, channel_id, now_ms)?;
    if !db.edit_group_msg(target, &identity.public_key(), new_text.as_bytes())? {
        return Err(CoreError::OpRejected("message inéditable"));
    }
    search::reindex_message(db, search_key, target, new_text)?;
    seal_body(
        db,
        group_id,
        channel_id,
        &MsgBody::Edit {
            target: *target,
            new_text: new_text.to_string(),
        },
        now_ms,
    )
}

/// Compose la suppression d'un de nos messages de groupe (tombstone local
/// immédiat ; la suppression de modération passe par l'op-log signée).
pub fn compose_group_delete(
    db: &Db,
    identity: &Identity,
    group_id: &[u8; 16],
    channel_id: &[u8; 16],
    target: &[u8; 16],
    now_ms: u64,
) -> Result<CoreMsg, CoreError> {
    require_send(db, identity, group_id, channel_id, now_ms)?;
    if !db.delete_group_msg(target, Some(&identity.public_key()))? {
        return Err(CoreError::OpRejected("message insupprimable"));
    }
    db.unindex_msg(target)?;
    seal_body(
        db,
        group_id,
        channel_id,
        &MsgBody::Delete { target: *target },
        now_ms,
    )
}

/// Compose l'envoi d'un sticker de serveur pour un salon (D-047). Le nom
/// doit référencer un sticker enregistré dans l'état courant du groupe : la
/// racine Merkle est dérivée localement de cet état, **jamais fournie par
/// l'appelant** — un client ne peut donc pas forger un couple
/// `(nom, racine)` sans rapport avec un sticker réellement enregistré.
/// Persisté comme tout message de salon (voir [`compose_group_message`]
/// pour le cas `Text`), sans indexation recherche (pas de texte).
pub fn compose_group_sticker(
    db: &Db,
    identity: &Identity,
    group_id: &[u8; 16],
    channel_id: &[u8; 16],
    name: &str,
    now_ms: u64,
) -> Result<CoreMsg, CoreError> {
    let state = require_send(db, identity, group_id, channel_id, now_ms)?;
    if !check_slowmode(
        db,
        &state,
        group_id,
        channel_id,
        &identity.public_key(),
        now_ms,
    )? {
        return Err(CoreError::OpRejected(
            "mode lent actif : patiente encore un instant",
        ));
    }
    let merkle_root = *state
        .stickers
        .get(name)
        .ok_or(CoreError::Invalid("sticker inconnu"))?;
    let body = MsgBody::Sticker {
        name: name.to_string(),
        merkle_root,
    };
    let msg = seal_body(db, group_id, channel_id, &body, now_ms)?;
    let CoreMsg::GroupMsg {
        msg_id, lamport, ..
    } = &msg
    else {
        return Err(CoreError::Invalid("enveloppe de groupe inattendue"));
    };
    db.insert_group_msg(&GroupMsgRecord {
        msg_id: *msg_id,
        group_id: *group_id,
        channel_id: *channel_id,
        author: identity.public_key(),
        lamport: *lamport,
        sent_ms: now_ms,
        kind: body.kind(),
        body: body.encode_body(),
        deleted: false,
        edited: None,
    })?;
    Ok(msg)
}

/// Compose un sondage pour un salon (D-048) : question + options 1-10,
/// mêmes bornes que le filaire ([`MAX_POLL_QUESTION_BYTES`],
/// [`MIN_POLL_OPTIONS`]-[`MAX_POLL_OPTIONS`], [`MAX_POLL_OPTION_BYTES`]),
/// revalidées ici puisque rien d'autre ne les revérifie côté composeur (à la
/// différence du décodage, jamais invoqué en local). Persisté comme tout
/// message de salon (voir [`compose_group_message`]).
///
/// `poll_id`/`msg_id` sont fournis par l'appelant plutôt que générés ici
/// (D-048 fix MEDIUM) : le nœud doit les connaître *avant* d'appeler cette
/// fonction pour pouvoir enregistrer `GroupOpBody::PollCreate { poll_id,
/// channel_id, msg_id }` dans l'op-log **avant** de composer/diffuser le
/// message — si l'enregistrement échoue (p. ex. plafond `MAX_POLLS`
/// atteint), rien n'est composé ni envoyé, au lieu d'un message posté dont
/// le dépouillement ne serait jamais connu de l'op-log.
#[allow(clippy::too_many_arguments)]
pub fn compose_group_poll(
    db: &Db,
    identity: &Identity,
    group_id: &[u8; 16],
    channel_id: &[u8; 16],
    poll_id: [u8; 16],
    msg_id: [u8; 16],
    question: &str,
    options: Vec<String>,
    now_ms: u64,
) -> Result<CoreMsg, CoreError> {
    let question = question.trim();
    if question.is_empty() || question.len() > MAX_POLL_QUESTION_BYTES || !poll_text_ok(question) {
        return Err(CoreError::Invalid("question de sondage invalide"));
    }
    if !(MIN_POLL_OPTIONS..=MAX_POLL_OPTIONS).contains(&options.len()) {
        return Err(CoreError::Invalid(
            "nombre d'options de sondage invalide (2-10)",
        ));
    }
    let options: Vec<String> = options.iter().map(|o| o.trim().to_string()).collect();
    if options
        .iter()
        .any(|o| o.is_empty() || o.len() > MAX_POLL_OPTION_BYTES || !poll_text_ok(o))
    {
        return Err(CoreError::Invalid("option de sondage invalide"));
    }
    // Beyond the write-permission gate, `state` is also needed for the slow
    // mode check below — the poll's tally itself is registered separately
    // by the caller via `GroupOpBody::PollCreate` (already authored by now).
    let state = require_send(db, identity, group_id, channel_id, now_ms)?;
    if !check_slowmode(
        db,
        &state,
        group_id,
        channel_id,
        &identity.public_key(),
        now_ms,
    )? {
        return Err(CoreError::OpRejected(
            "mode lent actif : patiente encore un instant",
        ));
    }
    let body = MsgBody::Poll {
        poll_id,
        question: question.to_string(),
        options,
    };
    let msg = seal_body_with_id(db, group_id, channel_id, msg_id, &body, now_ms)?;
    let CoreMsg::GroupMsg { lamport, .. } = &msg else {
        return Err(CoreError::Invalid("enveloppe de groupe inattendue"));
    };
    db.insert_group_msg(&GroupMsgRecord {
        msg_id,
        group_id: *group_id,
        channel_id: *channel_id,
        author: identity.public_key(),
        lamport: *lamport,
        sent_ms: now_ms,
        kind: body.kind(),
        body: body.encode_body(),
        deleted: false,
        edited: None,
    })?;
    Ok(msg)
}

/// Compose l'ajout ou le retrait d'une réaction (appliquée localement).
#[allow(clippy::too_many_arguments)]
pub fn compose_group_reaction(
    db: &Db,
    identity: &Identity,
    group_id: &[u8; 16],
    channel_id: &[u8; 16],
    target: &[u8; 16],
    emoji: &str,
    add: bool,
    now_ms: u64,
) -> Result<CoreMsg, CoreError> {
    if emoji.is_empty() || emoji.len() > MAX_EMOJI_BYTES {
        return Err(CoreError::Invalid("emoji invalide"));
    }
    require_send(db, identity, group_id, channel_id, now_ms)?;
    db.set_reaction(target, &identity.public_key(), emoji, add)?;
    seal_body(
        db,
        group_id,
        channel_id,
        &MsgBody::Reaction {
            target: *target,
            emoji: emoji.to_string(),
            add,
        },
        now_ms,
    )
}

/// Compose un indicateur de frappe éphémère pour un salon : chiffré comme un
/// message de groupe (corps `MsgBody::Typing`) mais jamais persisté. Rend le
/// `CoreMsg` à diffuser aux membres joignables ; un salon inconnu ou un droit
/// d'écriture refusé produit une erreur (rien n'est émis).
pub fn compose_group_typing(
    db: &Db,
    identity: &Identity,
    group_id: &[u8; 16],
    channel_id: &[u8; 16],
    now_ms: u64,
) -> Result<CoreMsg, CoreError> {
    require_send(db, identity, group_id, channel_id, now_ms)?;
    seal_body(db, group_id, channel_id, &MsgBody::Typing, now_ms)
}

/// Ingère un message de groupe reçu d'un pair authentifié (`sender` = clé de
/// la session émettrice, créditée comme auteur).
#[allow(clippy::too_many_arguments)]
pub fn ingest_group_message(
    db: &Db,
    search_key: &[u8; 32],
    sender: &[u8; 32],
    group_id: &[u8; 16],
    channel_id: &[u8; 16],
    msg_id: &[u8; 16],
    lamport: u64,
    sent_ms: u64,
    local_now_ms: u64,
    key_epoch: u32,
    body_enc: &[u8],
) -> Result<GroupMsgEvent, CoreError> {
    let state = group_state(db, group_id)?;
    // Groupe inconnu (aucune op) ou contexte invalide : silence, pas d'oracle.
    // `can_send_message` reflète exactement `require_send` côté émission : il
    // exige VIEW ET SEND effectifs (overrides de salon compris — un membre à
    // qui le salon est masqué ne peut pas y injecter via un client modifié qui
    // aurait gardé le bit SEND), rejette un membre en sourdine (échéance
    // comparée à `sent_ms`, l'horloge murale du message) et réserve les salons
    // d'annonces aux porteurs de MANAGE_CHANNELS.
    if state.founder.is_none() || !state.can_send_message(sender, channel_id, sent_ms) {
        return Ok(GroupMsgEvent::Ignored);
    }
    // Anti-forge : `sent_ms` est auto-déclaré par l'émetteur et non authentifié
    // (hors AAD). Un membre en sourdine pourrait post-dater `sent_ms` au-delà de
    // l'échéance pour contourner le mute. On exige donc aussi qu'il ne soit pas
    // en sourdine selon l'horloge locale du RÉCEPTEUR (non contrôlable par
    // l'attaquant). Un message honnête composé après l'échéance passe les deux.
    if state.is_timed_out(sender, local_now_ms) {
        return Ok(GroupMsgEvent::Ignored);
    }
    let Some(key) = db.group_key(group_id, key_epoch)? else {
        return Ok(GroupMsgEvent::Ignored);
    };
    let plain = crypt::decrypt_group_msg(&key, group_id, channel_id, msg_id, key_epoch, body_enc)?;
    let (kind, encoded) = match plain.split_first() {
        Some(split) => split,
        None => return Err(CoreError::Invalid("clair de message vide")),
    };
    // Validation stricte du corps avant persistance.
    let body = MsgBody::decode_body(*kind, encoded)?;
    db.bump_lamport(lamport)?;

    match body {
        MsgBody::Text {
            ref text,
            ref attachments,
            ..
        } => {
            // Duplicate/replay check first (idempotent, unrelated to slow
            // mode): a re-delivery of an already-accepted message must never
            // be rejected as "too fast" nor bump the tracked cooldown again.
            if db.group_msg(msg_id)?.is_some() {
                return Ok(GroupMsgEvent::Duplicate);
            }
            if !check_slowmode(db, &state, group_id, channel_id, sender, local_now_ms)? {
                return Ok(GroupMsgEvent::Ignored);
            }
            let inserted = db.insert_group_msg(&GroupMsgRecord {
                msg_id: *msg_id,
                group_id: *group_id,
                channel_id: *channel_id,
                author: *sender,
                lamport,
                sent_ms,
                kind: *kind,
                body: encoded.to_vec(),
                deleted: false,
                edited: None,
            })?;
            if !inserted {
                return Ok(GroupMsgEvent::Duplicate);
            }
            db.put_msg_attachments(msg_id, attachments)?;
            search::index_message(db, search_key, msg_id, text)?;
            Ok(GroupMsgEvent::Stored)
        }
        MsgBody::Edit { target, new_text } => {
            if new_text.trim().is_empty() || new_text.len() > MAX_TEXT_BYTES {
                return Err(CoreError::Invalid("texte d'édition vide ou trop long"));
            }
            // L'auteur seul édite : la contrainte SQL (msg_id + author)
            // rejette toute usurpation par un autre membre.
            if db.edit_group_msg(&target, sender, new_text.as_bytes())? {
                search::reindex_message(db, search_key, &target, &new_text)?;
                Ok(GroupMsgEvent::Edited)
            } else {
                Ok(GroupMsgEvent::Ignored)
            }
        }
        MsgBody::Delete { target } => {
            // L'auteur seul supprime par message ; la modération d'un message
            // d'autrui passe par l'op-log signée (`DeleteMsg`).
            if db.delete_group_msg(&target, Some(sender))? {
                db.unindex_msg(&target)?;
                Ok(GroupMsgEvent::Deleted)
            } else {
                Ok(GroupMsgEvent::Ignored)
            }
        }
        MsgBody::Reaction { target, emoji, add } => {
            db.set_reaction(&target, sender, &emoji, add)?;
            Ok(GroupMsgEvent::Reacted)
        }
        // Sticker (D-047) : pas de validation croisée contre le registre de
        // stickers à l'ingestion (comme les émojis de réaction, un nom/racine
        // sans rapport avec un sticker enregistré est affiché tel quel côté
        // UI plutôt que rejeté — dégradation gracieuse, pas d'oracle réseau).
        MsgBody::Sticker { .. } => {
            if db.group_msg(msg_id)?.is_some() {
                return Ok(GroupMsgEvent::Duplicate);
            }
            if !check_slowmode(db, &state, group_id, channel_id, sender, local_now_ms)? {
                return Ok(GroupMsgEvent::Ignored);
            }
            let inserted = db.insert_group_msg(&GroupMsgRecord {
                msg_id: *msg_id,
                group_id: *group_id,
                channel_id: *channel_id,
                author: *sender,
                lamport,
                sent_ms,
                kind: *kind,
                body: encoded.to_vec(),
                deleted: false,
                edited: None,
            })?;
            if !inserted {
                return Ok(GroupMsgEvent::Duplicate);
            }
            Ok(GroupMsgEvent::Stored)
        }
        // Sondage (D-048) : bornes d'octets/vide/UTF-8 déjà vérifiées au
        // décodage ; on revalide ici l'absence de caractères de contrôle
        // (même politique que l'édition de texte ci-dessus) avant
        // persistance. Le dépouillement (qui a le droit de voter/clore) vit
        // séparément dans `GroupState::polls`, alimenté par l'op-log
        // (`PollCreate`/`PollVote`/`PollClose`) — jamais recalculé depuis ce
        // message, exactement comme un sticker n'est pas revalidé contre le
        // registre de stickers à l'ingestion (dégradation gracieuse, pas
        // d'oracle réseau).
        MsgBody::Poll {
            poll_id,
            ref question,
            ref options,
        } => {
            if !poll_text_ok(question) || !options.iter().all(|o| poll_text_ok(o)) {
                return Err(CoreError::Invalid("texte de sondage invalide"));
            }
            if db.group_msg(msg_id)?.is_some() {
                return Ok(GroupMsgEvent::Duplicate);
            }
            // Anti-usurpation (D-048 fix HIGH-2) : `GroupOpBody::PollCreate`
            // lie `poll_id` à un unique (auteur, `msg_id`) canonique. Si
            // l'op-log local connaît déjà ce `poll_id` (un `PollCreate` a
            // été replié), seul le message qui correspond exactement à cet
            // auteur ET ce `msg_id` est stocké — un pair qui rejoue ce
            // `poll_id` avec un autre auteur (vol d'un sondage existant) ou
            // un autre `msg_id` (question/options forgées sous le même
            // identifiant) ne crée ni un second sondage visible, ni ne
            // « rehéberge » le dépouillement existant sur son propre
            // contenu — premier message canonique gagnant. Si l'op-log
            // local ne connaît pas encore ce `poll_id` (le `PollCreate`
            // correspondant n'est pas encore arrivé — livraison
            // message/op non ordonnée entre elles), aucune contrainte
            // n'est appliquée ici : dégradation gracieuse identique à un
            // sticker non enregistré, l'établissement canonique se fait
            // au repli de l'op-log.
            if let Some(existing) = state.polls.get(&poll_id) {
                if existing.author != *sender || existing.msg_id != *msg_id {
                    return Ok(GroupMsgEvent::Ignored);
                }
            }
            if !check_slowmode(db, &state, group_id, channel_id, sender, local_now_ms)? {
                return Ok(GroupMsgEvent::Ignored);
            }
            let inserted = db.insert_group_msg(&GroupMsgRecord {
                msg_id: *msg_id,
                group_id: *group_id,
                channel_id: *channel_id,
                author: *sender,
                lamport,
                sent_ms,
                kind: *kind,
                body: encoded.to_vec(),
                deleted: false,
                edited: None,
            })?;
            if !inserted {
                return Ok(GroupMsgEvent::Duplicate);
            }
            Ok(GroupMsgEvent::Stored)
        }
        // Saisie : éphémère, signalée à l'UI mais jamais persistée.
        MsgBody::Typing => Ok(GroupMsgEvent::Typing),
        // Accusés de lecture : sans objet dans un salon (marque locale).
        MsgBody::ReadReceipt { .. } => Ok(GroupMsgEvent::Ignored),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::group::{author_op, create_group, ingest_op};
    use accord_proto::core_msg::{ChannelKind, GroupOpBody};

    fn identity() -> Identity {
        Identity::generate_with_pow_bits(1)
    }

    fn open_db() -> Db {
        Db::open_in_memory(&[7u8; 32]).expect("base mémoire")
    }

    /// Monte un groupe avec un salon sur la base de `founder`, retourne
    /// `(group_id, channel_id)` et rejoue les ops sur les bases `others`.
    fn build_group(
        founder: &Identity,
        db: &Db,
        members: &[(&Identity, &Db)],
    ) -> ([u8; 16], [u8; 16]) {
        let created = create_group(db, founder, "Test", 1_000).expect("création");
        let channel_id = new_id16();
        let add = author_op(
            db,
            founder,
            &created.group_id,
            &GroupOpBody::AddChannel {
                channel_id,
                name: "général".into(),
                category: None,
                kind: ChannelKind::Text,
                position: 0,
            },
            1_001,
        )
        .expect("salon");
        let mut ops = vec![created.op.clone(), add];
        for (idx, (member, _)) in members.iter().enumerate() {
            let join = author_op(
                db,
                founder,
                &created.group_id,
                &GroupOpBody::AddMember {
                    member: member.public_key(),
                    invite_id: None,
                },
                1_002 + idx as u64,
            )
            .expect("admission");
            ops.push(join);
        }
        // Rejoue l'op-log complet + copie la clé de groupe chez chaque membre.
        let key = db
            .latest_group_key(&created.group_id)
            .expect("lecture clé")
            .expect("clé présente");
        for (_, other) in members {
            for op in &ops {
                ingest_op(other, op).expect("rejeu op");
            }
            other
                .put_group_key(&created.group_id, key.key_epoch, &key.key)
                .expect("clé copiée");
        }
        (created.group_id, channel_id)
    }

    /// D-048 fix HIGH-2 : un `Poll` forgé qui rejoue un `poll_id` déjà
    /// enregistré (par un `PollCreate` connu localement) sous un AUTRE
    /// auteur ou un AUTRE `msg_id` est ignoré à l'ingestion — il ne crée
    /// jamais une seconde carte de sondage visible, et ne « rehéberge »
    /// jamais le dépouillement existant sur son propre contenu (question/
    /// options forgées).
    #[test]
    fn poll_message_reusing_existing_poll_id_from_different_author_is_ignored() {
        let alice = identity();
        let bob = identity();
        let db_a = open_db();
        let db_b = open_db();
        let (gid, chan) = build_group(&alice, &db_a, &[(&bob, &db_b)]);

        // Alice crée un sondage réel : l'op PollCreate d'abord (ordre
        // op-first, D-048 fix MEDIUM), puis le message qui l'accompagne,
        // tous deux liés au même (poll_id, msg_id).
        let poll_id = new_id16();
        let msg_id = new_id16();
        let create_op = author_op(
            &db_a,
            &alice,
            &gid,
            &GroupOpBody::PollCreate {
                poll_id,
                channel_id: chan,
                msg_id,
            },
            2_000,
        )
        .expect("PollCreate");
        ingest_op(&db_b, &create_op).expect("réplication PollCreate");

        let real_msg = compose_group_poll(
            &db_a,
            &alice,
            &gid,
            &chan,
            poll_id,
            msg_id,
            "Vrai sondage ?",
            vec!["Oui".into(), "Non".into()],
            2_001,
        )
        .expect("composition sondage réel");
        let CoreMsg::GroupMsg {
            lamport,
            sent_ms,
            key_epoch,
            body_enc,
            ..
        } = &real_msg
        else {
            panic!("GroupMsg attendu");
        };
        assert_eq!(
            ingest_group_message(
                &db_b,
                &[9; 32],
                &alice.public_key(),
                &gid,
                &chan,
                &msg_id,
                *lamport,
                *sent_ms,
                *sent_ms,
                *key_epoch,
                body_enc,
            )
            .expect("ingestion sondage réel"),
            GroupMsgEvent::Stored
        );

        // Bob forge un message concurrent qui rejoue le `poll_id` d'Alice,
        // sous un `msg_id` frais et une question/options différentes — un
        // ingest naïf qui ne vérifierait que les bornes filaires stockerait
        // ceci comme une seconde carte de sondage pour le MÊME poll_id,
        // trompant les votants sur ce pour quoi ils votent.
        let forged_msg_id = new_id16();
        let forged_body = MsgBody::Poll {
            poll_id,
            question: "FAUX sondage (piraté) ?".into(),
            options: vec!["Piège A".into(), "Piège B".into()],
        };
        let forged = seal_body_with_id(&db_b, &gid, &chan, forged_msg_id, &forged_body, 2_002)
            .expect("scellement forgé");
        let CoreMsg::GroupMsg {
            lamport: f_lamport,
            sent_ms: f_sent_ms,
            key_epoch: f_key_epoch,
            body_enc: f_body_enc,
            ..
        } = &forged
        else {
            panic!("GroupMsg attendu");
        };
        let event = ingest_group_message(
            &db_a,
            &[10; 32],
            &bob.public_key(),
            &gid,
            &chan,
            &forged_msg_id,
            *f_lamport,
            *f_sent_ms,
            *f_sent_ms,
            *f_key_epoch,
            f_body_enc,
        )
        .expect("ingestion sondage forgé");
        assert_eq!(
            event,
            GroupMsgEvent::Ignored,
            "un message d'un autre auteur pour un poll_id déjà connu doit être ignoré"
        );

        // Aucun second message n'atterrit chez Alice, et le dépouillement
        // reste lié à Alice — jamais « rehébergé » vers Bob.
        let history = db_a
            .group_history(&gid, &chan, u64::MAX, 10)
            .expect("historique");
        assert_eq!(history.len(), 1, "pas de second message stocké");
        assert_eq!(history[0].msg_id, msg_id);
        let state = group_state(&db_a, &gid).expect("état");
        let poll = state.polls.get(&poll_id).expect("sondage connu");
        assert_eq!(poll.author, alice.public_key());
        assert_eq!(poll.msg_id, msg_id);

        // Défense en profondeur : même le VRAI auteur (Alice) ne peut pas
        // faire glisser le sondage vers un second message sous le même
        // poll_id — seul le `msg_id` canonique enregistré par PollCreate
        // est accepté.
        let alice_second_msg_id = new_id16();
        let alice_second_body = MsgBody::Poll {
            poll_id,
            question: "Deuxième message, même poll_id ?".into(),
            options: vec!["A".into(), "B".into()],
        };
        let alice_second = seal_body_with_id(
            &db_a,
            &gid,
            &chan,
            alice_second_msg_id,
            &alice_second_body,
            2_003,
        )
        .expect("scellement second message");
        let CoreMsg::GroupMsg {
            lamport: s_lamport,
            sent_ms: s_sent_ms,
            key_epoch: s_key_epoch,
            body_enc: s_body_enc,
            ..
        } = &alice_second
        else {
            panic!("GroupMsg attendu");
        };
        let event2 = ingest_group_message(
            &db_b,
            &[11; 32],
            &alice.public_key(),
            &gid,
            &chan,
            &alice_second_msg_id,
            *s_lamport,
            *s_sent_ms,
            *s_sent_ms,
            *s_key_epoch,
            s_body_enc,
        )
        .expect("ingestion second message");
        assert_eq!(
            event2,
            GroupMsgEvent::Ignored,
            "même le vrai auteur ne peut pas rejouer poll_id sous un second msg_id"
        );
    }

    #[test]
    fn compose_then_ingest_roundtrips() {
        let alice = identity();
        let bob = identity();
        let db_a = open_db();
        let db_b = open_db();
        let (gid, chan) = build_group(&alice, &db_a, &[(&bob, &db_b)]);

        let msg = compose_group_message(
            &db_a,
            &alice,
            &[1; 32],
            &gid,
            &chan,
            "salut",
            None,
            vec![],
            2_000,
        )
        .expect("composition");
        let CoreMsg::GroupMsg {
            group_id,
            channel_id,
            msg_id,
            lamport,
            sent_ms,
            key_epoch,
            body_enc,
        } = msg
        else {
            panic!("GroupMsg attendu");
        };
        let event = ingest_group_message(
            &db_b,
            &[2; 32],
            &alice.public_key(),
            &group_id,
            &channel_id,
            &msg_id,
            lamport,
            sent_ms,
            sent_ms,
            key_epoch,
            &body_enc,
        )
        .expect("ingestion");
        assert_eq!(event, GroupMsgEvent::Stored);

        let history = db_b
            .group_history(&gid, &chan, u64::MAX, 10)
            .expect("historique");
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].author, alice.public_key());
        let body = MsgBody::decode_body(history[0].kind, &history[0].body).expect("corps");
        assert!(matches!(body, MsgBody::Text { text, .. } if text == "salut"));

        // Redélivrance : dupliqué, pas de double stockage.
        let again = ingest_group_message(
            &db_b,
            &[2; 32],
            &alice.public_key(),
            &group_id,
            &channel_id,
            &msg_id,
            lamport,
            sent_ms,
            sent_ms,
            key_epoch,
            &body_enc,
        )
        .expect("réingestion");
        assert_eq!(again, GroupMsgEvent::Duplicate);
    }

    #[test]
    fn non_member_sender_is_ignored() {
        let alice = identity();
        let bob = identity();
        let mallory = identity();
        let db_a = open_db();
        let db_b = open_db();
        let (gid, chan) = build_group(&alice, &db_a, &[(&bob, &db_b)]);

        let msg = compose_group_message(
            &db_a,
            &alice,
            &[1; 32],
            &gid,
            &chan,
            "psst",
            None,
            vec![],
            2_000,
        )
        .expect("composition");
        let CoreMsg::GroupMsg {
            msg_id,
            lamport,
            sent_ms,
            key_epoch,
            body_enc,
            ..
        } = msg
        else {
            panic!("GroupMsg attendu");
        };
        // Même chiffré valide, mais session émettrice non membre : silence.
        let event = ingest_group_message(
            &db_b,
            &[2; 32],
            &mallory.public_key(),
            &gid,
            &chan,
            &msg_id,
            lamport,
            sent_ms,
            sent_ms,
            key_epoch,
            &body_enc,
        )
        .expect("ingestion");
        assert_eq!(event, GroupMsgEvent::Ignored);
        assert!(db_b
            .group_history(&gid, &chan, u64::MAX, 10)
            .expect("historique")
            .is_empty());
    }

    #[test]
    #[allow(clippy::vec_init_then_push)]
    fn channel_override_denying_send_or_view_blocks_compose() {
        let alice = identity();
        let bob = identity();
        let db_a = open_db();
        let db_b = open_db();
        let (gid, chan) = build_group(&alice, &db_a, &[(&bob, &db_b)]);

        // A "Muet" role denied SEND in the channel, assigned to Bob.
        let mut extra = Vec::new();
        extra.push(
            author_op(
                &db_a,
                &alice,
                &gid,
                &GroupOpBody::AddRole {
                    role_id: [3; 16],
                    name: "Muet".into(),
                    color: 0,
                    position: 1,
                    permissions: 0,
                },
                2_000,
            )
            .unwrap(),
        );
        extra.push(
            author_op(
                &db_a,
                &alice,
                &gid,
                &GroupOpBody::AssignRole {
                    member: bob.public_key(),
                    role_id: [3; 16],
                },
                2_001,
            )
            .unwrap(),
        );
        extra.push(
            author_op(
                &db_a,
                &alice,
                &gid,
                &GroupOpBody::SetChannelPerms {
                    channel_id: chan,
                    role_id: [3; 16],
                    allow: 0,
                    deny: perms::SEND,
                },
                2_002,
            )
            .unwrap(),
        );
        for op in &extra {
            ingest_op(&db_b, op).unwrap();
        }
        let err = compose_group_message(
            &db_b,
            &bob,
            &[2; 32],
            &gid,
            &chan,
            "chut",
            None,
            vec![],
            3_000,
        );
        assert!(matches!(err, Err(CoreError::OpRejected(_))));

        // Denying VIEW alone blocks writing too.
        let view_deny = author_op(
            &db_a,
            &alice,
            &gid,
            &GroupOpBody::SetChannelPerms {
                channel_id: chan,
                role_id: [3; 16],
                allow: perms::SEND,
                deny: perms::VIEW,
            },
            3_001,
        )
        .unwrap();
        ingest_op(&db_b, &view_deny).unwrap();
        let err = compose_group_message(
            &db_b,
            &bob,
            &[2; 32],
            &gid,
            &chan,
            "toc",
            None,
            vec![],
            3_002,
        );
        assert!(matches!(err, Err(CoreError::OpRejected(_))));

        // Alice (founder) still writes fine.
        compose_group_message(
            &db_a,
            &alice,
            &[1; 32],
            &gid,
            &chan,
            "ok",
            None,
            vec![],
            3_003,
        )
        .unwrap();
    }

    #[test]
    fn ingest_rejects_sender_denied_view_even_if_send_kept() {
        // Sécurité : un membre à qui le salon est masqué (deny VIEW, SEND encore
        // positionné) ne doit pas pouvoir y injecter via un client modifié. Le
        // récepteur doit ignorer le message à l'ingestion, symétriquement à
        // `require_send` côté émission.
        let alice = identity();
        let bob = identity();
        let db_a = open_db();
        let db_b = open_db();
        let (gid, chan) = build_group(&alice, &db_a, &[(&bob, &db_b)]);

        // Bob, membre normal, compose pendant qu'il a encore VIEW+SEND.
        let msg = compose_group_message(
            &db_b,
            &bob,
            &[2; 32],
            &gid,
            &chan,
            "coucou",
            None,
            vec![],
            2_000,
        )
        .expect("composition");

        // Alice masque le salon à Bob (deny VIEW), SEND laissé intact.
        for (op, lamport) in [
            GroupOpBody::AddRole {
                role_id: [7; 16],
                name: "Masqué".into(),
                color: 0,
                position: 1,
                permissions: 0,
            },
            GroupOpBody::AssignRole {
                member: bob.public_key(),
                role_id: [7; 16],
            },
            GroupOpBody::SetChannelPerms {
                channel_id: chan,
                role_id: [7; 16],
                allow: perms::SEND,
                deny: perms::VIEW,
            },
        ]
        .into_iter()
        .zip(3_000..)
        {
            let signed = author_op(&db_a, &alice, &gid, &op, lamport).unwrap();
            ingest_op(&db_a, &signed).unwrap();
        }

        // Le message de Bob (VIEW refusé) est ignoré par Alice.
        let (g, c, id, lam, ms, epoch, enc) = parts(msg);
        let event = ingest_group_message(
            &db_a,
            &[1; 32],
            &bob.public_key(),
            &g,
            &c,
            &id,
            lam,
            ms,
            ms,
            epoch,
            &enc,
        )
        .expect("ingestion");
        assert_eq!(event, GroupMsgEvent::Ignored);
        assert!(db_a
            .group_history(&gid, &chan, u64::MAX, 10)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn missing_epoch_key_is_ignored_and_wrong_context_fails() {
        let alice = identity();
        let bob = identity();
        let db_a = open_db();
        let db_b = open_db();
        let (gid, chan) = build_group(&alice, &db_a, &[(&bob, &db_b)]);

        let msg = compose_group_message(
            &db_a,
            &alice,
            &[1; 32],
            &gid,
            &chan,
            "clé ?",
            None,
            vec![],
            2_000,
        )
        .expect("composition");
        let CoreMsg::GroupMsg {
            msg_id,
            lamport,
            sent_ms,
            key_epoch,
            body_enc,
            ..
        } = msg
        else {
            panic!("GroupMsg attendu");
        };
        // Epoch inconnu chez B : ignoré sans erreur.
        let event = ingest_group_message(
            &db_b,
            &[2; 32],
            &alice.public_key(),
            &gid,
            &chan,
            &msg_id,
            lamport,
            sent_ms,
            sent_ms,
            key_epoch + 7,
            &body_enc,
        )
        .expect("ingestion");
        assert_eq!(event, GroupMsgEvent::Ignored);
        // msg_id falsifié : l'AAD ne colle plus, le déchiffrement échoue.
        let err = ingest_group_message(
            &db_b,
            &[2; 32],
            &alice.public_key(),
            &gid,
            &chan,
            &[9u8; 16],
            lamport,
            sent_ms,
            sent_ms,
            key_epoch,
            &body_enc,
        );
        assert!(err.is_err());
    }

    /// Champs d'ingestion d'une enveloppe `GroupMsg` :
    /// `(group_id, channel_id, msg_id, lamport, sent_ms, key_epoch, body_enc)`.
    type GroupMsgParts = ([u8; 16], [u8; 16], [u8; 16], u64, u64, u32, Vec<u8>);

    /// Déstructure une enveloppe `GroupMsg` en champs d'ingestion.
    fn parts(msg: CoreMsg) -> GroupMsgParts {
        let CoreMsg::GroupMsg {
            group_id,
            channel_id,
            msg_id,
            lamport,
            sent_ms,
            key_epoch,
            body_enc,
        } = msg
        else {
            panic!("GroupMsg attendu");
        };
        (
            group_id, channel_id, msg_id, lamport, sent_ms, key_epoch, body_enc,
        )
    }

    /// Ingère une enveloppe chez `db` au nom de `sender`.
    fn ingest(db: &Db, sender: &Identity, msg: CoreMsg) -> GroupMsgEvent {
        // Cas honnête : l'horloge locale du récepteur coïncide avec `sent_ms`.
        ingest_at(db, sender, msg, None)
    }

    /// Ingestion avec une horloge locale explicite du récepteur (`None` =
    /// coïncide avec `sent_ms`). Sert à éprouver la forge de `sent_ms`.
    fn ingest_at(
        db: &Db,
        sender: &Identity,
        msg: CoreMsg,
        local_now_ms: Option<u64>,
    ) -> GroupMsgEvent {
        let (gid, cid, mid, lamport, sent_ms, epoch, enc) = parts(msg);
        ingest_group_message(
            db,
            &[2; 32],
            &sender.public_key(),
            &gid,
            &cid,
            &mid,
            lamport,
            sent_ms,
            local_now_ms.unwrap_or(sent_ms),
            epoch,
            &enc,
        )
        .expect("ingestion")
    }

    #[test]
    fn edit_delete_react_roundtrip_between_members() {
        let alice = identity();
        let bob = identity();
        let db_a = open_db();
        let db_b = open_db();
        let (gid, chan) = build_group(&alice, &db_a, &[(&bob, &db_b)]);

        // Alice publie, Bob reçoit.
        let text = compose_group_message(
            &db_a,
            &alice,
            &[1; 32],
            &gid,
            &chan,
            "v1",
            None,
            vec![],
            2_000,
        )
        .expect("texte");
        let target = match &text {
            CoreMsg::GroupMsg { msg_id, .. } => *msg_id,
            _ => unreachable!(),
        };
        assert_eq!(ingest(&db_b, &alice, text), GroupMsgEvent::Stored);

        // Édition par l'auteur : appliquée des deux côtés.
        let edit = compose_group_edit(&db_a, &alice, &[1; 32], &gid, &chan, &target, "v2", 2_001)
            .expect("édition");
        assert_eq!(ingest(&db_b, &alice, edit), GroupMsgEvent::Edited);
        let rec = db_b.group_msg(&target).expect("lecture").expect("présent");
        assert_eq!(rec.edited.as_deref(), Some(b"v2".as_slice()));

        // Réaction de Bob : répliquée chez Alice.
        let react = compose_group_reaction(&db_b, &bob, &gid, &chan, &target, "👍", true, 2_002)
            .expect("réaction");
        assert_eq!(ingest(&db_a, &bob, react), GroupMsgEvent::Reacted);
        assert_eq!(
            db_a.reactions(&target).expect("réactions"),
            vec![("👍".to_string(), bob.public_key())]
        );

        // Suppression par l'auteur : tombstone des deux côtés.
        let del =
            compose_group_delete(&db_a, &alice, &gid, &chan, &target, 2_003).expect("suppression");
        assert_eq!(ingest(&db_b, &alice, del), GroupMsgEvent::Deleted);
        let rec = db_b.group_msg(&target).expect("lecture").expect("présent");
        assert!(rec.deleted && rec.body.is_empty());
    }

    #[test]
    fn peer_cannot_edit_or_delete_someone_elses_group_message() {
        let alice = identity();
        let bob = identity();
        let db_a = open_db();
        let db_b = open_db();
        let (gid, chan) = build_group(&alice, &db_a, &[(&bob, &db_b)]);

        let text = compose_group_message(
            &db_a,
            &alice,
            &[1; 32],
            &gid,
            &chan,
            "à moi",
            None,
            vec![],
            2_000,
        )
        .expect("texte");
        let target = match &text {
            CoreMsg::GroupMsg { msg_id, .. } => *msg_id,
            _ => unreachable!(),
        };
        assert_eq!(ingest(&db_b, &alice, text), GroupMsgEvent::Stored);

        // Bob n'est pas l'auteur : édition et suppression refusées chez lui
        // à la composition…
        assert!(
            compose_group_edit(&db_b, &bob, &[1; 32], &gid, &chan, &target, "piraté", 3_000)
                .is_err()
        );
        assert!(compose_group_delete(&db_b, &bob, &gid, &chan, &target, 3_001).is_err());

        // … et une enveloppe forgée par Bob est ignorée chez Alice (la
        // contrainte d'auteur tient à l'ingestion aussi).
        let forged = seal_body(
            &db_b,
            &gid,
            &chan,
            &MsgBody::Edit {
                target,
                new_text: "piraté".into(),
            },
            3_002,
        )
        .expect("scellement");
        assert_eq!(ingest(&db_a, &bob, forged), GroupMsgEvent::Ignored);
        let rec = db_a.group_msg(&target).expect("lecture").expect("présent");
        assert_eq!(rec.edited, None);
    }

    #[test]
    fn reply_to_travels_and_persists_on_both_sides() {
        let alice = identity();
        let bob = identity();
        let db_a = open_db();
        let db_b = open_db();
        let (gid, chan) = build_group(&alice, &db_a, &[(&bob, &db_b)]);

        // Message d'origine, puis une réponse qui le cite.
        let first = compose_group_message(
            &db_a,
            &alice,
            &[1; 32],
            &gid,
            &chan,
            "question",
            None,
            vec![],
            1,
        )
        .expect("origine");
        let origin_id = match &first {
            CoreMsg::GroupMsg { msg_id, .. } => *msg_id,
            _ => unreachable!(),
        };
        assert_eq!(ingest(&db_b, &alice, first), GroupMsgEvent::Stored);

        let reply = compose_group_message(
            &db_a,
            &alice,
            &[1; 32],
            &gid,
            &chan,
            "réponse",
            Some(origin_id),
            vec![],
            2,
        )
        .expect("réponse");
        assert_eq!(ingest(&db_b, &alice, reply), GroupMsgEvent::Stored);

        // Le `reply_to` est restitué à l'identique côté Bob.
        let hist = db_b.group_history(&gid, &chan, u64::MAX, 10).expect("hist");
        let quoted: Vec<Option<[u8; 16]>> = hist
            .iter()
            .filter_map(|m| match MsgBody::decode_body(m.kind, &m.body) {
                Ok(MsgBody::Text { text, reply_to, .. }) if text == "réponse" => Some(reply_to),
                _ => None,
            })
            .collect();
        assert_eq!(quoted, vec![Some(origin_id)]);
    }

    #[test]
    fn attachments_travel_and_persist_on_both_sides() {
        let alice = identity();
        let bob = identity();
        let db_a = open_db();
        let db_b = open_db();
        let (gid, chan) = build_group(&alice, &db_a, &[(&bob, &db_b)]);

        let atts = vec![FileRef {
            merkle_root: [9; 32],
            name: "plan.pdf".into(),
            size: 12_345,
            mime: "application/pdf".into(),
        }];
        let msg = compose_group_message(
            &db_a,
            &alice,
            &[1; 32],
            &gid,
            &chan,
            "ci-joint",
            None,
            atts.clone(),
            2_000,
        )
        .expect("texte");
        let target = match &msg {
            CoreMsg::GroupMsg { msg_id, .. } => *msg_id,
            _ => unreachable!(),
        };
        assert_eq!(db_a.msg_attachments(&target).expect("pj locales"), atts);
        assert_eq!(ingest(&db_b, &alice, msg), GroupMsgEvent::Stored);
        assert_eq!(db_b.msg_attachments(&target).expect("pj reçues"), atts);
    }

    #[test]
    fn channel_override_denying_send_blocks_composition() {
        let alice = identity();
        let bob = identity();
        let db_a = open_db();
        let db_b = open_db();
        let (gid, chan) = build_group(&alice, &db_a, &[(&bob, &db_b)]);

        // Rôle « Muet » refusant SEND sur le salon, attribué à Bob.
        let role_id = new_id16();
        let ops = [
            GroupOpBody::AddRole {
                role_id,
                name: "Muet".into(),
                color: 0,
                position: 1,
                permissions: 0,
            },
            GroupOpBody::AssignRole {
                member: bob.public_key(),
                role_id,
            },
            GroupOpBody::SetChannelPerms {
                channel_id: chan,
                role_id,
                allow: 0,
                deny: accord_proto::core_msg::perms::SEND,
            },
        ];
        for (i, body) in ops.iter().enumerate() {
            let op = author_op(&db_a, &alice, &gid, body, 3_000 + i as u64).expect("op");
            ingest_op(&db_b, &op).expect("rejeu");
        }
        // Bob ne peut plus écrire dans ce salon.
        assert!(matches!(
            compose_group_message(
                &db_b,
                &bob,
                &[1; 32],
                &gid,
                &chan,
                "chut",
                None,
                vec![],
                4_000
            ),
            Err(CoreError::OpRejected(_))
        ));
        // Et une enveloppe qu'il forcerait serait ignorée chez Alice.
        let forged = seal_body(
            &db_b,
            &gid,
            &chan,
            &MsgBody::Text {
                text: "chut".into(),
                reply_to: None,
                attachments: vec![],
            },
            4_001,
        )
        .expect("scellement");
        assert_eq!(ingest(&db_a, &bob, forged), GroupMsgEvent::Ignored);
    }

    #[test]
    fn compose_requires_channel_key_and_membership() {
        let alice = identity();
        let db = open_db();
        let created = create_group(&db, &alice, "Solo", 1_000).expect("création");
        // Salon inexistant.
        assert!(compose_group_message(
            &db,
            &alice,
            &[1; 32],
            &created.group_id,
            &[9; 16],
            "x",
            None,
            vec![],
            2_000
        )
        .is_err());
        // Message vide.
        let chan = new_id16();
        author_op(
            &db,
            &alice,
            &created.group_id,
            &GroupOpBody::AddChannel {
                channel_id: chan,
                name: "g".into(),
                category: None,
                kind: ChannelKind::Text,
                position: 0,
            },
            1_001,
        )
        .expect("salon");
        assert!(compose_group_message(
            &db,
            &alice,
            &[1; 32],
            &created.group_id,
            &chan,
            "   ",
            None,
            vec![],
            2_000
        )
        .is_err());
    }

    #[test]
    fn timeout_blocks_compose_and_ingest() {
        let alice = identity();
        let bob = identity();
        let db_a = open_db();
        let db_b = open_db();
        let (gid, chan) = build_group(&alice, &db_a, &[(&bob, &db_b)]);

        // Bob composes a valid message while still un-muted (sent_ms = 5_000).
        let early = compose_group_message(
            &db_b,
            &bob,
            &[2; 32],
            &gid,
            &chan,
            "avant",
            None,
            vec![],
            5_000,
        )
        .expect("composition");

        // Alice times Bob out until 10_000; replicate the op to Bob.
        let timeout = author_op(
            &db_a,
            &alice,
            &gid,
            &GroupOpBody::TimeoutMember {
                member: bob.public_key(),
                until_ms: 10_000,
            },
            6_000,
        )
        .unwrap();
        ingest_op(&db_b, &timeout).unwrap();

        // Compose is refused while the timeout is active…
        let err = compose_group_message(
            &db_b,
            &bob,
            &[2; 32],
            &gid,
            &chan,
            "pendant",
            None,
            vec![],
            7_000,
        );
        assert!(matches!(err, Err(CoreError::OpRejected(_))));
        // …and allowed again once it has expired (sent_ms = 10_000).
        compose_group_message(
            &db_b,
            &bob,
            &[2; 32],
            &gid,
            &chan,
            "après",
            None,
            vec![],
            10_000,
        )
        .expect("compose after expiry");

        // Bob's earlier message (sent_ms = 5_000, inside the window) is ignored
        // at ingestion on Alice's side once she holds the timeout op.
        assert_eq!(ingest(&db_a, &bob, early), GroupMsgEvent::Ignored);
        assert!(db_a
            .group_history(&gid, &chan, u64::MAX, 10)
            .unwrap()
            .is_empty());

        // Anti-forge : Bob, toujours en sourdine (échéance 10_000), forge un
        // message avec sent_ms = 10_001 pour paraître « après expiration ». Il
        // est quand même ignoré car l'horloge locale d'Alice (7_500 < 10_000)
        // le voit encore en sourdine.
        let forged = seal_body(
            &db_b,
            &gid,
            &chan,
            &MsgBody::Text {
                text: "contournement".into(),
                reply_to: None,
                attachments: vec![],
            },
            10_001,
        )
        .expect("scellement");
        assert_eq!(
            ingest_at(&db_a, &bob, forged, Some(7_500)),
            GroupMsgEvent::Ignored
        );
    }

    #[test]
    fn announcement_channel_is_read_only_for_plain_members() {
        let alice = identity();
        let bob = identity();
        let db_a = open_db();
        let db_b = open_db();
        let (gid, _text) = build_group(&alice, &db_a, &[(&bob, &db_b)]);

        // Alice adds an announcement channel; replicate the op to Bob.
        let announce = new_id16();
        let add = author_op(
            &db_a,
            &alice,
            &gid,
            &GroupOpBody::AddChannel {
                channel_id: announce,
                name: "annonces".into(),
                category: None,
                kind: ChannelKind::Announcement,
                position: 5,
            },
            2_000,
        )
        .unwrap();
        ingest_op(&db_b, &add).unwrap();

        // Bob (plain member) cannot post in the announcement channel…
        let err = compose_group_message(
            &db_b,
            &bob,
            &[2; 32],
            &gid,
            &announce,
            "coucou",
            None,
            vec![],
            3_000,
        );
        assert!(matches!(err, Err(CoreError::OpRejected(_))));
        // …while Alice (MANAGE_CHANNELS via founder) posts fine.
        compose_group_message(
            &db_a,
            &alice,
            &[1; 32],
            &gid,
            &announce,
            "annonce",
            None,
            vec![],
            3_001,
        )
        .expect("founder posts to announcement");

        // A message Bob forces into the announcement channel is ignored on
        // ingestion, symmetrically with the compose-side gate.
        let forged = seal_body(
            &db_b,
            &gid,
            &announce,
            &MsgBody::Text {
                text: "spam".into(),
                reply_to: None,
                attachments: vec![],
            },
            3_002,
        )
        .expect("scellement");
        assert_eq!(ingest(&db_a, &bob, forged), GroupMsgEvent::Ignored);
    }

    /// Zero (the default, and the only value reachable without an explicit
    /// `SetChannelSlowmode`) means slow mode is off: rapid-fire messages
    /// from the same author are never blocked.
    #[test]
    fn slowmode_zero_never_blocks() {
        let alice = identity();
        let bob = identity();
        let db_a = open_db();
        let db_b = open_db();
        let (gid, chan) = build_group(&alice, &db_a, &[(&bob, &db_b)]);

        compose_group_message(
            &db_b,
            &bob,
            &[2; 32],
            &gid,
            &chan,
            "un",
            None,
            vec![],
            1_000,
        )
        .expect("premier");
        compose_group_message(
            &db_b,
            &bob,
            &[2; 32],
            &gid,
            &chan,
            "deux",
            None,
            vec![],
            1_001,
        )
        .expect("immédiatement après, sans mode lent actif");
    }

    #[test]
    fn slowmode_blocks_second_message_within_cooldown_then_allows_after() {
        let alice = identity();
        let bob = identity();
        let db_a = open_db();
        let db_b = open_db();
        let (gid, chan) = build_group(&alice, &db_a, &[(&bob, &db_b)]);

        // Alice (founder, MANAGE_CHANNELS) sets a 10s slow mode; replicated
        // to Bob's node exactly like any other config op.
        let set = author_op(
            &db_a,
            &alice,
            &gid,
            &GroupOpBody::SetChannelSlowmode {
                channel_id: chan,
                seconds: 10,
            },
            2_000,
        )
        .expect("SetChannelSlowmode");
        ingest_op(&db_b, &set).expect("réplication du mode lent");

        // Bob's first message goes through and starts the cooldown.
        compose_group_message(
            &db_b,
            &bob,
            &[2; 32],
            &gid,
            &chan,
            "un",
            None,
            vec![],
            3_000,
        )
        .expect("premier message");

        // 2s later (< 10s cooldown): rejected.
        let err = compose_group_message(
            &db_b,
            &bob,
            &[2; 32],
            &gid,
            &chan,
            "deux",
            None,
            vec![],
            5_000,
        );
        assert!(matches!(err, Err(CoreError::OpRejected(_))));

        // A message to a DIFFERENT channel from the same author, or a
        // message from a DIFFERENT author in the same channel, are both
        // unaffected by Bob's own cooldown (scoped per (channel, author)).
        let announce = new_id16();
        let add = author_op(
            &db_a,
            &alice,
            &gid,
            &GroupOpBody::AddChannel {
                channel_id: announce,
                name: "autre".into(),
                category: None,
                kind: ChannelKind::Text,
                position: 1,
            },
            2_001,
        )
        .expect("second salon");
        ingest_op(&db_b, &add).expect("réplication salon");
        compose_group_message(
            &db_b,
            &bob,
            &[2; 32],
            &gid,
            &announce,
            "ailleurs",
            None,
            vec![],
            5_001,
        )
        .expect("un autre salon n'est pas concerné par le cooldown de Bob");
        compose_group_message(
            &db_a,
            &alice,
            &[1; 32],
            &gid,
            &chan,
            "alice",
            None,
            vec![],
            5_002,
        )
        .expect("un autre auteur n'est pas concerné par le cooldown de Bob");

        // Once >= 10s have elapsed since Bob's first message, he can post again.
        compose_group_message(
            &db_b,
            &bob,
            &[2; 32],
            &gid,
            &chan,
            "trois",
            None,
            vec![],
            13_000,
        )
        .expect("après expiration du cooldown");
    }

    #[test]
    fn slowmode_exempts_manage_channels_and_manage_messages_holders() {
        let alice = identity();
        let db_a = open_db();
        let (gid, chan) = build_group(&alice, &db_a, &[]);

        author_op(
            &db_a,
            &alice,
            &gid,
            &GroupOpBody::SetChannelSlowmode {
                channel_id: chan,
                seconds: 3_600,
            },
            2_000,
        )
        .expect("SetChannelSlowmode");

        // Alice (founder, MANAGE_CHANNELS+MANAGE_MESSAGES via ALL_PERMS) can
        // post back-to-back despite the 1h slow mode (Discord behavior:
        // moderators are never bridled by their own configuration).
        compose_group_message(
            &db_a,
            &alice,
            &[1; 32],
            &gid,
            &chan,
            "un",
            None,
            vec![],
            3_000,
        )
        .expect("premier");
        compose_group_message(
            &db_a,
            &alice,
            &[1; 32],
            &gid,
            &chan,
            "deux",
            None,
            vec![],
            3_001,
        )
        .expect("exemptée, immédiatement après");
    }

    /// Anti-forgery (mirrors `timeout_blocks_compose_and_ingest`): the
    /// ingest-side cooldown is keyed off the RECEIVER's own local clock,
    /// never the sender's self-declared `sent_ms` — a forged `sent_ms` far
    /// in the future buys nothing, every honest peer independently
    /// rate-limits based on when IT actually saw the author's messages.
    #[test]
    fn slowmode_ingest_ignores_too_fast_message_even_with_forged_sent_ms() {
        let alice = identity();
        let bob = identity();
        let db_a = open_db();
        let db_b = open_db();
        let (gid, chan) = build_group(&alice, &db_a, &[(&bob, &db_b)]);

        let set = author_op(
            &db_a,
            &alice,
            &gid,
            &GroupOpBody::SetChannelSlowmode {
                channel_id: chan,
                seconds: 10,
            },
            2_000,
        )
        .expect("SetChannelSlowmode");
        ingest_op(&db_b, &set).expect("réplication");

        // Bob composes and Alice ingests a first message honestly (receiver
        // local clock == sent_ms == 5_000, via the `ingest` helper).
        let first = compose_group_message(
            &db_b,
            &bob,
            &[2; 32],
            &gid,
            &chan,
            "un",
            None,
            vec![],
            5_000,
        )
        .expect("premier");
        assert_eq!(ingest(&db_a, &bob, first), GroupMsgEvent::Stored);

        // Bob (or a modified client) forges a second message claiming
        // `sent_ms` far in the "future" — as if the 10s cooldown had long
        // elapsed — but it reaches Alice almost immediately in real time
        // (her own local clock has only advanced 100ms).
        let forged = seal_body(
            &db_b,
            &gid,
            &chan,
            &MsgBody::Text {
                text: "deux".into(),
                reply_to: None,
                attachments: vec![],
            },
            999_999_000,
        )
        .expect("scellement forgé");
        assert_eq!(
            ingest_at(&db_a, &bob, forged, Some(5_100)),
            GroupMsgEvent::Ignored,
            "le mensonge sur sent_ms ne contourne pas le cooldown, gouverné par l'horloge locale du récepteur"
        );
    }
}
