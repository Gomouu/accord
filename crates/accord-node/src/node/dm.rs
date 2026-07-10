//! Messagerie directe : composition et routage des messages, éditions,
//! suppressions et réactions (bloc `impl Node` du domaine `dm.*`).

use accord_core::db::DmRecord;
use accord_core::messaging;
use accord_proto::core_msg::{CoreMsg, FileRef};
use serde_json::{json, Value};

use crate::error::NodeError;
use crate::hex;
use crate::outbound::Outbound;

use super::{dm_mark_key, now_ms, read_u64, Node};

/// Meta key of the read-receipts privacy toggle (absent = enabled).
const READ_RECEIPTS_KEY: &str = "dm.read_receipts";

/// Rend une liste de pièces jointes en JSON (forme gelée côté UI).
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

impl Node {
    /// Compose et route un message texte ; persiste et met en file si le pair
    /// est hors ligne (géré par la boucle réseau).
    pub fn dm_send(
        &self,
        peer_pubkey: &[u8; 32],
        text: &str,
        reply_to: Option<[u8; 16]>,
    ) -> Result<String, NodeError> {
        self.dm_send_with_attachments(peer_pubkey, text, reply_to, vec![])
    }

    /// Compose et route un message texte avec pièces jointes (≤ 10, déjà
    /// publiées dans le magasin de fichiers).
    pub fn dm_send_with_attachments(
        &self,
        peer_pubkey: &[u8; 32],
        text: &str,
        reply_to: Option<[u8; 16]>,
        attachments: Vec<FileRef>,
    ) -> Result<String, NodeError> {
        let msg = self.with_db(|db| {
            Ok(messaging::compose_text(
                db,
                &self.identity,
                &self.search_key,
                peer_pubkey,
                text,
                reply_to,
                attachments,
                now_ms(),
            )?)
        })?;
        let msg_id = match &msg {
            CoreMsg::DirectMsg { msg_id, .. } => hex::encode(msg_id),
            _ => unreachable!("compose_text produit un DirectMsg"),
        };
        self.outbound.send(Outbound::Core {
            to: *peer_pubkey,
            msg: Box::new(msg),
        });
        Ok(msg_id)
    }

    /// Pièces jointes persistées d'un message (DM ou groupe).
    pub fn attachments_of(&self, msg_id: &[u8; 16]) -> Result<Vec<FileRef>, NodeError> {
        self.with_db(|db| Ok(db.msg_attachments(msg_id)?))
    }

    /// Historique d'une conversation directe.
    pub fn dm_history(
        &self,
        peer_pubkey: &[u8; 32],
        before_lamport: u64,
        limit: usize,
    ) -> Result<Vec<DmRecord>, NodeError> {
        self.with_db(|db| Ok(db.dm_history(peer_pubkey, before_lamport, limit)?))
    }

    /// Émet un indicateur de frappe éphémère vers un ami. Jamais persisté,
    /// jamais mis en file : si le pair n'est pas présumé en ligne, silence
    /// (« pair injoignable = silencieusement ignoré », SPEC §6).
    pub fn dm_typing(&self, peer_pubkey: &[u8; 32]) -> Result<(), NodeError> {
        if !self.is_online(peer_pubkey) {
            return Ok(());
        }
        let msg = self.with_db(|db| {
            Ok(messaging::compose_typing(
                db,
                &self.identity,
                peer_pubkey,
                now_ms(),
            )?)
        })?;
        self.outbound.send(Outbound::Core {
            to: *peer_pubkey,
            msg: Box::new(msg),
        });
        Ok(())
    }

    /// Marque la conversation avec `peer` lue jusqu'à `lamport` (position
    /// locale, persistée dans les métadonnées, pour le calcul des non-lus).
    ///
    /// Best-effort read receipt (ephemeral, like typing): when the mark
    /// actually advances, the privacy toggle is on and the peer is presumed
    /// online, a `ReadReceipt` targeting the peer's latest covered message is
    /// emitted — never persisted, never queued offline.
    pub fn dm_mark_read(&self, peer_pubkey: &[u8; 32], lamport: u64) -> Result<(), NodeError> {
        let previous = self.with_db(|db| Ok(read_u64(db.meta(&dm_mark_key(peer_pubkey))?)))?;
        self.with_db(|db| Ok(db.set_meta(&dm_mark_key(peer_pubkey), &lamport.to_be_bytes())?))?;
        // Throttle: only marks that advance emit a receipt (re-marking the
        // same position, e.g. on window refocus, stays silent).
        if lamport <= previous || !self.read_receipts_enabled()? || !self.is_online(peer_pubkey) {
            return Ok(());
        }
        let receipt = self.with_db(|db| {
            let Some(up_to) = db.latest_dm_from_peer(peer_pubkey, lamport)? else {
                return Ok(None);
            };
            Ok(Some(messaging::compose_read_receipt(
                db,
                &self.identity,
                peer_pubkey,
                &up_to,
                now_ms(),
            )?))
        });
        // Best-effort: a receipt that cannot be composed (e.g. the contact
        // is not a friend anymore) is silently dropped.
        if let Ok(Some(msg)) = receipt {
            self.outbound.send(Outbound::Core {
                to: *peer_pubkey,
                msg: Box::new(msg),
            });
        }
        Ok(())
    }

