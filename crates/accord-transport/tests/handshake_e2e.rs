//! Tests end-to-end de l'endpoint transport sur le mesh simulé.

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
    }
}

struct Node {
    ep: Arc<Endpoint>,
    events: mpsc::UnboundedReceiver<TransportEvent>,
    addr: SocketAddr,
    static_pub: [u8; 32],
}

fn spawn_node(net: &SimNet, clock: &ManualClock, addr: &str) -> Node {
    let id = Arc::new(Identity::generate_with_pow_bits(POW));
    spawn_node_avec_identite(net, clock, addr, id)
}

/// Variante à identité imposée : indispensable pour simuler le REDÉMARRAGE
/// d'un pair (même clé statique, nouvelle adresse).
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

async fn recv_message(node: &mut Node) -> ChannelMsg {
    loop {
        match tokio::time::timeout(Duration::from_secs(2), node.events.recv())
            .await
            .expect("timeout événement")
            .expect("canal fermé")
        {
            TransportEvent::Message { msg, .. } => return *msg,
            _ => continue,
        }
    }
}

#[tokio::test]
async fn two_nodes_handshake_and_exchange() {
    let clock = ManualClock::new(1_000_000);
    let net = SimNet::new(1, NetConditions::default());
    let mut alice = spawn_node(&net, &clock, "10.0.0.1:4000");
    let bob = spawn_node(&net, &clock, "10.0.0.2:4000");

    // Alice envoie un message applicatif à Bob : déclenche le handshake, met
    // en file, puis livre après WELCOME.
    let hello_msg = ChannelMsg::Control(ControlMsg::Ping { token: 42 });
    alice.ep.send(bob.addr, &hello_msg).await.unwrap();

    // Bob reçoit le PING (traité en interne → répond PONG), Alice le PONG.
    // On vérifie surtout que la session est établie des deux côtés.
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if alice.ep.session_count() == 1 && bob.ep.session_count() == 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("sessions établies");

    // Après établissement, un message applicatif non-contrôle est remonté.
    let profile = ChannelMsg::Core(accord_proto::core_msg::CoreMsg::Presence {
        status: 0,
        custom: Some("coucou".into()),
    });
    bob.ep.send(alice.addr, &profile).await.unwrap();
    let got = recv_message(&mut alice).await;
    match got {
        ChannelMsg::Core(accord_proto::core_msg::CoreMsg::Presence { custom, .. }) => {
            assert_eq!(custom, Some("coucou".into()));
        }
        other => panic!("message inattendu: {other:?}"),
    }
}

/// H1 : `NODE_ANNOUNCE` est un message de contrôle traité DANS la couche
/// transport (il ne passe pas par les token-buckets DHT). Sans bornage par
/// session, un pair déjà authentifié pourrait inonder des annonces à plein
/// débit — chaque remontée déclenchant une insertion de table et un événement
/// sur un canal non borné. Le seau de contrôle par session (horloge figée : pas
/// de recharge) écrête le flot : au plus `CTRL_MSG_BURST` annonces sont
/// ACCEPTÉES (événements émis) sur toute la vie de la session, pas une par
/// message reçu.
#[tokio::test]
async fn node_announce_flood_borne_par_session() {
    let clock = ManualClock::new(1_000_000);
    let net = SimNet::new(101, NetConditions::default());
    let mut victim = spawn_node(&net, &clock, "10.0.9.1:4000");
    let attacker = spawn_node(&net, &clock, "10.0.9.2:4000");

    // Établit une session directe (l'attaquant initie).
    attacker
        .ep
        .send(
            victim.addr,
            &ChannelMsg::Control(ControlMsg::Ping { token: 1 }),
        )
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if attacker.ep.session_count() == 1 && victim.ep.session_count() == 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("session établie");

    // Inonde 200 NODE_ANNOUNCE (bien au-delà du quota de rafale).
    const FLOOD: usize = 200;
    for i in 0..FLOOD {
        attacker
            .ep
            .send(
                victim.addr,
                &ChannelMsg::Control(ControlMsg::NodeAnnounce {
                    pow_nonce: i as u64,
                    flags: 0,
                }),
            )
            .await
            .unwrap();
    }

    // Draine les événements de la victime et compte les annonces ACCEPTÉES
    // (émises) émanant de l'attaquant, jusqu'au silence.
    let mut accepted = 0usize;
    loop {
        match tokio::time::timeout(Duration::from_millis(300), victim.events.recv()).await {
            Ok(Some(TransportEvent::NodeAnnounced { static_pub, .. }))
                if static_pub == attacker.static_pub =>
            {
                accepted += 1;
            }
            Ok(Some(_)) => continue,
            Ok(None) => break,
            Err(_) => break, // plus rien ne vient : le flot a été écrêté
        }
    }

    // Horloge figée ⇒ aucune recharge : au plus la capacité de rafale du seau
    // de contrôle (8) est acceptée, TRÈS en deçà des 200 émises. Sans le
    // correctif, `accepted` vaudrait ~200 (une par message).
    assert!(
        accepted <= 8,
        "flood non borné : {accepted} annonces acceptées sur {FLOOD} (attendu ≤ 8)"
    );
    assert!(accepted >= 1, "l'annonce initiale légitime doit passer");
}

