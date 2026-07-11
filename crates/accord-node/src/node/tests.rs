//! Tests du nœud : profil, périphériques voix, amis, messagerie et groupes.

use super::*;

fn node() -> Node {
    let id = Identity::generate_with_pow_bits(1);
    let db = Db::open_in_memory(&[1u8; 32]).unwrap();
    Node::new(id, db, OutboundSink::null())
}

#[test]
fn self_profile_is_consistent() {
    let n = node();
    let p = n.self_profile().unwrap();
    assert_eq!(p.pubkey.len(), 64);
    assert_eq!(p.node_id.len(), 64);
    assert!(p.friend_code.contains('-'));
    assert_eq!(p.name, None);
}

#[test]
fn profile_name_set_get_and_validation() {
    let n = node();
    assert_eq!(n.profile_name().unwrap(), None);
    n.profile_set_name("  Anna  ").unwrap();
    assert_eq!(n.profile_name().unwrap(), Some("Anna".into()));
    assert_eq!(n.self_profile().unwrap().name, Some("Anna".into()));
    // Bornes : erreur explicite, pseudo précédent conservé.
    assert!(n.profile_set_name("x").is_err());
    assert!(n.profile_set_name(&"x".repeat(33)).is_err());
    assert_eq!(n.profile_name().unwrap(), Some("Anna".into()));
}

#[test]
fn voice_devices_config_persists_and_resets() {
    let n = node();
    // Jamais défini : périphériques par défaut des deux côtés.
    assert_eq!(n.voice_devices_config().unwrap(), (None, None));

    // Champ absent = inchangé ; chaque côté se règle indépendamment.
    n.set_voice_devices_config(Some(Some("Micro USB")), None)
        .unwrap();
    assert_eq!(
        n.voice_devices_config().unwrap(),
        (Some("Micro USB".into()), None)
    );
    n.set_voice_devices_config(None, Some(Some("Casque")))
        .unwrap();
    assert_eq!(
        n.voice_devices_config().unwrap(),
        (Some("Micro USB".into()), Some("Casque".into()))
    );

    // `Some(None)` = retour au périphérique par défaut.
    n.set_voice_devices_config(Some(None), None).unwrap();
    assert_eq!(
        n.voice_devices_config().unwrap(),
        (None, Some("Casque".into()))
    );
}

#[test]
fn voice_device_names_are_validated_without_effect() {
    let n = node();
    for bad in ["", &"x".repeat(257) as &str, "nom\u{0007}sonore"] {
        assert!(n.set_voice_devices_config(Some(Some(bad)), None).is_err());
    }
    // Validation avant écriture : une sortie valide accompagnée d'une
    // entrée invalide n'est pas persistée.
    assert!(n
        .set_voice_devices_config(Some(Some("")), Some(Some("Casque")))
        .is_err());
    assert_eq!(n.voice_devices_config().unwrap(), (None, None));
}

/// Nœud avec un ami établi (demande sortante acceptée par le pair).
fn node_with_friend() -> (Node, Identity) {
    let n = node();
    let peer = Identity::generate_with_pow_bits(1);
    n.friend_request(&peer.public_key(), "Pair").unwrap();
    n.ingest_core(
        &peer.public_key(),
        CoreMsg::FriendResponse { accepted: true },
    )
    .unwrap();
    (n, peer)
}

#[test]
fn profile_from_friend_updates_contact_name() {
    let (n, peer) = node_with_friend();
    let replies = n
        .ingest_core(
            &peer.public_key(),
            CoreMsg::Profile {
                display_name: " Pair Renommé ".into(),
                bio: String::new(),
                avatar: None,
                banner: None,
            },
        )
        .unwrap();
    assert!(replies.is_empty());
    let contacts = n.contacts().unwrap();
    let contact = contacts
        .iter()
        .find(|c| c.pubkey == peer.public_key())
        .unwrap();
    assert_eq!(contact.display_name, "Pair Renommé");
}

#[test]
fn profile_from_non_friend_is_ignored() {
    let n = node();
    let stranger = Identity::generate_with_pow_bits(1);
    let replies = n
        .ingest_core(
            &stranger.public_key(),
            CoreMsg::Profile {
                display_name: "Imposteur".into(),
                bio: String::new(),
                avatar: None,
                banner: None,
            },
        )
        .unwrap();
    assert!(replies.is_empty());
    assert!(n.contacts().unwrap().is_empty());
}

#[test]
fn invalid_profile_from_friend_is_rejected_without_effect() {
    let (n, peer) = node_with_friend();
    assert!(n
        .ingest_core(
            &peer.public_key(),
            CoreMsg::Profile {
                display_name: "x".into(),
                bio: String::new(),
                avatar: None,
                banner: None,
            },
        )
        .is_err());
    let contacts = n.contacts().unwrap();
    assert_eq!(contacts[0].display_name, "Pair");
}

