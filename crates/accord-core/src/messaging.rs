//! Messagerie directe (SPEC §5.3) : composition locale, ingestion distante,
//! accusés, éditions, suppressions, réactions et accusés de lecture.
//!
//! Le pair d'un message n'est jamais un champ du message : c'est la clé
//! publique authentifiée de la session de transport. Un message n'est accepté
//! que d'un ami ; un pair bloqué est ignoré sans réponse observable.

use accord_crypto::{node_id_of, Identity};
use accord_proto::core_msg::{CoreMsg, FileRef, MsgBody};
use accord_proto::limits::MAX_TEXT_BYTES;

use crate::db::{ContactState, Db, DmRecord};
use crate::error::CoreError;
use crate::group::new_id16;
use crate::search;

/// Nombre maximal de pièces jointes par message.
pub const MAX_ATTACHMENTS: usize = 10;

/// Borne du nom de fichier et du type MIME d'une pièce jointe (octets UTF-8).
const MAX_ATTACHMENT_LABEL: usize = 256;

/// Valide une liste de pièces jointes à la composition (le décodage filaire
/// impose déjà les mêmes bornes à l'ingestion).
pub(crate) fn validate_attachments(attachments: &[FileRef]) -> Result<(), CoreError> {
    if attachments.len() > MAX_ATTACHMENTS {
        return Err(CoreError::Invalid("trop de pièces jointes (max 10)"));
    }
    for a in attachments {
        if a.name.trim().is_empty() || a.name.len() > MAX_ATTACHMENT_LABEL {
            return Err(CoreError::Invalid("nom de pièce jointe vide ou trop long"));
        }
        if a.mime.is_empty() || a.mime.len() > MAX_ATTACHMENT_LABEL {
            return Err(CoreError::Invalid("type MIME de pièce jointe invalide"));
        }
        if a.size == 0 || a.size > accord_proto::limits::MAX_FILE_SIZE {
            return Err(CoreError::Invalid("taille de pièce jointe invalide"));
        }
    }
    Ok(())
}

/// Événement produit par l'ingestion d'un `DirectMsg`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DmEvent {
    /// Nouveau message persisté.
    Stored,
    /// Message existant édité par son auteur.
    Edited,
    /// Message existant supprimé par son auteur.
    Deleted,
    /// Réaction ajoutée ou retirée.
    Reacted,
    /// Le pair est en train d'écrire (éphémère).
    Typing,
    /// Le pair a lu jusqu'à un message donné.
    Read,
    /// Doublon ou cible inconnue : rien à faire.
    Noop,
    /// Pair non ami ou bloqué : ignoré silencieusement.
    Ignored,
}

impl DmEvent {
    /// Vrai si l'enveloppe reçue doit être acquittée (`CoreMsg::MsgAck`).
    pub fn should_ack(&self) -> bool {
        matches!(
            self,
            DmEvent::Stored | DmEvent::Edited | DmEvent::Deleted | DmEvent::Reacted
        )
    }
}

/// Vérifie que `peer` est un ami actif.
fn require_friend(db: &Db, peer: &[u8; 32]) -> Result<(), CoreError> {
    match db.contact(&node_id_of(peer).0)?.map(|c| c.state) {
        Some(ContactState::Friend) => Ok(()),
        Some(ContactState::Blocked) => Err(CoreError::OpRejected("pair bloqué")),
        _ => Err(CoreError::OpRejected("pair non ami")),
    }
}

/// Construit l'enveloppe `DirectMsg` d'un corps et le journalise localement
/// si nécessaire. Cœur commun des fonctions `compose_*`.
fn compose(
    db: &Db,
    identity: &Identity,
    peer: &[u8; 32],
    body: &MsgBody,
    now_ms: u64,
) -> Result<CoreMsg, CoreError> {
    require_friend(db, peer)?;
    let msg_id = new_id16();
    let lamport = db.bump_lamport(0)?;
    let encoded = body.encode_body();
    if let MsgBody::Text { attachments, .. } = body {
        db.insert_dm(&DmRecord {
            msg_id,
            peer: *peer,
            author: identity.public_key(),
            lamport,
            sent_ms: now_ms,
            kind: body.kind(),
            body: encoded.clone(),
            acked: false,
            deleted: false,
            edited: None,
        })?;
        db.put_msg_attachments(&msg_id, attachments)?;
    }
    Ok(CoreMsg::DirectMsg {
        msg_id,
        lamport,
        sent_ms: now_ms,
        kind: body.kind(),
        body: encoded,
    })
}

