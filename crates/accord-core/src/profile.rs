//! Profil local de l'utilisateur (pseudo, bio, avatar, bannière) et
//! réconciliation des profils annoncés par les amis (D-027, D-032).
//!
//! Le profil local vit dans la table `meta` de la base SQLCipher (clés
//! [`META_NAME_KEY`], [`META_BIO_KEY`], [`META_AVATAR_KEY`],
//! [`META_BANNER_KEY`], pas de migration de schéma). Côté réception, le profil
//! porté par un message `Profile` n'est pris en compte que si l'émetteur est un
//! **ami** (anti-abus) et que les champs sont valides ; le pseudo remplace
//! alors le nom d'affichage du contact rendu par `friends.list`, la bio et les
//! hashes d'avatar et de bannière sont persistés dans `meta` sous des clés
//! dérivées du node_id du contact.

use accord_crypto::node_id_of;

use crate::db::{ContactState, Db};
use crate::error::CoreError;
use crate::group::state::{is_spoofing_char, is_valid_display_label, strip_spoofing_chars};

/// Clé de métadonnée du pseudo local dans la table `meta`.
const META_NAME_KEY: &str = "profile.name";
/// Clé de métadonnée de la bio locale dans la table `meta`.
const META_BIO_KEY: &str = "profile.bio";
/// Clé de métadonnée du hash d'avatar local dans la table `meta`.
const META_AVATAR_KEY: &str = "profile.avatar";
/// Clé de métadonnée du hash de bannière local dans la table `meta`.
const META_BANNER_KEY: &str = "profile.banner";
/// Clé de métadonnée des pronoms locaux dans la table `meta`.
const META_PRONOUNS_KEY: &str = "profile.pronouns";
/// Clé de métadonnée de la couleur d'accent locale dans la table `meta`.
const META_ACCENT_COLOR_KEY: &str = "profile.accent_color";
/// Clé de métadonnée de la couleur de bannière locale dans la table `meta`.
const META_BANNER_COLOR_KEY: &str = "profile.banner_color";
/// Préfixe des métadonnées de profil des contacts (`profile.peer.<hex>.bio`,
/// `profile.peer.<hex>.avatar`, `profile.peer.<hex>.banner`,
/// `profile.peer.<hex>.pronouns`, `profile.peer.<hex>.accent_color` et
/// `profile.peer.<hex>.banner_color`).
const PEER_META_PREFIX: &str = "profile.peer.";

/// Longueur minimale d'un pseudo (caractères, après trim).
pub const NAME_MIN_CHARS: usize = 2;
/// Longueur maximale d'un pseudo (caractères, après trim).
pub const NAME_MAX_CHARS: usize = 32;
/// Longueur maximale d'une bio (caractères, après trim).
pub const BIO_MAX_CHARS: usize = 2048;
/// Borne filaire d'une bio (octets UTF-8, alignée sur la limite de décodage
/// du message `Profile` côté protocole).
pub const BIO_MAX_BYTES: usize = 2048;
/// Longueur maximale de pronoms (caractères, après trim).
pub const PRONOUNS_MAX_CHARS: usize = 40;
/// Borne filaire de pronoms (octets UTF-8, alignée sur la limite de décodage
/// `profile.pronouns` côté protocole).
pub const PRONOUNS_MAX_BYTES: usize = 40;
/// Borne d'une couleur `0xRRGGBB` (accent ou bannière), alignée sur la limite
/// de décodage côté protocole.
pub const COLOR_MAX: u32 = 0xFF_FF_FF;

/// Profil d'un contact appliqué à l'ingestion d'un message `Profile`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerProfile {
    /// Pseudo canonique (trimé) appliqué au contact.
    pub name: String,
    /// Bio persistée (`None` si vide : effacée).
    pub bio: Option<String>,
    /// Hash d'avatar persisté (`None` si retiré).
    pub avatar: Option<[u8; 32]>,
    /// Hash de bannière persisté (`None` si retiré).
    pub banner: Option<[u8; 32]>,
    /// Pronoms persistés, meilleur effort (`None` si absents, vides ou
    /// entièrement composés de caractères indésirables une fois nettoyés).
    pub pronouns: Option<String>,
    /// Couleur d'accent persistée (`None` si absente ou hors bornes,
    /// silencieusement ignorée dans ce dernier cas).
    pub accent_color: Option<u32>,
    /// Couleur de bannière persistée (`None` si absente ou hors bornes,
    /// silencieusement ignorée dans ce dernier cas).
    pub banner_color: Option<u32>,
    /// Vrai si le hash d'avatar DIFFÈRE de celui déjà stocké pour ce contact
    /// (nouveau ou retiré). Seule une vraie nouveauté justifie une
    /// récupération en arrière-plan : une ré-annonce du même profil (ou un
    /// spam de hashes) ne crée alors aucune intention de téléchargement.
    pub avatar_changed: bool,
    /// Idem pour la bannière : vrai si le hash diffère de celui stocké.
    pub banner_changed: bool,
}

