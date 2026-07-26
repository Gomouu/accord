//! Correspondance AutoMod côté nœud (SPEC §7, modèle sans serveur).
//!
//! 🔗 JUMEAU de `app/src/lib/automod.ts`. L'interface masque les mots filtrés
//! au rendu ; le nœud applique ici la MÊME règle pour retrancher les messages
//! masqués du compteur de non-lus. Les deux doivent décider pareil : un
//! message dont le mot est remplacé par des `█` mais qui continue d'allumer la
//! pastille rouge désigne exactement ce que le filtre prétendait cacher.
//!
//! Les deux implémentations sont volontairement écrites à l'identique :
//! minuscules, décomposition canonique, suppression des diacritiques
//! combinantes, puis recherche d'occurrence bornée aux frontières de mot
//! (`Alphabetic`, `N` et `_` sont des caractères de mot des deux côtés). Les
//! suites de tests partagent les mêmes cas, nommés pareil.
//!
//! Ce module ne décide RIEN : il ne supprime pas, ne réécrit pas, ne bloque
//! aucune émission. L'AutoMod d'Accord est une convention d'affichage entre
//! clients honnêtes, la liste de mots voyage en clair dans l'op-log signée du
//! groupe, et un client modifié verra toujours le texte entier.

use accord_proto::core_msg::MsgBody;
use unicode_normalization::UnicodeNormalization;

/// Vrai si `c` fait partie d'un mot : ce sont ces caractères qui, collés au
/// mot filtré, empêchent la correspondance (« concert » contre « con »).
///
/// `is_alphabetic() || is_numeric()` correspond exactement à
/// `[\p{Alphabetic}\p{N}]` du jumeau TypeScript.
fn is_word_char(c: char) -> bool {
    c.is_alphabetic() || c.is_numeric() || c == '_'
}

/// Forme comparable d'un texte ou d'un mot filtré : minuscules, décomposé,
/// diacritiques combinantes retirées.
///
/// Le même mot arrive tantôt précomposé (« é » = U+00E9), tantôt décomposé
/// (« e » + U+0301) selon le clavier ou le système de l'émetteur — macOS
/// produit couramment la forme décomposée. Sans ce repli, le filtre marchait
/// ou non selon la machine d'en face.
fn fold(input: &str) -> String {
    input
        .chars()
        .flat_map(char::to_lowercase)
        .nfd()
        .filter(|c| !('\u{0300}'..='\u{036F}').contains(c))
        .collect()
}

/// Prépare la liste de mots filtrés d'un groupe : repli une fois pour toutes,
/// mots vides écartés.
///
/// Séparée de [`matches`] parce qu'un salon compare la MÊME liste à des
/// centaines de messages : la replier à chaque message serait du travail
/// refait pour rien.
pub fn prepare<S: AsRef<str>>(words: impl IntoIterator<Item = S>) -> Vec<String> {
    words
        .into_iter()
        .map(|w| fold(w.as_ref().trim()))
        .filter(|w| !w.is_empty())
        .collect()
}

/// Vrai si `hay` contient `needle` comme mot entier. Les deux sont attendus
/// déjà repliés par [`fold`].
fn contains_word(hay: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    let mut from = 0;
    while let Some(rel) = hay[from..].find(needle) {
        let at = from + rel;
        let end = at + needle.len();
        let left_ok = hay[..at]
            .chars()
            .next_back()
            .is_none_or(|c| !is_word_char(c));
        let right_ok = hay[end..].chars().next().is_none_or(|c| !is_word_char(c));
        if left_ok && right_ok {
            return true;
        }
        // Avance d'UN caractère : `hay[at..]` commence par `needle`, donc la
        // longueur du premier caractère de `needle` est une frontière valide
        // (un `+ 1` brut couperait un caractère multi-octets et paniquerait).
        from = at + needle.chars().next().map_or(1, char::len_utf8);
    }
    false
}

/// Vrai si `text` contient au moins un mot filtré. `prepared` sort de
/// [`prepare`].
pub fn matches(text: &str, prepared: &[String]) -> bool {
    if prepared.is_empty() || text.is_empty() {
        return false;
    }
    let hay = fold(text);
    prepared.iter().any(|w| contains_word(&hay, w))
}

