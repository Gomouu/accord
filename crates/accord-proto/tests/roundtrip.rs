//! Tests de round-trip et vecteurs figés du protocole filaire (SPEC.md).

use accord_proto::core_msg::{ChannelKind, CoreMsg, FileRef, GroupOp, GroupOpBody, MsgBody};
use accord_proto::dht_msg::{DhtBody, DhtMessage};
use accord_proto::file_msg::{FileMsg, Manifest};
use accord_proto::*;
use std::net::SocketAddr;

fn roundtrip_packet(p: &Packet) {
    let bytes = p.to_bytes();
    let back = Packet::from_bytes(&bytes).expect("decode");
    assert_eq!(&back, p);
}

fn roundtrip_channel(m: &ChannelMsg) {
    let bytes = m.to_bytes();
    let back = ChannelMsg::from_bytes(&bytes).expect("decode");
    assert_eq!(&back, m);
}

fn sample_hello() -> Hello {
    Hello {
        eph_pub: [1; 32],
        static_pub: [2; 32],
        pow_nonce: 0xDEAD_BEEF,
        timestamp_ms: 1_700_000_000_000,
        nonce: [3; 16],
        cookie: vec![],
        sig: [4; 64],
    }
}

#[test]
fn packet_hello_roundtrip() {
    roundtrip_packet(&Packet::Hello(sample_hello()));
    let mut with_cookie = sample_hello();
    with_cookie.cookie = vec![9; 16];
    roundtrip_packet(&Packet::Hello(with_cookie));
}

#[test]
fn packet_hello_golden_vector() {
    // Vecteur figé : toute divergence casse la compatibilité filaire.
    let bytes = Packet::Hello(sample_hello()).to_bytes();
    assert_eq!(bytes.len(), 1 + 1 + 32 + 32 + 8 + 8 + 16 + 2 + 64);
    assert_eq!(bytes[0], 0x01, "version");
    assert_eq!(bytes[1], 0x01, "class hello");
    assert_eq!(&bytes[2..34], &[1u8; 32][..], "eph_pub");
    assert_eq!(&bytes[34..66], &[2u8; 32][..], "static_pub");
    assert_eq!(
        &bytes[66..74],
        &0xDEAD_BEEFu64.to_be_bytes()[..],
        "pow_nonce big-endian"
    );
    assert_eq!(&bytes[74..82], &1_700_000_000_000u64.to_be_bytes()[..]);
    assert_eq!(&bytes[82..98], &[3u8; 16][..], "nonce");
    assert_eq!(&bytes[98..100], &[0, 0], "cookie vide");
    assert_eq!(&bytes[100..], &[4u8; 64][..], "sig");
}

#[test]
fn packet_welcome_and_cookie_roundtrip() {
    roundtrip_packet(&Packet::Welcome(Welcome {
        eph_pub: [5; 32],
        static_pub: [6; 32],
        pow_nonce: 7,
        timestamp_ms: 8,
        nonce: [9; 16],
        session_id: [10; 8],
        sig: [11; 64],
    }));
    roundtrip_packet(&Packet::Cookie(CookiePacket {
        cookie: vec![1, 2, 3],
    }));
}

#[test]
fn packet_data_roundtrip_and_aad() {
    let d = DataPacket {
        session_id: [7; 8],
        epoch: 2,
        counter: 42,
        ciphertext: vec![0xAB; 100],
    };
    let aad = d.aad();
    assert_eq!(aad[0], PROTOCOL_VERSION);
    assert_eq!(aad[1], 0x03);
    assert_eq!(&aad[2..10], &[7; 8]);
    assert_eq!(aad[10], 2);
    assert_eq!(&aad[11..19], &42u64.to_be_bytes());
    // L'AAD est exactement le préfixe du paquet encodé.
    let bytes = Packet::Data(d.clone()).to_bytes();
    assert_eq!(&bytes[..19], &aad[..]);
    roundtrip_packet(&Packet::Data(d));
}

#[test]
fn unknown_version_rejected() {
    let mut bytes = Packet::Hello(sample_hello()).to_bytes();
    bytes[0] = 9;
    assert_eq!(
        Packet::from_bytes(&bytes),
        Err(DecodeError::UnsupportedVersion(9))
    );
    bytes[0] = 0;
    assert!(Packet::from_bytes(&bytes).is_err());
}

#[test]
fn trailing_bytes_rejected() {
    let mut bytes = Packet::Welcome(Welcome {
        eph_pub: [5; 32],
        static_pub: [6; 32],
        pow_nonce: 7,
        timestamp_ms: 8,
        nonce: [9; 16],
        session_id: [10; 8],
        sig: [11; 64],
    })
    .to_bytes();
    bytes.push(0);
    assert_eq!(Packet::from_bytes(&bytes), Err(DecodeError::TrailingBytes));
}

#[test]
fn truncated_rejected() {
    let bytes = Packet::Hello(sample_hello()).to_bytes();
    for cut in [1, 2, 50, bytes.len() - 1] {
        assert!(Packet::from_bytes(&bytes[..cut]).is_err(), "cut={cut}");
    }
}

