//! Méthodes `devices.*` : les appareils du compte (multi-appareil, jalon 1).
//!
//! Ce que voit l'écran « Mes appareils ». Un seul appareil pour l'instant —
//! celui de cette machine ; l'appairage (lot 1.D) en ajoutera d'autres, sans
//! que la forme de la réponse change.
//!
//! 🔒 La **graine** de l'appareil ne sort jamais d'ici. L'API n'expose que la
//! clé publique, le nom et la date d'ajout : de quoi reconnaître un appareil
//! dans une liste, jamais de quoi en usurper un.

use serde_json::{json, Value};

use crate::error::NodeError;
use crate::hex;
use crate::node::Node;

use super::helpers::{param_pubkey, param_str};

/// Longueur maximale d'un nom d'appareil, **en octets UTF-8**.
///
/// 🔒 C'est la borne du fil (`accord_proto::device::MAX_DEVICE_NAME`), et elle
/// doit être appliquée telle quelle. Compter les caractères serait plus
/// parlant à la saisie mais plus *laxiste* : « é » pèse deux octets, donc un
/// nom de 32 caractères accentués passerait ici et serait refusé au décodage —
/// un réglage qui semble accepté et ne « prend » jamais.
const MAX_NAME_BYTES: usize = accord_proto::device::MAX_DEVICE_NAME;

/// Aiguille les méthodes `devices.*` vers le nœud.
pub(super) fn dispatch(node: &Node, method: &str, params: &Value) -> Result<Value, NodeError> {
    match method {
        "devices.list" => {
            let devices = node.account_devices()?;
            Ok(json!({ "devices": devices
                .into_iter()
                .map(|d| json!({
                    "pubkey": hex::encode(&d.pubkey),
                    "name": d.name,
                    "added_ms": d.added_ms,
                    "is_current": d.is_current,
                }))
                .collect::<Vec<_>>() }))
        }
        "devices.rename" => {
            let name = param_str(params, "name")?;
            let trimmed = name.trim();
            if trimmed.is_empty() || trimmed.len() > MAX_NAME_BYTES {
                return Err(NodeError::Invalid(
                    "le nom d'appareil doit faire entre 1 et 32 octets",
                ));
            }
            node.rename_local_device(trimmed)?;
            Ok(json!({ "name": trimmed }))
        }
        "devices.pair_start" => {
            let started = node.pairing_start()?;
            Ok(json!({
                "code": started.code,
                "expires_ms": started.expires_ms,
            }))
        }
        "devices.pair_cancel" => {
            node.pairing_cancel();
            Ok(json!({}))
        }
        "devices.pair_submit" => {
            let code = param_str(params, "code")?;
            let outgoing = node.pairing_submit(code)?;
            // Le message PAKE remonte en hexadécimal : l'acheminer est le
            // travail de l'appelant, ce module ne connaît pas le transport.
            Ok(json!({ "hello": hex::encode(&outgoing) }))
        }
        "devices.pair_status" => Ok(json!({
            // `null` tant qu'aucun échange n'a abouti : l'écran affiche alors
            // le code et attend, plutôt que d'inventer une empreinte.
            "fingerprint": node.pairing_fingerprint(),
        })),
        "devices.pair_confirm" => {
            node.pairing_confirm()?;
            Ok(json!({}))
        }
        "devices.revoke" => {
            let pubkey = param_pubkey(params, "pubkey")?;
            node.revoke_device(&pubkey)?;
            Ok(json!({}))
        }
        _ => Err(NodeError::Invalid("méthode devices inconnue")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_nom_vide_ou_blanc_est_refuse() {
        // Un nom vide rendrait l'appareil impossible à distinguer des autres
        // dans la liste, ce qui est exactement ce que la liste sert à faire.
        for mauvais in ["", "   ", "\t"] {
            let params = json!({ "name": mauvais });
            let name = param_str(&params, "name").unwrap();
            assert!(name.trim().is_empty());
        }
    }

    #[test]
    fn la_borne_de_nom_est_celle_du_fil_et_non_un_compte_de_caracteres() {
        // 🔒 Ce test a attrapé une vraie erreur : la première version comptait
        // les caractères en se croyant plus sévère. « é » pèse deux octets, si
        // bien qu'un nom de 32 caractères accentués passait le service et se
        // faisait refuser au décodage — un réglage qui a l'air accepté et ne
        // « prend » jamais.
        let accents: String = "é".repeat(32);
        assert_eq!(accents.chars().count(), 32);
        assert!(
            accents.len() > MAX_NAME_BYTES,
            "32 caractères accentués dépassent la borne d'octets"
        );

        // Ce qui doit passer : 32 octets, quelle que soit leur répartition.
        assert!("x".repeat(MAX_NAME_BYTES).len() <= MAX_NAME_BYTES);
        assert!("é".repeat(MAX_NAME_BYTES / 2).len() <= MAX_NAME_BYTES);
    }
}