/// Vrai si le message est masqué par l'AutoMod, d'après le texte que
/// l'interface AFFICHERAIT.
///
/// Même choix que `displayText` côté interface : la dernière édition prime sur
/// le corps d'origine, et seul un corps texte porte du texte masquable. Les
/// sondages, stickers et pièces jointes ne sont pas masqués au rendu
/// (`PollCard`, `StickerImage`, `AttachmentRow` ignorent la liste) ; les
/// compter comme masqués ferait retomber une pastille sur un message
/// parfaitement lisible.
pub fn message_matches(kind: u8, body: &[u8], edited: Option<&[u8]>, prepared: &[String]) -> bool {
    if prepared.is_empty() {
        return false;
    }
    if let Some(edit) = edited {
        return matches(&String::from_utf8_lossy(edit), prepared);
    }
    match MsgBody::decode_body(kind, body) {
        Ok(MsgBody::Text { text, .. }) => matches(&text, prepared),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kw(words: &[&str]) -> Vec<String> {
        prepare(words)
    }

    // jumeau: « ne masque pas un mot filtré au milieu d'un autre mot »
    #[test]
    fn matches_whole_words_only() {
        let words = kw(&["con"]);
        assert!(matches("quel con celui-là", &words));
        // Le cas qui donne son sens à la frontière de mot.
        assert!(!matches("on va au concert", &words));
        assert!(!matches("un jambon suffit", &words));
        assert!(matches("con.", &words));
        assert!(matches("(con)", &words));
        assert!(matches("con", &words));

        let chat = kw(&["chat"]);
        assert!(!matches("le chaton dort", &chat));
        assert!(!matches("achat en ligne", &chat));
        assert!(matches("le chat et le chaton", &chat));
        // Frontière Unicode : une lettre accentuée collée reste le même mot.
        assert!(!matches("idiotè", &kw(&["idiot"])));
    }

    #[test]
    fn matches_ignores_case() {
        let words = kw(&["idiot"]);
        assert!(matches("IDIOT", &words));
        assert!(matches("Idiot", &words));
        assert!(matches("quel IDIOT alors", &words));
    }

    // jumeau: « masque un mot accentué filtré sans accent, et l'inverse »
    #[test]
    fn matches_ignores_accents_in_both_directions() {
        assert!(matches("espèce de crétin", &kw(&["cretin"])));
        assert!(matches("espece de cretin", &kw(&["crétin"])));
        assert!(matches("CRÉTIN va", &kw(&["crétin"])));
    }

    // jumeau: « masque quel que soit le codage Unicode de l'accent (NFC/NFD) »
    #[test]
    fn matches_precomposed_and_decomposed_alike() {
        let nfc = "crétin";
        let nfd = "cre\u{0301}tin";
        assert_ne!(nfc, nfd, "les deux formes doivent bien différer en octets");
        assert!(matches(&format!("quel {nfd} !"), &kw(&[nfc])));
        assert!(matches(&format!("quel {nfc} !"), &kw(&[nfd])));
    }

    #[test]
    fn matches_handles_empty_inputs() {
        assert!(!matches("n'importe quoi", &[]));
        assert!(!matches("", &kw(&["con"])));
        // Un mot vide ou blanc est écarté par `prepare`, jamais comparé.
        assert!(kw(&["", "   "]).is_empty());
        assert!(!matches("n'importe quoi", &kw(&["   "])));
    }

    #[test]
    fn matches_does_not_stop_at_the_first_glued_occurrence() {
        // « concert » vient AVANT le mot isolé : la recherche doit continuer.
        assert!(matches("le concert puis con", &kw(&["con"])));
    }

    #[test]
    fn message_matches_reads_text_and_edits_only() {
        let words = kw(&["spoiler"]);
        let text = MsgBody::Text {
            text: "attention spoiler".into(),
            reply_to: None,
            attachments: vec![],
        };
        assert!(message_matches(
            text.kind(),
            &text.encode_body(),
            None,
            &words
        ));

        let clean = MsgBody::Text {
            text: "rien à signaler".into(),
            reply_to: None,
            attachments: vec![],
        };
        assert!(!message_matches(
            clean.kind(),
            &clean.encode_body(),
            None,
            &words
        ));
        // Édité POUR contenir le mot : c'est le texte AFFICHÉ qui compte, et
        // c'est bien lui que l'interface masque.
        assert!(message_matches(
            clean.kind(),
            &clean.encode_body(),
            Some("finalement spoiler".as_bytes()),
            &words
        ));
        // ... et l'inverse : édité pour ne plus le contenir.
        assert!(!message_matches(
            text.kind(),
            &text.encode_body(),
            Some("rien finalement".as_bytes()),
            &words
        ));

        // Un sondage n'est pas masqué au rendu : il ne doit pas non plus
        // disparaître du compteur de non-lus.
        let poll = MsgBody::Poll {
            poll_id: [3; 16],
            question: "quel spoiler ?".into(),
            options: vec!["oui".into(), "non".into()],
        };
        assert!(!message_matches(
            poll.kind(),
            &poll.encode_body(),
            None,
            &words
        ));

        // Sans mot filtré, aucun corps n'est examiné.
        assert!(!message_matches(
            text.kind(),
            &text.encode_body(),
            None,
            &[]
        ));
        // Corps indécodable : jamais masqué.
        assert!(!message_matches(200, b"\xff\xff", None, &words));
    }
}