#[tokio::test]
async fn message_survives_packet_loss() {
    let clock = ManualClock::new(1_000_000);
    // 30 % de perte : les retransmissions de handshake doivent finir par passer.
    let net = SimNet::new(
        7,
        NetConditions {
            loss: 0.30,
            latency_min_ms: 0,
            latency_max_ms: 0,
        },
    );
    let alice = spawn_node(&net, &clock, "10.0.1.1:4000");
    let mut bob = spawn_node(&net, &clock, "10.0.1.2:4000");

    let msg = ChannelMsg::Core(accord_proto::core_msg::CoreMsg::Presence {
        status: 1,
        custom: None,
    });

    // Boucle de renvoi applicatif + avance d'horloge pour déclencher les
    // retransmissions de la maintenance (timeout 2 s).
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let mut delivered = false;
    while std::time::Instant::now() < deadline {
        alice.ep.send(bob.addr, &msg).await.unwrap();
        clock.advance(2_500);
        if let Ok(Some(ev)) =
            tokio::time::timeout(Duration::from_millis(200), bob.events.recv()).await
        {
            if matches!(ev, TransportEvent::Message { .. }) {
                delivered = true;
                break;
            }
        }
    }
    assert!(delivered, "le message n'a jamais été livré malgré la perte");
}

/// Construit un `FileMsg::Block` de `taille` octets (charge applicative bien
/// au-delà de la MTU : force la fragmentation transport).
fn gros_bloc(taille: usize) -> ChannelMsg {
    let data: Vec<u8> = (0..taille).map(|i| (i % 251) as u8).collect();
    ChannelMsg::File(accord_proto::file_msg::FileMsg::Block {
        root: [7u8; 32],
        index: 0,
        data,
    })
}

#[tokio::test]
async fn gros_message_fragmente_et_reassemble() {
    let clock = ManualClock::new(1_000_000);
    let net = SimNet::new(11, NetConditions::default());
    let mut alice = spawn_node(&net, &clock, "10.0.3.1:4000");
    let bob = spawn_node(&net, &clock, "10.0.3.2:4000");

    // Établir la session d'abord (PING de contrôle).
    alice.ep.connect(bob.addr).await.unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while alice.ep.session_count() == 0 || bob.ep.session_count() == 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("sessions établies");

    // Un bloc de 200 KiB : plusieurs centaines de fragments.
    let bloc = gros_bloc(200 * 1024);
    bob.ep.send(alice.addr, &bloc).await.unwrap();
    let got = recv_message(&mut alice).await;
    assert_eq!(
        got, bloc,
        "le gros message n'a pas été réassemblé à l'identique"
    );
}