#[test]
fn friendship_acceptance_replies_announce_profile() {
    // Réponse d'acceptation reçue : le demandeur annonce son pseudo.
    let n = node();
    n.profile_set_name("Anna").unwrap();
    let peer = Identity::generate_with_pow_bits(1);
    n.friend_request(&peer.public_key(), "Pair").unwrap();
    let replies = n
        .ingest_core(
            &peer.public_key(),
            CoreMsg::FriendResponse { accepted: true },
        )
        .unwrap();
    assert_eq!(
        replies,
        vec![CoreMsg::Profile {
            display_name: "Anna".into(),
            bio: String::new(),
            avatar: None,
            banner: None,
        }]
    );

    // Demandes croisées : l'accepteur automatique annonce aussi le sien.
    let n2 = node();
    n2.profile_set_name("Bertrand").unwrap();
    let peer2 = Identity::generate_with_pow_bits(1);
    n2.friend_request(&peer2.public_key(), "Pair").unwrap();
    let replies = n2
        .ingest_core(
            &peer2.public_key(),
            CoreMsg::FriendRequest {
                display_name: "Pair".into(),
                message: String::new(),
                verify_phrase: None,
            },
        )
        .unwrap();
    assert_eq!(replies.len(), 2);
    assert_eq!(replies[0], CoreMsg::FriendResponse { accepted: true });
    assert!(matches!(
        &replies[1],
        CoreMsg::Profile { display_name, .. } if display_name == "Bertrand"
    ));
}

#[test]
fn acceptance_without_local_name_announces_nothing() {
    let n = node();
    let peer = Identity::generate_with_pow_bits(1);
    n.friend_request(&peer.public_key(), "Pair").unwrap();
    let replies = n
        .ingest_core(
            &peer.public_key(),
            CoreMsg::FriendResponse { accepted: true },
        )
        .unwrap();
    assert!(replies.is_empty());
}

#[test]
fn friend_and_dm_flow_persists() {
    let n = node();
    let peer = Identity::generate_with_pow_bits(1);
    n.friend_request(&peer.public_key(), "Pair").unwrap();
    // Le pair accepte (réponse ingérée par la boucle réseau, simulée ici).
    n.with_db(|db| {
        Ok(friends::ingest_friend_response(
            db,
            &peer.public_key(),
            true,
            now_ms(),
        )?)
    })
    .unwrap();

    let id = n
        .dm_send(&peer.public_key(), "bonjour recherche", None)
        .unwrap();
    assert_eq!(id.len(), 32);
    let hist = n.dm_history(&peer.public_key(), u64::MAX, 10).unwrap();
    assert_eq!(hist.len(), 1);
    assert_eq!(n.search("recherche").unwrap(), vec![id]);
}

#[test]
fn search_filters_resolve_author_conversation_and_attachments() {
    use accord_proto::core_msg::FileRef;
    let n = node();
    let peer = Identity::generate_with_pow_bits(1);
    n.friend_request(&peer.public_key(), "Alice").unwrap();
    n.with_db(|db| {
        Ok(friends::ingest_friend_response(
            db,
            &peer.public_key(),
            true,
            now_ms(),
        )?)
    })
    .unwrap();
    n.dm_send(&peer.public_key(), "chat photo souvenir", None)
        .unwrap();
    let img = FileRef {
        merkle_root: [1; 32],
        name: "p.png".into(),
        size: 10,
        mime: "image/png".into(),
    };
    n.dm_send_with_attachments(&peer.public_key(), "album vacances", None, vec![img])
        .unwrap();

    // Plain word search returns metadata hits (conversation ref + author).
    let hits = n.search_filtered("photo").unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0]["conversation"]["type"], "dm");
    assert!(hits[0]["msg_id"].is_string() && hits[0]["timestamp"].is_number());

    // has:image narrows to attachment-bearing messages even with no word.
    assert_eq!(n.search_filtered("has:image").unwrap().len(), 1);

    // from: resolves against contacts: Alice authored none of OUR messages.
    assert!(n.search_filtered("from:alice photo").unwrap().is_empty());
    // from:me matches our own authored messages.
    assert_eq!(n.search_filtered("from:me photo").unwrap().len(), 1);
    // in: resolves the DM conversation by contact name.
    assert_eq!(n.search_filtered("in:alice souvenir").unwrap().len(), 1);
    assert!(n.search_filtered("in:inconnu souvenir").unwrap().is_empty());
}

#[test]
fn group_lifecycle() {
    let n = node();
    let gid_hex = n.group_create("Ma Guilde").unwrap();
    let gid = hex::decode::<16>(&gid_hex).unwrap();
    assert_eq!(n.group_ids().unwrap(), vec![gid_hex.clone()]);
    let chan_hex = n.group_add_channel(&gid, "général").unwrap();
    let chan = hex::decode::<16>(&chan_hex).unwrap();
    let state = n.group_state(&gid).unwrap();
    assert_eq!(state.name, "Ma Guilde");
    assert!(state.channels.contains_key(&chan));
}

// ---- Présence, frappe et marques de lecture ----

