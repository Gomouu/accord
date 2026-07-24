//! Tests end-to-end du poinçonnage UDP coordonné (SPEC §11, points 2-3) sur le
//! mesh simulé déterministe.

use accord_crypto::Identity;
use accord_proto::envelope::Packet;
use accord_proto::plaintext::ChannelMsg;
use accord_proto::WireDecode;
use accord_transport::clock::ManualClock;
use accord_transport::endpoint::{Endpoint, EndpointConfig, TransportEvent};
use accord_transport::nat::{Candidate, CandidateKind};
use accord_transport::socket::sim::{NetConditions, SimNet};
use accord_transport::socket::DatagramSocket;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
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
    let addr: SocketAddr = addr.parse().unwrap();
    let socket = Arc::new(net.bind(addr));
    build_node(socket, clock, addr)
}

fn build_node(socket: Arc<dyn DatagramSocket>, clock: &ManualClock, addr: SocketAddr) -> Node {
    let id = Arc::new(Identity::generate_with_pow_bits(POW));
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

fn hole_punch(addr: SocketAddr) -> Candidate {
    Candidate {
        addr,
        kind: CandidateKind::HolePunch,
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

fn presence(custom: &str) -> ChannelMsg {
    ChannelMsg::Core(accord_proto::core_msg::CoreMsg::Presence {
        status: 0,
        custom: Some(custom.into()),
    })
}

/// Socket simulé qui compte les HELLO émis (pour prouver l'arrêt anticipé).
struct CountingSocket {
    inner: accord_transport::socket::sim::SimSocket,
    hellos: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl DatagramSocket for CountingSocket {
    async fn send_to(&self, buf: &[u8], dst: SocketAddr) -> std::io::Result<usize> {
        if let Ok(Packet::Hello(_)) = Packet::from_bytes(buf) {
            self.hellos.fetch_add(1, Ordering::SeqCst);
        }
        self.inner.send_to(buf, dst).await
    }

    async fn recv_from(&self) -> std::io::Result<(Vec<u8>, SocketAddr)> {
        self.inner.recv_from().await
    }

    fn local_addr(&self) -> SocketAddr {
        self.inner.local_addr()
    }
}

/// Attend (borné) que les deux endpoints aient établi exactement une session.
async fn wait_single_session(a: &Arc<Endpoint>, b: &Arc<Endpoint>) {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if a.session_count() == 1 && b.session_count() == 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("les deux endpoints doivent établir une session unique");
}

#[tokio::test]
async fn mutual_punch_etablit_une_seule_session_bidirectionnelle() {
    // Deux pairs se « punchent » mutuellement : chacun émet des HELLO vers les
    // candidats de l'autre. L'ouverture simultanée doit se résoudre en UNE
    // seule session partagée (pas deux), utilisable dans les deux sens.
    let clock = ManualClock::new(1_000_000);
    let net = SimNet::new(101, NetConditions::default());
    let mut alice = spawn_node(&net, &clock, "10.1.0.1:4000");
    let mut bob = spawn_node(&net, &clock, "10.1.0.2:4000");

    let cands_for_bob = [hole_punch(bob.addr)];
    let cands_for_alice = [hole_punch(alice.addr)];

    let (ra, rb) = tokio::join!(
        alice.ep.punch(&cands_for_bob, bob.static_pub),
        bob.ep.punch(&cands_for_alice, alice.static_pub),
    );
    ra.unwrap();
    rb.unwrap();

    wait_single_session(&alice.ep, &bob.ep).await;
    assert_eq!(alice.ep.session_count(), 1, "Alice : une session");
    assert_eq!(bob.ep.session_count(), 1, "Bob : une session");

    // La session unique porte le trafic dans les deux sens.
    alice.ep.send(bob.addr, &presence("a->b")).await.unwrap();
    let got_b = recv_message(&mut bob).await;
    assert_eq!(got_b, presence("a->b"));

    bob.ep.send(alice.addr, &presence("b->a")).await.unwrap();
    let got_a = recv_message(&mut alice).await;
    assert_eq!(got_a, presence("b->a"));

    // Toujours une seule session de chaque côté après échange.
    assert_eq!(alice.ep.session_count(), 1);
    assert_eq!(bob.ep.session_count(), 1);
}

#[tokio::test]
async fn punch_arrete_apres_la_premiere_session() {
    // Alice punche Bob (passif, simple répondeur). Dès la première session
    // établie, plus aucun HELLO ne doit être émis : on compte exactement 1
    // HELLO (la première salve), pas les 5 tentatives.
    let clock = ManualClock::new(1_000_000);
    let net = SimNet::new(202, NetConditions::default());

    let alice_addr: SocketAddr = "10.2.0.1:4000".parse().unwrap();
    let hellos = Arc::new(AtomicUsize::new(0));
    let counting = Arc::new(CountingSocket {
        inner: net.bind(alice_addr),
        hellos: Arc::clone(&hellos),
    });
    let alice = build_node(counting, &clock, alice_addr);
    let bob = spawn_node(&net, &clock, "10.2.0.2:4000");

    alice
        .ep
        .punch(&[hole_punch(bob.addr)], bob.static_pub)
        .await
        .unwrap();

    assert_eq!(alice.ep.session_count(), 1, "session établie");
    assert_eq!(
        hellos.load(Ordering::SeqCst),
        1,
        "un seul HELLO émis : les salves restantes sont annulées après établissement"
    );
}

#[tokio::test]
async fn punch_reussit_quand_seul_le_dernier_candidat_repond() {
    // Liste de candidats dont seuls certains sont « vivants » : les adresses
    // muettes (non liées dans le mesh) avalent les HELLO, seul le dernier
    // candidat répond. La session doit tout de même s'établir avec lui.
    let clock = ManualClock::new(1_000_000);
    let net = SimNet::new(303, NetConditions::default());
    let alice = spawn_node(&net, &clock, "10.3.0.1:4000");
    let mut bob = spawn_node(&net, &clock, "10.3.0.2:4000");

    // Deux candidats muets (aucun endpoint à ces adresses) + le vrai Bob.
    let muet1: SocketAddr = "10.3.9.1:4000".parse().unwrap();
    let muet2: SocketAddr = "10.3.9.2:4000".parse().unwrap();
    let candidates = [
        Candidate {
            addr: muet1,
            kind: CandidateKind::LocalDirect,
        },
        Candidate {
            addr: muet2,
            kind: CandidateKind::PublicDirect,
        },
        hole_punch(bob.addr),
    ];

    alice.ep.punch(&candidates, bob.static_pub).await.unwrap();

    // Exactement une session : celle avec Bob (les muets ne créent pas de
    // session, seulement des pendings sans réponse).
    assert_eq!(
        alice.ep.session_count(),
        1,
        "une session établie malgré les candidats muets"
    );

    // Confirme que la session est bien celle avec Bob : le message y transite.
    alice.ep.send(bob.addr, &presence("vivant")).await.unwrap();
    let got = recv_message(&mut bob).await;
    assert_eq!(got, presence("vivant"));
}

#[tokio::test]
async fn punch_sans_reponse_epuise_les_tentatives_sans_session() {
    // Tous les candidats sont muets : le poinçonnage épuise ses salves et rend
    // la main sans erreur ni panique, aucune session établie (best-effort).
    let clock = ManualClock::new(1_000_000);
    let net = SimNet::new(404, NetConditions::default());
    let alice = spawn_node(&net, &clock, "10.4.0.1:4000");

    let muet1: SocketAddr = "10.4.9.1:4000".parse().unwrap();
    let muet2: SocketAddr = "10.4.9.2:4000".parse().unwrap();
    let candidates = [hole_punch(muet1), hole_punch(muet2)];

    // Ne doit pas paniquer ni renvoyer d'erreur dure.
    alice.ep.punch(&candidates, [0u8; 32]).await.unwrap();

    assert_eq!(
        alice.ep.session_count(),
        0,
        "aucune session sans réponse des candidats"
    );
}
