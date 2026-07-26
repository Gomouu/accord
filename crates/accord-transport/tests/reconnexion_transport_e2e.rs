//! Isolated root-cause regressions for reconnection reliability (Lot G).
//!
//! Each test drives the endpoint on the deterministic simulated mesh with a
//! manual clock and pins ONE mechanism red→green:
//!   - cause 1: a dial keeps retrying within the reconnection attempt instead of
//!     giving up after the old ~6 s window;
//!   - cause 2: a lost WELCOME recovers because the retry uses a FRESH nonce
//!     (an identical retransmission would be eaten by the responder's replay
//!     cache);
//!   - cause 4: a stale direct session from the peer's previous incarnation is
//!     evicted when the fresh one is established.
//!
//! Le dernier test de ce fichier n'est pas une cause racine de la reconnexion
//! mais la moitié TRANSPORT de la campagne de mobilité (§9.3, voir
//! `accord-node/tests/chaos_reseau_e2e.rs`) : un pair qui change d'adresse sans
//! redémarrer. Il vit ici parce qu'on y voit les ÉVÉNEMENTS de session, et que
//! la seule chose qui distingue une session déplacée d'une session refaite en
//! douce est l'absence de nouvelle poignée de main — le résultat visible, lui,
//! est le même dans les deux cas.

use accord_crypto::Identity;
use accord_proto::plaintext::ChannelMsg;
use accord_proto::ControlMsg;
use accord_transport::clock::ManualClock;
use accord_transport::endpoint::{Endpoint, EndpointConfig, TransportEvent};
use accord_transport::socket::sim::{NetConditions, SimNet};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

const POW: u32 = 4;

fn config() -> EndpointConfig {
    EndpointConfig {
        pow_bits: POW,
        keepalive_ms: 25_000,
        idle_timeout_ms: 120_000,
        cookie_pressure_per_s: 64,
        relay_serving: false,
        capabilities: None,
    }
}

struct Node {
    ep: Arc<Endpoint>,
    events: mpsc::UnboundedReceiver<TransportEvent>,
    addr: SocketAddr,
    static_pub: [u8; 32],
}

fn spawn_node(net: &SimNet, clock: &ManualClock, addr: &str) -> Node {
    spawn_node_avec_identite(
        net,
        clock,
        addr,
        Arc::new(Identity::generate_with_pow_bits(POW)),
    )
}

/// Imposed-identity variant: needed to simulate a peer RESTART (same static
/// key, new address).
fn spawn_node_avec_identite(
    net: &SimNet,
    clock: &ManualClock,
    addr: &str,
    id: Arc<Identity>,
) -> Node {
    let addr: SocketAddr = addr.parse().unwrap();
    let socket = Arc::new(net.bind(addr));
    let static_pub = id.public_key();
    let (ep, events) = Endpoint::new(
        socket,
        id,
        Arc::new(clock.clone()) as Arc<dyn accord_transport::Clock>,
        config(),
    );
    ep.spawn();
    Node {
        ep,
        events,
        addr,
        static_pub,
    }
}

/// Advances the clock and runs one maintenance pass, then yields real time so
/// the receive loops process any HELLO/WELCOME produced.
async fn tick(node: &Node, clock: &ManualClock) {
    clock.advance(2_500);
    node.ep.run_maintenance().await;
    tokio::time::sleep(Duration::from_millis(40)).await;
}

/// Cause 1: the reconnection attempt must survive past the old abandon window.
/// While the peer is unreachable, every HELLO is dropped; the pending must NOT
/// be given up, so once the peer returns the session still forms.
#[tokio::test]
async fn le_dial_persiste_au_dela_de_l_ancienne_fenetre_d_abandon() {
    let clock = ManualClock::new(1_000_000);
    let net = SimNet::new(1, NetConditions::default());
    let alice = spawn_node(&net, &clock, "10.0.10.1:4000");
    let bob = spawn_node(&net, &clock, "10.0.10.2:4000");

    // Alice unreachable: everything addressed TO her is dropped.
    net.set_conditions(
        alice.addr,
        NetConditions {
            loss: 1.0,
            latency_min_ms: 0,
            latency_max_ms: 0,
        },
    );

    bob.ep.connect(alice.addr).await.unwrap();
    // Drive well past the old 3-retransmission (~6 s) abandon threshold.
    for _ in 0..5 {
        tick(&bob, &clock).await;
    }
    assert_eq!(
        bob.ep.session_count(),
        0,
        "aucune session ne peut se former tant qu'Alice est injoignable"
    );

    // Alice returns: the persistent pending must reconnect on its own.
    net.set_conditions(alice.addr, NetConditions::default());
    let mut connected = false;
    for _ in 0..10 {
        tick(&bob, &clock).await;
        if bob.ep.session_count() == 1 && alice.ep.session_count() == 1 {
            connected = true;
            break;
        }
    }
    assert!(
        connected,
        "la reconnexion n'a pas persisté après le retour d'Alice (dial abandonné trop tôt)"
    );
}