fn sample_node(i: u8) -> NodeInfo {
    NodeInfo {
        node_id: NodeId([i; 32]),
        static_pub: [i.wrapping_add(1); 32],
        pow_nonce: u64::from(i),
        flags: types::node_flags::RELAY,
        addrs: vec![
            WireAddr("192.168.1.10:4433".parse::<SocketAddr>().unwrap()),
            WireAddr("[2001:db8::1]:4433".parse::<SocketAddr>().unwrap()),
        ],
    }
}

#[test]
fn control_msgs_roundtrip() {
    for m in [
        ControlMsg::Ping { token: 1 },
        ControlMsg::Pong { token: 2 },
        ControlMsg::Close { reason: 3 },
        ControlMsg::Rekey { new_epoch: 4 },
        ControlMsg::ObserveAddrReq,
        ControlMsg::ObserveAddrResp {
            addr: WireAddr("10.0.0.1:9".parse().unwrap()),
        },
        ControlMsg::PunchRequest {
            token: 5,
            candidates: vec![
                WireAddr("203.0.113.7:48016".parse().unwrap()),
                WireAddr("[2001:db8::1]:48016".parse().unwrap()),
            ],
        },
        ControlMsg::PunchResponse {
            token: 5,
            candidates: vec![],
        },
        ControlMsg::NodeAnnounce {
            pow_nonce: 0xDEAD_BEEF,
            flags: types::node_flags::RELAY,
        },
    ] {
        roundtrip_channel(&ChannelMsg::Control(m));
    }
}

#[test]
fn node_announce_refuse_les_octets_excedentaires() {
    // Décodage strict : une annonce suivie d'octets résiduels est rejetée
    // (aucune tolérance sur la nouvelle surface filaire, SPEC §12).
    let mut bytes = ChannelMsg::Control(ControlMsg::NodeAnnounce {
        pow_nonce: 7,
        flags: 0,
    })
    .to_bytes();
    bytes.push(0);
    assert!(ChannelMsg::from_bytes(&bytes).is_err());
    // Annonce tronquée : rejetée aussi.
    let ok = ChannelMsg::Control(ControlMsg::NodeAnnounce {
        pow_nonce: 7,
        flags: 0,
    })
    .to_bytes();
    assert!(ChannelMsg::from_bytes(&ok[..ok.len() - 1]).is_err());
}

#[test]
fn punch_candidates_bornes_au_decodage() {
    // Un pair malveillant qui force plus de MAX_PUNCH_CANDIDATES candidats doit
    // être rejeté au décodage (anti-arrosage : SPEC §11.2).
    let flood: Vec<WireAddr> = (0..limits::MAX_PUNCH_CANDIDATES + 1)
        .map(|i| WireAddr(format!("203.0.113.7:{}", 1000 + i).parse().unwrap()))
        .collect();
    let bytes = ChannelMsg::Control(ControlMsg::PunchRequest {
        token: 1,
        candidates: flood.clone(),
    })
    .to_bytes();
    assert!(
        ChannelMsg::from_bytes(&bytes).is_err(),
        "au-delà de la borne : décodage refusé"
    );
    // À la borne exacte : accepté.
    let max: Vec<WireAddr> = flood[..limits::MAX_PUNCH_CANDIDATES].to_vec();
    roundtrip_channel(&ChannelMsg::Control(ControlMsg::PunchResponse {
        token: 2,
        candidates: max,
    }));
}

#[test]
fn dht_msgs_roundtrip() {
    let record = DhtRecord {
        key: [1; 32],
        kind: RecordKind::Identity,
        value: vec![7; 100],
        publisher: [2; 32],
        timestamp_ms: 123,
        expiry_s: 3600,
        sig: [3; 64],
    };
    for body in [
        DhtBody::Ping,
        DhtBody::Pong,
        DhtBody::FindNode { target: [9; 32] },
        DhtBody::FoundNodes {
            nodes: vec![sample_node(1), sample_node(2)],
        },
        DhtBody::FindValue { key: [8; 32] },
        DhtBody::FoundValue {
            value: Some(record.clone()),
            nodes: vec![],
        },
        DhtBody::FoundValue {
            value: None,
            nodes: vec![sample_node(3)],
        },
        DhtBody::Store { record },
        DhtBody::StoreOk,
        DhtBody::Error { code: 0x05 },
    ] {
        roundtrip_channel(&ChannelMsg::Dht(DhtMessage {
            rpc_id: [0xCC; 20],
            body,
        }));
    }
}

#[test]
fn dht_record_signable_bytes_stable() {
    let record = DhtRecord {
        key: [1; 32],
        kind: RecordKind::Presence,
        value: vec![0xAA, 0xBB],
        publisher: [2; 32],
        timestamp_ms: 0x0102030405060708,
        expiry_s: 60,
        sig: [0; 64],
    };
    let sb = record.signable_bytes();
    // key(32) ‖ kind(1) ‖ lbytes(4+2) ‖ ts(8) ‖ expiry(4)
    assert_eq!(sb.len(), 32 + 1 + 4 + 2 + 8 + 4);
    assert_eq!(sb[32], 0x02);
    assert_eq!(&sb[33..37], &2u32.to_be_bytes());
}

