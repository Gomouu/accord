//! Mention inbox : messages où l'utilisateur local a été mentionné (`@pseudo`,
//! `@code`, `@everyone`/`@here`, `@rôle`). Purement local — la détection est
//! passive à l'ingestion (voir [`crate::mentions`]) ; aucune donnée de mention
//! ne transite sur le réseau. Une entrée par message (dédup sur `msg_id`).

use std::collections::HashSet;

use super::{blob, sql_placeholders, Db, IN_CHUNK};
use crate::error::CoreError;
use rusqlite::{params, OptionalExtension};

/// Portée d'une mention : conversation directe ou salon de groupe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MentionScope {
    /// Conversation directe (`conv_a` = clé publique du pair, 32 octets).
    Dm = 0,
    /// Salon de groupe (`conv_a` = group_id 16 octets, `conv_b` = channel_id).
    Group = 1,
}

impl MentionScope {
    fn from_u8(v: u8) -> Result<Self, CoreError> {
        match v {
            0 => Ok(Self::Dm),
            1 => Ok(Self::Group),
            _ => Err(CoreError::Invalid("portée de mention")),
        }
    }
}

/// Entrée de la boîte de mentions (référence de conversation, auteur, extrait).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MentionEntry {
    /// Message qui mentionne l'utilisateur local.
    pub msg_id: [u8; 16],
    /// Portée (DM ou groupe).
    pub scope: MentionScope,
    /// Référence primaire : pair (DM, 32 o) ou group_id (groupe, 16 o).
    pub conv_a: Vec<u8>,
    /// Salon pour un groupe ; `None` en DM.
    pub conv_b: Option<[u8; 16]>,
    /// Auteur du message (clé publique).
    pub author: [u8; 32],
    /// Horloge murale du message (ms), pour l'ordre de la boîte.
    pub ts_ms: u64,
    /// Horloge de Lamport du message.
    pub lamport: u64,
    /// Extrait borné du texte (jamais le corps complet).
    pub snippet: String,
    /// Lue (marquée par `mentions.mark_read`).
    pub read: bool,
}

/// Colonnes projetées d'une entrée de mention (ordre figé pour [`to_entry`]).
const MENTION_COLS: &str = "msg_id, scope, conv_a, conv_b, author, ts_ms, lamport, snippet, read";

type MentionRaw = (
    Vec<u8>,
    u8,
    Vec<u8>,
    Option<Vec<u8>>,
    Vec<u8>,
    u64,
    u64,
    String,
    bool,
);

fn mention_raw(row: &rusqlite::Row) -> rusqlite::Result<MentionRaw> {
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
    ))
}

fn to_entry(r: MentionRaw) -> Result<MentionEntry, CoreError> {
    Ok(MentionEntry {
        msg_id: blob(r.0)?,
        scope: MentionScope::from_u8(r.1)?,
        conv_a: r.2,
        conv_b: r.3.map(blob).transpose()?,
        author: blob(r.4)?,
        ts_ms: r.5,
        lamport: r.6,
        snippet: r.7,
        read: r.8,
    })
}

