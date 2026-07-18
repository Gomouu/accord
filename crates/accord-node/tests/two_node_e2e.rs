//! Test d'intégration bout-en-bout : deux nœuds complets (identité chiffrée,
//! transport UDP réel, DHT, cœur, API) échangent une amitié puis un message
//! direct à travers le réseau. Le profil (pseudo, D-027) est vérifié sur le
//! même harnais : échange à l'acceptation d'amitié, propagation d'un
//! changement et événement `event.profile` sur l'API WebSocket.

use std::time::Duration;

use accord_node::{identity, run, NodeConfig, Paths, RunningNode};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::Message;

async fn boot(dir: &std::path::Path) -> RunningNode {
    let paths = Paths::new(dir);
    let unlocked = identity::create(&paths, "phrase-de-passe-test", 1).unwrap();
    let config = NodeConfig {
        paths,
        p2p_addr: "127.0.0.1:0".parse().unwrap(),
        api_port: 0,
        pow_bits: 1,
        mdns_enabled: false,
        ..NodeConfig::default()
    };
    run(unlocked, config).await.unwrap()
}

/// Attend qu'une condition devienne vraie (interrogation courte, borne dure).
async fn eventually(mut cond: impl FnMut() -> bool) -> bool {
    for _ in 0..450 {
        if cond() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    cond()
}

#[tokio::test]
async fn two_nodes_befriend_and_exchange_dm() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let alice = boot(dir_a.path()).await;
    let bob = boot(dir_b.path()).await;

    let alice_pub = alice.node.public_key();
    let bob_pub = bob.node.public_key();

    // Amorçage de présence : chacun connaît l'adresse P2P de l'autre.
    alice.register_peer(bob_pub, bob.p2p_addr());
    bob.register_peer(alice_pub, alice.p2p_addr());

    // Alice demande Bob en ami ; la demande traverse le réseau.
    alice.node.friend_request(&bob_pub, "Alice").unwrap();
    let bob_saw_request = eventually(|| {
        bob.node
            .contacts()
            .map(|cs| cs.iter().any(|c| c.pubkey == alice_pub))
            .unwrap_or(false)
    })
    .await;
    assert!(bob_saw_request, "Bob n'a pas reçu la demande d'ami");

    // Bob accepte ; la réponse revient à Alice.
    bob.node.friend_respond(&alice_pub, true).unwrap();
    let alice_is_friends = eventually(|| {
        use accord_core::db::ContactState;
        alice
            .node
            .contacts()
            .map(|cs| {
                cs.iter()
                    .any(|c| c.pubkey == bob_pub && c.state == ContactState::Friend)
            })
            .unwrap_or(false)
    })
    .await;
    assert!(
        alice_is_friends,
        "l'amitié n'a pas été confirmée chez Alice"
    );

    // Alice envoie un message direct ; Bob le reçoit et l'historise.
    alice
        .node
        .dm_send(&bob_pub, "salut Bob, à travers le réseau", None)
        .unwrap();
    let bob_got_dm = eventually(|| {
        bob.node
            .dm_history(&alice_pub, u64::MAX, 10)
            .map(|h| h.iter().any(|m| m.author == alice_pub))
            .unwrap_or(false)
    })
    .await;
    assert!(bob_got_dm, "Bob n'a pas reçu le message direct");

    // L'accusé applicatif revient à Alice (son message est marqué acquitté).
    let alice_acked = eventually(|| {
        alice
            .node
            .dm_history(&bob_pub, u64::MAX, 10)
            .map(|h| h.iter().any(|m| m.author == alice_pub && m.acked))
            .unwrap_or(false)
    })
    .await;
    assert!(
        alice_acked,
        "l'accusé de réception n'est pas revenu à Alice"
    );

    // ---- Flux groupe complet : création, invitation, clé, messages ----

    // Alice crée un groupe avec un salon.
    let gid_hex = alice.node.group_create("Guilde").unwrap();
    let gid: [u8; 16] = accord_node::hex::decode(&gid_hex).unwrap();
    let chan_hex = alice.node.group_add_channel(&gid, "général").unwrap();
    let chan: [u8; 16] = accord_node::hex::decode(&chan_hex).unwrap();

    // Alice invite Bob : un ticket signé traverse le réseau, Bob l'accepte
    // explicitement (consentement en deux temps, D-045), ce qui déclenche
    // chez Alice la poussée de l'op-log complet et de la clé scellée.
    let invite_id_hex = alice.node.group_invite_create(&gid, &bob_pub).unwrap();
    let invite_id: [u8; 16] = accord_node::hex::decode(&invite_id_hex).unwrap();
    let bob_saw_ticket = eventually(|| {
        bob.node
            .group_invites_list()
            .map(|invites| invites.iter().any(|i| i.invite_id == invite_id))
            .unwrap_or(false)
    })
    .await;
    assert!(bob_saw_ticket, "Bob n'a pas reçu le ticket d'invitation");
    bob.node.group_invite_accept(&gid, &invite_id).unwrap();
    let bob_joined = eventually(|| {
        bob.node
            .group_state(&gid)
            .map(|s| s.is_member(&bob_pub) && s.channels.len() == 1)
            .unwrap_or(false)
    })
    .await;
    assert!(bob_joined, "Bob n'a pas matérialisé l'état du groupe");

    // Alice poste dans le salon ; Bob déchiffre et historise.
    alice
        .node
        .group_send(&gid, &chan, "bienvenue dans la guilde")
        .unwrap();
    let bob_got_group_msg = eventually(|| {
        bob.node
            .group_history(&gid, &chan, u64::MAX, 10)
            .map(|h| h.iter().any(|m| m.author == alice_pub))
            .unwrap_or(false)
    })
    .await;
    assert!(
        bob_got_group_msg,
        "Bob n'a pas déchiffré le message de groupe (clé ou op-log manquant)"
    );

    // Et dans l'autre sens : Bob répond, Alice reçoit.
    bob.node.group_send(&gid, &chan, "merci !").unwrap();
    let alice_got_reply = eventually(|| {
        alice
            .node
            .group_history(&gid, &chan, u64::MAX, 10)
            .map(|h| h.iter().any(|m| m.author == bob_pub))
            .unwrap_or(false)
    })
    .await;
    assert!(alice_got_reply, "Alice n'a pas reçu la réponse de Bob");

    alice.shutdown();
    bob.shutdown();
}