/// Compose un message texte à destination d'un ami. Le message est persisté
/// (non acquitté), indexé pour la recherche, et rendu prêt à émettre.
#[allow(clippy::too_many_arguments)]
pub fn compose_text(
    db: &Db,
    identity: &Identity,
    search_key: &[u8; 32],
    peer: &[u8; 32],
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
    let body = MsgBody::Text {
        text: text.to_string(),
        reply_to,
        attachments,
    };
    let msg = compose(db, identity, peer, &body, now_ms)?;
    if let CoreMsg::DirectMsg { msg_id, .. } = &msg {
        search::index_message(db, search_key, msg_id, text)?;
    }
    Ok(msg)
}

/// Compose l'édition d'un de nos messages. Refuse si le message n'existe
/// pas, n'est pas de nous, ou est supprimé.
pub fn compose_edit(
    db: &Db,
    identity: &Identity,
    search_key: &[u8; 32],
    peer: &[u8; 32],
    target: &[u8; 16],
    new_text: &str,
    now_ms: u64,
) -> Result<CoreMsg, CoreError> {
    if new_text.trim().is_empty() || new_text.len() > MAX_TEXT_BYTES {
        return Err(CoreError::Invalid("texte d'édition vide ou trop long"));
    }
    if !db.edit_dm(target, &identity.public_key(), new_text.as_bytes())? {
        return Err(CoreError::OpRejected("message inéditable"));
    }
    search::reindex_message(db, search_key, target, new_text)?;
    compose(
        db,
        identity,
        peer,
        &MsgBody::Edit {
            target: *target,
            new_text: new_text.to_string(),
        },
        now_ms,
    )
}

/// Compose la suppression d'un de nos messages (tombstone local immédiat).
pub fn compose_delete(
    db: &Db,
    identity: &Identity,
    peer: &[u8; 32],
    target: &[u8; 16],
    now_ms: u64,
) -> Result<CoreMsg, CoreError> {
    if !db.delete_dm(target, &identity.public_key())? {
        return Err(CoreError::OpRejected("message insupprimable"));
    }
    db.unindex_msg(target)?;
    compose(
        db,
        identity,
        peer,
        &MsgBody::Delete { target: *target },
        now_ms,
    )
}

/// Compose l'ajout ou le retrait d'une réaction (appliquée localement).
pub fn compose_reaction(
    db: &Db,
    identity: &Identity,
    peer: &[u8; 32],
    target: &[u8; 16],
    emoji: &str,
    add: bool,
    now_ms: u64,
) -> Result<CoreMsg, CoreError> {
    if emoji.is_empty() || emoji.len() > 32 {
        return Err(CoreError::Invalid("emoji invalide"));
    }
    db.set_reaction(target, &identity.public_key(), emoji, add)?;
    compose(
        db,
        identity,
        peer,
        &MsgBody::Reaction {
            target: *target,
            emoji: emoji.to_string(),
            add,
        },
        now_ms,
    )
}

/// Compose un indicateur de saisie (éphémère, jamais persisté).
pub fn compose_typing(
    db: &Db,
    identity: &Identity,
    peer: &[u8; 32],
    now_ms: u64,
) -> Result<CoreMsg, CoreError> {
    compose(db, identity, peer, &MsgBody::Typing, now_ms)
}

/// Compose un accusé de lecture jusqu'à `up_to`.
pub fn compose_read_receipt(
    db: &Db,
    identity: &Identity,
    peer: &[u8; 32],
    up_to: &[u8; 16],
    now_ms: u64,
) -> Result<CoreMsg, CoreError> {
    compose(
        db,
        identity,
        peer,
        &MsgBody::ReadReceipt { up_to: *up_to },
        now_ms,
    )
}

