//! Migration d'un profil vers le modèle compte/appareil (jalon 1, lot 1.B).
//!
//! Au premier démarrage sur un profil existant, la graine actuelle devient la
//! **racine du compte** — le code ami, le profil et toutes les amitiés
//! continuent de pointer sur la même clé publique, donc rien ne change pour
//! les correspondants — et une **clé d'appareil neuve et distincte** est
//! générée pour cette machine.
//!
//! 🔒 La distinction est tout l'objet du jalon. Si la graine de compte servait
//! aussi d'appareil, deux machines restaurées depuis la même phrase de
//! récupération partageraient leur identité de transport et s'évinceraient
//! l'une l'autre à chaque ami (invariant « au plus une session directe par
//! identité »). Voir `docs/MULTI_DEVICE.md` §1.
//!
//! À ce stade la clé d'appareil est **créée et persistée, pas encore
//! utilisée** : le transport continue de passer par la clé de compte. Le
//! basculement est le lot 1.C, et le faire ici changerait le comportement
//! filaire sans que rien en face ne sache le lire.

use accord_core::db::LocalDevice;
use accord_core::Db;
use accord_crypto::DeviceIdentity;

use crate::error::NodeError;

/// Nom par défaut d'un appareil migré, avant que l'utilisateur ne le renomme.
const DEFAULT_DEVICE_NAME: &str = "Cet appareil";

/// Garantit qu'une identité d'appareil existe pour cette machine, et la rend.
///
/// Idempotent : au deuxième démarrage, l'appareil persisté est simplement
/// rechargé. Générer une nouvelle clé à chaque lancement ferait de chaque
/// redémarrage un appareil de plus aux yeux des amis.
pub fn ensure_local_device(db: &Db) -> Result<DeviceIdentity, NodeError> {
    if let Some(stored) = db.local_device()? {
        return Ok(DeviceIdentity::from_seed(stored.seed));
    }
    let device = DeviceIdentity::generate();
    db.set_local_device(&LocalDevice {
        seed: *device.seed(),
        pow_nonce: device.pow_nonce(),
        name: DEFAULT_DEVICE_NAME.to_string(),
    })?;
    tracing::info!("identité d'appareil créée pour cette machine");
    Ok(device)
}

#[cfg(test)]
mod tests {
    use super::*;
    use accord_crypto::Identity;

    fn db() -> Db {
        Db::open_in_memory(&[4u8; 32]).expect("base en mémoire")
    }

    #[test]
    fn a_device_is_created_on_first_start() {
        let db = db();
        assert!(db.local_device().unwrap().is_none());
        let device = ensure_local_device(&db).unwrap();
        let stored = db.local_device().unwrap().expect("appareil persisté");
        assert_eq!(stored.seed, *device.seed());
        assert_eq!(stored.pow_nonce, device.pow_nonce());
    }

    #[test]
    fn restarting_reuses_the_same_device() {
        // Une clé neuve à chaque lancement ferait de chaque redémarrage un
        // appareil de plus dans la liste, jusqu'à en dépasser la borne.
        let db = db();
        let first = ensure_local_device(&db).unwrap().public_key();
        for _ in 0..5 {
            assert_eq!(ensure_local_device(&db).unwrap().public_key(), first);
        }
    }

    #[test]
    fn the_device_key_differs_from_the_account_key() {
        // 🔒 Le cœur du jalon : confondre les deux ramènerait l'éviction
        // mutuelle que tout ce chantier existe pour supprimer.
        let db = db();
        let account = Identity::generate_with_pow_bits(4);
        let device = ensure_local_device(&db).unwrap();
        assert_ne!(device.public_key(), account.public_key());
        assert_ne!(device.seed(), account.seed());
    }

    #[test]
    fn two_machines_of_the_same_account_get_distinct_devices() {
        // Deux profils distincts (deux machines) migrés depuis la même phrase
        // de récupération : même compte, appareils différents.
        let a = ensure_local_device(&db()).unwrap();
        let b = ensure_local_device(&db()).unwrap();
        assert_ne!(a.public_key(), b.public_key());
    }

    #[test]
    fn the_generated_device_carries_a_valid_proof_of_work() {
        // Sans preuve de travail, une clé d'appareil se fabrique en masse : la
        // liste d'appareils d'un compte deviendrait un vecteur d'inondation.
        let device = ensure_local_device(&db()).unwrap();
        assert!(accord_crypto::verify_pow(
            &device.public_key(),
            device.pow_nonce(),
            accord_proto::limits::IDENTITY_POW_BITS,
        ));
    }
}
