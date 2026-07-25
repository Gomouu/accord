//! Recherche filtrée : grammaire `from:`/`in:`/`has:`/`before:`/`after:`
//! au-dessus de l'index aveugle (SPEC §9). Les filtres structurés sont résolus
//! côté nœud (contacts, état des groupes) puis appliqués aux candidats avant de
//! rendre chaque résultat avec ses métadonnées (conversation, auteur, lamport,
//! horodatage) pour un rendu et une navigation directs côté UI.

use std::collections::HashSet;

use accord_core::db::{SearchCandidate, SearchScope};
use accord_core::search::{self, HasKind, ParsedQuery};
use accord_crypto::FriendCode;
use accord_proto::core_msg::{FileRef, MsgBody};
use serde_json::{json, Value};

use crate::error::NodeError;
use crate::hex;

use super::{now_ms, Node};

/// Plafond de candidats hydratés d'une recherche, mots-clés ou non : seuls les
/// plus récents sont considérés au-delà.
///
/// ⚠️ Ce plafond rend la recherche **incomplète** sur un mot fréquent d'un gros
/// historique : les correspondances plus anciennes que la fenêtre ne sont pas
/// examinées, donc un filtre `has:` peut manquer une vieille pièce jointe. Sans
/// lui, une recherche du mot « le » dans 100 000 messages tenait le verrou de la
/// base plus d'une seconde — donc suspendait aussi la réception des messages.
/// Cinq fois [`SEARCH_RESULT_CAP`] : de quoi laisser les filtres écarter les
/// quatre cinquièmes des candidats sans rendre une page courte.
const SEARCH_CANDIDATE_CAP: usize = 1_000;
/// Nombre maximal de résultats rendus (les plus récents d'abord).
const SEARCH_RESULT_CAP: usize = 200;

/// Conversation d'un résultat de recherche.
enum Conversation {
    /// Message direct avec un pair (clé publique).
    Dm { peer: [u8; 32] },
    /// Message d'un salon de groupe.
    Group {
        group_id: [u8; 16],
        channel_id: [u8; 16],
    },
}

impl Conversation {
    fn to_json(&self) -> Value {
        match self {
            Conversation::Dm { peer } => json!({ "type": "dm", "peer": hex::encode(peer) }),
            Conversation::Group {
                group_id,
                channel_id,
            } => json!({
                "type": "group",
                "group_id": hex::encode(group_id),
                "channel_id": hex::encode(channel_id),
            }),
        }
    }
}

/// Résultat hydraté (avec ce qu'il faut pour filtrer et pour l'UI).
struct SearchHit {
    msg_id: [u8; 16],
    conversation: Conversation,
    author: [u8; 32],
    lamport: u64,
    sent_ms: u64,
    /// Texte décodé (filtre `has:link`).
    text: String,
    /// Pièces jointes (filtres `has:image`/`has:file`).
    attachments: Vec<FileRef>,
}

/// Ensembles de conversations résolus depuis les opérandes `in:`.
#[derive(Default)]
struct InScope {
    peers: HashSet<[u8; 32]>,
    channels: HashSet<([u8; 16], [u8; 16])>,
}

impl InScope {
    fn matches(&self, conv: &Conversation) -> bool {
        match conv {
            Conversation::Dm { peer } => self.peers.contains(peer),
            Conversation::Group {
                group_id,
                channel_id,
            } => self.channels.contains(&(*group_id, *channel_id)),
        }
    }
}

impl Node {
    /// Recherche filtrée : rend les résultats les plus récents d'abord, avec
    /// leurs métadonnées. Une requête sans filtre se comporte comme la
    /// recherche par mots simple (rétrocompatibilité).
    pub fn search_filtered(&self, query: &str) -> Result<Vec<Value>, NodeError> {
        let parsed = search::parse_query(query);
        // Résolution des filtres nécessitant le carnet/les groupes, hors verrou
        // de la grande passe d'hydratation (le Mutex de la base n'est pas
        // réentrant).
        let from_authors = self.resolve_from(&parsed.from)?;
        let in_scope = self.resolve_in(&parsed.in_conversations)?;
        let now = now_ms();
        let before = parsed
            .before
            .as_deref()
            .and_then(|d| search::resolve_date(d, now));
        let after = parsed
            .after
            .as_deref()
            .and_then(|d| search::resolve_date(d, now));

        // A filter that was requested but resolved to nothing (unknown contact
        // or conversation) matches no message — it is strict, not ignored.
        let from_active = !parsed.from.is_empty();
        let in_active = !parsed.in_conversations.is_empty();
        let mut hits = self.gather_candidates(&parsed)?;
        hits.retain(|h| {
            (!from_active || from_authors.contains(&h.author))
                && (!in_active || in_scope.matches(&h.conversation))
                && parsed.has.iter().all(|k| has_kind(k, h))
                && before.map(|b| h.sent_ms < b).unwrap_or(true)
                && after.map(|a| h.sent_ms >= a).unwrap_or(true)
        });
        // Les plus récents d'abord, bornés.
        hits.sort_by(|a, b| b.sent_ms.cmp(&a.sent_ms).then(b.lamport.cmp(&a.lamport)));
        hits.truncate(SEARCH_RESULT_CAP);
        Ok(hits.iter().map(hit_json).collect())
    }

