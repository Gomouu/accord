//! File d'attente hors ligne persistante : retransmission avec backoff
//! exponentiel (5 s ×2, plafond 15 min) et expiration à 7 jours (SPEC §7).

use super::{blob, Db};
use crate::error::CoreError;
use rusqlite::params;

/// Backoff initial : 5 secondes.
pub const BACKOFF_BASE_MS: u64 = 5_000;
/// Plafond de backoff : 15 minutes.
pub const BACKOFF_MAX_MS: u64 = 15 * 60 * 1000;
/// Expiration d'un message en file : 7 jours.
pub const QUEUE_EXPIRY_MS: u64 = 7 * 24 * 3600 * 1000;

/// Délai avant la tentative `attempts + 1`.
pub fn backoff_ms(attempts: u32) -> u64 {
    BACKOFF_BASE_MS
        .saturating_mul(1u64 << attempts.min(20))
        .min(BACKOFF_MAX_MS)
}

/// Élément de la file d'envoi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboxItem {
    /// Identifiant local de file.
    pub id: i64,
    /// Destinataire (node_id).
    pub dest: [u8; 32],
    /// CoreMsg encodé, prêt à envoyer dans la session E2E.
    pub payload: Vec<u8>,
    /// Date de mise en file (ms).
    pub created_ms: u64,
    /// Tentatives déjà effectuées.
    pub attempts: u32,
    /// Jour Unix du dernier dépôt en boîte aux lettres DHT (0 = jamais).
    /// Un dépôt par jour et par destinataire : les clés DHT sont par jour et
    /// un nouveau dépôt du même jour remplace le précédent — re-déposer
    /// chaque jour entretient la fenêtre de 7 jours au lieu de la laisser
    /// expirer après l'unique dépôt initial.
    pub mailboxed_day: u64,
}

impl Db {
    /// Met un message en file pour un destinataire ; rend l'id local.
    pub fn enqueue(&self, dest: &[u8; 32], payload: &[u8], now_ms: u64) -> Result<i64, CoreError> {
        self.conn().execute(
            "INSERT INTO outbox (dest, payload, created_ms, next_attempt_ms)
             VALUES (?1, ?2, ?3, ?3)",
            params![dest, payload, now_ms],
        )?;
        Ok(self.conn().last_insert_rowid())
    }

