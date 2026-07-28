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
//! # Bibliothèque : `ml-kem` (RustCrypto), choix réexaminé le 2026-07-26
//!
//! Le premier arbitrage (lot 2.A) départageait les candidats en comptant leurs
//! `unsafe`. Le critère est mauvais pour une primitive de ce rang : il note la
//! forme du code, pas ce qui a été ÉTABLI à son sujet. Les 256 `unsafe` de
//! libcrux sont des intrinsics SIMD, ce qui est normal dans une implémentation
//! optimisée ; les compter revenait à pénaliser la seule des trois qui porte
//! des preuves. Le critère refait est donc : qu'est-ce qui est prouvé, par qui,
//! sur quelle version.
//!
//! **Ce qui a été relevé.** `libcrux-ml-kem` 0.0.10 (Cryspen, 2026-07-15) porte
//! de vraies preuves machine — F* par la chaîne hax — sur l'arithmétique de
//! corps, la NTT, la sérialisation et le code générique de haut niveau, en
//! portable et AVX2. C'est davantage que ce que `ml-kem` offre, et ce n'est pas
//! contesté. Mais la frontière de vérification est étroite là où notre
//! déploiement est large : le backend NEON — celui de toute cible ARM64 — est
//! *admis* en totalité, et `libcrux-intrinsics`, la couche d'abstraction de
//! plateforme, est axiomatisée. Kobeissi (Symbolic Software, eprint 2026/192,
//! dernière révision 2026-06-25) mesure 58,4 % du code d'un déploiement ML-KEM
//! réellement vérifié par le solveur, et recense treize vulnérabilités passées
//! à travers : neuf hors code vérifié — dont un bug d'endianness inter-backends
//! qui a fait échouer de vrais déchiffrements dans le cliquet post-quantique de
//! Signal — et quatre dans la spécification et les preuves elles-mêmes, côté
//! ML-KEM une constante de décompression fausse dans le spec F*. Cryspen
//! conteste ce dernier point (« aucun bug n'a été trouvé dans le code
//! vérifié ») et concède par ailleurs ne prouver aucune garantie de canal
//! auxiliaire sur l'exécutable produit. `ml-kem` 0.3.2 (RustCrypto,
//! 2026-05-10), lui, n'a aucune preuve et son README dit n'avoir jamais été
//! audité indépendamment : sa base de confiance est la conformité aux vecteurs
//! FIPS 203 / ACVP et la lisibilité d'un code sans `unsafe`.
//!
//! **Ce qui tranche**, et ce n'est pas « lequel est le plus vérifié » :
//!
//! 1. La partie PROUVÉE de libcrux (arithmétique, NTT, sérialisation) est
//!    exactement celle qu'un vecteur de test attrape aussi. La partie NON
//!    prouvée (intrinsics, dispatch de plateforme, colle d'API) est celle où
//!    les bugs ont réellement été trouvés — et c'est celle qui VARIE d'une
//!    cible à l'autre, alors que nous publions trois cibles. Le bénéfice de la
//!    preuve porte donc sur un risque que nous couvrons déjà autrement, et la
//!    frontière tombe pile sur celui que nous ne couvrons pas.
//! 2. 🔒 L'hybride borne la casse. La clé de session dérive de X25519 ‖ ML-KEM :
//!    une défaillance TOTALE de la moitié post-quantique nous ramène à la
//!    sécurité de X25519 seul, celle d'aujourd'hui. Le choix de la crate ML-KEM
//!    ne peut pas nous faire reculer, ce qui déplace légitimement le poids vers
//!    la maintenabilité et la portabilité.
//! 3. Pour une dépendance, la gouvernance est un facteur de risque à part
//!    entière : aux divulgations de février 2026, le compte GitHub du
//!    rapporteur a été bloqué et ses quatre correctifs fermés sans revue
//!    technique.
//!
//! ⚠️ Le revers est assumé, et il est réel : « aucun bug connu dans `ml-kem` »
//! peut simplement signifier que personne n'a cherché, là où libcrux a subi
//! l'examen d'un tiers hostile — ce qui a de la valeur. La contrepartie, c'est
//! la charge de preuve que ce choix nous transfère : puisque la base de
//! confiance est la conformité aux vecteurs, elle est vérifiée ICI et non
//! déléguée à la CI d'autrui — voir `la_generation_suit_le_vecteur_du_nist` et
//! `l_encapsulation_suit_le_vecteur_du_nist`.
//!
//! Les deux autres candidats restent écartés pour les raisons du lot 2.A, que
//! le réexamen ne touche pas : `pqcrypto-mlkem` embarque 2667 fichiers C
//! (34 Mo) et exige une chaîne C à la compilation — il est de surcroît déclaré
//! non maintenu depuis l'archivage de PQClean (avis du 2026-06-04). Le CPU
//! n'étant la contrainte mordante d'aucun des trois, le choix se joue bien sur
//! la surface de confiance.

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
        let (dk, _) = MlKem512::generate_keypair_from_rng(&mut OsRngBridge);
        Self::from_decapsulation_key(dk)
    }

    /// Sérialise une fois pour toutes la clé d'encapsulation associée.
    ///
    /// Séparé de `generate` pour que les vecteurs du NIST empruntent ce même
    /// chemin : `generate` tire sa graine de l'`OsRng` et n'est donc épinglable
    /// par aucun vecteur, alors que cette sérialisation-ci l'est.
    fn from_decapsulation_key(dk: ml_kem::ml_kem_512::DecapsulationKey) -> Self {
        let mut ek = Box::new([0u8; MLKEM512_EK_BYTES]);
        // `to_bytes` rend exactement `MLKEM512_EK_BYTES` octets : la taille est
        // celle du jeu de paramètres, fixée par le type, pas par une entrée.
        ek.copy_from_slice(&dk.encapsulation_key().to_bytes());
        Self { dk, ek }
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

    // Vecteurs FIPS 203 du programme de validation ACVP du NIST
    // (`usnistgov/ACVP-Server`, `gen-val/json-files/ML-KEM-*-FIPS203`), jeu
    // ML-KEM-512, premier cas de chaque groupe. Les deux valeurs longues sont
    // en fichier ; les courtes restent lisibles ici.
    const KEYGEN_D: &str = "47b893474672ba92e4b12ee44fb32953af8e8503b5fb471d1614fb8a021a660a";
    const KEYGEN_Z: &str = "1f8cb39e9e30bc458a0dc5408884b1187fb217018df760fa57317703b844a0a9";
    const KEYGEN_EK: &str = include_str!("../tests/vectors/mlkem512-keygen-ek.hex");
    const ENCAP_EK: &str = include_str!("../tests/vectors/mlkem512-encap-ek.hex");
    const ENCAP_M: &str = "d21b9dc789adf74054f59f2041108a1c5cf0cee1c4e08384814cfc8fce5f3e14";
    const ENCAP_CT: &str = include_str!("../tests/vectors/mlkem512-encap-ct.hex");
    const ENCAP_K: &str = "a4c4efa0d001251991dee900abaf8364ec7e4de11881e280239ad2ed6dbc564f";

    fn octets(hex_str: &str) -> Vec<u8> {
        hex::decode(hex_str.trim()).expect("vecteur de test hexadécimal valide")
    }

    /// Reconstruit l'état initiateur à partir d'une graine FIPS 203 (`d ‖ z`).
    /// Réservé aux vecteurs : en production la graine vient de l'`OsRng`, et
    /// c'est bien pour cela que `generate` ne peut pas être épinglé.
    fn initiateur_depuis_graine(d: &str, z: &str) -> PqInitiator {
        let mut graine = [0u8; 64];
        graine[..32].copy_from_slice(&octets(d));
        graine[32..].copy_from_slice(&octets(z));
        let seed = ml_kem::array::Array::try_from(&graine[..]).expect("graine de 64 octets");
        PqInitiator::from_decapsulation_key(ml_kem::ml_kem_512::DecapsulationKey::from_seed(seed))
    }

    #[test]
    fn la_generation_suit_le_vecteur_du_nist() {
        // 🔒 La base de confiance de `ml-kem` est la conformité aux vecteurs, et
        // non une preuve (voir la tête de module). Elle est donc éprouvée ici :
        // déléguer ce contrôle à la CI d'autrui reviendrait à n'avoir ni preuve
        // ni vecteur. Les tests voisins sont auto-cohérents — ils passeraient
        // sur une implémentation qui calcule fidèlement la MAUVAISE fonction ;
        // celui-ci est le seul qui compare à une autorité extérieure.
        let init = initiateur_depuis_graine(KEYGEN_D, KEYGEN_Z);
        assert_eq!(
            init.encapsulation_key().as_slice(),
            octets(KEYGEN_EK).as_slice(),
            "la clé d'encapsulation dérivée de (d, z) doit être celle du NIST"
        );
    }

    #[test]
    fn l_encapsulation_suit_le_vecteur_du_nist() {
        // Encapsulation déterministe : à `m` fixé, FIPS 203 impose un chiffré et
        // un secret uniques. C'est le seul point où le secret partagé lui-même
        // est confronté à une valeur extérieure.
        let ek = ml_kem::ml_kem_512::EncapsulationKey::new_from_slice(&octets(ENCAP_EK))
            .expect("clé d'encapsulation du vecteur");
        let m = ml_kem::array::Array::try_from(&octets(ENCAP_M)[..]).expect("m de 32 octets");
        let (ct, ss) = ek.encapsulate_deterministic(&m);
        assert_eq!(ct.as_slice(), octets(ENCAP_CT).as_slice(), "chiffré");
        assert_eq!(ss.as_slice(), octets(ENCAP_K).as_slice(), "secret partagé");
    }

    #[test]
    fn la_decapsulation_rend_le_secret_d_une_encapsulation_epinglee() {
        // ⚠️ Portée limitée, et c'est assumé : l'ACVP ne publie la clé de
        // décapsulation que sous sa forme étendue de 1632 octets, que `ml-kem`
        // 0.3 ne décode plus qu'à travers une API dépréciée. La décapsulation
        // n'est donc pas confrontée DIRECTEMENT à un vecteur. Ce qu'on épingle
        // à la place : elle rend bien le secret d'une encapsulation dont le test
        // voisin a montré qu'elle suit le NIST octet pour octet, sur une paire
        // de clés dont la génération suit aussi le NIST. Les deux moitiés du
        // chemin sont ancrées ; c'est leur jonction qui est testée ici.
        let init = initiateur_depuis_graine(KEYGEN_D, KEYGEN_Z);
        let m = ml_kem::array::Array::try_from(&octets(ENCAP_M)[..]).expect("m de 32 octets");
        let (ct, attendu) = init.dk.encapsulation_key().encapsulate_deterministic(&m);
        let mut ct_bytes = [0u8; MLKEM512_CT_BYTES];
        ct_bytes.copy_from_slice(&ct);
        let obtenu = init.decapsulate(&ct_bytes).unwrap();
        assert_eq!(obtenu.as_slice(), attendu.as_slice());
    }

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