/// Cause 2: a WELCOME lost while the responder has already cached the HELLO
/// nonce is unrecoverable with identical retransmissions (replay-rejected). The
/// fix restarts the handshake with a FRESH nonce, so once the reply path heals
/// the session forms.
#[tokio::test]
async fn un_welcome_perdu_se_recupere_par_un_nonce_frais() {
    let clock = ManualClock::new(1_000_000);
    let net = SimNet::new(2, NetConditions::default());
    let alice = spawn_node(&net, &clock, "10.0.11.1:4000");
    let bob = spawn_node(&net, &clock, "10.0.11.2:4000");

    // Drop everything addressed TO Bob: his HELLO reaches Alice (she caches the
    // nonce and establishes her side), but every WELCOME back to Bob is lost.
    net.set_conditions(
        bob.addr,
        NetConditions {
            loss: 1.0,
            latency_min_ms: 0,
            latency_max_ms: 0,
        },
    );

    bob.ep.connect(alice.addr).await.unwrap();
    // Alice caches the first nonce and welcomes (dropped); identical
    // retransmissions are now replay-rejected forever.
    for _ in 0..5 {
        tick(&bob, &clock).await;
    }
    assert_eq!(
        bob.ep.session_count(),
        0,
        "Bob ne peut pas établir sa session : ses WELCOME sont tous perdus"
    );

    // Heal the reply path: only a FRESH nonce (not an identical replay) can now
    // elicit a WELCOME Alice will actually send.
    net.set_conditions(bob.addr, NetConditions::default());
    let mut connected = false;
    for _ in 0..12 {
        tick(&bob, &clock).await;
        if bob.ep.session_count() == 1 {
            connected = true;
            break;
        }
    }
    assert!(
        connected,
        "le WELCOME perdu n'a pas été récupéré : le handshake reste bloqué sur l'anti-rejeu"
    );
}

/// Cause 4: after a peer restarts silently on a new address, the responder must
/// evict the corpse session so exactly one direct session per identity survives
/// (deterministic link selection).
#[tokio::test]
async fn la_session_cadavre_est_evincee_a_la_reconnexion() {
    let clock = ManualClock::new(1_000_000);
    let net = SimNet::new(3, NetConditions::default());
    let mut alice = spawn_node(&net, &clock, "10.0.12.1:4000");
    let bob_id = Arc::new(Identity::generate_with_pow_bits(POW));

    // First incarnation of Bob establishes a direct session with Alice.
    let bob1 = spawn_node_avec_identite(&net, &clock, "10.0.12.2:4000", Arc::clone(&bob_id));
    bob1.ep
        .send(
            alice.addr,
            &ChannelMsg::Control(ControlMsg::Ping { token: 1 }),
        )
        .await
        .unwrap();
    wait_until(Duration::from_secs(2), || {
        alice.ep.direct_session_addr(&bob_id.public_key()) == Some(bob1.addr)
    })
    .await
    .expect("session initiale avec la première incarnation de Bob");
    assert_eq!(alice.ep.session_count(), 1);

    // Bob vanishes silently (UDP death, no goodbye) and restarts on a new port
    // with the SAME identity. Alice still holds the corpse.
    net.set_down(bob1.addr, true);
    let bob2 = spawn_node_avec_identite(&net, &clock, "10.0.12.3:4000", Arc::clone(&bob_id));
    bob2.ep
        .send(
            alice.addr,
            &ChannelMsg::Control(ControlMsg::Ping { token: 2 }),
        )
        .await
        .unwrap();
    wait_until(Duration::from_secs(2), || {
        alice.ep.direct_session_addr(&bob_id.public_key()) == Some(bob2.addr)
    })
    .await
    .expect("session avec la seconde incarnation de Bob");

    // The corpse must be gone: exactly one direct session for Bob's identity,
    // pointing at the fresh address.
    assert_eq!(
        alice.ep.session_count(),
        1,
        "la session cadavre n'a pas été évincée (livraison non déterministe)"
    );
    // Drain any stray event so the field is exercised (identity is the anchor).
    let _ = alice.events.try_recv();
}

