//! Conteneur chiffré pour les sauvegardes : Argon2id + XChaCha20-Poly1305 en
//! tranches. Même famille de primitives que le coffre d'identité (`vault`),
//! mais pensé pour des archives de plusieurs Gio : le contenu est scellé par
//! tranches indépendantes, si bien que l'appelant peut travailler en flux
//! (jamais l'archive entière en mémoire) et qu'une corruption locale est
//! détectée à la tranche près.
//!
//! Format (tous les entiers en gros-boutien) :
//!
//! ```text
//! "ACCBKP01" ‖ m_kib(4) ‖ t_cost(4) ‖ p_cost(4) ‖ sel(16) ‖ prefixe_nonce(16)
//! ‖ longueur(4) ‖ tranche_de_verification
//! puis, par tranche : longueur(4) ‖ fin(1) ‖ tranche_chiffree
//! ```
//!
//! Le nonce d'une tranche est `prefixe_nonce ‖ index(8)` — unique par tranche
//! sous un même sel/préfixe tirés aléatoirement à chaque scellement. Les
//! données associées portent l'index et le drapeau « fin » (transmis en clair
//! pour le décodage en flux, mais authentifié) : réordonnance, suppression,
//! troncature de la fin ou drapeau falsifié sont détectés.
//!
//! La tranche de VÉRIFICATION (vide, nonce d'index `u64::MAX`, données
//! associées dédiées) suit immédiatement l'en-tête : son déchiffrement prouve
//! que le secret est le bon AVANT de toucher aux données. L'ouverture
//! distingue ainsi sans ambiguïté « mauvaise phrase de passe » (échec sur la
//! vérification) de « conteneur altéré/tronqué » (échec au-delà).
//!
//! Ce module reste PUR (aucune E/S) : il scelle/ouvre des tranches en mémoire,
//! l'appelant assemble le flux (voir `accord-node/src/backup.rs`).

use crate::error::CryptoError;
use crate::vault::VaultParams;
use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use rand::rngs::OsRng;
use rand::RngCore;
use zeroize::Zeroizing;

const MAGIC: &[u8; 8] = b"ACCBKP01";
const SALT_LEN: usize = 16;
const NONCE_PREFIX_LEN: usize = 16;
/// Longueur du tag Poly1305 (surcoût d'une tranche scellée).
pub const TAG_LEN: usize = 16;
/// Taille de tranche en clair (4 Mio) : borne la mémoire par tranche.
pub const CHUNK_LEN: usize = 4 * 1024 * 1024;
/// Longueur de l'en-tête du conteneur (avant la tranche de vérification).
pub const HEADER_LEN: usize = 8 + 12 + SALT_LEN + NONCE_PREFIX_LEN;

/// Index de nonce réservé à la tranche de vérification du secret (jamais
/// atteint par les tranches de données : elles seraient bornées bien avant).
const VERIF_INDEX: u64 = u64::MAX;
/// Données associées de la tranche de vérification.
const VERIF_AAD: &[u8] = b"ACCBKP01-verification";

