//! Index de recherche aveugle : jetons HMACés → identifiants de messages.
//! La base ne stocke jamais les mots en clair ; la tokenisation et le HMAC
//! relèvent de [`crate::search`].

use super::{blob, Db};
use crate::error::CoreError;
use rusqlite::params;

/// Conversation d'où sort un candidat de recherche.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchScope {
    /// Message direct échangé avec ce pair.
    Dm {
        /// Interlocuteur de la conversation.
        peer: [u8; 32],
    },
    /// Message d'un salon de groupe.
    Group {
        /// Groupe.
        group_id: [u8; 16],
        /// Salon.
        channel_id: [u8; 16],
    },
}

/// Candidat de recherche : de quoi appliquer les filtres du nœud et rendre le
/// résultat à l'interface, sans une requête de plus par identifiant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchCandidate {
    /// Identifiant du message.
    pub msg_id: [u8; 16],
    /// Conversation d'origine.
    pub scope: SearchScope,
    /// Auteur (clé publique).
    pub author: [u8; 32],
    /// Horloge de Lamport.
    pub lamport: u64,
    /// Horloge murale d'envoi (ms), auto-déclarée par l'auteur.
    pub sent_ms: u64,
    /// Discriminant du corps.
    pub kind: u8,
    /// Corps encodé.
    pub body: Vec<u8>,
}

/// Multiple de la borne au-delà duquel les correspondances d'un jeton sont
/// jugées « denses ». Quatre : il faut que descendre les messages par récence
/// trouve ses `cap` correspondances vite, or un mot présent dans un message sur
/// quatre les trouve en quatre fois la borne de lignes lues — du même ordre que
/// ce que le plan par index aurait trié.
const DENSE_HIT_FACTOR: usize = 4;

/// Table de messages interrogée par [`candidates_sql`] : les deux natures de
/// conversation vivent dans des tables distinctes, aux colonnes différentes.
#[derive(Clone, Copy)]
enum Source {
    /// `dm_messages` : `conv_a` est le pair, `conv_b` est `NULL`.
    Dm,
    /// `group_messages` : `(conv_a, conv_b)` est `(groupe, salon)`.
    Group,
}

impl Source {
    /// Projection des colonnes du candidat, dans l'ordre de lecture. La table
    /// est aliasée `m` : les colonnes de l'index aveugle portent les mêmes noms,
    /// et un `msg_id` non qualifié dans une sous-requête se résoudrait sur elle.
    fn projection(self) -> &'static str {
        match self {
            Source::Dm => {
                "SELECT m.msg_id, m.author, m.lamport, m.sent_ms, m.kind, m.body,
                        m.peer AS conv_a, NULL AS conv_b FROM dm_messages m"
            }
            Source::Group => {
                "SELECT m.msg_id, m.author, m.lamport, m.sent_ms, m.kind, m.body,
                        m.group_id AS conv_a, m.channel_id AS conv_b FROM group_messages m"
            }
        }
    }
}

/// Stratégie de restriction aux messages portant tous les jetons demandés.
///
/// Les deux plans rendent le même résultat ; ils diffèrent par ce dont le coût
/// dépend, et **le choix a été mesuré** (`accord-node/benches/history.rs`, corpus
/// de 100 000 messages) :
///
/// | plan | mot rare (100 messages) | mot fréquent (100 000) |
/// |---|---|---|
/// | [`Restriction::ByIndex`] | 0,59 ms | 317 ms |
/// | [`Restriction::ByRecency`] | 79 ms | 2,9 ms |
///
/// D'où la bascule de [`Db::search_candidates`] : un mot rare se cherche par
/// l'index aveugle, un mot fréquent en descendant les messages du plus récent.
#[derive(Clone, Copy)]
enum Restriction {
    /// Aucune : les plus récents, toutes conversations confondues.
    None,
    /// Par l'index aveugle : SQLite matérialise l'intersection puis la trie.
    /// Coût proportionnel au nombre de correspondances, pas à la borne.
    ByIndex,
    /// Par récence : SQLite descend l'index `sent_ms` du plus récent au plus
    /// ancien et sonde l'index aveugle à chaque ligne, jusqu'à la borne. Coût
    /// proportionnel à la borne quand les correspondances sont denses, mais
    /// jusqu'à un balayage complet quand elles sont rares.
    ByRecency,
}

