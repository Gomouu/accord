//! Résolution de code ami quand l'invité est derrière un NAT (le vrai premier
//! pas d'une invitation : « ajouter par code »). L'inviteur A résout le CODE
//! AMI de B — `FIND_VALUE` DHT du record d'identité de B — sans jamais avoir
//! reçu la clé publique de B autrement.
//!
//! C'est le chemin qui produit « Code ami introuvable sur le réseau » en
//! production quand B est NATé et que le correctif premier-contact n'a pas
//! suffi : `nat_first_contact_e2e` PARTAGE déjà la clé publique et saute donc
//! cette étape. Ici on ne partage QUE le code (dérivé de la clé, mais résolu
//! via la DHT comme le ferait l'UI).
//!
//! Topologie : R public (nœud d'amorçage/relais partagé), A et B derrière deux
//! NAT symétriques distincts. B publie son record d'identité par la maintenance
//! ; A doit le résoudre via R sans ouvrir de port.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use accord_crypto::FriendCode;
use accord_node::{identity, run_with_socket, MaintenanceConfig, NodeConfig, Paths, RunningNode};
use accord_transport::socket::sim::{NetConditions, SimNet};

const PASSPHRASE: &str = "phrase-de-passe-test";

fn fast_maintenance() -> MaintenanceConfig {
    MaintenanceConfig {
        enabled: true,
        identity_republish: Duration::from_millis(300),
        presence_publish: Duration::from_millis(200),
        presence_resolve: Duration::from_millis(300),
        outbox_flush: Duration::from_millis(400),
        mailbox_poll: Duration::from_millis(500),
        group_sync: Duration::from_millis(300),
        event_check: Duration::from_millis(300),
        bootstrap_reconnect: Duration::from_millis(300),
        jitter: 0.2,
        outbox_batch: 16,
        contacts_per_tick: 8,
        mailbox_after_attempts: 2,
    }
}

async fn boot_sim(dir: &std::path::Path, net: &SimNet, addr: SocketAddr) -> RunningNode {
    let paths = Paths::new(dir);
    let unlocked = if paths.has_identity() {
        identity::unlock(&paths, PASSPHRASE).unwrap()
    } else {
        identity::create(&paths, PASSPHRASE, 1).unwrap()
    };
    let config = NodeConfig {
        paths,
        p2p_addr: addr,
        api_port: 0,
        pow_bits: 1,
        nat_enabled: false,
        mdns_enabled: false,
        ..NodeConfig::default()
    };
    let socket = Arc::new(net.bind(addr));
    run_with_socket(unlocked, config, fast_maintenance(), socket)
        .await
        .unwrap()
}

/// Résout `code` sur `who`, avec plusieurs tentatives bornées (la publication
/// du record d'identité de la cible + la propagation prennent quelques passes).
async fn resolve_eventually(who: &RunningNode, code: &FriendCode, secs: u64) -> Option<[u8; 32]> {
    for _ in 0..(secs * 4) {
        if let Ok(pk) = who.resolve_friend_code(code).await {
            return Some(pk);
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    who.resolve_friend_code(code).await.ok()
}

#[tokio::test]
async fn resout_le_code_ami_d_un_invite_nate() {
    let net = SimNet::new(2, NetConditions::default());
    let r_addr: SocketAddr = "127.1.0.1:4000".parse().unwrap();
    let a_addr: SocketAddr = "10.0.0.2:4001".parse().unwrap();
    let b_addr: SocketAddr = "10.0.1.2:4002".parse().unwrap();
    net.set_symmetric_nat(a_addr, "127.8.0.1".parse().unwrap());
    net.set_symmetric_nat(b_addr, "127.9.0.1".parse().unwrap());

    let dir_r = tempfile::tempdir().unwrap();
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let r = boot_sim(dir_r.path(), &net, r_addr).await;
    let a = boot_sim(dir_a.path(), &net, a_addr).await;
    let b = boot_sim(dir_b.path(), &net, b_addr).await;

    // Amorçage par adresse SEULE vers le nœud public partagé.
    a.add_bootstrap_peer(r_addr).await.unwrap();
    b.add_bootstrap_peer(r_addr).await.unwrap();

    let b_pub = b.node.public_key();
    // L'inviteur A ne connaît QUE le code ami de B (ce qu'on partage hors bande).
    let b_code = FriendCode::of_pubkey(&b_pub);

    let resolved = resolve_eventually(&a, &b_code, 60).await;
    assert_eq!(
        resolved,
        Some(b_pub),
        "A n'a pas pu résoudre le code ami de B derrière un NAT symétrique \
         (« Code ami introuvable sur le réseau »)"
    );

    a.shutdown();
    b.shutdown();
    r.shutdown();
}
