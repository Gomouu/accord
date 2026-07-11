//! Tests d'intégration des appels vocaux 1-à-1 et de la modération vocale :
//! deux nœuds complets (UDP réel, mode voix simulé) établissent une amitié,
//! s'appellent (sonnerie → acceptation → trames audio → raccrochage), se
//! refusent un appel, et un modérateur force la sourdine d'un membre dans un
//! salon vocal de groupe (op 0x1F appliquée des deux côtés).

use std::time::Duration;

use accord_node::{identity, run, CallPhase, NodeConfig, Paths, RunningNode, VoiceBackend};
use accord_voice::params::FRAME_SAMPLES;
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
        voice_backend: VoiceBackend::Simule,
        ..NodeConfig::default()
    };
    run(unlocked, config).await.unwrap()
}

/// Attend qu'une condition asynchrone devienne vraie (borne dure ~10 s).
async fn eventually<F, Fut>(mut cond: F) -> bool
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    for _ in 0..200 {
        if cond().await {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    cond().await
}

/// Trame de parole : alternance pleine d'amplitude, au-dessus du seuil VAD.
fn tone() -> Vec<i16> {
    (0..FRAME_SAMPLES)
        .map(|i| if i % 2 == 0 { 20_000 } else { -20_000 })
        .collect()
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

/// Draine le WebSocket jusqu'à voir l'événement `method` satisfaisant le
/// prédicat sur ses paramètres (borne dure ~10 s) ; rend les paramètres.
async fn wait_event(
    ws: &mut WsClient,
    method: &str,
    pred: impl Fn(&Value) -> bool,
) -> Option<Value> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let next = tokio::time::timeout_at(deadline, ws.next()).await;
        let Ok(Some(Ok(Message::Text(text)))) = next else {
            return None;
        };
        let Ok(v) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        if v["method"] == method && pred(&v["params"]) {
            return Some(v["params"].clone());
        }
    }
}

/// Amorce deux nœuds amis (adresses enregistrées, amitié confirmée).
async fn befriended_pair(
    dir_a: &std::path::Path,
    dir_b: &std::path::Path,
) -> (RunningNode, RunningNode) {
    let alice = boot(dir_a).await;
    let bob = boot(dir_b).await;
    let alice_pub = alice.node.public_key();
    let bob_pub = bob.node.public_key();
    alice.register_peer(bob_pub, bob.p2p_addr());
    bob.register_peer(alice_pub, alice.p2p_addr());
    alice.node.friend_request(&bob_pub, "Alice").unwrap();
    assert!(
        eventually(|| async {
            bob.node
                .contacts()
                .map(|cs| cs.iter().any(|c| c.pubkey == alice_pub))
                .unwrap_or(false)
        })
        .await,
        "Bob n'a pas reçu la demande d'ami"
    );
    bob.node.friend_respond(&alice_pub, true).unwrap();
    assert!(
        eventually(|| async {
            alice
                .node
                .friend_pubkeys()
                .map(|f| f.contains(&bob_pub))
                .unwrap_or(false)
        })
        .await,
        "l'amitié n'a pas été confirmée chez Alice"
    );
    (alice, bob)
}