#[test]
fn friend_activity_marks_online_and_presence_offline_resets() {
    let (n, peer) = node_with_friend();
    let pk = peer.public_key();
    // Une annonce hors-ligne (arrêt propre) repasse le pair hors ligne.
    n.ingest_core(
        &pk,
        CoreMsg::Presence {
            status: 3,
            custom: None,
        },
    )
    .unwrap();
    assert!(!n.is_online(&pk));
    // Une annonce en ligne le remarque en ligne.
    n.ingest_core(
        &pk,
        CoreMsg::Presence {
            status: 0,
            custom: None,
        },
    )
    .unwrap();
    assert!(n.is_online(&pk));
    // Repasse hors ligne, puis tout message ordinaire d'un ami atteste aussi
    // sa présence (premier message = en ligne).
    n.ingest_core(
        &pk,
        CoreMsg::Presence {
            status: 3,
            custom: None,
        },
    )
    .unwrap();
    assert!(!n.is_online(&pk));
    let body = accord_proto::core_msg::MsgBody::Typing.encode_body();
    n.ingest_core(
        &pk,
        CoreMsg::DirectMsg {
            msg_id: [1; 16],
            lamport: 1,
            sent_ms: 0,
            kind: 5,
            body,
        },
    )
    .unwrap();
    assert!(n.is_online(&pk));
}

#[test]
fn presence_from_non_friend_creates_no_relationship() {
    let n = node();
    let stranger = Identity::generate_with_pow_bits(1);
    n.ingest_core(
        &stranger.public_key(),
        CoreMsg::Presence {
            status: 0,
            custom: None,
        },
    )
    .unwrap();
    // Anti-abus : aucune relation créée ; la présence n'est exposée qu'entre
    // amis (aucun `event.presence` n'est émis pour un inconnu).
    assert!(n.contacts().unwrap().is_empty());
}

/// Nœud avec un ami établi (amitié posée directement en base, donc le pair
/// démarre hors ligne) et canal sortant capturé, drainé avant de rendre la
/// main.
fn node_with_friend_and_channel() -> (
    Node,
    Identity,
    tokio::sync::mpsc::Receiver<crate::outbound::Outbound>,
) {
    let id = Identity::generate_with_pow_bits(1);
    let db = Db::open_in_memory(&[1u8; 32]).unwrap();
    let (sink, mut rx) = OutboundSink::channel(64);
    let n = Node::new(id, db, sink);
    let peer = Identity::generate_with_pow_bits(1);
    n.friend_request(&peer.public_key(), "Pair").unwrap();
    n.with_db(|db| {
        Ok(friends::ingest_friend_response(
            db,
            &peer.public_key(),
            true,
            now_ms(),
        )?)
    })
    .unwrap();
    while rx.try_recv().is_ok() {}
    (n, peer, rx)
}

#[test]
fn dm_typing_is_ephemeral_and_gated_on_online() {
    let (n, peer, mut rx) = node_with_friend_and_channel();
    // Pair hors ligne : aucune émission (pas d'outbox, silence).
    n.dm_typing(&peer.public_key()).unwrap();
    assert!(
        rx.try_recv().is_err(),
        "aucune frappe vers un pair hors ligne"
    );
    // Le pair devient joignable, puis la frappe part comme DirectMsg kind 5.
    n.ingest_core(
        &peer.public_key(),
        CoreMsg::Presence {
            status: 0,
            custom: None,
        },
    )
    .unwrap();
    n.dm_typing(&peer.public_key()).unwrap();
    match rx.try_recv() {
        Ok(crate::outbound::Outbound::Core { to, msg }) => {
            assert_eq!(to, peer.public_key());
            assert!(matches!(*msg, CoreMsg::DirectMsg { kind, .. } if kind == 5));
        }
        other => panic!("attendu une frappe DirectMsg, obtenu {other:?}"),
    }
    // Aucune persistance : l'historique reste vide.
    assert!(n
        .dm_history(&peer.public_key(), u64::MAX, 10)
        .unwrap()
        .is_empty());
}

#[test]
fn dm_unread_counts_peer_messages_until_mark_read() {
    let (n, peer) = node_with_friend();
    let peer_pub = peer.public_key();
    // Deux messages reçus du pair.
    for (i, id) in [1u8, 2u8].into_iter().enumerate() {
        let body = accord_proto::core_msg::MsgBody::Text {
            text: format!("m{id}"),
            reply_to: None,
            attachments: vec![],
        };
        n.ingest_core(
            &peer_pub,
            CoreMsg::DirectMsg {
                msg_id: [id; 16],
                lamport: (i as u64) + 1,
                sent_ms: 0,
                kind: body.kind(),
                body: body.encode_body(),
            },
        )
        .unwrap();
    }
    assert_eq!(n.dm_unread(&peer_pub).unwrap(), 2);
    // Marque tout lu : plus aucun non-lu.
    n.dm_mark_read(&peer_pub, u64::MAX).unwrap();
    assert_eq!(n.dm_unread(&peer_pub).unwrap(), 0);
}

// ---- Réplication de groupe entre deux nœuds (canal sortant capturé) ----