#[tokio::test]
async fn dm_de_20000_caracteres_traverse_le_reseau() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let alice = boot(dir_a.path()).await;
    let bob = boot(dir_b.path()).await;

    let alice_pub = alice.node.public_key();
    let bob_pub = bob.node.public_key();
    alice.register_peer(bob_pub, bob.p2p_addr());
    bob.register_peer(alice_pub, alice.p2p_addr());

    // Amitié préalable (prérequis à l'échange de DM).
    alice.node.friend_request(&bob_pub, "Alice").unwrap();
    let bob_saw = eventually(|| {
        bob.node
            .contacts()
            .map(|cs| cs.iter().any(|c| c.pubkey == alice_pub))
            .unwrap_or(false)
    })
    .await;
    assert!(bob_saw, "Bob n'a pas reçu la demande d'ami");
    bob.node.friend_respond(&alice_pub, true).unwrap();
    let amis = eventually(|| {
        use accord_core::db::ContactState;
        alice
            .node
            .contacts()
            .map(|cs| {
                cs.iter()
                    .any(|c| c.pubkey == bob_pub && c.state == ContactState::Friend)
            })
            .unwrap_or(false)
    })
    .await;
    assert!(amis, "l'amitié n'a pas été confirmée");

    // Un DM de 20 000 caractères : bien au-delà de la MTU applicative de
    // 1 200 o, il traverse le réseau fragmenté puis réassemblé par le transport.
    let texte: String = "é".repeat(10_000) + &"a".repeat(10_000);
    assert_eq!(texte.chars().count(), 20_000);
    alice.node.dm_send(&bob_pub, &texte, None).unwrap();

    // Le corps persisté est le `MsgBody` encodé : on le décode pour comparer le
    // texte exact (intégrité de bout en bout après réassemblage).
    use accord_proto::core_msg::MsgBody;
    let bob_recu = eventually(|| {
        bob.node
            .dm_history(&alice_pub, u64::MAX, 10)
            .map(|h| {
                h.iter().any(|m| {
                    m.author == alice_pub
                        && matches!(
                            MsgBody::decode_body(m.kind, &m.body),
                            Ok(MsgBody::Text { ref text, .. }) if *text == texte
                        )
                })
            })
            .unwrap_or(false)
    })
    .await;
    assert!(
        bob_recu,
        "Bob n'a pas reçu le DM de 20 000 caractères intact"
    );

    alice.shutdown();
    bob.shutdown();
}

