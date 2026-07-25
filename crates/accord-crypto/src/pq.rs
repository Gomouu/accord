//! Encapsulation de clé post-quantique ML-KEM-512 (FIPS 203) pour le
//! handshake hybride (ROADMAP §7, SPEC §2.2).
//!
//! # Pourquoi hybride, et pourquoi ces chiffres
//!
//! 🔒 X25519 **n'est pas retiré**. La clé de session dérive des deux secrets
//! concaténés — voir `handshake::derive_keys`. Si ML-KEM tombe, X25519 tient ;
//! si un ordinateur quantique casse X25519, ML-KEM tient. Un remplacement pur
//! serait un pari sur une primitive jeune ; l'hybride n'en est pas un.
//!
//! Le jeu de paramètres est **512**, décidé sur mesures et non par défaut
//! (lot 2.A, banc sur Apple M1 Pro, release, 3000 itérations) :
//!
//! - Taille. C'est le **seul** jeu qui tient sous `UDP_MTU` (1200 o) :
//!   HELLO 968 o et WELCOME 942 o (mesurés sur les octets encodés), contre
//!   1352 o / 1262 o en 768 et 1736 o / 1742 o en 1024. Fragmenter le handshake était l'option (a) de la
//!   feuille de route, écartée : le réassemblage précède l'établissement de
//!   session, il n'est donc pas authentifié.
//! - Coût. 29,6 µs côté initiateur (keygen + décapsulation) et 12,1 µs côté
//!   répondeur (encapsulation), à comparer aux 49,2 µs que le X25519 du même
//!   handshake coûte déjà. Le post-quantique est **moins cher** que ce qu'il
//!   double.
//! - Marge de sécurité. Catégorie NIST 1 (≈ recherche de clé AES-128), soit
//!   exactement le niveau de la moitié classique X25519 avec laquelle il est
//!   combiné. Monter à 768 relèverait une moitié sans relever l'autre, au prix
//!   d'un handshake fragmenté.
//!
//! Bibliothèque : `ml-kem` (RustCrypto). **Zéro `unsafe`**, licence
//! Apache-2.0/MIT, même famille que les cinq crates RustCrypto déjà en place.
//! `libcrux-ml-kem` est 2,5× plus rapide mais porte 63 + 193 occurrences
//! d'`unsafe` (SIMD) sous un numéro de version 0.0.10 ; `pqcrypto-mlkem` embarque
//! 2667 fichiers C (34 Mo) et exige une chaîne C à la compilation, ce qui pèse
//! sur les trois cibles de release et sur le mobile à venir. Le CPU n'étant pas
//! la contrainte mordante, le choix se fait sur la surface de confiance.

use crate::error::CryptoError;
use accord_proto::limits::{MLKEM512_CT_BYTES, MLKEM512_EK_BYTES};
use ml_kem::kem::{Decapsulate, Encapsulate};
use ml_kem::{Kem, KeyExport, MlKem512, TryKeyInit};
use zeroize::Zeroizing;

/// Secret partagé produit par l'encapsulation, effacé à la libération.
pub type PqSharedSecret = Zeroizing<[u8; 32]>;

/// Pont entre l'`OsRng` de `rand` 0.8 — la source d'aléa de tout le crate — et
/// le `CryptoRng` de `rand_core` 0.10 qu'attend `ml-kem`.
///
/// Il ne fait qu'acheminer des octets : aucune génération propre, aucun cache,
/// aucun état. `Infallible` parce que l'`OsRng` de rand 0.8 est infaillible par
/// signature — c'est déjà la source que `handshake::Initiator::start` utilise
/// pour ses nonces, la posture ne change pas.
struct OsRngBridge;

impl rand_core::TryRng for OsRngBridge {
    type Error = core::convert::Infallible;

    fn try_next_u32(&mut self) -> Result<u32, Self::Error> {
        Ok(rand::RngCore::next_u32(&mut rand::rngs::OsRng))
    }

    fn try_next_u64(&mut self) -> Result<u64, Self::Error> {
        Ok(rand::RngCore::next_u64(&mut rand::rngs::OsRng))
    }

    fn try_fill_bytes(&mut self, dst: &mut [u8]) -> Result<(), Self::Error> {
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, dst);
        Ok(())
    }
}

impl rand_core::TryCryptoRng for OsRngBridge {}

/// Matériel ML-KEM éphémère de l'initiateur, vivant le temps d'un handshake.
///
/// La clé de décapsulation ne quitte jamais ce type ; seule la clé
/// d'encapsulation part sur le fil, dans le HELLO.
pub struct PqInitiator {
    dk: ml_kem::ml_kem_512::DecapsulationKey,
    ek: Box<[u8; MLKEM512_EK_BYTES]>,
}

impl PqInitiator {
    /// Tire une paire ML-KEM-512 fraîche. Éphémère par handshake : aucune clé
    /// post-quantique n'est conservée au repos, donc rien à voler plus tard.
    pub fn generate() -> Self {
        let (dk, ek) = MlKem512::generate_keypair_from_rng(&mut OsRngBridge);
        let mut bytes = Box::new([0u8; MLKEM512_EK_BYTES]);
        // `to_bytes` rend exactement `MLKEM512_EK_BYTES` octets : la taille est
        // celle du jeu de paramètres, fixée par le type, pas par une entrée.
        bytes.copy_from_slice(&ek.to_bytes());
        Self { dk, ek: bytes }
    }

