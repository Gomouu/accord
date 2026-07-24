//! Liste d'appareils d'un compte (multi-appareil, jalon 1).
//!
//! Voir `docs/MULTI_DEVICE.md` pour la conception. En deux phrases : une
//! identité Accord devient un **compte** (clé racine) qui autorise plusieurs
//! **appareils**, chacun avec sa propre clé. C'est ce qui permet à deux
//! machines d'exister en même temps sans que l'invariant du transport — au
//! plus une session directe par identité — ne les fasse s'évincer l'une
//! l'autre.
//!
//! Ce module ne définit que la **structure filaire** et son encodage. La
//! signature et la vérification vivent dans `accord-crypto` ; la publication
//! dans la DHT, dans `accord-node`.
//!
//! 🔒 Cette structure arrive de la DHT, donc d'inconnus, et est décodée
//! **avant** que quoi que ce soit d'elle ne soit vérifié. Toutes ses bornes
//! sont donc appliquées au décodage, pas à l'usage.

use crate::wire::{DecodeError, Reader, WireDecode, WireEncode, Writer};

/// Appareils maximum par compte. Borne volontairement basse : personne n'en a
/// huit, et chaque entrée est du travail de vérification et de chiffrement en
/// plus pour tous les correspondants du compte.
pub const MAX_DEVICES: usize = 8;

/// Révocations conservées dans une liste. Au-delà, les plus anciennes sortent :
/// un appareil révoqué depuis longtemps est de toute façon inconnu des pairs
/// récents, et la liste ne doit pas croître sans fin.
pub const MAX_REVOKED: usize = 32;

/// Longueur maximale du nom d'un appareil (octets UTF-8).
pub const MAX_DEVICE_NAME: usize = 32;

/// Un appareil autorisé à agir pour le compte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceEntry {
    /// Clé publique Ed25519 de l'appareil. C'est **elle** que le transport
    /// utilise pour ses sessions, jamais la clé du compte.
    pub pubkey: [u8; 32],
    /// Preuve de travail de la clé d'appareil (même règle que toute identité).
    pub pow_nonce: u64,
    /// Nom lisible, choisi par l'utilisateur (« Portable »).
    pub name: String,
    /// Date d'ajout (ms epoch).
    pub added_ms: u64,
    /// Drapeaux réservés. 🔒 Les bits inconnus sont **ignorés**, jamais une
    /// erreur : c'est ce qui permettra d'en ajouter sans casser les anciens.
    pub flags: u32,
}

/// Un appareil retiré du compte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevokedEntry {
    /// Clé publique de l'appareil révoqué.
    pub pubkey: [u8; 32],
    /// Date de révocation (ms epoch).
    pub revoked_ms: u64,
}

/// Liste d'appareils signée par la clé racine du compte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceList {
    /// Clé publique Ed25519 racine du compte.
    pub account: [u8; 32],
    /// Numéro de version **monotone**. Une version inférieure ou égale à
    /// celle qu'on détient déjà est ignorée : c'est la seule défense contre le
    /// rejeu d'une liste ancienne qui ressusciterait un appareil révoqué.
    pub version: u64,
    /// Date d'émission (ms epoch).
    pub issued_ms: u64,
    /// Durée de validité (secondes). Passé ce délai, un détenteur doit
    /// rafraîchir avant de faire confiance : sans quoi un appareil révoqué
    /// pourrait vivre indéfiniment sur une liste périmée.
    pub valid_for_s: u32,
    /// Appareils autorisés.
    pub devices: Vec<DeviceEntry>,
    /// Appareils explicitement révoqués.
    pub revoked: Vec<RevokedEntry>,
    /// Signature racine sur [`DeviceList::signable_bytes`].
    pub sig: [u8; 64],
}

impl DeviceList {
    /// Octets couverts par la signature racine : **tout sauf la signature**.
    ///
    /// 🔒 `version` en fait partie. Sans elle, un attaquant réécrirait le
    /// numéro d'une liste authentique pour la faire passer pour plus récente,
    /// et ferait ainsi ressusciter un appareil révoqué.
    pub fn signable_bytes(&self) -> Vec<u8> {
        let mut w = Writer::with_capacity(128 + self.devices.len() * 64);
        w.put_raw(b"accord-device-list-v1");
        w.put_arr(&self.account);
        w.put_u64(self.version);
        w.put_u64(self.issued_ms);
        w.put_u32(self.valid_for_s);
        w.put_list(&self.devices, |w, d| {
            w.put_arr(&d.pubkey);
            w.put_u64(d.pow_nonce);
            w.put_str(&d.name);
            w.put_u64(d.added_ms);
            w.put_u32(d.flags);
        });
        w.put_list(&self.revoked, |w, r| {
            w.put_arr(&r.pubkey);
            w.put_u64(r.revoked_ms);
        });
        w.into_bytes()
    }

