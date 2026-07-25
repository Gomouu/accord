//! Local storage statistics (Lot E3, privacy dashboard): read-only counts
//! and sizes of what this device stores — everything lives in the encrypted
//! SQLCipher database or the local blob store, nothing on any server.

use super::Db;
use crate::db::ContactState;
use crate::error::CoreError;

/// Read-only snapshot of the local store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageStats {
    /// Confirmed friends.
    pub friends: u64,
    /// Direct messages kept locally.
    pub dm_messages: u64,
    /// Groups joined (visible as servers).
    pub groups: u64,
    /// Group messages kept locally.
    pub group_messages: u64,
    /// File entries in the local store (sent or fetched attachments, media).
    pub files: u64,
    /// Declared total size of those files, in bytes.
    pub file_bytes: u64,
    /// Pinned direct messages.
    pub pins: u64,
    /// Size of the encrypted database file on disk, `None` for an in-memory
    /// database.
    pub db_bytes: Option<u64>,
}

impl Db {
    /// Collects [`StorageStats`]. Counting queries only — never mutates.
    pub fn storage_stats(&self) -> Result<StorageStats, CoreError> {
        let count = |sql: &str| -> Result<u64, CoreError> {
            Ok(self
                .conn()
                .query_row(sql, [], |r| r.get::<_, i64>(0))?
                .max(0) as u64)
        };
        let friends = self.conn().query_row(
            "SELECT count(*) FROM contacts WHERE state = ?1",
            [ContactState::Friend as u8],
            |r| r.get::<_, i64>(0),
        )?;
        let joined = self.conn().query_row(
            "SELECT count(*) FROM group_membership_local WHERE state = 2",
            [],
            |r| r.get::<_, i64>(0),
        )?;
        // The main database file path comes from SQLite itself (empty for an
        // in-memory database) — no plumbing of host paths into the core.
        let db_path: String = self.conn().query_row(
            "SELECT file FROM pragma_database_list WHERE name = 'main'",
            [],
            |r| r.get(0),
        )?;
        let db_bytes = if db_path.is_empty() {
            None
        } else {
            std::fs::metadata(&db_path).ok().map(|m| m.len())
        };
        Ok(StorageStats {
            friends: friends.max(0) as u64,
            dm_messages: count("SELECT count(*) FROM dm_messages")?,
            groups: joined.max(0) as u64,
            group_messages: count("SELECT count(*) FROM group_messages")?,
            files: count("SELECT count(*) FROM files")?,
            file_bytes: count("SELECT COALESCE(SUM(size), 0) FROM files")?,
            pins: count("SELECT count(*) FROM dm_pins")?,
            db_bytes,
        })
    }

    /// Nombre de lignes de l'index de recherche aveugle (un couple
    /// `(jeton, message)` par mot distinct de chaque message).
    ///
    /// Hors [`StorageStats`] à dessein : c'est une mesure d'ingénierie, pas un
    /// chiffre à montrer dans le tableau de bord de confidentialité. À gros
    /// historique, cette table pèse plus lourd que les messages eux-mêmes —
    /// sans ce compteur, la taille de la base n'est pas explicable.
    pub fn search_index_rows(&self) -> Result<u64, CoreError> {
        Ok(self
            .conn()
            .query_row("SELECT count(*) FROM search_index", [], |r| {
                r.get::<_, i64>(0)
            })?
            .max(0) as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Contact, DmRecord};

    #[test]
    fn stats_reflect_a_seeded_database() {
        let db = Db::open_in_memory(&[1; 32]).unwrap();
        let empty = db.storage_stats().unwrap();
        assert_eq!(empty.friends, 0);
        assert_eq!(empty.dm_messages, 0);
        assert_eq!(empty.db_bytes, None);

        db.upsert_contact(&Contact {
            node_id: [1; 32],
            pubkey: [1; 32],
            display_name: "ami".into(),
            state: ContactState::Friend,
            added_ms: 0,
            last_seen_ms: 0,
            verified_at: None,
            verified_pubkey: None,
        })
        .unwrap();
        db.upsert_contact(&Contact {
            node_id: [2; 32],
            pubkey: [2; 32],
            display_name: "en attente".into(),
            state: ContactState::PendingIn,
            added_ms: 0,
            last_seen_ms: 0,
            verified_at: None,
            verified_pubkey: None,
        })
        .unwrap();
        for i in 0..3u8 {
            db.insert_dm(&DmRecord {
                msg_id: [i; 16],
                peer: [1; 32],
                author: [1; 32],
                lamport: i as u64,
                sent_ms: 0,
                kind: 0,
                body: vec![],
                acked: false,
                deleted: false,
                edited: None,
            })
            .unwrap();
        }
        db.dm_pin(&[1; 32], &[0; 16]).unwrap();

        let stats = db.storage_stats().unwrap();
        assert_eq!(stats.friends, 1);
        assert_eq!(stats.dm_messages, 3);
        assert_eq!(stats.pins, 1);
        assert_eq!(stats.groups, 0);
    }

    #[test]
    fn db_bytes_is_reported_for_an_on_disk_database() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("core.db");
        let db = Db::open(&path, &[7; 32]).unwrap();
        let stats = db.storage_stats().unwrap();
        assert!(stats.db_bytes.unwrap_or(0) > 0);
    }
}