#[test]
fn dht_record_expiry_bound() {
    let mut w = Writer::new();
    DhtRecord {
        key: [1; 32],
        kind: RecordKind::Presence,
        value: vec![],
        publisher: [2; 32],
        timestamp_ms: 0,
        expiry_s: limits::DHT_MAX_EXPIRY_S + 1,
        sig: [0; 64],
    }
    .encode(&mut w);
    assert_eq!(
        DhtRecord::from_bytes(&w.into_bytes()),
        Err(DecodeError::TooLarge("record.expiry"))
    );
}

#[test]
fn msg_body_roundtrip() {
    let bodies = [
        MsgBody::Text {
            text: "salut ⚡ **gras**".into(),
            reply_to: Some([1; 16]),
            attachments: vec![FileRef {
                merkle_root: [2; 32],
                name: "photo.png".into(),
                size: 12345,
                mime: "image/png".into(),
            }],
        },
        MsgBody::Edit {
            target: [3; 16],
            new_text: "corrigé".into(),
        },
        MsgBody::Delete { target: [4; 16] },
        MsgBody::Reaction {
            target: [5; 16],
            emoji: "🔥".into(),
            add: true,
        },
        MsgBody::Sticker {
            name: "wave".into(),
            merkle_root: [7; 32],
        },
        MsgBody::Typing,
        MsgBody::ReadReceipt { up_to: [6; 16] },
        MsgBody::Poll {
            poll_id: [8; 16],
            question: "Pizza ou sushis ?".into(),
            options: vec!["Pizza".into(), "Sushis".into()],
        },
        MsgBody::Pin {
            msg_id: [9; 16],
            pinned: true,
        },
    ];
    for body in bodies {
        let enc = body.encode_body();
        let back = MsgBody::decode_body(body.kind(), &enc).expect("decode");
        assert_eq!(back, body);
    }
}

#[test]
fn group_op_bodies_roundtrip() {
    let bodies = [
        GroupOpBody::Create {
            name: "Mon serveur".into(),
        },
        GroupOpBody::SetMeta {
            name: "Renommé".into(),
            icon: Some([1; 32]),
            banner_color: Some(0x5865F2),
        },
        GroupOpBody::AddChannel {
            channel_id: [2; 16],
            name: "général".into(),
            category: None,
            kind: ChannelKind::Text,
            position: 0,
        },
        GroupOpBody::AddChannel {
            channel_id: [3; 16],
            name: "Vocal".into(),
            category: Some([4; 16]),
            kind: ChannelKind::Voice,
            position: 1,
        },
        GroupOpBody::AddChannel {
            channel_id: [5; 16],
            name: "Forum".into(),
            category: None,
            kind: ChannelKind::Forum,
            position: 3,
        },
        GroupOpBody::EditChannel {
            channel_id: [2; 16],
            name: "général-2".into(),
            position: 2,
        },
        GroupOpBody::DelChannel {
            channel_id: [2; 16],
        },
        GroupOpBody::AddCategory {
            category_id: [4; 16],
            name: "SALONS".into(),
            position: 0,
        },
        GroupOpBody::AddMember {
            member: [5; 32],
            invite_id: Some([6; 16]),
        },
        GroupOpBody::Kick { member: [5; 32] },
        GroupOpBody::Ban { member: [5; 32] },
        GroupOpBody::Unban { member: [5; 32] },
        GroupOpBody::AddRole {
            role_id: [7; 16],
            name: "Modo".into(),
            color: 0x5865F2,
            position: 1,
            permissions: 0x1FF,
        },
        GroupOpBody::EditRole {
            role_id: [7; 16],
            name: "Modérateur".into(),
            color: 0xFF0000,
            position: 2,
            permissions: 0x0FF,
        },
        GroupOpBody::DelRole { role_id: [7; 16] },
        GroupOpBody::AssignRole {
            member: [5; 32],
            role_id: [7; 16],
        },
        GroupOpBody::UnassignRole {
            member: [5; 32],
            role_id: [7; 16],
        },
        GroupOpBody::SetChannelPerms {
            channel_id: [2; 16],
            role_id: [7; 16],
            allow: 3,
            deny: 4,
        },
        GroupOpBody::Pin {
            channel_id: [2; 16],
            msg_id: [8; 16],
        },
        GroupOpBody::Unpin {
            channel_id: [2; 16],
            msg_id: [8; 16],
        },
        GroupOpBody::DeleteMsg {
            channel_id: [2; 16],
            msg_id: [8; 16],
        },
        GroupOpBody::SetTopic {
            channel_id: [2; 16],
            topic: "Bienvenue !".into(),
        },
        GroupOpBody::InviteCreate {
            invite_id: [9; 16],
            code_hash: [10; 32],
            max_uses: 5,
            expires_ms: 999,
        },
        GroupOpBody::InviteRevoke { invite_id: [9; 16] },
        GroupOpBody::Leave,
        GroupOpBody::AddEmoji {
            name: "party_parrot".into(),
            file: [11; 32],
        },
        GroupOpBody::DelEmoji {
            name: "party_parrot".into(),
        },
        GroupOpBody::EditCategory {
            category_id: [12; 16],
            name: "Textuels".into(),
            position: 3,
        },
        GroupOpBody::DelCategory {
            category_id: [12; 16],
        },
        GroupOpBody::SetChannelCategory {
            channel_id: [4; 16],
            category: Some([12; 16]),
        },
        GroupOpBody::SetChannelCategory {
            channel_id: [4; 16],
            category: None,
        },
        GroupOpBody::TimeoutMember {
            member: [6; 32],
            until_ms: 1_700_000_000_000,
        },
        GroupOpBody::TimeoutMember {
            member: [6; 32],
            until_ms: 0,
        },
        GroupOpBody::SetNickname {
            member: [6; 32],
            name: "Capitaine".into(),
        },
        GroupOpBody::SetNickname {
            member: [6; 32],
            name: String::new(),
        },
        GroupOpBody::EventCreate {
            event_id: [13; 16],
            title: "Soirée jeux".into(),
            description: "Amenez vos manettes.".into(),
            start_ms: 1_700_000_000_000,
            channel_id: Some([2; 16]),
        },
        GroupOpBody::EventEdit {
            event_id: [13; 16],
            title: "Soirée jeux (reportée)".into(),
            description: String::new(),
            start_ms: 1_700_100_000_000,
            channel_id: None,
        },
        GroupOpBody::EventDelete { event_id: [13; 16] },
        GroupOpBody::EventRsvp {
            event_id: [13; 16],
            interested: true,
        },
        GroupOpBody::StickerAdd {
            name: "wave".into(),
            file: [14; 32],
        },
        GroupOpBody::StickerRemove {
            name: "wave".into(),
        },
        GroupOpBody::SetMemberAvatar {
            avatar: Some([15; 32]),
        },
        GroupOpBody::SetMemberAvatar { avatar: None },
        GroupOpBody::PollVote {
            poll_id: [16; 16],
            option_index: 2,
        },
        GroupOpBody::PollClose { poll_id: [16; 16] },
        GroupOpBody::PollCreate {
            poll_id: [16; 16],
            channel_id: [17; 16],
            msg_id: [18; 16],
        },
        GroupOpBody::PollDelete { poll_id: [16; 16] },
        GroupOpBody::SetAutoModWords {
            words: vec!["spam".into(), "vilain-mot".into()],
        },
        GroupOpBody::SetAutoModWords { words: vec![] },
        GroupOpBody::CreateThread {
            thread_id: [19; 16],
            parent_channel: [2; 16],
            root_msg: [20; 16],
            name: "Discussion annexe".into(),
        },
        GroupOpBody::SetThreadArchived {
            thread_id: [19; 16],
            archived: true,
        },
        GroupOpBody::SetBanner {
            banner: Some([21; 32]),
        },
        GroupOpBody::SetBanner { banner: None },
    ];
    for body in bodies {
        let enc = body.encode_body();
        let back = GroupOpBody::decode_body(body.kind(), &enc).expect("decode");
        assert_eq!(back, body);
    }
}

