//! Tests de la frontière API : formes JSON exactes des contrats gelés.

use super::*;
use crate::hex;
use crate::outbound::OutboundSink;
use accord_core::db::Db;
use accord_crypto::Identity;
use accord_proto::core_msg::{CoreMsg, MsgBody};
use serde_json::json;

fn service() -> NodeService {
    let id = Identity::generate_with_pow_bits(1);
    let db = Db::open_in_memory(&[1u8; 32]).unwrap();
    NodeService::new(Arc::new(Node::new(id, db, OutboundSink::null())))
}

/// Nœud avec un ami établi (demande sortante acceptée par le pair).
fn node_with_friend() -> (Arc<Node>, Identity) {
    let id = Identity::generate_with_pow_bits(1);
    let db = Db::open_in_memory(&[1u8; 32]).unwrap();
    let node = Arc::new(Node::new(id, db, OutboundSink::null()));
    let peer = Identity::generate_with_pow_bits(1);
    node.friend_request(&peer.public_key(), "Pair").unwrap();
    node.ingest_core(
        &peer.public_key(),
        CoreMsg::FriendResponse { accepted: true },
    )
    .unwrap();
    (node, peer)
}

/// Service adossé à un nœud avec ami ; rend la clé publique du pair (hex).
fn service_with_friend() -> (NodeService, String) {
    let (node, peer) = node_with_friend();
    (NodeService::new(node), hex::encode(&peer.public_key()))
}

/// Clés d'un objet JSON, triées (pour asserter la forme exacte).
fn sorted_keys(v: &Value) -> Vec<String> {
    let mut keys: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
    keys.sort();
    keys
}

#[tokio::test]
async fn identity_self_returns_profile() {
    let s = service();
    let v = s.call("identity.self", json!({})).await.unwrap();
    // Forme exacte du contrat gelé : `name`, `bio`, `avatar`, `banner`
    // présents (null si non définis).
    assert_eq!(
        sorted_keys(&v),
        [
            "avatar",
            "banner",
            "bio",
            "friend_code",
            "name",
            "node_id",
            "pubkey"
        ]
    );
    assert_eq!(v["pubkey"].as_str().unwrap().len(), 64);
    assert!(v["friend_code"].as_str().unwrap().contains('-'));
    assert!(v["name"].is_null());
}

// ---- Frontière JSON : contrat gelé du profil (D-027) ----

#[tokio::test]
async fn profile_get_set_exact_shapes() {
    let s = service();
    // Jamais défini : tout null.
    assert_eq!(
        s.call("profile.get", json!({})).await.unwrap(),
        json!({ "name": null, "bio": null, "avatar": null, "banner": null })
    );
    // Définition : résultat vide exact, pseudo trimé.
    assert_eq!(
        s.call("profile.set", json!({ "name": "  Anna  " }))
            .await
            .unwrap(),
        json!({})
    );
    assert_eq!(
        s.call("profile.get", json!({})).await.unwrap(),
        json!({ "name": "Anna", "bio": null, "avatar": null, "banner": null })
    );
    // Bio seule : le pseudo reste ; bio vide = effacer.
    s.call("profile.set", json!({ "bio": "  ma bio  " }))
        .await
        .unwrap();
    assert_eq!(
        s.call("profile.get", json!({})).await.unwrap(),
        json!({ "name": "Anna", "bio": "ma bio", "avatar": null, "banner": null })
    );
    s.call("profile.set", json!({ "bio": "" })).await.unwrap();
    assert_eq!(
        s.call("profile.get", json!({})).await.unwrap(),
        json!({ "name": "Anna", "bio": null, "avatar": null, "banner": null })
    );
    // Répercuté dans identity.self.
    s.call("profile.set", json!({ "bio": "présentation" }))
        .await
        .unwrap();
    let me = s.call("identity.self", json!({})).await.unwrap();
    assert_eq!(me["name"], json!("Anna"));
    assert_eq!(me["bio"], json!("présentation"));
    assert_eq!(me["avatar"], json!(null));
    assert_eq!(me["banner"], json!(null));
}

#[tokio::test]
async fn profile_set_rejects_invalid_names_explicitly() {
    let s = service();
    for bad in ["x", "   x   ", "", &"x".repeat(33) as &str] {
        let err = s
            .call("profile.set", json!({ "name": bad }))
            .await
            .unwrap_err();
        assert_eq!(err.code, accord_api::rpc::INVALID_PARAMS);
        assert!(err.message.contains("2 à 32"), "message : {}", err.message);
    }
    // Bio trop longue : refus explicite.
    let err = s
        .call("profile.set", json!({ "bio": "x".repeat(2049) }))
        .await
        .unwrap_err();
    assert_eq!(err.code, accord_api::rpc::INVALID_PARAMS);
    assert!(err.message.contains("2048"), "message : {}", err.message);
    // Paramètres tous manquants : refus à la frontière.
    let err = s.call("profile.set", json!({})).await.unwrap_err();
    assert_eq!(err.code, accord_api::rpc::INVALID_PARAMS);
    // Type inattendu : refus à la frontière.
    let err = s
        .call("profile.set", json!({ "bio": 42 }))
        .await
        .unwrap_err();
    assert_eq!(err.code, accord_api::rpc::INVALID_PARAMS);
    // Rien n'a été enregistré.
    assert_eq!(
        s.call("profile.get", json!({})).await.unwrap(),
        json!({ "name": null, "bio": null, "avatar": null, "banner": null })
    );
}

#[tokio::test]
async fn profile_set_avatar_validates_at_the_boundary() {
    let s = service();
    // data_b64 manquant ou de type inattendu : refus.
    for bad in [json!({}), json!({ "data_b64": 42, "mime": "image/png" })] {
        let err = s.call("profile.set_avatar", bad).await.unwrap_err();
        assert_eq!(err.code, accord_api::rpc::INVALID_PARAMS);
    }
    // MIME hors liste blanche : refus avant tout décodage.
    let err = s
        .call(
            "profile.set_avatar",
            json!({ "data_b64": "Zm9v", "mime": "image/svg+xml" }),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code, accord_api::rpc::INVALID_PARAMS);
    assert!(
        err.message.contains("image/png"),
        "message : {}",
        err.message
    );
    // Base64 invalide : refus.
    let err = s
        .call(
            "profile.set_avatar",
            json!({ "data_b64": "$$$$", "mime": "image/png" }),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code, accord_api::rpc::INVALID_PARAMS);
    // Trop volumineux (garde-fou avant décodage) : refus.
    let huge = "A".repeat(700_000);
    let err = s
        .call(
            "profile.set_avatar",
            json!({ "data_b64": huge, "mime": "image/png" }),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code, accord_api::rpc::INVALID_PARAMS);
    assert!(err.message.contains("512"), "message : {}", err.message);
    // Retrait (`data_b64: null`) : accepté même sans avatar, forme exacte.
    assert_eq!(
        s.call("profile.set_avatar", json!({ "data_b64": null }))
            .await
            .unwrap(),
        json!({ "avatar": null })
    );
    // Entrée valide : atteint le magasin de fichiers — indisponible sur la
    // base en mémoire de la fixture, l'erreur remonte et rien n'est persisté.
    let err = s
        .call(
            "profile.set_avatar",
            json!({ "data_b64": "iVBORw0KGgo=", "mime": "image/png" }),
        )
        .await
        .unwrap_err();
    assert!(
        err.message.contains("magasin de fichiers indisponible"),
        "message : {}",
        err.message
    );
    assert_eq!(
        s.call("profile.get", json!({})).await.unwrap(),
        json!({ "name": null, "bio": null, "avatar": null, "banner": null })
    );
}

