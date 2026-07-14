//! Méthodes `profile.*` et `identity.self` : profil public local (D-027,
//! D-032) — pseudo, bio, avatar et bannière. L'avatar et la bannière arrivent
//! encodés en base64 à la frontière, sont validés (type MIME, taille décodée)
//! puis publiés dans le magasin de fichiers ; seul leur hash (hex 64) transite
//! ensuite dans l'API.

use serde_json::{json, Value};

use crate::error::NodeError;
use crate::hex;
use crate::node::Node;

use super::helpers::{b64_decode, param_opt_color, param_opt_id, param_opt_str, param_str};

/// Taille maximale d'un avatar une fois décodé (512 Kio).
const AVATAR_MAX_BYTES: usize = 512 * 1024;
/// Longueur base64 maximale correspondante (garde-fou avant décodage).
const AVATAR_MAX_B64_LEN: usize = AVATAR_MAX_BYTES.div_ceil(3) * 4;
/// Types MIME d'avatar acceptés.
const AVATAR_MIMES: [&str; 3] = ["image/png", "image/jpeg", "image/webp"];

/// Taille maximale d'une bannière une fois décodée (1 Mio) : format paysage,
/// plus grand qu'un avatar (D-032).
const BANNER_MAX_BYTES: usize = 1024 * 1024;
/// Longueur base64 maximale correspondante (garde-fou avant décodage).
const BANNER_MAX_B64_LEN: usize = BANNER_MAX_BYTES.div_ceil(3) * 4;
/// Types MIME de bannière acceptés (identiques à l'avatar).
const BANNER_MIMES: [&str; 3] = ["image/png", "image/jpeg", "image/webp"];

/// Aiguille les méthodes `profile.*` et `identity.self` vers le nœud.
pub(super) fn dispatch(node: &Node, method: &str, params: &Value) -> Result<Value, NodeError> {
    match method {
        "identity.self" => Ok(serde_json::to_value(node.self_profile()?)
            .map_err(|_| NodeError::Invalid("sérialisation"))?),
        "profile.get" => Ok(json!({
            "name": node.profile_name()?,
            "bio": node.profile_bio()?,
            "avatar": node.profile_avatar()?.map(|h| hex::encode(&h)),
            "banner": node.profile_banner()?.map(|h| hex::encode(&h)),
            "pronouns": node.profile_pronouns()?,
            "accent_color": node.profile_accent_color()?,
            "banner_color": node.profile_banner_color()?,
            "avatar_decoration": node.profile_avatar_decoration()?,
            "profile_effect": node.profile_profile_effect()?,
        })),
        "profile.set" => {
            let name = param_opt_str(params, "name")?;
            let bio = param_opt_str(params, "bio")?;
            let pronouns = param_opt_str(params, "pronouns")?;
            let accent_color = param_opt_color(params, "accent_color")?;
            let banner_color = param_opt_color(params, "banner_color")?;
            let avatar_decoration = param_opt_id(params, "avatar_decoration")?;
            let profile_effect = param_opt_id(params, "profile_effect")?;
            // `Option<Option<String>>` → `Option<Option<&str>>` sans copier :
            // le cœur revalide l'id (alphabet, borne) avant écriture.
            node.profile_update(
                name,
                bio,
                pronouns,
                accent_color,
                banner_color,
                avatar_decoration.as_ref().map(|o| o.as_deref()),
                profile_effect.as_ref().map(|o| o.as_deref()),
            )?;
            Ok(json!({}))
        }
        "profile.set_avatar" => {
            let hash = match params.get("data_b64") {
                Some(Value::Null) => node.profile_set_avatar(None)?,
                Some(Value::String(b64)) => {
                    let mime = param_str(params, "mime")?;
                    if !AVATAR_MIMES.contains(&mime) {
                        return Err(NodeError::Invalid(
                            "mime : image/png, image/jpeg ou image/webp requis",
                        ));
                    }
                    let octets = decode_avatar_b64(b64)?;
                    node.profile_set_avatar(Some((mime, octets)))?
                }
                Some(_) => {
                    return Err(NodeError::Invalid(
                        "data_b64 : chaîne base64 ou null requis",
                    ))
                }
                None => return Err(NodeError::Invalid("data_b64 requis (base64 ou null)")),
            };
            Ok(json!({ "avatar": hash.map(|h| hex::encode(&h)) }))
        }
        "profile.set_banner" => {
            let hash = match params.get("data_b64") {
                Some(Value::Null) => node.profile_set_banner(None)?,
                Some(Value::String(b64)) => {
                    let mime = param_str(params, "mime")?;
                    if !BANNER_MIMES.contains(&mime) {
                        return Err(NodeError::Invalid(
                            "mime : image/png, image/jpeg ou image/webp requis",
                        ));
                    }
                    let octets = decode_banner_b64(b64)?;
                    node.profile_set_banner(Some((mime, octets)))?
                }
                Some(_) => {
                    return Err(NodeError::Invalid(
                        "data_b64 : chaîne base64 ou null requis",
                    ))
                }
                None => return Err(NodeError::Invalid("data_b64 requis (base64 ou null)")),
            };
            Ok(json!({ "banner": hash.map(|h| hex::encode(&h)) }))
        }
        _ => Err(NodeError::Invalid("méthode inconnue")),
    }
}

