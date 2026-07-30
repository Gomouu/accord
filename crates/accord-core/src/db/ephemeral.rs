//! Disappearing messages (Lot E2): per-conversation TTL, honoured LOCALLY.
//!
//! The timer only deletes rows from *this* device's encrypted store — there
//! is no wire negotiation and no control message (a bilaterally negotiated
//! variant would be a future, separate wire extension). `scope` is the peer
//! public key (32 bytes) for a DM or the group id (16 bytes) for a group.

use super::{sql_placeholders, Db, IN_CHUNK};
use crate::error::CoreError;
use rusqlite::params;

impl Db {
    /// Sets or clears the disappearing-message TTL of a conversation.
    /// `None` disables the timer (row removed).
    pub fn set_conversation_ttl(
        &self,
        scope: &[u8],
        ttl_secs: Option<u64>,
    ) -> Result<(), CoreError> {
        match ttl_secs {
            None => {
                self.conn().execute(
                    "DELETE FROM conversation_ephemeral WHERE scope = ?1",
                    [scope],
                )?;
            }
            Some(ttl) => {
                self.conn().execute(
                    "INSERT INTO conversation_ephemeral (scope, ttl_secs) VALUES (?1, ?2)
                     ON CONFLICT(scope) DO UPDATE SET ttl_secs = excluded.ttl_secs",
                    params![scope, ttl.min(i64::MAX as u64) as i64],
                )?;
            }
        }
        Ok(())
    }

    /// TTL of a conversation, `None` when disabled.
    pub fn conversation_ttl(&self, scope: &[u8]) -> Result<Option<u64>, CoreError> {
        let mut stmt = self
            .conn()
            .prepare("SELECT ttl_secs FROM conversation_ephemeral WHERE scope = ?1")?;
        let mut rows = stmt.query([scope])?;
        match rows.next()? {
            Some(row) => Ok(Some(row.get::<_, i64>(0)?.max(0) as u64)),
            None => Ok(None),
        }
    }

    /// Every conversation with an active timer.
    pub fn conversation_ttls(&self) -> Result<Vec<(Vec<u8>, u64)>, CoreError> {
        let mut stmt = self
            .conn()
            .prepare("SELECT scope, ttl_secs FROM conversation_ephemeral")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, i64>(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows
            .into_iter()
            .map(|(scope, ttl)| (scope, ttl.max(0) as u64))
            .collect())
    }

    /// Deletes expired messages of every timed conversation. Bounded work:
    /// at most `max_per_scope` messages per conversation per call (the next
    /// pass finishes the backlog). Attachments, reactions, pins, mention
    /// inbox entries and search-index tokens of a purged message are removed
    /// with it — nothing derived from the content may outlive it. Returns
    /// the number of deleted messages.
    pub fn purge_ephemeral(&self, now_ms: u64, max_per_scope: usize) -> Result<u64, CoreError> {
        let mut total = 0u64;
        for (scope, ttl_secs) in self.conversation_ttls()? {
            let cutoff = now_ms.saturating_sub(ttl_secs.saturating_mul(1000));
            let cutoff = cutoff.min(i64::MAX as u64) as i64;
            let ids = match scope.len() {
                32 => self.expired_ids(
                    "SELECT msg_id FROM dm_messages
                     WHERE peer = ?1 AND sent_ms < ?2 LIMIT ?3",
                    &scope,
                    cutoff,
                    max_per_scope,
                )?,
                16 => self.expired_ids(
                    "SELECT msg_id FROM group_messages
                     WHERE group_id = ?1 AND sent_ms < ?2 LIMIT ?3",
                    &scope,
                    cutoff,
                    max_per_scope,
                )?,
                _ => continue,
            };
            total += self.purge_messages(&ids)?;
        }
        Ok(total)
    }

