//! Mentions : détection passive à l'ingestion, boîte de mentions et
//! comptage par conversation (bloc `impl Node` du domaine `mentions.*`).
//!
//! Purement local : rien n'est émis sur le réseau. À la réception d'un message
//! texte (DM ou salon), le nœud compare le texte à ses propres identifiants
//! (pseudo, code ami), aux jetons `@everyone`/`@here` et aux rôles qu'il
//! détient dans le groupe ([`accord_core::mentions`]) ; une entrée dédoublonnée
//! est stockée si l'utilisateur local est visé.

use accord_core::db::{MentionEntry, MentionScope};
use accord_core::group::{self, GroupState};
use accord_core::mentions::{self, MentionSelf};
use accord_crypto::FriendCode;
use accord_proto::core_msg::MsgBody;

use crate::error::NodeError;

use super::Node;

/// Noms des rôles détenus par `who` dans un groupe (ordre du `BTreeSet`).
fn role_names_of(state: &GroupState, who: &[u8; 32]) -> Vec<String> {
    match state.members.get(who) {
        Some(member) => member
            .roles
            .iter()
            .filter_map(|rid| state.roles.get(rid).map(|r| r.name.clone()))
            .collect(),
        None => Vec::new(),
    }
}

impl Node {
    /// Code ami affiché de l'identité locale.
    fn self_friend_code(&self) -> String {
        FriendCode::of_pubkey(&self.public_key()).display()
    }

    /// Extrait le texte d'un corps encodé si c'est un message texte.
    fn text_of_body(kind: u8, body: &[u8]) -> Option<String> {
        match MsgBody::decode_body(kind, body) {
            Ok(MsgBody::Text { text, .. }) => Some(text),
            _ => None,
        }
    }

    /// Détecte et enregistre (dédoublonné) une mention pour un DM entrant tout
    /// juste stocké. Rend `true` si une **nouvelle** entrée a été créée.
    pub(super) fn record_dm_mention(
        &self,
        peer: &[u8; 32],
        msg_id: &[u8; 16],
        sent_ms: u64,
        lamport: u64,
        kind: u8,
        body: &[u8],
    ) -> Result<bool, NodeError> {
        let Some(text) = Self::text_of_body(kind, body) else {
            return Ok(false);
        };
        let name = self.profile_name()?;
        let code = self.self_friend_code();
        let me = MentionSelf {
            name: name.as_deref(),
            code: &code,
            roles: &[],
        };
        if !mentions::detect(&text, &me) {
            return Ok(false);
        }
        let entry = MentionEntry {
            msg_id: *msg_id,
            scope: MentionScope::Dm,
            conv_a: peer.to_vec(),
            conv_b: None,
            author: *peer,
            ts_ms: sent_ms,
            lamport,
            snippet: mentions::snippet(&text),
            read: false,
        };
        self.with_db(|db| Ok(db.insert_mention(&entry)?))
    }

    /// Détecte et enregistre (dédoublonné) une mention pour un message de salon
    /// tout juste stocké. Le texte est relu du clair persisté ; les rôles
    /// détenus sont dérivés de l'état matérialisé du groupe.
    pub(super) fn record_group_mention(
        &self,
        group_id: &[u8; 16],
        channel_id: &[u8; 16],
        msg_id: &[u8; 16],
        author: &[u8; 32],
        sent_ms: u64,
        lamport: u64,
    ) -> Result<bool, NodeError> {
        let me_pubkey = self.public_key();
        let (text, roles) = self.with_db(|db| {
            let Some(rec) = db.group_msg(msg_id)? else {
                return Ok((None, Vec::new()));
            };
            let text = Self::text_of_body(rec.kind, &rec.body);
            let state = group::group_state(db, group_id)?;
            Ok((text, role_names_of(&state, &me_pubkey)))
        })?;
        let Some(text) = text else {
            return Ok(false);
        };
        let name = self.profile_name()?;
        let code = self.self_friend_code();
        let me = MentionSelf {
            name: name.as_deref(),
            code: &code,
            roles: &roles,
        };
        if !mentions::detect(&text, &me) {
            return Ok(false);
        }
        let entry = MentionEntry {
            msg_id: *msg_id,
            scope: MentionScope::Group,
            conv_a: group_id.to_vec(),
            conv_b: Some(*channel_id),
            author: *author,
            ts_ms: sent_ms,
            lamport,
            snippet: mentions::snippet(&text),
            read: false,
        };
        self.with_db(|db| Ok(db.insert_mention(&entry)?))
    }

    /// Boîte de mentions, la plus récente d'abord, bornée à `limit`, avant
    /// l'horloge murale `before` (pagination ; `None` = depuis le présent).
    pub fn mention_inbox(
        &self,
        before: Option<u64>,
        limit: usize,
    ) -> Result<Vec<MentionEntry>, NodeError> {
        self.with_db(|db| Ok(db.mention_inbox(before.unwrap_or(u64::MAX), limit)?))
    }

