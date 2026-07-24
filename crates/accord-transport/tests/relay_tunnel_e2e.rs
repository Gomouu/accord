//! Tests end-to-end du TUNNEL RELAIS côté CLIENT (SPEC §10-§11.3) sur le mesh
//! simulé déterministe.
//!
//! Le tunnel relais est le dernier maillon du NAT traversal : quand deux amis A
//! et B ne peuvent PAS se livrer de datagrammes directement (NAT symétrique,
//! simulé ici par un lien direct A↔B coupé), leur session bout-en-bout passe *à
//! travers* un relais public R. Le relais n'achemine que des blobs opaques : il
//! ne peut ni déchiffrer, ni se substituer à B (liaison d'identité D-037).

use accord_crypto::handshake::Initiator;
use accord_crypto::Identity;
use accord_proto::envelope::Packet;
use accord_proto::plaintext::{ChannelMsg, RelayMsg};
use accord_proto::WireEncode;
use accord_transport::clock::ManualClock;
use accord_transport::endpoint::{Endpoint, EndpointConfig, TransportEvent};
use accord_transport::error::TransportError;
use accord_transport::socket::sim::{NetConditions, SimNet};
use accord_transport::socket::{sim::SimSocket, DatagramSocket};
use accord_transport::Clock;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

const POW: u32 = 4;

fn config(relay_serving: bool) -> EndpointConfig {
    EndpointConfig {
        pow_bits: POW,
        relay_serving,
        ..EndpointConfig::default()
    }
}

struct Node {
    ep: Arc<Endpoint>,
    events: mpsc::UnboundedReceiver<TransportEvent>,
    addr: SocketAddr,
    static_pub: [u8; 32],
}

/// Socket simulé qui simule l'ABSENCE de route directe vers certaines adresses :
/// tout datagramme destiné à une adresse `blocked` est silencieusement avalé
/// (comme un NAT symétrique qui empêche la livraison directe), les autres
/// passent normalement. C'est ce qui force la session A↔B à emprunter le relais.
struct BlockingSocket {
    inner: SimSocket,
    blocked: Vec<SocketAddr>,
}

#[async_trait::async_trait]
impl DatagramSocket for BlockingSocket {
    async fn send_to(&self, buf: &[u8], dst: SocketAddr) -> io::Result<usize> {
        if self.blocked.contains(&dst) {
            return Ok(buf.len()); // avalé : pas de route directe
        }
        self.inner.send_to(buf, dst).await
    }

    async fn recv_from(&self) -> io::Result<(Vec<u8>, SocketAddr)> {
        self.inner.recv_from().await
    }

    fn local_addr(&self) -> SocketAddr {
        self.inner.local_addr()
    }
}

fn spawn_node(net: &SimNet, clock: &ManualClock, addr: &str, relay_serving: bool) -> Node {
    let addr: SocketAddr = addr.parse().unwrap();
    let socket = Arc::new(net.bind(addr));
    make_node(socket, clock, addr, relay_serving)
}

/// Nœud dont le lien direct vers `blocked` est coupé (pas de route directe).
fn spawn_blocking(
    net: &SimNet,
    clock: &ManualClock,
    addr: &str,
    relay_serving: bool,
    blocked: &[&str],
) -> Node {
    let addr: SocketAddr = addr.parse().unwrap();
    let blocked: Vec<SocketAddr> = blocked.iter().map(|s| s.parse().unwrap()).collect();
    let socket = Arc::new(BlockingSocket {
        inner: net.bind(addr),
        blocked,
    });
    make_node(socket, clock, addr, relay_serving)
}

fn make_node(
    socket: Arc<dyn DatagramSocket>,
    clock: &ManualClock,
    addr: SocketAddr,
    relay_serving: bool,
) -> Node {
    let id = Arc::new(Identity::generate_with_pow_bits(POW));
    let static_pub = id.public_key();
    let (ep, events) = Endpoint::new(
        socket,
        id,
        Arc::new(clock.clone()) as Arc<dyn accord_transport::Clock>,
        config(relay_serving),
    );
    ep.spawn();
    Node {
        ep,
        events,
        addr,
        static_pub,
    }
}

fn presence(custom: &str) -> ChannelMsg {
    ChannelMsg::Core(accord_proto::core_msg::CoreMsg::Presence {
        status: 0,
        custom: Some(custom.into()),
    })
}

/// Établit une session directe `client → relais` et attend, borné, que les deux
/// extrémités l'aient enregistrée (même helper que `relay_e2e`).
async fn establish(client: &Node, relay: &Node, expected_relay_sessions: usize) {
    client.ep.connect(relay.addr).await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if client.ep.session_count() >= 1 && relay.ep.session_count() >= expected_relay_sessions
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("session client↔relais établie");
}