/// Valide un pseudo : 2 à 32 caractères après trim, sans caractère de
/// contrôle ni de format Unicode trompeur (bidi, largeur nulle — même garde
/// que les pseudos de serveur, [`is_spoofing_char`]). Rend la forme
/// canonique (trimée).
pub fn validate_name(name: &str) -> Result<&str, CoreError> {
    let trimmed = name.trim();
    let chars = trimmed.chars().count();
    if !(NAME_MIN_CHARS..=NAME_MAX_CHARS).contains(&chars) {
        return Err(CoreError::Invalid(
            "pseudo : 2 à 32 caractères requis (espaces de bord ignorés)",
        ));
    }
    if trimmed
        .chars()
        .any(|c| c.is_control() || is_spoofing_char(c))
    {
        return Err(CoreError::Invalid(
            "pseudo : caractères de contrôle ou de format trompeur interdits",
        ));
    }
    Ok(trimmed)
}

/// Valide des pronoms : au plus 40 caractères après trim (et 40 octets
/// UTF-8, borne filaire), sans caractère de contrôle ni de format trompeur
/// (réutilise [`is_valid_display_label`]). Des pronoms vides sont valides et
/// signifient « effacer », comme pour la bio. Rend la forme canonique
/// (trimée).
pub fn validate_pronouns(pronouns: &str) -> Result<&str, CoreError> {
    let trimmed = pronouns.trim();
    if trimmed.is_empty() {
        return Ok(trimmed);
    }
    if trimmed.len() > PRONOUNS_MAX_BYTES {
        return Err(CoreError::Invalid(
            "pronoms : 40 octets UTF-8 maximum une fois encodés",
        ));
    }
    if !is_valid_display_label(trimmed, PRONOUNS_MAX_CHARS) {
        return Err(CoreError::Invalid(
            "pronoms : 40 caractères maximum, sans caractère de contrôle ni de format trompeur",
        ));
    }
    Ok(trimmed)
}

/// Valide une couleur `0xRRGGBB` (accent ou bannière) : au plus 24 bits.
pub fn validate_color(color: u32) -> Result<u32, CoreError> {
    if color > COLOR_MAX {
        return Err(CoreError::Invalid(
            "couleur : 0xRRGGBB attendu (≤ 0xFFFFFF)",
        ));
    }
    Ok(color)
}

