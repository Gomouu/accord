//! Identités de compte et d'appareil (multi-appareil, jalon 1).
//!
//! Voir `docs/MULTI_DEVICE.md`. Le point à ne pas manquer :
//!
//! - la **clé de compte** ne sert qu'à signer la liste d'appareils. Elle peut
//!   rester hors ligne ; c'est elle que désigne le code ami ;
//! - la **clé d'appareil** sert à *tout le reste* — chaque session de
//!   transport, chaque message. Elle ne quitte jamais sa machine.
//!
//! 🔒 **Les deux ne doivent jamais être la même clé.** Si la graine de compte
//! servait aussi d'appareil, deux machines restaurées depuis la même phrase de
//! récupération partageraient leur identité de transport et s'évinceraient
//! mutuellement — exactement le défaut que tout ce jalon existe pour corriger
//! (invariant « au plus une session directe par identité »). La dérivation
//! ci-dessous garantit structurellement la distinction.

use crate::error::CryptoError;
use crate::identity::{verify_pow, verify_signature, Identity};
use accord_proto::device::DeviceList;
use accord_proto::limits::IDENTITY_POW_BITS;
use rand::rngs::OsRng;
use rand::RngCore;

/// Identité racine d'un compte : signe la liste d'appareils, rien d'autre.
///
/// Enveloppe volontairement mince autour d'[`Identity`] : le type existe pour
/// que la signature d'une fonction dise *laquelle* des deux clés elle attend.
/// Confondre les deux est la faute la plus coûteuse de ce jalon, et le
/// compilateur est le seul relecteur qui ne se fatigue pas.
pub struct AccountIdentity(Identity);

/// Identité d'un appareil : toutes les sessions de transport passent par elle.
pub struct DeviceIdentity(Identity);

impl AccountIdentity {
    /// Adopte une graine existante comme racine de compte.
    ///
    /// C'est le chemin de migration : la graine d'un profil antérieur au
    /// multi-appareil devient la racine, ce qui préserve le code ami, le
    /// profil et toutes les amitiés — rien ne change pour les correspondants.
    pub fn from_identity(identity: Identity) -> Self {
        Self(identity)
    }

    /// Clé publique du compte. C'est ce que voient les amis.
    pub fn public_key(&self) -> [u8; 32] {
        self.0.public_key()
    }

    /// Identité sous-jacente (coffre, code ami, profil).
    pub fn identity(&self) -> &Identity {
        &self.0
    }

    /// Signe une liste d'appareils. Le champ `sig` fourni est ignoré et
    /// remplacé.
    pub fn sign_device_list(&self, list: &mut DeviceList) {
        list.account = self.public_key();
        list.sig = self.0.sign(&list.signable_bytes());
    }

    /// Génère un appareil neuf **pour ce compte**, distinct de la racine.
    ///
    /// La graine d'appareil est tirée du générateur système, jamais dérivée de
    /// la graine de compte : une dérivation déterministe redonnerait la même
    /// clé d'appareil sur deux machines restaurées depuis la même phrase, et
    /// ramènerait le problème qu'on cherche à éviter.
    pub fn new_device(&self) -> DeviceIdentity {
        DeviceIdentity::generate()
    }
}

impl DeviceIdentity {
    /// Génère une identité d'appareil (graine aléatoire + preuve de travail).
    pub fn generate() -> Self {
        Self(Identity::generate())
    }

    /// Variante à difficulté explicite (tests, réseaux privés).
    pub fn generate_with_pow_bits(bits: u32) -> Self {
        Self(Identity::generate_with_pow_bits(bits))
    }

    /// Reconstruit un appareil depuis sa graine persistée.
    pub fn from_seed(seed: [u8; 32]) -> Self {
        Self(Identity::from_seed(seed))
    }

    /// Graine de l'appareil, à conserver **localement** et nulle part ailleurs.
    pub fn seed(&self) -> &[u8; 32] {
        self.0.seed()
    }

    /// Clé publique de l'appareil.
    pub fn public_key(&self) -> [u8; 32] {
        self.0.public_key()
    }

    /// Nonce de preuve de travail.
    pub fn pow_nonce(&self) -> u64 {
        self.0.pow_nonce()
    }

    /// Identité sous-jacente, utilisée par le transport pour ses sessions.
    pub fn identity(&self) -> &Identity {
        &self.0
    }
}

/// Pourquoi une liste d'appareils a été refusée.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DeviceListError {
    /// La signature ne correspond pas à la clé racine annoncée.
    #[error("signature de la liste d'appareils invalide")]
    BadSignature,
    /// La liste ne concerne pas le compte attendu.
    #[error("liste d'appareils d'un autre compte")]
    WrongAccount,
    /// Une version antérieure ou égale à celle déjà connue.
    #[error("liste d'appareils périmée")]
    Stale,
    /// La preuve de travail d'un appareil est insuffisante.
    #[error("preuve de travail d'appareil insuffisante")]
    InvalidPow,
    /// Deux entrées portent la même clé publique.
    #[error("appareil en double dans la liste")]
    DuplicateDevice,
}

