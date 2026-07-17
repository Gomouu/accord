//! Cible fuzz : décodage des corps d'opérations de groupe.
//!
//! Invariant : `GroupOpBody::decode_body` ne panique jamais, même avec un
//! discriminant `kind` arbitraire (inconnu ⇒ `Err(InvalidValue)`) ou un corps
//! tronqué/excédentaire (⇒ `Err`).

#![no_main]

use accord_proto::core_msg::GroupOpBody;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Premier octet = discriminant `kind`, le reste = corps encodé.
    let Some((&kind, body)) = data.split_first() else {
        return;
    };
    let _ = GroupOpBody::decode_body(kind, body);
});