#[test]
fn emoji_op_kinds_and_name_bound() {
    assert_eq!(
        GroupOpBody::AddEmoji {
            name: "a".into(),
            file: [0; 32],
        }
        .kind(),
        0x18
    );
    assert_eq!(GroupOpBody::DelEmoji { name: "a".into() }.kind(), 0x19);
    // Un nom au-delà de 32 octets est rejeté au décodage (l'op sera ignorée
    // au repli du journal, jamais paniquée).
    let mut w = accord_proto::wire::Writer::new();
    w.put_str(&"x".repeat(33));
    w.put_arr(&[0u8; 32]);
    assert!(GroupOpBody::decode_body(0x18, &w.into_bytes()).is_err());
}

#[test]
fn core_msgs_roundtrip() {
    let op = GroupOp {
        op_id: [1; 16],
        group_id: [2; 16],
        lamport: 3,
        wall_ms: 4,
        author: [5; 32],
        kind: 0x01,
        body: GroupOpBody::Create { name: "G".into() }.encode_body(),
        sig: [6; 64],
    };
    let msgs = [
        CoreMsg::DirectMsg {
            msg_id: [1; 16],
            lamport: 2,
            sent_ms: 3,
            kind: 0,
            body: vec![1, 2, 3],
        },
        CoreMsg::MsgAck { msg_id: [1; 16] },
        CoreMsg::FriendRequest {
            display_name: "Anna".into(),
            message: "salut !".into(),
            verify_phrase: Some("tournesol".into()),
        },
        CoreMsg::FriendResponse { accepted: true },
        CoreMsg::GroupOpMsg { op: op.clone() },
        CoreMsg::GroupMsg {
            group_id: [2; 16],
            channel_id: [3; 16],
            msg_id: [4; 16],
            lamport: 5,
            sent_ms: 6,
            key_epoch: 7,
            body_enc: vec![9; 48],
        },
        CoreMsg::GroupKey {
            group_id: [2; 16],
            key_epoch: 8,
            sealed_key: [0xEE; 80],
        },
        CoreMsg::Presence {
            status: 1,
            custom: Some("en pause".into()),
        },
        CoreMsg::Profile {
            display_name: "Anna".into(),
            bio: "salut".into(),
            avatar: Some([1; 32]),
            banner: None,
            pronouns: Some("il/lui".into()),
            accent_color: Some(0x00_FF_AA),
            banner_color: None,
            avatar_decoration: Some("neon_ring".into()),
            profile_effect: Some("aurora".into()),
            profile_frame: Some("crystal_edge".into()),
        },
        CoreMsg::VoiceSignal {
            group_id: [2; 16],
            channel_id: [3; 16],
            action: 0,
            media_kinds: 1,
            mute: false,
        },
        CoreMsg::GroupSync {
            group_id: [2; 16],
            max_lamport: 10,
            op_count: 4,
            digest: [7; 32],
        },
        CoreMsg::GroupSyncPull {
            group_id: [2; 16],
            since_lamport: 3,
        },
        CoreMsg::FriendRemove,
        CoreMsg::InviteTicket {
            group_id: [2; 16],
            invite_id: [9; 16],
            group_name: "Guilde".into(),
            inviter: [5; 32],
            secret: [11; 32],
            expires_ms: 123_456,
            sig: [6; 64],
        },
        CoreMsg::InviteAccept {
            group_id: [2; 16],
            invite_id: [9; 16],
            secret: [11; 32],
        },
        CoreMsg::InviteDecline {
            group_id: [2; 16],
            invite_id: [9; 16],
        },
    ];
    for m in msgs {
        roundtrip_channel(&ChannelMsg::Core(m));
    }
    // La signature couvre un encodage stable.
    let sb1 = op.signable_bytes();
    let sb2 = op.signable_bytes();
    assert_eq!(sb1, sb2);
    assert!(sb1.starts_with(b"accord-groupop-v1"));
}