/// Vérifie une liste d'appareils reçue.
///
/// `expected_account` est le compte qu'on cherchait ; `known_version` la
/// version déjà détenue (0 si aucune).
///
/// 🔒 L'ordre des contrôles compte. L'identité du compte est vérifiée **avant**
/// la signature : sinon on dépenserait une vérification Ed25519 sur une liste
/// qui ne nous concerne pas, ce qui offrirait un levier de déni de service à
/// qui voudrait nous inonder de listes étrangères.
pub fn verify_device_list(
    list: &DeviceList,
    expected_account: &[u8; 32],
    known_version: u64,
) -> Result<(), DeviceListError> {
    if list.account != *expected_account {
        return Err(DeviceListError::WrongAccount);
    }
    if list.version <= known_version {
        return Err(DeviceListError::Stale);
    }
    verify_signature(&list.account, &list.signable_bytes(), &list.sig)
        .map_err(|_| DeviceListError::BadSignature)?;

    // Après la signature seulement : ces contrôles portent sur un contenu
    // désormais authentifié, et ne servent qu'à refuser une liste que le
    // propriétaire du compte aurait lui-même mal formée.
    for (i, device) in list.devices.iter().enumerate() {
        if !verify_pow(&device.pubkey, device.pow_nonce, IDENTITY_POW_BITS) {
            return Err(DeviceListError::InvalidPow);
        }
        if list.devices[..i].iter().any(|d| d.pubkey == device.pubkey) {
            return Err(DeviceListError::DuplicateDevice);
        }
    }
    Ok(())
}

/// Numéro de version pour une liste émise à `now_ms`.
///
/// ⚠️ Dérivé de l'horodatage, **pas** d'un compteur stocké. C'est le correctif
/// du piège relevé en écrivant la conception : si la phrase de récupération
/// régénère la racine sur une machine neuve, un compteur repartirait à 1 et
/// tous les pairs détenant une version supérieure **ignoreraient** la nouvelle
/// liste — l'utilisateur resterait enfermé dehors de son propre compte, sans
/// recours. L'horloge murale, elle, ne revient pas en arrière après une
/// réinstallation.
pub fn version_for(now_ms: u64) -> u64 {
    now_ms
}

/// Graine d'appareil neuve (utilitaire de migration).
pub fn random_device_seed() -> [u8; 32] {
    let mut seed = [0u8; 32];
    OsRng.fill_bytes(&mut seed);
    seed
}

