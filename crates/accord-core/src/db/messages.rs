//! Historique des messages directs et de groupe : insertion idempotente,
//! éditions, tombstones, réactions, accusés de lecture.

use std::collections::BTreeSet;

use super::{blob, Db};
use crate::error::CoreError;
use accord_proto::core_msg::FileRef;
use rusqlite::{params, ToSql};

/// Colonnes projetées d'un message direct (ordre figé pour [`to_dm_record`]).
const DM_COLS: &str = "msg_id, peer, author, lamport, sent_ms, kind, body, acked, deleted, edited";

/// Colonnes projetées d'un message de groupe (ordre figé pour [`to_group_record`]).
const GROUP_COLS: &str =
    "msg_id, group_id, channel_id, author, lamport, sent_ms, kind, body, deleted, edited";

/// Ligne brute d'un message direct, dans l'ordre de [`DM_COLS`].
type DmRaw = (
    Vec<u8>,
    Vec<u8>,
    Vec<u8>,
    u64,
    u64,
    u8,
    Vec<u8>,
    bool,
    bool,
    Option<Vec<u8>>,
);

/// Ligne brute d'un message de groupe, dans l'ordre de [`GROUP_COLS`].
type GroupRaw = (
    Vec<u8>,
    Vec<u8>,
    Vec<u8>,
    Vec<u8>,
    u64,
    u64,
    u8,
    Vec<u8>,
    bool,
    Option<Vec<u8>>,
);

fn dm_raw(row: &rusqlite::Row) -> rusqlite::Result<DmRaw> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
        row.get(8)?,
        row.get(9)?,
    ))
}

fn group_raw(row: &rusqlite::Row) -> rusqlite::Result<GroupRaw> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
        row.get(8)?,
        row.get(9)?,
    ))
}

fn to_dm_record(r: DmRaw) -> Result<DmRecord, CoreError> {
    Ok(DmRecord {
        msg_id: blob(r.0)?,
        peer: blob(r.1)?,
        author: blob(r.2)?,
        lamport: r.3,
        sent_ms: r.4,
        kind: r.5,
        body: r.6,
        acked: r.7,
        deleted: r.8,
        edited: r.9,
    })
}

fn to_group_record(r: GroupRaw) -> Result<GroupMsgRecord, CoreError> {
    Ok(GroupMsgRecord {
        msg_id: blob(r.0)?,
        group_id: blob(r.1)?,
        channel_id: blob(r.2)?,
        author: blob(r.3)?,
        lamport: r.4,
        sent_ms: r.5,
        kind: r.6,
        body: r.7,
        deleted: r.8,
        edited: r.9,
    })
}

/// Message direct persisté (corps encodé [`accord_proto::core_msg::MsgBody`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DmRecord {
    /// Identifiant unique du message.
    pub msg_id: [u8; 16],
    /// Interlocuteur de la conversation (node_id).
    pub peer: [u8; 32],
    /// Auteur du message (node_id ; nous ou le pair).
    pub author: [u8; 32],
    /// Horloge de Lamport de l'auteur.
    pub lamport: u64,
    /// Horloge murale d'envoi (ms).
    pub sent_ms: u64,
    /// Discriminant du corps.
    pub kind: u8,
    /// Corps encodé.
    pub body: Vec<u8>,
    /// Accusé de réception reçu.
    pub acked: bool,
    /// Tombstone de suppression.
    pub deleted: bool,
    /// Nouveau corps si le message a été édité.
    pub edited: Option<Vec<u8>>,
}

/// Message de groupe persisté (corps déchiffré).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupMsgRecord {
    /// Identifiant unique du message.
    pub msg_id: [u8; 16],
    /// Groupe.
    pub group_id: [u8; 16],
    /// Salon.
    pub channel_id: [u8; 16],
    /// Auteur (clé publique).
    pub author: [u8; 32],
    /// Horloge de Lamport.
    pub lamport: u64,
    /// Horloge murale d'envoi (ms).
    pub sent_ms: u64,
    /// Discriminant du corps.
    pub kind: u8,
    /// Corps encodé (déchiffré).
    pub body: Vec<u8>,
    /// Tombstone de suppression.
    pub deleted: bool,
    /// Nouveau corps si édité.
    pub edited: Option<Vec<u8>>,
}

impl Db {
    // ---- Messages directs ----

