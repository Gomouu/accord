//! Base locale chiffrée (SQLCipher) : schéma, ouverture par `db_key`,
//! horloge de Lamport persistante et métadonnées (SPEC §2.6).

mod contacts;
mod ephemeral;
mod files;
mod groups;
mod invites;
mod mentions;
mod messages;
mod outbox;
mod reminders;
mod scheduled;
mod search;
mod stats;

pub use contacts::{Contact, ContactState};
pub use files::{FetchIntent, FileEntry};
pub use groups::{LocalMembership, StoredGroupKey};
pub use invites::IncomingInvite;
pub use mentions::{MentionEntry, MentionScope};
pub use messages::{DmRecord, GroupMsgRecord};
pub use outbox::OutboxItem;
pub use reminders::Reminder;
pub use scheduled::ScheduledMessage;
pub use stats::StorageStats;

use crate::error::CoreError;
use rusqlite::Connection;
use std::collections::HashMap;
use std::path::Path;

/// Version de schéma courante (migrations linéaires).
///
/// Le lot de création est entièrement idempotent (`IF NOT EXISTS`) : monter
/// la version suffit pour créer les nouvelles tables sur une base existante.
/// Modifier des colonnes existantes exige en revanche une vraie migration —
/// voir [`MIGRATIONS`].
const SCHEMA_VERSION: i64 = 12;

/// Version au-delà de laquelle les évolutions passent par [`MIGRATIONS`].
///
/// Tout ce qui précède est reconstitué par le lot idempotent de `bootstrap` :
/// ce socle correspond à un historique de versions dont il n'existe plus de
/// base réelle isolable, et le réécrire en étapes numérotées serait un risque
/// sans contrepartie. À partir d'ici, chaque évolution est une étape
/// explicite, appliquée en séquence et dans une transaction unique.
const BASELINE_VERSION: i64 = 12;

/// Nombre de sauvegardes pré-migration conservées par base.
const KEPT_BACKUPS: usize = 3;

/// Suffixe des sauvegardes automatiques prises avant une migration.
const BACKUP_SUFFIX: &str = ".premigration.bak";

/// Une étape de migration : fait passer le schéma à `to` depuis `to - 1`.
struct Migration {
    /// Version atteinte une fois l'étape appliquée.
    to: i64,
    /// Description courte, journalisée (jamais de contenu utilisateur).
    label: &'static str,
    /// Application de l'étape. Exécutée dans la transaction commune ; toute
    /// erreur annule l'ensemble des étapes en attente.
    apply: fn(&Connection) -> Result<(), CoreError>,
}

/// Étapes de migration au-delà de [`BASELINE_VERSION`], dans l'ordre.
///
/// 🔒 Une étape publiée ne se modifie plus : des bases l'ont déjà appliquée.
/// Une correction se fait par une étape suivante.
const MIGRATIONS: &[Migration] = &[];

/// Copie la base avant une migration, et purge les copies les plus anciennes.
///
/// Le WAL est d'abord replié dans le fichier principal (`TRUNCATE`), sans quoi
/// la copie manquerait les derniers commits. La copie est faite AVANT toute
/// écriture de schéma ; si elle échoue, la migration n'a pas lieu — mieux vaut
/// un démarrage en erreur qu'une migration sans filet.
///
/// La purge, elle, est au mieux : ne pas réussir à effacer une vieille copie
/// n'est pas une raison d'empêcher l'utilisateur d'ouvrir son application.
fn backup_before_migration(conn: &Connection, path: &Path) -> Result<(), CoreError> {
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{stamp}{BACKUP_SUFFIX}"));
    let target = path.with_file_name(&name);
    std::fs::copy(path, &target)?;
    tracing::info!("sauvegarde de la base avant migration du schéma");
    purge_old_backups(path);
    Ok(())
}

