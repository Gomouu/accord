//! Aperçu de liens (jalon 5) — **désactivé par défaut**, sur demande explicite.
//!
//! **Ce que ça coûte, et pourquoi c'est un réglage.** Afficher l'aperçu d'un
//! lien suppose d'aller chercher la page. Le destinataire révèle donc son
//! adresse IP au site lié, sans l'avoir choisi : c'est l'expéditeur qui a
//! choisi le lien. Dans une conversation de groupe, quelqu'un de malveillant
//! peut poster un lien vers un serveur qu'il contrôle et **récolter l'IP de
//! tous ceux qui ont les aperçus activés** — un lien suffit à désanonymiser un
//! salon entier. C'est pour ça que le réglage est éteint à l'installation et
//! que son libellé dit ce qu'il fait, pas « améliorer l'affichage des liens ».
//!
//! **Pourquoi ici et pas dans `accord-node`.** `reqwest` et `rustls` sont déjà
//! dans l'arbre de cette couche, via le plugin de mise à jour. Le nœud, lui,
//! n'émet aucune requête HTTP sortante et n'en émettra pas pour ça : le démon
//! headless (`accord-noded`) n'a pas d'aperçus à afficher, et lui greffer une
//! pile web élargirait sa surface sans contrepartie.
//!
//! 🔒 **SSRF.** Une URL vient d'un pair : elle est hostile par défaut. Sans
//! garde-fou, `http://127.0.0.1:48016/...` ferait interroger le service local
//! par le nœud lui-même, et `http://169.254.169.254/` les métadonnées d'une
//! instance cloud. Toute adresse résolue non publique est donc refusée, à
//! chaque saut de redirection, et la connexion est **épinglée sur l'adresse
//! vérifiée** pour qu'un second DNS ne puisse pas la remplacer (rebinding).

use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::time::Duration;

/// Redirections suivies au plus. Trois suffisent aux raccourcisseurs d'URL
/// usuels ; au-delà, c'est une chaîne qui cherche à épuiser ou à égarer.
const MAX_REDIRECTIONS: usize = 3;

/// Octets lus au plus dans le corps. Les balises `og:` vivent dans le `<head>` ;
/// lire au-delà ne sert qu'à se faire nourrir une page sans fin.
const MAX_CORPS: usize = 256 * 1024;

/// Délai total accordé à la requête.
const DELAI: Duration = Duration::from_secs(5);

/// Ce qu'un aperçu retient d'une page. Tous les champs sont facultatifs : une
/// page sans `og:` n'est pas une erreur, juste un aperçu maigre.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, Default)]
pub struct Apercu {
    /// URL finale, après redirections — celle réellement atteinte.
    pub url: String,
    /// Titre `og:title`, à défaut `<title>`.
    pub titre: Option<String>,
    /// Résumé `og:description`, absent sur la plupart des pages.
    pub description: Option<String>,
    /// URL de l'image d'illustration, jamais téléchargée ici.
    pub image: Option<String>,
    /// Nom d'hôte final, affiché pour que l'utilisateur voie où mène le lien.
    pub hote: String,
}

/// Refus motivé. Le motif reste générique côté interface : dire « 127.0.0.1
/// refusé » confirmerait à qui sonde ce que la machine héberge.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ErreurApercu {
    #[error("URL invalide")]
    /// Ni découpable, ni sûre : autorité vide, ou identifiants glissés dedans.
    UrlInvalide,
    #[error("schéma non autorisé")]
    /// Autre chose que `http` ou `https` — `file:`, `data:`, `javascript:`…
    SchemaRefuse,
    #[error("adresse non publique")]
    /// L'hôte résout vers la machine, le LAN ou un service d'infrastructure.
    AdresseRefusee,
    #[error("trop de redirections")]
    /// Chaîne plus longue que [`MAX_REDIRECTIONS`].
    TropDeRedirections,
    #[error("réseau")]
    /// Résolution, connexion ou lecture en échec — motif volontairement vague.
    Reseau,
}