    /// Insère un message direct ; rend `false` s'il était déjà connu
    /// (idempotent : les retransmissions ne dupliquent pas).
    pub fn insert_dm(&self, m: &DmRecord) -> Result<bool, CoreError> {
        let n = self.conn().execute(
            "INSERT OR IGNORE INTO dm_messages
               (msg_id, peer, author, lamport, sent_ms, kind, body, acked, deleted, edited)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                m.msg_id, m.peer, m.author, m.lamport, m.sent_ms, m.kind, m.body, m.acked,
                m.deleted, m.edited
            ],
        )?;
        Ok(n > 0)
    }

    /// Exécute une requête projetant [`DM_COLS`] et matérialise les records.
    fn dm_rows(&self, sql: &str, args: &[&dyn ToSql]) -> Result<Vec<DmRecord>, CoreError> {
        let mut stmt = self.conn().prepare(sql)?;
        let raws = stmt
            .query_map(args, dm_raw)?
            .collect::<Result<Vec<_>, _>>()?;
        raws.into_iter().map(to_dm_record).collect()
    }

    /// Un message direct par identifiant (contrôles d'auteur/pair avant un
    /// épinglage ou une nouvelle tentative d'envoi).
    pub fn dm_message(&self, msg_id: &[u8; 16]) -> Result<Option<DmRecord>, CoreError> {
        let sql = format!("SELECT {DM_COLS} FROM dm_messages WHERE msg_id = ?1");
        Ok(self
            .dm_rows(&sql, &[&msg_id.as_slice()])?
            .into_iter()
            .next())
    }

    /// Historique d'une conversation, du plus récent au plus ancien,
    /// borné à `limit`, strictement avant `before_lamport` (pagination).
    pub fn dm_history(
        &self,
        peer: &[u8; 32],
        before_lamport: u64,
        limit: usize,
    ) -> Result<Vec<DmRecord>, CoreError> {
        let sql = format!(
            "SELECT {DM_COLS} FROM dm_messages
             WHERE peer = ?1 AND lamport < ?2
             ORDER BY lamport DESC, msg_id DESC LIMIT ?3"
        );
        self.dm_rows(
            &sql,
            &[
                &peer.as_slice(),
                &(before_lamport.min(i64::MAX as u64) as i64),
                &(limit as i64),
            ],
        )
    }

    /// Fenêtre d'historique centrée sur `msg_id` : jusqu'à `limit / 2` messages
    /// plus anciens, la cible, puis jusqu'à `limit / 2` plus récents. Rend
    /// `None` si la cible est inconnue dans cette conversation (fenêtre vide,
    /// `found = false` côté API). L'ordre rendu est celui de [`Db::dm_history`]
    /// (du plus récent au plus ancien) pour un rendu homogène.
    pub fn dm_history_around(
        &self,
        peer: &[u8; 32],
        msg_id: &[u8; 16],
        limit: usize,
    ) -> Result<Option<Vec<DmRecord>>, CoreError> {
        let Some(target) = self.dm_message(msg_id)? else {
            return Ok(None);
        };
        if target.peer != *peer {
            return Ok(None);
        }
        let half = (limit / 2) as i64;
        let tl = target.lamport.min(i64::MAX as u64) as i64;
        // Plus anciens : strictement avant la cible dans l'ordre total
        // (lamport, msg_id), du plus proche au plus lointain.
        let older = self.dm_rows(
            &format!(
                "SELECT {DM_COLS} FROM dm_messages
                 WHERE peer = ?1 AND (lamport < ?2 OR (lamport = ?2 AND msg_id < ?3))
                 ORDER BY lamport DESC, msg_id DESC LIMIT ?4"
            ),
            &[&peer.as_slice(), &tl, &msg_id.as_slice(), &half],
        )?;
        // Plus récents : strictement après la cible, du plus proche au plus loin.
        let mut newer = self.dm_rows(
            &format!(
                "SELECT {DM_COLS} FROM dm_messages
                 WHERE peer = ?1 AND (lamport > ?2 OR (lamport = ?2 AND msg_id > ?3))
                 ORDER BY lamport ASC, msg_id ASC LIMIT ?4"
            ),
            &[&peer.as_slice(), &tl, &msg_id.as_slice(), &half],
        )?;
        // Fenêtre en ordre décroissant : récents (renversés) ‖ cible ‖ anciens.
        newer.reverse();
        let mut window = Vec::with_capacity(newer.len() + 1 + older.len());
        window.append(&mut newer);
        window.push(target);
        window.extend(older);
        Ok(Some(window))
    }

    /// Messages directs les plus récents, toutes conversations confondues,
    /// bornés à `cap` (candidats de recherche filtrée sans mot-clé).
    pub fn dm_recent(&self, cap: usize) -> Result<Vec<DmRecord>, CoreError> {
        let sql = format!(
            "SELECT {DM_COLS} FROM dm_messages
             ORDER BY lamport DESC, msg_id DESC LIMIT ?1"
        );
        self.dm_rows(&sql, &[&(cap as i64)])
    }

    /// Marque un message direct comme acquitté.
    pub fn ack_dm(&self, msg_id: &[u8; 16]) -> Result<(), CoreError> {
        self.conn().execute(
            "UPDATE dm_messages SET acked = 1 WHERE msg_id = ?1",
            [msg_id.as_slice()],
        )?;
        Ok(())
    }

    /// Applique une édition : seul l'auteur d'origine peut éditer.
    pub fn edit_dm(
        &self,
        msg_id: &[u8; 16],
        author: &[u8; 32],
        new_body: &[u8],
    ) -> Result<bool, CoreError> {
        let n = self.conn().execute(
            "UPDATE dm_messages SET edited = ?3
             WHERE msg_id = ?1 AND author = ?2 AND deleted = 0",
            params![msg_id, author, new_body],
        )?;
        Ok(n > 0)
    }

    /// Applique une suppression (tombstone) : le corps est effacé, ainsi que
    /// les pièces jointes associées.
    pub fn delete_dm(&self, msg_id: &[u8; 16], author: &[u8; 32]) -> Result<bool, CoreError> {
        let n = self.conn().execute(
            "UPDATE dm_messages SET deleted = 1, body = x'', edited = NULL
             WHERE msg_id = ?1 AND author = ?2",
            params![msg_id, author],
        )?;
        if n > 0 {
            self.delete_msg_attachments(msg_id)?;
            // Un message effacé ne reste jamais épinglé (vue locale du DM).
            self.conn()
                .execute("DELETE FROM dm_pins WHERE msg_id = ?1", [msg_id.as_slice()])?;
            // Ni dans la boîte de mentions (l'extrait ne doit pas survivre).
            self.delete_mention(msg_id)?;
        }
        Ok(n > 0)
    }

    // ---- Épingles de conversation directe (vue locale, sans op filaire) ----

    /// Épingle un message direct (idempotent). L'appartenance du message à la
    /// conversation est vérifiée en amont ([`crate::Node`]).
    pub fn dm_pin(&self, peer: &[u8; 32], msg_id: &[u8; 16]) -> Result<(), CoreError> {
        self.conn().execute(
            "INSERT OR IGNORE INTO dm_pins (peer, msg_id) VALUES (?1, ?2)",
            params![peer, msg_id],
        )?;
        Ok(())
    }

    /// Retire l'épingle d'un message direct (sans effet si absente).
    pub fn dm_unpin(&self, peer: &[u8; 32], msg_id: &[u8; 16]) -> Result<(), CoreError> {
        self.conn().execute(
            "DELETE FROM dm_pins WHERE peer = ?1 AND msg_id = ?2",
            params![peer, msg_id],
        )?;
        Ok(())
    }

    /// Messages épinglés d'une conversation, dans l'ordre des identifiants.
    pub fn dm_pins(&self, peer: &[u8; 32]) -> Result<Vec<[u8; 16]>, CoreError> {
        let mut stmt = self
            .conn()
            .prepare("SELECT msg_id FROM dm_pins WHERE peer = ?1 ORDER BY msg_id ASC")?;
        let raws = stmt
            .query_map([peer.as_slice()], |row| row.get::<_, Vec<u8>>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        raws.into_iter().map(blob).collect()
    }

    /// Ensemble des messages épinglés d'une conversation (annotation d'historique).
    pub fn dm_pinned_set(&self, peer: &[u8; 32]) -> Result<BTreeSet<[u8; 16]>, CoreError> {
        Ok(self.dm_pins(peer)?.into_iter().collect())
    }

    // ---- Pièces jointes (communes DM/groupe, indexées par msg_id) ----

    /// Persiste les pièces jointes d'un message dans l'ordre d'apparition
    /// (idempotent : les retransmissions ne dupliquent pas).
    pub fn put_msg_attachments(
        &self,
        msg_id: &[u8; 16],
        attachments: &[FileRef],
    ) -> Result<(), CoreError> {
        for (position, a) in attachments.iter().enumerate() {
            self.conn().execute(
                "INSERT OR IGNORE INTO msg_attachments
                   (msg_id, position, merkle_root, name, size, mime)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    msg_id,
                    position as i64,
                    a.merkle_root,
                    a.name,
                    a.size,
                    a.mime
                ],
            )?;
        }
        Ok(())
    }

    /// Pièces jointes d'un message, dans l'ordre d'apparition.
    pub fn msg_attachments(&self, msg_id: &[u8; 16]) -> Result<Vec<FileRef>, CoreError> {
        let mut stmt = self.conn().prepare(
            "SELECT merkle_root, name, size, mime FROM msg_attachments
             WHERE msg_id = ?1 ORDER BY position ASC",
        )?;
        let raws = stmt
            .query_map([msg_id.as_slice()], |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, u64>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        raws.into_iter()
            .map(|(root, name, size, mime)| {
                Ok(FileRef {
                    merkle_root: blob(root)?,
                    name,
                    size,
                    mime,
                })
            })
            .collect()
    }

    /// Efface les pièces jointes d'un message supprimé.
    fn delete_msg_attachments(&self, msg_id: &[u8; 16]) -> Result<(), CoreError> {
        self.conn().execute(
            "DELETE FROM msg_attachments WHERE msg_id = ?1",
            [msg_id.as_slice()],
        )?;
        Ok(())
    }

    // ---- Réactions (communes DM/groupe, indexées par msg_id) ----

    /// Ajoute ou retire une réaction.
    pub fn set_reaction(
        &self,
        msg_id: &[u8; 16],
        author: &[u8; 32],
        emoji: &str,
        add: bool,
    ) -> Result<(), CoreError> {
        if add {
            self.conn().execute(
                "INSERT OR IGNORE INTO reactions (msg_id, author, emoji) VALUES (?1, ?2, ?3)",
                params![msg_id, author, emoji],
            )?;
        } else {
            self.conn().execute(
                "DELETE FROM reactions WHERE msg_id = ?1 AND author = ?2 AND emoji = ?3",
                params![msg_id, author, emoji],
            )?;
        }
        Ok(())
    }

    /// Réactions d'un message : `(emoji, auteurs)`.
    pub fn reactions(&self, msg_id: &[u8; 16]) -> Result<Vec<(String, [u8; 32])>, CoreError> {
        let mut stmt = self
            .conn()
            .prepare("SELECT emoji, author FROM reactions WHERE msg_id = ?1 ORDER BY emoji")?;
        let raws = stmt
            .query_map([msg_id.as_slice()], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        raws.into_iter().map(|(e, a)| Ok((e, blob(a)?))).collect()
    }

    // ---- Accusés de lecture ----

    /// Enregistre le dernier message lu par un pair (accusé de lecture entrant).
    ///
    /// Défense en profondeur : la cible doit être un message que *nous* lui
    /// avons envoyé dans cette conversation (`author <> peer`), et la marque ne
    /// fait qu'avancer — un accusé forgé ou rembobiné (`up_to` inconnu, ou dont
    /// le lamport n'est pas strictement supérieur au précédent) est ignoré
    /// silencieusement.
    pub fn set_read_mark(&self, peer: &[u8; 32], up_to: &[u8; 16]) -> Result<(), CoreError> {
        let conn = self.conn();
        // Appartenance : la cible acquittée doit être un de nos messages sortants
        // vers ce pair (dans un DM 1:1, `author <> peer` ⇒ auteur = nous).
        let mut stmt = conn.prepare(
            "SELECT lamport FROM dm_messages WHERE peer = ?1 AND msg_id = ?2 AND author <> ?1",
        )?;
        let mut rows = stmt.query(params![peer, up_to])?;
        let Some(row) = rows.next()? else {
            return Ok(()); // cible inconnue/étrangère : ignorée
        };
        let new_lamport: u64 = row.get(0)?;
        drop(rows);
        drop(stmt);
        // Monotonie : ne jamais reculer la position de lecture du pair.
        let mut cur = conn.prepare(
            "SELECT m.lamport FROM read_marks r
             JOIN dm_messages m ON m.msg_id = r.up_to
             WHERE r.peer = ?1",
        )?;
        let mut cur_rows = cur.query(params![peer])?;
        if let Some(current) = cur_rows.next()? {
            if new_lamport <= current.get::<_, u64>(0)? {
                return Ok(());
            }
        }
        drop(cur_rows);
        drop(cur);
        conn.execute(
            "INSERT INTO read_marks (peer, up_to) VALUES (?1, ?2)
             ON CONFLICT(peer) DO UPDATE SET up_to = excluded.up_to",
            params![peer, up_to],
        )?;
        Ok(())
    }

    /// Dernier message lu par un pair.
    pub fn read_mark(&self, peer: &[u8; 32]) -> Result<Option<[u8; 16]>, CoreError> {
        let mut stmt = self
            .conn()
            .prepare("SELECT up_to FROM read_marks WHERE peer = ?1")?;
        let mut rows = stmt.query([peer.as_slice()])?;
        match rows.next()? {
            Some(row) => Ok(Some(blob(row.get::<_, Vec<u8>>(0)?)?)),
            None => Ok(None),
        }
    }

    /// Lamport clock of a stored direct message (`None` if unknown). Maps a
    /// read-receipt target (`msg_id`) back to a conversation position.
    pub fn dm_lamport(&self, msg_id: &[u8; 16]) -> Result<Option<u64>, CoreError> {
        let mut stmt = self
            .conn()
            .prepare("SELECT lamport FROM dm_messages WHERE msg_id = ?1")?;
        let mut rows = stmt.query([msg_id.as_slice()])?;
        match rows.next()? {
            Some(row) => Ok(Some(row.get::<_, u64>(0)?)),
            None => Ok(None),
        }
    }

    /// Most recent non-deleted message authored by `peer` with a lamport at
    /// most `up_to_lamport` — the target of an outgoing read receipt.
    pub fn latest_dm_from_peer(
        &self,
        peer: &[u8; 32],
        up_to_lamport: u64,
    ) -> Result<Option<[u8; 16]>, CoreError> {
        let mut stmt = self.conn().prepare(
            "SELECT msg_id FROM dm_messages
             WHERE peer = ?1 AND author = ?1 AND lamport <= ?2 AND deleted = 0
             ORDER BY lamport DESC, msg_id DESC LIMIT 1",
        )?;
        let mut rows = stmt.query(params![peer, up_to_lamport.min(i64::MAX as u64) as i64])?;
        match rows.next()? {
            Some(row) => Ok(Some(blob(row.get::<_, Vec<u8>>(0)?)?)),
            None => Ok(None),
        }
    }

    /// Nombre de messages directs reçus de `peer` (dont il est l'auteur),
    /// strictement après `after_lamport` et non supprimés : badge de non-lus
    /// d'une conversation directe.
    pub fn count_dm_unread(&self, peer: &[u8; 32], after_lamport: u64) -> Result<u64, CoreError> {
        Ok(self.conn().query_row(
            "SELECT COUNT(*) FROM dm_messages
             WHERE peer = ?1 AND author = ?1 AND lamport > ?2 AND deleted = 0",
            params![peer, after_lamport.min(i64::MAX as u64) as i64],
            |row| row.get(0),
        )?)
    }

    // ---- Messages de groupe ----

    /// Insère un message de groupe (idempotent).
    pub fn insert_group_msg(&self, m: &GroupMsgRecord) -> Result<bool, CoreError> {
        let n = self.conn().execute(
            "INSERT OR IGNORE INTO group_messages
               (msg_id, group_id, channel_id, author, lamport, sent_ms, kind, body, deleted, edited)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                m.msg_id,
                m.group_id,
                m.channel_id,
                m.author,
                m.lamport,
                m.sent_ms,
                m.kind,
                m.body,
                m.deleted,
                m.edited
            ],
        )?;
        Ok(n > 0)
    }

    /// Un message de groupe par identifiant (contrôles d'auteur avant une
    /// suppression ou un épinglage).
    pub fn group_msg(&self, msg_id: &[u8; 16]) -> Result<Option<GroupMsgRecord>, CoreError> {
        let mut stmt = self.conn().prepare(
            "SELECT msg_id, group_id, channel_id, author, lamport, sent_ms, kind, body, deleted, edited
             FROM group_messages WHERE msg_id = ?1",
        )?;
        let mut rows = stmt.query([msg_id.as_slice()])?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };
        Ok(Some(GroupMsgRecord {
            msg_id: blob(row.get::<_, Vec<u8>>(0)?)?,
            group_id: blob(row.get::<_, Vec<u8>>(1)?)?,
            channel_id: blob(row.get::<_, Vec<u8>>(2)?)?,
            author: blob(row.get::<_, Vec<u8>>(3)?)?,
            lamport: row.get(4)?,
            sent_ms: row.get(5)?,
            kind: row.get(6)?,
            body: row.get(7)?,
            deleted: row.get(8)?,
            edited: row.get(9)?,
        }))
    }

    /// Exécute une requête projetant [`GROUP_COLS`] et matérialise les records.
    fn group_rows(&self, sql: &str, args: &[&dyn ToSql]) -> Result<Vec<GroupMsgRecord>, CoreError> {
        let mut stmt = self.conn().prepare(sql)?;
        let raws = stmt
            .query_map(args, group_raw)?
            .collect::<Result<Vec<_>, _>>()?;
        raws.into_iter().map(to_group_record).collect()
    }

    /// Historique d'un salon, du plus récent au plus ancien.
    pub fn group_history(
        &self,
        group_id: &[u8; 16],
        channel_id: &[u8; 16],
        before_lamport: u64,
        limit: usize,
    ) -> Result<Vec<GroupMsgRecord>, CoreError> {
        let sql = format!(
            "SELECT {GROUP_COLS} FROM group_messages
             WHERE group_id = ?1 AND channel_id = ?2 AND lamport < ?3
             ORDER BY lamport DESC, msg_id DESC LIMIT ?4"
        );
        self.group_rows(
            &sql,
            &[
                &group_id.as_slice(),
                &channel_id.as_slice(),
                &(before_lamport.min(i64::MAX as u64) as i64),
                &(limit as i64),
            ],
        )
    }

    /// Fenêtre d'historique d'un salon centrée sur `msg_id` (cf.
    /// [`Db::dm_history_around`]). Rend `None` si la cible est inconnue dans ce
    /// salon.
    pub fn group_history_around(
        &self,
        group_id: &[u8; 16],
        channel_id: &[u8; 16],
        msg_id: &[u8; 16],
        limit: usize,
    ) -> Result<Option<Vec<GroupMsgRecord>>, CoreError> {
        let Some(target) = self.group_msg(msg_id)? else {
            return Ok(None);
        };
        if target.group_id != *group_id || target.channel_id != *channel_id {
            return Ok(None);
        }
        let half = (limit / 2) as i64;
        let tl = target.lamport.min(i64::MAX as u64) as i64;
        let older = self.group_rows(
            &format!(
                "SELECT {GROUP_COLS} FROM group_messages
                 WHERE group_id = ?1 AND channel_id = ?2
                   AND (lamport < ?3 OR (lamport = ?3 AND msg_id < ?4))
                 ORDER BY lamport DESC, msg_id DESC LIMIT ?5"
            ),
            &[
                &group_id.as_slice(),
                &channel_id.as_slice(),
                &tl,
                &msg_id.as_slice(),
                &half,
            ],
        )?;
        let mut newer = self.group_rows(
            &format!(
                "SELECT {GROUP_COLS} FROM group_messages
                 WHERE group_id = ?1 AND channel_id = ?2
                   AND (lamport > ?3 OR (lamport = ?3 AND msg_id > ?4))
                 ORDER BY lamport ASC, msg_id ASC LIMIT ?5"
            ),
            &[
                &group_id.as_slice(),
                &channel_id.as_slice(),
                &tl,
                &msg_id.as_slice(),
                &half,
            ],
        )?;
        newer.reverse();
        let mut window = Vec::with_capacity(newer.len() + 1 + older.len());
        window.append(&mut newer);
        window.push(target);
        window.extend(older);
        Ok(Some(window))
    }

    /// Messages de groupe les plus récents, tous salons confondus, bornés à
    /// `cap` (candidats de recherche filtrée sans mot-clé).
    pub fn group_recent(&self, cap: usize) -> Result<Vec<GroupMsgRecord>, CoreError> {
        let sql = format!(
            "SELECT {GROUP_COLS} FROM group_messages
             ORDER BY lamport DESC, msg_id DESC LIMIT ?1"
        );
        self.group_rows(&sql, &[&(cap as i64)])
    }

    /// Nombre de messages d'un salon strictement après `after_lamport`, écrits
    /// par un autre que `exclude_author` (nos propres messages sont toujours
    /// « lus ») et non supprimés : badge de non-lus d'un salon.
    pub fn count_group_unread(
        &self,
        group_id: &[u8; 16],
        channel_id: &[u8; 16],
        after_lamport: u64,
        exclude_author: &[u8; 32],
    ) -> Result<u64, CoreError> {
        Ok(self.conn().query_row(
            "SELECT COUNT(*) FROM group_messages
             WHERE group_id = ?1 AND channel_id = ?2 AND lamport > ?3
               AND author <> ?4 AND deleted = 0",
            params![
                group_id,
                channel_id,
                after_lamport.min(i64::MAX as u64) as i64,
                exclude_author
            ],
            |row| row.get(0),
        )?)
    }

    /// Édition d'un message de groupe par son auteur.
    pub fn edit_group_msg(
        &self,
        msg_id: &[u8; 16],
        author: &[u8; 32],
        new_body: &[u8],
    ) -> Result<bool, CoreError> {
        let n = self.conn().execute(
            "UPDATE group_messages SET edited = ?3
             WHERE msg_id = ?1 AND author = ?2 AND deleted = 0",
            params![msg_id, author, new_body],
        )?;
        Ok(n > 0)
    }

    /// Suppression d'un message de groupe (pièces jointes comprises).
    /// `by_author = None` pour la modération (autorisation déjà vérifiée par
    /// l'op-log signée).
    pub fn delete_group_msg(
        &self,
        msg_id: &[u8; 16],
        by_author: Option<&[u8; 32]>,
    ) -> Result<bool, CoreError> {
        let n = match by_author {
            Some(author) => self.conn().execute(
                "UPDATE group_messages SET deleted = 1, body = x'', edited = NULL
                 WHERE msg_id = ?1 AND author = ?2",
                params![msg_id, author],
            )?,
            None => self.conn().execute(
                "UPDATE group_messages SET deleted = 1, body = x'', edited = NULL
                 WHERE msg_id = ?1",
                [msg_id.as_slice()],
            )?,
        };
        if n > 0 {
            self.delete_msg_attachments(msg_id)?;
            // L'extrait de mention d'un message effacé ne doit pas survivre.
            self.delete_mention(msg_id)?;
        }
        Ok(n > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dm(id: u8, lamport: u64) -> DmRecord {
        DmRecord {
            msg_id: [id; 16],
            peer: [1; 32],
            author: [2; 32],
            lamport,
            sent_ms: 1000 + lamport,
            kind: 0,
            body: vec![id],
            acked: false,
            deleted: false,
            edited: None,
        }
    }

    #[test]
    fn insert_is_idempotent_and_history_paginates() {
        let db = Db::open_in_memory(&[1; 32]).unwrap();
        for i in 1..=5u8 {
            assert!(db.insert_dm(&dm(i, i as u64)).unwrap());
        }
        assert!(!db.insert_dm(&dm(3, 3)).unwrap(), "doublon accepté");
        let page = db.dm_history(&[1; 32], u64::MAX, 3).unwrap();
        assert_eq!(page.len(), 3);
        assert_eq!(page[0].lamport, 5);
        let older = db.dm_history(&[1; 32], page[2].lamport, 10).unwrap();
        assert_eq!(older.len(), 2);
        assert_eq!(older[0].lamport, 2);
    }

    #[test]
    fn edit_requires_original_author_and_delete_wipes_body() {
        let db = Db::open_in_memory(&[1; 32]).unwrap();
        db.insert_dm(&dm(1, 1)).unwrap();
        assert!(!db.edit_dm(&[1; 16], &[9; 32], b"pirate").unwrap());
        assert!(db.edit_dm(&[1; 16], &[2; 32], b"corrige").unwrap());
        assert!(db.delete_dm(&[1; 16], &[2; 32]).unwrap());
        let h = db.dm_history(&[1; 32], u64::MAX, 10).unwrap();
        assert!(h[0].deleted);
        assert!(h[0].body.is_empty());
        assert_eq!(h[0].edited, None);
        // Une édition post-suppression est refusée.
        assert!(!db.edit_dm(&[1; 16], &[2; 32], b"trop tard").unwrap());
    }

    #[test]
    fn reactions_add_remove_and_read_marks() {
        let db = Db::open_in_memory(&[1; 32]).unwrap();
        db.set_reaction(&[1; 16], &[2; 32], "👍", true).unwrap();
        db.set_reaction(&[1; 16], &[3; 32], "👍", true).unwrap();
        db.set_reaction(&[1; 16], &[2; 32], "👍", true).unwrap(); // idempotent
        assert_eq!(db.reactions(&[1; 16]).unwrap().len(), 2);
        db.set_reaction(&[1; 16], &[2; 32], "👍", false).unwrap();
        assert_eq!(db.reactions(&[1; 16]).unwrap().len(), 1);

        // Read marks: the target must be one of OUR outgoing messages to the
        // peer, and the mark only advances (defense in depth).
        assert_eq!(db.read_mark(&[1; 32]).unwrap(), None);
        db.insert_dm(&dm(4, 4)).unwrap(); // ours (author [2;32]), lamport 4
        db.insert_dm(&dm(5, 8)).unwrap(); // ours, lamport 8
        db.set_read_mark(&[1; 32], &[4; 16]).unwrap();
        assert_eq!(db.read_mark(&[1; 32]).unwrap(), Some([4; 16]));
        db.set_read_mark(&[1; 32], &[5; 16]).unwrap(); // advances (8 > 4)
        assert_eq!(db.read_mark(&[1; 32]).unwrap(), Some([5; 16]));
        // Rewind ignored: [4;16] has a lower lamport than the current mark.
        db.set_read_mark(&[1; 32], &[4; 16]).unwrap();
        assert_eq!(db.read_mark(&[1; 32]).unwrap(), Some([5; 16]));
        // Forged/unknown target ignored (never one of our messages).
        db.set_read_mark(&[1; 32], &[99; 16]).unwrap();
        assert_eq!(db.read_mark(&[1; 32]).unwrap(), Some([5; 16]));
        // A message the PEER authored (not ours) is not a valid ack target.
        db.insert_dm(&DmRecord {
            msg_id: [7; 16],
            peer: [1; 32],
            author: [1; 32],
            lamport: 20,
            sent_ms: 0,
            kind: 0,
            body: vec![7],
            acked: true,
            deleted: false,
            edited: None,
        })
        .unwrap();
        db.set_read_mark(&[1; 32], &[7; 16]).unwrap();
        assert_eq!(db.read_mark(&[1; 32]).unwrap(), Some([5; 16]));
    }

    #[test]
    fn dm_lamport_lookup_and_latest_from_peer() {
        let db = Db::open_in_memory(&[1; 32]).unwrap();
        // Peer [1;32]: their messages have author == peer; ours author [2;32].
        db.insert_dm(&dm(1, 1)).unwrap(); // ours (author [2;32])
        for (id, l) in [(2u8, 2u64), (3, 5), (4, 9)] {
            db.insert_dm(&DmRecord {
                msg_id: [id; 16],
                peer: [1; 32],
                author: [1; 32],
                lamport: l,
                sent_ms: 0,
                kind: 0,
                body: vec![id],
                acked: true,
                deleted: false,
                edited: None,
            })
            .unwrap();
        }
        assert_eq!(db.dm_lamport(&[3; 16]).unwrap(), Some(5));
        assert_eq!(db.dm_lamport(&[9; 16]).unwrap(), None);

        // Latest peer message at or below the mark; our own are excluded.
        assert_eq!(db.latest_dm_from_peer(&[1; 32], 9).unwrap(), Some([4; 16]));
        assert_eq!(db.latest_dm_from_peer(&[1; 32], 6).unwrap(), Some([3; 16]));
        assert_eq!(db.latest_dm_from_peer(&[1; 32], 1).unwrap(), None);
        // A deleted message is no longer a receipt target.
        db.delete_dm(&[4; 16], &[1; 32]).unwrap();
        assert_eq!(db.latest_dm_from_peer(&[1; 32], 9).unwrap(), Some([3; 16]));
    }

    #[test]
    fn attachments_roundtrip_ordered_and_wiped_on_delete() {
        let db = Db::open_in_memory(&[1; 32]).unwrap();
        db.insert_dm(&dm(1, 1)).unwrap();
        let atts = vec![
            FileRef {
                merkle_root: [7; 32],
                name: "a.png".into(),
                size: 10,
                mime: "image/png".into(),
            },
            FileRef {
                merkle_root: [8; 32],
                name: "b.pdf".into(),
                size: 20,
                mime: "application/pdf".into(),
            },
        ];
        db.put_msg_attachments(&[1; 16], &atts).unwrap();
        // Rejeu idempotent.
        db.put_msg_attachments(&[1; 16], &atts).unwrap();
        assert_eq!(db.msg_attachments(&[1; 16]).unwrap(), atts);
        assert!(db.msg_attachments(&[9; 16]).unwrap().is_empty());
        // La suppression du message efface aussi ses pièces jointes.
        assert!(db.delete_dm(&[1; 16], &[2; 32]).unwrap());
        assert!(db.msg_attachments(&[1; 16]).unwrap().is_empty());
    }

    #[test]
    fn dm_message_getter_and_history_around_window() {
        let db = Db::open_in_memory(&[1; 32]).unwrap();
        // Sept messages du même pair, lamport 1..=7.
        for i in 1..=7u8 {
            db.insert_dm(&dm(i, i as u64)).unwrap();
        }
        assert_eq!(db.dm_message(&[4; 16]).unwrap().unwrap().lamport, 4);
        assert!(db.dm_message(&[9; 16]).unwrap().is_none());

        // Fenêtre centrée sur lamport 4 avec limit 4 (2 avant + cible + 2 après).
        let win = db
            .dm_history_around(&[1; 32], &[4; 16], 4)
            .unwrap()
            .unwrap();
        let lamports: Vec<u64> = win.iter().map(|m| m.lamport).collect();
        assert_eq!(
            lamports,
            vec![6, 5, 4, 3, 2],
            "récent → ancien, cible incluse"
        );

        // Cible en bord : moins d'anciens/récents disponibles, pas de panique.
        let win = db
            .dm_history_around(&[1; 32], &[1; 16], 4)
            .unwrap()
            .unwrap();
        assert_eq!(
            win.iter().map(|m| m.lamport).collect::<Vec<_>>(),
            vec![3, 2, 1]
        );

        // Cible inconnue ou d'un autre pair : None (found=false côté API).
        assert!(db
            .dm_history_around(&[1; 32], &[9; 16], 4)
            .unwrap()
            .is_none());
        assert!(db
            .dm_history_around(&[2; 32], &[4; 16], 4)
            .unwrap()
            .is_none());
    }

    #[test]
    fn dm_pins_add_list_remove_and_wiped_on_delete() {
        let db = Db::open_in_memory(&[1; 32]).unwrap();
        db.insert_dm(&dm(1, 1)).unwrap();
        db.insert_dm(&dm(2, 2)).unwrap();
        assert!(db.dm_pins(&[1; 32]).unwrap().is_empty());
        db.dm_pin(&[1; 32], &[2; 16]).unwrap();
        db.dm_pin(&[1; 32], &[1; 16]).unwrap();
        db.dm_pin(&[1; 32], &[1; 16]).unwrap(); // idempotent
        assert_eq!(db.dm_pins(&[1; 32]).unwrap(), vec![[1; 16], [2; 16]]);
        assert!(db.dm_pinned_set(&[1; 32]).unwrap().contains(&[2; 16]));
        db.dm_unpin(&[1; 32], &[1; 16]).unwrap();
        assert_eq!(db.dm_pins(&[1; 32]).unwrap(), vec![[2; 16]]);
        // La suppression d'un message retire aussi son épingle.
        db.dm_pin(&[1; 32], &[2; 16]).unwrap();
        db.delete_dm(&[2; 16], &[2; 32]).unwrap();
        assert!(db.dm_pins(&[1; 32]).unwrap().is_empty());
    }

    #[test]
    fn group_history_around_scoped_to_channel() {
        let db = Db::open_in_memory(&[1; 32]).unwrap();
        for i in 1..=5u8 {
            db.insert_group_msg(&GroupMsgRecord {
                msg_id: [i; 16],
                group_id: [7; 16],
                channel_id: [8; 16],
                author: [2; 32],
                lamport: i as u64,
                sent_ms: 0,
                kind: 0,
                body: vec![i],
                deleted: false,
                edited: None,
            })
            .unwrap();
        }
        let win = db
            .group_history_around(&[7; 16], &[8; 16], &[3; 16], 2)
            .unwrap()
            .unwrap();
        assert_eq!(
            win.iter().map(|m| m.lamport).collect::<Vec<_>>(),
            vec![4, 3, 2]
        );
        // Mauvais salon : None.
        assert!(db
            .group_history_around(&[7; 16], &[9; 16], &[3; 16], 2)
            .unwrap()
            .is_none());
    }

    #[test]
    fn group_msg_lookup_by_id() {
        let db = Db::open_in_memory(&[1; 32]).unwrap();
        let m = GroupMsgRecord {
            msg_id: [4; 16],
            group_id: [7; 16],
            channel_id: [8; 16],
            author: [2; 32],
            lamport: 4,
            sent_ms: 99,
            kind: 0,
            body: vec![1],
            deleted: false,
            edited: None,
        };
        db.insert_group_msg(&m).unwrap();
        assert_eq!(db.group_msg(&[4; 16]).unwrap(), Some(m));
        assert_eq!(db.group_msg(&[5; 16]).unwrap(), None);
    }

    #[test]
    fn group_messages_roundtrip() {
        let db = Db::open_in_memory(&[1; 32]).unwrap();
        let m = GroupMsgRecord {
            msg_id: [1; 16],
            group_id: [7; 16],
            channel_id: [8; 16],
            author: [2; 32],
            lamport: 4,
            sent_ms: 99,
            kind: 0,
            body: vec![1, 2, 3],
            deleted: false,
            edited: None,
        };
        assert!(db.insert_group_msg(&m).unwrap());
        assert!(!db.insert_group_msg(&m).unwrap());
        let h = db.group_history(&[7; 16], &[8; 16], u64::MAX, 10).unwrap();
        assert_eq!(h, vec![m]);
        // Modération sans auteur.
        assert!(db.delete_group_msg(&[1; 16], None).unwrap());
        let h = db.group_history(&[7; 16], &[8; 16], u64::MAX, 10).unwrap();
        assert!(h[0].deleted && h[0].body.is_empty());
    }

    #[test]
    fn unread_counts_exclude_own_read_and_deleted() {
        let db = Db::open_in_memory(&[1; 32]).unwrap();
        let me = [1; 32];
        let peer = [2; 32];
        // Trois messages du pair (lamport 1,2,3) et un de nous (lamport 4).
        for (id, l, author) in [(1u8, 1u64, peer), (2, 2, peer), (3, 3, peer), (4, 4, me)] {
            db.insert_group_msg(&GroupMsgRecord {
                msg_id: [id; 16],
                group_id: [7; 16],
                channel_id: [8; 16],
                author,
                lamport: l,
                sent_ms: 0,
                kind: 0,
                body: vec![id],
                deleted: false,
                edited: None,
            })
            .unwrap();
        }
        // Marque à 1 : deux messages du pair restent (lamport 2,3), le nôtre
        // (lamport 4) est exclu.
        assert_eq!(
            db.count_group_unread(&[7; 16], &[8; 16], 1, &me).unwrap(),
            2
        );
        // Aucun non-lu au-delà du dernier.
        assert_eq!(
            db.count_group_unread(&[7; 16], &[8; 16], 3, &me).unwrap(),
            0
        );
        // La suppression retire du décompte.
        db.delete_group_msg(&[2; 16], None).unwrap();
        assert_eq!(
            db.count_group_unread(&[7; 16], &[8; 16], 1, &me).unwrap(),
            1
        );
    }

    #[test]
    fn dm_unread_counts_only_peer_messages_after_mark() {
        let db = Db::open_in_memory(&[1; 32]).unwrap();
        // peer=[1;32], leurs messages author=[1;32] ; un des nôtres author=[2;32].
        db.insert_dm(&dm(1, 1)).unwrap(); // author [2;32] (nous)
        for id in 2u8..=4 {
            db.insert_dm(&DmRecord {
                msg_id: [id; 16],
                peer: [1; 32],
                author: [1; 32],
                lamport: id as u64,
                sent_ms: 0,
                kind: 0,
                body: vec![id],
                acked: true,
                deleted: false,
                edited: None,
            })
            .unwrap();
        }
        // Après marque 1 : trois messages du pair (lamport 2,3,4), le nôtre exclu.
        assert_eq!(db.count_dm_unread(&[1; 32], 1).unwrap(), 3);
        assert_eq!(db.count_dm_unread(&[1; 32], 4).unwrap(), 0);
    }
}
