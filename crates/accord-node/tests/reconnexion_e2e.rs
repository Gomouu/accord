//! Scénario isolé dans son propre binaire : une seule paire de nœuds, mais
//! Bob redémarre en cours de route — plusieurs incarnations partageraient le
//! multicast mDNS avec d'autres paires et se pollueraient (artefact de test).

//! Reconnexion via le CARNET D'ADRESSES PERSISTANT (A1) : après une première
//! session, chaque ami mémorise l'adresse directe de l'autre dans sa base.
//! Bob redémarre — SANS ré-enregistrer l'adresse d'Alice et SANS DHT (topologie
//! de test : aucun pair d'amorçage). Son seul moyen de retrouver Alice est le
//! cache. Un message qu'il lui envoie doit donc arriver : c'est la preuve
//! rouge/vert que la reconnexion au démarrage fonctionne (sans le cache, le
//! carnet mémoire est vide au redémarrage → le message ne peut pas être routé).

use std::time::Duration;

use accord_core::db::ContactState;
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

async fn eventually(mut cond: impl FnMut() -> bool) -> bool {
    for _ in 0..450 {
        if cond() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    false
}

async fn lier_amis(alice: &RunningNode, bob: &RunningNode) {
    let alice_pub = alice.node.public_key();
    let bob_pub = bob.node.public_key();
    alice.register_peer(bob_pub, bob.p2p_addr());
    bob.register_peer(alice_pub, alice.p2p_addr());
    alice.node.friend_request(&bob_pub, "Alice").unwrap();
    assert!(
        eventually(|| bob
            .node
            .contacts()
            .map(|cs| cs.iter().any(|c| c.pubkey == alice_pub))
            .unwrap_or(false))
        .await,
        "demande d'ami non reçue"
    );
    bob.node.friend_respond(&alice_pub, true).unwrap();
    assert!(
        eventually(|| alice
            .node
            .contacts()
            .map(|cs| cs
                .iter()
                .any(|c| c.pubkey == bob_pub && c.state == ContactState::Friend))
            .unwrap_or(false))
        .await,
        "amitié non confirmée"
    );
}

#[tokio::test]
async fn ami_se_reconnecte_via_le_cache_d_adresses_apres_redemarrage() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let alice = boot(dir_a.path()).await;
    let bob = boot(dir_b.path()).await;

    let alice_pub = alice.node.public_key();
    let bob_pub = bob.node.public_key();

    lier_amis(&alice, &bob).await;

    // Première session confirmée par un aller simple Alice → Bob : garantit que
    // Bob a mémorisé l'adresse d'Alice (persistée à l'événement `Connected`).
    alice.node.dm_send(&bob_pub, "avant redémarrage", None).unwrap();
    assert!(
        eventually(|| bob
            .node
            .dm_history(&alice_pub, u64::MAX, 10)
            .map(|h| h.iter().any(|m| m.author == alice_pub))
            .unwrap_or(false))
        .await,
        "Bob n'a pas reçu le premier message (session initiale absente)"
    );

    // Bob redémarre sur le même profil : identité déverrouillée, carnet
    // d'adresses relu depuis la base. Aucun ré-enregistrement manuel.
    bob.shutdown();
    drop(bob);
    let bob = boot(dir_b.path()).await;

    // Bob écrit à Alice SANS que son adresse ait été ré-injectée : seul le cache
    // persistant (chargé au démarrage) peut router ce message.
    bob.node.dm_send(&alice_pub, "de retour", None).unwrap();
    assert!(
        eventually(|| alice
            .node
            .dm_history(&bob_pub, u64::MAX, 10)
            .map(|h| h.iter().any(|m| m.author == bob_pub))
            .unwrap_or(false))
        .await,
        "reconnexion via le cache d'adresses échouée : Alice n'a pas reçu le message post-redémarrage"
    );

    alice.shutdown();
    bob.shutdown();
}
