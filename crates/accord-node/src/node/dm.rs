//! Messagerie directe : composition et routage des messages, éditions,
//! suppressions et réactions (bloc `impl Node` du domaine `dm.*`).

use std::collections::{BTreeSet, HashMap};

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

/// Direct-send attempts after which an unacked message is surfaced as
/// `failed`. The message keeps being retried in the background (backoff) until
/// the offline-queue expiry; `failed` is a UI hint, not a terminal state.
const DM_FAILED_ATTEMPTS: u32 = 5;

/// Offline-queue lifetime (SPEC §7, mirrors `db::outbox::QUEUE_EXPIRY_MS`):
/// past it, an unacked message that is no longer queued is `failed`.
const DM_QUEUE_EXPIRY_MS: u64 = 7 * 24 * 3600 * 1000;

/// Per-message delivery state derived from the ack flag and the outbox.
///
/// - `sent`: acked by the peer (or an incoming message, delivered by definition);
/// - `failed`: our unacked message whose direct retries are exhausted
///   (`attempts >= DM_FAILED_ATTEMPTS`) or which is unacked, no longer queued
///   and older than the queue expiry;
/// - `pending`: our unacked message still in flight or being retried.
fn dm_delivery_state(
    rec: &DmRecord,
    me: &[u8; 32],
    outbox: &HashMap<[u8; 16], (u32, bool)>,
    now: u64,
) -> &'static str {
    if rec.acked || rec.author != *me {
        return "sent";
    }
    match outbox.get(&rec.msg_id) {
        Some((attempts, _)) if *attempts >= DM_FAILED_ATTEMPTS => "failed",
        Some(_) => "pending",
        None if now.saturating_sub(rec.sent_ms) > DM_QUEUE_EXPIRY_MS => "failed",
        None => "pending",
    }
}

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
    /// La marque est monotone : une position inférieure ou égale à celle déjà
    /// retenue est ignorée en silence.
    ///
    /// Best-effort read receipt (ephemeral, like typing): when the mark
    /// actually advances, the privacy toggle is on and the peer is presumed
    /// online, a `ReadReceipt` targeting the peer's latest covered message is
    /// emitted — never persisted, never queued offline.
    pub fn dm_mark_read(&self, peer_pubkey: &[u8; 32], lamport: u64) -> Result<(), NodeError> {
        let key = dm_mark_key(peer_pubkey);
        let previous = self.with_db(|db| Ok(read_u64(db.meta(&key)?)))?;
        // Un compte a plusieurs appareils, et « lu = lu sur au moins un
        // appareil » (MULTI_DEVICE.md §5). Sans cette monotonie, un appareil
        // resté trois jours éteint rouvrirait la conversation sur sa vieille
        // position et ferait « redevenir non lus » des messages lus depuis
        // longtemps : la conversation se remarquerait non lue toute seule ici,
        // et l'accusé émis rembobinerait la vue de l'expéditeur. Un appareil en
        // retard n'est pas une erreur — sa position est ignorée en silence.
        let advanced = lamport > previous;
        if advanced {
            self.with_db(|db| Ok(db.set_meta(&key, &lamport.to_be_bytes())?))?;
        }
        // Ouvrir une conversation éteint ses mentions non lues (parité Discord).
        // Sans condition de position : acquitter une mention ne va que dans le
        // sens du lu, donc ne peut pas faire régresser l'état.
        self.with_db(|db| Ok(db.mark_dm_mentions_read(peer_pubkey)?))?;
        // Throttle: only marks that advance emit a receipt (re-marking the
        // same position, e.g. on window refocus, stays silent).
        if !advanced || !self.read_receipts_enabled()? || !self.is_online(peer_pubkey) {
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

    /// Notre propre marque de lecture locale dans cette conversation (lamport
    /// du dernier message lu, `0` si jamais marquée) — sert au séparateur
    /// « nouveaux messages » de l'UI, capturé à l'ouverture avant le marquage.
    pub fn dm_read_lamport(&self, peer_pubkey: &[u8; 32]) -> Result<u64, NodeError> {
        self.with_db(|db| Ok(read_u64(db.meta(&dm_mark_key(peer_pubkey))?)))
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

    /// Fenêtre d'historique centrée sur `msg_id` (jump-to-message) : moitié
    /// avant, la cible, moitié après. Rend `(fenêtre, found)` ; `found = false`
    /// avec une fenêtre vide si la cible est inconnue localement.
    pub fn dm_history_around(
        &self,
        peer_pubkey: &[u8; 32],
        msg_id: &[u8; 16],
        limit: usize,
    ) -> Result<(Vec<DmRecord>, bool), NodeError> {
        self.with_db(
            |db| match db.dm_history_around(peer_pubkey, msg_id, limit)? {
                Some(window) => Ok((window, true)),
                None => Ok((Vec::new(), false)),
            },
        )
    }

    /// Épingle un message direct puis réplique l'état au pair sur le chemin
    /// fiable de [`Node::dm_send`]. Le message doit être connu localement et
    /// appartenir à cette conversation ; le jeu d'épingles est borné à
    /// [`messaging::MAX_DM_PINS`].
    pub fn dm_pin(&self, peer_pubkey: &[u8; 32], msg_id: &[u8; 16]) -> Result<(), NodeError> {
        let msg = self.with_db(|db| {
            match db.dm_message(msg_id)? {
                Some(rec) if rec.peer == *peer_pubkey => {}
                _ => {
                    return Err(NodeError::NotFound(
                        "message inconnu dans cette conversation",
                    ))
                }
            }
            let pins = db.dm_pins(peer_pubkey)?;
            if pins.len() >= messaging::MAX_DM_PINS && !pins.contains(msg_id) {
                return Err(NodeError::Invalid("trop de messages épinglés"));
            }
            db.dm_pin(peer_pubkey, msg_id)?;
            Ok(messaging::compose_pin(
                db,
                &self.identity,
                peer_pubkey,
                msg_id,
                true,
                now_ms(),
            )?)
        })?;
        self.outbound.send(Outbound::Core {
            to: *peer_pubkey,
            msg: Box::new(msg),
        });
        Ok(())
    }

    /// Retire l'épingle d'un message direct (sans effet si absente) puis
    /// réplique le retrait au pair sur le chemin fiable de [`Node::dm_send`].
    pub fn dm_unpin(&self, peer_pubkey: &[u8; 32], msg_id: &[u8; 16]) -> Result<(), NodeError> {
        let msg = self.with_db(|db| {
            db.dm_unpin(peer_pubkey, msg_id)?;
            Ok(messaging::compose_pin(
                db,
                &self.identity,
                peer_pubkey,
                msg_id,
                false,
                now_ms(),
            )?)
        })?;
        self.outbound.send(Outbound::Core {
            to: *peer_pubkey,
            msg: Box::new(msg),
        });
        Ok(())
    }

    /// Messages épinglés d'une conversation directe (hex), ordre d'identifiant.
    pub fn dm_pins(&self, peer_pubkey: &[u8; 32]) -> Result<Vec<String>, NodeError> {
        self.with_db(|db| {
            Ok(db
                .dm_pins(peer_pubkey)?
                .iter()
                .map(|id| hex::encode(id))
                .collect())
        })
    }

    /// Ensemble des messages épinglés (annotation `pinned` de l'historique).
    pub fn dm_pinned_set(&self, peer_pubkey: &[u8; 32]) -> Result<BTreeSet<[u8; 16]>, NodeError> {
        self.with_db(|db| Ok(db.dm_pinned_set(peer_pubkey)?))
    }

    /// État de livraison des `DirectMsg` encore en file pour un pair
    /// (`msg_id → (tentatives, déposé en boîte)`), pour calculer `delivery`.
    pub fn dm_outbox_states(
        &self,
        peer_pubkey: &[u8; 32],
    ) -> Result<HashMap<[u8; 16], (u32, bool)>, NodeError> {
        self.with_db(|db| {
            let mut map = HashMap::new();
            for item in db.outbox_for(peer_pubkey)? {
                if let Ok(CoreMsg::DirectMsg { msg_id, .. }) =
                    crate::maintenance::decode_core(&item.payload)
                {
                    map.insert(msg_id, (item.attempts, item.mailboxed_day > 0));
                }
            }
            Ok(map)
        })
    }

    /// État de livraison d'un message (`"sent"` | `"pending"` | `"failed"`),
    /// dérivé de l'accusé et de la file d'attente (`dm_outbox_states`).
    pub fn dm_delivery(
        &self,
        rec: &DmRecord,
        outbox: &HashMap<[u8; 16], (u32, bool)>,
    ) -> &'static str {
        dm_delivery_state(rec, &self.public_key(), outbox, now_ms())
    }

    /// Relance l'envoi d'un de nos messages directs non acquitté (jump-to-retry
    /// d'un état `failed`/`pending`). Purge toute copie en file (backoff remis à
    /// zéro) puis réémet sur le même chemin que [`Node::dm_send`].
    pub fn dm_retry(&self, peer_pubkey: &[u8; 32], msg_id: &[u8; 16]) -> Result<(), NodeError> {
        let me = self.public_key();
        let rec = self
            .with_db(|db| Ok(db.dm_message(msg_id)?))?
            .ok_or(NodeError::NotFound("message inconnu"))?;
        if rec.peer != *peer_pubkey || rec.author != me {
            return Err(NodeError::Invalid("message non renvoyable"));
        }
        if rec.deleted {
            return Err(NodeError::Invalid("message supprimé"));
        }
        if rec.acked {
            return Err(NodeError::Invalid("message déjà livré"));
        }
        // Retire toute copie encore en file pour repartir d'un backoff neuf : la
        // réémission ci-dessous en recréera une si le pair est injoignable.
        self.outbox_ack(peer_pubkey, msg_id)?;
        let msg = CoreMsg::DirectMsg {
            msg_id: rec.msg_id,
            lamport: rec.lamport,
            sent_ms: rec.sent_ms,
            kind: rec.kind,
            body: rec.body,
        };
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
    fn pin_unpin_and_history_around_window() {
        let (node, peer, _lamport, _rx) = node_with_incoming_dm();
        let hex_id = node.dm_send(&peer, "à épingler", None).unwrap();
        let mid = crate::hex::decode::<16>(&hex_id).unwrap();
        // Pinning an unknown message fails; a known one succeeds (idempotent).
        assert!(node.dm_pin(&peer, &[0xEE; 16]).is_err());
        node.dm_pin(&peer, &mid).unwrap();
        assert_eq!(node.dm_pins(&peer).unwrap(), vec![hex_id]);
        assert!(node.dm_pinned_set(&peer).unwrap().contains(&mid));
        node.dm_unpin(&peer, &mid).unwrap();
        assert!(node.dm_pins(&peer).unwrap().is_empty());

        // history_around centers on the target; unknown id ⇒ found = false.
        let (window, found) = node.dm_history_around(&peer, &mid, 10).unwrap();
        assert!(found && window.iter().any(|m| m.msg_id == mid));
        let (empty, found) = node.dm_history_around(&peer, &[0xEE; 16], 10).unwrap();
        assert!(!found && empty.is_empty());
    }

    #[test]
    fn delivery_states_and_retry_reemits() {
        let (node, peer, _lamport, mut rx) = node_with_incoming_dm();
        let hex_id = node.dm_send(&peer, "coucou", None).unwrap();
        let mid = crate::hex::decode::<16>(&hex_id).unwrap();
        while rx.try_recv().is_ok() {}
        let rec = || {
            node.dm_history(&peer, u64::MAX, 10)
                .unwrap()
                .into_iter()
                .find(|m| m.msg_id == mid)
                .unwrap()
        };
        // Not queued, fresh ⇒ pending.
        let empty: HashMap<[u8; 16], (u32, bool)> = HashMap::new();
        assert_eq!(node.dm_delivery(&rec(), &empty), "pending");
        // Exhausted direct retries ⇒ failed.
        let mut map = HashMap::new();
        map.insert(mid, (DM_FAILED_ATTEMPTS, false));
        assert_eq!(node.dm_delivery(&rec(), &map), "failed");

        // Retry re-emits the same DirectMsg (kind 0 = Text).
        node.dm_retry(&peer, &mid).unwrap();
        match next_dm_of_kind(&mut rx, 0).expect("réémission attendue") {
            CoreMsg::DirectMsg { msg_id, .. } => assert_eq!(msg_id, mid),
            other => panic!("message inattendu : {other:?}"),
        }

        // Acked ⇒ sent; retrying a delivered message is refused.
        node.ingest_core(&peer, CoreMsg::MsgAck { msg_id: mid })
            .unwrap();
        assert_eq!(node.dm_delivery(&rec(), &map), "sent");
        assert!(node.dm_retry(&peer, &mid).is_err());
    }

    #[test]
    fn la_marque_de_lecture_avance_mais_ne_recule_jamais() {
        let (node, peer, lamport, _rx) = node_with_incoming_dm();
        assert_eq!(node.dm_read_lamport(&peer).unwrap(), 0);

        // Une marque plus récente avance.
        node.dm_mark_read(&peer, lamport).unwrap();
        assert_eq!(node.dm_read_lamport(&peer).unwrap(), lamport);
        node.dm_mark_read(&peer, lamport + 10).unwrap();
        assert_eq!(node.dm_read_lamport(&peer).unwrap(), lamport + 10);

        // Une marque plus ancienne est ignorée en silence : pas d'erreur, c'est
        // simplement un appareil du compte resté en arrière.
        node.dm_mark_read(&peer, lamport).unwrap();
        assert_eq!(node.dm_read_lamport(&peer).unwrap(), lamport + 10);

        // Une marque identique ne change rien (re-marquage au refocus).
        node.dm_mark_read(&peer, lamport + 10).unwrap();
        assert_eq!(node.dm_read_lamport(&peer).unwrap(), lamport + 10);
    }

    #[test]
    fn deux_appareils_dont_un_en_retard_retiennent_la_marque_la_plus_avancee() {
        // « Lu = lu sur au moins un appareil » (MULTI_DEVICE.md §5) : quel que
        // soit l'ordre d'arrivée, la marque retenue est la plus avancée des
        // deux. L'appareil en retard se place avant le message entrant, donc
        // une régression rendrait la conversation non lue — le symptôme exact
        // que la monotonie protège.
        let a_jour = |base: u64| base + 10;
        let en_retard = |base: u64| base - 4;

        // L'appareil en retard parle en dernier : sa position est ignorée.
        let (node, peer, lamport, _rx) = node_with_incoming_dm();
        node.dm_mark_read(&peer, a_jour(lamport)).unwrap();
        node.dm_mark_read(&peer, en_retard(lamport)).unwrap();
        assert_eq!(node.dm_read_lamport(&peer).unwrap(), a_jour(lamport));
        assert_eq!(node.dm_unread(&peer).unwrap(), 0);

        // Ordre inverse : l'appareil à jour rattrape et l'emporte.
        let (node, peer, lamport, _rx) = node_with_incoming_dm();
        node.dm_mark_read(&peer, en_retard(lamport)).unwrap();
        node.dm_mark_read(&peer, a_jour(lamport)).unwrap();
        assert_eq!(node.dm_read_lamport(&peer).unwrap(), a_jour(lamport));
        assert_eq!(node.dm_unread(&peer).unwrap(), 0);
    }

    #[test]
    fn un_accuse_de_lecture_perime_ne_recule_pas_la_position_du_pair() {
        let (node, peer, _lamport, _rx) = node_with_incoming_dm();
        // Deux messages sortants : le pair pourra accuser l'un puis l'autre.
        let premier = crate::hex::decode::<16>(&node.dm_send(&peer, "un", None).unwrap()).unwrap();
        let second = crate::hex::decode::<16>(&node.dm_send(&peer, "deux", None).unwrap()).unwrap();
        let position = |id: &[u8; 16]| {
            node.dm_history(&peer, u64::MAX, 10)
                .unwrap()
                .into_iter()
                .find(|m| m.msg_id == *id)
                .unwrap()
                .lamport
        };
        let ingest_receipt = |up_to: [u8; 16], msg_id: [u8; 16], lamport: u64| {
            let rr = MsgBody::ReadReceipt { up_to };
            node.ingest_core(
                &peer,
                CoreMsg::DirectMsg {
                    msg_id,
                    lamport,
                    sent_ms: 4_000,
                    kind: rr.kind(),
                    body: rr.encode_body(),
                },
            )
            .unwrap();
        };

        ingest_receipt(second, [21; 16], position(&second) + 1);
        assert_eq!(
            node.dm_peer_read_lamport(&peer).unwrap(),
            Some(position(&second))
        );

        // Le second appareil du pair, en retard, accuse un message plus ancien :
        // sans monotonie, l'expéditeur verrait « deux » redevenir non lu.
        ingest_receipt(premier, [22; 16], position(&second) + 2);
        assert_eq!(
            node.dm_peer_read_lamport(&peer).unwrap(),
            Some(position(&second))
        );
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