    /// Vrai si l'émission des accusés de lecture est activée (réglage de
    /// confidentialité, persisté dans la table meta ; activé par défaut).
    /// Les accusés entrants restent enregistrés quel que soit le réglage.
    pub fn read_receipts_enabled(&self) -> Result<bool, NodeError> {
        self.with_db(|db| {
            Ok(db
                .meta(READ_RECEIPTS_KEY)?
                .map(|v| v.first() != Some(&0))
                .unwrap_or(true))
        })
    }

    /// Active ou coupe l'émission des accusés de lecture (persisté).
    pub fn set_read_receipts(&self, enabled: bool) -> Result<(), NodeError> {
        self.with_db(|db| Ok(db.set_meta(READ_RECEIPTS_KEY, &[u8::from(enabled)])?))
    }

    /// Position de lecture du pair dans la conversation (lamport du dernier
    /// message couvert par son accusé de lecture), si connue.
    pub fn dm_peer_read_lamport(&self, peer_pubkey: &[u8; 32]) -> Result<Option<u64>, NodeError> {
        self.with_db(|db| {
            let Some(msg_id) = db.read_mark(peer_pubkey)? else {
                return Ok(None);
            };
            Ok(db.dm_lamport(&msg_id)?)
        })
    }

    /// Nombre de messages du pair reçus après notre marque de lecture locale.
    pub fn dm_unread(&self, peer_pubkey: &[u8; 32]) -> Result<u64, NodeError> {
        self.with_db(|db| {
            let mark = read_u64(db.meta(&dm_mark_key(peer_pubkey))?);
            Ok(db.count_dm_unread(peer_pubkey, mark)?)
        })
    }

    /// Édite un de nos messages directs (auteur seul, refusé sinon) puis
    /// route l'édition vers le pair, sur le même chemin que [`Node::dm_send`].
    pub fn dm_edit(
        &self,
        peer_pubkey: &[u8; 32],
        target: &[u8; 16],
        new_text: &str,
    ) -> Result<(), NodeError> {
        let msg = self.with_db(|db| {
            Ok(messaging::compose_edit(
                db,
                &self.identity,
                &self.search_key,
                peer_pubkey,
                target,
                new_text,
                now_ms(),
            )?)
        })?;
        self.outbound.send(Outbound::Core {
            to: *peer_pubkey,
            msg: Box::new(msg),
        });
        Ok(())
    }

    /// Supprime un de nos messages directs (tombstone local immédiat) puis
    /// route la suppression vers le pair.
    pub fn dm_delete(&self, peer_pubkey: &[u8; 32], target: &[u8; 16]) -> Result<(), NodeError> {
        let msg = self.with_db(|db| {
            Ok(messaging::compose_delete(
                db,
                &self.identity,
                peer_pubkey,
                target,
                now_ms(),
            )?)
        })?;
        self.outbound.send(Outbound::Core {
            to: *peer_pubkey,
            msg: Box::new(msg),
        });
        Ok(())
    }