    /// Éléments dus (prochaine tentative atteinte), plus anciens d'abord.
    pub fn outbox_due(&self, now_ms: u64, limit: usize) -> Result<Vec<OutboxItem>, CoreError> {
        let mut stmt = self.conn().prepare(
            "SELECT id, dest, payload, created_ms, attempts, mailboxed_day
             FROM outbox WHERE next_attempt_ms <= ?1
             ORDER BY created_ms ASC LIMIT ?2",
        )?;
        let raws = stmt
            .query_map(params![now_ms, limit as i64], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, u64>(3)?,
                    row.get::<_, u32>(4)?,
                    row.get::<_, u64>(5)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        raws.into_iter()
            .map(|r| {
                Ok(OutboxItem {
                    id: r.0,
                    dest: blob(r.1)?,
                    payload: r.2,
                    created_ms: r.3,
                    attempts: r.4,
                    mailboxed_day: r.5,
                })
            })
            .collect()
    }

    /// Tous les éléments en file pour un destinataire (reconnexion du pair).
    pub fn outbox_for(&self, dest: &[u8; 32]) -> Result<Vec<OutboxItem>, CoreError> {
        let mut stmt = self.conn().prepare(
            "SELECT id, dest, payload, created_ms, attempts, mailboxed_day
             FROM outbox WHERE dest = ?1 ORDER BY created_ms ASC",
        )?;
        let raws = stmt
            .query_map([dest.as_slice()], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, u64>(3)?,
                    row.get::<_, u32>(4)?,
                    row.get::<_, u64>(5)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        raws.into_iter()
            .map(|r| {
                Ok(OutboxItem {
                    id: r.0,
                    dest: blob(r.1)?,
                    payload: r.2,
                    created_ms: r.3,
                    attempts: r.4,
                    mailboxed_day: r.5,
                })
            })
            .collect()
    }

    /// Destinataires DISTINCTS ayant au moins un message en file, bornés à
    /// `limit` (ordre stable par premier message en file, plus ancien d'abord).
    /// Sert à élargir la résolution de présence aux destinataires non-amis
    /// (demande d'ami sortante) sans jamais balayer une file arbitrairement
    /// grande en une passe.
    pub fn outbox_dests(&self, limit: usize) -> Result<Vec<[u8; 32]>, CoreError> {
        let mut stmt = self.conn().prepare(
            "SELECT dest, MIN(created_ms) AS oldest FROM outbox
             GROUP BY dest ORDER BY oldest ASC LIMIT ?1",
        )?;
        let raws = stmt
            .query_map([limit as i64], |row| row.get::<_, Vec<u8>>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        raws.into_iter().map(blob).collect()
    }

    /// Replanifie après un échec d'envoi : incrémente `attempts` et applique
    /// le backoff exponentiel.
    pub fn outbox_reschedule(&self, id: i64, now_ms: u64) -> Result<(), CoreError> {
        self.conn().execute(
            "UPDATE outbox SET attempts = attempts + 1,
               next_attempt_ms = ?2 + ?3
             WHERE id = ?1",
            params![id, now_ms, backoff_ms(self.outbox_attempts(id)?)],
        )?;
        Ok(())
    }

    fn outbox_attempts(&self, id: i64) -> Result<u32, CoreError> {
        Ok(self
            .conn()
            .query_row("SELECT attempts FROM outbox WHERE id = ?1", [id], |row| {
                row.get(0)
            })?)
    }

    /// Marque un élément comme déposé en boîte aux lettres le jour `day`.
    pub fn outbox_mark_mailboxed(&self, id: i64, day: u64) -> Result<(), CoreError> {
        self.conn().execute(
            "UPDATE outbox SET mailboxed_day = ?2 WHERE id = ?1",
            params![id, day],
        )?;
        Ok(())
    }

    /// Retire un élément livré (ACK reçu).
    pub fn outbox_remove(&self, id: i64) -> Result<(), CoreError> {
        self.conn()
            .execute("DELETE FROM outbox WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Purge les éléments expirés (> 7 jours) ; rend le nombre supprimé.
    pub fn outbox_purge_expired(&self, now_ms: u64) -> Result<usize, CoreError> {
        let n = self.conn().execute(
            "DELETE FROM outbox WHERE created_ms + ?1 <= ?2",
            params![QUEUE_EXPIRY_MS, now_ms],
        )?;
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_doubles_and_caps() {
        assert_eq!(backoff_ms(0), 5_000);
        assert_eq!(backoff_ms(1), 10_000);
        assert_eq!(backoff_ms(2), 20_000);
        assert_eq!(backoff_ms(7), 640_000);
        assert_eq!(backoff_ms(8), BACKOFF_MAX_MS);
        assert_eq!(backoff_ms(63), BACKOFF_MAX_MS, "pas de débordement");
    }

    #[test]
    fn queue_lifecycle() {
        let db = Db::open_in_memory(&[1; 32]).unwrap();
        let id = db.enqueue(&[7; 32], b"msg", 1000).unwrap();
        assert_eq!(db.outbox_due(999, 10).unwrap().len(), 0);
        let due = db.outbox_due(1000, 10).unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].payload, b"msg");

        // Échec ⇒ replanifié à +5 s, plus dû immédiatement.
        db.outbox_reschedule(id, 1000).unwrap();
        assert_eq!(db.outbox_due(1000, 10).unwrap().len(), 0);
        let due = db.outbox_due(6000, 10).unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].attempts, 1);

        db.outbox_mark_mailboxed(id, 42).unwrap();
        assert_eq!(db.outbox_for(&[7; 32]).unwrap()[0].mailboxed_day, 42);

        db.outbox_remove(id).unwrap();
        assert!(db.outbox_for(&[7; 32]).unwrap().is_empty());
    }

    #[test]
    fn destinations_distinctes_ordonnees_et_bornees() {
        let db = Db::open_in_memory(&[1; 32]).unwrap();
        assert!(db.outbox_dests(8).unwrap().is_empty(), "file vide");
        // Deux messages pour [7], un pour [9] plus ancien : distincts, ordre
        // par plus ancien message d'abord.
        db.enqueue(&[9; 32], b"a", 500).unwrap();
        db.enqueue(&[7; 32], b"b", 1000).unwrap();
        db.enqueue(&[7; 32], b"c", 2000).unwrap();
        assert_eq!(db.outbox_dests(8).unwrap(), vec![[9; 32], [7; 32]]);
        // Borne appliquée.
        assert_eq!(db.outbox_dests(1).unwrap(), vec![[9; 32]]);
    }

    #[test]
    fn outbox_for_utilise_l_index_par_destinataire() {
        // Migration v10 : sans `outbox_by_dest`, `outbox_for` balayait toute
        // la file (SCAN). Le plan doit passer par l'index, qui couvre aussi
        // l'ordre `created_ms` (pas d'étape de tri résiduelle).
        let db = Db::open_in_memory(&[1; 32]).unwrap();
        let plan: String = db
            .conn()
            .query_row(
                "EXPLAIN QUERY PLAN SELECT id, dest, payload, created_ms, attempts, mailboxed_day
                 FROM outbox WHERE dest = x'00' ORDER BY created_ms ASC",
                [],
                |row| row.get(3),
            )
            .unwrap();
        assert!(plan.contains("outbox_by_dest"), "plan sans index : {plan}");
    }

    #[test]
    fn migration_v10_cree_l_index_sur_une_base_v9() {
        // Base persistée « v9 » : mêmes tables, sans l'index, version 9. La
        // réouverture doit rejouer le lot idempotent et créer l'index sans
        // toucher aux lignes existantes.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("core.db");
        let key = [5u8; 32];
        {
            let db = Db::open(&path, &key).unwrap();
            db.enqueue(&[7; 32], b"msg", 1000).unwrap();
            db.conn()
                .execute_batch("DROP INDEX outbox_by_dest; PRAGMA user_version = 9;")
                .unwrap();
        }
        let db = Db::open(&path, &key).unwrap();
        let n: i64 = db
            .conn()
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = 'index' AND name = 'outbox_by_dest'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(n, 1, "index recréé par la migration");
        assert_eq!(
            db.outbox_for(&[7; 32]).unwrap().len(),
            1,
            "données intactes"
        );
    }

    #[test]
    fn expiry_purges_old_items() {
        let db = Db::open_in_memory(&[1; 32]).unwrap();
        db.enqueue(&[7; 32], b"vieux", 0).unwrap();
        db.enqueue(&[7; 32], b"neuf", QUEUE_EXPIRY_MS).unwrap();
        assert_eq!(db.outbox_purge_expired(QUEUE_EXPIRY_MS).unwrap(), 1);
        let rest = db.outbox_for(&[7; 32]).unwrap();
        assert_eq!(rest.len(), 1);
        assert_eq!(rest[0].payload, b"neuf");
    }
}