/// Nœud dont les actions réseau sont capturées dans un récepteur.
fn node_with_channel() -> (Node, tokio::sync::mpsc::Receiver<crate::outbound::Outbound>) {
    let id = Identity::generate_with_pow_bits(1);
    let db = Db::open_in_memory(&[1u8; 32]).unwrap();
    let (sink, rx) = OutboundSink::channel(256);
    (Node::new(id, db, sink), rx)
}

/// Vide le canal sortant de `from` et livre chaque message à `to` comme si
/// le réseau l'avait transporté (l'émetteur est `from_pub`). Rend aussi les
/// réponses produites par l'ingestion.
fn deliver(
    rx: &mut tokio::sync::mpsc::Receiver<crate::outbound::Outbound>,
    from_pub: &[u8; 32],
    to: &Node,
    to_pub: &[u8; 32],
) {
    use crate::outbound::Outbound;
    while let Ok(action) = rx.try_recv() {
        match action {
            Outbound::GroupOp { op } => {
                to.ingest_core(from_pub, CoreMsg::GroupOpMsg { op: *op })
                    .unwrap();
            }
            Outbound::GroupCast { msg, .. } => {
                to.ingest_core(from_pub, *msg).unwrap();
            }
            Outbound::Core { to: dest, msg } => {
                if dest == *to_pub {
                    to.ingest_core(from_pub, *msg).unwrap();
                }
            }
            Outbound::DhtPublish { .. } => {}
        }
    }
}

/// Fait rejoindre `bob` à un groupe déjà créé par `alice`, comme un pair
/// réseau le ferait réellement : ticket signé -> acceptation explicite ->
/// op-log complet + clé (consentement en deux temps, D-045). Remplace
/// l'ancien `alice.group_invite(...)` à un coup, qui forçait l'adhésion.
fn invite_and_join(
    alice: &Node,
    rx_a: &mut tokio::sync::mpsc::Receiver<crate::outbound::Outbound>,
    alice_pub: &[u8; 32],
    bob: &Node,
    rx_b: &mut tokio::sync::mpsc::Receiver<crate::outbound::Outbound>,
    bob_pub: &[u8; 32],
    gid: &[u8; 16],
) {
    let invite_id = hex::decode::<16>(&alice.group_invite_create(gid, bob_pub).unwrap()).unwrap();
    // Le ticket (et l'op InviteCreate, ignorée par Bob tant qu'il n'a pas
    // consenti) traverse le « réseau ».
    deliver(rx_a, alice_pub, bob, bob_pub);
    assert!(
        bob.group_invites_list()
            .unwrap()
            .iter()
            .any(|i| i.invite_id == invite_id),
        "Bob doit avoir reçu le ticket avant de pouvoir l'accepter"
    );
    bob.group_invite_accept(gid, &invite_id).unwrap();
    // L'acceptation revient à Alice, qui pousse l'op-log complet et la clé.
    deliver(rx_b, bob_pub, alice, alice_pub);
    deliver(rx_a, alice_pub, bob, bob_pub);
}

#[test]
fn group_replication_hierarchy_and_moderation_between_nodes() {
    let (alice, mut rx_a) = node_with_channel();
    let (bob, mut rx_b) = node_with_channel();
    let alice_pub = alice.public_key();
    let bob_pub = bob.public_key();

    // Alice monte le serveur et invite Bob ; tout transite par le « réseau ».
    let gid = hex::decode::<16>(&alice.group_create("Guilde").unwrap()).unwrap();
    let chan = hex::decode::<16>(&alice.group_add_channel(&gid, "général").unwrap()).unwrap();
    invite_and_join(
        &alice, &mut rx_a, &alice_pub, &bob, &mut rx_b, &bob_pub, &gid,
    );

    // Bob a matérialisé l'état et reçu la clé : il peut écrire.
    let state_bob = bob.group_state(&gid).unwrap();
    assert!(state_bob.is_member(&bob_pub));
    let mid = hex::decode::<16>(&bob.group_send(&gid, &chan, "salut la guilde").unwrap()).unwrap();
    deliver(&mut rx_b, &bob_pub, &alice, &alice_pub);
    let hist_a = alice.group_history(&gid, &chan, u64::MAX, 10).unwrap();
    assert_eq!(hist_a.len(), 1);
    assert_eq!(hist_a[0].author, bob_pub);

    // Hiérarchie : Bob, simple membre, ne peut ni expulser ni bannir Alice,
    // ni éditer son état de serveur.
    assert!(bob.group_kick(&gid, &alice_pub).is_err());
    assert!(bob.group_ban(&gid, &alice_pub).is_err());
    assert!(bob.group_rename(&gid, "Piratée").is_err());
    // Et Alice ne peut pas éditer le message de Bob (auteur seul).
    assert!(alice.group_edit_msg(&gid, &chan, &mid, "réécrit").is_err());

    // Modération : Alice (fondatrice, MANAGE_MESSAGES) supprime le message
    // de Bob via l'op-log signée ; le tombstone se réplique chez Bob.
    alice.group_delete_msg(&gid, &chan, &mid).unwrap();
    let hist_a = alice.group_history(&gid, &chan, u64::MAX, 10).unwrap();
    assert!(hist_a[0].deleted && hist_a[0].body.is_empty());
    deliver(&mut rx_a, &alice_pub, &bob, &bob_pub);
    let hist_b = bob.group_history(&gid, &chan, u64::MAX, 10).unwrap();
    assert!(hist_b[0].deleted, "tombstone de modération répliqué");

    // Bob quitte le groupe ; Alice l'apprend par l'op répliquée.
    bob.group_leave(&gid).unwrap();
    deliver(&mut rx_b, &bob_pub, &alice, &alice_pub);
    assert!(!alice.group_state(&gid).unwrap().is_member(&bob_pub));
}

