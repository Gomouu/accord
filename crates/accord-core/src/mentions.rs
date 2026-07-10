//! Détection de mention (locale, passive). Un message de groupe ou direct ne
//! porte **aucune** métadonnée de mention sur le fil : le texte contient
//! littéralement le `@…` tapé par l'émetteur. À l'ingestion, le nœud décide si
//! l'utilisateur **local** est visé en comparant le texte à ses propres
//! identifiants (pseudo, code ami), aux jetons spéciaux `@everyone`/`@here`, et
//! aux noms des rôles qu'il détient dans le groupe.
//!
//! `@here` est traité comme `@everyone` : la présence effective (« membre en
//! ligne à l'instant T ») n'est pas connaissable dans un réseau P2P sans
//! serveur, donc la détection reste littérale. L'abus (`@everyone` répété) est
//! un problème social, non technique ; la boîte de mentions dédoublonne au
//! niveau du message ([`crate::db::Db::insert_mention`]).

/// Longueur maximale de l'extrait conservé dans la boîte de mentions (en
/// caractères Unicode, pas en octets).
pub const MAX_SNIPPET_CHARS: usize = 140;

/// Facette d'identité locale utilisée pour la comparaison des mentions.
#[derive(Debug, Clone, Copy)]
pub struct MentionSelf<'a> {
    /// Pseudo local, s'il est défini.
    pub name: Option<&'a str>,
    /// Code ami affiché (jamais vide).
    pub code: &'a str,
    /// Noms des rôles détenus dans la conversation (vide en DM).
    pub roles: &'a [String],
}

/// Vrai si `text` mentionne l'utilisateur local d'après `me`.
///
/// La comparaison est littérale, insensible à la casse et bornée : `@` ne doit
/// pas être précédé d'un caractère alphanumérique (les adresses e-mail
/// `a@b` ne déclenchent donc pas) et le jeton ne doit pas être suivi d'un
/// caractère alphanumérique (le pseudo « an » ne se déclenche pas sur
/// `@anne`).
pub fn detect(text: &str, me: &MentionSelf) -> bool {
    if text.is_empty() {
        return false;
    }
    let hay = text.to_lowercase();
    // @everyone / @here visent tout le monde (here == everyone, voir en-tête).
    if contains_mention(&hay, "everyone") || contains_mention(&hay, "here") {
        return true;
    }
    if let Some(name) = me.name {
        if token_matches(&hay, name) {
            return true;
        }
    }
    if token_matches(&hay, me.code) {
        return true;
    }
    me.roles.iter().any(|role| token_matches(&hay, role))
}

/// Normalise puis teste un jeton candidat (vide après trim ⇒ jamais).
fn token_matches(hay_lower: &str, token: &str) -> bool {
    let trimmed = token.trim();
    !trimmed.is_empty() && contains_mention(hay_lower, &trimmed.to_lowercase())
}

/// Vrai si `hay_lower` (déjà en minuscules) contient `@token` (déjà en
/// minuscules) comme mention bornée.
fn contains_mention(hay_lower: &str, token_lower: &str) -> bool {
    if token_lower.is_empty() {
        return false;
    }
    let mut from = 0;
    while let Some(rel) = hay_lower[from..].find('@') {
        let at = from + rel;
        let after_at = at + 1;
        if hay_lower[after_at..].starts_with(token_lower) {
            let end = after_at + token_lower.len();
            let left_ok = hay_lower[..at]
                .chars()
                .next_back()
                .is_none_or(|c| !c.is_alphanumeric());
            let right_ok = hay_lower[end..]
                .chars()
                .next()
                .is_none_or(|c| !c.is_alphanumeric());
            if left_ok && right_ok {
                return true;
            }
        }
        from = at + 1;
    }
    false
}

/// Extrait borné d'un texte pour la boîte de mentions (jamais le corps
/// complet) : rogné, tronqué à [`MAX_SNIPPET_CHARS`] caractères, suffixé d'une
/// ellipse si tronqué.
pub fn snippet(text: &str) -> String {
    let trimmed = text.trim();
    let mut out: String = trimmed.chars().take(MAX_SNIPPET_CHARS).collect();
    if trimmed.chars().count() > MAX_SNIPPET_CHARS {
        out.push('…');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn me<'a>(name: Option<&'a str>, roles: &'a [String]) -> MentionSelf<'a> {
        MentionSelf {
            name,
            code: "ALPHA-BRAVO-CHARLIE-DELTA-ECHO-1234",
            roles,
        }
    }

    #[test]
    fn matches_display_name_case_insensitively() {
        let no_roles: [String; 0] = [];
        let ctx = me(Some("Anna"), &no_roles);
        assert!(detect("salut @anna ça va", &ctx));
        assert!(detect("Coucou @ANNA !", &ctx));
        assert!(detect("@Anna", &ctx));
    }

    #[test]
    fn requires_word_boundaries() {
        let no_roles: [String; 0] = [];
        let ctx = me(Some("an"), &no_roles);
        // « @anne » ne doit pas déclencher le pseudo « an ».
        assert!(!detect("coucou @anne", &ctx));
        // Adresse e-mail : le « @ » précédé d'alphanumérique ne compte pas.
        let ctx2 = me(Some("bob"), &no_roles);
        assert!(!detect("écris à alice@bob.example", &ctx2));
    }

    #[test]
    fn everyone_and_here_both_trigger() {
        let no_roles: [String; 0] = [];
        let ctx = me(Some("zoe"), &no_roles);
        assert!(detect("annonce @everyone", &ctx));
        assert!(detect("dispo @here ?", &ctx));
        assert!(!detect("just @somewhere else", &ctx));
    }

    #[test]
    fn matches_role_names() {
        let roles = vec!["Admins".to_string(), "Modérateurs".to_string()];
        let ctx = me(Some("zoe"), &roles);
        assert!(detect("ping @Modérateurs svp", &ctx));
        assert!(detect("@admins réunion", &ctx));
        // Rôle non détenu : pas de mention.
        assert!(!detect("@membres coucou", &ctx));
    }

    #[test]
    fn matches_friend_code() {
        let no_roles: [String; 0] = [];
        let ctx = me(None, &no_roles);
        assert!(detect(
            "ajoute @ALPHA-BRAVO-CHARLIE-DELTA-ECHO-1234 stp",
            &ctx
        ));
    }

    #[test]
    fn no_mention_returns_false() {
        let no_roles: [String; 0] = [];
        let ctx = me(Some("anna"), &no_roles);
        assert!(!detect("un message sans arobase", &ctx));
        assert!(!detect("", &ctx));
        // Pseudo absent + pas de jeton : jamais.
        let ctx_no_name = me(None, &no_roles);
        assert!(!detect("bonjour tout le monde", &ctx_no_name));
    }

    #[test]
    fn snippet_trims_and_truncates() {
        assert_eq!(snippet("  bonjour  "), "bonjour");
        let long = "a".repeat(MAX_SNIPPET_CHARS + 20);
        let s = snippet(&long);
        assert_eq!(s.chars().count(), MAX_SNIPPET_CHARS + 1); // + ellipse
        assert!(s.ends_with('…'));
    }
}
