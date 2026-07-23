//! Cause 3 (Lot G): the runtime and its UDP socket are fully released at
//! shutdown. Before the fix, `event_loop`/`outbound_loop` ignored the stop
//! signal and `recv_loop` parked on `recv_from`, so their `Arc` cycle kept the
//! whole runtime — and the bound UDP port — alive across every lock/unlock.
//! Here we prove the port frees: a plain rebind on the old port succeeds only
//! once every loop has exited and dropped its `Arc<Endpoint>`.

use std::net::SocketAddr;
use std::time::Duration;

use accord_node::{
    identity, run_with_maintenance, MaintenanceConfig, NodeConfig, Paths, RunningNode,
};

const PASSPHRASE: &str = "phrase-de-passe-test";

fn fast_maintenance() -> MaintenanceConfig {
    MaintenanceConfig {
        dht_republish: Duration::from_secs(3600),
        enabled: true,
        identity_republish: Duration::from_millis(500),
        presence_publish: Duration::from_millis(150),
        presence_resolve: Duration::from_millis(150),
        outbox_flush: Duration::from_millis(400),
        mailbox_poll: Duration::from_millis(500),
        group_sync: Duration::from_millis(300),
        event_check: Duration::from_millis(300),
        bootstrap_reconnect: Duration::from_millis(300),
        jitter: 0.2,
        outbox_batch: 16,
        contacts_per_tick: 8,
        mailbox_after_attempts: 2,
        ephemeral_purge: Duration::from_secs(3600),
    }
}

async fn boot(dir: &std::path::Path) -> RunningNode {
    let paths = Paths::new(dir);
    let unlocked = if paths.has_identity() {
        identity::unlock(&paths, PASSPHRASE).unwrap()
    } else {
        identity::create(&paths, PASSPHRASE, 1).unwrap()
    };
    let config = NodeConfig {
        paths,
        p2p_addr: "127.0.0.1:0".parse().unwrap(),
        api_port: 0,
        pow_bits: 1,
        mdns_enabled: false,
        ..NodeConfig::default()
    };
    run_with_maintenance(unlocked, config, fast_maintenance())
        .await
        .unwrap()
}

#[tokio::test]
async fn le_port_udp_est_libere_a_l_arret() {
    let dir = tempfile::tempdir().unwrap();
    let node = boot(dir.path()).await;
    let port = node.p2p_addr().port();
    assert_ne!(port, 0, "port éphémère non résolu");

    node.shutdown();
    drop(node);

    // Bind WITHOUT reuse: it fails while any leaked loop still holds the socket
    // and succeeds once the runtime is truly gone. The loops exit within a few
    // hundred milliseconds of shutdown; a leak would keep the port bound
    // forever, so this poll would time out.
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
    let mut freed = false;
    for _ in 0..100 {
        if std::net::UdpSocket::bind(addr).is_ok() {
            freed = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(
        freed,
        "le port UDP {port} n'a pas été libéré après l'arrêt : une boucle du runtime a fui"
    );
}
