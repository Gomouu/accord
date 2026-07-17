//! Cible fuzz : décodage des enveloppes réseau accord-proto.
//!
//! Invariant : quel que soit l'octet stream fourni, le décodage retourne
//! `Ok`/`Err` sans jamais paniquer ni allouer hors bornes (les longueurs
//! déclarées sont validées avant toute allocation, SPEC §0).

#![no_main]

use accord_proto::{tcp_deframe, ChannelMsg, Packet, WireDecode};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Enveloppe externe (HELLO/WELCOME/DATA/COOKIE).
    let _ = Packet::from_bytes(data);
    // Contenu déchiffré d'un DATA, démultiplexé par canal.
    let _ = ChannelMsg::from_bytes(data);
    // Framing TCP (préfixe de longueur u32 big-endian, borné à MAX_TCP_FRAME).
    let _ = tcp_deframe(data);
});