    /// Vrai si `pubkey` figure parmi les appareils autorisés.
    ///
    /// Une clé présente **à la fois** dans `devices` et dans `revoked` compte
    /// comme révoquée : sur une liste incohérente, le refus est le seul choix
    /// sûr, et une liste forgée ne doit pas pouvoir réautoriser par simple
    /// duplication.
    pub fn authorises(&self, pubkey: &[u8; 32]) -> bool {
        if self.revoked.iter().any(|r| r.pubkey == *pubkey) {
            return false;
        }
        self.devices.iter().any(|d| d.pubkey == *pubkey)
    }

    /// Vrai si la liste est encore dans sa fenêtre de validité à `now_ms`.
    ///
    /// Une liste émise « dans le futur » (horloge du publieur en avance, ou
    /// horodatage gonflé pour la rendre éternelle) est traitée comme valide
    /// seulement jusqu'à sa date de fin : `issued_ms` ne sert qu'à calculer
    /// l'échéance, jamais à prolonger la confiance.
    pub fn is_fresh(&self, now_ms: u64) -> bool {
        let ttl = u64::from(self.valid_for_s).saturating_mul(1000);
        now_ms <= self.issued_ms.saturating_add(ttl)
    }
}

impl WireEncode for DeviceList {
    fn encode(&self, w: &mut Writer) {
        w.put_raw(&self.signable_bytes());
        w.put_arr(&self.sig);
    }
}

