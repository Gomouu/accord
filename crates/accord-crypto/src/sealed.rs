//! Boîtes scellées : chiffrement asymétrique vers la clé statique d'un
//! destinataire hors session (clés de groupe, boîtes aux lettres — SPEC §6.4).
//!
//! Format : `eph_pub(32) ‖ AEAD(plaintext)` où la clé et le nonce sont dérivés
//! par HKDF du secret DH(éphémère, statique destinataire) lié aux deux clés
//! publiques. Le nonce est déterministe : la clé AEAD est unique par éphémère.

use crate::error::CryptoError;
use crate::identity::{x25519_public_of, Identity};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use hkdf::Hkdf;
use rand::rngs::OsRng;
use sha2::Sha256;
use zeroize::Zeroizing;

/// Surcoût d'une boîte scellée : clé éphémère (32) + tag Poly1305 (16).
pub const SEALED_OVERHEAD: usize = 48;

fn derive(shared: &[u8; 32], eph_pub: &[u8; 32], recipient_x: &[u8; 32]) -> ([u8; 32], XNonce) {
    let mut salt = [0u8; 64];
    salt[..32].copy_from_slice(eph_pub);
    salt[32..].copy_from_slice(recipient_x);
    let hk = Hkdf::<Sha256>::new(Some(&salt), shared);
    let mut key = [0u8; 32];
    crate::hkdf_expand_fixe(&hk, b"accord-seal", &mut key);
    let mut nonce = [0u8; 24];
    crate::hkdf_expand_fixe(&hk, b"accord-seal-nonce", &mut nonce);
    (key, *XNonce::from_slice(&nonce))
}

/// Scelle `plaintext` pour le détenteur de la clé Ed25519 `recipient_ed_pub`.
pub fn seal(recipient_ed_pub: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let recipient_x = x25519_public_of(recipient_ed_pub)?;
    let eph = x25519_dalek::StaticSecret::random_from_rng(OsRng);
    let eph_pub = x25519_dalek::PublicKey::from(&eph).to_bytes();
    let shared = eph.diffie_hellman(&x25519_dalek::PublicKey::from(recipient_x));
    if !shared.was_contributory() {
        return Err(CryptoError::InvalidPublicKey);
    }
    let (key, nonce) = derive(shared.as_bytes(), &eph_pub, &recipient_x);
    let key = Zeroizing::new(key);
    let cipher = XChaCha20Poly1305::new(key.as_ref().into());
    let boxed = cipher
        .encrypt(&nonce, plaintext)
        .map_err(|_| CryptoError::DecryptFailed)?;
    let mut out = Vec::with_capacity(32 + boxed.len());
    out.extend_from_slice(&eph_pub);
    out.extend_from_slice(&boxed);
    Ok(out)
}

/// Ouvre une boîte scellée avec l'identité du destinataire.
pub fn open(identity: &Identity, sealed: &[u8]) -> Result<Vec<u8>, CryptoError> {
    if sealed.len() < SEALED_OVERHEAD {
        return Err(CryptoError::BadSealedBox);
    }
    let mut eph_pub = [0u8; 32];
    eph_pub.copy_from_slice(&sealed[..32]);
    let recipient_x = identity.x25519_public();
    let shared = identity
        .x25519_secret()
        .diffie_hellman(&x25519_dalek::PublicKey::from(eph_pub));
    if !shared.was_contributory() {
        return Err(CryptoError::InvalidPublicKey);
    }
    let (key, nonce) = derive(shared.as_bytes(), &eph_pub, &recipient_x);
    let key = Zeroizing::new(key);
    let cipher = XChaCha20Poly1305::new(key.as_ref().into());
    cipher
        .decrypt(&nonce, &sealed[32..])
        .map_err(|_| CryptoError::DecryptFailed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_open_roundtrip() {
        let recipient = Identity::generate_with_pow_bits(1);
        let sealed = seal(&recipient.public_key(), b"cle de groupe 32 octets........").unwrap();
        assert_eq!(
            open(&recipient, &sealed).unwrap(),
            b"cle de groupe 32 octets........"
        );
    }

    #[test]
    fn group_key_sealed_size_is_80() {
        // SPEC §6.4 : sealed_key d'une clé de 32 octets = 80 octets.
        let recipient = Identity::generate_with_pow_bits(1);
        let sealed = seal(&recipient.public_key(), &[0u8; 32]).unwrap();
        assert_eq!(sealed.len(), 80);
    }

    #[test]
    fn wrong_recipient_fails() {
        let recipient = Identity::generate_with_pow_bits(1);
        let other = Identity::generate_with_pow_bits(1);
        let sealed = seal(&recipient.public_key(), b"secret").unwrap();
        assert!(open(&other, &sealed).is_err());
    }

    #[test]
    fn tampered_box_fails() {
        let recipient = Identity::generate_with_pow_bits(1);
        let mut sealed = seal(&recipient.public_key(), b"secret").unwrap();
        let last = sealed.len() - 1;
        sealed[last] ^= 1;
        assert!(open(&recipient, &sealed).is_err());
        assert_eq!(
            open(&recipient, &[0u8; 10]).unwrap_err(),
            CryptoError::BadSealedBox
        );
    }
}
