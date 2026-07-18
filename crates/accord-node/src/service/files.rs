//! Méthodes `files.*` : partage de fichiers (publication, lecture en ligne,
//! état, sauvegarde).
//!
//! La frontière transporte les racines Merkle en hexadécimal (64 caractères)
//! et les petits contenus en base64 (`files.read`, borné à 8 Mio — au-delà,
//! `files.save` copie le blob sans le faire transiter par le canal JSON).

use std::path::Path;

use accord_core::files::merkle;
use serde_json::{json, Value};

use crate::error::NodeError;
use crate::hex;
use crate::node::Node;

use super::helpers::{b64_decode, param_str};

/// Taille maximale d'une lecture en ligne (`files.read`), en octets.
const LECTURE_EN_LIGNE_MAX: u64 = 8 * 1024 * 1024;

/// Racine Merkle des paramètres (hexadécimal, 64 caractères).
fn param_racine(params: &Value) -> Result<[u8; 32], NodeError> {
    hex::decode::<32>(param_str(params, "merkle_root")?)
        .ok_or(NodeError::Invalid("racine Merkle invalide"))
}

/// Indice de source optionnel (`hint` : clé publique hexadécimale). Absent :
/// `None` ; présent mais illisible : erreur franche.
fn param_indice(params: &Value) -> Result<Option<[u8; 32]>, NodeError> {
    match params.get("hint") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => hex::decode::<32>(s)
            .map(Some)
            .ok_or(NodeError::Invalid("indice de source invalide")),
        Some(_) => Err(NodeError::Invalid("indice de source : chaîne requise")),
    }
}

/// Alphabet base64 standard (RFC 4648, avec bourrage).
const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Encode des octets en base64 standard.
fn base64(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        out.push(B64[(n >> 18) as usize & 63] as char);
        out.push(B64[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() >= 2 {
            B64[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() == 3 {
            B64[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// Blocs détenus d'après la bitmap persistée (bit i = bloc i, borné).
fn blocs_detenus(bitmap: &[u8], total: usize) -> usize {
    (0..total)
        .filter(|&i| bitmap.get(i / 8).is_some_and(|b| b & (1 << (i % 8)) != 0))
        .count()
}

/// Aiguille les méthodes `files.*` vers le nœud.
pub(super) fn dispatch(node: &Node, method: &str, params: &Value) -> Result<Value, NodeError> {
    match method {
        "files.share" => {
            let path = param_str(params, "path")?;
            let f = node.files_publish(Path::new(path))?;
            Ok(json!({ "file": {
                "merkle_root": hex::encode(&f.merkle_root),
                "name": f.name,
                "size": f.size,
                "mime": f.mime,
            }}))
        }
        "files.share_bytes" => {
            // Publication depuis l'UI web (pas de chemin disque côté
            // navigateur) : les octets transitent en base64, bornés comme la
            // lecture en ligne — au-delà, passer par `files.share` (chemin).
            let nom = param_str(params, "name")?;
            let mime = param_str(params, "mime")?;
            let b64 = param_str(params, "data_b64")?;
            if b64.len() as u64 > LECTURE_EN_LIGNE_MAX / 3 * 4 + 4 {
                return Err(NodeError::Invalid(
                    "fichier trop volumineux pour un envoi en ligne (8 Mio) : utiliser files.share",
                ));
            }
            let octets = b64_decode(b64).ok_or(NodeError::Invalid("base64 invalide"))?;
            let f = node.files_publish_bytes(nom, mime, octets)?;
            Ok(json!({ "file": {
                "merkle_root": hex::encode(&f.merkle_root),
                "name": f.name,
                "size": f.size,
                "mime": f.mime,
            }}))
        }
        "files.read" => {
            let racine = param_racine(params)?;
            let Some(chemin) = node.files_local_path(&racine)? else {
                // Pas (encore) complet en local : déclenche ou poursuit le
                // téléchargement ; l'UI suivra `event.file_progress` puis
                // rappellera `files.read`. Média de RENDU (icône/bannière de
                // serveur, avatar, émoji…) : l'UI passe `media:true` et le
                // téléchargement est PLAFONNÉ à MEDIA_AUTO_FETCH_MAX (8 Mio) —
                // un MANAGE_CHANNELS malveillant ne peut pas faire
                // auto-télécharger un blob de 2 Gio sous couvert d'icône de
                // serveur (audit 1.0). Une pièce jointe (potentiellement
                // volumineuse) est lue SANS ce drapeau → téléchargement non
                // plafonné (borne `MAX_FILE_SIZE`), puis copiée via `files.save`.
                let indice = param_indice(params)?;
                if params
                    .get("media")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    node.files_fetch_media(&racine, indice)?;
                } else {
                    node.files_fetch(&racine, indice)?;
                }
                return Ok(json!({ "pending": true }));
            };
            let entry = node
                .files_entry(&racine)?
                .ok_or(NodeError::NotFound("fichier"))?;
            if entry.size > LECTURE_EN_LIGNE_MAX {
                return Err(NodeError::Invalid(
                    "fichier trop volumineux pour une lecture en ligne (8 Mio) : utiliser files.save",
                ));
            }
            let data = std::fs::read(&chemin)?;
            Ok(json!({
                "data_b64": base64(&data),
                "name": entry.name,
                "mime": entry.mime,
                "size": entry.size,
            }))
        }
        "files.status" => {
            let racine = param_racine(params)?;
            match node.files_entry(&racine)? {
                Some(e) => {
                    let total = merkle::block_count(e.size);
                    let done = if e.complete {
                        total
                    } else {
                        blocs_detenus(&e.bitmap, total)
                    };
                    // `path` (additif, D-055) : chemin local du blob complet —
                    // l'UI de bureau le sert en streaming via le protocole
                    // asset (lecteur vidéo au-delà de la lecture en ligne).
                    let path = if e.complete {
                        node.files_local_path(&racine)?
                            .map(|p| p.to_string_lossy().into_owned())
                    } else {
                        None
                    };
                    Ok(json!({
                        "known": true,
                        "complete": e.complete,
                        "done": done,
                        "total": total,
                        "name": e.name,
                        "size": e.size,
                        "mime": e.mime,
                        "path": path,
                    }))
                }
                None => Ok(json!({
                    "known": false,
                    "complete": false,
                    "done": 0,
                    "total": 0,
                })),
            }
        }
        "files.save" => {
            let racine = param_racine(params)?;
            let dest = param_str(params, "path")?;
            let source = node
                .files_local_path(&racine)?
                .ok_or(NodeError::NotFound("fichier complet dans le magasin"))?;
            std::fs::copy(&source, Path::new(dest))?;
            Ok(json!({ "ok": true }))
        }
        _ => Err(NodeError::Invalid("méthode inconnue")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_standard_avec_bourrage() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64(&[0xFF, 0x00, 0xEE]), "/wDu");
    }

    #[test]
    fn compte_des_blocs_detenus() {
        assert_eq!(blocs_detenus(&[0b101], 3), 2);
        assert_eq!(blocs_detenus(&[0xFF, 0b111], 11), 11);
        assert_eq!(blocs_detenus(&[], 4), 0);
    }
}
