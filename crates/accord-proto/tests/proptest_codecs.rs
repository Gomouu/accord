//! Property tests (proptest) des codecs filaires — complètent les vecteurs
//! figés de `roundtrip.rs` et le fuzzing de `fuzz/`. Deux familles d'invariants
//! sont couvertes sur des entrées générées :
//!
//! 1. **Round-trip** : encoder puis décoder une valeur typée rend une valeur
//!    égale (`decode(encode(x)) == x`) — le codec ne perd ni ne déforme rien.
//! 2. **Robustesse** : décoder des octets ARBITRAIRES ne panique jamais
//!    (Ok/Err), et un octet excédentaire est toujours rejeté (décodage strict,
//!    SPEC §0).

use accord_proto::core_msg::GroupOp;
use accord_proto::*;
use proptest::prelude::*;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

/// Stratégie d'un `Hello` de handshake (champs de longueur fixe + cookie borné).
fn hello_strategy() -> impl Strategy<Value = Hello> {
    (
        any::<[u8; 32]>(),
        any::<[u8; 32]>(),
        any::<u64>(),
        any::<u64>(),
        any::<[u8; 16]>(),
        prop::collection::vec(any::<u8>(), 0..40),
        any::<[u8; 64]>(),
    )
        .prop_map(
            |(eph_pub, static_pub, pow_nonce, timestamp_ms, nonce, cookie, sig)| Hello {
                eph_pub,
                static_pub,
                pow_nonce,
                timestamp_ms,
                nonce,
                cookie,
                sig,
            },
        )
}

/// Stratégie d'un `DataPacket` (texte chiffré de longueur bornée réaliste).
fn data_strategy() -> impl Strategy<Value = DataPacket> {
    (
        any::<[u8; 8]>(),
        any::<u8>(),
        any::<u64>(),
        prop::collection::vec(any::<u8>(), 0..600),
    )
        .prop_map(|(session_id, epoch, counter, ciphertext)| DataPacket {
            session_id,
            epoch,
            counter,
            ciphertext,
        })
}

/// Stratégie d'une adresse (IPv4 et IPv6, port quelconque).
fn addr_strategy() -> impl Strategy<Value = SocketAddr> {
    prop_oneof![
        (any::<[u8; 4]>(), any::<u16>())
            .prop_map(|(o, p)| SocketAddr::new(IpAddr::V4(Ipv4Addr::from(o)), p)),
        (any::<[u8; 16]>(), any::<u16>())
            .prop_map(|(o, p)| SocketAddr::new(IpAddr::V6(Ipv6Addr::from(o)), p)),
    ]
}

/// Stratégie d'un `GroupOp` (corps opaque de taille bornée : le décodage du
/// corps est validé séparément, ici c'est l'enveloppe qui round-trip).
fn group_op_strategy() -> impl Strategy<Value = GroupOp> {
    (
        any::<[u8; 16]>(),
        any::<[u8; 16]>(),
        any::<u64>(),
        any::<u64>(),
        any::<[u8; 32]>(),
        any::<u8>(),
        prop::collection::vec(any::<u8>(), 0..300),
        any::<[u8; 64]>(),
    )
        .prop_map(
            |(op_id, group_id, lamport, wall_ms, author, kind, body, sig)| GroupOp {
                op_id,
                group_id,
                lamport,
                wall_ms,
                author,
                kind,
                body,
                sig,
            },
        )
}

proptest! {
    /// Un `Hello` généré survit à un aller-retour d'encodage.
    #[test]
    fn hello_roundtrip(h in hello_strategy()) {
        let p = Packet::Hello(h);
        let back = Packet::from_bytes(&p.to_bytes()).expect("décodage");
        prop_assert_eq!(back, p);
    }

    /// Idem pour un paquet DATA (en-tête + texte chiffré).
    #[test]
    fn data_roundtrip(d in data_strategy()) {
        let p = Packet::Data(d);
        let back = Packet::from_bytes(&p.to_bytes()).expect("décodage");
        prop_assert_eq!(back, p);
    }

    /// L'AAD d'un paquet DATA est exactement le préfixe de l'encodage (lien
    /// authentifié en-tête ↔ octets émis).
    #[test]
    fn data_aad_prefixe_l_encodage(d in data_strategy()) {
        let aad = d.aad();
        let bytes = Packet::Data(d).to_bytes();
        prop_assert_eq!(&bytes[..aad.len()], &aad[..]);
    }

    /// Une `WireAddr` (IPv4/IPv6) round-trip via l'encodage de nœud.
    #[test]
    fn node_info_roundtrip(
        addrs in prop::collection::vec(addr_strategy(), 0..=limits::MAX_NODE_ADDRS),
    ) {
        let info = NodeInfo {
            node_id: NodeId([7; 32]),
            static_pub: [8; 32],
            pow_nonce: 42,
            flags: types::node_flags::RELAY,
            addrs: addrs.into_iter().map(WireAddr).collect(),
        };
        let back = NodeInfo::from_bytes(&info.to_bytes()).expect("décodage");
        prop_assert_eq!(back, info);
    }

    /// L'enveloppe d'un `GroupOp` round-trip (corps opaque préservé octet à octet).
    #[test]
    fn group_op_enveloppe_roundtrip(op in group_op_strategy()) {
        let back = GroupOp::from_bytes(&op.to_bytes()).expect("décodage");
        prop_assert_eq!(back, op);
    }

    /// Un octet excédentaire fait échouer le décodage strict (SPEC §0).
    #[test]
    fn octet_excedentaire_rejete(h in hello_strategy()) {
        let mut bytes = Packet::Hello(h).to_bytes();
        bytes.push(0);
        prop_assert!(Packet::from_bytes(&bytes).is_err());
    }

    /// Décoder des octets ARBITRAIRES ne panique jamais (Ok ou Err).
    #[test]
    fn decode_arbitraire_ne_panique_pas(bytes in prop::collection::vec(any::<u8>(), 0..1024)) {
        let _ = Packet::from_bytes(&bytes);
        let _ = ChannelMsg::from_bytes(&bytes);
        let _ = GroupOp::from_bytes(&bytes);
        let _ = tcp_deframe(&bytes);
    }
}
