//! Transfert de fichier entre deux amis DERRIÈRE NAT SYMÉTRIQUE (SPEC §10,
//! §11.3) : reproduction du signalement « les images de MP entre amis
//! n'arrivent jamais alors que les messages passent ».
//!
//! A et B sont chacun derrière un NAT symétrique simulé (aucun entrant non
//! sollicité) ; R est le seul nœud public (relais domicile par dérivation).
//! L'amitié et les DM passent (couvert par `nat_first_contact_e2e`) — ici on
//! vérifie que la PIÈCE JOINTE (image) traverse AUSSI : le canal FILE doit
//! emprunter le circuit relais quand aucune session directe n'existe.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use accord_core::db::ContactState;
use accord_node::{identity, run_with_socket, MaintenanceConfig, NodeConfig, Paths, RunningNode};
use accord_transport::socket::sim::{NetConditions, SimNet};

const PASSPHRASE: &str = "phrase-de-passe-test";

/// Intervalles raccourcis (mêmes ordres de grandeur que `maintenance_e2e`).
fn fast_maintenance() -> MaintenanceConfig {
    MaintenanceConfig {
        dht_republish: Duration::from_secs(3600),
        enabled: true,
        identity_republish: Duration::from_millis(500),
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

async fn eventually(secs: u64, mut cond: impl FnMut() -> bool) -> bool {
    for _ in 0..(secs * 5) {
        if cond() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    cond()
}

#[tokio::test]
async fn image_de_mp_traverse_le_nat_symetrique_via_le_relais() {
    let net = SimNet::new(1, NetConditions::default());
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

    a.add_bootstrap_peer(r_addr).await.unwrap();
    b.add_bootstrap_peer(r_addr).await.unwrap();

    let a_pub = a.node.public_key();
    let b_pub = b.node.public_key();

    a.node.friend_request(&b_pub, "Alice").unwrap();
    assert!(
        eventually(120, || {
            b.node
                .contacts()
                .map(|cs| cs.iter().any(|c| c.pubkey == a_pub))
                .unwrap_or(false)
        })
        .await,
        "demande d'ami jamais reçue (pré-requis du scénario)"
    );
    b.node.friend_respond(&a_pub, true).unwrap();
    assert!(
        eventually(60, || {
            a.node
                .contacts()
                .map(|cs| {
                    cs.iter()
                        .any(|c| c.pubkey == b_pub && c.state == ContactState::Friend)
                })
                .unwrap_or(false)
        })
        .await,
        "amitié jamais confirmée (pré-requis du scénario)"
    );

    // A envoie une « image » de 600 Kio (3 blocs) en pièce jointe de MP.
    let octets: Vec<u8> = (0..600 * 1024u32).map(|i| (i % 251) as u8).collect();
    let piece = a
        .node
        .files_publish_bytes("photo.png", "image/png", octets)
        .unwrap();
    a.node
        .dm_send_with_attachments(&b_pub, "", None, vec![piece.clone()])
        .unwrap();
    assert!(
        eventually(60, || {
            b.node
                .dm_history(&a_pub, u64::MAX, 10)
                .map(|h| h.iter().any(|m| m.author == a_pub))
                .unwrap_or(false)
        })
        .await,
        "le message porteur de la pièce jointe n'est pas arrivé"
    );

    // B demande la vignette (même chemin que `files.read` media de l'UI).
    b.node
        .files_fetch_media(&piece.merkle_root, Some(a_pub))
        .unwrap();
    assert!(
        eventually(120, || {
            b.node
                .files_local_path(&piece.merkle_root)
                .map(|p| p.is_some())
                .unwrap_or(false)
        })
        .await,
        "l'image n'a jamais été téléchargée : le canal FILE ne traverse pas \
         le lien relayé alors que les DM passent"
    );

    r.shutdown();
    a.shutdown();
    b.shutdown();
}