#[tokio::test]
async fn gros_message_perdu_partiellement_echoue_puis_reussit_a_la_reemission() {
    let clock = ManualClock::new(1_000_000);
    // 5 % de perte : un seul fragment perdu fait échouer tout le message (pas de
    // retransmission au niveau transport) ; la réémission applicative finit par
    // faire passer une copie complète. Message modéré (~8 fragments) pour rester
    // sous le plafond anti-DoS de réassemblages simultanés.
    let net = SimNet::new(
        23,
        NetConditions {
            loss: 0.05,
            latency_min_ms: 0,
            latency_max_ms: 0,
        },
    );
    let mut alice = spawn_node(&net, &clock, "10.0.4.1:4000");
    let bob = spawn_node(&net, &clock, "10.0.4.2:4000");

    alice.ep.connect(bob.addr).await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while alice.ep.session_count() == 0 || bob.ep.session_count() == 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("sessions établies");

    let bloc = gros_bloc(8 * 1024);
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    let mut delivered = false;
    while std::time::Instant::now() < deadline {
        bob.ep.send(alice.addr, &bloc).await.unwrap();
        // Purge d'horloge pour libérer d'éventuels réassemblages timeoutés.
        clock.advance(1_000);
        if let Ok(Some(TransportEvent::Message { msg, .. })) =
            tokio::time::timeout(Duration::from_millis(200), alice.events.recv()).await
        {
            if *msg == bloc {
                delivered = true;
                break;
            }
        }
    }
    assert!(
        delivered,
        "le gros message n'a jamais été livré malgré la perte de fragments"
    );
}

#[tokio::test]
async fn observe_addr_reports_public_address() {
    let clock = ManualClock::new(1_000_000);
    let net = SimNet::new(3, NetConditions::default());
    let mut alice = spawn_node(&net, &clock, "10.0.2.1:4000");
    let bob = spawn_node(&net, &clock, "10.0.2.2:4000");

    // Établir d'abord la session.
    alice.ep.connect(bob.addr).await.unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while alice.ep.session_count() == 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();

    // Alice demande son adresse observée.
    alice
        .ep
        .send(bob.addr, &ChannelMsg::Control(ControlMsg::ObserveAddrReq))
        .await
        .unwrap();

    let observed = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Some(TransportEvent::ObservedAddr { observed, .. }) = alice.events.recv().await {
                break observed;
            }
        }
    })
    .await
    .expect("adresse observée reçue");
    assert_eq!(observed, alice.addr);
}

#[tokio::test]
async fn bound_send_to_expected_identity_succeeds() {
    // Cas nominal lié : Alice envoie à Bob en visant explicitement l'identité
    // de Bob ; la session s'établit et le message est livré comme sans liaison.
    let clock = ManualClock::new(1_000_000);
    let net = SimNet::new(31, NetConditions::default());
    let alice = spawn_node(&net, &clock, "10.0.6.1:4000");
    let mut bob = spawn_node(&net, &clock, "10.0.6.2:4000");

    let msg = ChannelMsg::Core(accord_proto::core_msg::CoreMsg::Presence {
        status: 0,
        custom: Some("lié".into()),
    });
    alice
        .ep
        .send_to(bob.addr, Some(bob.static_pub), &msg)
        .await
        .unwrap();

    let got = recv_message(&mut bob).await;
    match got {
        ChannelMsg::Core(accord_proto::core_msg::CoreMsg::Presence { custom, .. }) => {
            assert_eq!(custom, Some("lié".into()));
        }
        other => panic!("message inattendu: {other:?}"),
    }
    assert_eq!(
        alice.ep.session_count(),
        1,
        "session liée établie côté Alice"
    );
}