/// Sanitize des pronoms annoncés par un **pair** : meilleur effort plutôt
/// que rejet total (miroir de [`crate::presence::sanitize_peer_custom`]) —
/// caractères de contrôle et de format trompeur retirés, résultat trimé et
/// borné à [`PRONOUNS_MAX_CHARS`] caractères. Rend `None` si rien de
/// signifiant ne subsiste.
pub fn sanitize_peer_pronouns(text: &str) -> Option<String> {
    let cleaned: String = text
        .chars()
        .filter(|c| !c.is_control() && !is_spoofing_char(*c))
        .take(PRONOUNS_MAX_CHARS)
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Valide une bio : au plus 2048 caractères après trim (et 2048 octets UTF-8,
/// borne filaire), sans caractère de contrôle autre que les sauts de ligne et
/// tabulations. Une bio vide est valide et signifie « effacer ». Rend la
/// forme canonique (trimée).
pub fn validate_bio(bio: &str) -> Result<&str, CoreError> {
    let trimmed = bio.trim();
    if trimmed.chars().count() > BIO_MAX_CHARS {
        return Err(CoreError::Invalid("bio : 2048 caractères maximum"));
    }
    if trimmed.len() > BIO_MAX_BYTES {
        return Err(CoreError::Invalid(
            "bio : 2048 octets UTF-8 maximum une fois encodée",
        ));
    }
    if trimmed
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
    {
        return Err(CoreError::Invalid("bio : caractères de contrôle interdits"));
    }
    Ok(trimmed)
}

/// Enregistre le pseudo local après validation ; rend la forme canonique
/// stockée (trimée).
pub fn set_local_name(db: &Db, name: &str) -> Result<String, CoreError> {
    let canon = validate_name(name)?;
    db.set_meta(META_NAME_KEY, canon.as_bytes())?;
    Ok(canon.to_string())
}

/// Pseudo local, s'il a déjà été défini.
pub fn local_name(db: &Db) -> Result<Option<String>, CoreError> {
    match db.meta(META_NAME_KEY)? {
        None => Ok(None),
        Some(bytes) => {
            Ok(Some(String::from_utf8(bytes).map_err(|_| {
                CoreError::Invalid("pseudo local corrompu")
            })?))
        }
    }
}

/// Enregistre la bio locale après validation ; une bio vide (après trim)
/// l'efface. Rend la forme canonique stockée (`None` si effacée).
pub fn set_local_bio(db: &Db, bio: &str) -> Result<Option<String>, CoreError> {
    let canon = validate_bio(bio)?;
    db.set_meta(META_BIO_KEY, canon.as_bytes())?;
    if canon.is_empty() {
        Ok(None)
    } else {
        Ok(Some(canon.to_string()))
    }
}

/// Bio locale, si elle est définie et non vide.
pub fn local_bio(db: &Db) -> Result<Option<String>, CoreError> {
    read_bio(db, META_BIO_KEY)
}

/// Enregistre (ou efface, avec `None`) le hash d'avatar local.
pub fn set_local_avatar(db: &Db, avatar: Option<&[u8; 32]>) -> Result<(), CoreError> {
    db.set_meta(META_AVATAR_KEY, avatar.map_or(&[][..], |h| &h[..]))
}

/// Hash d'avatar local, s'il est défini.
pub fn local_avatar(db: &Db) -> Result<Option<[u8; 32]>, CoreError> {
    read_avatar(db, META_AVATAR_KEY)
}

/// Enregistre (ou efface, avec `None`) le hash de bannière local.
pub fn set_local_banner(db: &Db, banner: Option<&[u8; 32]>) -> Result<(), CoreError> {
    db.set_meta(META_BANNER_KEY, banner.map_or(&[][..], |h| &h[..]))
}

/// Hash de bannière local, s'il est défini.
pub fn local_banner(db: &Db) -> Result<Option<[u8; 32]>, CoreError> {
    read_banner(db, META_BANNER_KEY)
}

/// Enregistre des pronoms locaux après validation ; des pronoms vides
/// (après trim) les effacent. Rend la forme canonique stockée (`None` si
/// effacés).
pub fn set_local_pronouns(db: &Db, pronouns: &str) -> Result<Option<String>, CoreError> {
    let canon = validate_pronouns(pronouns)?;
    db.set_meta(META_PRONOUNS_KEY, canon.as_bytes())?;
    if canon.is_empty() {
        Ok(None)
    } else {
        Ok(Some(canon.to_string()))
    }
}

/// Pronoms locaux, s'ils sont définis et non vides.
pub fn local_pronouns(db: &Db) -> Result<Option<String>, CoreError> {
    read_string(db, META_PRONOUNS_KEY, "pronoms stockés corrompus")
}

/// Enregistre (ou efface, avec `None`) la couleur d'accent locale après
/// validation.
pub fn set_local_accent_color(db: &Db, color: Option<u32>) -> Result<(), CoreError> {
    let validated = color.map(validate_color).transpose()?;
    write_color_meta(db, META_ACCENT_COLOR_KEY, validated)
}

/// Couleur d'accent locale, si elle est définie.
pub fn local_accent_color(db: &Db) -> Result<Option<u32>, CoreError> {
    read_color(db, META_ACCENT_COLOR_KEY)
}

/// Enregistre (ou efface, avec `None`) la couleur de bannière locale après
/// validation.
pub fn set_local_banner_color(db: &Db, color: Option<u32>) -> Result<(), CoreError> {
    let validated = color.map(validate_color).transpose()?;
    write_color_meta(db, META_BANNER_COLOR_KEY, validated)
}

/// Couleur de bannière locale, si elle est définie.
pub fn local_banner_color(db: &Db) -> Result<Option<u32>, CoreError> {
    read_color(db, META_BANNER_COLOR_KEY)
}

/// Bio persistée d'un contact (annoncée par lui), si non vide.
pub fn peer_bio(db: &Db, node_id: &[u8; 32]) -> Result<Option<String>, CoreError> {
    read_bio(db, &peer_meta_key(node_id, "bio"))
}

/// Hash d'avatar persisté d'un contact (annoncé par lui), s'il existe.
pub fn peer_avatar(db: &Db, node_id: &[u8; 32]) -> Result<Option<[u8; 32]>, CoreError> {
    read_avatar(db, &peer_meta_key(node_id, "avatar"))
}

/// Hash de bannière persisté d'un contact (annoncé par lui), s'il existe.
pub fn peer_banner(db: &Db, node_id: &[u8; 32]) -> Result<Option<[u8; 32]>, CoreError> {
    read_banner(db, &peer_meta_key(node_id, "banner"))
}

/// Pronoms persistés d'un contact (annoncés par lui), si non vides.
pub fn peer_pronouns(db: &Db, node_id: &[u8; 32]) -> Result<Option<String>, CoreError> {
    read_string(
        db,
        &peer_meta_key(node_id, "pronouns"),
        "pronoms de pair corrompus",
    )
}

/// Couleur d'accent persistée d'un contact (annoncée par lui), si définie.
pub fn peer_accent_color(db: &Db, node_id: &[u8; 32]) -> Result<Option<u32>, CoreError> {
    read_color(db, &peer_meta_key(node_id, "accent_color"))
}

/// Couleur de bannière persistée d'un contact (annoncée par lui), si
/// définie.
pub fn peer_banner_color(db: &Db, node_id: &[u8; 32]) -> Result<Option<u32>, CoreError> {
    read_color(db, &peer_meta_key(node_id, "banner_color"))
}

/// Ingère le profil annoncé par un pair (message `Profile`, authentifié par
/// la session chiffrée : `peer_pubkey` est la clé de session, pas un champ).
///
/// Anti-abus : seuls les **amis** sont pris en compte ; toute autre relation
/// (inconnu, demande en attente, bloqué) est ignorée silencieusement
/// (`Ok(None)`). Rend le profil canonique appliqué au contact : pseudo dans
/// `contacts.display_name`, bio, hashes d'avatar et de bannière, pronoms et
/// couleurs dans `meta` (une bio, des pronoms, un avatar ou une bannière
/// absents **effacent** la valeur connue).
///
/// Validation à deux vitesses, cohérente avec l'anti-usurpation des pseudos
/// de serveur :
/// - **pseudo et bio** restent stricts (tout ou rien : un pseudo ou une bio
///   invalide rejette le message entier, sans effet) — le pseudo se voit
///   d'abord retirer ses caractères de format Unicode trompeurs
///   ([`strip_spoofing_chars`]) en meilleur effort (miroir de
///   [`crate::presence::sanitize_peer_custom`]), pour qu'un seul caractère
///   indésirable ne fasse pas échouer tout le profil ; la validation stricte
///   (longueur, contrôle) s'applique ensuite normalement ;
/// - **pronoms et couleurs** sont des champs annexes, toujours en meilleur
///   effort ([`sanitize_peer_pronouns`], bornes de couleur ignorées si
///   dépassées) : ils ne peuvent jamais faire échouer l'ingestion.
#[allow(clippy::too_many_arguments)]
pub fn ingest_peer_profile(
    db: &Db,
    peer_pubkey: &[u8; 32],
    name: &str,
    bio: &str,
    avatar: Option<[u8; 32]>,
    banner: Option<[u8; 32]>,
    pronouns: Option<&str>,
    accent_color: Option<u32>,
    banner_color: Option<u32>,
    now_ms: u64,
) -> Result<Option<PeerProfile>, CoreError> {
    let node_id = node_id_of(peer_pubkey).0;
    match db.contact(&node_id)?.map(|c| c.state) {
        Some(ContactState::Friend) => {
            // Tout valider avant la première écriture (tout ou rien) pour le
            // pseudo et la bio.
            let sanitized_name = strip_spoofing_chars(name);
            let canon_name = validate_name(&sanitized_name)?;
            let canon_bio = validate_bio(bio)?;
            // Champs annexes : meilleur effort, jamais de rejet du profil.
            let canon_pronouns = pronouns.and_then(sanitize_peer_pronouns);
            let canon_accent = accent_color.filter(|c| *c <= COLOR_MAX);
            let canon_banner_color = banner_color.filter(|c| *c <= COLOR_MAX);

            // Diff avant écriture : ne signaler « changé » (donc ne déclencher
            // une récupération) que si le hash diffère du hash déjà stocké.
            // Racine du correctif anti-DoS : sans ce diff, chaque `Profile`
            // (même identique) créait une intention de téléchargement.
            let avatar_changed = avatar != peer_avatar(db, &node_id)?;
            let banner_changed = banner != peer_banner(db, &node_id)?;

            db.set_contact_name(&node_id, canon_name, now_ms)?;
            db.set_meta(&peer_meta_key(&node_id, "bio"), canon_bio.as_bytes())?;
            db.set_meta(
                &peer_meta_key(&node_id, "avatar"),
                avatar.as_ref().map_or(&[][..], |h| &h[..]),
            )?;
            db.set_meta(
                &peer_meta_key(&node_id, "banner"),
                banner.as_ref().map_or(&[][..], |h| &h[..]),
            )?;
            db.set_meta(
                &peer_meta_key(&node_id, "pronouns"),
                canon_pronouns.as_deref().unwrap_or("").as_bytes(),
            )?;
            write_color_meta(db, &peer_meta_key(&node_id, "accent_color"), canon_accent)?;
            write_color_meta(
                db,
                &peer_meta_key(&node_id, "banner_color"),
                canon_banner_color,
            )?;
            Ok(Some(PeerProfile {
                name: canon_name.to_string(),
                bio: (!canon_bio.is_empty()).then(|| canon_bio.to_string()),
                avatar,
                banner,
                pronouns: canon_pronouns,
                accent_color: canon_accent,
                banner_color: canon_banner_color,
                avatar_changed,
                banner_changed,
            }))
        }
        _ => Ok(None),
    }
}

/// Clé `meta` d'un champ de profil d'un contact : `profile.peer.<hex>.<champ>`.
fn peer_meta_key(node_id: &[u8; 32], field: &str) -> String {
    use std::fmt::Write;
    let mut key = String::with_capacity(PEER_META_PREFIX.len() + 64 + 1 + field.len());
    key.push_str(PEER_META_PREFIX);
    for b in node_id {
        // L'écriture dans une String ne peut pas échouer.
        let _ = write!(key, "{b:02x}");
    }
    key.push('.');
    key.push_str(field);
    key
}

/// Lit une chaîne UTF-8 stockée sous `key` (`None` si absente ou vide).
/// `what` qualifie l'erreur si le contenu stocké n'est pas de l'UTF-8 valide.
fn read_string(db: &Db, key: &str, what: &'static str) -> Result<Option<String>, CoreError> {
    match db.meta(key)? {
        None => Ok(None),
        Some(bytes) if bytes.is_empty() => Ok(None),
        Some(bytes) => Ok(Some(
            String::from_utf8(bytes).map_err(|_| CoreError::Invalid(what))?,
        )),
    }
}

/// Lit une bio stockée sous `key` (`None` si absente ou vide).
fn read_bio(db: &Db, key: &str) -> Result<Option<String>, CoreError> {
    read_string(db, key, "bio stockée corrompue")
}

/// Lit un hash d'avatar stocké sous `key` (`None` si absent ou effacé).
fn read_avatar(db: &Db, key: &str) -> Result<Option<[u8; 32]>, CoreError> {
    match db.meta(key)? {
        None => Ok(None),
        Some(bytes) if bytes.is_empty() => Ok(None),
        Some(bytes) => {
            Ok(Some(bytes.try_into().map_err(|_| {
                CoreError::Invalid("hash d'avatar stocké corrompu")
            })?))
        }
    }
}

/// Lit un hash de bannière stocké sous `key` (`None` si absent ou effacé).
fn read_banner(db: &Db, key: &str) -> Result<Option<[u8; 32]>, CoreError> {
    match db.meta(key)? {
        None => Ok(None),
        Some(bytes) if bytes.is_empty() => Ok(None),
        Some(bytes) => {
            Ok(Some(bytes.try_into().map_err(|_| {
                CoreError::Invalid("hash de bannière stocké corrompu")
            })?))
        }
    }
}

/// Lit une couleur `0xRRGGBB` stockée sous `key` (`None` si absente ou
/// effacée).
fn read_color(db: &Db, key: &str) -> Result<Option<u32>, CoreError> {
    match db.meta(key)? {
        None => Ok(None),
        Some(bytes) if bytes.is_empty() => Ok(None),
        Some(bytes) => {
            let arr: [u8; 4] = bytes
                .try_into()
                .map_err(|_| CoreError::Invalid("couleur stockée corrompue"))?;
            Ok(Some(u32::from_be_bytes(arr)))
        }
    }
}

/// Enregistre (ou efface, avec `None`) une couleur `0xRRGGBB` sous `key`.
fn write_color_meta(db: &Db, key: &str, color: Option<u32>) -> Result<(), CoreError> {
    match color {
        Some(c) => db.set_meta(key, &c.to_be_bytes()),
        None => db.set_meta(key, &[]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Contact;

    fn db() -> Db {
        Db::open_in_memory(&[3u8; 32]).unwrap()
    }

    fn friend(db: &Db, pubkey: &[u8; 32], state: ContactState) {
        db.upsert_contact(&Contact {
            node_id: node_id_of(pubkey).0,
            pubkey: *pubkey,
            display_name: "étiquette-locale".into(),
            state,
            added_ms: 1,
            last_seen_ms: 1,
        })
        .unwrap();
    }

    #[test]
    fn validation_enforces_bounds_and_trims() {
        assert_eq!(validate_name("  Anna  ").unwrap(), "Anna");
        assert_eq!(validate_name("ab").unwrap(), "ab");
        assert_eq!(validate_name(&"x".repeat(32)).unwrap(), "x".repeat(32));
        // Bornes en caractères, pas en octets : 32 caractères multi-octets.
        assert_eq!(validate_name(&"é".repeat(32)).unwrap(), "é".repeat(32));
        assert!(validate_name("a").is_err());
        assert!(validate_name("   a   ").is_err());
        assert!(validate_name("").is_err());
        assert!(validate_name(&"x".repeat(33)).is_err());
        assert!(validate_name("an\u{0007}na").is_err());
    }

    #[test]
    fn validate_name_rejects_spoofing_chars() {
        // U+202E RIGHT-TO-LEFT OVERRIDE (RLO) : usurpation visuelle, refusée
        // comme pour les pseudos de serveur (D-045 / anti-spoofing).
        assert!(validate_name("An\u{202E}na").is_err());
        // Zero-width space, BOM : mêmes garde-fous.
        assert!(validate_name("\u{200B}Anna").is_err());
        assert!(validate_name("Anna\u{FEFF}").is_err());
    }

    #[test]
    fn bio_validation_bounds_and_control_chars() {
        assert_eq!(validate_bio("").unwrap(), "");
        assert_eq!(validate_bio("   ").unwrap(), "");
        assert_eq!(validate_bio("  salut  ").unwrap(), "salut");
        // Sauts de ligne et tabulations autorisés, autres contrôles refusés.
        assert_eq!(
            validate_bio("ligne 1\nligne 2\tfin").unwrap(),
            "ligne 1\nligne 2\tfin"
        );
        assert!(validate_bio("bip\u{0007}").is_err());
        // Borne en caractères.
        assert!(validate_bio(&"x".repeat(2048)).is_ok());
        assert!(validate_bio(&"x".repeat(2049)).is_err());
        // Borne filaire en octets : 1500 caractères « é » = 3000 octets.
        assert!(validate_bio(&"é".repeat(1500)).is_err());
        assert!(validate_bio(&"é".repeat(1024)).is_ok());
    }

    #[test]
    fn validate_pronouns_bounds_control_and_spoofing_chars() {
        assert_eq!(validate_pronouns("  il/lui  ").unwrap(), "il/lui");
        // Vide (ou seulement des espaces) = effacer, valide.
        assert_eq!(validate_pronouns("").unwrap(), "");
        assert_eq!(validate_pronouns("   ").unwrap(), "");
        // Borne en caractères.
        assert!(validate_pronouns(&"x".repeat(40)).is_ok());
        assert!(validate_pronouns(&"x".repeat(41)).is_err());
        // Caractère de contrôle et caractère de format trompeur refusés.
        assert!(validate_pronouns("il\u{0007}lui").is_err());
        assert!(validate_pronouns("il\u{202E}lui").is_err());
    }

    #[test]
    fn validate_color_rejects_beyond_24_bits() {
        assert_eq!(validate_color(0).unwrap(), 0);
        assert_eq!(validate_color(0xFF_FF_FF).unwrap(), 0xFF_FF_FF);
        assert!(validate_color(0x0100_0000).is_err());
    }

    #[test]
    fn sanitize_peer_pronouns_strips_bad_chars_without_rejecting() {
        assert_eq!(sanitize_peer_pronouns("  il/lui  "), Some("il/lui".into()));
        // Caractère de contrôle et de format trompeur retirés, le reste
        // conservé.
        assert_eq!(
            sanitize_peer_pronouns("il\u{202E}/\u{0007}lui"),
            Some("il/lui".into())
        );
        // Rien de signifiant : `None`.
        assert_eq!(sanitize_peer_pronouns("\u{200B}\u{FEFF}"), None);
        assert_eq!(sanitize_peer_pronouns(""), None);
        // Bornée aux 40 caractères.
        let long = "x".repeat(60);
        let cleaned = sanitize_peer_pronouns(&long).unwrap();
        assert!(cleaned.chars().count() <= PRONOUNS_MAX_CHARS);
    }

    #[test]
    fn local_name_roundtrips_and_defaults_to_none() {
        let db = db();
        assert_eq!(local_name(&db).unwrap(), None);
        assert_eq!(set_local_name(&db, "  Anna  ").unwrap(), "Anna");
        assert_eq!(local_name(&db).unwrap(), Some("Anna".into()));
        // Remplacement.
        set_local_name(&db, "Bertrand").unwrap();
        assert_eq!(local_name(&db).unwrap(), Some("Bertrand".into()));
        // Invalide : refusé, l'ancien pseudo reste.
        assert!(set_local_name(&db, "x").is_err());
        assert_eq!(local_name(&db).unwrap(), Some("Bertrand".into()));
    }

    #[test]
    fn local_bio_roundtrips_and_empty_clears() {
        let db = db();
        assert_eq!(local_bio(&db).unwrap(), None);
        assert_eq!(
            set_local_bio(&db, "  ma bio  ").unwrap(),
            Some("ma bio".into())
        );
        assert_eq!(local_bio(&db).unwrap(), Some("ma bio".into()));
        // Vide = effacer.
        assert_eq!(set_local_bio(&db, "").unwrap(), None);
        assert_eq!(local_bio(&db).unwrap(), None);
        // Invalide : refusée, sans effet.
        set_local_bio(&db, "durable").unwrap();
        assert!(set_local_bio(&db, &"x".repeat(2049)).is_err());
        assert_eq!(local_bio(&db).unwrap(), Some("durable".into()));
    }

    #[test]
    fn local_avatar_roundtrips_and_none_clears() {
        let db = db();
        assert_eq!(local_avatar(&db).unwrap(), None);
        set_local_avatar(&db, Some(&[9u8; 32])).unwrap();
        assert_eq!(local_avatar(&db).unwrap(), Some([9u8; 32]));
        set_local_avatar(&db, None).unwrap();
        assert_eq!(local_avatar(&db).unwrap(), None);
    }

    #[test]
    fn local_banner_roundtrips_and_none_clears() {
        let db = db();
        assert_eq!(local_banner(&db).unwrap(), None);
        set_local_banner(&db, Some(&[7u8; 32])).unwrap();
        assert_eq!(local_banner(&db).unwrap(), Some([7u8; 32]));
        set_local_banner(&db, None).unwrap();
        assert_eq!(local_banner(&db).unwrap(), None);
        // Bannière et avatar sont stockés sous des clés distinctes.
        set_local_avatar(&db, Some(&[9u8; 32])).unwrap();
        set_local_banner(&db, Some(&[7u8; 32])).unwrap();
        assert_eq!(local_avatar(&db).unwrap(), Some([9u8; 32]));
        assert_eq!(local_banner(&db).unwrap(), Some([7u8; 32]));
    }

    #[test]
    fn local_pronouns_roundtrips_and_empty_clears() {
        let db = db();
        assert_eq!(local_pronouns(&db).unwrap(), None);
        assert_eq!(
            set_local_pronouns(&db, "  il/lui  ").unwrap(),
            Some("il/lui".into())
        );
        assert_eq!(local_pronouns(&db).unwrap(), Some("il/lui".into()));
        // Vide = effacer.
        assert_eq!(set_local_pronouns(&db, "").unwrap(), None);
        assert_eq!(local_pronouns(&db).unwrap(), None);
        // Invalide : refusés, sans effet.
        set_local_pronouns(&db, "elle/iel").unwrap();
        assert!(set_local_pronouns(&db, &"x".repeat(41)).is_err());
        assert_eq!(local_pronouns(&db).unwrap(), Some("elle/iel".into()));
    }

    #[test]
    fn local_colors_roundtrip_and_none_clears() {
        let db = db();
        assert_eq!(local_accent_color(&db).unwrap(), None);
        assert_eq!(local_banner_color(&db).unwrap(), None);
        set_local_accent_color(&db, Some(0x00_FF_AA)).unwrap();
        set_local_banner_color(&db, Some(0x11_22_33)).unwrap();
        assert_eq!(local_accent_color(&db).unwrap(), Some(0x00_FF_AA));
        assert_eq!(local_banner_color(&db).unwrap(), Some(0x11_22_33));
        // Retrait.
        set_local_accent_color(&db, None).unwrap();
        assert_eq!(local_accent_color(&db).unwrap(), None);
        assert_eq!(local_banner_color(&db).unwrap(), Some(0x11_22_33));
        // Hors bornes : refusé, sans effet.
        assert!(set_local_banner_color(&db, Some(0x0100_0000)).is_err());
        assert_eq!(local_banner_color(&db).unwrap(), Some(0x11_22_33));
    }

    #[test]
    fn peer_profile_updates_friend_contact_only() {
        let db = db();
        let peer = [7u8; 32];
        friend(&db, &peer, ContactState::Friend);
        let applied = ingest_peer_profile(
            &db,
            &peer,
            " Anna ",
            " sa bio ",
            Some([4u8; 32]),
            Some([6u8; 32]),
            Some(" il/lui "),
            Some(0x00_FF_AA),
            Some(0x11_22_33),
            9,
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            applied,
            PeerProfile {
                name: "Anna".into(),
                bio: Some("sa bio".into()),
                avatar: Some([4u8; 32]),
                banner: Some([6u8; 32]),
                pronouns: Some("il/lui".into()),
                accent_color: Some(0x00_FF_AA),
                banner_color: Some(0x11_22_33),
                avatar_changed: true,
                banner_changed: true,
            }
        );
        let node_id = node_id_of(&peer).0;
        let contact = db.contact(&node_id).unwrap().unwrap();
        assert_eq!(contact.display_name, "Anna");
        assert_eq!(contact.last_seen_ms, 9);
        assert_eq!(peer_bio(&db, &node_id).unwrap(), Some("sa bio".into()));
        assert_eq!(peer_avatar(&db, &node_id).unwrap(), Some([4u8; 32]));
        assert_eq!(peer_banner(&db, &node_id).unwrap(), Some([6u8; 32]));
        assert_eq!(peer_pronouns(&db, &node_id).unwrap(), Some("il/lui".into()));
        assert_eq!(peer_accent_color(&db, &node_id).unwrap(), Some(0x00_FF_AA));
        assert_eq!(peer_banner_color(&db, &node_id).unwrap(), Some(0x11_22_33));
    }

    #[test]
    fn re_annonce_du_meme_avatar_n_est_pas_marquee_changee() {
        let db = db();
        let peer = [7u8; 32];
        friend(&db, &peer, ContactState::Friend);
        let ingest = |avatar, banner, ts| {
            ingest_peer_profile(
                &db, &peer, "Anna", "bio", avatar, banner, None, None, None, ts,
            )
            .unwrap()
            .unwrap()
        };
        // Première annonce : hashes inédits → changés (récupération justifiée).
        let a = ingest(Some([4u8; 32]), Some([6u8; 32]), 1);
        assert!(a.avatar_changed && a.banner_changed);
        // Ré-annonce du MÊME profil : rien de changé → aucune récupération.
        let b = ingest(Some([4u8; 32]), Some([6u8; 32]), 2);
        assert!(!b.avatar_changed && !b.banner_changed);
        // Un vrai nouveau hash d'avatar est de nouveau changé (une fois).
        let c = ingest(Some([5u8; 32]), Some([6u8; 32]), 3);
        assert!(c.avatar_changed && !c.banner_changed);
    }

    #[test]
    fn peer_profile_empty_fields_clear_previous_values() {
        let db = db();
        let peer = [7u8; 32];
        friend(&db, &peer, ContactState::Friend);
        ingest_peer_profile(
            &db,
            &peer,
            "Anna",
            "bio",
            Some([4u8; 32]),
            Some([6u8; 32]),
            Some("il/lui"),
            Some(0x00_FF_AA),
            Some(0x11_22_33),
            1,
        )
        .unwrap();
        // Nouvelle annonce sans bio, avatar, bannière, pronoms ni couleurs :
        // les valeurs connues s'effacent.
        let applied = ingest_peer_profile(&db, &peer, "Anna", "", None, None, None, None, None, 2)
            .unwrap()
            .unwrap();
        assert_eq!(applied.bio, None);
        assert_eq!(applied.avatar, None);
        assert_eq!(applied.banner, None);
        assert_eq!(applied.pronouns, None);
        assert_eq!(applied.accent_color, None);
        assert_eq!(applied.banner_color, None);
        let node_id = node_id_of(&peer).0;
        assert_eq!(peer_bio(&db, &node_id).unwrap(), None);
        assert_eq!(peer_avatar(&db, &node_id).unwrap(), None);
        assert_eq!(peer_banner(&db, &node_id).unwrap(), None);
        assert_eq!(peer_pronouns(&db, &node_id).unwrap(), None);
        assert_eq!(peer_accent_color(&db, &node_id).unwrap(), None);
        assert_eq!(peer_banner_color(&db, &node_id).unwrap(), None);
    }

    #[test]
    fn peer_profile_from_non_friend_is_silently_ignored() {
        let db = db();
        let unknown = [8u8; 32];
        assert_eq!(
            ingest_peer_profile(&db, &unknown, "Anna", "", None, None, None, None, None, 1)
                .unwrap(),
            None
        );
        for state in [
            ContactState::PendingIn,
            ContactState::PendingOut,
            ContactState::Blocked,
        ] {
            let peer = [state as u8 + 10; 32];
            friend(&db, &peer, state);
            assert_eq!(
                ingest_peer_profile(
                    &db,
                    &peer,
                    "Anna",
                    "bio",
                    Some([1u8; 32]),
                    Some([2u8; 32]),
                    Some("il/lui"),
                    Some(0x00_FF_AA),
                    Some(0x11_22_33),
                    1
                )
                .unwrap(),
                None
            );
            let node_id = node_id_of(&peer).0;
            let contact = db.contact(&node_id).unwrap().unwrap();
            assert_eq!(contact.display_name, "étiquette-locale");
            assert_eq!(peer_bio(&db, &node_id).unwrap(), None);
            assert_eq!(peer_avatar(&db, &node_id).unwrap(), None);
            assert_eq!(peer_banner(&db, &node_id).unwrap(), None);
            assert_eq!(peer_pronouns(&db, &node_id).unwrap(), None);
            assert_eq!(peer_accent_color(&db, &node_id).unwrap(), None);
            assert_eq!(peer_banner_color(&db, &node_id).unwrap(), None);
        }
    }

    #[test]
    fn invalid_peer_profile_from_friend_is_rejected_without_effect() {
        let db = db();
        let peer = [7u8; 32];
        friend(&db, &peer, ContactState::Friend);
        // Pseudo invalide.
        assert!(ingest_peer_profile(
            &db,
            &peer,
            &"x".repeat(33),
            "",
            None,
            None,
            None,
            None,
            None,
            1
        )
        .is_err());
        // Bio invalide : rien n'est écrit, pas même le pseudo pourtant valide.
        assert!(ingest_peer_profile(
            &db,
            &peer,
            "Anna",
            &"x".repeat(2049),
            None,
            None,
            None,
            None,
            None,
            1
        )
        .is_err());
        let node_id = node_id_of(&peer).0;
        let contact = db.contact(&node_id).unwrap().unwrap();
        assert_eq!(contact.display_name, "étiquette-locale");
        assert_eq!(peer_bio(&db, &node_id).unwrap(), None);
    }

    #[test]
    fn peer_profile_strips_spoofing_chars_from_display_name_without_rejecting() {
        // Un pseudo de pair truffé de caractères de format trompeurs
        // (RLO, U+202E) n'est jamais rejeté en bloc pour ce seul motif :
        // les caractères indésirables sont retirés, puis la validation
        // normale (longueur, contrôle) s'applique au résultat nettoyé.
        let db = db();
        let peer = [7u8; 32];
        friend(&db, &peer, ContactState::Friend);
        let applied = ingest_peer_profile(
            &db,
            &peer,
            "An\u{202E}na",
            "",
            None,
            None,
            None,
            None,
            None,
            1,
        )
        .unwrap()
        .unwrap();
        assert_eq!(applied.name, "Anna");
        let node_id = node_id_of(&peer).0;
        assert_eq!(db.contact(&node_id).unwrap().unwrap().display_name, "Anna");
    }

    #[test]
    fn peer_profile_sanitizes_pronouns_and_ignores_out_of_range_colors() {
        // Pronoms avec un caractère indésirable : nettoyés, jamais de rejet
        // total du profil pour ce champ annexe. Couleurs hors bornes :
        // ignorées (`None`), le reste du profil s'applique normalement.
        let db = db();
        let peer = [7u8; 32];
        friend(&db, &peer, ContactState::Friend);
        let applied = ingest_peer_profile(
            &db,
            &peer,
            "Anna",
            "",
            None,
            None,
            Some("il\u{202E}/lui"),
            Some(0x0100_0000), // hors bornes (> 0xFFFFFF)
            Some(0x00_11_22),  // dans les bornes
            1,
        )
        .unwrap()
        .unwrap();
        assert_eq!(applied.pronouns, Some("il/lui".into()));
        assert_eq!(applied.accent_color, None);
        assert_eq!(applied.banner_color, Some(0x00_11_22));
        let node_id = node_id_of(&peer).0;
        assert_eq!(peer_pronouns(&db, &node_id).unwrap(), Some("il/lui".into()));
        assert_eq!(peer_accent_color(&db, &node_id).unwrap(), None);
        assert_eq!(peer_banner_color(&db, &node_id).unwrap(), Some(0x00_11_22));
    }

    #[test]
    fn peer_meta_keys_are_distinct_per_contact_and_field() {
        let a = peer_meta_key(&[1u8; 32], "bio");
        let b = peer_meta_key(&[1u8; 32], "avatar");
        let c = peer_meta_key(&[2u8; 32], "bio");
        let d = peer_meta_key(&[1u8; 32], "banner");
        assert!(a.starts_with("profile.peer.01"));
        assert!(a.ends_with(".bio"));
        assert_ne!(a, b);
        assert_ne!(a, c);
        assert_ne!(b, d);
        assert!(d.ends_with(".banner"));
    }
}
