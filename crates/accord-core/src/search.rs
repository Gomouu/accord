//! Recherche locale aveugle (SPEC §9).
//!
//! L'index ne contient jamais de texte : chaque mot est réduit à
//! `HMAC-SHA-256(clé_de_recherche, mot_normalisé)`. La clé de recherche est
//! dérivée de la graine d'identité ([`accord_crypto::derive_search_key`]) et
//! ne quitte jamais l'appareil — même un vol de la base ne révèle pas le
//! vocabulaire sans elle. Une requête est l'intersection des messages
//! contenant tous les mots demandés.

use std::collections::BTreeSet;

use hmac::digest::KeyInit;
use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::db::{Db, SearchCandidate};
use crate::error::CoreError;

/// Longueur minimale d'un mot indexé (élimine le bruit).
const MIN_TOKEN_CHARS: usize = 2;
/// Longueur maximale d'un mot indexé.
const MAX_TOKEN_CHARS: usize = 64;
/// Nombre maximal de mots indexés par message.
const MAX_TOKENS_PER_MSG: usize = 128;

/// Normalise et découpe un texte en mots uniques triés.
///
/// Minuscule Unicode, découpe sur tout caractère non alphanumérique ; les
/// mots trop courts ou trop longs sont écartés.
pub fn tokenize(text: &str) -> Vec<String> {
    let lowered = text.to_lowercase();
    let set: BTreeSet<String> = lowered
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| {
            let n = w.chars().count();
            (MIN_TOKEN_CHARS..=MAX_TOKEN_CHARS).contains(&n)
        })
        .map(str::to_string)
        .collect();
    set.into_iter().take(MAX_TOKENS_PER_MSG).collect()
}

/// HMAC d'un mot normalisé sous la clé de recherche locale.
fn token_hmac(search_key: &[u8; 32], token: &str) -> [u8; 32] {
    // HMAC complète les clés courtes par des zéros jusqu'à la taille de bloc
    // (64 octets pour SHA-256) : ce constructeur infaillible est strictement
    // équivalent à `new_from_slice(search_key)`.
    let mut padded = [0u8; 64];
    padded[..32].copy_from_slice(search_key);
    let mut mac = <Hmac<Sha256> as KeyInit>::new(&padded.into());
    mac.update(token.as_bytes());
    mac.finalize().into_bytes().into()
}

/// Jetons HMAC d'un texte (indexation comme requête).
pub fn hashed_tokens(search_key: &[u8; 32], text: &str) -> Vec<[u8; 32]> {
    tokenize(text)
        .iter()
        .map(|t| token_hmac(search_key, t))
        .collect()
}

/// Indexe le texte d'un message. Sans effet si aucun mot exploitable.
pub fn index_message(
    db: &Db,
    search_key: &[u8; 32],
    msg_id: &[u8; 16],
    text: &str,
) -> Result<(), CoreError> {
    let tokens = hashed_tokens(search_key, text);
    if tokens.is_empty() {
        return Ok(());
    }
    db.index_tokens(msg_id, &tokens)
}

/// Réindexe un message après édition (l'ancien vocabulaire est effacé).
pub fn reindex_message(
    db: &Db,
    search_key: &[u8; 32],
    msg_id: &[u8; 16],
    new_text: &str,
) -> Result<(), CoreError> {
    db.unindex_msg(msg_id)?;
    index_message(db, search_key, msg_id, new_text)
}

/// Messages contenant tous les mots de `query` (intersection), sans borne.
///
/// Primitive complète : elle rend TOUS les messages qui portent les mots. Le
/// chemin utilisateur passe par [`search_recent`], qui borne le travail — sur un
/// mot fréquent d'un gros historique, « tous » se compte en dizaines de
/// milliers.
pub fn search(db: &Db, search_key: &[u8; 32], query: &str) -> Result<Vec<[u8; 16]>, CoreError> {
    let tokens = hashed_tokens(search_key, query);
    if tokens.is_empty() {
        return Ok(Vec::new());
    }
    db.search_tokens(&tokens)
}

/// Les `cap` candidats les plus récents pour `query`, hydratés (conversation,
/// auteur, horloges, corps). Une requête sans mot rend simplement les `cap`
/// messages les plus récents : c'est le cas d'une recherche qui ne porte que
/// des filtres (`from:`, `has:`…), résolus par l'appelant.
///
/// ⚠️ Borné : voir [`Db::search_candidates`] pour ce que la borne écarte.
pub fn search_recent(
    db: &Db,
    search_key: &[u8; 32],
    query: &str,
    cap: usize,
) -> Result<Vec<SearchCandidate>, CoreError> {
    db.search_candidates(&hashed_tokens(search_key, query), cap)
}

// ---- Grammaire de recherche filtrée (SPEC §9, résolution côté nœud) ----

