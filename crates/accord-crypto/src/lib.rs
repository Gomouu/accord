//! # accord-crypto
//!
//! Couche cryptographique d'Accord (SPEC §2, §5) :
//!
//! - [`identity`] : paire Ed25519 immuable, NodeId, preuve de travail,
//!   clé X25519 statique dérivée ;
//! - [`handshake`] : établissement 1-RTT mutuellement authentifié avec
//!   transcript hash, anti-rejeu et cookies anti-DoS ;
//! - [`session`] : AEAD XChaCha20-Poly1305, nonces directionnels stricts,
//!   fenêtre anti-rejeu, re-keying par epochs (forward secrecy périodique) ;
//! - [`sealed`] : boîtes scellées vers une clé statique (clés de groupe,
//!   boîtes aux lettres) ;
//! - [`vault`] : stockage chiffré de l'identité au repos (Argon2id) ;
//! - [`mnemonic`] : phrase de récupération BIP39 de 12 mots ;
//! - [`friendcode`] : codes amis `MOT-MOT-MOT-1234` avec somme de contrôle.
//!
//! Primitives : crates RustCrypto auditées (décision D-001). Aucun `unsafe`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod archive;
pub mod error;
pub mod friendcode;
pub mod handshake;
pub mod identity;
pub mod mnemonic;
pub mod sealed;
pub mod session;
pub mod vault;

pub use error::CryptoError;
pub use friendcode::{FriendCode, FRIENDCODE_PAYLOAD_LEN};
pub use handshake::{respond, CookieJar, Established, Initiator, NonceCache};
pub use identity::{node_id_of, verify_pow, verify_signature, Identity};
pub use session::{SessionCrypto, SessionKeys};
pub use vault::{open_vault, seal_vault, VaultParams};

/// Lit un `u32` big-endian à `offset` ; `None` hors borne. Lecture PURE et
/// sans voie de panique (D23) : les décodeurs de conteneurs (coffre, archive)
/// propagent `VaultCorrupt` au lieu de faire confiance à une borne déjà
/// vérifiée en amont.
pub(crate) fn be_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let arr: [u8; 4] = bytes.get(offset..offset + 4)?.try_into().ok()?;
    Some(u32::from_be_bytes(arr))
}

/// Lit un `u64` big-endian à `offset` ; `None` hors borne (voir [`be_u32`]).
pub(crate) fn be_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    let arr: [u8; 8] = bytes.get(offset..offset + 8)?.try_into().ok()?;
    Some(u64::from_be_bytes(arr))
}

/// Expansion HKDF-SHA256 vers un tampon de taille FIXE : infaillible par
/// construction (la seule erreur possible d'`expand` est une sortie
/// dépassant 255 × 32 octets, jamais atteinte par nos dérivations de
/// 24-32 octets). Centralise l'unique `expect` justifié du crate (D23).
#[allow(clippy::expect_used)]
pub(crate) fn hkdf_expand_fixe(hk: &hkdf::Hkdf<sha2::Sha256>, info: &[u8], sortie: &mut [u8]) {
    hk.expand(info, sortie)
        .expect("sortie HKDF de taille fixe ≤ 8160 octets");
}

/// Dérive la clé de chiffrement de la base locale depuis la seed d'identité :
/// `HKDF-Extract(salt="accord-db", ikm=seed)` puis `Expand("sqlite", 32)`
/// (SPEC §2.6).
pub fn derive_db_key(seed: &[u8; 32]) -> [u8; 32] {
    use hkdf::Hkdf;
    use sha2::Sha256;
    let hk = Hkdf::<Sha256>::new(Some(b"accord-db"), seed);
    let mut key = [0u8; 32];
    crate::hkdf_expand_fixe(&hk, b"sqlite", &mut key);
    key
}

/// Dérive la clé HMAC de l'index de recherche (décision D-011).
pub fn derive_search_key(seed: &[u8; 32]) -> [u8; 32] {
    use hkdf::Hkdf;
    use sha2::Sha256;
    let hk = Hkdf::<Sha256>::new(Some(b"accord-db"), seed);
    let mut key = [0u8; 32];
    crate::hkdf_expand_fixe(&hk, b"search", &mut key);
    key
}

#[cfg(test)]
mod tests {
    #[test]
    fn derived_keys_distinct() {
        let seed = [1u8; 32];
        assert_ne!(super::derive_db_key(&seed), super::derive_search_key(&seed));
        assert_ne!(
            super::derive_db_key(&seed),
            super::derive_db_key(&[2u8; 32])
        );
    }
}
