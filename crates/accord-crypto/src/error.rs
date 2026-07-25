//! Erreurs typées du module cryptographique.

use thiserror::Error;

/// Erreur cryptographique. Aucune variante ne transporte de matière secrète.
#[derive(Debug, Error, PartialEq, Eq, Clone)]
pub enum CryptoError {
    /// Signature Ed25519 invalide.
    #[error("signature invalide")]
    InvalidSignature,
    /// Clé publique mal formée (point invalide).
    #[error("clé publique invalide")]
    InvalidPublicKey,
    /// Preuve de travail d'identité insuffisante.
    #[error("preuve de travail invalide")]
    InvalidPow,
    /// Horodatage hors de la fenêtre anti-rejeu (±90 s).
    #[error("horodatage hors fenêtre")]
    ClockSkew,
    /// Nonce de handshake déjà vu.
    #[error("rejeu de handshake détecté")]
    HandshakeReplay,
    /// L'identité statique authentifiée du pair ne correspond pas à la cible
    /// attendue (liaison d'identité du handshake : MITM on-path déjoué).
    #[error("identité du pair inattendue")]
    PeerIdentityMismatch,
    /// Échec de déchiffrement AEAD (clé, nonce ou AAD incorrects, ou altération).
    #[error("déchiffrement impossible")]
    DecryptFailed,
    /// Compteur de trame rejoué ou hors fenêtre.
    #[error("trame rejouée ou hors fenêtre")]
    FrameReplay,
    /// Epoch de session inconnu (re-keying désynchronisé).
    #[error("epoch de session inconnu: {0}")]
    UnknownEpoch(u8),
    /// Le compteur d'émission a atteint sa limite : re-keying requis.
    #[error("re-keying requis")]
    RekeyRequired,
    /// Appairage refusé : code mal formé, ou échange PAKE qui n'aboutit pas.
    #[error("appairage refusé : {0}")]
    Pairing(&'static str),
    /// Coffre d'identité corrompu ou format inconnu.
    #[error("coffre d'identité corrompu")]
    VaultCorrupt,
    /// Secret de déverrouillage incorrect.
    #[error("secret de déverrouillage incorrect")]
    VaultWrongSecret,
    /// Phrase de récupération invalide (mots ou checksum).
    #[error("phrase de récupération invalide")]
    BadMnemonic,
    /// Code ami mal formé ou somme de contrôle incorrecte.
    #[error("code ami invalide")]
    BadFriendCode,
    /// Paramètres Argon2 hors bornes.
    #[error("paramètres de dérivation invalides")]
    BadKdfParams,
    /// Boîte scellée mal formée.
    #[error("boîte scellée invalide")]
    BadSealedBox,
    /// Matériel post-quantique mal formé : clé d'encapsulation non canonique
    /// (FIPS 203 §7.2) ou chiffré de taille inattendue.
    #[error("matériel post-quantique invalide")]
    InvalidPqMaterial,
}