#[test]
fn group_edit_and_reaction_replicate_between_nodes() {
    let (alice, mut rx_a) = node_with_channel();
    let (bob, mut rx_b) = node_with_channel();
    let alice_pub = alice.public_key();
    let bob_pub = bob.public_key();

    let gid = hex::decode::<16>(&alice.group_create("Guilde").unwrap()).unwrap();
    let chan = hex::decode::<16>(&alice.group_add_channel(&gid, "général").unwrap()).unwrap();
    invite_and_join(
        &alice, &mut rx_a, &alice_pub, &bob, &mut rx_b, &bob_pub, &gid,
    );

    // Alice écrit ; Bob reçoit, réagit, Alice voit la réaction.
    let mid = hex::decode::<16>(&alice.group_send(&gid, &chan, "v1").unwrap()).unwrap();
    deliver(&mut rx_a, &alice_pub, &bob, &bob_pub);
    bob.group_react(&gid, &chan, &mid, "👍", true).unwrap();
    deliver(&mut rx_b, &bob_pub, &alice, &alice_pub);
    assert_eq!(
        alice.reactions_of(&mid).unwrap(),
        vec![("👍".to_string(), bob_pub)]
    );

    // Alice édite son message ; l'édition se réplique chez Bob.
    alice.group_edit_msg(&gid, &chan, &mid, "v2").unwrap();
    deliver(&mut rx_a, &alice_pub, &bob, &bob_pub);
    let hist_b = bob.group_history(&gid, &chan, u64::MAX, 10).unwrap();
    assert_eq!(hist_b[0].edited.as_deref(), Some(b"v2".as_slice()));
}

/// Nœud adossé à une base sur disque (le magasin de fichiers, requis par les
/// émojis, exige un profil réel) avec canal sortant capturé.
fn node_on_disk_with_channel() -> (
    Node,
    tempfile::TempDir,
    tokio::sync::mpsc::Receiver<crate::outbound::Outbound>,
) {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("accord.db"), &[1u8; 32]).unwrap();
    let id = Identity::generate_with_pow_bits(1);
    let (sink, rx) = OutboundSink::channel(256);
    (Node::new(id, db, sink), dir, rx)
}

#[test]
fn group_emojis_replicate_between_nodes() {
    let (alice, _dir_a, mut rx_a) = node_on_disk_with_channel();
    let (bob, _dir_b, mut rx_b) = node_on_disk_with_channel();
    let alice_pub = alice.public_key();
    let bob_pub = bob.public_key();

    let gid = hex::decode::<16>(&alice.group_create("Guilde").unwrap()).unwrap();
    invite_and_join(
        &alice, &mut rx_a, &alice_pub, &bob, &mut rx_b, &bob_pub, &gid,
    );

    // Alice ajoute un émoji : l'image est publiée localement et l'op AddEmoji
    // est diffusée à Bob.
    let root = alice
        .group_emoji_add(&gid, "parrot", "image/png", vec![1, 2, 3, 4])
        .unwrap();
    deliver(&mut rx_a, &alice_pub, &bob, &bob_pub);
    let st = bob.group_state(&gid).unwrap();
    assert_eq!(st.emojis.get("parrot").map(|h| hex::encode(h)), Some(root));

    // Bob, simple membre, ne peut pas gérer les émojis.
    assert!(bob
        .group_emoji_add(&gid, "boom", "image/png", vec![9])
        .is_err());

    // Suppression par Alice répliquée chez Bob.
    alice.group_emoji_del(&gid, "parrot").unwrap();
    deliver(&mut rx_a, &alice_pub, &bob, &bob_pub);
    assert!(bob.group_state(&gid).unwrap().emojis.is_empty());

    // Le canal résiduel de Bob n'est pas exploité dans ce scénario.
    let _ = &mut rx_b;
}

