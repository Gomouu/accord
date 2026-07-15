//! Flux d'invitation COMPLET entre deux amis tous deux derrière un NAT
//! symétrique, via un nœud d'amorçage/relais PAR DÉFAUT (rendez-vous partagé
//! livré avec l'app) — sans aucune ouverture de port ni amorçage manuel entre
//! les deux amis.
//!
//! C'est le scénario « Code ami introuvable sur le réseau » : deux pairs qui ne
//! s'amorcent que l'un sur l'autre, tous deux NATés, n'ont AUCUN rendez-vous
//! joignable commun — aucun des deux n'est joignable, donc la résolution du code
//! ami (FIND_VALUE DHT) échoue (reproduit : `dht_nodes = 0`). Le correctif est
//! un nœud d'amorçage par défaut (`NodeConfig::default_bootstrap`) que les deux
//! nœuds rejoignent automatiquement, exactement comme les bootstrap nodes
//! d'IPFS/BitTorrent (pas un serveur central : il ne voit que du chiffré).
//!
//! Le test exerce le VRAI premier pas — résolution du CODE AMI par la DHT (pas
//! de clé publique pré-partagée, contrairement à `nat_first_contact_e2e`) —
//! puis demande d'ami, acceptation et DM aller-retour.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use accord_core::db::ContactState;
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

/// Démarre un nœud sur le mesh simulé avec une liste de nœuds d'amorçage PAR
/// DÉFAUT (aucun amorçage manuel entre amis).
async fn boot_sim(
    dir: &std::path::Path,
    net: &SimNet,
    addr: SocketAddr,
    default_bootstrap: Vec<SocketAddr>,
) -> RunningNode {
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
        default_bootstrap,
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
async fn invitation_complete_via_amorcage_par_defaut_deux_nat_symetriques() {
    let net = SimNet::new(3, NetConditions::default());
    let r_addr: SocketAddr = "127.1.0.1:4000".parse().unwrap();
    let a_addr: SocketAddr = "10.0.0.2:4001".parse().unwrap();
    let b_addr: SocketAddr = "10.0.1.2:4002".parse().unwrap();
    // A et B tous deux derrière un NAT symétrique (deux box FAI) : aucun n'est
    // joignable de l'extérieur, aucun entrant non sollicité.
    net.set_symmetric_nat(a_addr, "127.8.0.1".parse().unwrap());
    net.set_symmetric_nat(b_addr, "127.9.0.1".parse().unwrap());

    let dir_r = tempfile::tempdir().unwrap();
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    // R : nœud public, sans amorçage. A et B : R comme amorçage PAR DÉFAUT,
    // AUCUN amorçage manuel de A vers B ni de B vers A.
    let r = boot_sim(dir_r.path(), &net, r_addr, vec![]).await;
    let a = boot_sim(dir_a.path(), &net, a_addr, vec![r_addr]).await;
    let b = boot_sim(dir_b.path(), &net, b_addr, vec![r_addr]).await;

    let a_pub = a.node.public_key();
    let b_pub = b.node.public_key();
    // A ne connaît QUE le code ami de B (chaîne partagée hors bande).
    let b_code = FriendCode::of_pubkey(&b_pub);

    // 1. Résolution du code ami (le pas qui affichait « introuvable »).
    let resolved = resolve_eventually(&a, &b_code, 60).await;
    assert_eq!(
        resolved,
        Some(b_pub),
        "A n'a pas résolu le code ami de B via l'amorçage par défaut \
         (« Code ami introuvable sur le réseau ») — deux NAT symétriques"
    );

    // 2. Demande d'ami A -> B (livrée par le rendez-vous relais).
    a.node.friend_request(&b_pub, "Alice").unwrap();
    assert!(
        eventually(90, || {
            b.node
                .contacts()
                .map(|cs| cs.iter().any(|c| c.pubkey == a_pub))
                .unwrap_or(false)
        })
        .await,
        "B n'a pas reçu la demande d'ami"
    );

    // 3. B accepte -> confirmation chez A.
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
        "l'amitié n'a pas été confirmée chez A"
    );

    // 4. Messagerie aller-retour.
    a.node
        .dm_send(&b_pub, "salut sans ouvrir de port", None)
        .unwrap();
    assert!(
        eventually(60, || {
            b.node
                .dm_history(&a_pub, u64::MAX, 10)
                .map(|h| h.iter().any(|m| m.author == a_pub))
                .unwrap_or(false)
        })
        .await,
        "B n'a pas reçu le DM de A"
    );

    a.shutdown();
    b.shutdown();
    r.shutdown();
}