#[tokio::test]
async fn one_to_one_call_rings_connects_carries_audio_and_hangs_up() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let (alice, bob) = befriended_pair(dir_a.path(), dir_b.path()).await;
    let alice_pub = alice.node.public_key();
    let bob_pub = bob.node.public_key();
    let alice_hex = accord_node::hex::encode(&alice_pub);

    let mut ws_bob = ws_client(&bob).await;
    let mut ws_alice = ws_client(&alice).await;

    // Alice appelle Bob : sonnerie sortante chez elle, entrante chez lui.
    let call_id = alice.voice().call_start(bob_pub).await.unwrap();
    let call_hex = accord_node::hex::encode(&call_id);
    assert_eq!(
        alice.voice().call_status().await.unwrap().phase,
        CallPhase::OutgoingRinging
    );
    let incoming = wait_event(&mut ws_bob, "event.call_incoming", |p| {
        p["peer"] == json!(alice_hex)
    })
    .await
    .expect("event.call_incoming non reçu chez Bob");
    assert_eq!(incoming["call_id"], json!(call_hex));
    let snapshot = bob.voice().call_status().await.unwrap();
    assert_eq!(snapshot.phase, CallPhase::IncomingRinging);
    assert_eq!(snapshot.call_id, Some(call_id));

    // Pendant la sonnerie, l'état « occupé » est opposé à un autre appel.
    let err = bob.voice().call_start(alice_pub).await.unwrap_err();
    assert!(err.to_string().contains("en cours"), "erreur : {err}");

    // Bob accepte : session audio active des deux côtés (salon = call_id).
    bob.voice().call_accept(call_id).await.unwrap();
    assert!(
        eventually(|| async {
            alice.voice().call_status().await.unwrap().phase == CallPhase::Active
        })
        .await,
        "l'appel n'est pas devenu actif chez Alice"
    );
    assert!(
        wait_event(&mut ws_alice, "event.call_accepted", |p| {
            p["call_id"] == json!(call_hex)
        })
        .await
        .is_some(),
        "event.call_accepted non reçu chez Alice"
    );
    for side in [&alice, &bob] {
        let status = side.voice().status().await.unwrap().unwrap();
        assert!(status.is_call, "session active non marquée appel");
        assert_eq!(status.channel_id, call_id);
    }

    // La parole injectée chez Alice traverse la session d'appel.
    assert!(
        eventually(|| async {
            for _ in 0..5 {
                alice.voice().inject_pcm(tone());
            }
            match bob.voice().status().await {
                Ok(Some(s)) => s
                    .participants
                    .iter()
                    .any(|p| p.pubkey == alice_pub && p.speaking),
                _ => false,
            }
        })
        .await,
        "les trames d'appel d'Alice ne sont pas arrivées chez Bob"
    );

    // Bob raccroche : fin d'appel des deux côtés, sessions fermées.
    bob.voice().call_hangup().await.unwrap();
    let ended = wait_event(&mut ws_alice, "event.call_ended", |p| {
        p["call_id"] == json!(call_hex)
    })
    .await
    .expect("event.call_ended non reçu chez Alice");
    assert_eq!(ended["reason"], json!("hangup"));
    assert!(
        eventually(|| async {
            alice.voice().call_status().await.unwrap().phase == CallPhase::Idle
                && alice.voice().status().await.unwrap().is_none()
        })
        .await,
        "l'appel ne s'est pas terminé chez Alice"
    );
    assert_eq!(
        bob.voice().call_status().await.unwrap().phase,
        CallPhase::Idle
    );
    assert!(bob.voice().status().await.unwrap().is_none());

    // Second appel : Bob refuse, Alice apprend le refus.
    let call2 = alice.voice().call_start(bob_pub).await.unwrap();
    let call2_hex = accord_node::hex::encode(&call2);
    assert!(
        eventually(|| async {
            bob.voice().call_status().await.unwrap().phase == CallPhase::IncomingRinging
        })
        .await,
        "le second appel ne sonne pas chez Bob"
    );
    bob.voice().call_decline(call2).await.unwrap();
    let ended = wait_event(&mut ws_alice, "event.call_ended", |p| {
        p["call_id"] == json!(call2_hex)
    })
    .await
    .expect("event.call_ended (refus) non reçu chez Alice");
    assert_eq!(ended["reason"], json!("declined"));
    assert_eq!(
        alice.voice().call_status().await.unwrap().phase,
        CallPhase::Idle
    );

    alice.shutdown();
    bob.shutdown();
}

