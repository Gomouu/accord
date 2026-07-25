//! Cible fuzz : décodage de la liste d'appareils d'un compte.
//!
//! Cette structure arrive de la **DHT**, donc d'inconnus, et est décodée
//! **avant** que quoi que ce soit d'elle ne soit vérifié — ni signature, ni
//! preuve de travail, ni fraîcheur. C'est le seul endroit du multi-appareil où
//! un octet arbitraire touche un décodeur ; ses bornes (`MAX_DEVICES`,
//! `MAX_REVOKED`, `MAX_DEVICE_NAME`) sont donc appliquées au décodage et pas à
//! l'usage, et c'est cette promesse-là que la cible tient sous pression.
//!
//! Invariants vérifiés :
//! - le décodage strict ne panique ni n'alloue hors bornes sur une entrée
//!   arbitraire ;
//! - ce qui se décode se ré-encode **à l'identique**, sans quoi deux pairs
//!   calculeraient des signatures différentes sur la même liste ;
//! - les prédicats lus sans vérification préalable — `authorises`, `is_fresh`,
//!   `presents_own_key` — ne paniquent pas non plus, y compris sur des
//!   horodatages absurdes.

#![no_main]

use accord_proto::device::{DeviceEntry, DeviceList};
use accord_proto::{WireDecode, WireEncode};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Une entrée seule : c'est la forme que l'appairage transporte, scellée
    // sous la clé du canal — donc lisible par un pair qui a le code, et par
    // lui seul, mais décodée avant toute autre vérification.
    let _ = DeviceEntry::from_bytes(data);

    let Ok(list) = DeviceList::from_bytes(data) else {
        return;
    };

    // 🔒 Aller-retour exact. La signature couvre `signable_bytes`, calculé à
    // partir des champs décodés : si l'encodage d'une liste décodée différait
    // d'un seul octet, un pair validerait une signature que l'autre rejette.
    assert_eq!(
        DeviceList::from_bytes(&list.to_bytes()).as_ref(),
        Ok(&list),
        "une liste décodée doit se ré-encoder à l'identique"
    );

    // Prédicats appelés sur une liste non vérifiée : ils doivent tolérer
    // n'importe quoi, y compris des dates au bord du domaine.
    for now in [0u64, 1_700_000_000_000, u64::MAX] {
        let _ = list.is_fresh(now);
    }
    for device in &list.devices {
        let _ = list.authorises(&device.pubkey);
        let _ = device.presents_own_key();
    }
    for revoked in &list.revoked {
        assert!(
            !list.authorises(&revoked.pubkey),
            "une clé révoquée ne doit jamais être autorisée"
        );
    }
});