impl Db {
    /// Enregistre une mention (idempotent : rend `false` si le message avait
    /// déjà une entrée). L'entrée naît non lue.
    pub fn insert_mention(&self, m: &MentionEntry) -> Result<bool, CoreError> {
        let n = self.conn().execute(
            "INSERT OR IGNORE INTO mentions
               (msg_id, scope, conv_a, conv_b, author, ts_ms, lamport, snippet, read)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0)",
            params![
                m.msg_id,
                m.scope as u8,
                m.conv_a,
                m.conv_b,
                m.author,
                m.ts_ms,
                m.lamport,
                m.snippet,
            ],
        )?;
        Ok(n > 0)
    }

    /// Vrai si le message porte une entrée de mention (annotation `mentions_me`
    /// de l'historique).
    pub fn mention_recorded(&self, msg_id: &[u8; 16]) -> Result<bool, CoreError> {
        Ok(self
            .conn()
            .query_row(
                "SELECT 1 FROM mentions WHERE msg_id = ?1",
                [msg_id.as_slice()],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }

    /// Sous-ensemble d'un LOT de messages portant une entrée de mention
    /// (annotation `mentions_me` de l'historique). Une requête par tranche de
    /// [`IN_CHUNK`] identifiants au lieu d'une par message.
    pub fn mentions_recorded_for(
        &self,
        msg_ids: &[[u8; 16]],
    ) -> Result<HashSet<[u8; 16]>, CoreError> {
        let mut out = HashSet::new();
        for chunk in msg_ids.chunks(IN_CHUNK) {
            let sql = format!(
                "SELECT msg_id FROM mentions WHERE msg_id IN ({})",
                sql_placeholders(chunk.len())
            );
            let mut stmt = self.conn().prepare_cached(&sql)?;
            let raws = stmt
                .query_map(
                    rusqlite::params_from_iter(chunk.iter().map(|id| id.as_slice())),
                    |row| row.get::<_, Vec<u8>>(0),
                )?
                .collect::<Result<Vec<_>, _>>()?;
            for id in raws {
                out.insert(blob(id)?);
            }
        }
        Ok(out)
    }

    /// Boîte de mentions, de la plus récente à la plus ancienne, bornée à
    /// `limit`, strictement avant `before_ts` (pagination par horloge murale).
    pub fn mention_inbox(
        &self,
        before_ts: u64,
        limit: usize,
    ) -> Result<Vec<MentionEntry>, CoreError> {
        let sql = format!(
            "SELECT {MENTION_COLS} FROM mentions
             WHERE ts_ms < ?1
             ORDER BY ts_ms DESC, msg_id DESC LIMIT ?2"
        );
        let mut stmt = self.conn().prepare(&sql)?;
        let raws = stmt
            .query_map(
                params![before_ts.min(i64::MAX as u64) as i64, limit as i64],
                mention_raw,
            )?
            .collect::<Result<Vec<_>, _>>()?;
        raws.into_iter().map(to_entry).collect()
    }

    /// Marque toutes les mentions comme lues ; rend le nombre affecté.
    pub fn mark_mentions_read_all(&self) -> Result<usize, CoreError> {
        Ok(self
            .conn()
            .execute("UPDATE mentions SET read = 1 WHERE read = 0", [])?)
    }

    /// Marque les mentions désignées comme lues ; rend le nombre affecté.
    ///
    /// Une requête par tranche de [`IN_CHUNK`] identifiants au lieu d'une par
    /// identifiant : marquer une boîte entière (l'appelant passe la liste
    /// affichée) ne coûte plus une transaction implicite par ligne. Le compte
    /// rendu est le même — seules les entrées encore non lues sont affectées.
    pub fn mark_mentions_read(&self, msg_ids: &[[u8; 16]]) -> Result<usize, CoreError> {
        let mut affected = 0;
        for chunk in msg_ids.chunks(IN_CHUNK) {
            let sql = format!(
                "UPDATE mentions SET read = 1 WHERE read = 0 AND msg_id IN ({})",
                sql_placeholders(chunk.len())
            );
            let mut stmt = self.conn().prepare_cached(&sql)?;
            affected += stmt.execute(rusqlite::params_from_iter(
                chunk.iter().map(|id| id.as_slice()),
            ))?;
        }
        Ok(affected)
    }

    /// Nombre de mentions **non lues** d'une conversation directe (pair).
    pub fn count_dm_mentions(&self, peer: &[u8; 32]) -> Result<u64, CoreError> {
        Ok(self.conn().query_row(
            "SELECT COUNT(*) FROM mentions WHERE scope = 0 AND conv_a = ?1 AND read = 0",
            [peer.as_slice()],
            |row| row.get(0),
        )?)
    }

    /// Nombre de mentions **non lues** d'un groupe (tous salons confondus).
    pub fn count_group_mentions(&self, group_id: &[u8; 16]) -> Result<u64, CoreError> {
        Ok(self.conn().query_row(
            "SELECT COUNT(*) FROM mentions WHERE scope = 1 AND conv_a = ?1 AND read = 0",
            [group_id.as_slice()],
            |row| row.get(0),
        )?)
    }

    /// Mentions **non lues** par salon d'un groupe : `(channel_id, n)` pour
    /// chaque salon en portant (portée groupe, salon renseigné). Alimente les
    /// pastilles de mention par salon (`groups.list.channel_mentions`).
    pub fn count_group_channel_mentions(
        &self,
        group_id: &[u8; 16],
    ) -> Result<Vec<([u8; 16], u64)>, CoreError> {
        let mut stmt = self.conn().prepare(
            "SELECT conv_b, COUNT(*) FROM mentions
             WHERE scope = 1 AND conv_a = ?1 AND conv_b IS NOT NULL AND read = 0
             GROUP BY conv_b",
        )?;
        let rows = stmt
            .query_map([group_id.as_slice()], |row| {
                let cid: Vec<u8> = row.get(0)?;
                let n: u64 = row.get(1)?;
                Ok((cid, n))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|(cid, n)| Ok((blob(cid)?, n)))
            .collect()
    }

    /// Marque lues les mentions non lues d'un salon de groupe (à l'ouverture du
    /// salon, parité Discord) ; rend le nombre affecté.
    pub fn mark_group_channel_mentions_read(
        &self,
        group_id: &[u8; 16],
        channel_id: &[u8; 16],
    ) -> Result<usize, CoreError> {
        Ok(self.conn().execute(
            "UPDATE mentions SET read = 1
             WHERE scope = 1 AND conv_a = ?1 AND conv_b = ?2 AND read = 0",
            params![group_id.as_slice(), channel_id.as_slice()],
        )?)
    }

    /// Marque lues les mentions non lues d'une conversation directe (à son
    /// ouverture, parité Discord) ; rend le nombre affecté.
    pub fn mark_dm_mentions_read(&self, peer: &[u8; 32]) -> Result<usize, CoreError> {
        Ok(self.conn().execute(
            "UPDATE mentions SET read = 1 WHERE scope = 0 AND conv_a = ?1 AND read = 0",
            [peer.as_slice()],
        )?)
    }

    /// Retire l'entrée de mention d'un message (appelé à sa suppression : un
    /// message effacé ne doit pas laisser d'extrait dans la boîte).
    pub(crate) fn delete_mention(&self, msg_id: &[u8; 16]) -> Result<(), CoreError> {
        self.conn().execute(
            "DELETE FROM mentions WHERE msg_id = ?1",
            [msg_id.as_slice()],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: u8, ts: u64, scope: MentionScope) -> MentionEntry {
        MentionEntry {
            msg_id: [id; 16],
            scope,
            conv_a: match scope {
                MentionScope::Dm => vec![9u8; 32],
                MentionScope::Group => vec![7u8; 16],
            },
            conv_b: match scope {
                MentionScope::Dm => None,
                MentionScope::Group => Some([8u8; 16]),
            },
            author: [2u8; 32],
            ts_ms: ts,
            lamport: ts,
            snippet: format!("extrait {id}"),
            read: false,
        }
    }

    #[test]
    fn mentions_par_lot_egalent_l_acces_unitaire() {
        // Le sous-ensemble rendu par lot doit coïncider avec
        // `mention_recorded` message par message.
        let db = Db::open_in_memory(&[1; 32]).unwrap();
        let ids: Vec<[u8; 16]> = (1..=12u8).map(|i| [i; 16]).collect();
        for i in (1..=12u8).step_by(3) {
            db.insert_mention(&entry(i, u64::from(i), MentionScope::Dm))
                .unwrap();
        }
        let batch = db.mentions_recorded_for(&ids).unwrap();
        for id in &ids {
            assert_eq!(
                batch.contains(id),
                db.mention_recorded(id).unwrap(),
                "divergence pour {:?}",
                id[0]
            );
        }
        assert!(!batch.contains(&[99; 16]));
    }

    #[test]
    fn insert_is_idempotent_and_recorded_flag() {
        let db = Db::open_in_memory(&[1; 32]).unwrap();
        assert!(db.insert_mention(&entry(1, 10, MentionScope::Dm)).unwrap());
        // Dédup : deuxième insertion du même msg_id ignorée.
        assert!(!db.insert_mention(&entry(1, 10, MentionScope::Dm)).unwrap());
        assert!(db.mention_recorded(&[1; 16]).unwrap());
        assert!(!db.mention_recorded(&[2; 16]).unwrap());
    }

    #[test]
    fn inbox_orders_recent_first_and_paginates() {
        let db = Db::open_in_memory(&[1; 32]).unwrap();
        for i in 1..=5u8 {
            db.insert_mention(&entry(i, i as u64 * 10, MentionScope::Dm))
                .unwrap();
        }
        let page = db.mention_inbox(u64::MAX, 3).unwrap();
        assert_eq!(
            page.iter().map(|e| e.ts_ms).collect::<Vec<_>>(),
            vec![50, 40, 30]
        );
        let older = db.mention_inbox(page[2].ts_ms, 10).unwrap();
        assert_eq!(
            older.iter().map(|e| e.ts_ms).collect::<Vec<_>>(),
            vec![20, 10]
        );
    }

    #[test]
    fn group_entry_roundtrips_channel() {
        let db = Db::open_in_memory(&[1; 32]).unwrap();
        db.insert_mention(&entry(3, 5, MentionScope::Group))
            .unwrap();
        let got = db.mention_inbox(u64::MAX, 10).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].scope, MentionScope::Group);
        assert_eq!(got[0].conv_a, vec![7u8; 16]);
        assert_eq!(got[0].conv_b, Some([8u8; 16]));
    }

    #[test]
    fn counts_and_mark_read() {
        let db = Db::open_in_memory(&[1; 32]).unwrap();
        db.insert_mention(&entry(1, 10, MentionScope::Dm)).unwrap();
        db.insert_mention(&entry(2, 20, MentionScope::Dm)).unwrap();
        db.insert_mention(&entry(3, 30, MentionScope::Group))
            .unwrap();
        assert_eq!(db.count_dm_mentions(&[9; 32]).unwrap(), 2);
        assert_eq!(db.count_group_mentions(&[7; 16]).unwrap(), 1);

        // Marquer une entrée précise.
        assert_eq!(db.mark_mentions_read(&[[1; 16]]).unwrap(), 1);
        assert_eq!(db.count_dm_mentions(&[9; 32]).unwrap(), 1);
        // Idempotent : déjà lue ⇒ 0 affectée.
        assert_eq!(db.mark_mentions_read(&[[1; 16]]).unwrap(), 0);
        // Tout marquer.
        assert_eq!(db.mark_mentions_read_all().unwrap(), 2);
        assert_eq!(db.count_dm_mentions(&[9; 32]).unwrap(), 0);
        assert_eq!(db.count_group_mentions(&[7; 16]).unwrap(), 0);
    }

    #[test]
    fn marquer_lues_par_lot_franchit_les_tranches_et_compte_juste() {
        // 🔒 Le marquage groupe les identifiants par tranches de `IN_CHUNK` :
        // aucune entrée ne doit rester non lue au-delà de la première tranche,
        // et le compte rendu doit toujours être celui des entrées RÉELLEMENT
        // affectées (déjà lues ou inconnues exclues).
        let db = Db::open_in_memory(&[1; 32]).unwrap();
        let total = IN_CHUNK + 7;
        let ids: Vec<[u8; 16]> = (0..total)
            .map(|i| {
                let mut id = [0u8; 16];
                id[..8].copy_from_slice(&(i as u64).to_be_bytes());
                id
            })
            .collect();
        for (i, id) in ids.iter().enumerate() {
            let mut e = entry(0, i as u64, MentionScope::Dm);
            e.msg_id = *id;
            db.insert_mention(&e).unwrap();
        }
        assert_eq!(db.count_dm_mentions(&[9; 32]).unwrap(), total as u64);
        // Un identifiant inconnu glissé dans le lot ne compte pas.
        let mut lot = ids.clone();
        lot.push([0xFF; 16]);
        assert_eq!(db.mark_mentions_read(&lot).unwrap(), total);
        assert_eq!(db.count_dm_mentions(&[9; 32]).unwrap(), 0);
        // Idempotent : plus rien à marquer au second passage.
        assert_eq!(db.mark_mentions_read(&lot).unwrap(), 0);
        assert_eq!(db.mark_mentions_read(&[]).unwrap(), 0);
    }

    #[test]
    fn delete_mention_removes_entry() {
        let db = Db::open_in_memory(&[1; 32]).unwrap();
        db.insert_mention(&entry(1, 10, MentionScope::Dm)).unwrap();
        db.delete_mention(&[1; 16]).unwrap();
        assert!(!db.mention_recorded(&[1; 16]).unwrap());
        assert_eq!(db.count_dm_mentions(&[9; 32]).unwrap(), 0);
    }
}