/// Vrai si `bytes` commence par la signature d'un conteneur de sauvegarde
/// chiffré (permet de distinguer l'ancien format zip en clair).
pub fn is_sealed_backup(bytes: &[u8]) -> bool {
    bytes.len() >= MAGIC.len() && &bytes[..MAGIC.len()] == MAGIC
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

/// Nonce de la tranche `index` : préfixe aléatoire ‖ index gros-boutien.
fn chunk_nonce(prefix: &[u8; NONCE_PREFIX_LEN], index: u64) -> [u8; 24] {
    let mut nonce = [0u8; 24];
    nonce[..NONCE_PREFIX_LEN].copy_from_slice(prefix);
    nonce[NONCE_PREFIX_LEN..].copy_from_slice(&index.to_be_bytes());
    nonce
}

/// Données associées d'une tranche : index ‖ drapeau fin.
fn chunk_aad(index: u64, last: bool) -> [u8; 9] {
    let mut aad = [0u8; 9];
    aad[..8].copy_from_slice(&index.to_be_bytes());
    aad[8] = u8::from(last);
    aad
}

/// Contexte de scellement : en-tête émis à la création, puis une tranche à la
/// fois — l'appelant écrit les octets où il veut (fichier, tampon…).
pub struct BackupSealer {
    cipher: XChaCha20Poly1305,
    prefix: [u8; NONCE_PREFIX_LEN],
    /// En-tête + tranche de vérification, à écrire tels quels en tête de flux.
    header: Vec<u8>,
}

impl BackupSealer {
    /// Prépare un scellement sous `secret` : dérive la clé (coûteux, Argon2id)
    /// et construit l'en-tête complet (tranche de vérification incluse).
    pub fn new(secret: &[u8], params: VaultParams) -> Result<Self, CryptoError> {
        let mut salt = [0u8; SALT_LEN];
        OsRng.fill_bytes(&mut salt);
        let mut prefix = [0u8; NONCE_PREFIX_LEN];
        OsRng.fill_bytes(&mut prefix);
        let key = derive_key(secret, &salt, params)?;
        let cipher = XChaCha20Poly1305::new(key.as_ref().into());

        let mut header = Vec::with_capacity(HEADER_LEN + 4 + TAG_LEN);
        header.extend_from_slice(MAGIC);
        header.extend_from_slice(&params.m_kib.to_be_bytes());
        header.extend_from_slice(&params.t_cost.to_be_bytes());
        header.extend_from_slice(&params.p_cost.to_be_bytes());
        header.extend_from_slice(&salt);
        header.extend_from_slice(&prefix);
        let verif = cipher
            .encrypt(
                XNonce::from_slice(&chunk_nonce(&prefix, VERIF_INDEX)),
                Payload {
                    msg: &[],
                    aad: VERIF_AAD,
                },
            )
            .map_err(|_| CryptoError::DecryptFailed)?;
        header.extend_from_slice(&(verif.len() as u32).to_be_bytes());
        header.extend_from_slice(&verif);
        Ok(Self {
            cipher,
            prefix,
            header,
        })
    }

    /// En-tête complet du conteneur (à écrire avant la première tranche).
    pub fn header(&self) -> &[u8] {
        &self.header
    }

    /// Scelle la tranche `index` (au plus [`CHUNK_LEN`] octets, `last` pour la
    /// dernière) et rend la trame complète `longueur ‖ fin ‖ scellé`.
    pub fn seal_chunk(&self, index: u64, last: bool, plain: &[u8]) -> Result<Vec<u8>, CryptoError> {
        debug_assert!(plain.len() <= CHUNK_LEN, "tranche trop grande");
        let scelle = self
            .cipher
            .encrypt(
                XNonce::from_slice(&chunk_nonce(&self.prefix, index)),
                Payload {
                    msg: plain,
                    aad: &chunk_aad(index, last),
                },
            )
            .map_err(|_| CryptoError::DecryptFailed)?;
        let mut trame = Vec::with_capacity(5 + scelle.len());
        trame.extend_from_slice(&(scelle.len() as u32).to_be_bytes());
        trame.push(u8::from(last));
        trame.extend_from_slice(&scelle);
        Ok(trame)
    }
}

/// Contexte d'ouverture : construit depuis l'en-tête (vérification du secret
/// incluse), puis une tranche à la fois.
pub struct BackupOpener {
    cipher: XChaCha20Poly1305,
    prefix: [u8; NONCE_PREFIX_LEN],
}

impl BackupOpener {
    /// Longueur d'en-tête à lire avant [`BackupOpener::new`] : en-tête fixe +
    /// trame de vérification (longueur connue : tag seul).
    pub const INTRO_LEN: usize = HEADER_LEN + 4 + TAG_LEN;

    /// Valide `intro` (les [`Self::INTRO_LEN`] premiers octets du conteneur) et
    /// le secret. [`CryptoError::VaultCorrupt`] si la structure est invalide,
    /// [`CryptoError::VaultWrongSecret`] si la phrase ne correspond pas.
    pub fn new(intro: &[u8], secret: &[u8]) -> Result<Self, CryptoError> {
        if intro.len() < Self::INTRO_LEN || !is_sealed_backup(intro) {
            return Err(CryptoError::VaultCorrupt);
        }
        let m_kib = crate::be_u32(intro, 8).ok_or(CryptoError::VaultCorrupt)?;
        let t_cost = crate::be_u32(intro, 12).ok_or(CryptoError::VaultCorrupt)?;
        let p_cost = crate::be_u32(intro, 16).ok_or(CryptoError::VaultCorrupt)?;
        // Mêmes bornes anti-DoS que le coffre : un en-tête altéré ne doit pas
        // demander une dérivation démesurée.
        if !(8..=1024 * 1024).contains(&m_kib)
            || !(1..=16).contains(&t_cost)
            || !(1..=16).contains(&p_cost)
        {
            return Err(CryptoError::VaultCorrupt);
        }
        let mut salt = [0u8; SALT_LEN];
        salt.copy_from_slice(&intro[20..20 + SALT_LEN]);
        let mut prefix = [0u8; NONCE_PREFIX_LEN];
        let prefix_start = 20 + SALT_LEN;
        prefix.copy_from_slice(&intro[prefix_start..prefix_start + NONCE_PREFIX_LEN]);

        let verif_len = crate::be_u32(intro, HEADER_LEN).ok_or(CryptoError::VaultCorrupt)? as usize;
        if verif_len != TAG_LEN {
            return Err(CryptoError::VaultCorrupt);
        }
        let verif = &intro[HEADER_LEN + 4..HEADER_LEN + 4 + TAG_LEN];

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
        cipher
            .decrypt(
                XNonce::from_slice(&chunk_nonce(&prefix, VERIF_INDEX)),
                Payload {
                    msg: verif,
                    aad: VERIF_AAD,
                },
            )
            .map_err(|_| CryptoError::VaultWrongSecret)?;
        Ok(Self { cipher, prefix })
    }

    /// Ouvre la tranche `index` (corps `scelle` d'une trame dont l'appelant a
    /// lu `longueur` et `fin`). Le secret ayant été validé à la construction,
    /// tout échec est une altération du conteneur.
    pub fn open_chunk(
        &self,
        index: u64,
        last: bool,
        scelle: &[u8],
    ) -> Result<Vec<u8>, CryptoError> {
        if scelle.len() < TAG_LEN || scelle.len() > CHUNK_LEN + TAG_LEN {
            return Err(CryptoError::VaultCorrupt);
        }
        self.cipher
            .decrypt(
                XNonce::from_slice(&chunk_nonce(&self.prefix, index)),
                Payload {
                    msg: scelle,
                    aad: &chunk_aad(index, last),
                },
            )
            .map_err(|_| CryptoError::VaultCorrupt)
    }
}

/// Scelle `plaintext` entier en mémoire (pratique et tests ; pour les gros
/// volumes, préférer [`BackupSealer`] en flux).
pub fn seal_backup(
    plaintext: &[u8],
    secret: &[u8],
    params: VaultParams,
) -> Result<Vec<u8>, CryptoError> {
    let sealer = BackupSealer::new(secret, params)?;
    let mut out = sealer.header().to_vec();
    let tranches: Vec<&[u8]> = if plaintext.is_empty() {
        vec![&[]]
    } else {
        plaintext.chunks(CHUNK_LEN).collect()
    };
    let derniere = tranches.len() - 1;
    for (index, tranche) in tranches.into_iter().enumerate() {
        out.extend_from_slice(&sealer.seal_chunk(index as u64, index == derniere, tranche)?);
    }
    Ok(out)
}

/// Ouvre un conteneur entier en mémoire (pendant de [`seal_backup`]).
/// Distingue [`CryptoError::VaultWrongSecret`] (phrase) de
/// [`CryptoError::VaultCorrupt`] (structure, troncature, altération, données
/// résiduelles après la tranche finale).
pub fn open_backup(sealed: &[u8], secret: &[u8]) -> Result<Vec<u8>, CryptoError> {
    if sealed.len() < BackupOpener::INTRO_LEN {
        return Err(CryptoError::VaultCorrupt);
    }
    let opener = BackupOpener::new(&sealed[..BackupOpener::INTRO_LEN], secret)?;
    let mut reste = &sealed[BackupOpener::INTRO_LEN..];
    let mut out = Vec::new();
    let mut index: u64 = 0;
    loop {
        if reste.len() < 5 {
            // Fin d'entrée sans tranche « fin » vue : conteneur tronqué.
            return Err(CryptoError::VaultCorrupt);
        }
        let longueur = crate::be_u32(reste, 0).ok_or(CryptoError::VaultCorrupt)? as usize;
        let fin = match reste[4] {
            0 => false,
            1 => true,
            _ => return Err(CryptoError::VaultCorrupt),
        };
        reste = &reste[5..];
        if reste.len() < longueur {
            return Err(CryptoError::VaultCorrupt);
        }
        let (tranche, apres) = reste.split_at(longueur);
        reste = apres;
        out.extend_from_slice(&opener.open_chunk(index, fin, tranche)?);
        if fin {
            // Rien ne doit suivre la tranche finale.
            if reste.is_empty() {
                return Ok(out);
            }
            return Err(CryptoError::VaultCorrupt);
        }
        index += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PARAMS: VaultParams = VaultParams {
        m_kib: 8,
        t_cost: 1,
        p_cost: 1,
    };

    #[test]
    fn aller_retour_petit_et_vide() {
        for contenu in [&b""[..], b"bonjour", &[0xAB; 1000]] {
            let scelle = seal_backup(contenu, b"phrase", PARAMS).unwrap();
            assert!(is_sealed_backup(&scelle));
            assert_eq!(open_backup(&scelle, b"phrase").unwrap(), contenu);
        }
    }

    #[test]
    fn aller_retour_multi_tranches() {
        // Trois tranches : deux pleines + une partielle.
        let contenu: Vec<u8> = (0..CHUNK_LEN * 2 + 5).map(|i| (i % 251) as u8).collect();
        let scelle = seal_backup(&contenu, b"phrase", PARAMS).unwrap();
        assert_eq!(open_backup(&scelle, b"phrase").unwrap(), contenu);

        // Taille exactement multiple d'une tranche : la dernière est pleine.
        let rond: Vec<u8> = vec![0x5A; CHUNK_LEN];
        let scelle = seal_backup(&rond, b"phrase", PARAMS).unwrap();
        assert_eq!(open_backup(&scelle, b"phrase").unwrap(), rond);
    }

    #[test]
    fn mauvais_secret_rejete() {
        let scelle = seal_backup(b"contenu", b"phrase", PARAMS).unwrap();
        assert_eq!(
            open_backup(&scelle, b"mauvaise").unwrap_err(),
            CryptoError::VaultWrongSecret
        );
    }

    #[test]
    fn troncature_et_alteration_rejetees() {
        let contenu: Vec<u8> = vec![7u8; CHUNK_LEN + 10];
        let scelle = seal_backup(&contenu, b"phrase", PARAMS).unwrap();

        // Troncature : retirer la dernière tranche (drapeau fin jamais vu).
        // Intro (en-tête + vérification) puis première tranche pleine.
        let tronque = &scelle[..BackupOpener::INTRO_LEN + 5 + CHUNK_LEN + TAG_LEN];
        assert_eq!(
            open_backup(tronque, b"phrase").unwrap_err(),
            CryptoError::VaultCorrupt
        );

        // Altération d'un octet du corps.
        let mut altere = scelle.clone();
        let dernier = altere.len() - 1;
        altere[dernier] ^= 0x01;
        assert_eq!(
            open_backup(&altere, b"phrase").unwrap_err(),
            CryptoError::VaultCorrupt
        );

        // Drapeau « fin » falsifié sur la première tranche : la trame ne
        // s'authentifie plus (le drapeau est dans les données associées).
        let mut fin_falsifiee = scelle.clone();
        fin_falsifiee[BackupOpener::INTRO_LEN + 4] = 1;
        assert_eq!(
            open_backup(&fin_falsifiee, b"phrase").unwrap_err(),
            CryptoError::VaultCorrupt
        );

        // Données résiduelles après la tranche finale.
        let mut trainant = scelle.clone();
        trainant.extend_from_slice(b"reste");
        assert_eq!(
            open_backup(&trainant, b"phrase").unwrap_err(),
            CryptoError::VaultCorrupt
        );

        // Magie inconnue et paramètres Argon2 hors bornes.
        assert_eq!(
            open_backup(b"PAS-UNE-ARCHIVE", b"phrase").unwrap_err(),
            CryptoError::VaultCorrupt
        );
        let mut params_fous = scelle;
        params_fous[8..12].copy_from_slice(&u32::MAX.to_be_bytes());
        assert_eq!(
            open_backup(&params_fous, b"phrase").unwrap_err(),
            CryptoError::VaultCorrupt
        );
    }

    #[test]
    fn scellement_en_flux_equivalent_au_scellement_memoire() {
        // Le chemin « flux » (BackupSealer/BackupOpener tranche par tranche)
        // doit produire un conteneur que le chemin mémoire ouvre, et
        // réciproquement.
        let contenu: Vec<u8> = (0..CHUNK_LEN + 123).map(|i| (i % 199) as u8).collect();
        let sealer = BackupSealer::new(b"phrase", PARAMS).unwrap();
        let mut flux = sealer.header().to_vec();
        let tranches: Vec<&[u8]> = contenu.chunks(CHUNK_LEN).collect();
        for (i, t) in tranches.iter().enumerate() {
            flux.extend_from_slice(
                &sealer
                    .seal_chunk(i as u64, i == tranches.len() - 1, t)
                    .unwrap(),
            );
        }
        assert_eq!(open_backup(&flux, b"phrase").unwrap(), contenu);
    }

    #[test]
    fn zip_en_clair_non_reconnu_comme_scelle() {
        assert!(!is_sealed_backup(b"PK\x03\x04reste-de-zip"));
        assert!(!is_sealed_backup(b""));
    }
}