#[tokio::test]
async fn welcome_from_wrong_identity_is_rejected() {
    // MITM on-path : Alice résout « Bob » vers une adresse tenue par Mallory et
    // envoie un DM lié à l'identité de Bob. Mallory répond un WELCOME frais,
    // valide et signé de SA propre identité. Alice doit refuser la liaison :
    // aucune session établie chez Alice, et le DM ne doit JAMAIS être scellé ni
    // livré à Mallory.
    let clock = ManualClock::new(1_000_000);
    let net = SimNet::new(37, NetConditions::default());
    let alice = spawn_node(&net, &clock, "10.0.7.1:4000");
    let mut mallory = spawn_node(&net, &clock, "10.0.7.2:4000");

    // Identité cible tierce (le « vrai Bob »), distincte de Mallory.
    let bob_pub = Identity::generate_with_pow_bits(POW).public_key();
    assert_ne!(bob_pub, mallory.static_pub);

    let secret = ChannelMsg::Core(accord_proto::core_msg::CoreMsg::Presence {
        status: 0,
        custom: Some("confidentiel".into()),
    });
    // L'envoi lui-même réussit (le HELLO part) : le rejet survient au WELCOME.
    alice
        .ep
        .send_to(mallory.addr, Some(bob_pub), &secret)
        .await
        .unwrap();

    // Mallory ne doit jamais recevoir le DM en clair. On attend suffisamment
    // pour que HELLO/WELCOME s'échangent : seul un événement Message trahirait
    // une fuite (Mallory établit bien une session de son côté, mais la file
    // d'Alice n'est jamais scellée sous cette session usurpée).
    let leaked = tokio::time::timeout(Duration::from_millis(500), async {
        loop {
            match mallory.events.recv().await {
                Some(TransportEvent::Message { .. }) => return true,
                Some(_) => continue,
                None => return false,
            }
        }
    })
    .await
    .unwrap_or(false);
    assert!(!leaked, "le DM a fuité vers une identité usurpée (MITM)");
    assert_eq!(
        alice.ep.session_count(),
        0,
        "Alice n'aurait pas dû établir de session avec une identité inattendue"
    );
}

/// Croisement de handshakes (reconnexion du terrain : dial + poinçonnage
/// simultanés) : chaque côté ouvre SON handshake avant d'avoir vu celui de
/// l'autre — les HELLO se croisent et DEUX sessions s'établissent, chaque
/// côté pouvant retenir « en dernier » un handshake différent. La session
/// remplacée doit rester RECEVABLE (elle n'est plus le chemin d'envoi) :
/// sinon, tout un sens de la conversation part dans un trou noir silencieux
/// jusqu'au timeout d'inactivité — vécu sur le terrain comme un profil ou
/// une bannière « jamais reçus ». On vérifie que les deux sens livrent, à
/// l'établissement comme APRÈS stabilisation.
#[tokio::test]
async fn croisement_de_handshakes_ne_perd_aucune_direction() {
    let clock = ManualClock::new(1_000_000);
    let net = SimNet::new(7, NetConditions::default());
    let mut alice = spawn_node(&net, &clock, "10.0.7.1:4000");
    let mut bob = spawn_node(&net, &clock, "10.0.7.2:4000");

    let depuis_alice = ChannelMsg::Core(accord_proto::core_msg::CoreMsg::Presence {
        status: 0,
        custom: Some("annonce-d-alice".into()),
    });
    let depuis_bob = ChannelMsg::Core(accord_proto::core_msg::CoreMsg::Presence {
        status: 0,
        custom: Some("annonce-de-bob".into()),
    });

    // Envois SIMULTANÉS : les deux pendings initiateurs existent avant que le
    // moindre HELLO ne soit traité — croisement garanti.
    let (ra, rb) = tokio::join!(
        alice.ep.send(bob.addr, &depuis_alice),
        bob.ep.send(alice.addr, &depuis_bob),
    );
    ra.unwrap();
    rb.unwrap();

    // Les DEUX messages d'établissement arrivent (aucun sens perdu).
    let recu_par_bob = recv_message(&mut bob).await;
    match recu_par_bob {
        ChannelMsg::Core(accord_proto::core_msg::CoreMsg::Presence { custom, .. }) => {
            assert_eq!(custom, Some("annonce-d-alice".into()));
        }
        other => panic!("message inattendu chez Bob: {other:?}"),
    }
    let recu_par_alice = recv_message(&mut alice).await;
    match recu_par_alice {
        ChannelMsg::Core(accord_proto::core_msg::CoreMsg::Presence { custom, .. }) => {
            assert_eq!(custom, Some("annonce-de-bob".into()));
        }
        other => panic!("message inattendu chez Alice: {other:?}"),
    }

    // Après stabilisation (les deux handshakes ont fini de s'installer), les
    // envois suivants livrent TOUJOURS dans les deux sens.
    let tardif_alice = ChannelMsg::Core(accord_proto::core_msg::CoreMsg::Presence {
        status: 1,
        custom: Some("tardif-alice".into()),
    });
    alice.ep.send(bob.addr, &tardif_alice).await.unwrap();
    match recv_message(&mut bob).await {
        ChannelMsg::Core(accord_proto::core_msg::CoreMsg::Presence { custom, .. }) => {
            assert_eq!(custom, Some("tardif-alice".into()));
        }
        other => panic!("message tardif inattendu chez Bob: {other:?}"),
    }
    let tardif_bob = ChannelMsg::Core(accord_proto::core_msg::CoreMsg::Presence {
        status: 1,
        custom: Some("tardif-bob".into()),
    });
    bob.ep.send(alice.addr, &tardif_bob).await.unwrap();
    match recv_message(&mut alice).await {
        ChannelMsg::Core(accord_proto::core_msg::CoreMsg::Presence { custom, .. }) => {
            assert_eq!(custom, Some("tardif-bob".into()));
        }
        other => panic!("message tardif inattendu chez Alice: {other:?}"),
    }
}

