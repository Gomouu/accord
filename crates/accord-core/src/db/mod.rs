//! Base locale chiffrée (SQLCipher) : schéma, ouverture par `db_key`,
//! horloge de Lamport persistante et métadonnées (SPEC §2.6).

mod contacts;
mod files;
mod groups;
mod invites;
mod mentions;
mod messages;
mod outbox;
mod search;

pub use contacts::{Contact, ContactState};
pub use files::{FetchIntent, FileEntry};
pub use groups::{LocalMembership, StoredGroupKey};
pub use invites::IncomingInvite;
pub use mentions::{MentionEntry, MentionScope};
pub use messages::{DmRecord, GroupMsgRecord};
pub use outbox::OutboxItem;

use crate::error::CoreError;
use rusqlite::Connection;
use std::collections::HashMap;
use std::path::Path;

/// Version de schéma courante (migrations linéaires).
///
/// Le lot de création est entièrement idempotent (`IF NOT EXISTS`) : monter
/// la version suffit pour créer les nouvelles tables sur une base existante.
/// Modifier des colonnes existantes exigera en revanche une vraie migration.
const SCHEMA_VERSION: i64 = 10;

/// Convertit un blob SQL en tableau de taille fixe.
pub(crate) fn blob<const N: usize>(v: Vec<u8>) -> Result<[u8; N], CoreError> {
    v.try_into()
        .map_err(|_| CoreError::Invalid("taille de blob"))
}

/// Taille de tranche des requêtes `IN (…)` par lot : sous la limite SQLite de
/// variables liées (999 par défaut), et bornée pour que `prepare_cached` ne
/// garde qu'un petit nombre de formes de requête distinctes.
pub(crate) const IN_CHUNK: usize = 256;

/// Liste de `n` marqueurs `?` pour une clause `IN (…)` construite par lot.
/// Aucune donnée n'est interpolée : uniquement des marqueurs, les valeurs
/// restent liées par paramètres.
pub(crate) fn sql_placeholders(n: usize) -> String {
    let mut s = String::with_capacity(n * 2);
    for i in 0..n {
        if i > 0 {
            s.push(',');
        }
        s.push('?');
    }
    s
}

