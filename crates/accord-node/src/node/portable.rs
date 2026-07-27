//! Export lisible et réimportable des conversations (jalon 7, §19.4.2).
//!
//! **Pourquoi ce format à côté de `.accordbackup`.** La sauvegarde chiffrée
//! existe déjà et fait très bien son travail : elle restaure une machine à
//! l'identique. Mais elle ne s'ouvre qu'avec Accord, et un utilisateur qui ne
//! peut pas partir avec ses données est captif. Un projet qui prône la
//! souveraineté doit être exemplaire là-dessus — d'où un second format, en
//! clair, documenté, et que n'importe quel outil sait lire.
//!
//! Les deux ne se remplacent pas : la sauvegarde est faite pour revenir, cet
//! export est fait pour partir.
//!
//! 🔒 **Ce fichier n'est pas chiffré.** Il contient l'intégralité des
//! conversations en clair — c'est précisément ce qu'on lui demande, et c'est
//! aussi ce qui en fait le fichier le plus dangereux que l'application sache
//! produire. L'appelant le dit à l'utilisateur ; le format, lui, le rappelle
//! dans son propre en-tête (`warning`), pour que le fichier reste explicite
//! même séparé de l'application qui l'a produit.

use accord_proto::core_msg::MsgBody;
use serde_json::{json, Map, Value};

use crate::error::NodeError;
use crate::hex;

use super::Node;

/// Version du format. Incrémentée à tout changement incompatible ; un
/// importeur refuse ce qu'il ne connaît pas plutôt que de deviner.
pub const EXPORT_FORMAT: u32 = 1;

/// Conversations couvertes par un export, au plus.
///
/// ⚠️ Généreux, et **annoncé dans le document** quand la borne mord
/// (`truncated`). Un export tronqué en silence est pire qu'un export court :
/// le lecteur conclut qu'il n'y avait rien de plus.
const MAX_CONVERSATIONS: usize = 10_000;

/// Messages exportés par conversation, au plus. Même règle d'annonce.
const MAX_MESSAGES_PAR_CONVERSATION: usize = 100_000;

/// Bilan d'un import : ce qui est entré, ce qui existait déjà, ce qui a été
/// écarté.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ImportReport {
    /// Messages réellement insérés.
    pub inserted: usize,
    /// Messages déjà présents (même `msg_id`) — l'import est idempotent.
    pub skipped: usize,
    /// Messages écartés : conversation avec un pair inconnu, ou entrée
    /// malformée.
    pub rejected: usize,
}

/// Texte lisible d'un message, ou `None` si son corps n'en porte pas.
///
/// Sépare délibérément le *contenu* de l'*enveloppe* : l'export porte les deux,
/// le texte pour être lu par un humain et les octets d'origine pour pouvoir
/// être réimporté à l'identique. Rendre seulement le texte ferait un export
/// joli et non réimportable ; ne rendre que les octets ferait l'inverse.
pub fn texte_lisible(kind: u8, body: &[u8]) -> Option<String> {
    match MsgBody::decode_body(kind, body).ok()? {
        MsgBody::Text { text, .. } => Some(text),
        MsgBody::Edit { new_text, .. } => Some(new_text),
        _ => None,
    }
}

/// Lit une chaîne hexadécimale de `N` octets dans un objet JSON.
fn champ_hex<const N: usize>(o: &Map<String, Value>, cle: &str) -> Option<[u8; N]> {
    hex::decode::<N>(o.get(cle)?.as_str()?)
}

/// Lit un entier non signé dans un objet JSON.
fn champ_u64(o: &Map<String, Value>, cle: &str) -> Option<u64> {
    o.get(cle)?.as_u64()
}

impl Node {
    /// Document d'export complet : profil, contacts, conversations.
    pub fn export_document(&self) -> Result<Value, NodeError> {
        let moi = self.public_key();
        let contacts = self.contacts()?;
        let convs = self.with_db(|db| Ok(db.dm_conversations(MAX_CONVERSATIONS)?))?;
        let conversations_tronquees = convs.len() == MAX_CONVERSATIONS;

        let mut conversations = Vec::with_capacity(convs.len());
        for pair in &convs {
            // `dm_history` pagine du plus récent au plus ancien ; on prend une
            // fenêtre bornée et on note si elle a mordu.
            let mut msgs = self.dm_history(pair, u64::MAX, MAX_MESSAGES_PAR_CONVERSATION)?;
            let tronquee = msgs.len() == MAX_MESSAGES_PAR_CONVERSATION;
            // Ordre de lecture : du plus ancien au plus récent, comme à
            // l'écran. Un export qu'il faut relire à l'envers n'est pas lisible.
            msgs.reverse();
            let nom = contacts
                .iter()
                .find(|c| c.pubkey == *pair)
                .map(|c| c.display_name.clone());
            conversations.push(json!({
                "peer": hex::encode(pair),
                "peer_name": nom,
                "truncated": tronquee,
                "messages": msgs.iter().map(|m| json!({
                    "msg_id": hex::encode(&m.msg_id),
                    "author": hex::encode(&m.author),
                    "lamport": m.lamport,
                    "sent_ms": m.sent_ms,
                    "deleted": m.deleted,
                    // Le texte pour l'humain…
                    "text": texte_lisible(m.kind, m.edited.as_deref().unwrap_or(&m.body)),
                    // …et l'enveloppe d'origine pour la machine, sans quoi
                    // l'import ne rendrait pas le message tel qu'il était.
                    "kind": m.kind,
                    "body_hex": hex::encode(&m.body),
                    "edited_hex": m.edited.as_ref().map(|e| hex::encode(e)),
                })).collect::<Vec<_>>(),
            }));
        }

        Ok(json!({
            "format": EXPORT_FORMAT,
            "generator": concat!("accord ", env!("CARGO_PKG_VERSION")),
            "warning": "This file is NOT encrypted. It contains every conversation in clear text.",
            "account": hex::encode(&moi),
            "truncated": conversations_tronquees,
            "contacts": contacts.iter().map(|c| json!({
                "pubkey": hex::encode(&c.pubkey),
                "display_name": c.display_name,
                "added_ms": c.added_ms,
            })).collect::<Vec<_>>(),
            "conversations": conversations,
        }))
    }