/// Attend (borné) un `Connected` dont l'identité est `expected`, en ignorant les
/// événements antérieurs (Connected d'une autre session, RelayAccepted…).
async fn wait_connected_with(node: &mut Node, expected: [u8; 32]) {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            match node.events.recv().await {
                Some(TransportEvent::Connected { static_pub, .. }) if static_pub == expected => {
                    return
                }
                Some(_) => continue,
                None => panic!("canal d'événements fermé"),
            }
        }
    })
    .await
    .expect("Connected attendu avec l'identité visée");
}

/// Attend (borné) le prochain message applicatif remonté.
async fn recv_app_message(node: &mut Node) -> ChannelMsg {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            match node.events.recv().await {
                Some(TransportEvent::Message { msg, .. }) => return *msg,
                Some(_) => continue,
                None => panic!("canal d'événements fermé"),
            }
        }
    })
    .await
    .expect("message applicatif attendu")
}

/// Ouvre une session directe `from → to` et attend (borné) que les DEUX
/// extrémités aient atteint au moins le nombre de sessions indiqué. Utile quand
/// l'un des nœuds possède déjà des sessions (le simple `establish` court-circuite
/// alors le test de comptage).
async fn connect_until(from: &Node, from_min: usize, to: &Node, to_min: usize) {
    from.ep.connect(to.addr).await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if from.ep.session_count() >= from_min && to.ep.session_count() >= to_min {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("session directe établie");
}

/// Établit un tunnel A↔B à travers le relais R (les sessions directes A↔R et B↔R
/// sont supposées déjà en place) et rend l'identifiant de circuit côté A.
async fn establish_tunnel(alice: &mut Node, relay: &Node, bob: &mut Node) -> u32 {
    let circuit = alice
        .ep
        .open_relay_circuit(relay.addr, relay.ep.node_id(), bob.static_pub)
        .await
        .expect("circuit ouvert");
    alice
        .ep
        .connect_via_relay(circuit, bob.static_pub)
        .await
        .expect("handshake tunnelé lancé");
    wait_connected_with(alice, bob.static_pub).await;
    wait_connected_with(bob, alice.static_pub).await;
    circuit
}

/// `RelayMsg::Data{circuit, blob}` prêt à émettre via une session directe.
fn relay_data(circuit: u32, blob: Vec<u8>) -> ChannelMsg {
    ChannelMsg::Relay(RelayMsg::Data { circuit, blob })
}

/// `RelayMsg::Close{circuit}` prêt à émettre via une session directe.
fn relay_close(circuit: u32) -> ChannelMsg {
    ChannelMsg::Relay(RelayMsg::Close { circuit })
}

/// Sérialise un `Packet::Hello` frais pour l'identité `id` (blob à tunneler).
fn hello_blob(id: &Identity, now: u64) -> Vec<u8> {
    let init = Initiator::start(id, now, Vec::new(), POW, None, None);
    Packet::Hello(init.hello().clone()).to_bytes()
}

#[tokio::test]
async fn tunnel_relais_etablit_session_et_echange_bidirectionnel() {
    // Topologie : A et B ne peuvent PAS se joindre directement (chacun bloque
    // l'adresse de l'autre), mais tous deux joignent le relais R. Leur session
    // bout-en-bout doit s'établir ET porter du trafic applicatif dans les deux
    // sens, EXCLUSIVEMENT via R.
    let clock = ManualClock::new(1_000_000);
    let net = SimNet::new(601, NetConditions::default());
    let a_addr = "10.60.0.1:4000";
    let r_addr = "10.60.0.2:4000";
    let b_addr = "10.60.0.3:4000";
    let mut alice = spawn_blocking(&net, &clock, a_addr, false, &[b_addr]);
    let relay = spawn_node(&net, &clock, r_addr, true);
    let mut bob = spawn_blocking(&net, &clock, b_addr, false, &[a_addr]);

    // Sessions directes A↔R puis B↔R (R doit finir avec 2 sessions).
    establish(&alice, &relay, 1).await;
    establish(&bob, &relay, 2).await;

    // Preuve d'absence de route directe : un envoi direct A→B n'établit AUCUNE
    // session chez B (le HELLO est avalé par le lien bloqué).
    alice.ep.send(bob.addr, &presence("direct")).await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(
        bob.ep.session_count(),
        1,
        "aucune session directe A↔B : la route directe est bien coupée"
    );

    // A ouvre un circuit vers B via R, puis initie la session A↔B à travers lui.
    let circuit = alice
        .ep
        .open_relay_circuit(relay.addr, relay.ep.node_id(), bob.static_pub)
        .await
        .expect("circuit ouvert");
    alice
        .ep
        .connect_via_relay(circuit, bob.static_pub)
        .await
        .expect("handshake tunnelé lancé");

    // Les deux extrémités voient un Connected avec l'identité de l'autre.
    wait_connected_with(&mut alice, bob.static_pub).await;
    wait_connected_with(&mut bob, alice.static_pub).await;

    // A → B : message applicatif via le tunnel, reçu octet pour octet.
    let msg_ab = presence("bonjour-de-A");
    alice.ep.send_via_relay(circuit, &msg_ab).await.unwrap();
    assert_eq!(recv_app_message(&mut bob).await, msg_ab);

    // B → A : réponse sur le même circuit (B le retrouve par le NodeId de A).
    let bob_circuit = bob
        .ep
        .circuit_for_peer(alice.ep.node_id())
        .expect("B connaît le circuit vers A");
    let msg_ba = presence("reponse-de-B");
    bob.ep.send_via_relay(bob_circuit, &msg_ba).await.unwrap();
    assert_eq!(recv_app_message(&mut alice).await, msg_ba);

    // Sessions bout-en-bout des deux côtés (A↔R + A↔B ; B↔R + B↔A) ; R n'a que
    // ses deux sessions directes : il ne fait qu'acheminer.
    assert_eq!(alice.ep.session_count(), 2, "A : A↔R + A↔B");
    assert_eq!(bob.ep.session_count(), 2, "B : B↔R + B↔A");
    assert_eq!(
        relay.ep.session_count(),
        2,
        "R : A↔R + B↔R (aucune session E2E)"
    );
}

#[tokio::test]
async fn tunnel_refuse_identite_incorrecte_au_bout_du_circuit() {
    // Liaison d'identité (D-037) à travers le tunnel : A ouvre un circuit dont le
    // bout est Mallory (c'est là que R route honnêtement), MAIS initie la session
    // en se liant à l'identité de Bob. Mallory répond avec SA clé statique : la
    // liaison refuse la session. Le relais ne peut jamais substituer un pair.
    let clock = ManualClock::new(1_000_000);
    let net = SimNet::new(602, NetConditions::default());
    let a_addr = "10.61.0.1:4000";
    let r_addr = "10.61.0.2:4000";
    let m_addr = "10.61.0.3:4000";
    let alice = spawn_blocking(&net, &clock, a_addr, false, &[m_addr]);
    let relay = spawn_node(&net, &clock, r_addr, true);
    let mallory = spawn_blocking(&net, &clock, m_addr, false, &[a_addr]);

    // Identité tierce (le « vrai Bob »), absente du réseau et distincte de Mallory.
    let bob_static = Identity::generate_with_pow_bits(POW).public_key();
    assert_ne!(bob_static, mallory.static_pub);

    establish(&alice, &relay, 1).await;
    establish(&mallory, &relay, 2).await;

    // Circuit ouvert vers Mallory (R route vers lui), mais handshake lié à Bob.
    let circuit = alice
        .ep
        .open_relay_circuit(relay.addr, relay.ep.node_id(), mallory.static_pub)
        .await
        .expect("circuit ouvert vers Mallory");
    alice
        .ep
        .connect_via_relay(circuit, bob_static)
        .await
        .expect("handshake lancé, lié à l'identité de Bob");

    // Laisse HELLO/WELCOME s'échanger via R. Mallory répond, mais A refuse la
    // liaison : aucune session bout-en-bout ne doit s'établir chez A.
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(
        alice.ep.session_count(),
        1,
        "A ne garde que A↔R : la session au bout du circuit est refusée (D-037)"
    );
}

#[tokio::test]
async fn ouverture_de_circuit_refusee_est_remontee() {
    // Un relais qui n'assure pas le service refuse l'ouverture : l'erreur typée
    // remonte à l'appelant de `open_relay_circuit`.
    let clock = ManualClock::new(1_000_000);
    let net = SimNet::new(603, NetConditions::default());
    let alice = spawn_node(&net, &clock, "10.62.0.1:4000", false);
    // R n'assure PAS le relais → REJECT_NOT_RELAY (0x01).
    let relay = spawn_node(&net, &clock, "10.62.0.2:4000", false);

    establish(&alice, &relay, 1).await;

    let peer_static = Identity::generate_with_pow_bits(POW).public_key();
    let err = alice
        .ep
        .open_relay_circuit(relay.addr, relay.ep.node_id(), peer_static)
        .await
        .expect_err("l'ouverture doit être refusée");
    assert!(
        matches!(err, TransportError::RelayOpenRejected(0x01)),
        "REJECT_NOT_RELAY attendu, obtenu {err:?}"
    );
}

#[tokio::test]
async fn noeud_servant_est_aussi_extremite_cliente() {
    // Lève la limitation « un nœud servant ne peut être extrémité d'un circuit » :
    // R1 SERT le circuit A↔B ET est lui-même extrémité CLIENTE d'un circuit R1↔C
    // via R2. Les deux circuits portent le MÊME identifiant (1, chaque relais
    // ayant sa propre numérotation) : l'aiguillage doit trancher par la
    // provenance (le relais d'où arrive le `Data`).
    let clock = ManualClock::new(1_000_000);
    let net = SimNet::new(604, NetConditions::default());
    let mut alice = spawn_node(&net, &clock, "10.63.0.1:4000", false);
    let mut r1 = spawn_node(&net, &clock, "10.63.0.2:4000", true);
    let mut bob = spawn_node(&net, &clock, "10.63.0.3:4000", false);
    let r2 = spawn_node(&net, &clock, "10.63.0.4:4000", true);
    let mut carol = spawn_node(&net, &clock, "10.63.0.5:4000", false);

    // Sessions directes : A↔R1, B↔R1, R1↔R2, C↔R2.
    establish(&alice, &r1, 1).await;
    establish(&bob, &r1, 2).await;
    establish(&r1, &r2, 1).await;
    establish(&carol, &r2, 2).await;

    // A↔B à travers R1 : R1 SERT ce circuit (id 1 dans sa table).
    let c_ab = alice
        .ep
        .open_relay_circuit(r1.addr, r1.ep.node_id(), bob.static_pub)
        .await
        .unwrap();
    alice
        .ep
        .connect_via_relay(c_ab, bob.static_pub)
        .await
        .unwrap();
    wait_connected_with(&mut alice, bob.static_pub).await;
    wait_connected_with(&mut bob, alice.static_pub).await;

    // R1↔C à travers R2 : R1 est extrémité CLIENTE (id 1 dans la table de R2),
    // tout en continuant de servir A↔B.
    let c_r1c = r1
        .ep
        .open_relay_circuit(r2.addr, r2.ep.node_id(), carol.static_pub)
        .await
        .unwrap();
    r1.ep
        .connect_via_relay(c_r1c, carol.static_pub)
        .await
        .unwrap();
    wait_connected_with(&mut r1, carol.static_pub).await;
    wait_connected_with(&mut carol, r1.static_pub).await;

    // Même id des deux côtés : c'est bien le cas de collision que la provenance
    // doit départager.
    assert_eq!(c_ab, c_r1c, "collision d'identifiant de circuit attendue");

    // Aiguillage SERVI : A→B est FORWARDÉ par R1 (R1 n'émet pas de Message).
    let m_ab = presence("A-vers-B");
    alice.ep.send_via_relay(c_ab, &m_ab).await.unwrap();
    assert_eq!(recv_app_message(&mut bob).await, m_ab);

    // Aiguillage CLIENT : C→R1 est RÉINJECTÉ chez R1 (extrémité), pas forwardé.
    let carol_circuit = carol.ep.circuit_for_peer(r1.ep.node_id()).unwrap();
    let m_cr1 = presence("C-vers-R1");
    carol
        .ep
        .send_via_relay(carol_circuit, &m_cr1)
        .await
        .unwrap();
    assert_eq!(recv_app_message(&mut r1).await, m_cr1);
}

#[tokio::test]
async fn premier_contact_etranger_via_relais_domicile() {
    // PREMIER CONTACT sans port ouvert (SPEC §11.3, rendez-vous domicile) :
    // Alice et Bob sont des ÉTRANGERS (aucun échange préalable, pas d'amitié,
    // donc aucun relais de clé de paire calculable) et n'ont PAS de route
    // directe. Le relais n'achemine que vers un pair avec lequel il a DÉJÀ une
    // session : tant que Bob n'a pas enregistré son relais domicile,
    // l'ouverture est refusée (REJECT_NO_TARGET) ; dès que Bob y entretient sa
    // session, une inconnue ouvre un circuit, tunnelle son handshake (PoW
    // vérifié côté cible) et livre sa demande d'ami.
    let clock = ManualClock::new(1_000_000);
    let net = SimNet::new(614, NetConditions::default());
    let a_addr = "10.73.0.1:4000";
    let r_addr = "10.73.0.2:4000";
    let b_addr = "10.73.0.3:4000";
    let mut alice = spawn_blocking(&net, &clock, a_addr, false, &[b_addr]);
    let relay = spawn_node(&net, &clock, r_addr, true);
    let mut bob = spawn_blocking(&net, &clock, b_addr, false, &[a_addr]);

    // Alice joint le relais domicile de Bob (dérivé de la seule clé publique
    // de Bob côté nœud ; ici le mesh n'a qu'un relais).
    establish(&alice, &relay, 1).await;

    // AVANT l'enregistrement domicile de Bob : le relais n'a aucune session
    // avec lui, l'ouverture de circuit est refusée (REJECT_NO_TARGET = 0x02).
    let err = alice
        .ep
        .open_relay_circuit(relay.addr, relay.ep.node_id(), bob.static_pub)
        .await
        .expect_err("sans session Bob↔relais, l'ouverture doit être refusée");
    assert!(
        matches!(err, TransportError::RelayOpenRejected(0x02)),
        "REJECT_NO_TARGET attendu, obtenu {err:?}"
    );

    // Enregistrement domicile : Bob entretient une session directe vers SON
    // relais (ce que fait la passe de maintenance côté nœud).
    establish(&bob, &relay, 2).await;

    // Une ÉTRANGÈRE ouvre maintenant le circuit et tunnelle son handshake :
    // les deux extrémités voient un Connected lié à l'identité de l'autre.
    let circuit = establish_tunnel(&mut alice, &relay, &mut bob).await;

    // La demande d'ami — le tout premier message jamais échangé — est livrée
    // à travers le tunnel, sans que Bob n'ait ouvert le moindre port.
    let demande = ChannelMsg::Core(accord_proto::core_msg::CoreMsg::FriendRequest {
        display_name: "Alice".into(),
        message: "premier contact".into(),
        verify_phrase: None,
    });
    alice.ep.send_via_relay(circuit, &demande).await.unwrap();
    assert_eq!(recv_app_message(&mut bob).await, demande);
}

// ===========================================================================
// Tests de régression des failles de sécurité du tunnel relais (revue
// adversariale). Un test par faille : A (identité), B (DoS/panic), C (mémoire),
// D (provenance de fermeture).
// ===========================================================================

#[tokio::test]
async fn faille_a_tunnel_ignore_hello_reinjecte_d_identite_non_liee() {
    // FAILLE A (liaison d'identité contournée) : un relais malveillant réinjecte,
    // sur un circuit que A a ouvert vers B, un `Data{blob: Hello(Z)}` portant une
    // AUTRE identité. Sans liaison d'identité au niveau réinjection, `on_hello`
    // répondrait, installerait une session avec Z tout en gardant `peer_static`
    // lié à B, et ÉCRASERAIT le vrai handshake → MITM complet. Le correctif
    // l'ignore ; le tunnel A↔B reste intact.
    let clock = ManualClock::new(1_000_000);
    let net = SimNet::new(607, NetConditions::default());
    let mut alice = spawn_node(&net, &clock, "10.66.0.1:4000", false);
    let relay = spawn_node(&net, &clock, "10.66.0.2:4000", true);
    let mut bob = spawn_node(&net, &clock, "10.66.0.3:4000", false);

    establish(&alice, &relay, 1).await;
    establish(&bob, &relay, 2).await;
    let circuit = establish_tunnel(&mut alice, &relay, &mut bob).await;

    // Le relais (piloté ici par le test) réinjecte, scellé sous la session A↔R, un
    // HELLO d'un tiers Z sur le circuit : A le reçoit comme un `RelayMsg::Data` de
    // SON relais → chemin de réinjection, exactement le vecteur de la faille.
    let z = Identity::generate_with_pow_bits(POW);
    let z_static = z.public_key();
    let blob = hello_blob(&z, clock.now_ms());
    relay
        .ep
        .send(alice.addr, &relay_data(circuit, blob))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;

    // 1. Aucune session ne s'établit avec Z (aucun `Connected(Z)` remonté).
    let saw_z = tokio::time::timeout(Duration::from_millis(300), async {
        loop {
            match alice.events.recv().await {
                Some(TransportEvent::Connected { static_pub, .. }) if static_pub == z_static => {
                    return true
                }
                Some(_) => continue,
                None => return false,
            }
        }
    })
    .await
    .unwrap_or(false);
    assert!(
        !saw_z,
        "aucune session ne doit s'établir avec l'identité injectée Z"
    );

    // 2. A garde exactement ses 2 sessions (A↔R, A↔B) et le circuit reste lié à B.
    assert_eq!(alice.ep.session_count(), 2, "pas de session supplémentaire");
    let (_, _, bound) = alice.ep.relay_circuit_descriptor(circuit).unwrap();
    assert_eq!(
        bound, bob.static_pub,
        "le circuit reste lié à l'identité de B"
    );

    // 3. Preuve décisive : le tunnel A↔B fonctionne toujours. Sous l'ancien code,
    //    la session du circuit aurait été remplacée par A↔Z et B ne pourrait plus
    //    déchiffrer ce message.
    let msg = presence("apres-attaque");
    alice.ep.send_via_relay(circuit, &msg).await.unwrap();
    assert_eq!(recv_app_message(&mut bob).await, msg);
}

#[tokio::test]
async fn faille_b_session_tunnelee_expiree_nettoie_le_circuit_sans_paniquer() {
    // FAILLE B (correctifs 2 & 3) : à l'expiration d'inactivité d'une session
    // tunnelée, `client_circuits` doit être nettoyé, et un `send_via_relay`
    // ultérieur rendre une ERREUR gérée — jamais une panic qui empoisonnerait le
    // mutex d'état (DoS permanent déclenchable à distance).
    let clock = ManualClock::new(1_000_000);
    let net = SimNet::new(608, NetConditions::default());
    let mut alice = spawn_node(&net, &clock, "10.67.0.1:4000", false);
    let relay = spawn_node(&net, &clock, "10.67.0.2:4000", true);
    let mut bob = spawn_node(&net, &clock, "10.67.0.3:4000", false);

    establish(&alice, &relay, 1).await;
    establish(&bob, &relay, 2).await;
    let circuit = establish_tunnel(&mut alice, &relay, &mut bob).await;
    assert_eq!(alice.ep.client_circuit_count(), 1);

    // Force l'expiration d'inactivité (au-delà de `idle_timeout_ms`) puis
    // déclenche la maintenance de façon déterministe.
    clock.advance(EndpointConfig::default().idle_timeout_ms + 1_000);
    alice.ep.run_maintenance().await;

    // Le circuit client est nettoyé (correctif 2).
    assert_eq!(
        alice.ep.client_circuit_count(),
        0,
        "circuit client nettoyé à l'expiration de la session tunnelée"
    );
    // `send_via_relay` rend une erreur, ne panique pas (correctif 3).
    let err = alice
        .ep
        .send_via_relay(circuit, &presence("post-expiration"))
        .await
        .expect_err("une erreur est attendue après nettoyage");
    assert!(
        matches!(err, TransportError::UnknownPeer),
        "UnknownPeer attendu, obtenu {err:?}"
    );
}

#[tokio::test]
async fn faille_b_keepalive_tunnele_est_enveloppe_et_maintient_la_session() {
    // FAILLE B (correctif 1) : le keep-alive d'une session tunnelée doit voyager
    // DANS le tunnel (`RelayMsg::Data`). Émis brut vers le relais, il serait rejeté
    // (session_id inconnu) et la session finirait par expirer à tort. Enveloppé,
    // il atteint le pair, qui répond, et la session survit bien au-delà d'un
    // `idle_timeout_ms`.
    let clock = ManualClock::new(1_000_000);
    let net = SimNet::new(609, NetConditions::default());
    let mut alice = spawn_node(&net, &clock, "10.68.0.1:4000", false);
    let relay = spawn_node(&net, &clock, "10.68.0.2:4000", true);
    let mut bob = spawn_node(&net, &clock, "10.68.0.3:4000", false);

    establish(&alice, &relay, 1).await;
    establish(&bob, &relay, 2).await;
    let circuit = establish_tunnel(&mut alice, &relay, &mut bob).await;

    // Simule le temps par pas > keepalive (25 s) mais < idle (120 s), en pilotant
    // la maintenance des trois nœuds. Total simulé (180 s) ≫ idle : sans le
    // correctif, A↔B serait morte à ~120 s ; avec, les keep-alives tunnelés la
    // maintiennent (ping A→R→B, pong B→R→A ⇒ `last_recv` rafraîchi des deux côtés).
    for _ in 0..6 {
        clock.advance(30_000);
        alice.ep.run_maintenance().await;
        bob.ep.run_maintenance().await;
        relay.ep.run_maintenance().await;
        // Laisse le round-trip se propager dans le mesh simulé.
        tokio::time::sleep(Duration::from_millis(150)).await;
    }

    // Les sessions bout-en-bout survivent des deux côtés.
    assert_eq!(
        alice.ep.session_count(),
        2,
        "A↔R + A↔B maintenues par le keep-alive tunnelé"
    );
    assert_eq!(bob.ep.session_count(), 2, "B↔R + B↔A maintenues");
    // Et le tunnel porte toujours du trafic applicatif.
    let msg = presence("apres-keepalive");
    alice.ep.send_via_relay(circuit, &msg).await.unwrap();
    assert_eq!(recv_app_message(&mut bob).await, msg);
}

#[tokio::test]
async fn faille_c_client_circuits_plafonnes_contre_faux_hello_tunneles() {
    // FAILLE C (plafond) : un pair doté d'une simple session directe encapsule un
    // flux de faux HELLO tunnelés sur des circuits frais. Sans plafond,
    // `client_circuits` croîtrait sans borne, même sur un nœud non-relais.
    let clock = ManualClock::new(1_000_000);
    let net = SimNet::new(610, NetConditions::default());
    let victim = spawn_node(&net, &clock, "10.69.0.1:4000", false);
    let attacker = spawn_node(&net, &clock, "10.69.0.2:4000", false);

    // Session directe attaquant ↔ victime (le vecteur d'encapsulation).
    establish(&attacker, &victim, 1).await;

    let z = Identity::generate_with_pow_bits(POW);
    let cap = accord_transport::relay::MAX_CIRCUITS;
    let total = cap + 16;
    let now = clock.now_ms();
    for i in 0..total {
        let blob = hello_blob(&z, now);
        attacker
            .ep
            .send(victim.addr, &relay_data(1_000 + i as u32, blob))
            .await
            .unwrap();
    }

    // Le flux est traité de façon asynchrone : on attend, borné, que le plafond
    // soit atteint, puis on vérifie qu'il n'est jamais DÉPASSÉ.
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if victim.ep.client_circuit_count() >= cap {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("le plafond doit être atteint");
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(
        victim.ep.client_circuit_count(),
        cap,
        "client_circuits reste plafonné malgré {total} faux HELLO tunnelés"
    );
}

#[tokio::test]
async fn faille_c_circuit_client_inacheve_est_purge_par_la_maintenance() {
    // FAILLE C (expiration) : une entrée `client_circuits` dont le handshake
    // bout-en-bout n'aboutit jamais (`session_id == None`) est balayée après le
    // délai de maintenance, sans casser une session directe encore active.
    let clock = ManualClock::new(1_000_000);
    let net = SimNet::new(611, NetConditions::default());
    let alice = spawn_node(&net, &clock, "10.70.0.1:4000", false);
    let relay = spawn_node(&net, &clock, "10.70.0.2:4000", true);
    let bob = spawn_node(&net, &clock, "10.70.0.3:4000", false);

    establish(&alice, &relay, 1).await;
    establish(&bob, &relay, 2).await;

    // A ouvre un circuit vers B mais NE lance PAS le handshake : l'entrée reste
    // `session_id == None`.
    alice
        .ep
        .open_relay_circuit(relay.addr, relay.ep.node_id(), bob.static_pub)
        .await
        .expect("circuit ouvert");
    assert_eq!(alice.ep.client_circuit_count(), 1);

    // Avance au-delà du délai de handshake (mais SOUS l'idle des sessions) et
    // déclenche la maintenance : l'entrée inachevée est purgée, A↔R survit.
    clock.advance(31_000);
    alice.ep.run_maintenance().await;
    assert_eq!(
        alice.ep.client_circuit_count(),
        0,
        "circuit client inachevé purgé après le délai"
    );
    assert_eq!(alice.ep.session_count(), 1, "la session directe A↔R survit");
}

#[tokio::test]
async fn faille_d_close_relais_exige_la_provenance_du_pair() {
    // FAILLE D : `RelayMsg::Close` doit être lié à la provenance. Un tiers ne peut
    // fermer ni un circuit CLIENT d'autrui, ni un circuit HÉBERGÉ entre deux
    // autres pairs (identifiants petits et devinables). Seul le relais hôte (côté
    // client) ou une extrémité (côté serveur) le peut.
    let clock = ManualClock::new(1_000_000);
    let net = SimNet::new(612, NetConditions::default());
    let mut alice = spawn_node(&net, &clock, "10.71.0.1:4000", false);
    let relay = spawn_node(&net, &clock, "10.71.0.2:4000", true);
    let mut bob = spawn_node(&net, &clock, "10.71.0.3:4000", false);
    let attacker = spawn_node(&net, &clock, "10.71.0.4:4000", false);

    establish(&alice, &relay, 1).await;
    establish(&bob, &relay, 2).await;
    let circuit = establish_tunnel(&mut alice, &relay, &mut bob).await;

    // L'attaquant se dote de sessions directes avec le relais ET avec A (sans
    // aucun rapport avec le circuit A↔B).
    connect_until(&attacker, 1, &relay, 3).await;
    connect_until(&attacker, 2, &alice, 3).await;

    // --- Côté SERVEUR : l'attaquant tente de fermer le circuit hébergé A↔B. ---
    attacker
        .ep
        .send(relay.addr, &relay_close(circuit))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(150)).await;
    // Le relais n'a rien fermé : A→B est toujours acheminé.
    let m1 = presence("serveur-provenance");
    alice.ep.send_via_relay(circuit, &m1).await.unwrap();
    assert_eq!(recv_app_message(&mut bob).await, m1);

    // --- Côté CLIENT : l'attaquant tente de fermer le circuit client de A. ---
    assert_eq!(alice.ep.client_circuit_count(), 1);
    attacker
        .ep
        .send(alice.addr, &relay_close(circuit))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(
        alice.ep.client_circuit_count(),
        1,
        "un tiers ne peut pas fermer le circuit client de A (FAILLE D)"
    );
    // Le tunnel fonctionne encore (B→A).
    let bob_circuit = bob.ep.circuit_for_peer(alice.ep.node_id()).unwrap();
    let m2 = presence("client-provenance");
    bob.ep.send_via_relay(bob_circuit, &m2).await.unwrap();
    assert_eq!(recv_app_message(&mut alice).await, m2);

    // --- Le VRAI relais, lui, PEUT fermer le circuit client de A. ---
    relay
        .ep
        .send(alice.addr, &relay_close(circuit))
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if alice.ep.client_circuit_count() == 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("le relais hôte doit pouvoir fermer le circuit client");
}

/// Sérialise un `Packet::Hello` INVALIDE (PoW ET signature ne passant PAS
/// `respond`) pour l'identité `id` : c'est le « garbage » à coût crypto nul du
/// flood de la FAILLE C-bis. On part d'un HELLO valide puis on choisit un
/// `pow_nonce` qui ÉCHOUE la preuve de travail (de façon déterministe) et on
/// corrompt la signature.
fn hello_blob_invalide(id: &Identity, now: u64) -> Vec<u8> {
    let init = Initiator::start(id, now, Vec::new(), POW, None, None);
    let mut hello = init.hello().clone();
    // `pow_nonce` déterministe qui ÉCHOUE la preuve de travail : la réservation
    // d'un slot exige désormais un PoW valide, que ce HELLO n'a pas.
    let mut bad = hello.pow_nonce.wrapping_add(1);
    while accord_crypto::verify_pow(&hello.static_pub, bad, POW) {
        bad = bad.wrapping_add(1);
    }
    hello.pow_nonce = bad;
    // Signature aussi invalidée (défense en profondeur : même si le PoW passait
    // par chance, `respond` la rejetterait au rollback).
    hello.sig[0] ^= 0xFF;
    Packet::Hello(hello).to_bytes()
}

#[tokio::test]
async fn faille_c_bis_hello_invalide_ne_reserve_aucun_slot_et_le_legitime_passe() {
    // FAILLE C-bis (résiduelle du correctif C) : l'ancien code réservait un slot
    // `client_circuits` AVANT toute validation. Un pair doté d'une simple session
    // directe pouvait donc, à coût crypto nul par paquet, encapsuler un flux de
    // HELLO INVALIDES (PoW/signature bidon) sur des circuits frais et SATURER le
    // plafond GLOBAL (64) avec du garbage — bloquant tout circuit tunnelé entrant
    // LÉGITIME. Le correctif exige une preuve de travail valide AVANT de réserver :
    // le garbage ne consomme plus aucun slot, et un handshake tunnelé légitime
    // aboutit toujours après le flood.
    let clock = ManualClock::new(1_000_000);
    let net = SimNet::new(613, NetConditions::default());
    let mut victim = spawn_node(&net, &clock, "10.72.0.1:4000", false);
    let attacker = spawn_node(&net, &clock, "10.72.0.2:4000", false);

    // Session directe attaquant ↔ victime : l'UNIQUE coût PoW payé par l'attaquant.
    establish(&attacker, &victim, 1).await;

    // Flood de HELLO INVALIDES sur des circuits frais, bien au-delà du plafond.
    let garbage = Identity::generate_with_pow_bits(POW);
    let cap = accord_transport::relay::MAX_CIRCUITS;
    let flood = cap + 40; // 104 pour cap = 64
    let now = clock.now_ms();
    for i in 0..flood {
        let blob = hello_blob_invalide(&garbage, now);
        attacker
            .ep
            .send(victim.addr, &relay_data(2_000 + i as u32, blob))
            .await
            .unwrap();
    }

    // Laisse le flood se traiter, puis vérifie qu'AUCUN slot n'a été réservé : le
    // garbage est rejeté AVANT réservation (contraste avec l'ancien code, où le
    // plafond se serait saturé à 64 entrées `session_id: None`).
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(
        victim.ep.client_circuit_count(),
        0,
        "un flux de {flood} HELLO invalides ne doit réserver AUCUN slot"
    );

    // Un pair LÉGITIME (identité valide, HELLO signé) ouvre un circuit tunnelé
    // entrant via le MÊME vecteur : il est accepté. Le plafond n'a pas été saturé
    // par le garbage, et le rate-limiter par IP n'a pas été entamé par le flood
    // (celui-ci est écarté AVANT `on_hello`).
    let legit = Identity::generate_with_pow_bits(POW);
    let legit_static = legit.public_key();
    let blob = hello_blob(&legit, now);
    attacker
        .ep
        .send(victim.addr, &relay_data(3_000, blob))
        .await
        .unwrap();

    // La victime établit la session tunnelée et signale `Connected(legit)`.
    wait_connected_with(&mut victim, legit_static).await;
    assert_eq!(
        victim.ep.client_circuit_count(),
        1,
        "seul le circuit LÉGITIME occupe un slot après le flood"
    );
    assert_eq!(
        victim.ep.session_count(),
        2,
        "session directe attaquant↔victime + session tunnelée légitime"
    );
}