/// Vrai si l'adresse est publiquement routable — donc acceptable.
///
/// 🔒 Écrit en liste blanche à l'envers : on énumère ce qu'on REFUSE et on
/// accepte le reste, mais chaque famille refusée est nommée. `is_global` est
/// encore instable dans le std, d'où cette version explicite.
pub fn adresse_publique(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            !(v4.is_loopback()          // 127.0.0.0/8 — le service local
                || v4.is_private()      // 10/8, 172.16/12, 192.168/16 — le LAN
                || v4.is_link_local()   // 169.254/16 — métadonnées cloud
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_unspecified()  // 0.0.0.0
                || v4.is_multicast()
                // 100.64/10 (CGNAT) : ni privée ni vraiment publique, et
                // couramment le réseau de l'opérateur — refusée.
                || (v4.octets()[0] == 100 && (64..128).contains(&v4.octets()[1]))
                // 192.0.0.0/24 (IETF) et 198.18/15 (bancs de test).
                || v4.octets()[0..3] == [192, 0, 0]
                || (v4.octets()[0] == 198 && (18..20).contains(&v4.octets()[1])))
        }
        IpAddr::V6(v6) => {
            !(v6.is_loopback()          // ::1
                || v6.is_unspecified()  // ::
                || v6.is_multicast()
                // fc00::/7 — adresses locales uniques (l'équivalent du privé).
                || (v6.segments()[0] & 0xfe00) == 0xfc00
                // fe80::/10 — lien-local.
                || (v6.segments()[0] & 0xffc0) == 0xfe80
                // ::ffff:a.b.c.d — une IPv4 déguisée en IPv6 contournerait
                // sinon toute la branche ci-dessus.
                || v6.to_ipv4_mapped().is_some_and(|v4| !adresse_publique(IpAddr::V4(v4))))
        }
    }
}

/// Découpe une URL en (hôte, port, est_https) après validation.
///
/// 🔒 Refuse les identifiants dans l'URL (`http://user:pass@hôte/`) : les
/// transmettre à un site tiers ferait fuiter un secret que l'utilisateur n'a
/// jamais eu l'intention de partager, et l'affichage `user@hôte` sert
/// couramment à maquiller la destination réelle.
pub fn decouper_url(url: &str) -> Result<(String, u16, bool), ErreurApercu> {
    let (schema, reste) = url.split_once("://").ok_or(ErreurApercu::UrlInvalide)?;
    let https = match schema.to_ascii_lowercase().as_str() {
        "https" => true,
        "http" => false,
        _ => return Err(ErreurApercu::SchemaRefuse),
    };
    let autorite = reste
        .split(['/', '?', '#'])
        .next()
        .filter(|a| !a.is_empty())
        .ok_or(ErreurApercu::UrlInvalide)?;
    if autorite.contains('@') {
        return Err(ErreurApercu::UrlInvalide);
    }
    // IPv6 littérale : `[::1]:8080`.
    let (hote, port) = match autorite.strip_prefix('[') {
        Some(reste) => {
            let (dans, apres) = reste.split_once(']').ok_or(ErreurApercu::UrlInvalide)?;
            let port = match apres.strip_prefix(':') {
                Some(p) => p.parse().map_err(|_| ErreurApercu::UrlInvalide)?,
                None => {
                    if apres.is_empty() {
                        if https {
                            443
                        } else {
                            80
                        }
                    } else {
                        return Err(ErreurApercu::UrlInvalide);
                    }
                }
            };
            (dans.to_string(), port)
        }
        None => match autorite.rsplit_once(':') {
            Some((h, p)) => (
                h.to_string(),
                p.parse().map_err(|_| ErreurApercu::UrlInvalide)?,
            ),
            None => (autorite.to_string(), if https { 443 } else { 80 }),
        },
    };
    if hote.is_empty() {
        return Err(ErreurApercu::UrlInvalide);
    }
    Ok((hote, port, https))
}

/// Résout l'hôte et rend la première adresse publique, ou refuse.
///
/// 🔒 Rend l'adresse RETENUE, pas seulement un verdict : l'appelant doit s'y
/// connecter directement. Laisser le client HTTP re-résoudre le nom rouvrirait
/// la fenêtre du rebinding DNS — une réponse publique à la vérification, une
/// réponse en 127.0.0.1 à la connexion.
pub fn resoudre_publique(hote: &str, port: u16) -> Result<SocketAddr, ErreurApercu> {
    let adresses = (hote, port)
        .to_socket_addrs()
        .map_err(|_| ErreurApercu::Reseau)?;
    for addr in adresses {
        if adresse_publique(addr.ip()) {
            return Ok(addr);
        }
    }
    Err(ErreurApercu::AdresseRefusee)
}