/// `CoreMsg::Profile` sans AUCUN champ additif (pronoms, couleurs, décoration,
/// effet, cadre), tel qu'un émetteur antérieur à leur introduction l'aurait construit.
fn old_profile(
    display_name: &str,
    bio: &str,
    avatar: Option<[u8; 32]>,
    banner: Option<[u8; 32]>,
) -> CoreMsg {
    CoreMsg::Profile {
        display_name: display_name.into(),
        bio: bio.into(),
        avatar,
        banner,
        pronouns: None,
        accent_color: None,
        banner_color: None,
        avatar_decoration: None,
        profile_effect: None,
        profile_frame: None,
    }
}

#[test]
fn profile_name_bound_is_strict_at_decode() {
    // 128 octets UTF-8 passent le décodage (la validation sémantique
    // 2-32 caractères a lieu à l'ingestion, côté cœur).
    roundtrip_channel(&ChannelMsg::Core(old_profile(
        &"é".repeat(64), // 64 × 2 octets = 128 octets
        "",
        None,
        None,
    )));
    // 129 octets : rejet strict (anti-abus).
    let bytes = ChannelMsg::Core(old_profile(&"x".repeat(129), "", None, None)).to_bytes();
    assert_eq!(
        ChannelMsg::from_bytes(&bytes),
        Err(DecodeError::TooLarge("profile.name"))
    );
}

#[test]
fn profile_pronouns_and_color_bounds_are_strict_at_decode() {
    // 40 octets ASCII passent le décodage.
    roundtrip_channel(&ChannelMsg::Core(CoreMsg::Profile {
        display_name: "Anna".into(),
        bio: String::new(),
        avatar: None,
        banner: None,
        pronouns: Some("x".repeat(40)),
        accent_color: Some(0xFF_FF_FF),
        banner_color: Some(0),
        avatar_decoration: None,
        profile_effect: None,
        profile_frame: None,
    }));
    // 41 octets : rejet strict.
    let bytes = ChannelMsg::Core(CoreMsg::Profile {
        display_name: "Anna".into(),
        bio: String::new(),
        avatar: None,
        banner: None,
        pronouns: Some("x".repeat(41)),
        accent_color: None,
        banner_color: None,
        avatar_decoration: None,
        profile_effect: None,
        profile_frame: None,
    })
    .to_bytes();
    assert_eq!(
        ChannelMsg::from_bytes(&bytes),
        Err(DecodeError::TooLarge("profile.pronouns"))
    );
    // Couleur > 0xFFFFFF : rejet strict, pour l'accent comme pour la
    // bannière.
    let bytes = ChannelMsg::Core(CoreMsg::Profile {
        display_name: "Anna".into(),
        bio: String::new(),
        avatar: None,
        banner: None,
        pronouns: None,
        accent_color: Some(0x0100_0000),
        banner_color: None,
        avatar_decoration: None,
        profile_effect: None,
        profile_frame: None,
    })
    .to_bytes();
    assert_eq!(
        ChannelMsg::from_bytes(&bytes),
        Err(DecodeError::InvalidValue("profile.accent_color"))
    );
    let bytes = ChannelMsg::Core(CoreMsg::Profile {
        display_name: "Anna".into(),
        bio: String::new(),
        avatar: None,
        banner: None,
        pronouns: None,
        accent_color: None,
        banner_color: Some(0x0100_0000),
        avatar_decoration: None,
        profile_effect: None,
        profile_frame: None,
    })
    .to_bytes();
    assert_eq!(
        ChannelMsg::from_bytes(&bytes),
        Err(DecodeError::InvalidValue("profile.banner_color"))
    );
}