/// Un pair qui s'éteint BRUTALEMENT (UDP : aucun adieu) puis redémarre à une
/// nouvelle adresse laisse chez son ami DEUX sessions directes pour la même
/// identité jusqu'à l'expiration d'inactivité (2 min) : la morte et la
/// fraîche. La résolution identité → session doit préférer la fraîche (dernier
/// trafic ENTRANT le plus récent) — un choix arbitraire (ordre de HashMap,
/// stable pour tout le processus) enverrait chaque annonce de profil dans la
/// session cadavre, sans erreur ni relance avant la ré-annonce périodique
/// (30 min) : le bug « bannière posée hors-ligne jamais rattrapée ».
#[tokio::test]
async fn redemarrage_silencieux_prefere_la_session_fraiche() {
    let clock = ManualClock::new(1_000_000);
    let net = SimNet::new(9, NetConditions::default());
    let alice = spawn_node(&net, &clock, "10.0.9.1:4000");
    let id_bob = Arc::new(Identity::generate_with_pow_bits(POW));
    let bob = spawn_node_avec_identite(&net, &clock, "10.0.9.2:4000", Arc::clone(&id_bob));
    let bob_pub = bob.static_pub;
    let ancienne_adresse = bob.addr;

    let ping = ChannelMsg::Control(ControlMsg::Ping { token: 1 });
    alice.ep.send(bob.addr, &ping).await.unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if alice.ep.session_count() == 1 && bob.ep.session_count() == 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("première session établie");
    assert_eq!(
        alice.ep.direct_session_addr(&bob_pub),
        Some(ancienne_adresse)
    );

    // Extinction silencieuse : aucun événement côté Alice, sa session survit.
    bob.ep.shutdown();
    drop(bob);
    clock.advance(5_000);

    // Redémarrage : même identité, NOUVELLE adresse ; Bob recontacte Alice.
    let bob2 = spawn_node_avec_identite(&net, &clock, "10.0.9.3:4000", id_bob);
    bob2.ep.send(alice.addr, &ping).await.unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if alice.ep.session_count() == 2 && bob2.ep.session_count() == 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("session de la nouvelle incarnation établie");

    // Les deux sessions coexistent : la résolution doit viser la VIVANTE.
    assert_eq!(
        alice.ep.direct_session_addr(&bob_pub),
        Some(bob2.addr),
        "la résolution identité → session vise la session cadavre"
    );
}