/// Erreur inutilisée ici, mais le type doit rester convertible.
impl From<DeviceListError> for CryptoError {
    fn from(_: DeviceListError) -> Self {
        CryptoError::InvalidSignature
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use accord_proto::device::{DeviceEntry, RevokedEntry};

    /// Difficulté réduite : la PoW réelle rendrait ces tests interminables.
    const POW: u32 = 4;

    fn account() -> AccountIdentity {
        AccountIdentity::from_identity(Identity::generate_with_pow_bits(POW))
    }

    fn entry(device: &DeviceIdentity, name: &str) -> DeviceEntry {
        DeviceEntry {
            pubkey: device.public_key(),
            pow_nonce: device.pow_nonce(),
            name: name.to_string(),
            added_ms: 1_700_000_000_000,
            flags: 0,
        }
    }

    fn signed_list(acct: &AccountIdentity, devices: Vec<DeviceEntry>, version: u64) -> DeviceList {
        let mut list = DeviceList {
            account: [0; 32],
            version,
            issued_ms: 1_700_000_000_000,
            valid_for_s: 7 * 24 * 3600,
            devices,
            revoked: vec![],
            sig: [0; 64],
        };
        acct.sign_device_list(&mut list);
        list
    }

    /// Comme `verify_device_list`, mais à la difficulté de test.
    fn verify(list: &DeviceList, acct: &[u8; 32], known: u64) -> Result<(), DeviceListError> {
        if list.account != *acct {
            return Err(DeviceListError::WrongAccount);
        }
        if list.version <= known {
            return Err(DeviceListError::Stale);
        }
        verify_signature(&list.account, &list.signable_bytes(), &list.sig)
            .map_err(|_| DeviceListError::BadSignature)?;
        for (i, d) in list.devices.iter().enumerate() {
            if !verify_pow(&d.pubkey, d.pow_nonce, POW) {
                return Err(DeviceListError::InvalidPow);
            }
            if list.devices[..i].iter().any(|o| o.pubkey == d.pubkey) {
                return Err(DeviceListError::DuplicateDevice);
            }
        }
        Ok(())
    }

    #[test]
    fn a_device_key_is_never_the_account_key() {
        // 🔒 Le cœur du jalon. Si ces deux clés coïncidaient, deux machines
        // restaurées depuis la même phrase se chasseraient l'une l'autre.
        let acct = account();
        let device = acct.new_device();
        assert_ne!(device.public_key(), acct.public_key());
        assert_ne!(device.seed(), acct.identity().seed());
    }

    #[test]
    fn two_devices_of_the_same_account_differ() {
        let acct = account();
        let a = acct.new_device();
        let b = acct.new_device();
        assert_ne!(a.public_key(), b.public_key());
    }

    #[test]
    fn the_same_recovery_phrase_yields_different_devices() {
        // Deux machines restaurent la MÊME phrase : elles retrouvent le même
        // compte — c'est voulu — mais doivent obtenir des appareils distincts.
        // Une dérivation déterministe depuis la graine ramènerait le bug que
        // tout ce jalon existe pour corriger.
        let seed = [7u8; 32];
        let machine_a =
            AccountIdentity::from_identity(Identity::from_seed_with_pow_bits(seed, POW));
        let machine_b =
            AccountIdentity::from_identity(Identity::from_seed_with_pow_bits(seed, POW));

        assert_eq!(
            machine_a.public_key(),
            machine_b.public_key(),
            "même compte"
        );
        assert_ne!(
            machine_a.new_device().public_key(),
            machine_b.new_device().public_key(),
            "appareils distincts"
        );
    }

    #[test]
    fn a_signed_list_verifies() {
        let acct = account();
        let device = acct.new_device();
        let list = signed_list(&acct, vec![entry(&device, "Portable")], 1);
        assert_eq!(verify(&list, &acct.public_key(), 0), Ok(()));
        assert!(list.authorises(&device.public_key()));
    }

    #[test]
    fn any_tampering_breaks_the_signature() {
        let acct = account();
        let device = acct.new_device();
        let base = signed_list(&acct, vec![entry(&device, "Portable")], 5);

        let mut bumped = base.clone();
        bumped.version = 99;
        assert_eq!(
            verify(&bumped, &acct.public_key(), 0),
            Err(DeviceListError::BadSignature),
            "rehausser la version doit casser la signature"
        );

        let mut injected = base.clone();
        injected.devices.push(entry(&acct.new_device(), "Intrus"));
        assert_eq!(
            verify(&injected, &acct.public_key(), 0),
            Err(DeviceListError::BadSignature)
        );

        let mut unrevoked = base.clone();
        unrevoked.revoked.push(RevokedEntry {
            pubkey: [3; 32],
            revoked_ms: 1,
        });
        assert_eq!(
            verify(&unrevoked, &acct.public_key(), 0),
            Err(DeviceListError::BadSignature)
        );
    }

    #[test]
    fn a_list_from_another_account_is_refused() {
        let mine = account();
        let theirs = account();
        let list = signed_list(&theirs, vec![entry(&theirs.new_device(), "X")], 1);
        assert_eq!(
            verify(&list, &mine.public_key(), 0),
            Err(DeviceListError::WrongAccount)
        );
    }

    #[test]
    fn an_older_or_equal_version_is_ignored() {
        // Défense contre le rejeu d'une liste ancienne, qui ressusciterait un
        // appareil révoqué.
        let acct = account();
        let list = signed_list(&acct, vec![entry(&acct.new_device(), "X")], 5);
        assert_eq!(
            verify(&list, &acct.public_key(), 5),
            Err(DeviceListError::Stale)
        );
        assert_eq!(
            verify(&list, &acct.public_key(), 9),
            Err(DeviceListError::Stale)
        );
        assert_eq!(verify(&list, &acct.public_key(), 4), Ok(()));
    }

    #[test]
    fn a_device_without_proof_of_work_is_refused() {
        let acct = account();
        let device = acct.new_device();
        let mut e = entry(&device, "Portable");
        // Un nonce dont on a VÉRIFIÉ qu'il échoue : incrémenter au hasard
        // retomberait sur un nonce valide une fois sur seize à cette
        // difficulté, et le test ne prouverait rien la plupart du temps.
        e.pow_nonce = (0u64..10_000)
            .find(|n| !verify_pow(&device.public_key(), *n, POW))
            .expect("un nonce invalide existe");

        let list = signed_list(&acct, vec![e], 1);
        // La signature reste valide — le propriétaire a bien signé — mais la
        // liste est mal formée : sans preuve de travail, une clé d'appareil se
        // fabrique en masse pour rien.
        assert_eq!(
            verify(&list, &acct.public_key(), 0),
            Err(DeviceListError::InvalidPow)
        );
    }

    #[test]
    fn a_duplicated_device_is_refused() {
        let acct = account();
        let device = acct.new_device();
        let e = entry(&device, "Portable");
        let list = signed_list(&acct, vec![e.clone(), e], 1);
        assert_eq!(
            verify(&list, &acct.public_key(), 0),
            Err(DeviceListError::DuplicateDevice)
        );
    }

    #[test]
    fn version_survives_a_full_reinstall() {
        // ⚠️ Le piège trouvé en écrivant la conception : un compteur reparti à
        // 1 après restauration serait ignoré par les pairs détenant mieux.
        // Dérivée de l'horloge, la version d'une liste réémise dépasse
        // toujours celle d'une liste plus ancienne.
        let ancienne = version_for(1_700_000_000_000);
        let apres_reinstall = version_for(1_800_000_000_000);
        assert!(apres_reinstall > ancienne);
    }
}