/// Décode et borne les octets d'un avatar transmis en base64 (alphabet
/// standard avec padding `=`, décodeur partagé de la frontière) : au plus
/// 512 Kio décodés, contenu non vide.
fn decode_avatar_b64(b64: &str) -> Result<Vec<u8>, NodeError> {
    if b64.len() > AVATAR_MAX_B64_LEN {
        return Err(NodeError::Invalid(
            "avatar : 512 Kio maximum une fois décodé",
        ));
    }
    let octets = b64_decode(b64).ok_or(NodeError::Invalid("data_b64 : base64 invalide"))?;
    if octets.is_empty() {
        return Err(NodeError::Invalid("avatar : contenu vide"));
    }
    if octets.len() > AVATAR_MAX_BYTES {
        return Err(NodeError::Invalid(
            "avatar : 512 Kio maximum une fois décodé",
        ));
    }
    Ok(octets)
}

/// Décode et borne les octets d'une bannière transmise en base64 (alphabet
/// standard avec padding `=`, décodeur partagé de la frontière) : au plus
/// 1 Mio décodé, contenu non vide.
fn decode_banner_b64(b64: &str) -> Result<Vec<u8>, NodeError> {
    if b64.len() > BANNER_MAX_B64_LEN {
        return Err(NodeError::Invalid(
            "bannière : 1 Mio maximum une fois décodée",
        ));
    }
    let octets = b64_decode(b64).ok_or(NodeError::Invalid("data_b64 : base64 invalide"))?;
    if octets.is_empty() {
        return Err(NodeError::Invalid("bannière : contenu vide"));
    }
    if octets.len() > BANNER_MAX_BYTES {
        return Err(NodeError::Invalid(
            "bannière : 1 Mio maximum une fois décodée",
        ));
    }
    Ok(octets)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn avatar_b64_decodes_standard_vectors() {
        assert_eq!(decode_avatar_b64("Zg==").unwrap(), b"f");
        assert_eq!(decode_avatar_b64("Zm8=").unwrap(), b"fo");
        assert_eq!(decode_avatar_b64("Zm9v").unwrap(), b"foo");
        assert_eq!(decode_avatar_b64("Zm9vYg==").unwrap(), b"foob");
        assert_eq!(decode_avatar_b64("Zm9vYmE=").unwrap(), b"fooba");
        assert_eq!(decode_avatar_b64("Zm9vYmFy").unwrap(), b"foobar");
        // Octets non ASCII.
        assert_eq!(
            decode_avatar_b64("/////w==").unwrap(),
            [0xFF, 0xFF, 0xFF, 0xFF]
        );
    }

    #[test]
    fn avatar_b64_rejects_malformed_input() {
        // Vide ou longueur non multiple de 4 (padding obligatoire).
        assert!(decode_avatar_b64("").is_err());
        assert!(decode_avatar_b64("Zm9").is_err());
        // Caractères hors alphabet, espaces, padding mal placé.
        assert!(decode_avatar_b64("$$$$").is_err());
        assert!(decode_avatar_b64("Zm9v Zm9v").is_err());
        assert!(decode_avatar_b64("Zm=v").is_err());
        // Groupe entièrement rembourré : plus de 2 `=`.
        assert!(decode_avatar_b64("====").is_err());
        assert!(decode_avatar_b64("Z===").is_err());
    }

    #[test]
    fn avatar_bounds_are_enforced_before_publication() {
        // Trop long avant même le décodage.
        let huge = "A".repeat(AVATAR_MAX_B64_LEN + 4);
        assert!(decode_avatar_b64(&huge).is_err());
        // Valide.
        assert_eq!(decode_avatar_b64("Zm9v").unwrap(), b"foo");
    }

    #[test]
    fn banner_b64_decodes_and_rejects_like_avatar() {
        assert_eq!(decode_banner_b64("Zm9vYmFy").unwrap(), b"foobar");
        // Vide, longueur non multiple de 4, hors alphabet.
        assert!(decode_banner_b64("").is_err());
        assert!(decode_banner_b64("Zm9").is_err());
        assert!(decode_banner_b64("$$$$").is_err());
    }

    #[test]
    fn banner_bound_is_one_mib_larger_than_avatar() {
        // La bannière tolère 1 Mio (contre 512 Kio pour l'avatar) : une charge
        // rejetée par l'avatar reste acceptée par la bannière.
        let over_avatar = "A".repeat(AVATAR_MAX_B64_LEN + 4);
        assert!(decode_avatar_b64(&over_avatar).is_err());
        assert!(decode_banner_b64(&over_avatar).is_ok());
        // Trop long avant même le décodage (au-delà d'1 Mio).
        let huge = "A".repeat(BANNER_MAX_B64_LEN + 4);
        assert!(decode_banner_b64(&huge).is_err());
    }
}
