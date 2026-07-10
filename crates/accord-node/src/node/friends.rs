//! Amis : contacts, demandes, réponses, retrait, blocage et statut de
//! présence local (bloc `impl Node` du domaine `friends.*`).

use accord_core::db::Contact;
use accord_core::{friends, presence};
use accord_crypto::FriendCode;
use accord_proto::core_msg::CoreMsg;
use serde_json::json;

use crate::error::NodeError;
use crate::hex;
use crate::outbound::Outbound;

use super::{now_ms, Node};

impl Node {
    /// Liste des contacts.
    pub fn contacts(&self) -> Result<Vec<Contact>, NodeError> {
        self.with_db(|db| Ok(db.contacts()?))
    }

    /// Prépare et route une demande d'ami vers une clé publique.
    pub fn friend_request(
        &self,
        peer_pubkey: &[u8; 32],
        display_name: &str,
    ) -> Result<(), NodeError> {
        let action = self.with_db(|db| {
            Ok(friends::request_friend(
                db,
                peer_pubkey,
                display_name,
                now_ms(),
            )?)
        })?;
        // Nom annoncé au pair : le pseudo de profil s'il est défini, sinon le
        // code ami (D-027).
        let my_name = match self.profile_name()? {
            Some(name) => name,
            None => FriendCode::of_pubkey(&self.identity.public_key()).display(),
        };
        let msg = match action {
            friends::OutgoingAction::SendRequest => CoreMsg::FriendRequest {
                display_name: my_name,
                message: String::new(),
                verify_phrase: None,
            },
            friends::OutgoingAction::SendAccept => CoreMsg::FriendResponse { accepted: true },
        };
        self.outbound.send(Outbound::Core {
            to: *peer_pubkey,
            msg: Box::new(msg),
        });
        // Demandes croisées : amitié établie, annoncer aussi notre pseudo.
        if action == friends::OutgoingAction::SendAccept {
            self.announce_profile_to(peer_pubkey)?;
        }
        Ok(())
    }

    /// Répond à une demande entrante. Sur acceptation, annonce aussi notre
    /// pseudo au nouvel ami (D-027).
    pub fn friend_respond(&self, peer_pubkey: &[u8; 32], accept: bool) -> Result<(), NodeError> {
        self.with_db(|db| Ok(friends::respond_friend(db, peer_pubkey, accept)?))?;
        self.outbound.send(Outbound::Core {
            to: *peer_pubkey,
            msg: Box::new(CoreMsg::FriendResponse { accepted: accept }),
        });
        if accept {
            self.announce_profile_to(peer_pubkey)?;
        }
        Ok(())
    }

    /// Removes an established friendship (distinct from a block): the contact
    /// disappears locally, DM history is kept, and the peer is notified
    /// best-effort with a `FriendRemove` wire message (session-authenticated,
    /// never queued offline). Emits `event.friend_removed` so every local UI
    /// client refreshes.
    pub fn friend_remove(&self, peer_pubkey: &[u8; 32]) -> Result<(), NodeError> {
        self.with_db(|db| Ok(friends::remove_friend(db, peer_pubkey)?))?;
        self.outbound.send(Outbound::Core {
            to: *peer_pubkey,
            msg: Box::new(CoreMsg::FriendRemove),
        });
        self.emit(
            "event.friend_removed",
            json!({ "peer": hex::encode(peer_pubkey) }),
        );
        Ok(())
    }

    /// Sets the local presence status (`friends.set_status`): persists it in
    /// the meta table then announces it to all confirmed friends (invisible
    /// is announced as plain offline). `custom`: `None` keeps the current
    /// text, an empty string clears it.
    pub fn set_own_presence(
        &self,
        status: presence::OwnStatus,
        custom: Option<&str>,
    ) -> Result<(), NodeError> {
        self.with_db(|db| Ok(presence::set_own_presence(db, status, custom)?))?;
        self.broadcast_presence(true)
    }

    /// Persisted local presence status (`friends.get_status`); defaults to
    /// online without custom text.
    pub fn own_presence(&self) -> Result<(presence::OwnStatus, Option<String>), NodeError> {
        self.with_db(|db| Ok(presence::own_presence(db)?))
    }

    /// Bloque un pair.
    pub fn friend_block(&self, peer_pubkey: &[u8; 32]) -> Result<(), NodeError> {
        self.with_db(|db| Ok(friends::block(db, peer_pubkey, now_ms())?))
    }

    /// Débloque un pair.
    pub fn friend_unblock(&self, peer_pubkey: &[u8; 32]) -> Result<(), NodeError> {
        self.with_db(|db| Ok(friends::unblock(db, peer_pubkey)?))
    }

    /// Clés publiques des amis confirmés (présence, relève des boîtes).
    pub fn friend_pubkeys(&self) -> Result<Vec<[u8; 32]>, NodeError> {
        self.with_db(|db| {
            Ok(db
                .contacts()?
                .into_iter()
                .filter(|c| c.state == accord_core::db::ContactState::Friend)
                .map(|c| c.pubkey)
                .collect())
        })
    }
}

#[cfg(test)]
mod tests {
    use accord_core::db::Db;
    use accord_core::presence::OwnStatus;
    use accord_crypto::Identity;
    use tokio::sync::mpsc;

    use super::*;
    use crate::outbound::OutboundSink;