/// Ingère un `DirectMsg` reçu d'une session authentifiée comme `peer`.
///
/// Rend l'événement produit ; l'appelant émet `MsgAck { msg_id }` si
/// [`DmEvent::should_ack`]. Les messages d'un pair bloqué ou inconnu sont
/// ignorés sans effet observable.
#[allow(clippy::too_many_arguments)]
pub fn ingest_dm(
    db: &Db,
    search_key: &[u8; 32],
    peer: &[u8; 32],
    msg_id: &[u8; 16],
    lamport: u64,
    sent_ms: u64,
    kind: u8,
    body: &[u8],
) -> Result<DmEvent, CoreError> {
    match db.contact(&node_id_of(peer).0)?.map(|c| c.state) {
        Some(ContactState::Friend) => {}
        _ => return Ok(DmEvent::Ignored),
    }
    db.bump_lamport(lamport)?;
    let decoded = MsgBody::decode_body(kind, body)?;
    match decoded {
        MsgBody::Text {
            ref text,
            ref attachments,
            ..
        } => {
            let inserted = db.insert_dm(&DmRecord {
                msg_id: *msg_id,
                peer: *peer,
                author: *peer,
                lamport,
                sent_ms,
                kind,
                body: body.to_vec(),
                acked: true, // reçu = livré ; l'ack concerne l'expéditeur
                deleted: false,
                edited: None,
            })?;
            if !inserted {
                return Ok(DmEvent::Noop);
            }
            db.put_msg_attachments(msg_id, attachments)?;
            search::index_message(db, search_key, msg_id, text)?;
            Ok(DmEvent::Stored)
        }
        MsgBody::Edit { target, new_text } => {
            if new_text.trim().is_empty() || new_text.len() > MAX_TEXT_BYTES {
                return Err(CoreError::Invalid("texte d'édition vide ou trop long"));
            }
            if db.edit_dm(&target, peer, new_text.as_bytes())? {
                search::reindex_message(db, search_key, &target, &new_text)?;
                Ok(DmEvent::Edited)
            } else {
                Ok(DmEvent::Noop)
            }
        }
        MsgBody::Delete { target } => {
            if db.delete_dm(&target, peer)? {
                db.unindex_msg(&target)?;
                Ok(DmEvent::Deleted)
            } else {
                Ok(DmEvent::Noop)
            }
        }
        MsgBody::Reaction { target, emoji, add } => {
            db.set_reaction(&target, peer, &emoji, add)?;
            Ok(DmEvent::Reacted)
        }
        // Server stickers are a group-channel feature only (D-047); a DM
        // carrying one is simply unsupported — ignored without error like
        // any other well-formed-but-inapplicable body.
        MsgBody::Sticker { .. } => Ok(DmEvent::Noop),
        MsgBody::Typing => Ok(DmEvent::Typing),
        MsgBody::ReadReceipt { up_to } => {
            db.set_read_mark(peer, &up_to)?;
            Ok(DmEvent::Read)
        }
    }
}

