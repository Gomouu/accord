//! Tests end-to-end du repli TCP (SPEC §11.3) : le protocole de session
//! complet (handshake, chiffrement, échange applicatif) transite inchangé sur
//! un lien TCP adopté par le [`MuxSocket`], et la surface TCP résiste aux
//! octets forgés.

use accord_crypto::Identity;
use accord_proto::core_msg::CoreMsg;
use accord_proto::plaintext::ChannelMsg;
use accord_transport::endpoint::{Endpoint, EndpointConfig, TransportEvent};
use accord_transport::nat::{Candidate, CandidateKind};
use accord_transport::socket::UdpDatagram;
use accord_transport::tcp::{MuxSocket, TcpLinks};
use accord_transport::SystemClock;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

const POW: u32 = 4;

struct Node {
    ep: Arc<Endpoint>,
    events: mpsc::UnboundedReceiver<TransportEvent>,
    links: Arc<TcpLinks>,
    static_pub: [u8; 32],
}

/// Endpoint réel (UDP loopback) derrière un multiplexeur TCP.
async fn spawn_mux_node() -> Node {
    let udp = Arc::new(
        UdpDatagram::bind("127.0.0.1:0".parse().unwrap())
            .await
            .unwrap(),
    );
    let (mux, links) = MuxSocket::new(udp);
    let id = Arc::new(Identity::generate_with_pow_bits(POW));
    let static_pub = id.public_key();
    let (ep, events) = Endpoint::new(
        mux,
        id,
        Arc::new(SystemClock),
        EndpointConfig {
            pow_bits: POW,
            keepalive_ms: 25_000,
            idle_timeout_ms: 120_000,
            cookie_pressure_per_s: 64,
            relay_serving: false,
            capabilities: None,
        },
    );
    ep.spawn();
    Node {
        ep,
        events,
        links,
        static_pub,
    }
}

/// Relie deux nœuds par une connexion TCP réelle (loopback) adoptée des deux
/// côtés ; rend (adresse de b vue par a, adresse de a vue par b).
async fn link_pair(a: &Node, b: &Node) -> (SocketAddr, SocketAddr) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target = listener.local_addr().unwrap();
    let (client, server) = tokio::join!(tokio::net::TcpStream::connect(target), listener.accept());
    let peer_b = a.links.adopt(client.unwrap()).unwrap();
    let peer_a = b.links.adopt(server.unwrap().0).unwrap();
    (peer_b, peer_a)
}

#[tokio::test]
async fn session_complete_sur_lien_tcp() {
    let mut a = spawn_mux_node().await;
    let mut b = spawn_mux_node().await;
    let (peer_b, _peer_a) = link_pair(&a, &b).await;

    // Poinçonnage « post-TCP » : le handshake part sur le lien fraîchement
    // adopté, lié à l'identité attendue de b (anti-MITM), comme le fait le
    // runtime après `punch_connect`.
    let cand = Candidate {
        addr: peer_b,
        kind: CandidateKind::HolePunch,
    };
    a.ep.punch(&[cand], b.static_pub).await.unwrap();

    // Les deux côtés voient la session s'établir.
    let a_connected = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Some(TransportEvent::Connected { static_pub, .. }) = a.events.recv().await {
                return static_pub;
            }
        }
    })
    .await
    .expect("session côté a");
    assert_eq!(a_connected, b.static_pub);
    assert!(a.ep.has_direct_session_with(&b.static_pub));

    // Un message applicatif circule de bout en bout à travers le flux TCP.
    let msg = ChannelMsg::Core(CoreMsg::MsgAck { msg_id: [7u8; 16] });
    a.ep.send_to(peer_b, Some(b.static_pub), &msg)
        .await
        .unwrap();
    let received = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Some(TransportEvent::Message {
                msg, static_pub, ..
            }) = b.events.recv().await
            {
                return (msg, static_pub);
            }
        }
    })
    .await
    .expect("message côté b");
    assert_eq!(*received.0, msg);
    assert_eq!(received.1, a.static_pub);
}

#[tokio::test]
async fn octets_forges_sur_tcp_ne_paniquent_pas_l_endpoint() {
    let a = spawn_mux_node().await;

    // Un attaquant se connecte au « port TCP » du nœud (ici : adoption directe,
    // comme le ferait l'accepteur) et envoie des trames bien encadrées mais au
    // contenu forgé : le décodage strict de l'endpoint doit tout rejeter sans
    // paniquer, et le nœud doit rester fonctionnel.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target = listener.local_addr().unwrap();
    let (client, server) = tokio::join!(tokio::net::TcpStream::connect(target), listener.accept());
    a.links.adopt(server.unwrap().0).unwrap();
    let mut attacker = client.unwrap();

    // Salve de trames forgées : garbage pur, faux HELLO tronqué, zéros.
    for payload in [
        &b"\xff\xff\xff\xffgarbage"[..],
        &b"\x01\x01"[..],
        &[0u8; 512][..],
    ] {
        let len = (payload.len() as u16).to_be_bytes();
        attacker.write_all(&len).await.unwrap();
        attacker.write_all(payload).await.unwrap();
    }
    attacker.flush().await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Le nœud est toujours vivant : une vraie session UDP s'établit ensuite.
    let mut b = spawn_mux_node().await;
    let udp_a = a.ep.local_addr();
    let cand = Candidate {
        addr: udp_a,
        kind: CandidateKind::LocalDirect,
    };
    b.ep.punch(&[cand], a.static_pub).await.unwrap();
    let connected = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Some(TransportEvent::Connected { static_pub, .. }) = b.events.recv().await {
                return static_pub;
            }
        }
    })
    .await
    .expect("le nœud doit rester fonctionnel après les trames forgées");
    assert_eq!(connected, a.static_pub);
}
