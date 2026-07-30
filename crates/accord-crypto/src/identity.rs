//! Identité de nœud : paire Ed25519 immuable, NodeId, preuve de travail et
//! clé X25519 statique dérivée (SPEC §2.1).

use crate::error::CryptoError;
use accord_proto::limits::IDENTITY_POW_BITS;
use accord_proto::types::NodeId;
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use rand::rngs::OsRng;
use rand::RngCore;
use sha2::{Digest, Sha256, Sha512};
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Compte le nombre de bits de tête à zéro d'un hash.
fn leading_zero_bits(hash: &[u8; 32]) -> u32 {
    let mut bits = 0;
    for b in hash {
        if *b == 0 {
            bits += 8;
        } else {
            bits += b.leading_zeros();
            break;
        }
    }
    bits
}

/// Vérifie la preuve de travail d'une identité : `SHA-256(pubkey ‖ nonce_be)`
/// doit avoir au moins `bits` bits de tête à zéro.
pub fn verify_pow(pubkey: &[u8; 32], pow_nonce: u64, bits: u32) -> bool {
    let mut h = Sha256::new();
    h.update(pubkey);
    h.update(pow_nonce.to_be_bytes());
    let digest: [u8; 32] = h.finalize().into();
    leading_zero_bits(&digest) >= bits
}

/// Calcule un nonce de preuve de travail pour une clé publique.
pub fn compute_pow(pubkey: &[u8; 32], bits: u32) -> u64 {
    let mut nonce: u64 = OsRng.next_u64();
    loop {
        if verify_pow(pubkey, nonce, bits) {
            return nonce;
        }
        nonce = nonce.wrapping_add(1);
    }
}

/// Identité complète d'un nœud : seed Ed25519 + PoW.
///
/// La seed est la seule matière à sauvegarder ; tout le reste en dérive.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct Identity {
    seed: [u8; 32],
    #[zeroize(skip)]
    pow_nonce: u64,
}

impl Identity {
    /// Génère une identité neuve avec preuve de travail à la difficulté réseau.
    pub fn generate() -> Self {
        Self::generate_with_pow_bits(IDENTITY_POW_BITS)
    }

    /// Génère une identité avec une difficulté explicite (tests, réseaux privés).
    pub fn generate_with_pow_bits(bits: u32) -> Self {
        let mut seed = [0u8; 32];
        OsRng.fill_bytes(&mut seed);
        Self::from_seed_with_pow_bits(seed, bits)
    }

    /// Reconstruit l'identité depuis sa seed (restauration), en recalculant
    /// la preuve de travail à la difficulté réseau.
    pub fn from_seed(seed: [u8; 32]) -> Self {
        Self::from_seed_with_pow_bits(seed, IDENTITY_POW_BITS)
    }

    /// Reconstruit l'identité depuis sa seed avec difficulté explicite.
    pub fn from_seed_with_pow_bits(seed: [u8; 32], bits: u32) -> Self {
        let signing = SigningKey::from_bytes(&seed);
        let pubkey = signing.verifying_key().to_bytes();
        let pow_nonce = compute_pow(&pubkey, bits);
        Self { seed, pow_nonce }
    }

    /// Reconstruit l'identité avec un nonce de PoW déjà connu (chargement du
    /// coffre). Erreur si le nonce ne satisfait pas la difficulté demandée.
    pub fn from_seed_and_pow(
        seed: [u8; 32],
        pow_nonce: u64,
        bits: u32,
    ) -> Result<Self, CryptoError> {
        let signing = SigningKey::from_bytes(&seed);
        let pubkey = signing.verifying_key().to_bytes();
        if !verify_pow(&pubkey, pow_nonce, bits) {
            return Err(CryptoError::InvalidPow);
        }
        Ok(Self { seed, pow_nonce })
    }

    /// Seed brute (pour le coffre et la phrase de récupération).
    pub fn seed(&self) -> &[u8; 32] {
        &self.seed
    }

    /// Nonce de preuve de travail.
    pub fn pow_nonce(&self) -> u64 {
        self.pow_nonce
    }

    /// Clé de signature Ed25519 (recalculée à la demande, jamais stockée).
    fn signing_key(&self) -> SigningKey {
        SigningKey::from_bytes(&self.seed)
    }

    /// Clé publique Ed25519.
    pub fn public_key(&self) -> [u8; 32] {
        self.signing_key().verifying_key().to_bytes()
    }

    /// Identifiant de nœud : SHA-256 de la clé publique.
    pub fn node_id(&self) -> NodeId {
        node_id_of(&self.public_key())
    }

    /// Signe un message avec la clé d'identité.
    pub fn sign(&self, msg: &[u8]) -> [u8; 64] {
        self.signing_key().sign(msg).to_bytes()
    }