/// Encode une clé binaire en littéral hexadécimal SQLCipher `x'…'`.
fn hex_key(key: &[u8; 32]) -> String {
    let mut s = String::with_capacity(64);
    for b in key {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Base locale chiffrée. Toutes les écritures passent par des requêtes
/// paramétrées ; aucun contenu n'est journalisé.
pub struct Db {
    conn: Connection,
    /// Cache mémoire des états de groupe matérialisés et des offres de
    /// synchronisation, invalidé à chaque insertion RÉELLE d'op. Sans lui,
    /// chaque message composé/ingéré et chaque tick d'anti-entropie
    /// rechargeait puis repliait TOUT l'op-log (O(ops), quadratique dans le
    /// temps sur un serveur actif). Le repli étant déterministe, un cache
    /// par groupe est exact tant que le log ne change pas.
    group_cache: std::sync::Mutex<HashMap<[u8; 16], GroupCacheEntry>>,
}

/// Entrée de cache d'un groupe : état replié et/ou offre de synchronisation.
#[derive(Default)]
pub(crate) struct GroupCacheEntry {
    pub(crate) state: Option<std::sync::Arc<crate::group::GroupState>>,
    pub(crate) offer: Option<crate::group::SyncOffer>,
}

impl Db {
    /// Ouvre (ou crée) la base au chemin donné, chiffrée par `db_key`.
    /// Échoue si la clé ne correspond pas à une base existante.
    pub fn open(path: &Path, db_key: &[u8; 32]) -> Result<Self, CoreError> {
        let conn = Connection::open(path)?;
        Self::init(conn, db_key)
    }

    /// Base en mémoire (tests). La clé est appliquée mais sans effet durable.
    pub fn open_in_memory(db_key: &[u8; 32]) -> Result<Self, CoreError> {
        let conn = Connection::open_in_memory()?;
        Self::init(conn, db_key)
    }

    fn init(conn: Connection, db_key: &[u8; 32]) -> Result<Self, CoreError> {
        // Clé brute SQLCipher (pas de KDF interne : db_key sort déjà de HKDF).
        conn.execute_batch(&format!("PRAGMA key = \"x'{}'\";", hex_key(db_key)))?;
        // Vérifie que la clé ouvre bien la base (première lecture réelle).
        conn.query_row("SELECT count(*) FROM sqlite_master", [], |_| Ok(()))?;
        // `synchronous = NORMAL` est sûr sous WAL : la base ne peut pas être
        // corrompue ; au pire, le dernier commit est perdu sur une coupure de
        // l'OS — acceptable pour un client, contre un fsync par message en
        // FULL. `cache_size` négatif = Kio (~16 Mio de cache de pages).
        // `mmap_size` est demandé mais SQLCipher désactive l'I/O mappée sur
        // une base chiffrée (les pages doivent être déchiffrées) : sans effet
        // ici, inoffensif, actif si la base devenait claire. `temp_store =
        // MEMORY` garde tris et index temporaires hors disque.
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;
             PRAGMA synchronous = NORMAL;
             PRAGMA cache_size = -16000;
             PRAGMA mmap_size = 268435456;
             PRAGMA temp_store = MEMORY;",
        )?;
        let db = Self {
            conn,
            group_cache: std::sync::Mutex::new(HashMap::new()),
        };
        db.migrate()?;
        Ok(db)
    }

    /// Vrai si `table` existe et porte la colonne `column` (PRAGMA
    /// table_info : vide pour une table inconnue). Support des migrations
    /// additives sur des tables créées par une version antérieure.
    fn has_column(&self, table: &str, column: &str) -> Result<bool, CoreError> {
        let mut stmt = self.conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            if row.get::<_, String>(1)? == column {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn migrate(&self) -> Result<(), CoreError> {
        let version: i64 = self
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version >= SCHEMA_VERSION {
            return Ok(());
        }
        // Migration v7 : backoff de ré-adoption des intentions de
        // téléchargement. Une base antérieure porte déjà `file_fetches` sans
        // ces colonnes ; on les ajoute AVANT le lot idempotent (qui, lui, les
        // crée directement sur une base neuve). Les anciennes lignes prennent
        // les valeurs par défaut (relance immédiate, aucun abandon compté).
        if self.has_column("file_fetches", "merkle_root")?
            && !self.has_column("file_fetches", "next_attempt_ms")?
        {
            self.conn.execute_batch(
                "ALTER TABLE file_fetches ADD COLUMN next_attempt_ms INTEGER NOT NULL DEFAULT 0;
                 ALTER TABLE file_fetches ADD COLUMN attempts INTEGER NOT NULL DEFAULT 0;",
            )?;
        }
        // Migration v8 : plafond de taille des médias auto-récupérés
        // (avatar/bannière de profil). Colonne annexe, nullable (`NULL` =
        // téléchargement ordinaire, sans plafond resserré) ; les anciennes
        // lignes restent non plafonnées.
        if self.has_column("file_fetches", "merkle_root")?
            && !self.has_column("file_fetches", "media_cap")?
        {
            self.conn
                .execute_batch("ALTER TABLE file_fetches ADD COLUMN media_cap INTEGER;")?;
        }
        // Migration v9 : re-dépôt quotidien des boîtes aux lettres. L'ancien
        // booléen `mailboxed` (un seul dépôt à vie, la fenêtre de 7 jours
        // expirait ensuite) devient `mailboxed_day` (jour Unix du dernier
        // dépôt). Les lignes existantes repartent à 0 : un re-dépôt de plus,
        // idempotent (remplacement du même jour côté DHT).
        if self.has_column("outbox", "mailboxed")? && !self.has_column("outbox", "mailboxed_day")? {
            self.conn.execute_batch(
                "ALTER TABLE outbox ADD COLUMN mailboxed_day INTEGER NOT NULL DEFAULT 0;",
            )?;
        }
        self.conn.execute_batch(
            "BEGIN;
             CREATE TABLE IF NOT EXISTS meta (
               key   TEXT PRIMARY KEY,
               value BLOB NOT NULL
             );
             CREATE TABLE IF NOT EXISTS contacts (
               node_id      BLOB PRIMARY KEY,
               pubkey       BLOB NOT NULL,
               display_name TEXT NOT NULL DEFAULT '',
               state        INTEGER NOT NULL,
               added_ms     INTEGER NOT NULL,
               last_seen_ms INTEGER NOT NULL DEFAULT 0
             );
             CREATE TABLE IF NOT EXISTS dm_messages (
               msg_id   BLOB PRIMARY KEY,
               peer     BLOB NOT NULL,
               author   BLOB NOT NULL,
               lamport  INTEGER NOT NULL,
               sent_ms  INTEGER NOT NULL,
               kind     INTEGER NOT NULL,
               body     BLOB NOT NULL,
               acked    INTEGER NOT NULL DEFAULT 0,
               deleted  INTEGER NOT NULL DEFAULT 0,
               edited   BLOB
             );
             CREATE INDEX IF NOT EXISTS dm_by_peer
               ON dm_messages(peer, lamport);
             CREATE TABLE IF NOT EXISTS reactions (
               msg_id BLOB NOT NULL,
               author BLOB NOT NULL,
               emoji  TEXT NOT NULL,
               PRIMARY KEY (msg_id, author, emoji)
             );
             CREATE TABLE IF NOT EXISTS read_marks (
               peer  BLOB PRIMARY KEY,
               up_to BLOB NOT NULL
             );
             CREATE TABLE IF NOT EXISTS group_ops (
               op_id    BLOB PRIMARY KEY,
               group_id BLOB NOT NULL,
               lamport  INTEGER NOT NULL,
               wall_ms  INTEGER NOT NULL,
               author   BLOB NOT NULL,
               kind     INTEGER NOT NULL,
               body     BLOB NOT NULL,
               sig      BLOB NOT NULL
             );
             CREATE INDEX IF NOT EXISTS ops_by_group
               ON group_ops(group_id, lamport);
             CREATE TABLE IF NOT EXISTS group_messages (
               msg_id     BLOB PRIMARY KEY,
               group_id   BLOB NOT NULL,
               channel_id BLOB NOT NULL,
               author     BLOB NOT NULL,
               lamport    INTEGER NOT NULL,
               sent_ms    INTEGER NOT NULL,
               kind       INTEGER NOT NULL,
               body       BLOB NOT NULL,
               deleted    INTEGER NOT NULL DEFAULT 0,
               edited     BLOB
             );
             CREATE INDEX IF NOT EXISTS gmsg_by_channel
               ON group_messages(group_id, channel_id, lamport);
             CREATE TABLE IF NOT EXISTS msg_attachments (
               msg_id      BLOB NOT NULL,
               position    INTEGER NOT NULL,
               merkle_root BLOB NOT NULL,
               name        TEXT NOT NULL,
               size        INTEGER NOT NULL,
               mime        TEXT NOT NULL,
               PRIMARY KEY (msg_id, position)
             );
             CREATE TABLE IF NOT EXISTS group_keys (
               group_id  BLOB NOT NULL,
               key_epoch INTEGER NOT NULL,
               key       BLOB NOT NULL,
               PRIMARY KEY (group_id, key_epoch)
             );
             CREATE TABLE IF NOT EXISTS outbox (
               id              INTEGER PRIMARY KEY AUTOINCREMENT,
               dest            BLOB NOT NULL,
               payload         BLOB NOT NULL,
               created_ms      INTEGER NOT NULL,
               next_attempt_ms INTEGER NOT NULL,
               attempts        INTEGER NOT NULL DEFAULT 0,
               mailboxed       INTEGER NOT NULL DEFAULT 0,
               mailboxed_day   INTEGER NOT NULL DEFAULT 0
             );
             CREATE INDEX IF NOT EXISTS outbox_due
               ON outbox(next_attempt_ms);
             -- Migration v10 : `outbox_for` (reconnexion d'un pair) et
             -- `outbox_dests` filtrent/groupent par destinataire — sans cet
             -- index, chaque ouverture de conversation balayait toute la
             -- file. (dest, created_ms) couvre l'égalité ET l'ordre de tri.
             CREATE INDEX IF NOT EXISTS outbox_by_dest
               ON outbox(dest, created_ms);
             CREATE TABLE IF NOT EXISTS files (
               merkle_root BLOB PRIMARY KEY,
               name        TEXT NOT NULL,
               size        INTEGER NOT NULL,
               mime        TEXT NOT NULL,
               manifest    BLOB NOT NULL,
               path        TEXT,
               bitmap      BLOB NOT NULL,
               complete    INTEGER NOT NULL DEFAULT 0,
               added_ms    INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS file_fetches (
               merkle_root     BLOB PRIMARY KEY,
               hint            BLOB,
               added_ms        INTEGER NOT NULL,
               next_attempt_ms INTEGER NOT NULL DEFAULT 0,
               attempts        INTEGER NOT NULL DEFAULT 0,
               media_cap       INTEGER
             );
             CREATE TABLE IF NOT EXISTS search_index (
               token  BLOB NOT NULL,
               msg_id BLOB NOT NULL,
               PRIMARY KEY (token, msg_id)
             );
             -- La recherche interroge par `token` (tête de la PK, couvert),
             -- mais la suppression/réindexation d'un message purge par
             -- `msg_id` (DELETE ... WHERE msg_id) — hors tête de PK : sans cet
             -- index, chaque suppression scanne tout l'index de recherche, qui
             -- porte un token par mot de chaque message.
             CREATE INDEX IF NOT EXISTS search_by_msg
               ON search_index(msg_id);
             CREATE TABLE IF NOT EXISTS dm_pins (
               peer   BLOB NOT NULL,
               msg_id BLOB NOT NULL,
               PRIMARY KEY (peer, msg_id)
             );
             CREATE TABLE IF NOT EXISTS mentions (
               msg_id  BLOB PRIMARY KEY,
               scope   INTEGER NOT NULL,
               conv_a  BLOB NOT NULL,
               conv_b  BLOB,
               author  BLOB NOT NULL,
               ts_ms   INTEGER NOT NULL,
               lamport INTEGER NOT NULL,
               snippet TEXT NOT NULL,
               read    INTEGER NOT NULL DEFAULT 0
             );
             CREATE INDEX IF NOT EXISTS mentions_by_ts
               ON mentions(ts_ms);
             CREATE INDEX IF NOT EXISTS mentions_by_conv
               ON mentions(scope, conv_a, read);
             CREATE TABLE IF NOT EXISTS contact_notes (
               pubkey BLOB PRIMARY KEY,
               note   TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS group_membership_local (
               group_id BLOB PRIMARY KEY,
               state    INTEGER NOT NULL DEFAULT 0
             );
             CREATE TABLE IF NOT EXISTS group_invites_incoming (
               group_id    BLOB NOT NULL,
               invite_id   BLOB NOT NULL,
               group_name  TEXT NOT NULL,
               inviter     BLOB NOT NULL,
               secret      BLOB NOT NULL,
               expires_ms  INTEGER NOT NULL,
               received_ms INTEGER NOT NULL,
               PRIMARY KEY (group_id, invite_id)
             );
             -- Porte de consentement (D-045) : avant cette version, un
             -- op-log de groupe présent en base signifiait déjà « rejoint »
             -- (l'ancien flux d'invitation poussait tout sans consentement).
             -- Migration ascendante : tout groupe déjà connu à cette date
             -- reste visible (aucune régression pour les utilisateurs
             -- existants) ; les groupes découverts après cette migration
             -- exigent, eux, une invitation acceptée localement.
             INSERT OR IGNORE INTO group_membership_local (group_id, state)
               SELECT DISTINCT group_id, 2 FROM group_ops;
             -- Suivi LOCAL (non répliqué) du mode lent par salon : dernier
             -- envoi ACCEPTÉ par (salon, auteur), horodaté par l'horloge
             -- locale du nœud (jamais `sent_ms` auto-déclaré par l'auteur) —
             -- voir accord_core::group::msg::check_slowmode. Borné par
             -- construction (au plus un couple salon×membre actif) et
             -- réélagué après chaque repli de l'op-log (salon supprimé ou
             -- auteur n'étant plus membre).
             CREATE TABLE IF NOT EXISTS group_slowmode (
               group_id   BLOB NOT NULL,
               channel_id BLOB NOT NULL,
               author     BLOB NOT NULL,
               last_ms    INTEGER NOT NULL,
               PRIMARY KEY (group_id, channel_id, author)
             );
             PRAGMA user_version = 10;
             COMMIT;",
        )?;
        Ok(())
    }

    /// Accès brut (réservé aux sous-modules du stockage).
    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }

    /// État de groupe en cache, s'il est encore valide.
    pub(crate) fn group_cache_state(
        &self,
        group_id: &[u8; 16],
    ) -> Option<std::sync::Arc<crate::group::GroupState>> {
        self.group_cache
            .lock()
            .ok()?
            .get(group_id)
            .and_then(|e| e.state.clone())
    }

    /// Mémorise l'état replié d'un groupe.
    pub(crate) fn group_cache_put_state(
        &self,
        group_id: [u8; 16],
        state: std::sync::Arc<crate::group::GroupState>,
    ) {
        if let Ok(mut cache) = self.group_cache.lock() {
            cache.entry(group_id).or_default().state = Some(state);
        }
    }

    /// Offre de synchronisation en cache, si elle est encore valide.
    pub(crate) fn group_cache_offer(&self, group_id: &[u8; 16]) -> Option<crate::group::SyncOffer> {
        self.group_cache.lock().ok()?.get(group_id)?.offer
    }

    /// Mémorise l'offre de synchronisation d'un groupe.
    pub(crate) fn group_cache_put_offer(&self, group_id: [u8; 16], offer: crate::group::SyncOffer) {
        if let Ok(mut cache) = self.group_cache.lock() {
            cache.entry(group_id).or_default().offer = Some(offer);
        }
    }

    /// Invalide le cache d'un groupe (log modifié).
    pub(crate) fn group_cache_invalidate(&self, group_id: &[u8; 16]) {
        if let Ok(mut cache) = self.group_cache.lock() {
            cache.remove(group_id);
        }
    }

    // ---- Métadonnées ----

    /// Écrit une métadonnée clé/valeur.
    pub fn set_meta(&self, key: &str, value: &[u8]) -> Result<(), CoreError> {
        self.conn.execute(
            "INSERT INTO meta(key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            rusqlite::params![key, value],
        )?;
        Ok(())
    }

    /// Lit une métadonnée.
    pub fn meta(&self, key: &str) -> Result<Option<Vec<u8>>, CoreError> {
        let mut stmt = self.conn.prepare("SELECT value FROM meta WHERE key = ?1")?;
        let mut rows = stmt.query([key])?;
        match rows.next()? {
            Some(row) => Ok(Some(row.get(0)?)),
            None => Ok(None),
        }
    }

    /// Supprime une métadonnée. Idempotent — sans effet si la clé est absente.
    pub fn del_meta(&self, key: &str) -> Result<(), CoreError> {
        self.conn
            .execute("DELETE FROM meta WHERE key = ?1", [key])?;
        Ok(())
    }

    // ---- Horloge de Lamport persistante ----

    /// Valeur courante de l'horloge de Lamport.
    pub fn lamport(&self) -> Result<u64, CoreError> {
        Ok(self
            .meta("lamport")?
            .and_then(|v| v.try_into().ok().map(u64::from_be_bytes))
            .unwrap_or(0))
    }

    /// Incrémente l'horloge (émission) : `max(locale, observée) + 1`.
    pub fn bump_lamport(&self, observed: u64) -> Result<u64, CoreError> {
        let next = self.lamport()?.max(observed) + 1;
        self.set_meta("lamport", &next.to_be_bytes())?;
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(b: u8) -> [u8; 32] {
        [b; 32]
    }

    #[test]
    fn schema_creates_and_meta_roundtrips() {
        let db = Db::open_in_memory(&key(1)).unwrap();
        db.set_meta("k", b"v1").unwrap();
        db.set_meta("k", b"v2").unwrap();
        assert_eq!(db.meta("k").unwrap().as_deref(), Some(&b"v2"[..]));
        assert_eq!(db.meta("absent").unwrap(), None);
    }

    #[test]
    fn lamport_is_monotonic_and_merges_observed() {
        let db = Db::open_in_memory(&key(1)).unwrap();
        assert_eq!(db.bump_lamport(0).unwrap(), 1);
        assert_eq!(db.bump_lamport(0).unwrap(), 2);
        assert_eq!(db.bump_lamport(100).unwrap(), 101);
        assert_eq!(db.bump_lamport(0).unwrap(), 102);
    }

    #[test]
    fn migration_marks_pre_existing_groups_as_joined() {
        // Simule une base au schéma v4 (pré-consentement) : la table
        // `group_ops` existe et porte un groupe déjà matérialisé par
        // l'ancien flux de force-join, mais `group_membership_local`
        // n'existe pas encore.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("core.db");
        let db_key = key(9);
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(&format!("PRAGMA key = \"x'{}'\";", hex_key(&db_key)))
                .unwrap();
            conn.query_row("SELECT count(*) FROM sqlite_master", [], |_| Ok(()))
                .unwrap();
            conn.execute_batch(
                "BEGIN;
                 CREATE TABLE group_ops (
                   op_id    BLOB PRIMARY KEY,
                   group_id BLOB NOT NULL,
                   lamport  INTEGER NOT NULL,
                   wall_ms  INTEGER NOT NULL,
                   author   BLOB NOT NULL,
                   kind     INTEGER NOT NULL,
                   body     BLOB NOT NULL,
                   sig      BLOB NOT NULL
                 );
                 PRAGMA user_version = 4;
                 COMMIT;",
            )
            .unwrap();
            conn.execute(
                "INSERT INTO group_ops (op_id, group_id, lamport, wall_ms, author, kind, body, sig)
                 VALUES (?1, ?2, 1, 0, ?3, 1, x'', ?4)",
                rusqlite::params![[1u8; 16], [2u8; 16], [3u8; 32], [0u8; 64]],
            )
            .unwrap();
        }
        // Réouverture avec le binaire courant : la migration ascendante vers
        // SCHEMA_VERSION doit créer les nouvelles tables et marquer le
        // groupe préexistant comme rejoint (aucune régression pour un
        // utilisateur existant).
        let db = Db::open(&path, &db_key).unwrap();
        assert_eq!(db.group_ids().unwrap(), vec![[2u8; 16]]);
        assert_eq!(
            db.group_membership(&[2u8; 16]).unwrap(),
            LocalMembership::Joined
        );
    }

    #[test]
    fn migration_v7_conserve_les_intentions_de_telechargement_existantes() {
        // Simule une base au schéma v6 : `file_fetches` sans les colonnes de
        // backoff, avec une intention déjà persistée.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("core.db");
        let db_key = key(9);
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(&format!("PRAGMA key = \"x'{}'\";", hex_key(&db_key)))
                .unwrap();
            conn.query_row("SELECT count(*) FROM sqlite_master", [], |_| Ok(()))
                .unwrap();
            conn.execute_batch(
                "BEGIN;
                 CREATE TABLE file_fetches (
                   merkle_root BLOB PRIMARY KEY,
                   hint        BLOB,
                   added_ms    INTEGER NOT NULL
                 );
                 PRAGMA user_version = 6;
                 COMMIT;",
            )
            .unwrap();
            conn.execute(
                "INSERT INTO file_fetches (merkle_root, hint, added_ms) VALUES (?1, ?2, 5)",
                rusqlite::params![[1u8; 32], [9u8; 32]],
            )
            .unwrap();
        }
        // Réouverture avec le binaire courant : les colonnes sont ajoutées et
        // l'ancienne ligne décode avec les valeurs par défaut (relance
        // immédiate, aucun abandon compté).
        let db = Db::open(&path, &db_key).unwrap();
        let intents = db.file_fetches().unwrap();
        assert_eq!(intents.len(), 1);
        assert_eq!(intents[0].merkle_root, [1u8; 32]);
        assert_eq!(intents[0].hint, Some([9u8; 32]));
        assert_eq!(intents[0].next_attempt_ms, 0);
        assert_eq!(intents[0].attempts, 0);
        // Réouverture idempotente (version déjà à jour) : rien ne casse.
        drop(db);
        assert!(Db::open(&path, &db_key).is_ok());
    }

    #[test]
    fn wrong_key_refuses_existing_db() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("core.db");
        {
            let db = Db::open(&path, &key(7)).unwrap();
            db.set_meta("k", b"v").unwrap();
        }
        assert!(Db::open(&path, &key(8)).is_err(), "mauvaise clé acceptée");
        let db = Db::open(&path, &key(7)).unwrap();
        assert_eq!(db.meta("k").unwrap().as_deref(), Some(&b"v"[..]));
    }

    #[test]
    fn db_file_is_not_plaintext() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("core.db");
        {
            let db = Db::open(&path, &key(7)).unwrap();
            db.set_meta("marqueur-clair", b"contenu-secret").unwrap();
        }
        let raw = std::fs::read(&path).unwrap();
        assert!(!raw.windows(14).any(|w| w == b"contenu-secret"));
        assert!(!raw.windows(13).any(|w| w == b"SQLite format"));
    }
}