/// Requête des `cap` candidats les plus récents d'UNE table, restreints aux
/// messages portant tous les jetons selon `restriction`.
///
/// Une table à la fois, jamais d'`UNION` : le tri par récence d'une requête
/// composée ne sait pas se servir des index `sent_ms`, et le premier
/// `ORDER BY ... LIMIT` de l'union relisait alors tout l'historique. Les `cap`
/// plus récents de l'ensemble sont forcément dans l'union des `cap` plus récents
/// de chaque table : l'appelant fusionne les deux moitiés.
///
/// Paramètres liés : les `tokens` dans l'ordre, puis `cap`.
fn candidates_sql(source: Source, restriction: Restriction, token_count: usize) -> String {
    // Sans jeton, il n'y a rien à restreindre, quel que soit le plan demandé.
    let restriction = if token_count == 0 {
        Restriction::None
    } else {
        restriction
    };
    let mut sql = String::new();
    if matches!(restriction, Restriction::ByIndex) {
        sql.push_str("WITH hits(msg_id) AS (");
        for i in 0..token_count {
            if i > 0 {
                sql.push_str(" INTERSECT ");
            }
            sql.push_str("SELECT msg_id FROM search_index WHERE token = ?");
            sql.push_str(&(i + 1).to_string());
        }
        sql.push_str(") ");
    }
    sql.push_str(source.projection());
    match restriction {
        Restriction::None => {}
        Restriction::ByIndex => sql.push_str(" WHERE m.msg_id IN (SELECT msg_id FROM hits)"),
        Restriction::ByRecency => {
            for i in 0..token_count {
                sql.push_str(if i == 0 { " WHERE " } else { " AND " });
                sql.push_str("EXISTS (SELECT 1 FROM search_index s WHERE s.token = ?");
                sql.push_str(&(i + 1).to_string());
                sql.push_str(" AND s.msg_id = m.msg_id)");
            }
        }
    }
    // Ordre de rendu de l'interface (les plus récents d'abord) : c'est donc
    // exactement ce que la borne conserve.
    sql.push_str(" ORDER BY m.sent_ms DESC, m.lamport DESC LIMIT ?");
    sql.push_str(&(token_count + 1).to_string());
    sql
}

impl Db {
    /// Indexe des jetons (déjà HMACés) pour un message.
    pub fn index_tokens(&self, msg_id: &[u8; 16], tokens: &[[u8; 32]]) -> Result<(), CoreError> {
        let mut stmt = self
            .conn()
            .prepare("INSERT OR IGNORE INTO search_index (token, msg_id) VALUES (?1, ?2)")?;
        for token in tokens {
            stmt.execute(params![token, msg_id])?;
        }
        Ok(())
    }