/// Ingère un accusé de réception applicatif du pair.
pub fn ingest_ack(db: &Db, msg_id: &[u8; 16]) -> Result<(), CoreError> {
    db.ack_dm(msg_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::friends;

    const SK: [u8; 32] = [6u8; 32];

    fn setup_friends() -> (Db, Identity, Identity) {
        let db = Db::open_in_memory(&[5u8; 32]).unwrap();
        let me = Identity::generate_with_pow_bits(1);
        let peer = Identity::generate_with_pow_bits(1);
        friends::request_friend(&db, &peer.public_key(), "Pair", 0).unwrap();
        friends::ingest_friend_response(&db, &peer.public_key(), true, 0).unwrap();
        (db, me, peer)
    }

    fn msg_id_of(msg: &CoreMsg) -> [u8; 16] {
        match msg {
            CoreMsg::DirectMsg { msg_id, .. } => *msg_id,
            _ => panic!("pas un DirectMsg"),
        }
    }

    #[test]
    fn compose_persists_indexes_and_ack_confirms() {
        let (db, me, peer) = setup_friends();
        let msg = compose_text(
            &db,
            &me,
            &SK,
            &peer.public_key(),
            "premier message important",
            None,
            vec![],
            1_000,
        )
        .unwrap();
        let id = msg_id_of(&msg);
        let hist = db.dm_history(&peer.public_key(), u64::MAX, 10).unwrap();
        assert_eq!(hist.len(), 1);
        assert!(!hist[0].acked);
        assert_eq!(search::search(&db, &SK, "important").unwrap(), vec![id]);

        ingest_ack(&db, &id).unwrap();
        let hist = db.dm_history(&peer.public_key(), u64::MAX, 10).unwrap();
        assert!(hist[0].acked);
    }

    #[test]
    fn compose_to_non_friend_is_refused() {
        let db = Db::open_in_memory(&[5u8; 32]).unwrap();
        let me = Identity::generate_with_pow_bits(1);
        let stranger = Identity::generate_with_pow_bits(1);
        let r = compose_text(
            &db,
            &me,
            &SK,
            &stranger.public_key(),
            "coucou",
            None,
            vec![],
            0,
        );
        assert!(matches!(r, Err(CoreError::OpRejected(_))));
    }

    #[test]
    fn ingest_stores_once_and_acks() {
        let (db, _, peer) = setup_friends();
        let body = MsgBody::Text {
            text: "salut de loin".into(),
            reply_to: None,
            attachments: vec![],
        };
        let enc = body.encode_body();
        let ev = ingest_dm(&db, &SK, &peer.public_key(), &[1; 16], 5, 5, 0, &enc).unwrap();
        assert_eq!(ev, DmEvent::Stored);
        assert!(ev.should_ack());
        // Rejeu : aucun doublon, pas de ré-ack nécessaire.
        let ev2 = ingest_dm(&db, &SK, &peer.public_key(), &[1; 16], 5, 5, 0, &enc).unwrap();
        assert_eq!(ev2, DmEvent::Noop);
        assert_eq!(
            db.dm_history(&peer.public_key(), u64::MAX, 10)
                .unwrap()
                .len(),
            1
        );
        // L'horloge locale a dépassé le lamport observé.
        assert!(db.lamport().unwrap() > 5);
    }

    #[test]
    fn ingest_from_stranger_or_blocked_is_ignored() {
        let (db, _, peer) = setup_friends();
        let stranger = Identity::generate_with_pow_bits(1);
        let enc = MsgBody::Typing.encode_body();
        assert_eq!(
            ingest_dm(&db, &SK, &stranger.public_key(), &[1; 16], 1, 1, 5, &enc).unwrap(),
            DmEvent::Ignored
        );
        friends::block(&db, &peer.public_key(), 2).unwrap();
        assert_eq!(
            ingest_dm(&db, &SK, &peer.public_key(), &[2; 16], 2, 2, 5, &enc).unwrap(),
            DmEvent::Ignored
        );
    }

    #[test]
    fn peer_can_only_edit_and_delete_own_messages() {
        let (db, me, peer) = setup_friends();
        // Notre message : le pair ne peut ni l'éditer ni le supprimer.
        let ours =
            compose_text(&db, &me, &SK, &peer.public_key(), "à moi", None, vec![], 1).unwrap();
        let target = msg_id_of(&ours);
        let edit = MsgBody::Edit {
            target,
            new_text: "piraté".into(),
        };
        let ev = ingest_dm(
            &db,
            &SK,
            &peer.public_key(),
            &[7; 16],
            9,
            9,
            edit.kind(),
            &edit.encode_body(),
        )
        .unwrap();
        assert_eq!(ev, DmEvent::Noop);
        let del = MsgBody::Delete { target };
        let ev = ingest_dm(
            &db,
            &SK,
            &peer.public_key(),
            &[8; 16],
            10,
            10,
            del.kind(),
            &del.encode_body(),
        )
        .unwrap();
        assert_eq!(ev, DmEvent::Noop);

        // Son propre message : édition acceptée et réindexée.
        let their_body = MsgBody::Text {
            text: "original unique".into(),
            reply_to: None,
            attachments: vec![],
        };
        ingest_dm(
            &db,
            &SK,
            &peer.public_key(),
            &[9; 16],
            11,
            11,
            0,
            &their_body.encode_body(),
        )
        .unwrap();
        let edit2 = MsgBody::Edit {
            target: [9; 16],
            new_text: "corrigé désormais".into(),
        };
        let ev = ingest_dm(
            &db,
            &SK,
            &peer.public_key(),
            &[10; 16],
            12,
            12,
            edit2.kind(),
            &edit2.encode_body(),
        )
        .unwrap();
        assert_eq!(ev, DmEvent::Edited);
        assert!(search::search(&db, &SK, "original").unwrap().is_empty());
        assert_eq!(
            search::search(&db, &SK, "corrigé").unwrap(),
            vec![[9u8; 16]]
        );
    }

    #[test]
    fn reactions_and_read_receipts_roundtrip() {
        let (db, me, peer) = setup_friends();
        let msg =
            compose_text(&db, &me, &SK, &peer.public_key(), "réagis", None, vec![], 1).unwrap();
        let target = msg_id_of(&msg);
        let react = MsgBody::Reaction {
            target,
            emoji: "👍".into(),
            add: true,
        };
        ingest_dm(
            &db,
            &SK,
            &peer.public_key(),
            &[3; 16],
            2,
            2,
            react.kind(),
            &react.encode_body(),
        )
        .unwrap();
        assert_eq!(
            db.reactions(&target).unwrap(),
            vec![("👍".to_string(), peer.public_key())]
        );

        let rr = MsgBody::ReadReceipt { up_to: target };
        let ev = ingest_dm(
            &db,
            &SK,
            &peer.public_key(),
            &[4; 16],
            3,
            3,
            rr.kind(),
            &rr.encode_body(),
        )
        .unwrap();
        assert_eq!(ev, DmEvent::Read);
        assert!(!ev.should_ack());
        assert_eq!(db.read_mark(&peer.public_key()).unwrap(), Some(target));
    }

    fn file_ref(b: u8) -> FileRef {
        FileRef {
            merkle_root: [b; 32],
            name: format!("piece-{b}.png"),
            size: 1_000,
            mime: "image/png".into(),
        }
    }

    #[test]
    fn compose_persists_attachments_and_delete_wipes_them() {
        let (db, me, peer) = setup_friends();
        let atts = vec![file_ref(1), file_ref(2)];
        let msg = compose_text(
            &db,
            &me,
            &SK,
            &peer.public_key(),
            "avec pièces",
            None,
            atts.clone(),
            1,
        )
        .unwrap();
        let id = msg_id_of(&msg);
        assert_eq!(db.msg_attachments(&id).unwrap(), atts);
        compose_delete(&db, &me, &peer.public_key(), &id, 2).unwrap();
        assert!(db.msg_attachments(&id).unwrap().is_empty());
    }

    #[test]
    fn compose_rejects_invalid_attachments() {
        let (db, me, peer) = setup_friends();
        // Trop de pièces jointes.
        let too_many: Vec<FileRef> = (0..11).map(|i| file_ref(i as u8)).collect();
        assert!(matches!(
            compose_text(&db, &me, &SK, &peer.public_key(), "x", None, too_many, 1),
            Err(CoreError::Invalid(_))
        ));
        // Nom vide.
        let mut bad = file_ref(1);
        bad.name = "  ".into();
        assert!(compose_text(&db, &me, &SK, &peer.public_key(), "x", None, vec![bad], 1).is_err());
        // Taille nulle.
        let mut bad = file_ref(1);
        bad.size = 0;
        assert!(compose_text(&db, &me, &SK, &peer.public_key(), "x", None, vec![bad], 1).is_err());
        // Message sans texte mais avec pièce jointe : accepté.
        assert!(compose_text(
            &db,
            &me,
            &SK,
            &peer.public_key(),
            "",
            None,
            vec![file_ref(3)],
            1
        )
        .is_ok());
    }

    #[test]
    fn ingest_persists_attachments() {
        let (db, _, peer) = setup_friends();
        let atts = vec![file_ref(9)];
        let body = MsgBody::Text {
            text: "regarde".into(),
            reply_to: None,
            attachments: atts.clone(),
        };
        let enc = body.encode_body();
        let ev = ingest_dm(&db, &SK, &peer.public_key(), &[4; 16], 5, 5, 0, &enc).unwrap();
        assert_eq!(ev, DmEvent::Stored);
        assert_eq!(db.msg_attachments(&[4; 16]).unwrap(), atts);
    }

    #[test]
    fn local_delete_wipes_body_and_index() {
        let (db, me, peer) = setup_friends();
        let msg = compose_text(
            &db,
            &me,
            &SK,
            &peer.public_key(),
            "secret à effacer",
            None,
            vec![],
            1,
        )
        .unwrap();
        let target = msg_id_of(&msg);
        compose_delete(&db, &me, &peer.public_key(), &target, 2).unwrap();
        let hist = db.dm_history(&peer.public_key(), u64::MAX, 10).unwrap();
        let rec = hist.iter().find(|m| m.msg_id == target).unwrap();
        assert!(rec.deleted);
        assert!(rec.body.is_empty());
        assert!(search::search(&db, &SK, "secret").unwrap().is_empty());
    }
}