#[test]
fn profile_decodes_old_format_without_additive_fields_to_none() {
    // Vecteur figé : un message `Profile` tel qu'un émetteur antérieur à
    // l'introduction de pronoms/couleurs l'aurait construit ne porte
    // strictement AUCUN octet pour eux — pas même le tag `opt` — puisque son
    // code ne connaît pas ces champs. Notre encodeur actuel, lui, écrit
    // toujours un tag `opt` (0 ou 1) pour chaque champ, y compris `None`
    // (`Writer::put_opt`) : `old_profile(..).to_bytes()` n'est donc PAS un
    // vecteur d'ancien format, seulement un message moderne dont les champs
    // additifs valent `None`. Pour simuler fidèlement l'ancien
    // format, on retire les 6 octets de tag `None` finaux (un par champ
    // additif : pronoms, accent, bannière, décoration d'avatar, effet, cadre) de cet
    // encodage.
    let old = old_profile("Anna", "bio", Some([1; 32]), Some([2; 32]));
    let modern_bytes = old.to_bytes();
    let legacy_bytes = &modern_bytes[..modern_bytes.len() - 6];
    let decoded =
        CoreMsg::from_bytes(legacy_bytes).expect("un ancien message doit toujours décoder");
    assert_eq!(decoded, old);
    match decoded {
        CoreMsg::Profile {
            pronouns,
            accent_color,
            banner_color,
            ..
        } => {
            assert_eq!(pronouns, None);
            assert_eq!(accent_color, None);
            assert_eq!(banner_color, None);
        }
        other => panic!("variant inattendu : {other:?}"),
    }
    // Même vérification via le framing complet `ChannelMsg` (canal 0x02 +
    // `CoreMsg`) : seuls les 6 derniers octets diffèrent entre l'ancien et
    // le nouveau format.
    let modern_channel_bytes = ChannelMsg::Core(old.clone()).to_bytes();
    let legacy_channel_bytes = &modern_channel_bytes[..modern_channel_bytes.len() - 6];
    let decoded_channel = ChannelMsg::from_bytes(legacy_channel_bytes)
        .expect("un ancien message doit toujours décoder (framing complet)");
    assert_eq!(decoded_channel, ChannelMsg::Core(old));
}

#[test]
fn profile_decodes_previous_format_without_frame_to_none() {
    let expected = CoreMsg::Profile {
        display_name: "Anna".into(),
        bio: "bio".into(),
        avatar: None,
        banner: None,
        pronouns: Some("elle".into()),
        accent_color: Some(0x12_34_56),
        banner_color: Some(0x65_43_21),
        avatar_decoration: Some("neon_ring".into()),
        profile_effect: Some("aurora".into()),
        profile_frame: None,
    };
    let bytes = expected.to_bytes();
    let decoded = CoreMsg::from_bytes(&bytes[..bytes.len() - 1]).unwrap();
    assert_eq!(decoded, expected);
}

#[test]
fn profile_decoration_ids_roundtrip_and_reject_malformed_to_none() {
    // Ids bien formés (`[a-z0-9_-]`, ≤ 24 octets) : aller-retour fidèle.
    roundtrip_channel(&ChannelMsg::Core(CoreMsg::Profile {
        display_name: "Anna".into(),
        bio: String::new(),
        avatar: None,
        banner: None,
        pronouns: None,
        accent_color: None,
        banner_color: None,
        avatar_decoration: Some("neon_ring".into()),
        profile_effect: Some("falling_petals".into()),
        profile_frame: Some("crystal_edge".into()),
    }));

    // Ids malformés forgés par un pair malveillant : hors alphabet
    // (majuscules/espaces), trop longs (25 octets), non ASCII ou vides.
    // Chacun est réduit à `None` AU DÉCODAGE sans faire échouer le profil (les
    // autres champs survivent) — jamais de panique, jamais de rejet du message
    // entier (robustesse frontière de confiance P2P, `Reader::opt_tail_short_id`).
    for (deco, effect, frame) in [
        (
            Some("BAD ID!".to_string()),
            Some("UPPER".to_string()),
            Some("BAD FRAME!".to_string()),
        ),
        (
            Some("x".repeat(25)),
            Some("y".repeat(30)),
            Some("z".repeat(26)),
        ),
        (
            Some(String::new()),
            Some("\u{202E}bad".to_string()),
            Some(String::new()),
        ),
    ] {
        let bytes = ChannelMsg::Core(CoreMsg::Profile {
            display_name: "Anna".into(),
            bio: "bio".into(),
            avatar: None,
            banner: None,
            pronouns: None,
            accent_color: None,
            banner_color: None,
            avatar_decoration: deco,
            profile_effect: effect,
            profile_frame: frame,
        })
        .to_bytes();
        let decoded = ChannelMsg::from_bytes(&bytes).expect("profil décode malgré ids malformés");
        match decoded {
            ChannelMsg::Core(CoreMsg::Profile {
                display_name,
                bio,
                avatar_decoration,
                profile_effect,
                profile_frame,
                ..
            }) => {
                assert_eq!(display_name, "Anna");
                assert_eq!(bio, "bio");
                assert_eq!(avatar_decoration, None);
                assert_eq!(profile_effect, None);
                assert_eq!(profile_frame, None);
            }
            other => panic!("variant inattendu : {other:?}"),
        }
    }
}