#[tokio::test]
async fn profile_set_banner_validates_at_the_boundary() {
    let s = service();
    // data_b64 manquant ou de type inattendu : refus.
    for bad in [json!({}), json!({ "data_b64": 42, "mime": "image/png" })] {
        let err = s.call("profile.set_banner", bad).await.unwrap_err();
        assert_eq!(err.code, accord_api::rpc::INVALID_PARAMS);
    }
    // MIME hors liste blanche : refus avant tout décodage.
    let err = s
        .call(
            "profile.set_banner",
            json!({ "data_b64": "Zm9v", "mime": "image/svg+xml" }),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code, accord_api::rpc::INVALID_PARAMS);
    assert!(
        err.message.contains("image/png"),
        "message : {}",
        err.message
    );
    // Base64 invalide : refus.
    let err = s
        .call(
            "profile.set_banner",
            json!({ "data_b64": "$$$$", "mime": "image/png" }),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code, accord_api::rpc::INVALID_PARAMS);
    // Trop volumineux (garde-fou avant décodage, borne 1 Mio) : refus.
    let huge = "A".repeat(1_400_001);
    let err = s
        .call(
            "profile.set_banner",
            json!({ "data_b64": huge, "mime": "image/png" }),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code, accord_api::rpc::INVALID_PARAMS);
    assert!(err.message.contains("1 Mio"), "message : {}", err.message);
    // Retrait (`data_b64: null`) : accepté même sans bannière, forme exacte.
    assert_eq!(
        s.call("profile.set_banner", json!({ "data_b64": null }))
            .await
            .unwrap(),
        json!({ "banner": null })
    );
    // Entrée valide : atteint le magasin de fichiers — indisponible sur la
    // base en mémoire de la fixture, l'erreur remonte et rien n'est persisté.
    let err = s
        .call(
            "profile.set_banner",
            json!({ "data_b64": "iVBORw0KGgo=", "mime": "image/png" }),
        )
        .await
        .unwrap_err();
    assert!(
        err.message.contains("magasin de fichiers indisponible"),
        "message : {}",
        err.message
    );
    assert_eq!(
        s.call("profile.get", json!({})).await.unwrap(),
        json!({ "name": null, "bio": null, "avatar": null, "banner": null })
    );
}

#[tokio::test]
async fn profile_of_friend_updates_friends_list() {
    let (node, peer) = node_with_friend();
    let s = NodeService::new(Arc::clone(&node));
    node.ingest_core(
        &peer.public_key(),
        CoreMsg::Profile {
            display_name: "Pair Renommé".into(),
            bio: "sa bio".into(),
            avatar: Some([5u8; 32]),
            banner: Some([6u8; 32]),
        },
    )
    .unwrap();
    let v = s.call("friends.list", json!({})).await.unwrap();
    let contact = &v["contacts"][0];
    assert_eq!(contact["display_name"], json!("Pair Renommé"));
    // Profil public annoncé exposé dans friends.list : bio, avatar, bannière.
    assert_eq!(contact["bio"], json!("sa bio"));
    assert_eq!(contact["avatar"], json!("05".repeat(32)));
    assert_eq!(contact["banner"], json!("06".repeat(32)));
    // Nouvelle annonce sans bannière : le champ s'efface (null).
    node.ingest_core(
        &peer.public_key(),
        CoreMsg::Profile {
            display_name: "Pair Renommé".into(),
            bio: String::new(),
            avatar: None,
            banner: None,
        },
    )
    .unwrap();
    let v = s.call("friends.list", json!({})).await.unwrap();
    assert_eq!(v["contacts"][0]["banner"], json!(null));
}

#[tokio::test]
async fn unknown_method_is_rejected() {
    let s = service();
    let err = s.call("n.existe.pas", json!({})).await.unwrap_err();
    assert!(err.message.contains("inconnue"));
}