/// Extrait titre, description et image d'un fragment HTML.
///
/// Volontairement naïf : on cherche des balises `meta`, pas on ne construit un
/// arbre. Le contenu vient d'un site tiers et n'est jamais interprété comme du
/// HTML — il ressort en texte, échappé par React à l'affichage.
pub fn extraire_meta(html: &str) -> (Option<String>, Option<String>, Option<String>) {
    let bas = html.to_ascii_lowercase();
    let og = |propriete: &str| -> Option<String> {
        // On accepte `property=` (OpenGraph) comme `name=` (Twitter, et les
        // sites qui confondent les deux), dans l'ordre où ils apparaissent.
        for balise in decouper_balises(&bas, html) {
            let (b_bas, b_brut) = balise;
            if !b_bas.contains(propriete) {
                continue;
            }
            if let Some(v) = attribut(b_bas, b_brut, "content") {
                if !v.trim().is_empty() {
                    return Some(nettoyer(&v));
                }
            }
        }
        None
    };
    let titre = og("\"og:title\"")
        .or_else(|| og("'og:title'"))
        .or_else(|| titre_html(&bas, html));
    let description = og("\"og:description\"").or_else(|| og("'og:description'"));
    let image = og("\"og:image\"").or_else(|| og("'og:image'"));
    (titre, description, image)
}

/// Paires (balise en minuscules, balise d'origine) de chaque `<meta …>`.
fn decouper_balises<'a>(bas: &'a str, brut: &'a str) -> Vec<(&'a str, &'a str)> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(debut) = bas[i..].find("<meta") {
        let debut = i + debut;
        let Some(fin) = bas[debut..].find('>') else {
            break;
        };
        let fin = debut + fin;
        out.push((&bas[debut..fin], &brut[debut..fin]));
        i = fin + 1;
    }
    out
}

/// Valeur d'un attribut d'une balise, guillemets simples ou doubles.
fn attribut(bas: &str, brut: &str, nom: &str) -> Option<String> {
    let pos = bas.find(&format!("{nom}="))? + nom.len() + 1;
    let reste_bas = bas.get(pos..)?;
    let delim = reste_bas.chars().next()?;
    if delim != '"' && delim != '\'' {
        return None;
    }
    let fin = reste_bas[1..].find(delim)? + 1;
    Some(brut.get(pos + 1..pos + fin)?.to_string())
}

/// `<title>…</title>`, repli quand la page n'expose pas d'OpenGraph.
fn titre_html(bas: &str, brut: &str) -> Option<String> {
    let debut = bas.find("<title")?;
    let debut = debut + bas[debut..].find('>')? + 1;
    let fin = debut + bas[debut..].find("</title")?;
    let t = nettoyer(brut.get(debut..fin)?);
    (!t.is_empty()).then_some(t)
}

/// Réduit une valeur venue d'un site tiers à une ligne courte et sûre.
///
/// 🔒 Les caractères de contrôle sautent : une page peut glisser un `\n` ou un
/// bidi override dans son `og:title` pour maquiller ce que l'aperçu affiche.
/// La longueur est bornée pour la même raison — un titre de dix mille
/// caractères est une attaque d'affichage, pas un titre.
fn nettoyer(valeur: &str) -> String {
    const MAX: usize = 300;
    // Ordre important : `\n` et `\t` sont à la fois des contrôles et des
    // espaces. Les retirer purement souderait les mots entre eux ; on les
    // laisse au compactage, qui les réduit à une espace simple.
    let sans_controle: String = valeur
        .chars()
        .filter(|c| {
            c.is_whitespace() || (!c.is_control() && !('\u{202a}'..='\u{202e}').contains(c))
        })
        .collect();
    let compacte = sans_controle
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let decode = compacte
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">");
    decode.chars().take(MAX).collect()
}

/// Va chercher la page et en tire un aperçu.
///
/// 🔒 Chaque saut est vérifié à part entière : une première URL publique qui
/// redirige vers `127.0.0.1` ne doit pas passer parce que le premier saut était
/// propre. Et à chaque saut, la connexion est épinglée sur l'adresse qu'on
/// vient de valider (`resolve`) — sans quoi le client re-résoudrait le nom et
/// une réponse DNS à durée de vie nulle pourrait répondre autre chose la
/// seconde fois.
pub async fn recuperer(url_depart: &str) -> Result<Apercu, ErreurApercu> {
    let mut url = url_depart.to_string();
    for _ in 0..=MAX_REDIRECTIONS {
        let (hote, port, _) = decouper_url(&url)?;
        let addr = resoudre_publique(&hote, port)?;
        let client = reqwest::Client::builder()
            .timeout(DELAI)
            // Redirections suivies à la main : c'est la seule façon de
            // re-vérifier l'adresse de chaque saut.
            .redirect(reqwest::redirect::Policy::none())
            .resolve(&hote, addr)
            .build()
            .map_err(|_| ErreurApercu::Reseau)?;
        let reponse = client
            .get(&url)
            // Un agent explicite : le site visité doit pouvoir savoir qui
            // l'interroge, et le refuser s'il le souhaite.
            .header(reqwest::header::USER_AGENT, "Accord/link-preview")
            .header(reqwest::header::ACCEPT, "text/html")
            .send()
            .await
            .map_err(|_| ErreurApercu::Reseau)?;

        if reponse.status().is_redirection() {
            let cible = reponse
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or(ErreurApercu::Reseau)?;
            url = resoudre_relatif(&url, cible)?;
            continue;
        }

        let corps = lire_borne(reponse).await?;
        let (titre, description, image) = extraire_meta(&corps);
        let (hote_final, _, _) = decouper_url(&url)?;
        return Ok(Apercu {
            url,
            titre,
            description,
            image,
            hote: hote_final,
        });
    }
    Err(ErreurApercu::TropDeRedirections)
}