    /// Messages contenant TOUS les jetons donnés (intersection). Primitive
    /// d'index : non bornée, et sans jointure vers les messages — un
    /// identifiant rendu ici peut ne plus avoir de message (index à purger).
    pub fn search_tokens(&self, tokens: &[[u8; 32]]) -> Result<Vec<[u8; 16]>, CoreError> {
        let Some((first, rest)) = tokens.split_first() else {
            return Ok(Vec::new());
        };
        let mut stmt = self
            .conn()
            .prepare("SELECT msg_id FROM search_index WHERE token = ?1")?;
        let raws = stmt
            .query_map([first.as_slice()], |row| row.get::<_, Vec<u8>>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let mut ids: Vec<[u8; 16]> = raws.into_iter().map(blob).collect::<Result<Vec<_>, _>>()?;
        for token in rest {
            let mut keep = Vec::with_capacity(ids.len());
            let mut check = self
                .conn()
                .prepare("SELECT 1 FROM search_index WHERE token = ?1 AND msg_id = ?2")?;
            for id in ids {
                if check.exists(params![token, id])? {
                    keep.push(id);
                }
            }
            ids = keep;
            if ids.is_empty() {
                break;
            }
        }
        Ok(ids)
    }

    /// Les `cap` messages les plus récents portant TOUS les `tokens`, hydratés,
    /// du plus récent au plus ancien. Sans jeton : les `cap` messages les plus
    /// récents, toutes conversations confondues.
    ///
    /// ⚠️ **Borné, donc incomplet au-delà de `cap`.** Un mot fréquent peut
    /// apparaître dans tout l'historique ; seule sa fenêtre récente est rendue.
    /// C'est le prix d'une latence bornée : sans cette borne, une recherche d'un
    /// mot courant tenait le verrou de la base plus d'une seconde, ce qui
    /// suspendait aussi la réception des messages.
    pub fn search_candidates(
        &self,
        tokens: &[[u8; 32]],
        cap: usize,
    ) -> Result<Vec<SearchCandidate>, CoreError> {
        let restriction = if tokens.is_empty() {
            Restriction::None
        } else if self.hits_are_dense(tokens, cap)? {
            Restriction::ByRecency
        } else {
            Restriction::ByIndex
        };
        let mut candidates = self.candidates_of(Source::Dm, restriction, tokens, cap)?;
        candidates.extend(self.candidates_of(Source::Group, restriction, tokens, cap)?);
        // Les deux moitiés arrivent triées ; la fusion refait l'ordre global.
        candidates.sort_by(|a, b| {
            b.sent_ms
                .cmp(&a.sent_ms)
                .then_with(|| b.lamport.cmp(&a.lamport))
        });
        candidates.truncate(cap);
        Ok(candidates)
    }

    /// Vrai si CHAQUE jeton apparaît dans nettement plus de messages que la
    /// borne — auquel cas descendre les messages par récence trouve ses `cap`
    /// correspondances presque tout de suite (cf. [`Restriction`]).
    ///
    /// Sonde bornée : `LIMIT` arrête le comptage dès le seuil franchi, donc le
    /// coût de la décision ne dépend pas de la fréquence réelle du mot. Un seul
    /// jeton rare suffit à trancher pour l'index — l'intersection est alors au
    /// plus aussi grande que lui.
    fn hits_are_dense(&self, tokens: &[[u8; 32]], cap: usize) -> Result<bool, CoreError> {
        let seuil = cap.saturating_mul(DENSE_HIT_FACTOR);
        let mut stmt = self.conn().prepare_cached(
            "SELECT count(*) FROM (SELECT 1 FROM search_index WHERE token = ?1 LIMIT ?2)",
        )?;
        for token in tokens {
            let vus: i64 = stmt.query_row(params![token, seuil as i64], |r| r.get(0))?;
            if (vus as usize) < seuil {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Exécute [`candidates_sql`] pour une table et matérialise les candidats.
    fn candidates_of(
        &self,
        source: Source,
        restriction: Restriction,
        tokens: &[[u8; 32]],
        cap: usize,
    ) -> Result<Vec<SearchCandidate>, CoreError> {
        let mut stmt =
            self.conn()
                .prepare_cached(&candidates_sql(source, restriction, tokens.len()))?;
        let mut args: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(tokens.len() + 1);
        for token in tokens {
            args.push(token);
        }
        let cap = cap as i64;
        args.push(&cap);
        let raws = stmt
            .query_map(args.as_slice(), |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, u64>(2)?,
                    row.get::<_, u64>(3)?,
                    row.get::<_, u8>(4)?,
                    row.get::<_, Vec<u8>>(5)?,
                    row.get::<_, Vec<u8>>(6)?,
                    row.get::<_, Option<Vec<u8>>>(7)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        raws.into_iter()
            .map(
                |(msg_id, author, lamport, sent_ms, kind, body, conv_a, conv_b)| {
                    let scope = match conv_b {
                        None => SearchScope::Dm {
                            peer: blob(conv_a)?,
                        },
                        Some(channel) => SearchScope::Group {
                            group_id: blob(conv_a)?,
                            channel_id: blob(channel)?,
                        },
                    };
                    Ok(SearchCandidate {
                        msg_id: blob(msg_id)?,
                        scope,
                        author: blob(author)?,
                        lamport,
                        sent_ms,
                        kind,
                        body,
                    })
                },
            )
            .collect()
    }

    /// Désindexe un message (suppression/tombstone).
    pub fn unindex_msg(&self, msg_id: &[u8; 16]) -> Result<(), CoreError> {
        self.conn().execute(
            "DELETE FROM search_index WHERE msg_id = ?1",
            [msg_id.as_slice()],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{DmRecord, GroupMsgRecord};

    #[test]
    fn intersection_search_and_unindex() {
        let db = Db::open_in_memory(&[1; 32]).unwrap();
        db.index_tokens(&[1; 16], &[[10; 32], [11; 32]]).unwrap();
        db.index_tokens(&[2; 16], &[[10; 32]]).unwrap();
        db.index_tokens(&[1; 16], &[[10; 32]]).unwrap(); // idempotent

        assert_eq!(db.search_tokens(&[[10; 32]]).unwrap().len(), 2);
        assert_eq!(
            db.search_tokens(&[[10; 32], [11; 32]]).unwrap(),
            vec![[1; 16]]
        );
        assert!(db.search_tokens(&[[12; 32]]).unwrap().is_empty());
        assert!(db.search_tokens(&[]).unwrap().is_empty());

        db.unindex_msg(&[1; 16]).unwrap();
        assert!(db.search_tokens(&[[11; 32]]).unwrap().is_empty());
        assert_eq!(db.search_tokens(&[[10; 32]]).unwrap(), vec![[2; 16]]);
    }

    fn dm(n: u64, peer: [u8; 32]) -> DmRecord {
        let mut msg_id = [0u8; 16];
        msg_id[..8].copy_from_slice(&n.to_be_bytes());
        DmRecord {
            msg_id,
            peer,
            author: peer,
            lamport: n,
            sent_ms: 1_000 * n,
            kind: 0,
            body: vec![n as u8],
            acked: true,
            deleted: false,
            edited: None,
        }
    }

    fn group_msg(n: u64) -> GroupMsgRecord {
        let mut msg_id = [0u8; 16];
        msg_id[..8].copy_from_slice(&n.to_be_bytes());
        GroupMsgRecord {
            msg_id,
            group_id: [7; 16],
            channel_id: [8; 16],
            author: [9; 32],
            lamport: n,
            sent_ms: 1_000 * n,
            kind: 0,
            body: vec![n as u8],
            deleted: false,
            edited: None,
        }
    }

    #[test]
    fn les_candidats_melangent_les_deux_natures_de_conversation() {
        let db = Db::open_in_memory(&[1; 32]).unwrap();
        db.insert_dm(&dm(1, [42; 32])).unwrap();
        db.insert_group_msg(&group_msg(2)).unwrap();
        let tous = db.search_candidates(&[], 10).unwrap();
        assert_eq!(tous.len(), 2);
        // Le plus récent d'abord, et chacun avec sa conversation d'origine.
        assert_eq!(
            tous[0].scope,
            SearchScope::Group {
                group_id: [7; 16],
                channel_id: [8; 16]
            }
        );
        assert_eq!(tous[1].scope, SearchScope::Dm { peer: [42; 32] });
    }

    #[test]
    fn les_candidats_gardent_les_plus_recents_dans_la_borne() {
        let db = Db::open_in_memory(&[1; 32]).unwrap();
        for n in 1..=10 {
            db.insert_dm(&dm(n, [42; 32])).unwrap();
            db.index_tokens(&dm(n, [42; 32]).msg_id, &[[10; 32]])
                .unwrap();
        }
        // La borne coupe les PLUS ANCIENS, jamais les plus récents : c'est ce
        // que l'interface affiche en tête de résultats.
        let bornes = db.search_candidates(&[[10; 32]], 3).unwrap();
        assert_eq!(
            bornes.iter().map(|c| c.lamport).collect::<Vec<_>>(),
            vec![10, 9, 8]
        );
    }

    #[test]
    fn un_jeton_sans_message_ne_rend_aucun_candidat() {
        // L'index peut désigner un message absent (purge incomplète) : la
        // jointure doit l'ignorer au lieu de rendre un candidat creux.
        let db = Db::open_in_memory(&[1; 32]).unwrap();
        db.index_tokens(&[3; 16], &[[10; 32]]).unwrap();
        assert!(db.search_candidates(&[[10; 32]], 10).unwrap().is_empty());
    }

    #[test]
    fn l_intersection_des_candidats_exige_tous_les_jetons() {
        let db = Db::open_in_memory(&[1; 32]).unwrap();
        db.insert_dm(&dm(1, [42; 32])).unwrap();
        db.insert_dm(&dm(2, [42; 32])).unwrap();
        db.index_tokens(&dm(1, [42; 32]).msg_id, &[[10; 32], [11; 32]])
            .unwrap();
        db.index_tokens(&dm(2, [42; 32]).msg_id, &[[10; 32]])
            .unwrap();
        assert_eq!(db.search_candidates(&[[10; 32]], 10).unwrap().len(), 2);
        let deux = db.search_candidates(&[[10; 32], [11; 32]], 10).unwrap();
        assert_eq!(deux.len(), 1);
        assert_eq!(deux[0].lamport, 1);
    }

    /// Corpus qui déclenche le plan « par récence ».
    ///
    /// Ce plan ne s'active qu'au-delà de [`DENSE_HIT_FACTOR`] × borne
    /// correspondances : il faut donc INDEXER assez de messages, sinon les tests
    /// suivants éprouveraient le plan par index une fois de plus sans le dire.
    /// `messages_muets` en ajoute autant de plus récents ne portant pas le mot.
    ///
    /// Dans les deux cas, les deux correspondances les plus récentes portent les
    /// horloges 10 et 9.
    fn base_dense(messages_muets: bool) -> (Db, usize) {
        let db = Db::open_in_memory(&[1; 32]).unwrap();
        let cap = 2;
        let portant = (cap * DENSE_HIT_FACTOR + 2) as u64;
        let total = if messages_muets { portant * 2 } else { portant };
        for n in 1..=total {
            db.insert_dm(&dm(n, [42; 32])).unwrap();
            if n <= portant {
                db.index_tokens(&dm(n, [42; 32]).msg_id, &[[10; 32]])
                    .unwrap();
            }
        }
        (db, cap)
    }

    #[test]
    fn le_plan_dense_rend_les_plus_recents_portant_le_mot() {
        let (db, cap) = base_dense(false);
        let recents = db.search_candidates(&[[10; 32]], cap).unwrap();
        assert_eq!(
            recents.iter().map(|c| c.lamport).collect::<Vec<_>>(),
            vec![10, 9]
        );
    }

    #[test]
    fn le_plan_dense_saute_les_messages_sans_le_mot() {
        // 🔒 Garde-fou : le plan par récence descend les messages du plus récent
        // au plus ancien et sonde l'index à chaque ligne. Une sonde mal écrite
        // (identifiant non qualifié, que SQL peut résoudre sur l'index au lieu
        // du message) rendrait les plus récents SANS vérifier qu'ils portent le
        // mot : ici les dix plus récents n'en portent aucun.
        let (db, cap) = base_dense(true);
        let recents = db.search_candidates(&[[10; 32]], cap).unwrap();
        assert_eq!(
            recents.iter().map(|c| c.lamport).collect::<Vec<_>>(),
            vec![10, 9]
        );
    }
}