#[test]
fn group_unread_tracks_others_messages_until_mark_read() {
    let (alice, mut rx_a) = node_with_channel();
    let (bob, mut rx_b) = node_with_channel();
    let alice_pub = alice.public_key();
    let bob_pub = bob.public_key();

    let gid = hex::decode::<16>(&alice.group_create("Guilde").unwrap()).unwrap();
    let chan = hex::decode::<16>(&alice.group_add_channel(&gid, "général").unwrap()).unwrap();
    invite_and_join(
        &alice, &mut rx_a, &alice_pub, &bob, &mut rx_b, &bob_pub, &gid,
    );

    // Bob écrit deux messages ; Alice les reçoit.
    bob.group_send(&gid, &chan, "un").unwrap();
    bob.group_send(&gid, &chan, "deux").unwrap();
    deliver(&mut rx_b, &bob_pub, &alice, &alice_pub);

    // Côté Alice : deux non-lus dans ce salon ; ses propres messages ne
    // comptent pas.
    alice.group_send(&gid, &chan, "moi").unwrap();
    let unread = alice.group_unread(&gid).unwrap();
    assert_eq!(unread, vec![(chan, 2)]);

    // Après marque de lecture, plus aucun non-lu.
    alice.group_mark_read(&gid, &chan, u64::MAX).unwrap();
    assert!(alice.group_unread(&gid).unwrap().is_empty());
}

#[test]
fn group_typing_reaches_only_online_members() {
    let (alice, mut rx_a) = node_with_channel();
    let (bob, mut rx_b) = node_with_channel();
    let alice_pub = alice.public_key();
    let bob_pub = bob.public_key();

    let gid = hex::decode::<16>(&alice.group_create("Guilde").unwrap()).unwrap();
    let chan = hex::decode::<16>(&alice.group_add_channel(&gid, "général").unwrap()).unwrap();
    invite_and_join(
        &alice, &mut rx_a, &alice_pub, &bob, &mut rx_b, &bob_pub, &gid,
    );
    while rx_a.try_recv().is_ok() {}
    // Le consentement en deux temps (D-045) fait transiter `InviteAccept` de
    // Bob vers Alice : elle le sait donc déjà joignable à ce stade. On
    // réinitialise pour isoler le scénario « Bob hors ligne » qui suit.
    alice.set_presence(&bob_pub, false);

    // Bob hors ligne du point de vue d'Alice : la frappe ne part vers personne.
    alice.group_typing(&gid, &chan).unwrap();
    assert!(rx_a.try_recv().is_err());

    // Bob devient joignable (un message reçu de lui) : la frappe lui parvient
    // sans être persistée chez lui.
    bob.group_send(&gid, &chan, "coucou").unwrap();
    deliver(&mut rx_b, &bob_pub, &alice, &alice_pub);
    assert!(alice.is_online(&bob_pub));
    alice.group_typing(&gid, &chan).unwrap();
    match rx_a.try_recv() {
        Ok(crate::outbound::Outbound::Core { to, msg }) => {
            assert_eq!(to, bob_pub);
            assert!(matches!(*msg, CoreMsg::GroupMsg { .. }));
            // Livrée à Bob : signalée comme frappe, jamais stockée.
            let before = bob.group_history(&gid, &chan, u64::MAX, 10).unwrap().len();
            bob.ingest_core(&alice_pub, *msg).unwrap();
            let after = bob.group_history(&gid, &chan, u64::MAX, 10).unwrap().len();
            assert_eq!(before, after, "la frappe de salon n'est pas persistée");
        }
        other => panic!("attendu une frappe de salon, obtenu {other:?}"),
    }
}

#[test]
fn group_mention_records_inbox_entry_and_purges_on_delete() {
    let (alice, mut rx_a) = node_with_channel();
    let (bob, mut rx_b) = node_with_channel();
    let alice_pub = alice.public_key();
    let bob_pub = bob.public_key();

    let gid = hex::decode::<16>(&alice.group_create("Guilde").unwrap()).unwrap();
    let chan = hex::decode::<16>(&alice.group_add_channel(&gid, "général").unwrap()).unwrap();
    invite_and_join(
        &alice, &mut rx_a, &alice_pub, &bob, &mut rx_b, &bob_pub, &gid,
    );

    // Bob mentionne tout le monde ; Alice ingère et enregistre une entrée.
    let mid =
        hex::decode::<16>(&bob.group_send(&gid, &chan, "réunion @everyone").unwrap()).unwrap();
    deliver(&mut rx_b, &bob_pub, &alice, &alice_pub);
    assert!(alice.msg_mentions_me(&mid).unwrap());
    assert_eq!(alice.group_mention_count(&gid).unwrap(), 1);
    let inbox = alice.mention_inbox(None, 50).unwrap();
    assert_eq!(inbox.len(), 1);
    assert_eq!(inbox[0].msg_id, mid);
    assert_eq!(inbox[0].author, bob_pub);

    // L'émetteur ne se mentionne pas lui-même (message composé localement).
    assert!(!bob.msg_mentions_me(&mid).unwrap());

    // Un message ordinaire n'ajoute pas d'entrée.
    let plain =
        hex::decode::<16>(&bob.group_send(&gid, &chan, "rien de spécial").unwrap()).unwrap();
    deliver(&mut rx_b, &bob_pub, &alice, &alice_pub);
    assert!(!alice.msg_mentions_me(&plain).unwrap());
    assert_eq!(alice.group_mention_count(&gid).unwrap(), 1);

    // La suppression (modération) purge l'entrée de mention.
    alice.group_delete_msg(&gid, &chan, &mid).unwrap();
    assert!(!alice.msg_mentions_me(&mid).unwrap());
    assert_eq!(alice.group_mention_count(&gid).unwrap(), 0);
}