    /// Réimporte un document produit par [`Node::export_document`].
    ///
    /// 🔒 **N'importe que dans une conversation dont le pair est déjà une
    /// relation.** Un fichier d'export est une entrée non authentifiée — il
    /// arrive par le disque, personne ne l'a signé. L'accepter tel quel
    /// laisserait n'importe quel fichier inventer des correspondants et écrire
    /// des messages sous leur nom. La même règle que le rattrapage entre nos
    /// propres appareils applique déjà (`ingest_self_sync_item`), et pour la
    /// même raison : un import remplit un historique, il ne crée pas de
    /// relation.
    ///
    /// Idempotent : `INSERT OR IGNORE` sur `msg_id`, donc réimporter deux fois
    /// le même fichier ne duplique rien — c'est ce que compte `skipped`.
    pub fn import_document(&self, doc: &Value) -> Result<ImportReport, NodeError> {
        let obj = doc
            .as_object()
            .ok_or(NodeError::Invalid("document d'export illisible"))?;
        // Version refusée plutôt que devinée : un format futur peut avoir
        // changé le sens d'un champ sans en changer le nom.
        if obj.get("format").and_then(Value::as_u64) != Some(u64::from(EXPORT_FORMAT)) {
            return Err(NodeError::Invalid("version de format non prise en charge"));
        }
        let convs = obj
            .get("conversations")
            .and_then(Value::as_array)
            .ok_or(NodeError::Invalid("conversations manquantes"))?;

        let mut bilan = ImportReport::default();
        for conv in convs {
            let Some(conv) = conv.as_object() else {
                bilan.rejected += 1;
                continue;
            };
            let Some(pair) = champ_hex::<32>(conv, "peer") else {
                bilan.rejected += 1;
                continue;
            };
            if !self.is_relation(&pair) {
                // Toute la conversation est écartée, pas seulement un message :
                // compter chaque message serait plus précis et moins utile.
                bilan.rejected += conv
                    .get("messages")
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len);
                continue;
            }
            let Some(msgs) = conv.get("messages").and_then(Value::as_array) else {
                continue;
            };
            for m in msgs {
                match self.importer_message(&pair, m)? {
                    Some(true) => bilan.inserted += 1,
                    Some(false) => bilan.skipped += 1,
                    None => bilan.rejected += 1,
                }
            }
        }
        Ok(bilan)
    }

    /// Insère UN message importé. `Ok(None)` si l'entrée est malformée,
    /// `Ok(Some(true))` si elle est entrée, `Ok(Some(false))` si elle existait.
    fn importer_message(&self, pair: &[u8; 32], m: &Value) -> Result<Option<bool>, NodeError> {
        let Some(o) = m.as_object() else {
            return Ok(None);
        };
        let (Some(msg_id), Some(author), Some(lamport), Some(sent_ms), Some(kind)) = (
            champ_hex::<16>(o, "msg_id"),
            champ_hex::<32>(o, "author"),
            champ_u64(o, "lamport"),
            champ_u64(o, "sent_ms"),
            o.get("kind").and_then(Value::as_u64),
        ) else {
            return Ok(None);
        };
        // 🔒 Même contrôle que le rattrapage entre appareils : une conversation
        // n'a que deux auteurs possibles, le pair ou nous. Sans lui, un fichier
        // forgé classerait dans la conversation de P un message signé du nom de
        // Q, et l'interface l'afficherait comme tel.
        if author != *pair && author != self.public_key() {
            return Ok(None);
        }
        let Some(body) = o
            .get("body_hex")
            .and_then(Value::as_str)
            .map(hex::decode_vec)
        else {
            return Ok(None);
        };
        let Some(body) = body else {
            return Ok(None);
        };
        let edited = match o.get("edited_hex").and_then(Value::as_str) {
            Some(h) => match hex::decode_vec(h) {
                Some(v) => Some(v),
                None => return Ok(None),
            },
            None => None,
        };
        let Ok(kind) = u8::try_from(kind) else {
            return Ok(None);
        };
        let record = accord_core::db::DmRecord {
            msg_id,
            peer: *pair,
            author,
            lamport,
            sent_ms,
            kind,
            body,
            // Un message importé n'a pas d'accusé à attendre : il vient d'un
            // fichier, pas du réseau.
            acked: true,
            deleted: o.get("deleted").and_then(Value::as_bool).unwrap_or(false),
            edited,
        };
        let insere = self.with_db(|db| Ok(db.insert_dm(&record)?))?;
        Ok(Some(insere))
    }
}