    /// Secret X25519 statique dérivé : `clamp(SHA-512(seed)[0..32])`
    /// (scalaire Ed25519 standard, propriété birationnelle — SPEC §2.1).
    pub fn x25519_secret(&self) -> x25519_dalek::StaticSecret {
        let mut digest = Sha512::digest(self.seed);
        let mut scalar = [0u8; 32];
        scalar.copy_from_slice(&digest[..32]);
        let secret = x25519_dalek::StaticSecret::from(scalar);
        scalar.zeroize();
        // 🔒 Le condensat entier est effacé, pas seulement la copie du scalaire
        // qui en sort : ses trente-deux premiers octets SONT la clé privée
        // X25519 de l'identité, et la méthode est appelée à chaque handshake —
        // autant d'exemplaires abandonnés dans la pile sinon.
        digest[..].zeroize();
        secret
    }

    /// Clé publique X25519 statique correspondante.
    pub fn x25519_public(&self) -> [u8; 32] {
        x25519_dalek::PublicKey::from(&self.x25519_secret()).to_bytes()
    }
}

/// NodeId d'une clé publique quelconque.
pub fn node_id_of(pubkey: &[u8; 32]) -> NodeId {
    NodeId(Sha256::digest(pubkey).into())
}

/// Clé publique X25519 d'un pair, dérivée de sa clé publique Ed25519
/// (conversion birationnelle Edwards → Montgomery).
pub fn x25519_public_of(ed_pubkey: &[u8; 32]) -> Result<[u8; 32], CryptoError> {
    let vk = VerifyingKey::from_bytes(ed_pubkey).map_err(|_| CryptoError::InvalidPublicKey)?;
    Ok(vk.to_montgomery().to_bytes())
}

/// Vérifie une signature Ed25519 (rejet strict des signatures malléables).
pub fn verify_signature(pubkey: &[u8; 32], msg: &[u8], sig: &[u8; 64]) -> Result<(), CryptoError> {
    let vk = VerifyingKey::from_bytes(pubkey).map_err(|_| CryptoError::InvalidPublicKey)?;
    let sig = Signature::from_bytes(sig);
    vk.verify_strict(msg, &sig)
        .map_err(|_| CryptoError::InvalidSignature)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pow_generate_and_verify() {
        let id = Identity::generate_with_pow_bits(8);
        assert!(verify_pow(&id.public_key(), id.pow_nonce(), 8));
        assert!(!verify_pow(
            &id.public_key(),
            id.pow_nonce().wrapping_add(1),
            32
        ));
    }

    #[test]
    fn identity_deterministic_from_seed() {
        let id = Identity::generate_with_pow_bits(4);
        let restored = Identity::from_seed_with_pow_bits(*id.seed(), 4);
        assert_eq!(id.public_key(), restored.public_key());
        assert_eq!(id.node_id(), restored.node_id());
        assert_eq!(id.x25519_public(), restored.x25519_public());
    }

    #[test]
    fn sign_and_verify() {
        let id = Identity::generate_with_pow_bits(1);
        let sig = id.sign(b"bonjour");
        verify_signature(&id.public_key(), b"bonjour", &sig).unwrap();
        assert_eq!(
            verify_signature(&id.public_key(), b"autre", &sig),
            Err(CryptoError::InvalidSignature)
        );
    }

    #[test]
    fn x25519_birational_conversion_matches() {
        // La clé X25519 dérivée de la seed correspond à la conversion de la
        // clé publique Ed25519 : les deux chemins donnent le même point.
        let id = Identity::generate_with_pow_bits(1);
        let from_pub = x25519_public_of(&id.public_key()).unwrap();
        assert_eq!(id.x25519_public(), from_pub);
        // Et le DH fonctionne entre les deux chemins.
        let other = Identity::generate_with_pow_bits(1);
        let shared1 = id
            .x25519_secret()
            .diffie_hellman(&x25519_dalek::PublicKey::from(
                x25519_public_of(&other.public_key()).unwrap(),
            ));
        let shared2 = other
            .x25519_secret()
            .diffie_hellman(&x25519_dalek::PublicKey::from(id.x25519_public()));
        assert_eq!(shared1.as_bytes(), shared2.as_bytes());
    }

    #[test]
    fn ed25519_rfc8032_test_vector_1() {
        // RFC 8032 §7.1, TEST 1 : seed nulle... (seed = 9d61b19d...)
        let seed: [u8; 32] =
            hex_literal("9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60");
        let sk = SigningKey::from_bytes(&seed);
        assert_eq!(
            sk.verifying_key().to_bytes(),
            hex_literal::<32>("d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a")
        );
        let sig = sk.sign(b"");
        let expected: [u8; 64] = hex_literal(
            "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e06522490155\
             5fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b",
        );
        assert_eq!(sig.to_bytes(), expected);
    }

    fn hex_literal<const N: usize>(s: &str) -> [u8; N] {
        let bytes = hex::decode(s).unwrap();
        let mut out = [0u8; N];
        out.copy_from_slice(&bytes);
        out
    }
}
