//! Cible fuzz : ouverture d'un conteneur chiffré (coffre d'identité + archive
//! de sauvegarde) sur une entrée arbitraire.
//!
//! Invariant : `open_vault` et `BackupOpener::new` ne paniquent jamais sur des
//! octets arbitraires (en-tête tronqué, paramètres Argon2 démesurés, longueurs
//! forgées) — ils rendent `Err(VaultCorrupt)` sans allocation non bornée ni
//! index hors borne. Surface d'import de sauvegarde, exposée à un fichier
//! fourni par l'utilisateur (potentiellement malveillant ou corrompu).

#![no_main]

use accord_crypto::archive::{is_sealed_backup, BackupOpener};
use accord_crypto::open_vault;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Coffre d'identité : en-tête + paramètres KDF bornés, tag vérifié.
    let _ = open_vault(data, b"phrase-de-passe-fuzz");
    // Discrimination ancien/nouveau format (préfixe magique).
    let _ = is_sealed_backup(data);
    // Archive de sauvegarde : validation de l'en-tête + tranche de
    // vérification. Une phrase erronée ou un conteneur altéré rend Err.
    let _ = BackupOpener::new(data, b"phrase-de-passe-fuzz");
});