#[test]
fn voice_and_relay_roundtrip() {
    roundtrip_channel(&ChannelMsg::Voice(VoiceMsg::AudioFrame {
        room: [1; 16],
        media_type: 1,
        seq: 65000,
        ts_ms: 123456,
        payload: vec![0x55; 160],
    }));
    roundtrip_channel(&ChannelMsg::Voice(VoiceMsg::VoicePing {
        loss_pct: 12,
        rtt_ms: 80,
    }));
    for m in [
        RelayMsg::Open { target: [1; 32] },
        RelayMsg::Accept { circuit: 9 },
        RelayMsg::Reject { code: 2 },
        RelayMsg::Data {
            circuit: 9,
            blob: vec![1; 100],
        },
        RelayMsg::Close { circuit: 9 },
    ] {
        roundtrip_channel(&ChannelMsg::Relay(m));
    }
}

#[test]
fn file_msgs_roundtrip() {
    let manifest = Manifest {
        merkle_root: [1; 32],
        size: 300_000,
        name: "video.mp4".into(),
        mime: "video/mp4".into(),
        leaf_hashes: vec![[2; 32], [3; 32]],
        publisher: [4; 32],
        sig: [5; 64],
    };
    for m in [
        FileMsg::GetManifest { root: [1; 32] },
        FileMsg::ManifestMsg {
            manifest: manifest.clone(),
        },
        FileMsg::GetBlock {
            root: [1; 32],
            index: 7,
        },
        FileMsg::Block {
            root: [1; 32],
            index: 7,
            data: vec![9; 1000],
        },
        FileMsg::Have {
            root: [1; 32],
            bitmap: vec![0xFF, 0x01],
        },
        FileMsg::NotFound {
            root: [1; 32],
            index: 8,
        },
    ] {
        roundtrip_channel(&ChannelMsg::File(m));
    }
    assert!(manifest.signable_bytes().starts_with(b"accord-manifest-v1"));
}

#[test]
fn manifest_leaf_count_must_match_size() {
    let bad = Manifest {
        merkle_root: [1; 32],
        size: 300_000, // 2 blocs attendus
        name: "f".into(),
        mime: "m".into(),
        leaf_hashes: vec![[2; 32]; 3],
        publisher: [4; 32],
        sig: [5; 64],
    };
    let bytes = bad.to_bytes();
    assert_eq!(
        Manifest::from_bytes(&bytes),
        Err(DecodeError::InvalidValue("manifest.leaf_count"))
    );
}

#[test]
fn tcp_framing_roundtrip_and_bounds() {
    let payload = vec![7u8; 500];
    let framed = tcp_frame(&payload).unwrap();
    let (got, consumed) = tcp_deframe(&framed).unwrap().unwrap();
    assert_eq!(got, payload);
    assert_eq!(consumed, framed.len());
    // Incomplet.
    assert_eq!(tcp_deframe(&framed[..3]).unwrap(), None);
    assert_eq!(tcp_deframe(&framed[..100]).unwrap(), None);
    // Trop grand.
    let huge = (limits::MAX_TCP_FRAME as u32 + 1).to_be_bytes();
    assert!(tcp_deframe(&huge).is_err());
}

#[test]
fn node_id_bucket_index() {
    let a = NodeId([0; 32]);
    let mut b = [0u8; 32];
    b[0] = 0x80;
    assert_eq!(a.bucket_index(&NodeId(b)), Some(255));
    let mut c = [0u8; 32];
    c[31] = 0x01;
    assert_eq!(a.bucket_index(&NodeId(c)), Some(0));
    assert_eq!(a.bucket_index(&a), None);
    let d = a.distance(&NodeId(b));
    assert_eq!(d[0], 0x80);
}

#[test]
fn random_garbage_never_panics() {
    use rand::{Rng, SeedableRng};
    let mut rng = rand::rngs::StdRng::seed_from_u64(42);
    for _ in 0..5000 {
        let len = rng.gen_range(0..600);
        let buf: Vec<u8> = (0..len).map(|_| rng.gen()).collect();
        let _ = Packet::from_bytes(&buf);
        let _ = ChannelMsg::from_bytes(&buf);
        let _ = tcp_deframe(&buf);
    }
}