// ---- Consentement d'invitation (D-045) : régression du force-join ----

#[test]
fn unsolicited_group_push_never_surfaces_as_joined() {
    let (alice, mut rx_a) = node_with_channel();
    let bob = node();
    let alice_pub = alice.public_key();

    // Alice monte un serveur complet mais n'invite jamais Bob.
    let gid = hex::decode::<16>(&alice.group_create("Guilde").unwrap()).unwrap();
    alice.group_add_channel(&gid, "général").unwrap();

    // Un pair malveillant qui rejouerait l'op-log complet d'Alice vers Bob
    // (l'ancien comportement de `group_invite`, sans ticket ni acceptation)
    // ne doit plus faire apparaître le groupe : chaque op est ignorée en
    // silence tant qu'aucune intention locale de rejoindre n'existe.
    let mut forced_ops = 0;
    while let Ok(action) = rx_a.try_recv() {
        if let crate::outbound::Outbound::GroupOp { op } = action {
            forced_ops += 1;
            let replies = bob
                .ingest_core(&alice_pub, CoreMsg::GroupOpMsg { op: *op })
                .unwrap();
            assert!(replies.is_empty());
        }
    }
    assert!(forced_ops >= 2, "au moins CREATE et ADD_CHANNEL attendus");
    assert!(
        bob.group_ids().unwrap().is_empty(),
        "aucun groupe ne doit apparaître sans consentement local"
    );
    assert!(
        bob.group_state(&gid).is_err(),
        "l'op-log ne doit jamais être matérialisé sans consentement"
    );

    // Une clé de groupe poussée seule (même appel) est ignorée de la même
    // façon — jamais de tentative d'ouverture pour un groupe non consenti.
    assert!(bob
        .ingest_core(
            &alice_pub,
            CoreMsg::GroupKey {
                group_id: gid,
                key_epoch: 1,
                sealed_key: [0u8; 80],
            },
        )
        .unwrap()
        .is_empty());
    assert!(bob.group_ids().unwrap().is_empty());
}

#[test]
fn invite_ticket_accept_then_finalize_makes_group_joined_and_functional() {
    let (alice, mut rx_a) = node_with_channel();
    let (bob, mut rx_b) = node_with_channel();
    let alice_pub = alice.public_key();
    let bob_pub = bob.public_key();

    let gid = hex::decode::<16>(&alice.group_create("Guilde").unwrap()).unwrap();
    let chan = hex::decode::<16>(&alice.group_add_channel(&gid, "général").unwrap()).unwrap();

    // 1. Alice autorise une invitation et envoie un ticket signé à Bob seul.
    let invite_id = hex::decode::<16>(&alice.group_invite_create(&gid, &bob_pub).unwrap()).unwrap();
    deliver(&mut rx_a, &alice_pub, &bob, &bob_pub);

    // 2. Bob voit l'invitation en attente ; le groupe n'est pas encore visible.
    let pending = bob.group_invites_list().unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].invite_id, invite_id);
    assert_eq!(pending[0].group_name, "Guilde");
    assert!(bob.group_ids().unwrap().is_empty());

    // 3. Bob accepte explicitement : Alice reçoit la preuve de consentement
    //    et pousse l'op-log complet plus la clé de groupe.
    bob.group_invite_accept(&gid, &invite_id).unwrap();
    assert!(
        bob.group_invites_list().unwrap().is_empty(),
        "l'invitation acceptée n'est plus en attente"
    );
    deliver(&mut rx_b, &bob_pub, &alice, &alice_pub);
    deliver(&mut rx_a, &alice_pub, &bob, &bob_pub);

    // 4. Le groupe est désormais rejoint et pleinement fonctionnel.
    assert_eq!(bob.group_ids().unwrap(), vec![hex::encode(&gid)]);
    let state = bob.group_state(&gid).unwrap();
    assert!(state.is_member(&bob_pub));
    assert!(state.channels.contains_key(&chan));

    let mid = hex::decode::<16>(&bob.group_send(&gid, &chan, "salut !").unwrap()).unwrap();
    deliver(&mut rx_b, &bob_pub, &alice, &alice_pub);
    let hist = alice.group_history(&gid, &chan, u64::MAX, 10).unwrap();
    assert!(hist.iter().any(|m| m.msg_id == mid && m.author == bob_pub));
}