    /// Node wired to an outbound channel, with one established friend.
    fn node_with_friend() -> (Node, [u8; 32], mpsc::Receiver<Outbound>) {
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
        while rx.try_recv().is_ok() {}
        (node, peer.public_key(), rx)
    }

    /// Next `CoreMsg` pushed on the outbound channel, with its recipient.
    fn next_core(rx: &mut mpsc::Receiver<Outbound>) -> Option<([u8; 32], CoreMsg)> {
        while let Ok(action) = rx.try_recv() {
            if let Outbound::Core { to, msg } = action {
                return Some((to, *msg));
            }
        }
        None
    }

    #[test]
    fn friend_remove_drops_contact_and_notifies_peer() {
        let (node, peer, mut rx) = node_with_friend();
        node.friend_remove(&peer).unwrap();
        assert!(node.contacts().unwrap().is_empty());
        let (to, msg) = next_core(&mut rx).expect("notification attendue");
        assert_eq!(to, peer);
        assert_eq!(msg, CoreMsg::FriendRemove);
        // Not a friend anymore: a second removal is refused.
        assert!(node.friend_remove(&peer).is_err());
    }

    #[test]
    fn friend_remove_keeps_dm_history() {
        let (node, peer, _rx) = node_with_friend();
        node.dm_send(&peer, "avant retrait", None).unwrap();
        node.friend_remove(&peer).unwrap();
        assert_eq!(node.dm_history(&peer, u64::MAX, 10).unwrap().len(), 1);
        // Sending to a removed friend fails exactly like any non-friend.
        assert!(node.dm_send(&peer, "après retrait", None).is_err());
    }

    #[test]
    fn ingested_friend_remove_drops_friendship_only() {
        let (node, peer, _rx) = node_with_friend();
        let replies = node.ingest_core(&peer, CoreMsg::FriendRemove).unwrap();
        assert!(replies.is_empty());
        assert!(node.contacts().unwrap().is_empty());
        // Replay: idempotent, still no reply.
        assert!(node
            .ingest_core(&peer, CoreMsg::FriendRemove)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn own_presence_persists_and_broadcasts_status() {
        let (node, peer, mut rx) = node_with_friend();
        node.set_own_presence(OwnStatus::Dnd, Some("focus"))
            .unwrap();
        assert_eq!(
            node.own_presence().unwrap(),
            (OwnStatus::Dnd, Some("focus".into()))
        );
        let (to, msg) = next_core(&mut rx).expect("annonce attendue");
        assert_eq!(to, peer);
        assert_eq!(
            msg,
            CoreMsg::Presence {
                status: 2,
                custom: Some("focus".into())
            }
        );
    }

    #[test]
    fn invisible_broadcasts_offline_without_custom_text() {
        let (node, _peer, mut rx) = node_with_friend();
        node.set_own_presence(OwnStatus::Invisible, Some("caché"))
            .unwrap();
        let (_, msg) = next_core(&mut rx).expect("annonce attendue");
        assert_eq!(
            msg,
            CoreMsg::Presence {
                status: 3,
                custom: None
            }
        );
        // The status (and its text) stay persisted locally.
        assert_eq!(
            node.own_presence().unwrap(),
            (OwnStatus::Invisible, Some("caché".into()))
        );
        // A clean-shutdown broadcast stays offline too.
        node.broadcast_presence(false).unwrap();
        let (_, msg) = next_core(&mut rx).expect("annonce d'arrêt attendue");
        assert!(matches!(msg, CoreMsg::Presence { status: 3, .. }));
    }

    #[test]
    fn rich_presence_from_friend_is_tracked_and_cleared() {
        let (node, peer, _rx) = node_with_friend();
        node.ingest_core(
            &peer,
            CoreMsg::Presence {
                status: 1,
                custom: Some("afk".into()),
            },
        )
        .unwrap();
        assert_eq!(node.peer_presence(&peer), (1, Some("afk".into())));
        assert!(node.is_online(&peer));
        // Backward compatibility: a bare offline announcement clears all.
        node.ingest_core(
            &peer,
            CoreMsg::Presence {
                status: 3,
                custom: None,
            },
        )
        .unwrap();
        assert_eq!(node.peer_presence(&peer), (3, None));
        assert!(!node.is_online(&peer));
    }

    #[test]
    fn plain_reachability_does_not_override_explicit_status() {
        let (node, peer, _rx) = node_with_friend();
        node.ingest_core(
            &peer,
            CoreMsg::Presence {
                status: 2,
                custom: None,
            },
        )
        .unwrap();
        // Any later message keeps the explicit do-not-disturb status.
        node.ingest_core(
            &peer,
            CoreMsg::Profile {
                display_name: "Pair".into(),
                bio: String::new(),
                avatar: None,
                banner: None,
            },
        )
        .unwrap();
        assert_eq!(node.peer_presence(&peer), (2, None));
    }

    #[test]
    fn rich_presence_from_non_friend_only_tracks_reachability() {
        let (node, _peer, _rx) = node_with_friend();
        let stranger = Identity::generate_with_pow_bits(1);
        node.ingest_core(
            &stranger.public_key(),
            CoreMsg::Presence {
                status: 2,
                custom: Some("spam".into()),
            },
        )
        .unwrap();
        // Reachable, but no rich status is stored for strangers.
        assert!(node.is_online(&stranger.public_key()));
        assert_eq!(node.peer_presence(&stranger.public_key()), (0, None));
    }
}