/// Lit le corps en s'arrêtant à [`MAX_CORPS`] octets.
///
/// Le flux est consommé morceau par morceau plutôt que `.text()` : ce dernier
/// tamponnerait la réponse entière, et une page qui ne finit jamais épuiserait
/// la mémoire avant le délai.
async fn lire_borne(mut reponse: reqwest::Response) -> Result<String, ErreurApercu> {
    let mut octets: Vec<u8> = Vec::new();
    while let Some(morceau) = reponse.chunk().await.map_err(|_| ErreurApercu::Reseau)? {
        let reste = MAX_CORPS.saturating_sub(octets.len());
        if reste == 0 {
            break;
        }
        octets.extend_from_slice(&morceau[..morceau.len().min(reste)]);
    }
    // `from_utf8_lossy` : une page mal encodée doit donner un aperçu maigre,
    // pas une erreur — l'utilisateur verrait un échec là où il y a une page.
    Ok(String::from_utf8_lossy(&octets).into_owned())
}

/// Résout une cible de redirection, absolue ou relative, contre l'URL courante.
pub fn resoudre_relatif(base: &str, cible: &str) -> Result<String, ErreurApercu> {
    if cible.contains("://") {
        // Absolue : re-validée au tour suivant, schéma compris.
        return Ok(cible.to_string());
    }
    let (schema, reste) = base.split_once("://").ok_or(ErreurApercu::UrlInvalide)?;
    let autorite = reste.split(['/', '?', '#']).next().unwrap_or(reste);
    if let Some(chemin) = cible.strip_prefix('/') {
        return Ok(format!("{schema}://{autorite}/{chemin}"));
    }
    Ok(format!("{schema}://{autorite}/{cible}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    #[test]
    fn refuse_les_adresses_qui_pointent_vers_la_machine_ou_le_lan() {
        // 🔒 Le cœur du garde-fou. Chacune de ces adresses, acceptée, donnerait
        // à un lien posté par un pair un accès de lecture au réseau local.
        for ip in [
            "127.0.0.1",       // le service local (l'API JSON-RPC y écoute)
            "0.0.0.0",         //
            "10.1.2.3",        // LAN
            "192.168.1.1",     // la box
            "172.16.0.1",      // LAN
            "169.254.169.254", // métadonnées d'instance cloud
            "100.64.0.1",      // CGNAT
            "198.18.0.1",      // banc de test
        ] {
            let ip: IpAddr = ip.parse().expect("littéral de test valide");
            assert!(!adresse_publique(ip), "{ip} aurait dû être refusée");
        }
    }

    #[test]
    fn accepte_une_adresse_publique_ordinaire() {
        // Contrôle négatif : le refus ci-dessus tient aux plages, pas à un
        // « tout refuser » qui rendrait la fonctionnalité inerte.
        assert!(adresse_publique(IpAddr::V4(Ipv4Addr::new(
            93, 184, 216, 34
        ))));
        assert!(adresse_publique(IpAddr::V6(Ipv6Addr::new(
            0x2606, 0x2800, 0, 0, 0, 0, 0, 1
        ))));
    }

    #[test]
    fn refuse_une_ipv4_privee_deguisee_en_ipv6() {
        // ::ffff:127.0.0.1 — sans la branche `to_ipv4_mapped`, cette écriture
        // traverse toutes les vérifications IPv6 sans en déclencher aucune.
        let ip: IpAddr = "::ffff:127.0.0.1".parse().expect("littéral valide");
        assert!(!adresse_publique(ip));
        let ip: IpAddr = "::ffff:192.168.0.1".parse().expect("littéral valide");
        assert!(!adresse_publique(ip));
    }

    #[test]
    fn refuse_les_adresses_locales_ipv6() {
        for ip in ["::1", "fc00::1", "fd12::1", "fe80::1"] {
            let ip: IpAddr = ip.parse().expect("littéral valide");
            assert!(!adresse_publique(ip), "{ip} aurait dû être refusée");
        }
    }

    #[test]
    fn ne_garde_que_http_et_https() {
        // Avec `://`, le schéma est lu puis rejeté…
        for url in ["file:///etc/passwd", "ftp://exemple.fr/x"] {
            assert_eq!(decouper_url(url), Err(ErreurApercu::SchemaRefuse), "{url}");
        }
        // …sans `://`, le découpage échoue avant même de le lire. Deux motifs
        // différents, un seul résultat qui compte : rien n'est jamais récupéré.
        for url in [
            "javascript:alert(1)",
            "data:text/html,<b>x</b>",
            "//exemple.fr",
        ] {
            assert_eq!(decouper_url(url), Err(ErreurApercu::UrlInvalide), "{url}");
        }
    }

    #[test]
    fn refuse_des_identifiants_dans_lurl() {
        // 🔒 Deux raisons : ne pas transmettre un secret à un tiers, et ne pas
        // laisser `https://banque.fr@méchant.fr/` se faire passer pour la banque.
        assert_eq!(
            decouper_url("https://user:motdepasse@exemple.fr/page"),
            Err(ErreurApercu::UrlInvalide)
        );
    }

    #[test]
    fn decoupe_hote_et_port() {
        assert_eq!(
            decouper_url("https://exemple.fr/chemin?a=1"),
            Ok(("exemple.fr".into(), 443, true))
        );
        assert_eq!(
            decouper_url("http://exemple.fr:8080/x"),
            Ok(("exemple.fr".into(), 8080, false))
        );
        // IPv6 littérale, avec et sans port.
        assert_eq!(
            decouper_url("http://[2606:2800::1]/x"),
            Ok(("2606:2800::1".into(), 80, false))
        );
        assert_eq!(
            decouper_url("http://[::1]:48016/x"),
            Ok(("::1".into(), 48016, false))
        );
    }

    #[test]
    fn lit_les_balises_opengraph() {
        let html = r#"<html><head>
            <meta property="og:title" content="Un titre">
            <meta property="og:description" content="Une description">
            <meta property="og:image" content="https://exemple.fr/i.png">
        </head></html>"#;
        let (t, d, i) = extraire_meta(html);
        assert_eq!(t.as_deref(), Some("Un titre"));
        assert_eq!(d.as_deref(), Some("Une description"));
        assert_eq!(i.as_deref(), Some("https://exemple.fr/i.png"));
    }

    #[test]
    fn retombe_sur_le_titre_html() {
        let (t, d, i) = extraire_meta("<html><head><title>Sans OG</title></head></html>");
        assert_eq!(t.as_deref(), Some("Sans OG"));
        assert_eq!(d, None);
        assert_eq!(i, None);
    }

    #[test]
    fn nettoie_ce_qui_maquillerait_laffichage() {
        // Sauts de ligne et overrides bidi : de quoi faire afficher à l'aperçu
        // autre chose que ce qu'il contient.
        let html = "<meta property=\"og:title\" content=\"Ligne\u{202e}nu\nsuite\tfin\">";
        let (t, _, _) = extraire_meta(html);
        assert_eq!(t.as_deref(), Some("Lignenu suite fin"));
    }

    #[test]
    fn borne_la_longueur_dun_titre() {
        let long = "a".repeat(5_000);
        let html = format!("<meta property=\"og:title\" content=\"{long}\">");
        let (t, _, _) = extraire_meta(&html);
        assert_eq!(t.expect("un titre").chars().count(), 300);
    }

    #[test]
    fn resout_les_cibles_de_redirection() {
        let base = "https://exemple.fr/a/b?q=1";
        assert_eq!(
            resoudre_relatif(base, "/vers"),
            Ok("https://exemple.fr/vers".into())
        );
        assert_eq!(
            resoudre_relatif(base, "vers"),
            Ok("https://exemple.fr/vers".into())
        );
        // 🔒 Une redirection absolue repart au tour suivant, donc repasse par
        // `decouper_url` et `resoudre_publique`. C'est ce qui empêche
        // « page publique → 127.0.0.1 » de passer sur la foi du premier saut.
        assert_eq!(
            resoudre_relatif(base, "http://127.0.0.1:48016/x"),
            Ok("http://127.0.0.1:48016/x".into())
        );
        assert_eq!(
            decouper_url("http://127.0.0.1:48016/x").and_then(|(h, p, _)| resoudre_publique(&h, p)),
            Err(ErreurApercu::AdresseRefusee)
        );
    }

    #[test]
    fn ne_renvoie_rien_dune_page_vide() {
        let (t, d, i) = extraire_meta("<html><body>rien</body></html>");
        assert_eq!((t, d, i), (None, None, None));
    }
}