    /// Clé d'encapsulation à joindre au HELLO.
    pub fn encapsulation_key(&self) -> &[u8; MLKEM512_EK_BYTES] {
        &self.ek
    }

    /// Décapsule le chiffré reçu dans le WELCOME.
    ///
    /// Infaillible côté ML-KEM : FIPS 203 impose un **rejet implicite**, un
    /// chiffré invalide rend un secret pseudo-aléatoire au lieu d'une erreur.
    /// Ce n'est pas un trou : le chiffré est couvert par le transcript signé,
    /// donc une altération est déjà rejetée en amont par la signature. La seule
    /// erreur possible ici est une longueur incorrecte, que le type interdit.
    pub fn decapsulate(&self, ct: &[u8; MLKEM512_CT_BYTES]) -> Result<PqSharedSecret, CryptoError> {
        let ss = self
            .dk
            .decapsulate_slice(&ct[..])
            .map_err(|_| CryptoError::InvalidPqMaterial)?;
        let mut out = Zeroizing::new([0u8; 32]);
        out.copy_from_slice(&ss);
        Ok(out)
    }
}

impl std::fmt::Debug for PqInitiator {
    /// Debug sans matière secrète : ni la clé de décapsulation, ni sa taille.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PqInitiator").finish_non_exhaustive()
    }
}

/// Chiffré ML-KEM à joindre au WELCOME, avec le secret partagé correspondant.
pub type PqEncapsulation = (Box<[u8; MLKEM512_CT_BYTES]>, PqSharedSecret);

/// Encapsule un secret frais vers la clé annoncée par l'initiateur.
///
/// Erreur si la clé est mal formée : `ml-kem` applique la vérification de
/// cohérence modulaire de FIPS 203 §7.2, qui rejette un encodage non canonique.
/// C'est un contrôle **au décodage du matériel**, pas à l'usage.
pub fn encapsulate(ek: &[u8; MLKEM512_EK_BYTES]) -> Result<PqEncapsulation, CryptoError> {
    let key = ml_kem::ml_kem_512::EncapsulationKey::new_from_slice(&ek[..])
        .map_err(|_| CryptoError::InvalidPqMaterial)?;
    let (ct, ss) = key.encapsulate_with_rng(&mut OsRngBridge);
    let mut ct_bytes = Box::new([0u8; MLKEM512_CT_BYTES]);
    ct_bytes.copy_from_slice(&ct);
    let mut secret = Zeroizing::new([0u8; 32]);
    secret.copy_from_slice(&ss);
    Ok((ct_bytes, secret))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encapsulation_and_decapsulation_agree() {
        let init = PqInitiator::generate();
        let (ct, ss_responder) = encapsulate(init.encapsulation_key()).unwrap();
        let ss_initiator = init.decapsulate(&ct).unwrap();
        assert_eq!(*ss_initiator, *ss_responder);
    }

    #[test]
    fn two_handshakes_never_share_a_secret() {
        // Le matériel est éphémère : deux handshakes ne doivent avoir aucune
        // clé en commun, sans quoi la confidentialité persistante tombe.
        let a = PqInitiator::generate();
        let b = PqInitiator::generate();
        assert_ne!(a.encapsulation_key(), b.encapsulation_key());
        let (ct_a, ss_a) = encapsulate(a.encapsulation_key()).unwrap();
        let (ct_b, ss_b) = encapsulate(b.encapsulation_key()).unwrap();
        assert_ne!(ct_a, ct_b);
        assert_ne!(*ss_a, *ss_b);
    }

    #[test]
    fn encapsulating_twice_to_the_same_key_yields_distinct_secrets() {
        let init = PqInitiator::generate();
        let (ct1, ss1) = encapsulate(init.encapsulation_key()).unwrap();
        let (ct2, ss2) = encapsulate(init.encapsulation_key()).unwrap();
        assert_ne!(ct1, ct2);
        assert_ne!(*ss1, *ss2);
    }

    #[test]
    fn malformed_encapsulation_key_is_rejected() {
        // FIPS 203 §7.2 : un encodage non canonique (coefficient hors champ)
        // doit être refusé, pas encapsulé « au mieux ».
        let key = [0xFFu8; MLKEM512_EK_BYTES];
        assert_eq!(
            encapsulate(&key).unwrap_err(),
            CryptoError::InvalidPqMaterial
        );
    }

    #[test]
    fn tampered_ciphertext_yields_a_different_secret() {
        // Rejet implicite de FIPS 203 : pas d'erreur, mais un secret étranger.
        // C'est la signature du transcript qui rejette l'altération en amont ;
        // ce test documente que la décapsulation seule ne suffirait pas.
        let init = PqInitiator::generate();
        let (mut ct, ss) = encapsulate(init.encapsulation_key()).unwrap();
        ct[0] ^= 1;
        let autre = init.decapsulate(&ct).unwrap();
        assert_ne!(*autre, *ss);
    }

    #[test]
    fn parameter_sizes_match_the_protocol_bounds() {
        // Garde-fou : si `ml-kem` changeait de jeu de paramètres sous nos pieds,
        // les bornes de décodage d'`accord-proto` deviendraient fausses.
        let init = PqInitiator::generate();
        assert_eq!(init.encapsulation_key().len(), MLKEM512_EK_BYTES);
        let (ct, _) = encapsulate(init.encapsulation_key()).unwrap();
        assert_eq!(ct.len(), MLKEM512_CT_BYTES);
    }
}
