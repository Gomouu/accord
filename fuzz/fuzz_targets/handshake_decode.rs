//! Cible fuzz : décodage des enveloppes de handshake et du framing bas niveau.
//!
//! Invariant : `Packet::from_bytes` (HELLO/WELCOME/COOKIE/DATA) ne panique
//! jamais sur une entrée arbitraire — les longueurs et listes déclarées sont
//! bornées avant toute allocation (SPEC §0, §2). Complète `proto_decode` en
//! ciblant spécifiquement les chemins de handshake (Hello/Welcome portent des
//! clés publiques, une preuve de travail et des candidats d'adresse).

#![no_main]

use accord_proto::{Packet, WireDecode};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Enveloppe de handshake (Hello/Welcome/Cookie/Data) : décodage strict.
    let _ = Packet::from_bytes(data);
});