    /// Résout les opérandes `from:` en clés publiques d'auteurs. `me`/`moi`
    /// désigne notre propre identité ; les autres sont comparées au nom
    /// d'affichage (fragment) et au code ami des contacts.
    fn resolve_from(&self, operands: &[String]) -> Result<HashSet<[u8; 32]>, NodeError> {
        let mut set = HashSet::new();
        if operands.is_empty() {
            return Ok(set);
        }
        let me = self.public_key();
        let contacts = self.contacts()?;
        for op in operands {
            if op == "me" || op == "moi" {
                set.insert(me);
                continue;
            }
            for c in &contacts {
                let code = FriendCode::of_pubkey(&c.pubkey).display().to_lowercase();
                if c.display_name.to_lowercase().contains(op) || code.contains(op) {
                    set.insert(c.pubkey);
                }
            }
        }
        Ok(set)
    }

    /// Résout les opérandes `in:` en conversations : contact (DM), salon ou
    /// groupe (tous les salons du groupe correspondant).
    fn resolve_in(&self, operands: &[String]) -> Result<InScope, NodeError> {
        let mut scope = InScope::default();
        if operands.is_empty() {
            return Ok(scope);
        }
        let contacts = self.contacts()?;
        let group_ids = self.group_ids()?;
        for op in operands {
            for c in &contacts {
                if c.display_name.to_lowercase().contains(op) {
                    scope.peers.insert(c.pubkey);
                }
            }
            for gid_hex in &group_ids {
                let Some(gid) = hex::decode::<16>(gid_hex) else {
                    continue;
                };
                let Ok(state) = self.group_state(&gid) else {
                    continue;
                };
                let group_match = state.name.to_lowercase().contains(op);
                for (cid, ch) in &state.channels {
                    if group_match || ch.name.to_lowercase().contains(op) {
                        scope.channels.insert((gid, *cid));
                    }
                }
            }
        }
        Ok(scope)
    }

    /// Construit les candidats hydratés : les plus récents portant tous les mots
    /// simples de la requête, ou simplement les plus récents si elle n'en porte
    /// aucun (requête de filtres seuls). Bornés à [`SEARCH_CANDIDATE_CAP`].
    ///
    /// Deux requêtes en tout, quel que soit le nombre de correspondances : les
    /// candidats, puis leurs pièces jointes en un lot. La version précédente en
    /// faisait deux PAR correspondance.
    fn gather_candidates(&self, parsed: &ParsedQuery) -> Result<Vec<SearchHit>, NodeError> {
        self.with_db(|db| {
            let candidates =
                search::search_recent(db, &self.search_key, &parsed.text, SEARCH_CANDIDATE_CAP)?;
            let ids: Vec<[u8; 16]> = candidates.iter().map(|c| c.msg_id).collect();
            let mut attachments = db.msg_attachments_for(&ids)?;
            Ok(candidates
                .into_iter()
                .map(|c| {
                    let attachments = attachments.remove(&c.msg_id).unwrap_or_default();
                    hit_of(c, attachments)
                })
                .collect())
        })
    }
}

/// Convertit un candidat de la base en résultat filtrable.
fn hit_of(c: SearchCandidate, attachments: Vec<FileRef>) -> SearchHit {
    SearchHit {
        conversation: match c.scope {
            SearchScope::Dm { peer } => Conversation::Dm { peer },
            SearchScope::Group {
                group_id,
                channel_id,
            } => Conversation::Group {
                group_id,
                channel_id,
            },
        },
        text: body_text(c.kind, &c.body),
        msg_id: c.msg_id,
        author: c.author,
        lamport: c.lamport,
        sent_ms: c.sent_ms,
        attachments,
    }
}

/// Texte brut d'un corps encodé (vide si non textuel ou indécodable) — sert au
/// filtre `has:link`.
fn body_text(kind: u8, body: &[u8]) -> String {
    match MsgBody::decode_body(kind, body) {
        Ok(MsgBody::Text { text, .. }) => text,
        _ => String::new(),
    }
}

/// Vrai si un résultat satisfait un filtre `has:`.
fn has_kind(kind: &HasKind, hit: &SearchHit) -> bool {
    match kind {
        HasKind::Link => hit.text.contains("http://") || hit.text.contains("https://"),
        HasKind::Image => hit
            .attachments
            .iter()
            .any(|a| a.mime.to_lowercase().starts_with("image/")),
        HasKind::File => !hit.attachments.is_empty(),
    }
}

fn hit_json(hit: &SearchHit) -> Value {
    json!({
        "msg_id": hex::encode(&hit.msg_id),
        "author": hex::encode(&hit.author),
        "lamport": hit.lamport,
        "timestamp": hit.sent_ms,
        "conversation": hit.conversation.to_json(),
    })
}