/// Millisecondes par jour (résolution des filtres de date).
const MS_PER_DAY: u64 = 86_400_000;
/// Millisecondes par heure.
const MS_PER_HOUR: u64 = 3_600_000;
/// Millisecondes par minute.
const MS_PER_MIN: u64 = 60_000;

/// Nature de pièce jointe d'un filtre `has:`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HasKind {
    /// Le texte du message contient une URL (`http://` / `https://`).
    Link,
    /// Au moins une pièce jointe image (`image/*`).
    Image,
    /// Au moins une pièce jointe, quelle qu'en soit la nature.
    File,
}

/// Requête de recherche décomposée : mots simples + jetons de filtre.
///
/// Les opérandes `from:`/`in:` sont laissées telles quelles (minuscule) : leur
/// résolution contre les contacts et les groupes relève du nœud, qui seul
/// connaît le carnet et l'état des groupes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedQuery {
    /// Mots simples réinjectés dans l'index aveugle (joints par espace).
    pub text: String,
    /// Opérandes `from:` (fragment de nom de contact ou code ami), minuscule.
    pub from: Vec<String>,
    /// Opérandes `in:` (fragment de nom de contact, de salon ou de groupe).
    pub in_conversations: Vec<String>,
    /// Opérandes `has:`.
    pub has: Vec<HasKind>,
    /// Opérande `before:` brute (résolue par [`resolve_date`]).
    pub before: Option<String>,
    /// Opérande `after:` brute.
    pub after: Option<String>,
}

/// Retire un préfixe insensible à la casse ASCII (les préfixes sont ASCII, donc
/// la coupe tombe toujours sur une frontière de caractère valide).
fn strip_ci<'a>(tok: &'a str, prefix: &str) -> Option<&'a str> {
    let (pb, tb) = (prefix.as_bytes(), tok.as_bytes());
    if tb.len() >= pb.len() && tb[..pb.len()].eq_ignore_ascii_case(pb) {
        Some(&tok[pb.len()..])
    } else {
        None
    }
}

/// Décompose une requête en mots simples et jetons de filtre. Les jetons
/// inconnus (`has:` invalide, opérande vide) sont ignorés silencieusement ;
/// tout le reste retombe en mots simples (rétrocompatibilité stricte).
pub fn parse_query(query: &str) -> ParsedQuery {
    let mut parsed = ParsedQuery::default();
    let mut words: Vec<&str> = Vec::new();
    for tok in query.split_whitespace() {
        if let Some(v) = strip_ci(tok, "from:") {
            if !v.is_empty() {
                parsed.from.push(v.to_lowercase());
            }
        } else if let Some(v) = strip_ci(tok, "in:") {
            if !v.is_empty() {
                parsed.in_conversations.push(v.to_lowercase());
            }
        } else if let Some(v) = strip_ci(tok, "has:") {
            match v.to_lowercase().as_str() {
                "link" => parsed.has.push(HasKind::Link),
                "image" => parsed.has.push(HasKind::Image),
                "file" => parsed.has.push(HasKind::File),
                _ => {}
            }
        } else if let Some(v) = strip_ci(tok, "before:") {
            if !v.is_empty() {
                parsed.before = Some(v.to_lowercase());
            }
        } else if let Some(v) = strip_ci(tok, "after:") {
            if !v.is_empty() {
                parsed.after = Some(v.to_lowercase());
            }
        } else {
            words.push(tok);
        }
    }
    parsed.text = words.join(" ");
    parsed
}

/// Jours depuis l'époque Unix pour une date civile proleptique grégorienne
/// (algorithme de Howard Hinnant, sans dépendance calendrier).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let mp = if m > 2 { m - 3 } else { m + 9 }; // mars = 0
    let doy = (153 * mp + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146097 + doe - 719468
}

/// Convertit une date ISO `YYYY-MM-DD` en millisecondes (minuit UTC).
fn parse_iso_date(s: &str) -> Option<u64> {
    let mut parts = s.split('-');
    let y: i64 = parts.next()?.parse().ok()?;
    let m: i64 = parts.next()?.parse().ok()?;
    let d: i64 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    if !(1970..=9999).contains(&y) || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let days = days_from_civil(y, m, d);
    (days >= 0).then(|| days as u64 * MS_PER_DAY)
}

/// Convertit un décalage relatif `Nd` / `Nh` / `Nm` / `Nw` en instant passé.
fn parse_relative(s: &str, now_ms: u64) -> Option<u64> {
    let unit = s.chars().last()?;
    let num = &s[..s.len() - unit.len_utf8()];
    let n: u64 = num.parse().ok()?;
    let unit_ms = match unit {
        'd' => MS_PER_DAY,
        'h' => MS_PER_HOUR,
        'm' => MS_PER_MIN,
        'w' => MS_PER_DAY * 7,
        _ => return None,
    };
    Some(now_ms.saturating_sub(n.saturating_mul(unit_ms)))
}