#[test]
fn forged_invite_ticket_signature_is_rejected_before_any_storage() {
    let alice = node();
    let bob = node();
    let alice_pub = alice.public_key();

    let gid = hex::decode::<16>(&alice.group_create("Guilde").unwrap()).unwrap();
    let invite_id = accord_core::group::new_id16();
    // Ticket signé par une IDENTITÉ TIERCE mais prétendant venir d'Alice
    // (`inviter` usurpé) : la vérification de signature doit échouer.
    let forger = Identity::generate_with_pow_bits(1);
    let forged = accord_core::group::invite::build_invite_ticket(
        &forger, &gid, &invite_id, "Guilde", &[9u8; 32], 0,
    );
    let CoreMsg::InviteTicket {
        invite_id,
        group_name,
        secret,
        expires_ms,
        sig,
        ..
    } = forged
    else {
        unreachable!()
    };
    // Le message prétend venir d'Alice (usurpation du champ `inviter`) mais
    // porte la signature du forgeur : rejeté avant tout stockage.
    let replies = bob
        .ingest_core(
            &alice_pub,
            CoreMsg::InviteTicket {
                group_id: gid,
                invite_id,
                group_name,
                inviter: alice_pub,
                secret,
                expires_ms,
                sig,
            },
        )
        .unwrap();
    assert!(replies.is_empty());
    assert!(
        bob.group_invites_list().unwrap().is_empty(),
        "un ticket à la signature invalide ne doit jamais être stocké"
    );
}

#[test]
fn invite_ticket_with_spoofed_group_name_is_dropped_before_storage() {
    let bob = node();
    let attacker = Identity::generate_with_pow_bits(1);
    let attacker_pub = attacker.public_key();

    // Chaque variante est signature-valide (le ticket est authentique du
    // point de vue cryptographique) mais porte un `group_name` usurpateur
    // (MEDIUM-1) : aucune ne doit jamais atteindre la table des invitations
    // entrantes ni pouvoir déclencher `event.group_invite_pending`.
    for (i, spoofed_name) in [
        "bad\u{7}name",   // caractère de contrôle
        "\u{202E}pirate", // RLO (texte visuellement inversé)
        "z\u{200B}ero",   // espace de largeur nulle
        "\u{FEFF}bom",    // BOM / ZWNBSP
    ]
    .into_iter()
    .enumerate()
    {
        let gid = [i as u8; 16];
        let invite_id = [i as u8; 16];
        let ticket = accord_core::group::invite::build_invite_ticket(
            &attacker,
            &gid,
            &invite_id,
            spoofed_name,
            &[9u8; 32],
            0,
        );
        let replies = bob.ingest_core(&attacker_pub, ticket).unwrap();
        assert!(replies.is_empty());
    }
    assert!(
        bob.group_invites_list().unwrap().is_empty(),
        "un ticket au group_name usurpateur ne doit jamais être stocké"
    );
}

#[test]
fn invite_ticket_with_overlong_group_name_is_dropped_before_storage() {
    let bob = node();
    let attacker = Identity::generate_with_pow_bits(1);
    let attacker_pub = attacker.public_key();

    // MAX_LABEL_CHARS (borne des noms de groupe authorés localement) vaut
    // 100 : 101 caractères doit être rejeté.
    let overlong_name = "x".repeat(101);
    let ticket = accord_core::group::invite::build_invite_ticket(
        &attacker,
        &[1; 16],
        &[1; 16],
        &overlong_name,
        &[9u8; 32],
        0,
    );
    bob.ingest_core(&attacker_pub, ticket).unwrap();
    assert!(
        bob.group_invites_list().unwrap().is_empty(),
        "un group_name au-delà de MAX_LABEL_CHARS ne doit jamais être stocké"
    );
}

#[test]
fn twenty_first_incoming_invite_from_same_inviter_is_dropped_but_other_inviters_are_unaffected() {
    let bob = node();
    let attacker = Identity::generate_with_pow_bits(1);
    let attacker_pub = attacker.public_key();
    let other = Identity::generate_with_pow_bits(1);
    let other_pub = other.public_key();

    // Les 20 premiers tickets auto-signés par l'attaquant (inviter = sa
    // propre clé, donc toujours signature-valide), groupes/secrets
    // distincts, sont tous conservés.
    for i in 0..20u8 {
        let ticket = accord_core::group::invite::build_invite_ticket(
            &attacker, &[i; 16], &[i; 16], "Guilde", &[i; 32], 0,
        );
        bob.ingest_core(&attacker_pub, ticket).unwrap();
    }
    assert_eq!(bob.group_invites_list().unwrap().len(), 20);

    // Le 21e ticket du MÊME inviteur (nouvelle paire group_id/invite_id)
    // est abandonné (MEDIUM-2) : ni stockage ni événement.
    let ticket21 = accord_core::group::invite::build_invite_ticket(
        &attacker,
        &[21; 16],
        &[21; 16],
        "Guilde",
        &[21u8; 32],
        0,
    );
    bob.ingest_core(&attacker_pub, ticket21).unwrap();
    assert_eq!(
        bob.group_invites_list().unwrap().len(),
        20,
        "le 21e ticket du même inviteur doit être abandonné"
    );

    // Un ticket d'un inviteur DIFFÉRENT n'est pas soumis au plafond de
    // l'attaquant et est accepté normalement.
    let ticket_other = accord_core::group::invite::build_invite_ticket(
        &other, &[99; 16], &[99; 16], "Guilde", &[7u8; 32], 0,
    );
    bob.ingest_core(&other_pub, ticket_other).unwrap();
    assert_eq!(
        bob.group_invites_list().unwrap().len(),
        21,
        "un inviteur différent n'est pas soumis au plafond de l'attaquant"
    );
}