/// Ne conserve que les [`KEPT_BACKUPS`] sauvegardes les plus récentes de cette
/// base. Silencieuse : toute erreur d'E/S est ignorée.
fn purge_old_backups(path: &Path) {
    let Some(dir) = path.parent() else { return };
    let Some(stem) = path.file_name().and_then(|n| n.to_str()) else {
        return;
    };
    let prefix = format!("{stem}.");
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut backups: Vec<std::path::PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(&prefix) && n.ends_with(BACKUP_SUFFIX))
        })
        .collect();
    if backups.len() <= KEPT_BACKUPS {
        return;
    }
    // L'horodatage millisecondes est de largeur fixe sur toute période
    // plausible : l'ordre lexicographique des noms est l'ordre chronologique.
    backups.sort();
    for stale in &backups[..backups.len() - KEPT_BACKUPS] {
        let _ = std::fs::remove_file(stale);
    }
}

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
        Self::init(conn, db_key, Some(path))
    }

    /// Base en mémoire (tests). La clé est appliquée mais sans effet durable.
    pub fn open_in_memory(db_key: &[u8; 32]) -> Result<Self, CoreError> {
        let conn = Connection::open_in_memory()?;
        Self::init(conn, db_key, None)
    }

    fn init(conn: Connection, db_key: &[u8; 32], path: Option<&Path>) -> Result<Self, CoreError> {
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
        db.migrate(path)?;
        Ok(db)
    }

    /// Version de schéma enregistrée dans la base (0 pour une base neuve).
    fn schema_version(&self) -> Result<i64, CoreError> {
        Ok(self
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))?)
    }

    /// Vrai si la table existe dans le schéma courant.
    #[cfg(test)]
    fn has_table(&self, table: &str) -> Result<bool, CoreError> {
        let n: i64 = self.conn.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [table],
            |r| r.get(0),
        )?;
        Ok(n > 0)
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

    /// Amène le schéma à [`SCHEMA_VERSION`].
    ///
    /// Trois garanties, dans cet ordre :
    ///
    /// 1. **Refus de rétrograder.** Une base écrite par une version plus
    ///    récente n'est pas ouverte : le binaire courant ne sait pas ce que
    ///    contiennent ses tables, et écrire dedans détruirait des données.
    /// 2. **Sauvegarde avant écriture.** Une base existante qui doit bouger
    ///    est copiée d'abord ; l'échec de la copie annule la migration plutôt
    ///    que de la faire sans filet.
    /// 3. **Tout ou rien.** Les étapes numérotées s'appliquent dans une
    ///    transaction unique : une erreur au milieu laisse le schéma tel qu'il
    ///    était, jamais à moitié migré.
    fn migrate(&self, path: Option<&Path>) -> Result<(), CoreError> {
        let version = self.schema_version()?;
        if version > SCHEMA_VERSION {
            return Err(CoreError::SchemaTooNew {
                found: version,
                supported: SCHEMA_VERSION,
            });
        }
        if version == SCHEMA_VERSION {
            return Ok(());
        }
        // Une base neuve (version 0) n'a rien à sauvegarder.
        if version > 0 {
            if let Some(path) = path {
                backup_before_migration(&self.conn, path)?;
            }
        }
        if version < BASELINE_VERSION {
            self.bootstrap()?;
        }
        self.apply_migrations(version.max(BASELINE_VERSION))
    }

    /// Applique les étapes numérotées postérieures à `from`, dans l'ordre et
    /// dans une transaction unique.
    fn apply_migrations(&self, from: i64) -> Result<(), CoreError> {
        self.apply_migration_list(from, MIGRATIONS)
    }

    /// Cœur de [`Self::apply_migrations`], paramétré par le registre pour que
    /// les tests puissent éprouver le rollback sur des étapes fabriquées.
    fn apply_migration_list(&self, from: i64, migrations: &[Migration]) -> Result<(), CoreError> {
        let pending: Vec<&Migration> = migrations.iter().filter(|m| m.to > from).collect();
        if pending.is_empty() {
            return Ok(());
        }
        self.conn.execute_batch("BEGIN;")?;
        for migration in pending {
            tracing::info!(
                vers = migration.to,
                etape = migration.label,
                "migration du schéma local"
            );
            let step = (migration.apply)(&self.conn).and_then(|()| {
                self.conn
                    .execute_batch(&format!("PRAGMA user_version = {};", migration.to))
                    .map_err(CoreError::from)
            });
            if let Err(e) = step {
                // Le rollback ramène le schéma ET la version enregistrée à
                // leur état d'origine : rien n'est appliqué à moitié.
                let _ = self.conn.execute_batch("ROLLBACK;");
                return Err(e);
            }
        }
        self.conn.execute_batch("COMMIT;")?;
        Ok(())
    }

    /// Socle idempotent : reconstitue le schéma jusqu'à [`BASELINE_VERSION`].
    fn bootstrap(&self) -> Result<(), CoreError> {
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
        // Migration v11 (Lot E1): per-contact manual identity verification.
        // Nullable, additive — existing rows stay unverified. The pubkey seen
        // at verification time is stored so a later key substitution can be
        // detected ("verification broken" warning).
        if self.has_column("contacts", "node_id")? && !self.has_column("contacts", "verified_at")? {
            self.conn.execute_batch(
                "ALTER TABLE contacts ADD COLUMN verified_at INTEGER;
                 ALTER TABLE contacts ADD COLUMN verified_pubkey BLOB;",
            )?;
        }
        self.conn.execute_batch(
            "BEGIN;
             CREATE TABLE IF NOT EXISTS meta (
               key   TEXT PRIMARY KEY,
               value BLOB NOT NULL
             );
             CREATE TABLE IF NOT EXISTS contacts (
               node_id         BLOB PRIMARY KEY,
               pubkey          BLOB NOT NULL,
               display_name    TEXT NOT NULL DEFAULT '',
               state           INTEGER NOT NULL,
               added_ms        INTEGER NOT NULL,
               last_seen_ms    INTEGER NOT NULL DEFAULT 0,
               verified_at     INTEGER,
               verified_pubkey BLOB
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
             -- Lot E2: per-conversation disappearing-message timer, honoured
             -- LOCALLY only (no wire negotiation). `scope` is the peer pubkey
             -- (32 bytes, DM) or the group_id (16 bytes); no row = disabled.
             CREATE TABLE IF NOT EXISTS conversation_ephemeral (
               scope    BLOB PRIMARY KEY,
               ttl_secs INTEGER NOT NULL
             );
             -- Lot F1: locally scheduled messages (deferred send). Purely
             -- local — the maintenance loop routes a due row through the
             -- normal send path (outbox covers offline peers), then deletes
             -- it. `scope` is 'dm' or 'group'; `scope_id` the peer pubkey
             -- (32) or group_id (16); `channel_id` the group channel (16),
             -- NULL for a DM.
             CREATE TABLE IF NOT EXISTS scheduled_messages (
               id         BLOB PRIMARY KEY,
               scope      TEXT NOT NULL,
               scope_id   BLOB NOT NULL,
               channel_id BLOB,
               body       TEXT NOT NULL,
               fire_at    INTEGER NOT NULL,
               created_at INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS scheduled_by_fire
               ON scheduled_messages(fire_at);
             -- Lot F2: local reminders pinned on a message. Purely local — a
             -- due row emits `event.reminder` once (`fired_at` set), the user
             -- later dismisses it (row removed). `msg_ref` is the referenced
             -- message id (16), NULL for a free-standing reminder.
             CREATE TABLE IF NOT EXISTS reminders (
               id         BLOB PRIMARY KEY,
               scope      TEXT NOT NULL,
               scope_id   BLOB NOT NULL,
               msg_ref    BLOB,
               note       TEXT NOT NULL DEFAULT '',
               fire_at    INTEGER NOT NULL,
               fired_at   INTEGER,
               created_at INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS reminders_by_fire
               ON reminders(fire_at);",
        )?;
        // Même transaction : la version n'est marquée que si tout le lot a
        // été appliqué.
        self.conn.execute_batch(&format!(
            "PRAGMA user_version = {BASELINE_VERSION};
             COMMIT;"
        ))?;
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
    fn migration_v11_adds_verification_columns_to_existing_contacts() {
        // Simulates a v10 database: `contacts` without the verification
        // columns, with one row already persisted.
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
                 CREATE TABLE contacts (
                   node_id      BLOB PRIMARY KEY,
                   pubkey       BLOB NOT NULL,
                   display_name TEXT NOT NULL DEFAULT '',
                   state        INTEGER NOT NULL,
                   added_ms     INTEGER NOT NULL,
                   last_seen_ms INTEGER NOT NULL DEFAULT 0
                 );
                 PRAGMA user_version = 10;
                 COMMIT;",
            )
            .unwrap();
            conn.execute(
                "INSERT INTO contacts (node_id, pubkey, display_name, state, added_ms)
                 VALUES (?1, ?2, 'ami', 2, 7)",
                rusqlite::params![[1u8; 32], [2u8; 32]],
            )
            .unwrap();
        }
        // Reopening with the current binary adds the columns; the old row
        // reads back unverified and can then be verified.
        let db = Db::open(&path, &db_key).unwrap();
        let c = db.contact(&[1u8; 32]).unwrap().unwrap();
        assert_eq!((c.verified_at, c.verified_pubkey), (None, None));
        db.set_contact_verified(&[1u8; 32], Some((9, [2u8; 32])))
            .unwrap();
        assert_eq!(
            db.contact(&[1u8; 32]).unwrap().unwrap().verified_pubkey,
            Some([2u8; 32])
        );
        // The ephemeral table from the same migration exists and works.
        db.set_conversation_ttl(&[2u8; 32], Some(3600)).unwrap();
        assert_eq!(db.conversation_ttl(&[2u8; 32]).unwrap(), Some(3600));
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

    #[test]
    fn le_registre_de_migrations_est_coherent() {
        // Numéros strictement croissants, démarrant juste après le socle, et
        // la dernière étape définit la version courante. Un registre
        // incohérent laisserait des bases bloquées à mi-chemin.
        let mut attendu = BASELINE_VERSION;
        for m in MIGRATIONS {
            attendu += 1;
            assert_eq!(m.to, attendu, "étape « {} » mal numérotée", m.label);
            assert!(!m.label.is_empty());
        }
        assert_eq!(SCHEMA_VERSION, attendu);
    }

    #[test]
    fn une_base_dune_version_plus_recente_est_refusee() {
        // Rétrogradation : l'utilisateur a réinstallé une version antérieure.
        // Écrire dans un schéma qu'on ne connaît pas détruirait ses données.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("core.db");
        let db_key = key(3);
        Db::open(&path, &db_key).unwrap();
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(&format!("PRAGMA key = \"x'{}'\";", hex_key(&db_key)))
                .unwrap();
            conn.execute_batch(&format!("PRAGMA user_version = {};", SCHEMA_VERSION + 5))
                .unwrap();
        }
        match Db::open(&path, &db_key).err() {
            Some(CoreError::SchemaTooNew { found, supported }) => {
                assert_eq!(found, SCHEMA_VERSION + 5);
                assert_eq!(supported, SCHEMA_VERSION);
            }
            autre => panic!("attendu un refus de rétrogradation, obtenu {autre:?}"),
        }
    }

    #[test]
    fn une_migration_qui_echoue_ne_laisse_rien_a_moitie() {
        let db = Db::open_in_memory(&key(4)).unwrap();
        let avant = db.schema_version().unwrap();
        let etapes = &[
            Migration {
                to: avant + 1,
                label: "table temoin",
                apply: |c| {
                    c.execute_batch("CREATE TABLE temoin_migration (x INTEGER);")?;
                    Ok(())
                },
            },
            Migration {
                to: avant + 2,
                label: "etape qui echoue",
                apply: |_| Err(CoreError::Invalid("échec simulé")),
            },
        ];
        let err = db.apply_migration_list(avant, etapes).unwrap_err();
        assert!(matches!(err, CoreError::Invalid("échec simulé")));
        // Ni la table de la première étape, ni la version, ne subsistent.
        assert_eq!(db.schema_version().unwrap(), avant);
        assert!(!db.has_table("temoin_migration").unwrap());
        // La base reste utilisable après l'échec.
        assert!(db.contacts().is_ok());
    }

    #[test]
    fn les_migrations_sappliquent_en_sequence() {
        let db = Db::open_in_memory(&key(5)).unwrap();
        let avant = db.schema_version().unwrap();
        let etapes = &[
            Migration {
                to: avant + 1,
                label: "premiere",
                apply: |c| {
                    c.execute_batch("CREATE TABLE etape_un (x INTEGER);")?;
                    Ok(())
                },
            },
            Migration {
                to: avant + 2,
                label: "seconde",
                apply: |c| {
                    // Dépend de la précédente : prouve l'ordre d'application.
                    c.execute_batch("ALTER TABLE etape_un ADD COLUMN y INTEGER;")?;
                    Ok(())
                },
            },
        ];
        db.apply_migration_list(avant, etapes).unwrap();
        assert_eq!(db.schema_version().unwrap(), avant + 2);
        assert!(db.has_column("etape_un", "y").unwrap());
        // Ré-appliquer depuis la version atteinte ne fait plus rien.
        db.apply_migration_list(avant + 2, etapes).unwrap();
        assert_eq!(db.schema_version().unwrap(), avant + 2);
    }

    #[test]
    fn une_migration_sauvegarde_la_base_et_purge_les_anciennes_copies() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("core.db");
        let db_key = key(6);
        // Base « ancienne » : schéma minimal, version antérieure au socle.
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(&format!("PRAGMA key = \"x'{}'\";", hex_key(&db_key)))
                .unwrap();
            conn.query_row("SELECT count(*) FROM sqlite_master", [], |_| Ok(()))
                .unwrap();
            conn.execute_batch(
                "BEGIN; CREATE TABLE vieux (x INTEGER); PRAGMA user_version = 1; COMMIT;",
            )
            .unwrap();
        }
        let db = Db::open(&path, &db_key).unwrap();
        assert_eq!(db.schema_version().unwrap(), SCHEMA_VERSION);
        drop(db);
        assert_eq!(compte_sauvegardes(dir.path()), 1);

        // Plusieurs migrations successives : au plus KEPT_BACKUPS copies.
        for _ in 0..KEPT_BACKUPS + 2 {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(&format!("PRAGMA key = \"x'{}'\";", hex_key(&db_key)))
                .unwrap();
            conn.execute_batch("PRAGMA user_version = 1;").unwrap();
            drop(conn);
            // L'horodatage est en millisecondes : sans attente, deux
            // sauvegardes d'affilée porteraient le même nom.
            std::thread::sleep(std::time::Duration::from_millis(2));
            Db::open(&path, &db_key).unwrap();
        }
        assert_eq!(compte_sauvegardes(dir.path()), KEPT_BACKUPS);
    }

    fn compte_sauvegardes(dir: &std::path::Path) -> usize {
        std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .filter(|e| {
                e.file_name()
                    .to_str()
                    .is_some_and(|n| n.ends_with(BACKUP_SUFFIX))
            })
            .count()
    }
}
