//! Persistance des groupes : op-log répliqué, clés d'époque, appartenance
//! locale (porte de consentement, non répliquée) et suivi local du mode
//! lent par salon (non répliqué — voir `accord_core::group::msg`).

use super::{blob, Db};
use crate::error::CoreError;
use accord_proto::core_msg::GroupOp;
use rusqlite::params;
use std::collections::BTreeSet;

/// Clé de groupe persistée pour un epoch donné.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredGroupKey {
    /// Epoch de la clé.
    pub key_epoch: u32,
    /// Clé symétrique 32 octets (protégée au repos par SQLCipher).
    pub key: [u8; 32],
}

/// État d'appartenance **locale** (non répliquée) à un groupe : contrôle
/// exclusivement l'affichage local ([`Db::group_ids`]) et l'acceptation d'un
/// op-log ou d'une clé poussés par un pair. Un pair malveillant qui pousse un
/// op-log complet sans invitation acceptée localement ne peut donc plus faire
/// apparaître un groupe comme rejoint (D-045, ex force-join).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalMembership {
    /// Aucune trace locale d'intention de rejoindre : tout op-log ou clé
    /// reçus pour ce groupe sont ignorés (quarantaine implicite).
    None,
    /// Invitation acceptée localement (`InviteAccept` envoyée) ; op-log et
    /// clé pas encore reçus.
    Accepted,
    /// Groupe matérialisé : au moins une op reçue pendant que l'état était
    /// `Accepted` (ou groupe créé/fondé localement) — visible comme serveur.
    Joined,
}

impl LocalMembership {
    fn from_i64(v: i64) -> Self {
        match v {
            2 => Self::Joined,
            1 => Self::Accepted,
            _ => Self::None,
        }
    }

    fn to_i64(self) -> i64 {
        match self {
            Self::None => 0,
            Self::Accepted => 1,
            Self::Joined => 2,
        }
    }
}