#[test]
fn truncation_fuzz_channel_msgs() {
    // Toute troncature d'un message valide doit être rejetée proprement,
    // sauf les coupes qui tombent exactement à la frontière d'un champ
    // additif de fin de variant (pronoms, puis couleur d'accent, puis
    // couleur de bannière) : ces coupes précises sont indiscernables d'un
    // message légitime dans lequel les champs suivants seraient absents
    // (émetteur antérieur à leur introduction, ou une annonce n'en portant
    // qu'une partie) et doivent donc décoder avec succès — c'est la
    // rétrocompatibilité filaire voulue (voir `Reader::opt_tail`).
    let m = ChannelMsg::Core(CoreMsg::Profile {
        display_name: "Anna".into(),
        bio: "bio".into(),
        avatar: Some([1; 32]),
        banner: Some([2; 32]),
        pronouns: Some("il/lui".into()),
        accent_color: Some(0x00_FF_AA),
        banner_color: Some(0x11_22_33),
        avatar_decoration: Some("neon_ring".into()),
        profile_effect: Some("aurora".into()),
        profile_frame: Some("crystal_edge".into()),
    });
    let bytes = m.to_bytes();
    // Chaque frontière est calculée en encodant un message au préfixe
    // identique (mêmes champs avant elle), puis en retirant les octets de
    // tag `None` que notre encodeur écrit toujours pour les champs additifs
    // restants (`Writer::put_opt` écrit 1 octet même pour `None` — voir
    // `profile_decodes_old_format_without_additive_fields_to_none`) :
    // l'encodage étant strictement préfixe, le résultat coïncide
    // octet-à-octet avec le début de `bytes` jusqu'à cette frontière. Il y a
    // désormais 6 champs additifs en fin de variant (pronoms, accent,
    // bannière, décoration d'avatar, effet de profil, cadre de profil).
    let after_banner = ChannelMsg::Core(old_profile("Anna", "bio", Some([1; 32]), Some([2; 32])))
        .to_bytes()
        .len()
        - 6; // 6 champs additifs encore à `None`.
    let after_pronouns = ChannelMsg::Core(CoreMsg::Profile {
        display_name: "Anna".into(),
        bio: "bio".into(),
        avatar: Some([1; 32]),
        banner: Some([2; 32]),
        pronouns: Some("il/lui".into()),
        accent_color: None,
        banner_color: None,
        avatar_decoration: None,
        profile_effect: None,
        profile_frame: None,
    })
    .to_bytes()
    .len()
        - 5; // 5 champs additifs encore à `None`.
    let after_accent_color = ChannelMsg::Core(CoreMsg::Profile {
        display_name: "Anna".into(),
        bio: "bio".into(),
        avatar: Some([1; 32]),
        banner: Some([2; 32]),
        pronouns: Some("il/lui".into()),
        accent_color: Some(0x00_FF_AA),
        banner_color: None,
        avatar_decoration: None,
        profile_effect: None,
        profile_frame: None,
    })
    .to_bytes()
    .len()
        - 4; // 4 champs additifs encore à `None`.
    let after_banner_color = ChannelMsg::Core(CoreMsg::Profile {
        display_name: "Anna".into(),
        bio: "bio".into(),
        avatar: Some([1; 32]),
        banner: Some([2; 32]),
        pronouns: Some("il/lui".into()),
        accent_color: Some(0x00_FF_AA),
        banner_color: Some(0x11_22_33),
        avatar_decoration: None,
        profile_effect: None,
        profile_frame: None,
    })
    .to_bytes()
    .len()
        - 3; // 3 champs additifs encore à `None` (décoration, effet, cadre).
    let after_avatar_decoration = ChannelMsg::Core(CoreMsg::Profile {
        display_name: "Anna".into(),
        bio: "bio".into(),
        avatar: Some([1; 32]),
        banner: Some([2; 32]),
        pronouns: Some("il/lui".into()),
        accent_color: Some(0x00_FF_AA),
        banner_color: Some(0x11_22_33),
        avatar_decoration: Some("neon_ring".into()),
        profile_effect: None,
        profile_frame: None,
    })
    .to_bytes()
    .len()
        - 2; // 2 champs additifs encore à `None` (effet et cadre de profil).
    let after_profile_effect = ChannelMsg::Core(CoreMsg::Profile {
        display_name: "Anna".into(),
        bio: "bio".into(),
        avatar: Some([1; 32]),
        banner: Some([2; 32]),
        pronouns: Some("il/lui".into()),
        accent_color: Some(0x00_FF_AA),
        banner_color: Some(0x11_22_33),
        avatar_decoration: Some("neon_ring".into()),
        profile_effect: Some("aurora".into()),
        profile_frame: None,
    })
    .to_bytes()
    .len()
        - 1;
    let valid_prefixes = [
        after_banner,
        after_pronouns,
        after_accent_color,
        after_banner_color,
        after_avatar_decoration,
        after_profile_effect,
    ];
    for cut in 0..bytes.len() {
        if valid_prefixes.contains(&cut) {
            assert!(ChannelMsg::from_bytes(&bytes[..cut]).is_ok());
            continue;
        }
        assert!(ChannelMsg::from_bytes(&bytes[..cut]).is_err());
    }
}

#[test]
fn invite_ticket_truncation_never_panics_and_is_rejected() {
    // Le message d'invitation est le plus riche des nouveaux variants
    // (chaîne + plusieurs tableaux fixes + borne sur `expires_ms`) : chaque
    // troncature doit être rejetée proprement, jamais paniquer.
    let m = ChannelMsg::Core(CoreMsg::InviteTicket {
        group_id: [1; 16],
        invite_id: [2; 16],
        group_name: "Guilde des porteurs d'octets".into(),
        inviter: [3; 32],
        secret: [4; 32],
        expires_ms: 1_700_000_000_000,
        sig: [5; 64],
    });
    let bytes = m.to_bytes();
    for cut in 0..bytes.len() {
        assert!(ChannelMsg::from_bytes(&bytes[..cut]).is_err());
    }
    assert_eq!(ChannelMsg::from_bytes(&bytes).unwrap(), m);
}
