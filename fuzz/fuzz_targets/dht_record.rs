//! Cible fuzz : décodage des messages DHT et des records qu'ils transportent.
//!
//! Invariant : `DhtMessage::from_bytes` ne panique jamais sur une entrée
//! arbitraire — listes de nœuds bornées (`DHT_K`), records à longueur validée
//! avant allocation (SPEC §0, §4). Cible la surface de remise hors-ligne
//! (boîtes aux lettres, présence, identités) exposée à des pairs non fiables.

#![no_main]

use accord_proto::dht_msg::DhtMessage;
use accord_proto::WireDecode;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = DhtMessage::from_bytes(data);
});