/// Résout une opérande de date en borne temporelle (ms Unix) : date ISO
/// `YYYY-MM-DD` (minuit UTC), mots-clés `today`/`yesterday`, ou décalage
/// relatif `Nd`/`Nh`/`Nm`/`Nw` (compté depuis `now_ms`). `None` si illisible.
pub fn resolve_date(raw: &str, now_ms: u64) -> Option<u64> {
    let s = raw.trim().to_lowercase();
    match s.as_str() {
        "today" => return Some(now_ms - now_ms % MS_PER_DAY),
        "yesterday" => return Some((now_ms - now_ms % MS_PER_DAY).saturating_sub(MS_PER_DAY)),
        _ => {}
    }
    parse_iso_date(&s).or_else(|| parse_relative(&s, now_ms))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_normalizes_dedups_and_filters() {
        let tokens = tokenize("Bonjour, BONJOUR le Monde ! x 2026");
        assert_eq!(tokens, vec!["2026", "bonjour", "le", "monde"]);
    }

    #[test]
    fn index_and_search_intersection() {
        let db = Db::open_in_memory(&[4u8; 32]).unwrap();
        let key = [9u8; 32];
        index_message(&db, &key, &[1; 16], "rendez-vous demain au parc").unwrap();
        index_message(&db, &key, &[2; 16], "le parc est fermé demain").unwrap();
        index_message(&db, &key, &[3; 16], "réunion demain matin").unwrap();

        let both = search(&db, &key, "demain parc").unwrap();
        assert_eq!(both.len(), 2);
        assert!(both.contains(&[1; 16]) && both.contains(&[2; 16]));

        assert!(search(&db, &key, "demain inexistant").unwrap().is_empty());
        assert!(search(&db, &key, "!!!").unwrap().is_empty());
    }

    #[test]
    fn index_stores_no_plaintext_tokens() {
        let db = Db::open_in_memory(&[4u8; 32]).unwrap();
        let key = [9u8; 32];
        index_message(&db, &key, &[1; 16], "confidentiel").unwrap();
        // Le HMAC du mot n'est pas le mot : une autre clé ne trouve rien.
        assert!(search(&db, &[8u8; 32], "confidentiel").unwrap().is_empty());
        assert_eq!(search(&db, &key, "confidentiel").unwrap(), vec![[1; 16]]);
    }

    #[test]
    fn reindex_replaces_vocabulary() {
        let db = Db::open_in_memory(&[4u8; 32]).unwrap();
        let key = [9u8; 32];
        index_message(&db, &key, &[1; 16], "ancien contenu").unwrap();
        reindex_message(&db, &key, &[1; 16], "nouveau texte").unwrap();
        assert!(search(&db, &key, "ancien").unwrap().is_empty());
        assert_eq!(search(&db, &key, "nouveau").unwrap(), vec![[1; 16]]);
    }

    #[test]
    fn plain_query_parses_as_words_only() {
        let p = parse_query("bonjour le monde");
        assert_eq!(p.text, "bonjour le monde");
        assert!(p.from.is_empty() && p.in_conversations.is_empty() && p.has.is_empty());
        assert!(p.before.is_none() && p.after.is_none());
    }

    #[test]
    fn filters_are_extracted_and_words_kept() {
        let p =
            parse_query("From:Alice in:general has:image HAS:link photo before:2026-01-01 hello");
        assert_eq!(p.text, "photo hello");
        assert_eq!(p.from, vec!["alice"]);
        assert_eq!(p.in_conversations, vec!["general"]);
        assert_eq!(p.has, vec![HasKind::Image, HasKind::Link]);
        assert_eq!(p.before.as_deref(), Some("2026-01-01"));
        // Jeton has: inconnu ignoré, opérande vide ignorée.
        let p2 = parse_query("has:video from: réunion");
        assert!(p2.has.is_empty() && p2.from.is_empty());
        assert_eq!(p2.text, "réunion");
    }

    #[test]
    fn resolve_date_iso_relative_and_keywords() {
        // 2026-07-10 minuit UTC = 20644 jours depuis l'époque.
        let iso = resolve_date("2026-07-10", 0).unwrap();
        assert_eq!(iso, 20_644 * 86_400_000);
        // Relatif : now - 7 jours.
        let now = 1_000 * 86_400_000;
        assert_eq!(resolve_date("7d", now).unwrap(), now - 7 * 86_400_000);
        assert_eq!(resolve_date("24h", now).unwrap(), now - 86_400_000);
        assert_eq!(resolve_date("30m", now).unwrap(), now - 30 * 60_000);
        // Mots-clés : plancher au jour courant.
        let mid = now + 12 * 3_600_000 + 42;
        assert_eq!(resolve_date("today", mid).unwrap(), now);
        assert_eq!(resolve_date("yesterday", mid).unwrap(), now - 86_400_000);
        // Illisible.
        assert!(resolve_date("pas-une-date", now).is_none());
        assert!(resolve_date("2026-13-40", now).is_none());
    }
}