impl Db {
    /// Insère une opération dans le journal ; `false` si déjà connue.
    /// La validation (signature, permissions) relève de [`crate::group`].
    pub fn insert_group_op(&self, op: &GroupOp) -> Result<bool, CoreError> {
        let n = self.conn().execute(
            "INSERT OR IGNORE INTO group_ops
               (op_id, group_id, lamport, wall_ms, author, kind, body, sig)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                op.op_id,
                op.group_id,
                op.lamport,
                op.wall_ms,
                op.author,
                op.kind,
                op.body,
                op.sig
            ],
        )?;
        if n > 0 {
            // Étend l'état en cache quand c'est démontrablement équivalent à un
            // repli complet, invalide sinon — voir `Db::group_cache_extend`.
            self.group_cache_extend(op);
        }
        Ok(n > 0)
    }

    /// Colonnes projetées d'une op, dans l'ordre attendu par [`Self::ops_rows`].
    const OP_COLS: &'static str = "op_id, group_id, lamport, wall_ms, author, kind, body, sig";

    /// Exécute une requête projetant [`Self::OP_COLS`] et matérialise les ops.
    fn ops_rows(
        &self,
        sql: &str,
        args: &[&dyn rusqlite::ToSql],
    ) -> Result<Vec<GroupOp>, CoreError> {
        let mut stmt = self.conn().prepare_cached(sql)?;
        let raws = stmt
            .query_map(args, |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, u64>(2)?,
                    row.get::<_, u64>(3)?,
                    row.get::<_, Vec<u8>>(4)?,
                    row.get::<_, u8>(5)?,
                    row.get::<_, Vec<u8>>(6)?,
                    row.get::<_, Vec<u8>>(7)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        raws.into_iter()
            .map(|r| {
                Ok(GroupOp {
                    op_id: blob(r.0)?,
                    group_id: blob(r.1)?,
                    lamport: r.2,
                    wall_ms: r.3,
                    author: blob(r.4)?,
                    kind: r.5,
                    body: r.6,
                    sig: blob::<64>(r.7)?,
                })
            })
            .collect()
    }

    /// Journal complet d'un groupe dans l'ordre total `(lamport, author)`.
    pub fn group_ops(&self, group_id: &[u8; 16]) -> Result<Vec<GroupOp>, CoreError> {
        self.ops_rows(
            &format!(
                "SELECT {} FROM group_ops WHERE group_id = ?1
                 ORDER BY lamport ASC, author ASC",
                Self::OP_COLS
            ),
            &[&group_id.as_slice()],
        )
    }

    /// Lamport maximal connu et nombre d'ops (pour l'anti-entropie §6.2).
    pub fn group_op_summary(&self, group_id: &[u8; 16]) -> Result<(u64, u64), CoreError> {
        Ok(self.conn().query_row(
            "SELECT COALESCE(MAX(lamport), 0), COUNT(*) FROM group_ops WHERE group_id = ?1",
            [group_id.as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?)
    }

    /// Ops d'un groupe strictement au-delà d'un lamport (rattrapage pair).
    ///
    /// Le filtre est POSÉ EN SQL et non appliqué après coup : l'index
    /// `ops_by_group(group_id, lamport)` le résout sans lire le reste du
    /// journal. La version précédente chargeait TOUT l'op-log — corps et
    /// signatures compris — pour n'en garder que la queue, à chaque réponse
    /// d'anti-entropie d'un pair déjà presque à jour.
    pub fn group_ops_after(
        &self,
        group_id: &[u8; 16],
        after_lamport: u64,
    ) -> Result<Vec<GroupOp>, CoreError> {
        self.ops_rows(
            &format!(
                "SELECT {} FROM group_ops WHERE group_id = ?1 AND lamport > ?2
                 ORDER BY lamport ASC, author ASC",
                Self::OP_COLS
            ),
            &[
                &group_id.as_slice(),
                &(after_lamport.min(i64::MAX as u64) as i64),
            ],
        )
    }

    /// Vrai si le journal du groupe porte au moins une op de ce genre.
    ///
    /// Un `EXISTS` s'arrête à la première ligne : répondre « ce groupe a-t-il
    /// une CREATE ? » ne coûte plus la lecture et le décodage de tout l'op-log,
    /// ce que faisait l'appelant à chaque offre d'anti-entropie reçue.
    pub(crate) fn group_has_op_kind(
        &self,
        group_id: &[u8; 16],
        kind: u8,
    ) -> Result<bool, CoreError> {
        let mut stmt = self.conn().prepare_cached(
            "SELECT EXISTS (SELECT 1 FROM group_ops WHERE group_id = ?1 AND kind = ?2)",
        )?;
        Ok(stmt.query_row(params![group_id.as_slice(), kind], |row| {
            row.get::<_, i64>(0)
        })? != 0)
    }

    /// Identifiants des groupes **rejoints localement** (état `Joined` de
    /// [`LocalMembership`]) : un op-log présent en base sans intention locale
    /// de rejoindre (ancien force-join, ou invitation jamais acceptée) reste
    /// invisible ici.
    pub fn group_ids(&self) -> Result<Vec<[u8; 16]>, CoreError> {
        let mut stmt = self.conn().prepare(
            "SELECT DISTINCT g.group_id FROM group_ops g
             JOIN group_membership_local m ON m.group_id = g.group_id
             WHERE m.state = 2",
        )?;
        let raws = stmt
            .query_map([], |row| row.get::<_, Vec<u8>>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        raws.into_iter().map(blob).collect()
    }

    /// Fixe l'état d'appartenance locale d'un groupe (idempotent).
    pub fn set_group_membership(
        &self,
        group_id: &[u8; 16],
        state: LocalMembership,
    ) -> Result<(), CoreError> {
        self.conn().execute(
            "INSERT INTO group_membership_local (group_id, state) VALUES (?1, ?2)
             ON CONFLICT(group_id) DO UPDATE SET state = excluded.state",
            params![group_id.as_slice(), state.to_i64()],
        )?;
        Ok(())
    }

    /// Rend l'état d'appartenance locale d'un groupe (`None` par défaut,
    /// jamais d'erreur pour un groupe inconnu).
    pub fn group_membership(&self, group_id: &[u8; 16]) -> Result<LocalMembership, CoreError> {
        let mut stmt = self
            .conn()
            .prepare("SELECT state FROM group_membership_local WHERE group_id = ?1")?;
        let mut rows = stmt.query([group_id.as_slice()])?;
        match rows.next()? {
            Some(row) => Ok(LocalMembership::from_i64(row.get(0)?)),
            None => Ok(LocalMembership::None),
        }
    }

    /// Groupes en attente locale (`Accepted`) n'ayant encore AUCUN op-log
    /// matérialisé : candidats à la purge d'une acceptation/rachat fantôme
    /// (lien mort ou révoqué qui n'aboutira jamais). Un groupe portant déjà des
    /// ops en est exclu — il se matérialise par la porte de consentement
    /// normale ([`LocalMembership::Joined`]).
    pub fn accepted_without_ops_group_ids(&self) -> Result<Vec<[u8; 16]>, CoreError> {
        let mut stmt = self.conn().prepare(
            "SELECT m.group_id FROM group_membership_local m
             WHERE m.state = 1
               AND NOT EXISTS (SELECT 1 FROM group_ops g WHERE g.group_id = m.group_id)",
        )?;
        let raws = stmt
            .query_map([], |row| row.get::<_, Vec<u8>>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        raws.into_iter().map(blob).collect()
    }

    /// Efface toute trace d'appartenance locale d'un groupe (retour à `None`
    /// implicite). Idempotent — sans effet si aucune ligne n'existe.
    pub fn clear_group_membership(&self, group_id: &[u8; 16]) -> Result<(), CoreError> {
        self.conn().execute(
            "DELETE FROM group_membership_local WHERE group_id = ?1",
            [group_id.as_slice()],
        )?;
        Ok(())
    }

    // ---- Clés d'époque ----

    /// Stocke la clé d'un epoch (idempotent, premier arrivé conservé).
    pub fn put_group_key(
        &self,
        group_id: &[u8; 16],
        key_epoch: u32,
        key: &[u8; 32],
    ) -> Result<(), CoreError> {
        self.conn().execute(
            "INSERT OR IGNORE INTO group_keys (group_id, key_epoch, key) VALUES (?1, ?2, ?3)",
            params![group_id, key_epoch, key],
        )?;
        Ok(())
    }

    /// Clé d'un epoch donné.
    pub fn group_key(
        &self,
        group_id: &[u8; 16],
        key_epoch: u32,
    ) -> Result<Option<[u8; 32]>, CoreError> {
        let mut stmt = self
            .conn()
            .prepare("SELECT key FROM group_keys WHERE group_id = ?1 AND key_epoch = ?2")?;
        let mut rows = stmt.query(params![group_id, key_epoch])?;
        match rows.next()? {
            Some(row) => Ok(Some(blob(row.get::<_, Vec<u8>>(0)?)?)),
            None => Ok(None),
        }
    }

    /// Clé la plus récente d'un groupe (epoch maximal détenu).
    pub fn latest_group_key(
        &self,
        group_id: &[u8; 16],
    ) -> Result<Option<StoredGroupKey>, CoreError> {
        let mut stmt = self.conn().prepare(
            "SELECT key_epoch, key FROM group_keys WHERE group_id = ?1
             ORDER BY key_epoch DESC LIMIT 1",
        )?;
        let mut rows = stmt.query([group_id.as_slice()])?;
        match rows.next()? {
            Some(row) => Ok(Some(StoredGroupKey {
                key_epoch: row.get(0)?,
                key: blob(row.get::<_, Vec<u8>>(1)?)?,
            })),
            None => Ok(None),
        }
    }

    // ---- Mode lent (suivi LOCAL, non répliqué) ----

    /// Instant local (horloge du RÉCEPTEUR/émetteur — jamais `sent_ms`
    /// auto-déclaré) du dernier message ACCEPTÉ pour ce triplet (groupe,
    /// salon, auteur), le cas échéant.
    pub fn slowmode_last_ms(
        &self,
        group_id: &[u8; 16],
        channel_id: &[u8; 16],
        author: &[u8; 32],
    ) -> Result<Option<u64>, CoreError> {
        let mut stmt = self.conn().prepare(
            "SELECT last_ms FROM group_slowmode
             WHERE group_id = ?1 AND channel_id = ?2 AND author = ?3",
        )?;
        let mut rows = stmt.query(params![
            group_id.as_slice(),
            channel_id.as_slice(),
            author.as_slice()
        ])?;
        match rows.next()? {
            Some(row) => Ok(Some(row.get(0)?)),
            None => Ok(None),
        }
    }

    /// Enregistre `at_ms` comme dernier envoi accepté pour (salon, auteur).
    /// Ne retient jamais qu'un maximum : une livraison désordonnée (P2P,
    /// aucun ordre total entre messages) ne fait jamais reculer l'horloge
    /// suivie.
    pub fn bump_slowmode_last_ms(
        &self,
        group_id: &[u8; 16],
        channel_id: &[u8; 16],
        author: &[u8; 32],
        at_ms: u64,
    ) -> Result<(), CoreError> {
        self.conn().execute(
            "INSERT INTO group_slowmode (group_id, channel_id, author, last_ms)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(group_id, channel_id, author)
             DO UPDATE SET last_ms = MAX(last_ms, excluded.last_ms)",
            params![
                group_id.as_slice(),
                channel_id.as_slice(),
                author.as_slice(),
                at_ms
            ],
        )?;
        Ok(())
    }

    /// Purge les entrées de suivi devenues obsolètes pour un groupe : salon
    /// disparu ou auteur n'étant plus membre. Appelée après chaque repli de
    /// l'op-log ([`crate::group::author_op`]/[`crate::group::ingest_op`])
    /// pour borner la table (au plus un couple salon×membre actif à tout
    /// instant) — même déclencheur que le nettoyage des overrides de salon
    /// sur `DelChannel` et de la modération par membre sur `Kick`/`Ban`/
    /// `Leave`, mais appliqué ici puisque ce suivi vit hors de
    /// `GroupState` (non dérivable du seul op-log).
    pub fn prune_slowmode(
        &self,
        group_id: &[u8; 16],
        valid_channels: &BTreeSet<[u8; 16]>,
        valid_authors: &BTreeSet<[u8; 32]>,
    ) -> Result<(), CoreError> {
        let rows: Vec<(Vec<u8>, Vec<u8>)> = {
            let mut stmt = self.conn().prepare_cached(
                "SELECT channel_id, author FROM group_slowmode WHERE group_id = ?1",
            )?;
            let lues = stmt
                .query_map([group_id.as_slice()], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<Result<_, _>>()?;
            lues
        };
        // Les périmées d'abord, les suppressions ensuite : le curseur de
        // lecture est refermé avant d'écrire, et rien ne dépend de l'ordre.
        let mut perimees: Vec<([u8; 16], [u8; 32])> = Vec::new();
        for (channel_raw, author_raw) in rows {
            let channel_id: [u8; 16] = blob(channel_raw)?;
            let author: [u8; 32] = blob(author_raw)?;
            if !valid_channels.contains(&channel_id) || !valid_authors.contains(&author) {
                perimees.push((channel_id, author));
            }
        }
        if perimees.is_empty() {
            return Ok(());
        }
        // Une transaction et une requête PRÉPARÉE UNE FOIS : cette purge suit
        // chaque repli de l'op-log, donc chaque op ingérée. Recompiler le
        // `DELETE` et valider une transaction implicite par ligne y coûtait
        // plus cher que la suppression elle-même.
        let tx = self.conn().unchecked_transaction()?;
        {
            let mut del = tx.prepare_cached(
                "DELETE FROM group_slowmode
                 WHERE group_id = ?1 AND channel_id = ?2 AND author = ?3",
            )?;
            for (channel_id, author) in &perimees {
                del.execute(params![
                    group_id.as_slice(),
                    channel_id.as_slice(),
                    author.as_slice()
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn op(id: u8, lamport: u64, author: u8) -> GroupOp {
        GroupOp {
            op_id: [id; 16],
            group_id: [1; 16],
            lamport,
            wall_ms: 0,
            author: [author; 32],
            kind: 0x01,
            body: vec![],
            sig: [0; 64],
        }
    }

    #[test]
    fn oplog_orders_by_lamport_then_author() {
        let db = Db::open_in_memory(&[1; 32]).unwrap();
        assert!(db.insert_group_op(&op(1, 5, 9)).unwrap());
        assert!(db.insert_group_op(&op(2, 5, 3)).unwrap());
        assert!(db.insert_group_op(&op(3, 1, 9)).unwrap());
        assert!(!db.insert_group_op(&op(1, 5, 9)).unwrap(), "doublon");
        let ops = db.group_ops(&[1; 16]).unwrap();
        assert_eq!(
            ops.iter().map(|o| o.op_id[0]).collect::<Vec<_>>(),
            vec![3, 2, 1],
            "ordre total (lamport, author)"
        );
        assert_eq!(db.group_op_summary(&[1; 16]).unwrap(), (5, 3));
        assert_eq!(db.group_ops_after(&[1; 16], 1).unwrap().len(), 2);
        // Un op-log présent sans appartenance locale `Joined` reste invisible
        // (porte de consentement, D-045) : c'est le cas par défaut ici.
        assert!(db.group_ids().unwrap().is_empty());
        db.set_group_membership(&[1; 16], LocalMembership::Joined)
            .unwrap();
        assert_eq!(db.group_ids().unwrap(), vec![[1; 16]]);
    }

    #[test]
    fn les_ops_apres_un_lamport_sont_filtrees_en_base() {
        // 🔒 Le filtre est passé de Rust à SQL : il doit rendre EXACTEMENT le
        // même ensemble — borne STRICTE, ordre canonique préservé, et rien
        // d'un autre groupe.
        let db = Db::open_in_memory(&[1; 32]).unwrap();
        for o in [op(1, 5, 9), op(2, 5, 3), op(3, 1, 9), op(4, 7, 1)] {
            db.insert_group_op(&o).unwrap();
        }
        let mut autre = op(5, 9, 9);
        autre.group_id = [2; 16];
        db.insert_group_op(&autre).unwrap();

        for borne in [0u64, 1, 5, 7, u64::MAX] {
            let attendu: Vec<[u8; 16]> = db
                .group_ops(&[1; 16])
                .unwrap()
                .into_iter()
                .filter(|o| o.lamport > borne)
                .map(|o| o.op_id)
                .collect();
            let obtenu: Vec<[u8; 16]> = db
                .group_ops_after(&[1; 16], borne)
                .unwrap()
                .into_iter()
                .map(|o| o.op_id)
                .collect();
            assert_eq!(obtenu, attendu, "borne {borne}");
        }
        // Un lamport ÉGAL à la borne est exclu (borne stricte).
        assert!(db
            .group_ops_after(&[1; 16], 7)
            .unwrap()
            .iter()
            .all(|o| o.lamport > 7));
    }

    #[test]
    fn la_presence_dun_genre_dop_est_bornee_au_groupe() {
        let db = Db::open_in_memory(&[1; 32]).unwrap();
        assert!(!db.group_has_op_kind(&[1; 16], 0x01).unwrap());
        db.insert_group_op(&op(1, 1, 9)).unwrap(); // kind 0x01
        assert!(db.group_has_op_kind(&[1; 16], 0x01).unwrap());
        assert!(!db.group_has_op_kind(&[1; 16], 0x02).unwrap());
        assert!(!db.group_has_op_kind(&[2; 16], 0x01).unwrap());
    }

    #[test]
    fn local_membership_defaults_to_none_and_roundtrips() {
        let db = Db::open_in_memory(&[1; 32]).unwrap();
        assert_eq!(
            db.group_membership(&[4; 16]).unwrap(),
            LocalMembership::None
        );
        db.set_group_membership(&[4; 16], LocalMembership::Accepted)
            .unwrap();
        assert_eq!(
            db.group_membership(&[4; 16]).unwrap(),
            LocalMembership::Accepted
        );
        // L'état est remplacé, pas cumulé.
        db.set_group_membership(&[4; 16], LocalMembership::Joined)
            .unwrap();
        assert_eq!(
            db.group_membership(&[4; 16]).unwrap(),
            LocalMembership::Joined
        );
    }

    #[test]
    fn group_keys_by_epoch() {
        let db = Db::open_in_memory(&[1; 32]).unwrap();
        assert_eq!(db.latest_group_key(&[1; 16]).unwrap(), None);
        db.put_group_key(&[1; 16], 0, &[10; 32]).unwrap();
        db.put_group_key(&[1; 16], 1, &[11; 32]).unwrap();
        // Une réécriture du même epoch ne remplace pas la clé détenue.
        db.put_group_key(&[1; 16], 1, &[99; 32]).unwrap();
        assert_eq!(db.group_key(&[1; 16], 0).unwrap(), Some([10; 32]));
        assert_eq!(
            db.latest_group_key(&[1; 16]).unwrap(),
            Some(StoredGroupKey {
                key_epoch: 1,
                key: [11; 32]
            })
        );
    }

    #[test]
    fn slowmode_tracking_takes_the_max_and_is_scoped_per_channel_and_author() {
        let db = Db::open_in_memory(&[1; 32]).unwrap();
        let gid = [1u8; 16];
        let chan = [2u8; 16];
        let other_chan = [3u8; 16];
        let alice = [9u8; 32];
        let bob = [8u8; 32];

        assert_eq!(db.slowmode_last_ms(&gid, &chan, &alice).unwrap(), None);
        db.bump_slowmode_last_ms(&gid, &chan, &alice, 1_000)
            .unwrap();
        assert_eq!(
            db.slowmode_last_ms(&gid, &chan, &alice).unwrap(),
            Some(1_000)
        );

        // Out-of-order (older) delivery never rewinds the tracked clock.
        db.bump_slowmode_last_ms(&gid, &chan, &alice, 500).unwrap();
        assert_eq!(
            db.slowmode_last_ms(&gid, &chan, &alice).unwrap(),
            Some(1_000)
        );
        db.bump_slowmode_last_ms(&gid, &chan, &alice, 1_500)
            .unwrap();
        assert_eq!(
            db.slowmode_last_ms(&gid, &chan, &alice).unwrap(),
            Some(1_500)
        );

        // Scoped per (channel, author): Bob and another channel are unaffected.
        assert_eq!(db.slowmode_last_ms(&gid, &chan, &bob).unwrap(), None);
        assert_eq!(
            db.slowmode_last_ms(&gid, &other_chan, &alice).unwrap(),
            None
        );
    }

    #[test]
    fn prune_slowmode_drops_deleted_channels_and_departed_members_only() {
        let db = Db::open_in_memory(&[1; 32]).unwrap();
        let gid = [1u8; 16];
        let chan_a = [2u8; 16];
        let chan_b = [3u8; 16];
        let alice = [9u8; 32];
        let bob = [8u8; 32];

        db.bump_slowmode_last_ms(&gid, &chan_a, &alice, 100)
            .unwrap();
        db.bump_slowmode_last_ms(&gid, &chan_a, &bob, 200).unwrap();
        db.bump_slowmode_last_ms(&gid, &chan_b, &alice, 300)
            .unwrap();

        // chan_b no longer exists, Bob is no longer a member: only the
        // (chan_a, alice) entry should survive.
        let valid_channels: BTreeSet<[u8; 16]> = [chan_a].into_iter().collect();
        let valid_authors: BTreeSet<[u8; 32]> = [alice].into_iter().collect();
        db.prune_slowmode(&gid, &valid_channels, &valid_authors)
            .unwrap();

        assert_eq!(
            db.slowmode_last_ms(&gid, &chan_a, &alice).unwrap(),
            Some(100)
        );
        assert_eq!(db.slowmode_last_ms(&gid, &chan_a, &bob).unwrap(), None);
        assert_eq!(db.slowmode_last_ms(&gid, &chan_b, &alice).unwrap(), None);
    }
}