impl WireDecode for DeviceList {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        let mut domain = [0u8; 21];
        domain.copy_from_slice(r.take_exact(21)?);
        if &domain != b"accord-device-list-v1" {
            return Err(DecodeError::InvalidValue("device_list.domain"));
        }
        let account = r.arr()?;
        let version = r.u64()?;
        let issued_ms = r.u64()?;
        let valid_for_s = r.u32()?;
        let devices = r.list(MAX_DEVICES, "device_list.devices", |r| {
            Ok(DeviceEntry {
                pubkey: r.arr()?,
                pow_nonce: r.u64()?,
                name: r.str(MAX_DEVICE_NAME, "device.name")?,
                added_ms: r.u64()?,
                flags: r.u32()?,
            })
        })?;
        let revoked = r.list(MAX_REVOKED, "device_list.revoked", |r| {
            Ok(RevokedEntry {
                pubkey: r.arr()?,
                revoked_ms: r.u64()?,
            })
        })?;
        let sig = r.arr()?;
        Ok(DeviceList {
            account,
            version,
            issued_ms,
            valid_for_s,
            devices,
            revoked,
            sig,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(n: u8) -> DeviceEntry {
        DeviceEntry {
            pubkey: [n; 32],
            pow_nonce: u64::from(n),
            name: format!("Appareil {n}"),
            added_ms: 1_700_000_000_000,
            flags: 0,
        }
    }

    fn list() -> DeviceList {
        DeviceList {
            account: [1; 32],
            version: 3,
            issued_ms: 1_700_000_000_000,
            valid_for_s: 7 * 24 * 3600,
            devices: vec![device(10), device(11)],
            revoked: vec![RevokedEntry {
                pubkey: [12; 32],
                revoked_ms: 1_699_000_000_000,
            }],
            sig: [9; 64],
        }
    }

    #[test]
    fn roundtrips() {
        let l = list();
        assert_eq!(DeviceList::from_bytes(&l.to_bytes()).unwrap(), l);
    }

    #[test]
    fn empty_list_roundtrips() {
        let mut l = list();
        l.devices.clear();
        l.revoked.clear();
        assert_eq!(DeviceList::from_bytes(&l.to_bytes()).unwrap(), l);
    }

    #[test]
    fn signable_bytes_cover_the_version() {
        // 🔒 Sans ça, réécrire le numéro d'une liste authentique suffirait à la
        // faire passer pour plus récente — et à ressusciter un appareil révoqué.
        let a = list();
        let mut b = list();
        b.version += 1;
        assert_ne!(a.signable_bytes(), b.signable_bytes());
    }

    #[test]
    fn signable_bytes_cover_every_field_but_the_signature() {
        let base = list();
        let mut variants = Vec::new();

        let mut v = base.clone();
        v.account[0] ^= 1;
        variants.push(v);
        let mut v = base.clone();
        v.issued_ms += 1;
        variants.push(v);
        let mut v = base.clone();
        v.valid_for_s += 1;
        variants.push(v);
        let mut v = base.clone();
        v.devices.push(device(20));
        variants.push(v);
        let mut v = base.clone();
        v.devices[0].pubkey[0] ^= 1;
        variants.push(v);
        let mut v = base.clone();
        v.devices[0].name.push('x');
        variants.push(v);
        let mut v = base.clone();
        v.revoked.clear();
        variants.push(v);

        for v in variants {
            assert_ne!(v.signable_bytes(), base.signable_bytes());
        }

        // La signature, elle, n'est pas dans son propre domaine.
        let mut v = base.clone();
        v.sig[0] ^= 1;
        assert_eq!(v.signable_bytes(), base.signable_bytes());
    }

    #[test]
    fn too_many_devices_rejected_at_decode() {
        // La structure vient de la DHT : la borne s'applique au décodage, pas
        // à l'usage, sinon elle arrive trop tard.
        let mut l = list();
        l.devices = (0..=MAX_DEVICES as u8).map(device).collect();
        assert!(l.devices.len() > MAX_DEVICES);
        assert_eq!(
            DeviceList::from_bytes(&l.to_bytes()),
            Err(DecodeError::TooLarge("device_list.devices"))
        );
    }

    #[test]
    fn exactly_max_devices_accepted() {
        let mut l = list();
        l.devices = (0..MAX_DEVICES as u8).map(device).collect();
        assert!(DeviceList::from_bytes(&l.to_bytes()).is_ok());
    }

    #[test]
    fn too_many_revocations_rejected_at_decode() {
        let mut l = list();
        l.revoked = (0..=MAX_REVOKED as u8)
            .map(|n| RevokedEntry {
                pubkey: [n; 32],
                revoked_ms: 0,
            })
            .collect();
        assert_eq!(
            DeviceList::from_bytes(&l.to_bytes()),
            Err(DecodeError::TooLarge("device_list.revoked"))
        );
    }

    #[test]
    fn oversized_device_name_rejected() {
        let mut l = list();
        l.devices[0].name = "n".repeat(MAX_DEVICE_NAME + 1);
        assert_eq!(
            DeviceList::from_bytes(&l.to_bytes()),
            Err(DecodeError::TooLarge("device.name"))
        );
    }

    #[test]
    fn foreign_structure_rejected_by_the_domain_tag() {
        // Le préfixe de domaine empêche de faire passer une autre structure
        // signée pour une liste d'appareils.
        let mut bytes = list().to_bytes();
        bytes[0] ^= 1;
        assert_eq!(
            DeviceList::from_bytes(&bytes),
            Err(DecodeError::InvalidValue("device_list.domain"))
        );
    }

    #[test]
    fn truncated_list_rejected() {
        let bytes = list().to_bytes();
        for cut in [1, 10, 40, bytes.len() - 1] {
            assert!(DeviceList::from_bytes(&bytes[..cut]).is_err(), "cut={cut}");
        }
    }

    #[test]
    fn authorises_only_listed_devices() {
        let l = list();
        assert!(l.authorises(&[10; 32]));
        assert!(l.authorises(&[11; 32]));
        assert!(!l.authorises(&[99; 32]));
    }

    #[test]
    fn revocation_wins_over_listing() {
        // Liste incohérente (forgée ou boguée) : le refus est le seul choix
        // sûr, sinon dupliquer une clé suffirait à annuler sa révocation.
        let mut l = list();
        l.revoked.push(RevokedEntry {
            pubkey: [10; 32],
            revoked_ms: 0,
        });
        assert!(!l.authorises(&[10; 32]));
    }

    #[test]
    fn freshness_follows_the_declared_lifetime() {
        let l = list();
        let fin = l.issued_ms + u64::from(l.valid_for_s) * 1000;
        assert!(l.is_fresh(l.issued_ms));
        assert!(l.is_fresh(fin));
        assert!(!l.is_fresh(fin + 1));
    }

    #[test]
    fn absurd_lifetime_does_not_overflow() {
        // Un `valid_for_s` maximal ne doit pas déborder en une échéance qui
        // repasse dans le passé — ce qui rendrait la liste éternellement
        // périmée, ou éternellement valide selon le sens du débordement.
        let mut l = list();
        l.issued_ms = u64::MAX - 10;
        l.valid_for_s = u32::MAX;
        assert!(l.is_fresh(u64::MAX));
    }
}