#[tokio::test]
async fn server_voice_moderation_silences_a_member_across_the_network() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let (alice, bob) = befriended_pair(dir_a.path(), dir_b.path()).await;
    let alice_pub = alice.node.public_key();
    let bob_pub = bob.node.public_key();

    // Groupe partagé (Alice fondatrice), adhésion consentie de Bob.
    let gid: [u8; 16] =
        accord_node::hex::decode(&alice.node.group_create("Guilde").unwrap()).unwrap();
    let invite_id: [u8; 16] =
        accord_node::hex::decode(&alice.node.group_invite_create(&gid, &bob_pub).unwrap()).unwrap();
    assert!(
        eventually(|| async {
            bob.node
                .group_invites_list()
                .map(|invites| invites.iter().any(|i| i.invite_id == invite_id))
                .unwrap_or(false)
        })
        .await,
        "Bob n'a pas reçu le ticket d'invitation"
    );
    bob.node.group_invite_accept(&gid, &invite_id).unwrap();
    assert!(
        eventually(|| async {
            bob.node
                .group_state(&gid)
                .map(|s| s.is_member(&bob_pub))
                .unwrap_or(false)
        })
        .await,
        "Bob n'a pas matérialisé le groupe"
    );

    // Les deux rejoignent le salon vocal par défaut.
    alice.voice().join(gid, gid).await.unwrap();
    bob.voice().join(gid, gid).await.unwrap();
    assert!(
        eventually(|| async {
            match alice.voice().status().await {
                Ok(Some(s)) => s.participants.iter().any(|p| p.pubkey == bob_pub),
                _ => false,
            }
        })
        .await,
        "Alice ne voit pas Bob dans le salon"
    );

    // Bob parle : Alice l'entend (indicateur ouvert).
    assert!(
        eventually(|| async {
            for _ in 0..5 {
                bob.voice().inject_pcm(tone());
            }
            match alice.voice().status().await {
                Ok(Some(s)) => s
                    .participants
                    .iter()
                    .any(|p| p.pubkey == bob_pub && p.speaking),
                _ => false,
            }
        })
        .await,
        "les trames de Bob n'arrivent pas chez Alice"
    );

    // Alice (fondatrice, permission KICK) force la sourdine de Bob : l'op
    // 0x1F se réplique ; Bob coupe sa capture à la source, Alice jette ses
    // trames en défense en profondeur — l'indicateur reste fermé.
    alice
        .node
        .group_voice_moderate(&gid, &bob_pub, true, false)
        .unwrap();
    assert!(
        eventually(|| async {
            match bob.voice().status().await {
                Ok(Some(s)) => s
                    .participants
                    .iter()
                    .any(|p| p.pubkey == bob_pub && p.server_muted),
                _ => false,
            }
        })
        .await,
        "la modération n'est pas appliquée chez Bob"
    );
    assert!(
        eventually(|| async {
            for _ in 0..5 {
                bob.voice().inject_pcm(tone());
            }
            match alice.voice().status().await {
                Ok(Some(s)) => s
                    .participants
                    .iter()
                    .any(|p| p.pubkey == bob_pub && !p.speaking),
                _ => false,
            }
        })
        .await,
        "Bob se fait encore entendre après la modération"
    );

    // Un membre simple ne peut PAS modérer : l'op de Bob visant Alice est
    // rejetée à l'émission (rejeu local avant diffusion).
    let err = bob
        .node
        .group_voice_moderate(&gid, &alice_pub, true, true)
        .unwrap_err();
    assert!(
        err.to_string().contains("refusé"),
        "erreur inattendue : {err}"
    );

    // Levée de la modération : Bob se fait entendre à nouveau.
    alice
        .node
        .group_voice_moderate(&gid, &bob_pub, false, false)
        .unwrap();
    assert!(
        eventually(|| async {
            for _ in 0..5 {
                bob.voice().inject_pcm(tone());
            }
            match alice.voice().status().await {
                Ok(Some(s)) => s
                    .participants
                    .iter()
                    .any(|p| p.pubkey == bob_pub && p.speaking),
                _ => false,
            }
        })
        .await,
        "Bob reste muet après la levée de la modération"
    );

    alice.shutdown();
    bob.shutdown();
}
