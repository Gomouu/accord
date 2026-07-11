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
use accord_proto::core_msg::{ChannelKind, CoreMsg, FileRef, MsgBody};
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

/// Chiffre un corps par la clé de groupe courante et rend le `CoreMsg` à
/// diffuser (l'horloge de Lamport locale est avancée).
fn seal_body(
    db: &Db,
    group_id: &[u8; 16],
    channel_id: &[u8; 16],
    body: &MsgBody,
    now_ms: u64,
) -> Result<CoreMsg, CoreError> {
    let stored = db
        .latest_group_key(group_id)?
        .ok_or(CoreError::Invalid("clé de groupe absente"))?;
    let msg_id = new_id16();
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
    require_send(db, identity, group_id, channel_id, now_ms)?;

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
}