    /// Ajoute (`add = true`) ou retire une réaction sur un message direct,
    /// applique le changement localement puis le route vers le pair.
    pub fn dm_react(
        &self,
        peer_pubkey: &[u8; 32],
        target: &[u8; 16],
        emoji: &str,
        add: bool,
    ) -> Result<(), NodeError> {
        let msg = self.with_db(|db| {
            Ok(messaging::compose_reaction(
                db,
                &self.identity,
                peer_pubkey,
                target,
                emoji,
                add,
                now_ms(),
            )?)
        })?;
        self.outbound.send(Outbound::Core {
            to: *peer_pubkey,
            msg: Box::new(msg),
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use accord_core::db::Db;
    use accord_crypto::Identity;
    use accord_proto::core_msg::MsgBody;
    use tokio::sync::mpsc;

    use super::*;
    use crate::outbound::OutboundSink;

    /// Node wired to an outbound channel, with one established friend that
    /// already sent us a text message (lamport of that message returned).
    fn node_with_incoming_dm() -> (Node, [u8; 32], u64, mpsc::Receiver<Outbound>) {
        let id = Identity::generate_with_pow_bits(1);
        let db = Db::open_in_memory(&[1u8; 32]).unwrap();
        let (sink, mut rx) = OutboundSink::channel(64);
        let node = Node::new(id, db, sink);
        let peer = Identity::generate_with_pow_bits(1);
        node.friend_request(&peer.public_key(), "Pair").unwrap();
        node.ingest_core(
            &peer.public_key(),
            CoreMsg::FriendResponse { accepted: true },
        )
        .unwrap();
        let body = MsgBody::Text {
            text: "coucou".into(),
            reply_to: None,
            attachments: vec![],
        };
        let lamport = 7;
        node.ingest_core(
            &peer.public_key(),
            CoreMsg::DirectMsg {
                msg_id: [9; 16],
                lamport,
                sent_ms: 1_000,
                kind: body.kind(),
                body: body.encode_body(),
            },
        )
        .unwrap();
        while rx.try_recv().is_ok() {}
        (node, peer.public_key(), lamport, rx)
    }

    /// Next outgoing `DirectMsg` of the given body kind, if any.
    fn next_dm_of_kind(rx: &mut mpsc::Receiver<Outbound>, wanted: u8) -> Option<CoreMsg> {
        while let Ok(action) = rx.try_recv() {
            if let Outbound::Core { msg, .. } = action {
                if matches!(*msg, CoreMsg::DirectMsg { kind, .. } if kind == wanted) {
                    return Some(*msg);
                }
            }
        }
        None
    }

    #[test]
    fn mark_read_sends_receipt_to_online_peer_once() {
        let (node, peer, lamport, mut rx) = node_with_incoming_dm();
        // Peer is presumed online (their message was ingested).
        node.dm_mark_read(&peer, lamport).unwrap();
        let msg = next_dm_of_kind(&mut rx, 6).expect("accusé de lecture attendu");
        match msg {
            CoreMsg::DirectMsg { kind, body, .. } => {
                assert_eq!(kind, 6);
                assert_eq!(
                    MsgBody::decode_body(kind, &body).unwrap(),
                    MsgBody::ReadReceipt { up_to: [9; 16] }
                );
            }
            other => panic!("message inattendu : {other:?}"),
        }
        // Throttle: re-marking the same position emits nothing.
        node.dm_mark_read(&peer, lamport).unwrap();
        assert!(next_dm_of_kind(&mut rx, 6).is_none());
    }

    #[test]
    fn mark_read_stays_silent_for_offline_peer() {
        let (node, peer, lamport, mut rx) = node_with_incoming_dm();
        node.ingest_core(
            &peer,
            CoreMsg::Presence {
                status: 3,
                custom: None,
            },
        )
        .unwrap();
        node.dm_mark_read(&peer, lamport).unwrap();
        assert!(next_dm_of_kind(&mut rx, 6).is_none());
        // The local read mark is persisted anyway (unread counter drops).
        assert_eq!(node.dm_unread(&peer).unwrap(), 0);
    }

    #[test]
    fn privacy_toggle_disables_outgoing_receipts_only() {
        let (node, peer, lamport, mut rx) = node_with_incoming_dm();
        assert!(node.read_receipts_enabled().unwrap());
        node.set_read_receipts(false).unwrap();
        assert!(!node.read_receipts_enabled().unwrap());

        node.dm_mark_read(&peer, lamport).unwrap();
        assert!(next_dm_of_kind(&mut rx, 6).is_none());

        // Incoming receipts are still recorded (peer read our message).
        let msg_id = {
            let hex_id = node.dm_send(&peer, "lu ?", None).unwrap();
            crate::hex::decode::<16>(&hex_id).unwrap()
        };
        let rr = MsgBody::ReadReceipt { up_to: msg_id };
        node.ingest_core(
            &peer,
            CoreMsg::DirectMsg {
                msg_id: [8; 16],
                lamport: 50,
                sent_ms: 2_000,
                kind: rr.kind(),
                body: rr.encode_body(),
            },
        )
        .unwrap();
        assert!(node.dm_peer_read_lamport(&peer).unwrap().is_some());

        // Re-enabling restores emission on the next advance.
        node.set_read_receipts(true).unwrap();
        node.dm_mark_read(&peer, lamport + 100).unwrap();
        assert!(next_dm_of_kind(&mut rx, 6).is_some());
    }

    #[test]
    fn peer_read_lamport_maps_receipt_to_conversation_position() {
        let (node, peer, _lamport, _rx) = node_with_incoming_dm();
        assert_eq!(node.dm_peer_read_lamport(&peer).unwrap(), None);
        let hex_id = node.dm_send(&peer, "à lire", None).unwrap();
        let msg_id = crate::hex::decode::<16>(&hex_id).unwrap();
        let sent_lamport = node.dm_history(&peer, u64::MAX, 1).unwrap()[0].lamport;
        let rr = MsgBody::ReadReceipt { up_to: msg_id };
        node.ingest_core(
            &peer,
            CoreMsg::DirectMsg {
                msg_id: [7; 16],
                lamport: sent_lamport + 1,
                sent_ms: 3_000,
                kind: rr.kind(),
                body: rr.encode_body(),
            },
        )
        .unwrap();
        assert_eq!(
            node.dm_peer_read_lamport(&peer).unwrap(),
            Some(sent_lamport)
        );
    }
}