/// Régression 3.0.0 : l'installation de la session initiateur vivait dans un
/// `debug_assert!` — argument NON évalué en profil release. Tests debug tous
/// verts, mais l'app publiée jetait chaque session qu'elle initiait : plus
/// aucun message échangé dès que les deux pairs devaient recomposer leurs
/// sessions (churn de reconnexion infini, `direct_session_addr` toujours
/// vide). Ce test verrouille l'invariant DANS LES DEUX PROFILS — il tourne
/// aussi en release via le pas CI « Rust tests transport (release) ».
#[tokio::test]
async fn initiateur_installe_sa_session_et_peut_emettre() {
    let clock = ManualClock::new(1_000_000);
    let net = SimNet::new(11, NetConditions::default());
    let alice = spawn_node(&net, &clock, "10.0.11.1:4000");
    let mut bob = spawn_node(&net, &clock, "10.0.11.2:4000");
    let bob_pub = bob.static_pub;

    // Alice initie ; le WELCOME de Bob doit installer la session côté Alice.
    let ping = ChannelMsg::Control(ControlMsg::Ping { token: 7 });
    alice.ep.send(bob.addr, &ping).await.unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if alice.ep.direct_session_addr(&bob_pub).is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("la session initiateur doit être visible par identité");

    // Et un envoi POST-établissement (hors file du pending) doit être livré.
    let msg = ChannelMsg::Core(accord_proto::core_msg::CoreMsg::Presence {
        status: 0,
        custom: Some("post-handshake".into()),
    });
    alice
        .ep
        .send_to(bob.addr, Some(bob_pub), &msg)
        .await
        .unwrap();
    let got = recv_message(&mut bob).await;
    match got {
        ChannelMsg::Core(accord_proto::core_msg::CoreMsg::Presence { custom, .. }) => {
            assert_eq!(custom, Some("post-handshake".into()));
        }
        other => panic!("message inattendu: {other:?}"),
    }
}

/// D4/D35 : la latence par pair est mesurée sur le PING/PONG keep-alive
/// existant (aucun octet nouveau sur le fil) et exposée par `session_views`
/// avec la nature du lien et la fraîcheur du dernier trafic entrant.
#[tokio::test]
async fn keepalive_mesure_la_latence_et_session_views_l_expose() {
    let clock = ManualClock::new(1_000_000);
    let net = SimNet::new(13, NetConditions::default());
    let alice = spawn_node(&net, &clock, "10.0.13.1:4000");
    let bob = spawn_node(&net, &clock, "10.0.13.2:4000");
    let bob_pub = bob.static_pub;

    let ping = ChannelMsg::Control(ControlMsg::Ping { token: 3 });
    alice.ep.send(bob.addr, &ping).await.unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if alice.ep.session_count() == 1 && bob.ep.session_count() == 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("session établie");

    // Avant tout keep-alive : la vue existe, sans latence mesurée.
    let vues = alice.ep.session_views();
    assert_eq!(vues.len(), 1);
    assert_eq!(vues[0].peer_static, bob_pub);
    assert_eq!(vues[0].addr, bob.addr);
    assert!(vues[0].relay_circuit.is_none(), "session directe");
    assert!(
        vues[0].last_rtt_ms.is_none(),
        "aucun cycle keep-alive encore"
    );

    // Échéance keep-alive : la maintenance émet un PING corrélé ; le PONG de
    // Bob solde le cycle et enregistre l'aller-retour.
    clock.advance(25_000);
    alice.ep.run_maintenance().await;
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let vues = alice.ep.session_views();
            if vues.first().and_then(|v| v.last_rtt_ms).is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("latence mesurée sur le PONG keep-alive");
    // Horloge manuelle figée entre PING et PONG : aller-retour nul, mesuré.
    assert_eq!(alice.ep.session_views()[0].last_rtt_ms, Some(0));
}