    /// Message ids matching an expiry query (shared DM/group shape).
    fn expired_ids(
        &self,
        sql: &str,
        scope: &[u8],
        cutoff_ms: i64,
        limit: usize,
    ) -> Result<Vec<[u8; 16]>, CoreError> {
        let mut stmt = self.conn().prepare(sql)?;
        let rows = stmt
            .query_map(params![scope, cutoff_ms, limit as i64], |r| {
                r.get::<_, Vec<u8>>(0)
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows.into_iter().filter_map(|v| v.try_into().ok()).collect())
    }

    /// Hard-deletes messages and every derived trace, by [`IN_CHUNK`] batches.
    ///
    /// 🔒 Atomique par lot. Sept suppressions non transactionnelles laissaient,
    /// sur une erreur au milieu, un message vivant dont les réactions, épingles,
    /// pièces jointes et jetons de recherche avaient déjà disparu — un message
    /// mutilé plutôt qu'un message effacé.
    fn purge_messages(&self, ids: &[[u8; 16]]) -> Result<u64, CoreError> {
        if ids.is_empty() {
            return Ok(0);
        }
        let tx = self.conn().unchecked_transaction()?;
        let mut deleted = 0u64;
        for chunk in ids.chunks(IN_CHUNK) {
            let marks = sql_placeholders(chunk.len());
            let args = || rusqlite::params_from_iter(chunk.iter().map(|id| id.as_slice()));
            for table in [
                "msg_attachments",
                "reactions",
                "dm_pins",
                "mentions",
                "search_index",
                "dm_messages",
                "group_messages",
            ] {
                let n = tx.execute(
                    &format!("DELETE FROM {table} WHERE msg_id IN ({marks})"),
                    args(),
                )?;
                // Seules les deux tables de messages comptent des suppressions :
                // les traces dérivées sont un effet de bord, pas un message.
                if matches!(table, "dm_messages" | "group_messages") {
                    deleted += n as u64;
                }
            }
        }
        tx.commit()?;
        Ok(deleted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::DmRecord;

    fn db() -> Db {
        Db::open_in_memory(&[1; 32]).unwrap()
    }

    fn dm(peer: [u8; 32], id: u8, sent_ms: u64) -> DmRecord {
        DmRecord {
            msg_id: [id; 16],
            peer,
            author: peer,
            lamport: id as u64,
            sent_ms,
            kind: 0,
            body: b"corps".to_vec(),
            acked: false,
            deleted: false,
            edited: None,
        }
    }

    #[test]
    fn ttl_set_get_update_and_clear() {
        let db = db();
        let peer = [9u8; 32];
        assert_eq!(db.conversation_ttl(&peer).unwrap(), None);
        db.set_conversation_ttl(&peer, Some(3600)).unwrap();
        assert_eq!(db.conversation_ttl(&peer).unwrap(), Some(3600));
        db.set_conversation_ttl(&peer, Some(60)).unwrap();
        assert_eq!(db.conversation_ttl(&peer).unwrap(), Some(60));
        db.set_conversation_ttl(&peer, None).unwrap();
        assert_eq!(db.conversation_ttl(&peer).unwrap(), None);
        assert!(db.conversation_ttls().unwrap().is_empty());
    }

    #[test]
    fn purge_deletes_old_dm_messages_and_derived_traces_keeps_recent() {
        let db = db();
        let peer = [9u8; 32];
        let now = 10_000_000u64;
        db.set_conversation_ttl(&peer, Some(3600)).unwrap();
        // Old message (beyond TTL) with every derived trace attached.
        db.insert_dm(&dm(peer, 1, now - 3_601_000)).unwrap();
        db.dm_pin(&peer, &[1; 16]).unwrap();
        db.set_reaction(&[1; 16], &peer, "👍", true).unwrap();
        db.index_tokens(&[1; 16], &[[7; 32]]).unwrap();
        // Recent message: must survive.
        db.insert_dm(&dm(peer, 2, now - 10_000)).unwrap();
        assert_eq!(db.purge_ephemeral(now, 100).unwrap(), 1);
        assert!(db.dm_message(&[1; 16]).unwrap().is_none());
        assert!(db.dm_message(&[2; 16]).unwrap().is_some());
        assert!(db.dm_pins(&peer).unwrap().is_empty());
        assert!(db.reactions(&[1; 16]).unwrap().is_empty());
        assert!(db.search_tokens(&[[7; 32]]).unwrap().is_empty());
        // Idempotent: nothing left to purge.
        assert_eq!(db.purge_ephemeral(now, 100).unwrap(), 0);
    }

    #[test]
    fn purge_deletes_old_group_messages_and_attachments() {
        let db = db();
        let gid = [4u8; 16];
        db.set_conversation_ttl(&gid, Some(3600)).unwrap();
        let old = crate::db::GroupMsgRecord {
            msg_id: [1; 16],
            group_id: gid,
            channel_id: [5; 16],
            author: [9; 32],
            lamport: 1,
            sent_ms: 0,
            kind: 0,
            body: b"corps".to_vec(),
            deleted: false,
            edited: None,
        };
        db.insert_group_msg(&old).unwrap();
        db.put_msg_attachments(
            &[1; 16],
            &[accord_proto::core_msg::FileRef {
                merkle_root: [8; 32],
                name: "piece.bin".into(),
                size: 4,
                mime: "application/octet-stream".into(),
            }],
        )
        .unwrap();
        let recent = crate::db::GroupMsgRecord {
            msg_id: [2; 16],
            sent_ms: 9_000_000,
            lamport: 2,
            ..old.clone()
        };
        db.insert_group_msg(&recent).unwrap();
        assert_eq!(db.purge_ephemeral(10_000_000, 100).unwrap(), 1);
        assert!(db.group_msg(&[1; 16]).unwrap().is_none());
        assert!(db.group_msg(&[2; 16]).unwrap().is_some());
        assert!(db.msg_attachments(&[1; 16]).unwrap().is_empty());
    }

    #[test]
    fn purge_without_ttl_deletes_nothing() {
        let db = db();
        let peer = [9u8; 32];
        db.insert_dm(&dm(peer, 1, 0)).unwrap();
        assert_eq!(db.purge_ephemeral(u64::MAX, 100).unwrap(), 0);
        assert!(db.dm_message(&[1; 16]).unwrap().is_some());
    }

    #[test]
    fn purge_is_bounded_per_scope_and_finishes_next_pass() {
        let db = db();
        let peer = [9u8; 32];
        db.set_conversation_ttl(&peer, Some(1)).unwrap();
        for i in 0..5u8 {
            db.insert_dm(&dm(peer, i + 1, 0)).unwrap();
        }
        assert_eq!(db.purge_ephemeral(10_000, 2).unwrap(), 2);
        assert_eq!(db.purge_ephemeral(10_000, 100).unwrap(), 3);
    }
}