type WsClient =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// Client WebSocket authentifié sur l'API d'un nœud (reçoit les événements).
async fn ws_client(node: &RunningNode) -> WsClient {
    let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://{}", node.api_addr()))
        .await
        .unwrap();
    let auth = json!({
        "jsonrpc": "2.0", "id": 0, "method": "auth",
        "params": { "token": node.token.expose() },
    });
    ws.send(Message::Text(auth.to_string())).await.unwrap();
    loop {
        if let Message::Text(text) = ws.next().await.unwrap().unwrap() {
            let v: Value = serde_json::from_str(&text).unwrap();
            assert_eq!(v["result"]["protocole"], 1);
            break;
        }
    }
    ws
}

/// Draine le WebSocket jusqu'à voir `event.profile` avec ce couple
/// `{ pubkey, name }` exact (borne dure ~10 s).
async fn wait_profile_event(ws: &mut WsClient, pubkey: &str, name: &str) -> bool {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let next = tokio::time::timeout_at(deadline, ws.next()).await;
        let Ok(Some(Ok(Message::Text(text)))) = next else {
            return false;
        };
        let Ok(v) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        if v["method"] == "event.profile"
            && v["params"]["pubkey"] == pubkey
            && v["params"]["name"] == name
        {
            return true;
        }
    }
}

/// Nom d'affichage d'un contact tel que rendu par `friends.list`.
fn contact_name(node: &RunningNode, peer: &[u8; 32]) -> Option<String> {
    node.node
        .contacts()
        .ok()?
        .iter()
        .find(|c| c.pubkey == *peer)
        .map(|c| c.display_name.clone())
}

#[tokio::test]
async fn profile_names_exchange_on_friendship_and_propagate() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let alice = boot(dir_a.path()).await;
    let bob = boot(dir_b.path()).await;

    let alice_pub = alice.node.public_key();
    let bob_pub = bob.node.public_key();
    let alice_hex = accord_node::hex::encode(&alice_pub);

    alice.register_peer(bob_pub, bob.p2p_addr());
    bob.register_peer(alice_pub, alice.p2p_addr());

    // Pseudos définis avant l'amitié (aucun ami : rien n'est émis).
    alice.node.profile_set_name("Alice Prime").unwrap();
    bob.node.profile_set_name("Bob le Bricoleur").unwrap();

    // Client WebSocket de Bob : observera les event.profile.
    let mut ws_bob = ws_client(&bob).await;

    // Amitié : demande d'Alice (étiquette locale « pair-b »), Bob accepte.
    alice.node.friend_request(&bob_pub, "pair-b").unwrap();
    let bob_saw_request = eventually(|| {
        bob.node
            .contacts()
            .map(|cs| cs.iter().any(|c| c.pubkey == alice_pub))
            .unwrap_or(false)
    })
    .await;
    assert!(bob_saw_request, "Bob n'a pas reçu la demande d'ami");
    bob.node.friend_respond(&alice_pub, true).unwrap();

    // À l'acceptation, les pseudos s'échangent dans les deux sens : Alice
    // voit le pseudo de Bob (l'étiquette locale « pair-b » est remplacée)…
    let alice_sees_bob =
        eventually(|| contact_name(&alice, &bob_pub).as_deref() == Some("Bob le Bricoleur")).await;
    assert!(alice_sees_bob, "Alice n'a pas reçu le pseudo de Bob");
    // … et Bob voit celui d'Alice dans friends.list.
    let bob_sees_alice =
        eventually(|| contact_name(&bob, &alice_pub).as_deref() == Some("Alice Prime")).await;
    assert!(bob_sees_alice, "Bob n'a pas reçu le pseudo d'Alice");

    // Changement de pseudo après l'amitié : propagé à Bob, qui reçoit
    // event.profile { pubkey, name } et voit friends.list mis à jour.
    alice.node.profile_set_name("Alice Seconde").unwrap();
    assert!(
        wait_profile_event(&mut ws_bob, &alice_hex, "Alice Seconde").await,
        "event.profile non reçu sur l'API de Bob"
    );
    let bob_sees_update =
        eventually(|| contact_name(&bob, &alice_pub).as_deref() == Some("Alice Seconde")).await;
    assert!(bob_sees_update, "friends.list de Bob n'est pas à jour");

    alice.shutdown();
    bob.shutdown();
}