#[tokio::test]
async fn group_create_and_state_via_api() {
    let s = service();
    let created = s
        .call("groups.create", json!({"name": "Guilde"}))
        .await
        .unwrap();
    let gid = created["group_id"].as_str().unwrap().to_string();
    let chan = s
        .call(
            "groups.channel.add",
            json!({"group_id": gid, "name": "général"}),
        )
        .await
        .unwrap();
    assert_eq!(chan["channel_id"].as_str().unwrap().len(), 32);
    let state = s
        .call("groups.state", json!({"group_id": gid}))
        .await
        .unwrap();
    assert_eq!(state["name"], "Guilde");
    assert_eq!(state["channels"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn invalid_params_are_rejected_at_boundary() {
    let s = service();
    let err = s
        .call("groups.state", json!({"group_id": "zz"}))
        .await
        .unwrap_err();
    assert_eq!(err.code, accord_api::rpc::INVALID_PARAMS);
}

#[tokio::test]
async fn dm_requires_friend() {
    let s = service();
    let peer = Identity::generate_with_pow_bits(1);
    let err = s
        .call(
            "dm.send",
            json!({"pubkey": hex::encode(&peer.public_key()), "text": "salut"}),
        )
        .await
        .unwrap_err();
    assert!(err.message.contains("refusé") || err.message.contains("ami"));
}

// ---- Frontière JSON : forme exacte rendue par dm.history ----

#[tokio::test]
async fn dm_history_renders_exact_text_shape() {
    let (s, peer) = service_with_friend();
    s.call("dm.send", json!({"pubkey": peer, "text": "bonjour exact"}))
        .await
        .unwrap();
    let hist = s.call("dm.history", json!({"pubkey": peer})).await.unwrap();
    let m = &hist["messages"][0];
    // Forme exacte de l'enveloppe : pas de champ `kind`, `reactions` et
    // `attachments` toujours présents ; `pinned` et `delivery` ajoutés.
    assert_eq!(
        sorted_keys(m),
        [
            "acked",
            "attachments",
            "author",
            "body",
            "deleted",
            "delivery",
            "edited",
            "lamport",
            "mentions_me",
            "msg_id",
            "pinned",
            "reactions",
            "sent_ms"
        ]
    );
    // Freshly sent, unacked, not queued (null sink) ⇒ pending, not pinned.
    assert_eq!(m["pinned"], json!(false));
    assert_eq!(m["delivery"], json!("pending"));
    // Forme exacte du corps texte : `reply_to` présent (null si absent),
    // `attachments` est un compteur, pas de champ `kind` ; la liste détaillée
    // des pièces jointes vit au niveau de l'enveloppe.
    assert_eq!(
        m["body"],
        json!({
            "type": "text",
            "text": "bonjour exact",
            "reply_to": null,
            "attachments": 0,
        })
    );
    assert!(m["edited"].is_null());
    assert_eq!(m["deleted"], json!(false));
    assert_eq!(m["reactions"], json!([]));
    assert_eq!(m["attachments"], json!([]));
}

#[tokio::test]
async fn dm_pins_and_history_around_flow() {
    let (s, peer) = service_with_friend();
    let id = s
        .call("dm.send", json!({"pubkey": peer, "text": "cible du saut"}))
        .await
        .unwrap()["msg_id"]
        .as_str()
        .unwrap()
        .to_string();

    // Pin/list/unpin mirror the group pin API.
    s.call("dm.pin", json!({"pubkey": peer, "msg_id": id}))
        .await
        .unwrap();
    let pins = s.call("dm.pins", json!({"pubkey": peer})).await.unwrap();
    assert_eq!(pins["msg_ids"], json!([id]));
    // history reflects the pin flag.
    let hist = s.call("dm.history", json!({"pubkey": peer})).await.unwrap();
    assert_eq!(hist["messages"][0]["pinned"], json!(true));
    s.call("dm.unpin", json!({"pubkey": peer, "msg_id": id}))
        .await
        .unwrap();
    assert_eq!(
        s.call("dm.pins", json!({"pubkey": peer})).await.unwrap()["msg_ids"],
        json!([])
    );

    // history_around centers on the target and flags found.
    let win = s
        .call("dm.history_around", json!({"pubkey": peer, "msg_id": id}))
        .await
        .unwrap();
    assert_eq!(win["found"], json!(true));
    assert_eq!(win["messages"][0]["msg_id"], json!(id));
    // Unknown target ⇒ empty window, found = false.
    let miss = s
        .call(
            "dm.history_around",
            json!({"pubkey": peer, "msg_id": "ee".repeat(16)}),
        )
        .await
        .unwrap();
    assert_eq!(miss["found"], json!(false));
    assert_eq!(miss["messages"], json!([]));
}

#[tokio::test]
async fn dm_retry_rejects_delivered_message() {
    let (node, peer_id) = node_with_friend();
    let s = NodeService::new(Arc::clone(&node));
    let peer = hex::encode(&peer_id.public_key());
    let id = s
        .call("dm.send", json!({"pubkey": peer, "text": "à renvoyer"}))
        .await
        .unwrap()["msg_id"]
        .as_str()
        .unwrap()
        .to_string();
    let mid = hex::decode::<16>(&id).unwrap();
    // Unacked ⇒ retry accepted.
    s.call("dm.retry", json!({"pubkey": peer, "msg_id": id}))
        .await
        .unwrap();
    // Once acked (delivery = sent), retry is refused.
    node.ingest_core(&peer_id.public_key(), CoreMsg::MsgAck { msg_id: mid })
        .unwrap();
    let hist = s.call("dm.history", json!({"pubkey": peer})).await.unwrap();
    assert_eq!(hist["messages"][0]["delivery"], json!("sent"));
    assert!(s
        .call("dm.retry", json!({"pubkey": peer, "msg_id": id}))
        .await
        .is_err());
}

#[tokio::test]
async fn search_query_returns_hits_and_msg_ids() {
    let (s, peer) = service_with_friend();
    s.call(
        "dm.send",
        json!({"pubkey": peer, "text": "rendez-vous demain"}),
    )
    .await
    .unwrap();
    let res = s
        .call("search.query", json!({"query": "demain"}))
        .await
        .unwrap();
    assert_eq!(res["hits"].as_array().unwrap().len(), 1);
    assert_eq!(res["msg_ids"].as_array().unwrap().len(), 1);
    assert_eq!(res["hits"][0]["conversation"]["type"], json!("dm"));
    assert_eq!(res["hits"][0]["conversation"]["peer"], json!(peer));
    // A from: filter that resolves to nobody yields no hit.
    let none = s
        .call("search.query", json!({"query": "from:inconnu demain"}))
        .await
        .unwrap();
    assert_eq!(none["hits"], json!([]));
}

#[tokio::test]
async fn dm_history_renders_reply_to_when_set() {
    let (s, peer) = service_with_friend();
    let first = s
        .call("dm.send", json!({"pubkey": peer, "text": "premier"}))
        .await
        .unwrap();
    let first_id = first["msg_id"].as_str().unwrap().to_string();
    s.call(
        "dm.send",
        json!({"pubkey": peer, "text": "réponse", "reply_to": first_id}),
    )
    .await
    .unwrap();
    let hist = s.call("dm.history", json!({"pubkey": peer})).await.unwrap();
    // Trié du plus récent au plus ancien : la réponse est en tête.
    assert_eq!(hist["messages"][0]["body"]["reply_to"], json!(first_id));
    assert!(hist["messages"][1]["body"]["reply_to"].is_null());
}

#[tokio::test]
async fn dm_edit_fills_edited_and_keeps_original_body() {
    let (s, peer) = service_with_friend();
    let sent = s
        .call("dm.send", json!({"pubkey": peer, "text": "brouillon"}))
        .await
        .unwrap();
    let id = sent["msg_id"].as_str().unwrap().to_string();
    let before = s.call("dm.history", json!({"pubkey": peer})).await.unwrap();
    assert!(before["messages"][0]["edited"].is_null());

    s.call(
        "dm.edit",
        json!({"pubkey": peer, "msg_id": id, "text": "version finale"}),
    )
    .await
    .unwrap();
    let after = s.call("dm.history", json!({"pubkey": peer})).await.unwrap();
    // L'enveloppe d'édition n'est pas un nouveau message d'historique.
    assert_eq!(after["messages"].as_array().unwrap().len(), 1);
    let m = &after["messages"][0];
    assert_eq!(m["edited"], json!("version finale"));
    assert_eq!(m["body"]["text"], json!("brouillon"));
}

#[tokio::test]
async fn dm_delete_renders_unknown_body_and_tombstone() {
    let (s, peer) = service_with_friend();
    let sent = s
        .call("dm.send", json!({"pubkey": peer, "text": "à effacer"}))
        .await
        .unwrap();
    let id = sent["msg_id"].as_str().unwrap().to_string();
    s.call("dm.delete", json!({"pubkey": peer, "msg_id": id}))
        .await
        .unwrap();
    let hist = s.call("dm.history", json!({"pubkey": peer})).await.unwrap();
    let m = &hist["messages"][0];
    assert_eq!(m["body"], json!({ "type": "unknown" }));
    assert_eq!(m["deleted"], json!(true));
    assert!(m["edited"].is_null());
}

#[tokio::test]
async fn dm_react_adds_then_removes_reaction() {
    let (s, peer) = service_with_friend();
    let sent = s
        .call("dm.send", json!({"pubkey": peer, "text": "réagis-moi"}))
        .await
        .unwrap();
    let id = sent["msg_id"].as_str().unwrap().to_string();
    s.call(
        "dm.react",
        json!({"pubkey": peer, "msg_id": id, "emoji": "👍"}),
    )
    .await
    .unwrap();
    let hist = s.call("dm.history", json!({"pubkey": peer})).await.unwrap();
    let me = s.call("identity.self", json!({})).await.unwrap();
    assert_eq!(
        hist["messages"][0]["reactions"],
        json!([{ "emoji": "👍", "author": me["pubkey"] }])
    );

    s.call(
        "dm.react",
        json!({"pubkey": peer, "msg_id": id, "emoji": "👍", "remove": true}),
    )
    .await
    .unwrap();
    let hist = s.call("dm.history", json!({"pubkey": peer})).await.unwrap();
    assert_eq!(hist["messages"][0]["reactions"], json!([]));
}

#[tokio::test]
async fn dm_edit_of_peer_message_is_refused() {
    let (node, peer) = node_with_friend();
    let s = NodeService::new(Arc::clone(&node));
    let body = MsgBody::Text {
        text: "à lui".into(),
        reply_to: None,
        attachments: vec![],
    };
    node.ingest_core(
        &peer.public_key(),
        CoreMsg::DirectMsg {
            msg_id: [9; 16],
            lamport: 1,
            sent_ms: 1,
            kind: body.kind(),
            body: body.encode_body(),
        },
    )
    .unwrap();
    let err = s
        .call(
            "dm.edit",
            json!({
                "pubkey": hex::encode(&peer.public_key()),
                "msg_id": hex::encode(&[9u8; 16]),
                "text": "piraté",
            }),
        )
        .await
        .unwrap_err();
    assert!(err.message.contains("refusé"));
}

// ---- Frontière JSON : forme exacte rendue par groups.history ----

#[tokio::test]
async fn group_history_renders_exact_text_shape() {
    let s = service();
    let created = s
        .call("groups.create", json!({"name": "Guilde"}))
        .await
        .unwrap();
    let gid = created["group_id"].as_str().unwrap().to_string();
    let chan = s
        .call(
            "groups.channel.add",
            json!({"group_id": gid, "name": "général"}),
        )
        .await
        .unwrap();
    let cid = chan["channel_id"].as_str().unwrap().to_string();
    s.call(
        "groups.send",
        json!({"group_id": gid, "channel_id": cid, "text": "salut groupe"}),
    )
    .await
    .unwrap();
    let hist = s
        .call(
            "groups.history",
            json!({"group_id": gid, "channel_id": cid}),
        )
        .await
        .unwrap();
    let m = &hist["messages"][0];
    // Même schéma que dm.history, plus `channel_id`, sans `acked`,
    // sans champ `kind`.
    assert_eq!(
        sorted_keys(m),
        [
            "attachments",
            "author",
            "body",
            "channel_id",
            "deleted",
            "edited",
            "lamport",
            "mentions_me",
            "msg_id",
            "reactions",
            "sent_ms"
        ]
    );
    assert_eq!(
        m["body"],
        json!({
            "type": "text",
            "text": "salut groupe",
            "reply_to": null,
            "attachments": 0,
        })
    );
    assert_eq!(m["channel_id"], json!(cid));
    assert!(m["edited"].is_null());
    assert_eq!(m["deleted"], json!(false));
    assert_eq!(m["reactions"], json!([]));
    assert_eq!(m["attachments"], json!([]));
}

#[tokio::test]
async fn group_history_around_centers_on_target() {
    let s = service();
    let gid = s
        .call("groups.create", json!({"name": "Guilde"}))
        .await
        .unwrap()["group_id"]
        .as_str()
        .unwrap()
        .to_string();
    let cid = s
        .call(
            "groups.channel.add",
            json!({"group_id": gid, "name": "général"}),
        )
        .await
        .unwrap()["channel_id"]
        .as_str()
        .unwrap()
        .to_string();
    let mut ids = Vec::new();
    for i in 0..5 {
        let id = s
            .call(
                "groups.send",
                json!({"group_id": gid, "channel_id": cid, "text": format!("msg {i}")}),
            )
            .await
            .unwrap()["msg_id"]
            .as_str()
            .unwrap()
            .to_string();
        ids.push(id);
    }
    let target = ids[2].clone();
    let win = s
        .call(
            "groups.history_around",
            json!({"group_id": gid, "channel_id": cid, "msg_id": target, "limit": 2}),
        )
        .await
        .unwrap();
    assert_eq!(win["found"], json!(true));
    let got: Vec<String> = win["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["msg_id"].as_str().unwrap().to_string())
        .collect();
    assert!(got.contains(&target));
    // Unknown target ⇒ empty window, found = false.
    let miss = s
        .call(
            "groups.history_around",
            json!({"group_id": gid, "channel_id": cid, "msg_id": "ee".repeat(16)}),
        )
        .await
        .unwrap();
    assert_eq!(miss["found"], json!(false));
    assert_eq!(miss["messages"], json!([]));
}

// ---- Gestion de serveur : groupes, salons, rôles, modération ----

/// Service + `group_id` (hex) d'un groupe fraîchement créé.
async fn service_with_group() -> (NodeService, String) {
    let s = service();
    let created = s
        .call("groups.create", json!({"name": "Guilde"}))
        .await
        .unwrap();
    (s, created["group_id"].as_str().unwrap().to_string())
}

#[tokio::test]
async fn group_state_enriched_exact_shape() {
    let (s, gid) = service_with_group().await;
    let state = s
        .call("groups.state", json!({"group_id": gid}))
        .await
        .unwrap();
    assert_eq!(
        sorted_keys(&state),
        [
            "bans",
            "categories",
            "channels",
            "emojis",
            "founder",
            "group_id",
            "icon",
            "invites",
            "members",
            "my_permissions",
            "name",
            "overrides",
            "roles"
        ]
    );
    assert_eq!(state["group_id"], json!(gid));
    assert!(state["icon"].is_null());
    assert_eq!(state["overrides"], json!([]));
    // Fondateur : toutes les permissions (VIEW..ADMIN + MANAGE_EMOJIS = 0x3FF).
    assert_eq!(state["my_permissions"], json!(0x3FFu32));
    assert_eq!(state["emojis"], json!([]));
    let me = s.call("identity.self", json!({})).await.unwrap();
    assert_eq!(
        state["members"],
        json!([{
            "pubkey": me["pubkey"],
            "roles": [],
            "nickname": null,
            "timeout_until_ms": 0,
        }])
    );
    assert_eq!(state["bans"], json!([]));
    assert_eq!(state["invites"], json!([]));
}

#[tokio::test]
async fn group_rename_and_topic_are_reflected_in_state() {
    let (s, gid) = service_with_group().await;
    s.call(
        "groups.rename",
        json!({"group_id": gid, "name": "Renommée"}),
    )
    .await
    .unwrap();
    let chan = s
        .call(
            "groups.channel.add",
            json!({"group_id": gid, "name": "général"}),
        )
        .await
        .unwrap();
    let cid = chan["channel_id"].as_str().unwrap().to_string();
    s.call(
        "groups.set_topic",
        json!({"group_id": gid, "channel_id": cid, "topic": "les règles"}),
    )
    .await
    .unwrap();
    let state = s
        .call("groups.state", json!({"group_id": gid}))
        .await
        .unwrap();
    assert_eq!(state["name"], json!("Renommée"));
    let ch = &state["channels"][0];
    assert_eq!(
        sorted_keys(ch),
        [
            "category",
            "channel_id",
            "kind",
            "name",
            "position",
            "topic"
        ]
    );
    assert_eq!(ch["kind"], json!("text"));
    assert_eq!(ch["topic"], json!("les règles"));
    assert!(ch["category"].is_null());
}

#[tokio::test]
async fn group_channels_add_voice_edit_delete_and_categories() {
    let (s, gid) = service_with_group().await;
    let cat = s
        .call(
            "groups.category.add",
            json!({"group_id": gid, "name": "Vocaux"}),
        )
        .await
        .unwrap();
    let cat_id = cat["category_id"].as_str().unwrap().to_string();
    let chan = s
        .call(
            "groups.channel.add",
            json!({"group_id": gid, "name": "salon-vocal", "kind": "voice", "category": cat_id}),
        )
        .await
        .unwrap();
    let cid = chan["channel_id"].as_str().unwrap().to_string();

    let state = s
        .call("groups.state", json!({"group_id": gid}))
        .await
        .unwrap();
    assert_eq!(state["categories"][0]["name"], json!("Vocaux"));
    let ch = &state["channels"][0];
    assert_eq!(ch["kind"], json!("voice"));
    assert_eq!(ch["category"], json!(cat_id));

    // Édition partielle : position seule, le nom reste.
    s.call(
        "groups.channel.edit",
        json!({"group_id": gid, "channel_id": cid, "position": 7}),
    )
    .await
    .unwrap();
    let state = s
        .call("groups.state", json!({"group_id": gid}))
        .await
        .unwrap();
    assert_eq!(state["channels"][0]["name"], json!("salon-vocal"));
    assert_eq!(state["channels"][0]["position"], json!(7));

    // Suppression.
    s.call(
        "groups.channel.del",
        json!({"group_id": gid, "channel_id": cid}),
    )
    .await
    .unwrap();
    let state = s
        .call("groups.state", json!({"group_id": gid}))
        .await
        .unwrap();
    assert_eq!(state["channels"], json!([]));

    // Nature de salon inconnue : refusée à la frontière.
    let err = s
        .call(
            "groups.channel.add",
            json!({"group_id": gid, "name": "x", "kind": "video"}),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code, accord_api::rpc::INVALID_PARAMS);
    // Catégorie inconnue : introuvable.
    let err = s
        .call(
            "groups.channel.add",
            json!({"group_id": gid, "name": "x", "category": "00000000000000000000000000000000"}),
        )
        .await
        .unwrap_err();
    assert!(err.message.contains("catégorie"));
}

#[tokio::test]
async fn group_categories_edit_delete_and_channel_move() {
    let (s, gid) = service_with_group().await;
    let cat = s
        .call(
            "groups.category.add",
            json!({"group_id": gid, "name": "Vocaux"}),
        )
        .await
        .unwrap();
    let cat_id = cat["category_id"].as_str().unwrap().to_string();
    let chan = s
        .call(
            "groups.channel.add",
            json!({"group_id": gid, "name": "général", "category": cat_id}),
        )
        .await
        .unwrap();
    let cid = chan["channel_id"].as_str().unwrap().to_string();

    // Renommage de la catégorie.
    s.call(
        "groups.category.edit",
        json!({"group_id": gid, "category_id": cat_id, "name": "Textuels"}),
    )
    .await
    .unwrap();
    // Déplacement du salon hors de toute catégorie (`category: null`).
    s.call(
        "groups.channel.edit",
        json!({"group_id": gid, "channel_id": cid, "category": null}),
    )
    .await
    .unwrap();
    let state = s
        .call("groups.state", json!({"group_id": gid}))
        .await
        .unwrap();
    assert_eq!(state["categories"][0]["name"], json!("Textuels"));
    assert!(state["channels"][0]["category"].is_null());

    // Retour dans la catégorie, puis suppression : le salon survit.
    s.call(
        "groups.channel.edit",
        json!({"group_id": gid, "channel_id": cid, "category": cat_id}),
    )
    .await
    .unwrap();
    s.call(
        "groups.category.del",
        json!({"group_id": gid, "category_id": cat_id}),
    )
    .await
    .unwrap();
    let state = s
        .call("groups.state", json!({"group_id": gid}))
        .await
        .unwrap();
    assert_eq!(state["categories"], json!([]));
    assert_eq!(state["channels"][0]["name"], json!("général"));
    assert!(state["channels"][0]["category"].is_null());
}

#[tokio::test]
async fn group_channel_perms_override_and_scoped_my_permissions() {
    let (s, gid) = service_with_group().await;
    let chan = s
        .call(
            "groups.channel.add",
            json!({"group_id": gid, "name": "général"}),
        )
        .await
        .unwrap();
    let cid = chan["channel_id"].as_str().unwrap().to_string();
    let role = s
        .call(
            "groups.role.add",
            json!({"group_id": gid, "name": "Muet", "color": 0, "permissions": 0}),
        )
        .await
        .unwrap();
    let rid = role["role_id"].as_str().unwrap().to_string();

    s.call(
        "groups.channel.perms",
        json!({"group_id": gid, "channel_id": cid, "role_id": rid,
               "allow": 0, "deny": 0x2}),
    )
    .await
    .unwrap();
    let state = s
        .call("groups.state", json!({"group_id": gid}))
        .await
        .unwrap();
    assert_eq!(
        state["overrides"],
        json!([{ "channel_id": cid, "role_id": rid, "allow": 0, "deny": 0x2 }])
    );
    // Portée salon : le fondateur (ADMIN implicite) garde tout.
    let scoped = s
        .call("groups.state", json!({"group_id": gid, "channel_id": cid}))
        .await
        .unwrap();
    assert_eq!(scoped["my_permissions"], json!(0x3FFu32));

    // allow ∩ deny non vide : refusé à la frontière.
    let err = s
        .call(
            "groups.channel.perms",
            json!({"group_id": gid, "channel_id": cid, "role_id": rid,
                   "allow": 0x2, "deny": 0x2}),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code, accord_api::rpc::INVALID_PARAMS);
}

#[tokio::test]
async fn group_audit_lists_decoded_entries_newest_first() {
    let (s, gid) = service_with_group().await;
    s.call(
        "groups.channel.add",
        json!({"group_id": gid, "name": "général"}),
    )
    .await
    .unwrap();
    s.call(
        "groups.rename",
        json!({"group_id": gid, "name": "Renommée"}),
    )
    .await
    .unwrap();

    let page = s
        .call("groups.audit", json!({"group_id": gid, "limit": 2}))
        .await
        .unwrap();
    let entries = page["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(
        sorted_keys(&entries[0]),
        ["author", "kind", "lamport", "op_id", "params", "wall_ms"]
    );
    // Du plus récent au plus ancien : SET_META puis ADD_CHANNEL.
    assert_eq!(entries[0]["kind"], json!("set_meta"));
    assert_eq!(entries[0]["params"]["name"], json!("Renommée"));
    assert_eq!(entries[1]["kind"], json!("add_channel"));

    // Curseur : la page suivante remonte jusqu'au CREATE.
    let before = entries[1]["op_id"].as_str().unwrap();
    let page2 = s
        .call(
            "groups.audit",
            json!({"group_id": gid, "before": before, "limit": 10}),
        )
        .await
        .unwrap();
    let entries2 = page2["entries"].as_array().unwrap();
    assert_eq!(entries2.len(), 1);
    assert_eq!(entries2[0]["kind"], json!("create"));
}

#[tokio::test]
async fn group_roles_lifecycle_and_membership() {
    let (node, peer) = node_with_friend();
    let s = NodeService::new(Arc::clone(&node));
    let gid = node.group_create("Guilde").unwrap();
    let peer_hex = hex::encode(&peer.public_key());
    s.call(
        "groups.invite",
        json!({"group_id": gid, "pubkey": peer_hex}),
    )
    .await
    .unwrap();

    // Création d'un rôle Modo (KICK|BAN = 0x60).
    let role = s
        .call(
            "groups.role.add",
            json!({"group_id": gid, "name": "Modo", "color": 0xFF0000, "permissions": 0x60, "position": 5}),
        )
        .await
        .unwrap();
    let rid = role["role_id"].as_str().unwrap().to_string();

    s.call(
        "groups.role.assign",
        json!({"group_id": gid, "role_id": rid, "pubkey": peer_hex}),
    )
    .await
    .unwrap();
    let state = s
        .call("groups.state", json!({"group_id": gid}))
        .await
        .unwrap();
    let role_json = &state["roles"][0];
    assert_eq!(
        sorted_keys(role_json),
        ["color", "name", "permissions", "position", "role_id"]
    );
    assert_eq!(role_json["permissions"], json!(0x60));
    let member = state["members"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["pubkey"] == json!(peer_hex))
        .unwrap()
        .clone();
    assert_eq!(member["roles"], json!([rid]));

    // Édition partielle : les permissions seules changent.
    s.call(
        "groups.role.edit",
        json!({"group_id": gid, "role_id": rid, "permissions": 0x61}),
    )
    .await
    .unwrap();
    let state = s
        .call("groups.state", json!({"group_id": gid}))
        .await
        .unwrap();
    assert_eq!(state["roles"][0]["name"], json!("Modo"));
    assert_eq!(state["roles"][0]["permissions"], json!(0x61));

    // Retrait et suppression.
    s.call(
        "groups.role.unassign",
        json!({"group_id": gid, "role_id": rid, "pubkey": peer_hex}),
    )
    .await
    .unwrap();
    s.call("groups.role.del", json!({"group_id": gid, "role_id": rid}))
        .await
        .unwrap();
    let state = s
        .call("groups.state", json!({"group_id": gid}))
        .await
        .unwrap();
    assert_eq!(state["roles"], json!([]));

    // Bornes : couleur et bits inconnus refusés.
    let err = s
        .call(
            "groups.role.add",
            json!({"group_id": gid, "name": "X", "color": 0x1000000, "permissions": 1}),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code, accord_api::rpc::INVALID_PARAMS);
    let err = s
        .call(
            "groups.role.add",
            json!({"group_id": gid, "name": "X", "color": 0, "permissions": 0x400}),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code, accord_api::rpc::INVALID_PARAMS);
}

#[tokio::test]
async fn group_kick_ban_unban_and_leave() {
    let (node, peer) = node_with_friend();
    let s = NodeService::new(Arc::clone(&node));
    let gid = node.group_create("Guilde").unwrap();
    let peer_hex = hex::encode(&peer.public_key());

    // Le fondateur ne part pas tant qu'il reste des membres… enfin, tant
    // qu'il n'est pas le dernier : ici il est seul, le départ est permis mais
    // on teste d'abord la modération.
    s.call(
        "groups.invite",
        json!({"group_id": gid, "pubkey": peer_hex}),
    )
    .await
    .unwrap();
    // Fondateur avec un membre : départ refusé.
    let err = s
        .call("groups.leave", json!({"group_id": gid}))
        .await
        .unwrap_err();
    assert!(err.message.contains("refusé"));

    // Expulsion : le membre disparaît.
    s.call("groups.kick", json!({"group_id": gid, "pubkey": peer_hex}))
        .await
        .unwrap();
    let state = s
        .call("groups.state", json!({"group_id": gid}))
        .await
        .unwrap();
    assert_eq!(state["members"].as_array().unwrap().len(), 1);

    // Réadmission puis bannissement : membres -1, bans +1.
    s.call(
        "groups.invite",
        json!({"group_id": gid, "pubkey": peer_hex}),
    )
    .await
    .unwrap();
    s.call("groups.ban", json!({"group_id": gid, "pubkey": peer_hex}))
        .await
        .unwrap();
    let state = s
        .call("groups.state", json!({"group_id": gid}))
        .await
        .unwrap();
    assert_eq!(state["members"].as_array().unwrap().len(), 1);
    assert_eq!(state["bans"], json!([peer_hex]));

    // Un banni ne se réinvite pas.
    let err = s
        .call(
            "groups.invite",
            json!({"group_id": gid, "pubkey": peer_hex}),
        )
        .await
        .unwrap_err();
    assert!(err.message.contains("refusé"));

    // Levée du bannissement.
    s.call("groups.unban", json!({"group_id": gid, "pubkey": peer_hex}))
        .await
        .unwrap();
    let state = s
        .call("groups.state", json!({"group_id": gid}))
        .await
        .unwrap();
    assert_eq!(state["bans"], json!([]));
}

#[tokio::test]
async fn group_pins_roundtrip_and_unknown_message_is_refused() {
    let (s, gid) = service_with_group().await;
    let chan = s
        .call(
            "groups.channel.add",
            json!({"group_id": gid, "name": "général"}),
        )
        .await
        .unwrap();
    let cid = chan["channel_id"].as_str().unwrap().to_string();
    let sent = s
        .call(
            "groups.send",
            json!({"group_id": gid, "channel_id": cid, "text": "à épingler"}),
        )
        .await
        .unwrap();
    let mid = sent["msg_id"].as_str().unwrap().to_string();

    // Message inconnu : refus.
    let err = s
        .call(
            "groups.pin",
            json!({"group_id": gid, "channel_id": cid, "msg_id": "00000000000000000000000000000000"}),
        )
        .await
        .unwrap_err();
    assert!(err.message.contains("introuvable"));

    s.call(
        "groups.pin",
        json!({"group_id": gid, "channel_id": cid, "msg_id": mid}),
    )
    .await
    .unwrap();
    let pins = s
        .call("groups.pins", json!({"group_id": gid, "channel_id": cid}))
        .await
        .unwrap();
    assert_eq!(pins, json!({ "msg_ids": [mid] }));

    s.call(
        "groups.unpin",
        json!({"group_id": gid, "channel_id": cid, "msg_id": mid}),
    )
    .await
    .unwrap();
    let pins = s
        .call("groups.pins", json!({"group_id": gid, "channel_id": cid}))
        .await
        .unwrap();
    assert_eq!(pins, json!({ "msg_ids": [] }));
}

#[tokio::test]
async fn group_edit_delete_react_reflected_in_history() {
    let (s, gid) = service_with_group().await;
    let chan = s
        .call(
            "groups.channel.add",
            json!({"group_id": gid, "name": "général"}),
        )
        .await
        .unwrap();
    let cid = chan["channel_id"].as_str().unwrap().to_string();
    let sent = s
        .call(
            "groups.send",
            json!({"group_id": gid, "channel_id": cid, "text": "brouillon"}),
        )
        .await
        .unwrap();
    let mid = sent["msg_id"].as_str().unwrap().to_string();

    // Édition : `edited` porté par l'enveloppe, corps d'origine conservé.
    s.call(
        "groups.edit",
        json!({"group_id": gid, "channel_id": cid, "msg_id": mid, "text": "final"}),
    )
    .await
    .unwrap();
    let hist = s
        .call(
            "groups.history",
            json!({"group_id": gid, "channel_id": cid}),
        )
        .await
        .unwrap();
    assert_eq!(hist["messages"].as_array().unwrap().len(), 1);
    assert_eq!(hist["messages"][0]["edited"], json!("final"));
    assert_eq!(hist["messages"][0]["body"]["text"], json!("brouillon"));

    // Réaction : ajout puis retrait explicite (`add: false`).
    s.call(
        "groups.react",
        json!({"group_id": gid, "channel_id": cid, "msg_id": mid, "emoji": "🎉"}),
    )
    .await
    .unwrap();
    let hist = s
        .call(
            "groups.history",
            json!({"group_id": gid, "channel_id": cid}),
        )
        .await
        .unwrap();
    let me = s.call("identity.self", json!({})).await.unwrap();
    assert_eq!(
        hist["messages"][0]["reactions"],
        json!([{ "emoji": "🎉", "author": me["pubkey"] }])
    );
    s.call(
        "groups.react",
        json!({"group_id": gid, "channel_id": cid, "msg_id": mid, "emoji": "🎉", "add": false}),
    )
    .await
    .unwrap();
    let hist = s
        .call(
            "groups.history",
            json!({"group_id": gid, "channel_id": cid}),
        )
        .await
        .unwrap();
    assert_eq!(hist["messages"][0]["reactions"], json!([]));

    // Suppression par l'auteur : tombstone.
    s.call(
        "groups.delete",
        json!({"group_id": gid, "channel_id": cid, "msg_id": mid}),
    )
    .await
    .unwrap();
    let hist = s
        .call(
            "groups.history",
            json!({"group_id": gid, "channel_id": cid}),
        )
        .await
        .unwrap();
    assert_eq!(hist["messages"][0]["deleted"], json!(true));
    assert_eq!(hist["messages"][0]["body"], json!({ "type": "unknown" }));
}

// ---- Pièces jointes : frontière JSON ----

#[tokio::test]
async fn dm_send_with_attachments_renders_list_in_history() {
    let (s, peer) = service_with_friend();
    let att = json!({
        "merkle_root": "11".repeat(32),
        "name": "photo.png",
        "size": 2048,
        "mime": "image/png",
    });
    s.call(
        "dm.send",
        json!({"pubkey": peer, "text": "ci-joint", "attachments": [att]}),
    )
    .await
    .unwrap();
    let hist = s.call("dm.history", json!({"pubkey": peer})).await.unwrap();
    let m = &hist["messages"][0];
    assert_eq!(m["attachments"], json!([att]));
    // Le compteur du corps reste cohérent.
    assert_eq!(m["body"]["attachments"], json!(1));
}

#[tokio::test]
async fn group_send_with_attachments_renders_list_in_history() {
    let (s, gid) = service_with_group().await;
    let chan = s
        .call(
            "groups.channel.add",
            json!({"group_id": gid, "name": "général"}),
        )
        .await
        .unwrap();
    let cid = chan["channel_id"].as_str().unwrap().to_string();
    let att = json!({
        "merkle_root": "22".repeat(32),
        "name": "plan.pdf",
        "size": 4096,
        "mime": "application/pdf",
    });
    // Sans texte mais avec pièce jointe : accepté.
    s.call(
        "groups.send",
        json!({"group_id": gid, "channel_id": cid, "text": "", "attachments": [att]}),
    )
    .await
    .unwrap();
    let hist = s
        .call(
            "groups.history",
            json!({"group_id": gid, "channel_id": cid}),
        )
        .await
        .unwrap();
    assert_eq!(hist["messages"][0]["attachments"], json!([att]));
}

#[tokio::test]
async fn attachments_are_validated_at_boundary() {
    let (s, peer) = service_with_friend();
    let att = |root: &str| {
        json!({
            "merkle_root": root,
            "name": "x.bin",
            "size": 1,
            "mime": "application/octet-stream",
        })
    };
    // merkle_root non hexadécimal.
    let err = s
        .call(
            "dm.send",
            json!({"pubkey": peer, "text": "x", "attachments": [att("zz")]}),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code, accord_api::rpc::INVALID_PARAMS);
    // Plus de 10 pièces jointes.
    let many: Vec<Value> = (0..11).map(|_| att(&"33".repeat(32))).collect();
    let err = s
        .call(
            "dm.send",
            json!({"pubkey": peer, "text": "x", "attachments": many}),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code, accord_api::rpc::INVALID_PARAMS);
    // Liste mal typée.
    let err = s
        .call(
            "dm.send",
            json!({"pubkey": peer, "text": "x", "attachments": "oops"}),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code, accord_api::rpc::INVALID_PARAMS);
}

// ---- Icône de groupe : validations de la frontière ----

#[tokio::test]
async fn group_set_icon_validates_before_publishing() {
    let (s, gid) = service_with_group().await;
    // base64 invalide.
    let err = s
        .call(
            "groups.set_icon",
            json!({"group_id": gid, "mime": "image/png", "data_b64": "??!!"}),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code, accord_api::rpc::INVALID_PARAMS);
    // MIME non image.
    let err = s
        .call(
            "groups.set_icon",
            json!({"group_id": gid, "mime": "text/plain", "data_b64": "QUJD"}),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code, accord_api::rpc::INVALID_PARAMS);
    // Trop lourd une fois décodé (> 512 Kio).
    let big = "A".repeat(700 * 1024);
    let err = s
        .call(
            "groups.set_icon",
            json!({"group_id": gid, "mime": "image/png", "data_b64": big}),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code, accord_api::rpc::INVALID_PARAMS);
}

// ---- Émojis de serveur, marques de lecture, présence (B3) ----

/// Service adossé à une base sur disque (le magasin de fichiers, requis par
/// les émojis, exige un profil réel) avec un groupe créé.
async fn service_on_disk_with_group() -> (NodeService, String, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("accord.db"), &[1u8; 32]).unwrap();
    let id = Identity::generate_with_pow_bits(1);
    let s = NodeService::new(Arc::new(Node::new(id, db, OutboundSink::null())));
    let created = s
        .call("groups.create", json!({"name": "Guilde"}))
        .await
        .unwrap();
    let gid = created["group_id"].as_str().unwrap().to_string();
    (s, gid, dir)
}

#[tokio::test]
async fn group_emoji_add_del_and_state_shape() {
    let (s, gid, _dir) = service_on_disk_with_group().await;
    // Ajout : rend la racine Merkle de l'image publiée.
    let added = s
        .call(
            "groups.emoji.add",
            json!({"group_id": gid, "name": "parrot", "mime": "image/png", "data_b64": "QUJD"}),
        )
        .await
        .unwrap();
    let root = added["merkle_root"].as_str().unwrap().to_string();
    assert_eq!(root.len(), 64);
    // `groups.state` matérialise l'émoji : `[{ name, merkle_root }]`.
    let state = s
        .call("groups.state", json!({"group_id": gid}))
        .await
        .unwrap();
    assert_eq!(
        state["emojis"],
        json!([{ "name": "parrot", "merkle_root": root }])
    );
    // Suppression : l'état redevient vide.
    s.call(
        "groups.emoji.del",
        json!({"group_id": gid, "name": "parrot"}),
    )
    .await
    .unwrap();
    let state = s
        .call("groups.state", json!({"group_id": gid}))
        .await
        .unwrap();
    assert_eq!(state["emojis"], json!([]));
}

#[tokio::test]
async fn group_emoji_add_validates_at_boundary() {
    let (s, gid, _dir) = service_on_disk_with_group().await;
    // Nom invalide (majuscule).
    let err = s
        .call(
            "groups.emoji.add",
            json!({"group_id": gid, "name": "Bad", "mime": "image/png", "data_b64": "QUJD"}),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code, accord_api::rpc::INVALID_PARAMS);
    // MIME non pris en charge.
    let err = s
        .call(
            "groups.emoji.add",
            json!({"group_id": gid, "name": "ok", "mime": "image/svg+xml", "data_b64": "QUJD"}),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code, accord_api::rpc::INVALID_PARAMS);
    // Décodé trop lourd (> 256 Kio).
    let big = "A".repeat(400 * 1024);
    let err = s
        .call(
            "groups.emoji.add",
            json!({"group_id": gid, "name": "big", "mime": "image/png", "data_b64": big}),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code, accord_api::rpc::INVALID_PARAMS);
}

#[tokio::test]
async fn groups_list_reports_unread_and_mark_read_clears_it() {
    let (s, gid) = service_with_group().await;
    // Aucun message : `unread` présent et vide.
    let list = s.call("groups.list", json!({})).await.unwrap();
    assert_eq!(list["groups"], json!([gid]));
    assert_eq!(list["unread"], json!({}));
    // `groups.mark_read` accepte une position et rend `{ ok: true }`.
    let chan = s
        .call(
            "groups.channel.add",
            json!({"group_id": gid, "name": "général"}),
        )
        .await
        .unwrap();
    let cid = chan["channel_id"].as_str().unwrap().to_string();
    let ok = s
        .call(
            "groups.mark_read",
            json!({"group_id": gid, "channel_id": cid, "lamport": 5}),
        )
        .await
        .unwrap();
    assert_eq!(ok, json!({ "ok": true }));
}

#[tokio::test]
async fn dm_mark_read_ok_and_friends_list_has_presence_and_unread() {
    let (s, peer) = service_with_friend();
    // `dm.mark_read` rend `{ ok: true }`.
    let ok = s
        .call("dm.mark_read", json!({"pubkey": peer, "lamport": 3}))
        .await
        .unwrap();
    assert_eq!(ok, json!({ "ok": true }));
    // `friends.list` porte `online` (bool) et `unread` (entier) par contact.
    let list = s.call("friends.list", json!({})).await.unwrap();
    let contact = &list["contacts"][0];
    assert!(contact["online"].is_boolean());
    assert_eq!(contact["unread"], json!(0));
}

#[tokio::test]
async fn dm_typing_to_offline_peer_is_accepted_silently() {
    let (s, peer) = service_with_friend();
    // Le pair n'est pas présumé en ligne : aucune émission, mais la méthode
    // réussit (« silencieusement ignoré »).
    let ok = s.call("dm.typing", json!({"pubkey": peer})).await.unwrap();
    assert_eq!(ok, json!({ "ok": true }));
}

#[test]
fn b64_decode_strict_roundtrip() {
    use super::helpers::b64_decode;
    assert_eq!(b64_decode("QUJD").as_deref(), Some(b"ABC".as_slice()));
    assert_eq!(b64_decode("QUI=").as_deref(), Some(b"AB".as_slice()));
    assert_eq!(b64_decode("QQ==").as_deref(), Some(b"A".as_slice()));
    assert_eq!(b64_decode(""), None);
    assert_eq!(b64_decode("QUJ"), None, "longueur non multiple de 4");
    assert_eq!(b64_decode("Q?=="), None, "alphabet hors bornes");
    assert_eq!(b64_decode("QQ==QQ=="), None, "remplissage en milieu");
}

// ---- Frontière JSON : contrat gelé des salons vocaux (D-025) ----

/// Puits d'envoi voix inerte (les trames n'ont nulle part où aller).
struct NoopSender;

#[async_trait::async_trait]
impl crate::voice::FrameSender for NoopSender {
    async fn send_voice(&self, _to: &[u8; 32], _msg: accord_proto::plaintext::VoiceMsg) -> bool {
        true
    }
}

/// Service avec moteur voix simulé et un groupe créé ; rend aussi le
/// nœud partagé, `group_id` (hex) et la clé publique locale (hex).
fn service_with_voice() -> (NodeService, Arc<Node>, String, String) {
    let id = Identity::generate_with_pow_bits(1);
    let db = Db::open_in_memory(&[1u8; 32]).unwrap();
    let node = Arc::new(Node::new(id, db, OutboundSink::null()));
    let gid = node.group_create("Guilde").unwrap();
    let me = hex::encode(&node.public_key());
    let voice = crate::voice::spawn(crate::voice::VoiceDeps {
        node: Arc::clone(&node),
        outbound: OutboundSink::null(),
        hub: None,
        sender: Arc::new(NoopSender),
        backend: crate::voice::VoiceBackend::Simule,
    });
    (
        NodeService::new(Arc::clone(&node)).with_voice(voice),
        node,
        gid,
        me,
    )
}

#[tokio::test]
async fn voice_methods_require_the_subsystem() {
    let s = service();
    let err = s.call("voice.status", json!({})).await.unwrap_err();
    assert!(err.message.contains("voix"));
}

#[tokio::test]
async fn voice_join_status_mute_leave_exact_shapes() {
    let (s, _, gid, me) = service_with_voice();

    // join : channel_id == group_id (convention UI, clé opaque ici).
    let joined = s
        .call("voice.join", json!({"group_id": gid, "channel_id": gid}))
        .await
        .unwrap();
    assert_eq!(joined, json!({ "participants": [me] }));

    // status : forme exacte du contrat gelé (étendu de façon additive :
    // deafen et volumes).
    let status = s.call("voice.status", json!({})).await.unwrap();
    assert_eq!(
        status,
        json!({
            "active": {
                "group_id": gid,
                "channel_id": gid,
                "muted": false,
                "deafened": false,
                "participants": [{
                    "pubkey": me,
                    "speaking": false,
                    "muted": false,
                    "deafened": false,
                    "volume": 100,
                }],
            },
            "master_volume": 100,
        })
    );

    // mute : on reste dans le salon.
    assert_eq!(
        s.call("voice.mute", json!({"muted": true})).await.unwrap(),
        json!({})
    );
    let status = s.call("voice.status", json!({})).await.unwrap();
    assert_eq!(status["active"]["muted"], json!(true));

    // deafen : force le mute ; undeafen restaure l'état demandé.
    assert_eq!(
        s.call("voice.deafen", json!({"on": true})).await.unwrap(),
        json!({})
    );
    let status = s.call("voice.status", json!({})).await.unwrap();
    assert_eq!(status["active"]["deafened"], json!(true));
    assert_eq!(status["active"]["muted"], json!(true));
    assert_eq!(
        s.call("voice.deafen", json!({"on": false})).await.unwrap(),
        json!({})
    );
    let status = s.call("voice.status", json!({})).await.unwrap();
    assert_eq!(status["active"]["deafened"], json!(false));
    assert_eq!(status["active"]["muted"], json!(true));

    // set_volume : maître (sans `peer`) puis par pair, reflétés au statut.
    assert_eq!(
        s.call("voice.set_volume", json!({"volume": 150}))
            .await
            .unwrap(),
        json!({})
    );
    assert_eq!(
        s.call("voice.set_volume", json!({"peer": me, "volume": 40}))
            .await
            .unwrap(),
        json!({})
    );
    let status = s.call("voice.status", json!({})).await.unwrap();
    assert_eq!(status["master_volume"], json!(150));
    assert_eq!(status["active"]["participants"][0]["volume"], json!(40));

    // leave : plus de salon actif (le volume maître reste exposé).
    assert_eq!(s.call("voice.leave", json!({})).await.unwrap(), json!({}));
    let status = s.call("voice.status", json!({})).await.unwrap();
    assert_eq!(status, json!({ "active": null, "master_volume": 150 }));
}

#[tokio::test]
async fn voice_params_are_validated_at_boundary() {
    let (s, _, gid, _) = service_with_voice();
    let err = s
        .call("voice.join", json!({"group_id": "zz", "channel_id": gid}))
        .await
        .unwrap_err();
    assert_eq!(err.code, accord_api::rpc::INVALID_PARAMS);
    let err = s.call("voice.mute", json!({})).await.unwrap_err();
    assert_eq!(err.code, accord_api::rpc::INVALID_PARAMS);
    let err = s.call("voice.deafen", json!({})).await.unwrap_err();
    assert_eq!(err.code, accord_api::rpc::INVALID_PARAMS);
    let err = s.call("voice.set_volume", json!({})).await.unwrap_err();
    assert_eq!(err.code, accord_api::rpc::INVALID_PARAMS);
    let err = s
        .call("voice.set_volume", json!({"volume": 201}))
        .await
        .unwrap_err();
    assert_eq!(err.code, accord_api::rpc::INVALID_PARAMS);
    let err = s
        .call("voice.set_volume", json!({"peer": "zz", "volume": 100}))
        .await
        .unwrap_err();
    assert_eq!(err.code, accord_api::rpc::INVALID_PARAMS);
    let err = s.call("voice.nexiste", json!({})).await.unwrap_err();
    assert_eq!(err.code, accord_api::rpc::INVALID_PARAMS);
}

// ---- Frontière JSON : périphériques audio et test micro (D-029) ----

#[tokio::test]
async fn voice_devices_exact_shape_in_simulated_mode() {
    let (s, _, _, _) = service_with_voice();
    // Forme exacte du contrat gelé : listes vides, sélections null.
    assert_eq!(
        s.call("voice.devices", json!({})).await.unwrap(),
        json!({
            "inputs": [],
            "outputs": [],
            "selected_input": null,
            "selected_output": null,
        })
    );
}

#[tokio::test]
async fn voice_set_devices_persists_and_resets() {
    let (s, node, _, _) = service_with_voice();
    // Chaîne = nom cpal ; champ absent = inchangé. Résultat vide exact.
    assert_eq!(
        s.call("voice.set_devices", json!({ "input": "Micro USB" }))
            .await
            .unwrap(),
        json!({})
    );
    assert_eq!(
        node.voice_devices_config().unwrap(),
        (Some("Micro USB".into()), None)
    );
    s.call("voice.set_devices", json!({ "output": "Casque" }))
        .await
        .unwrap();
    assert_eq!(
        node.voice_devices_config().unwrap(),
        (Some("Micro USB".into()), Some("Casque".into()))
    );
    // null = retour au périphérique par défaut ; l'autre champ reste.
    s.call("voice.set_devices", json!({ "input": null }))
        .await
        .unwrap();
    assert_eq!(
        node.voice_devices_config().unwrap(),
        (None, Some("Casque".into()))
    );
    // Sans matériel, la sélection rendue reste null (contrat gelé).
    let devices = s.call("voice.devices", json!({})).await.unwrap();
    assert_eq!(devices["selected_output"], json!(null));
    // Requête vide : aucun changement, résultat vide.
    assert_eq!(
        s.call("voice.set_devices", json!({})).await.unwrap(),
        json!({})
    );
    assert_eq!(
        node.voice_devices_config().unwrap(),
        (None, Some("Casque".into()))
    );
}

#[tokio::test]
async fn voice_set_devices_rejects_bad_params_at_boundary() {
    let (s, node, _, _) = service_with_voice();
    // Type invalide : refusé à la frontière.
    let err = s
        .call("voice.set_devices", json!({ "input": 42 }))
        .await
        .unwrap_err();
    assert_eq!(err.code, accord_api::rpc::INVALID_PARAMS);
    // Nom vide : refusé par la validation, rien n'est persisté.
    let err = s
        .call("voice.set_devices", json!({ "input": "" }))
        .await
        .unwrap_err();
    assert_eq!(err.code, accord_api::rpc::INVALID_PARAMS);
    assert_eq!(node.voice_devices_config().unwrap(), (None, None));
}

#[tokio::test]
async fn voice_mic_test_is_explicitly_unavailable_without_hardware() {
    let (s, _, _, _) = service_with_voice();
    // Activation : erreur explicite du contrat gelé en mode simulé.
    let err = s
        .call("voice.mic_test", json!({ "enabled": true }))
        .await
        .unwrap_err();
    assert!(
        err.message.contains("matériel audio indisponible"),
        "message : {}",
        err.message
    );
    // Désactivation : idempotente, résultat vide exact.
    assert_eq!(
        s.call("voice.mic_test", json!({ "enabled": false }))
            .await
            .unwrap(),
        json!({})
    );
    // Paramètre manquant ou mal typé : refus à la frontière.
    let err = s.call("voice.mic_test", json!({})).await.unwrap_err();
    assert_eq!(err.code, accord_api::rpc::INVALID_PARAMS);
}

// ---- Frontière JSON : mentions (boîte locale) et notes privées ----

#[tokio::test]
async fn mentions_inbox_and_mark_read_roundtrip() {
    let (node, peer) = node_with_friend();
    let peer_hex = hex::encode(&peer.public_key());
    node.profile_set_name("Anna").unwrap();
    // Le pair nous envoie un DM qui nous mentionne.
    let body = MsgBody::Text {
        text: "coucou @Anna".into(),
        reply_to: None,
        attachments: vec![],
    };
    node.ingest_core(
        &peer.public_key(),
        CoreMsg::DirectMsg {
            msg_id: [5; 16],
            lamport: 3,
            sent_ms: 1_003,
            kind: body.kind(),
            body: body.encode_body(),
        },
    )
    .unwrap();
    let s = NodeService::new(node);

    let inbox = s.call("mentions.inbox", json!({})).await.unwrap();
    let entries = inbox["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    let e = &entries[0];
    // Forme exacte d'une entrée.
    assert_eq!(
        sorted_keys(e),
        [
            "author",
            "conversation",
            "lamport",
            "msg_id",
            "read",
            "snippet",
            "ts_ms"
        ]
    );
    assert_eq!(e["conversation"]["kind"], json!("dm"));
    assert_eq!(e["conversation"]["peer"], json!(peer_hex));
    assert_eq!(e["msg_id"], json!(hex::encode(&[5u8; 16])));
    assert_eq!(e["author"], json!(peer_hex));
    assert_eq!(e["read"], json!(false));
    assert_eq!(e["snippet"], json!("coucou @Anna"));

    // Marquer tout comme lu.
    let res = s.call("mentions.mark_read", json!({})).await.unwrap();
    assert_eq!(res["marked"], json!(1));
    let inbox2 = s.call("mentions.inbox", json!({})).await.unwrap();
    assert_eq!(inbox2["entries"][0]["read"], json!(true));

    // Le compteur de mentions du DM apparaît dans friends.list.
    let list = s.call("friends.list", json!({})).await.unwrap();
    assert_eq!(list["contacts"][0]["mention_count"], json!(0));
}

#[tokio::test]
async fn friends_note_set_get_and_folded_in_list() {
    let (s, peer) = service_with_friend();
    // Aucune note au départ.
    let got = s
        .call("friends.get_note", json!({ "pubkey": peer }))
        .await
        .unwrap();
    assert_eq!(got["note"], json!(null));

    // Écriture (rognée), relecture.
    s.call(
        "friends.set_note",
        json!({ "pubkey": peer, "note": "  vieil ami  " }),
    )
    .await
    .unwrap();
    let got = s
        .call("friends.get_note", json!({ "pubkey": peer }))
        .await
        .unwrap();
    assert_eq!(got["note"], json!("vieil ami"));

    // friends.list replie la note et le compteur de mentions.
    let list = s.call("friends.list", json!({})).await.unwrap();
    let c = &list["contacts"][0];
    assert_eq!(c["note"], json!("vieil ami"));
    assert_eq!(c["mention_count"], json!(0));

    // Une note vide efface l'entrée.
    s.call("friends.set_note", json!({ "pubkey": peer, "note": "" }))
        .await
        .unwrap();
    let got = s
        .call("friends.get_note", json!({ "pubkey": peer }))
        .await
        .unwrap();
    assert_eq!(got["note"], json!(null));
}
