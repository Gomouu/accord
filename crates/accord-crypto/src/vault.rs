//! Coffre d'identité chiffré au repos : Argon2id + XChaCha20-Poly1305
//! (SPEC §2.6). Contient la seed Ed25519 et le nonce de preuve de travail.

use crate::error::CryptoError;
use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use rand::rngs::OsRng;
use rand::RngCore;
use zeroize::Zeroizing;

const MAGIC: &[u8; 8] = b"ACCVLT01";
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 24;
const PLAINTEXT_LEN: usize = 32 + 8; // seed ‖ pow_nonce
const TAG_LEN: usize = 16;

/// Paramètres Argon2id du coffre.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VaultParams {
    /// Mémoire en KiB (défaut 64 MiB).
    pub m_kib: u32,
    /// Itérations (défaut 3).
    pub t_cost: u32,
    /// Parallélisme (défaut 4).
    pub p_cost: u32,
}

impl Default for VaultParams {
    fn default() -> Self {
        // SPEC §2.6 : Argon2id m=64 MiB, t=3, p=4.
        Self {
            m_kib: 64 * 1024,
            t_cost: 3,
            p_cost: 4,
        }
    }
}

impl VaultParams {
    /// Paramètres réduits pour les tests (rapides, jamais en production).
    pub fn insecure_for_tests() -> Self {
        Self {
            m_kib: 8,
            t_cost: 1,
            p_cost: 1,
        }
    }
}

fn derive_key(
    secret: &[u8],
    salt: &[u8; SALT_LEN],
    params: VaultParams,
) -> Result<Zeroizing<[u8; 32]>, CryptoError> {
    let a2 = Argon2::new(
        Algorithm::Argon2id,
        Version::V0x13,
        Params::new(params.m_kib, params.t_cost, params.p_cost, Some(32))
            .map_err(|_| CryptoError::BadKdfParams)?,
    );
    let mut key = Zeroizing::new([0u8; 32]);
    a2.hash_password_into(secret, salt, key.as_mut())
        .map_err(|_| CryptoError::BadKdfParams)?;
    Ok(key)
}

/// Scelle la seed et le nonce PoW sous un secret de déverrouillage.
///
/// Le secret est soit une clé aléatoire gardée par le trousseau OS
/// (déverrouillage transparent), soit un mot de passe utilisateur.
pub fn seal_vault(
    seed: &[u8; 32],
    pow_nonce: u64,
    secret: &[u8],
    params: VaultParams,
) -> Result<Vec<u8>, CryptoError> {
    let mut salt = [0u8; SALT_LEN];
    OsRng.fill_bytes(&mut salt);
    let mut nonce = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce);
    let key = derive_key(secret, &salt, params)?;
    let cipher = XChaCha20Poly1305::new(key.as_ref().into());

    let mut plaintext = Zeroizing::new([0u8; PLAINTEXT_LEN]);
    plaintext[..32].copy_from_slice(seed);
    plaintext[32..].copy_from_slice(&pow_nonce.to_be_bytes());

    let boxed = cipher
        .encrypt(XNonce::from_slice(&nonce), plaintext.as_ref())
        .map_err(|_| CryptoError::DecryptFailed)?;

    let mut out = Vec::with_capacity(MAGIC.len() + 12 + SALT_LEN + NONCE_LEN + boxed.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&params.m_kib.to_be_bytes());
    out.extend_from_slice(&params.t_cost.to_be_bytes());
    out.extend_from_slice(&params.p_cost.to_be_bytes());
    out.extend_from_slice(&salt);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&boxed);
    Ok(out)
}

/// Ouvre un coffre. Distingue corruption structurelle et mauvais secret.
pub fn open_vault(vault: &[u8], secret: &[u8]) -> Result<([u8; 32], u64), CryptoError> {
    let expected_len = MAGIC.len() + 12 + SALT_LEN + NONCE_LEN + PLAINTEXT_LEN + TAG_LEN;
    if vault.len() != expected_len || &vault[..8] != MAGIC {
        return Err(CryptoError::VaultCorrupt);
    }
    let m_kib = crate::be_u32(vault, 8).ok_or(CryptoError::VaultCorrupt)?;
    let t_cost = crate::be_u32(vault, 12).ok_or(CryptoError::VaultCorrupt)?;
    let p_cost = crate::be_u32(vault, 16).ok_or(CryptoError::VaultCorrupt)?;
    // Bornes anti-DoS : un coffre altéré ne doit pas demander 1 To de RAM.
    if !(8..=1024 * 1024).contains(&m_kib)
        || !(1..=16).contains(&t_cost)
        || !(1..=16).contains(&p_cost)
    {
        return Err(CryptoError::VaultCorrupt);
    }
    let mut salt = [0u8; SALT_LEN];
    salt.copy_from_slice(&vault[20..20 + SALT_LEN]);
    let nonce_start = 20 + SALT_LEN;
    let nonce = XNonce::from_slice(&vault[nonce_start..nonce_start + NONCE_LEN]);
    let boxed = &vault[nonce_start + NONCE_LEN..];

    let key = derive_key(
        secret,
        &salt,
        VaultParams {
            m_kib,
            t_cost,
            p_cost,
        },
    )?;
    let cipher = XChaCha20Poly1305::new(key.as_ref().into());
    let plaintext = Zeroizing::new(
        cipher
            .decrypt(nonce, boxed)
            .map_err(|_| CryptoError::VaultWrongSecret)?,
    );
    if plaintext.len() != PLAINTEXT_LEN {
        return Err(CryptoError::VaultCorrupt);
    }
    let mut seed = [0u8; 32];
    seed.copy_from_slice(&plaintext[..32]);
    let pow_nonce = crate::be_u64(&plaintext, 32).ok_or(CryptoError::VaultCorrupt)?;
    Ok((seed, pow_nonce))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vault_roundtrip() {
        let seed = [7u8; 32];
        let v = seal_vault(&seed, 42, b"secret", VaultParams::insecure_for_tests()).unwrap();
        let (seed2, pow) = open_vault(&v, b"secret").unwrap();
        assert_eq!(seed2, seed);
        assert_eq!(pow, 42);
    }

    #[test]
    fn wrong_secret_rejected() {
        let v = seal_vault(&[7u8; 32], 42, b"secret", VaultParams::insecure_for_tests()).unwrap();
        assert_eq!(
            open_vault(&v, b"mauvais").unwrap_err(),
            CryptoError::VaultWrongSecret
        );
    }

    #[test]
    fn corrupt_vault_rejected() {
        let mut v =
            seal_vault(&[7u8; 32], 42, b"secret", VaultParams::insecure_for_tests()).unwrap();
        v[0] = b'X';
        assert_eq!(
            open_vault(&v, b"secret").unwrap_err(),
            CryptoError::VaultCorrupt
        );
        assert_eq!(
            open_vault(&[0u8; 10], b"secret").unwrap_err(),
            CryptoError::VaultCorrupt
        );
        // Paramètres Argon2 hors bornes ⇒ corrompu, pas d'allocation géante.
        let mut v2 =
            seal_vault(&[7u8; 32], 42, b"secret", VaultParams::insecure_for_tests()).unwrap();
        v2[8..12].copy_from_slice(&u32::MAX.to_be_bytes());
        assert_eq!(
            open_vault(&v2, b"secret").unwrap_err(),
            CryptoError::VaultCorrupt
        );
    }
}