    /// Marque des mentions comme lues (`None` = toutes) ; rend le nombre affecté.
    pub fn mentions_mark_read(&self, msg_ids: Option<&[[u8; 16]]>) -> Result<usize, NodeError> {
        self.with_db(|db| {
            Ok(match msg_ids {
                Some(ids) => db.mark_mentions_read(ids)?,
                None => db.mark_mentions_read_all()?,
            })
        })
    }

    /// Vrai si le message porte une mention de l'utilisateur local
    /// (annotation `mentions_me` de l'historique).
    pub fn msg_mentions_me(&self, msg_id: &[u8; 16]) -> Result<bool, NodeError> {
        self.with_db(|db| Ok(db.mention_recorded(msg_id)?))
    }

    /// Nombre de mentions non lues d'une conversation directe.
    pub fn dm_mention_count(&self, peer: &[u8; 32]) -> Result<u64, NodeError> {
        self.with_db(|db| Ok(db.count_dm_mentions(peer)?))
    }

    /// Nombre de mentions non lues d'un groupe (tous salons confondus).
    pub fn group_mention_count(&self, group_id: &[u8; 16]) -> Result<u64, NodeError> {
        self.with_db(|db| Ok(db.count_group_mentions(group_id)?))
    }
}

#[cfg(test)]
mod tests {
    use accord_core::db::Db;
    use accord_crypto::Identity;
    use accord_proto::core_msg::{CoreMsg, MsgBody};

    use super::*;
    use crate::outbound::OutboundSink;

    /// Node with one established friend (their `FriendResponse` ingested).
    fn node_with_friend() -> (Node, [u8; 32]) {
        let id = Identity::generate_with_pow_bits(1);
        let db = Db::open_in_memory(&[1u8; 32]).unwrap();
        let (sink, _rx) = OutboundSink::channel(64);
        let node = Node::new(id, db, sink);
        let peer = Identity::generate_with_pow_bits(1);
        node.friend_request(&peer.public_key(), "Pair").unwrap();
        node.ingest_core(
            &peer.public_key(),
            CoreMsg::FriendResponse { accepted: true },
        )
        .unwrap();
        (node, peer.public_key())
    }

    fn text_dm(msg_id: [u8; 16], lamport: u64, text: &str) -> CoreMsg {
        let body = MsgBody::Text {
            text: text.into(),
            reply_to: None,
            attachments: vec![],
        };
        CoreMsg::DirectMsg {
            msg_id,
            lamport,
            sent_ms: 1_000 + lamport,
            kind: body.kind(),
            body: body.encode_body(),
        }
    }

    #[test]
    fn dm_mention_by_name_records_inbox_entry() {
        let (node, peer) = node_with_friend();
        node.profile_set_name("Anna").unwrap();
        node.ingest_core(&peer, text_dm([5; 16], 3, "salut @Anna ça va ?"))
            .unwrap();
        assert!(node.msg_mentions_me(&[5; 16]).unwrap());
        assert_eq!(node.dm_mention_count(&peer).unwrap(), 1);
        let inbox = node.mention_inbox(None, 50).unwrap();
        assert_eq!(inbox.len(), 1);
        assert_eq!(inbox[0].msg_id, [5; 16]);
        assert_eq!(inbox[0].author, peer);
        assert!(!inbox[0].read);

        // A message without a mention leaves no entry.
        node.ingest_core(&peer, text_dm([6; 16], 4, "rien de spécial"))
            .unwrap();
        assert!(!node.msg_mentions_me(&[6; 16]).unwrap());
        assert_eq!(node.dm_mention_count(&peer).unwrap(), 1);

        // mark_read clears the count but keeps the entry in the inbox.
        assert_eq!(node.mentions_mark_read(None).unwrap(), 1);
        assert_eq!(node.dm_mention_count(&peer).unwrap(), 0);
        assert_eq!(node.mention_inbox(None, 50).unwrap().len(), 1);
    }

    #[test]
    fn dm_everyone_triggers_without_a_local_name() {
        let (node, peer) = node_with_friend();
        node.ingest_core(&peer, text_dm([7; 16], 3, "annonce @everyone"))
            .unwrap();
        assert!(node.msg_mentions_me(&[7; 16]).unwrap());
        assert_eq!(node.dm_mention_count(&peer).unwrap(), 1);
    }

    #[test]
    fn contact_note_roundtrips_trims_and_bounds() {
        let (node, peer) = node_with_friend();
        assert_eq!(node.contact_note(&peer).unwrap(), None);
        node.set_contact_note(&peer, "  ami de longue date  ")
            .unwrap();
        assert_eq!(
            node.contact_note(&peer).unwrap(),
            Some("ami de longue date".into())
        );
        // An empty note clears the entry.
        node.set_contact_note(&peer, "   ").unwrap();
        assert_eq!(node.contact_note(&peer).unwrap(), None);
        // Over the 4096-char bound is refused.
        let too_long = "a".repeat(4097);
        assert!(node.set_contact_note(&peer, &too_long).is_err());
    }
}
