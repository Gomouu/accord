//! Phrase de récupération : 12 mots BIP39 (128 bits d'entropie + checksum)
//! dont dérive la seed d'identité (SPEC §2.6, décision D-005).

use crate::error::CryptoError;
use bip39::{Language, Mnemonic};
use hkdf::Hkdf;
use rand::rngs::OsRng;
use rand::RngCore;
use sha2::Sha256;
use zeroize::Zeroizing;

/// Dérive la seed Ed25519 depuis l'entropie de la phrase :
/// `HKDF-Extract(salt="accord-recovery", ikm=entropy)` puis
/// `HKDF-Expand(prk, "identity", 32)`.
pub fn seed_from_entropy(entropy: &[u8; 16]) -> [u8; 32] {
    let hk = Hkdf::<Sha256>::new(Some(b"accord-recovery"), entropy);
    let mut seed = [0u8; 32];
    crate::hkdf_expand_fixe(&hk, b"identity", &mut seed);
    seed
}

/// Génère une phrase de 12 mots et la seed d'identité correspondante.
// SÛRETÉ (D23) : 16 octets = 128 bits d'entropie, taille BIP-39 valide par
// construction — `from_entropy_in` est infaillible sur cette entrée. Allow
// ciblé plutôt qu'une signature `Result` que rien ne peut produire.
#[allow(clippy::expect_used)]
pub fn generate() -> (String, [u8; 32]) {
    let mut entropy = Zeroizing::new([0u8; 16]);
    OsRng.fill_bytes(entropy.as_mut());
    let mnemonic =
        Mnemonic::from_entropy_in(Language::English, entropy.as_ref()).expect("entropie 128 bits");
    let seed = seed_from_entropy(&entropy);
    (mnemonic.to_string(), seed)
}

/// Restaure la seed d'identité depuis une phrase de 12 mots.
///
/// Tolère la casse et les espaces multiples ; le checksum BIP39 détecte les
/// fautes de frappe.
pub fn restore(phrase: &str) -> Result<[u8; 32], CryptoError> {
    let normalized = phrase
        .split_whitespace()
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join(" ");
    let mnemonic = Mnemonic::parse_in_normalized(Language::English, &normalized)
        .map_err(|_| CryptoError::BadMnemonic)?;
    if mnemonic.word_count() != 12 {
        return Err(CryptoError::BadMnemonic);
    }
    let entropy = mnemonic.to_entropy();
    let mut e = Zeroizing::new([0u8; 16]);
    if entropy.len() != 16 {
        return Err(CryptoError::BadMnemonic);
    }
    e.copy_from_slice(&entropy);
    Ok(seed_from_entropy(&e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_and_restore_same_seed() {
        let (phrase, seed) = generate();
        assert_eq!(phrase.split_whitespace().count(), 12);
        assert_eq!(restore(&phrase).unwrap(), seed);
    }

    #[test]
    fn restore_tolerates_case_and_spacing() {
        let (phrase, seed) = generate();
        let messy = format!("  {}  ", phrase.to_uppercase().replace(' ', "   "));
        assert_eq!(restore(&messy).unwrap(), seed);
    }

    #[test]
    fn bad_checksum_rejected() {
        // Vecteur BIP39 valide, puis premier mot altéré.
        let valid = "legal winner thank year wave sausage worth useful legal winner thank yellow";
        assert!(restore(valid).is_ok());
        let invalid = "legal winner thank year wave sausage worth useful legal winner thank thank";
        assert_eq!(restore(invalid).unwrap_err(), CryptoError::BadMnemonic);
        assert_eq!(
            restore("pas des mots bip39").unwrap_err(),
            CryptoError::BadMnemonic
        );
    }

    #[test]
    fn bip39_trezor_vector() {
        // Vecteur officiel Trezor : entropie 0x80808080... → phrase connue.
        let entropy = [0x80u8; 16];
        let m = Mnemonic::from_entropy_in(Language::English, &entropy).unwrap();
        assert_eq!(
            m.to_string(),
            "letter advice cage absurd amount doctor acoustic avoid letter advice cage above"
        );
        // Et la dérivation de seed est déterministe.
        assert_eq!(seed_from_entropy(&entropy), seed_from_entropy(&entropy));
    }

    #[test]
    fn wrong_word_count_rejected() {
        let (phrase, _) = generate();
        let truncated: Vec<&str> = phrase.split_whitespace().take(9).collect();
        assert_eq!(
            restore(&truncated.join(" ")).unwrap_err(),
            CryptoError::BadMnemonic
        );
    }
}