/// Mobilité (§9.3) : le pair ne redémarre PAS — son adresse change SOUS une
/// session vivante (Wi-Fi → 4G, NAT qui refait son mapping). Rien n'est fermé,
/// rien n'est renégocié : les clés, le numéro de session et l'anti-rejeu restent
/// ceux d'avant. Le répondeur doit donc réaiguiller la session qu'il tient déjà,
/// et son seul indice est l'adresse SOURCE du datagramme suivant.
///
/// 🔒 Vérifier qu'un pair déplacé redevient JOIGNABLE ne prouve pas que sa
/// session a suivi : plusieurs chemins y mènent, et ils aboutissent au même
/// état. Au niveau nœud, le carnet d'adresses apprend la nouvelle adresse par
/// l'événement `Message` et la compose ; ici même, répondre à un paquet venu
/// d'une adresse inconnue ouvre un handshake vers elle. Dans les deux cas une
/// session NEUVE se forme, `install_session` évince le cadavre, et la table
/// finit identique — au prix d'un handshake complet (preuve de travail, clés
/// fraîches) là où la mobilité ne coûte qu'une mise à jour de champ.
///
/// Ce qui départage, c'est qu'AUCUN `Connected` n'ait été émis. D'où la
/// dernière assertion, qui est le cœur de ce test — vérifié en cassant : en
/// neutralisant la mise à jour d'adresse de `Endpoint::on_data`, c'est
/// exactement celle-là qui tombe, les précédentes restant vertes, et la
/// campagne de bout en bout avec elles.
#[tokio::test]
async fn une_session_directe_suit_son_pair_qui_change_d_adresse() {
    let clock = ManualClock::new(1_000_000);
    let net = SimNet::new(4, NetConditions::default());
    let mut alice = spawn_node(&net, &clock, "10.0.13.1:4000");
    let mut bob = spawn_node(&net, &clock, "10.0.13.2:4000");
    let bob_pub = bob.static_pub;
    // IP **et** port changent : un simple changement de port ne dirait rien
    // d'un basculement d'interface.
    let bob_apres: SocketAddr = "10.0.13.9:6000".parse().unwrap();

    // Session établie et prouvée : Alice sait où joindre Bob.
    bob.ep
        .send(
            alice.addr,
            &ChannelMsg::Control(ControlMsg::Ping { token: 1 }),
        )
        .await
        .unwrap();
    wait_until(Duration::from_secs(2), || {
        alice.ep.direct_session_addr(&bob_pub) == Some(bob.addr)
    })
    .await
    .expect("session initiale avec Bob");

    // Les événements déjà émis sont consommés ICI : ce qui apparaîtra APRÈS le
    // déménagement doit être vide de tout `Connected`, faute de quoi la session
    // aura été renégociée plutôt que déplacée.
    while alice.events.try_recv().is_ok() {}

    // Le déménagement, sous la session vivante.
    assert!(
        net.rebind(bob.addr, bob_apres),
        "le déplacement n'a pas eu lieu : le test ne prouverait rien"
    );
    assert_eq!(
        bob.ep.local_addr(),
        bob_apres,
        "le socket de Bob n'a pas suivi"
    );

    // Bob parle depuis sa nouvelle adresse. Aucun message ne l'annonce : c'est
    // l'enveloppe du datagramme, et elle seule, qui porte l'information.
    bob.ep
        .send(
            alice.addr,
            &ChannelMsg::Control(ControlMsg::Ping { token: 2 }),
        )
        .await
        .unwrap();
    wait_until(Duration::from_secs(5), || {
        alice.ep.direct_session_addr(&bob_pub) == Some(bob_apres)
    })
    .await
    .expect("la session d'Alice n'a pas suivi Bob : elle reste scellée vers une adresse morte");

    // Déplacée, pas dupliquée : une session directe par identité, sinon le
    // choix de lien redevient non déterministe (cause 4 ci-dessus).
    assert_eq!(
        alice.ep.session_count(),
        1,
        "le déplacement a laissé une session cadavre à l'ancienne adresse"
    );

    // Le retour : Alice ne connaît la nouvelle adresse de Bob que par sa
    // session, et ce qu'elle y envoie doit arriver.
    let vers_bob = alice
        .ep
        .direct_session_addr(&bob_pub)
        .expect("adresse de session");
    let retour = ChannelMsg::Core(accord_proto::core_msg::CoreMsg::Presence {
        status: 0,
        custom: Some("de retour".into()),
    });
    alice.ep.send(vers_bob, &retour).await.unwrap();
    let recu = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match bob.events.recv().await {
                Some(TransportEvent::Message { msg, .. }) => return *msg,
                Some(_) => continue,
                None => panic!("canal d'événements fermé"),
            }
        }
    })
    .await
    .expect("le retour n'est jamais arrivé à la nouvelle adresse de Bob");
    match recu {
        ChannelMsg::Core(accord_proto::core_msg::CoreMsg::Presence { custom, .. }) => {
            assert_eq!(custom, Some("de retour".into()));
        }
        autre => panic!("message inattendu : {autre:?}"),
    }

    // La session a SUIVI Bob, elle n'a pas été refaite : aucun nouveau
    // `Connected` n'a été émis depuis le déménagement.
    let mut renegociee = false;
    while let Ok(ev) = alice.events.try_recv() {
        if matches!(ev, TransportEvent::Connected { .. }) {
            renegociee = true;
        }
    }
    assert!(
        !renegociee,
        "une nouvelle session a été négociée : le pair déplacé a été traité \
         comme un inconnu, pas comme le même pair à une autre adresse"
    );

    // ⚠️ Ce que ce test ne prouve pas : qu'un pair déplacé reste joignable s'il
    // se TAIT. La mobilité est passive — tant que Bob n'émet rien, Alice écrit
    // vers une adresse morte, et seul le keep-alive (25 s) rouvre les yeux.
    // Ni qu'elle fonctionne à travers un RELAIS : `on_data` ne réaiguille que
    // les sessions directes, une session tunnelée gardant l'adresse du relais.
}

async fn wait_until(budget: Duration, mut cond: impl FnMut() -> bool) -> Result<(), ()> {
    let deadline = std::time::Instant::now() + budget;
    while std::time::Instant::now() < deadline {
        if cond() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    if cond() {
        Ok(())
    } else {
        Err(())
    }
}
