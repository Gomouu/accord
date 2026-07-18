//! Scénario isolé dans son propre binaire : plusieurs paires de nœuds dans
//! un même processus partagent le multicast mDNS et se polluent (artefact de
//! test, impossible en production — un nœud par machine).

//! Reproduction du problème « images de profil entre amis » (D-052) :
//! l'avatar et la bannière ne portent que des hashes dans l'annonce
//! `Profile` — les octets doivent être récupérés par le pair via le
//! sous-système fichiers. Ces tests vérifient la chaîne complète sur le
//! réseau UDP réel, dans la topologie du terrain (pas de DHT, adresses
//! enregistrées à la main, sessions qui vont et viennent) :
//!
//! 1. amis en ligne : avatar ET bannière posés après coup arrivent ;
//! 2. bannière posée pendant que l'ami est ÉTEINT : l'annonce attend dans
//!    l'outbox, l'ami redémarre, et les octets doivent finir chez lui.

use std::time::Duration;

use accord_core::db::ContactState;
use accord_node::{
    identity, run_with_maintenance, MaintenanceConfig, NodeConfig, Paths, RunningNode,
};

const PASSPHRASE: &str = "phrase-de-passe-test";

/// Cadences accélérées : outbox, ré-annonce de profil et boucles fichiers
/// tournent en centaines de millisecondes (bornes de test, pas de réseau
/// externe).
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

/// Démarre (ou redémarre : l'identité existante est déverrouillée) un nœud
/// complet sur un répertoire de profil, avec la maintenance fournie.
async fn boot_avec(dir: &std::path::Path, maintenance: MaintenanceConfig) -> RunningNode {
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
        ..NodeConfig::default()
    };
    run_with_maintenance(unlocked, config, maintenance)
        .await
        .unwrap()
}

/// Démarre un nœud avec les cadences accélérées par défaut.
async fn boot(dir: &std::path::Path) -> RunningNode {
    boot_avec(dir, fast_maintenance()).await
}

/// Réévalue `cond` jusqu'à ~20 s (pas de 100 ms — les reprises fichiers ont
/// leur propre backoff).
async fn attendre(mut cond: impl FnMut() -> bool) -> bool {
    for _ in 0..200 {
        if cond() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    false
}

/// Noue l'amitié Alice ↔ Bob (adresses enregistrées des deux côtés).
async fn lier_amis(alice: &RunningNode, bob: &RunningNode) {
    let alice_pub = alice.node.public_key();
    let bob_pub = bob.node.public_key();
    alice.register_peer(bob_pub, bob.p2p_addr());
    bob.register_peer(alice_pub, alice.p2p_addr());
    alice.node.friend_request(&bob_pub, "Alice").unwrap();
    assert!(
        attendre(|| {
            bob.node
                .contacts()
                .map(|cs| cs.iter().any(|c| c.pubkey == alice_pub))
                .unwrap_or(false)
        })
        .await,
        "demande d'ami non reçue"
    );
    bob.node.friend_respond(&alice_pub, true).unwrap();
    assert!(
        attendre(|| {
            alice
                .node
                .contacts()
                .map(|cs| {
                    cs.iter()
                        .any(|c| c.pubkey == bob_pub && c.state == ContactState::Friend)
                })
                .unwrap_or(false)
        })
        .await,
        "amitié non confirmée"
    );
}

/// Octets d'image factices, assez gros pour plusieurs blocs (~600 Kio).
fn image(seed: u8) -> Vec<u8> {
    (0..600 * 1024u32)
        .map(|i| ((i % 251) as u8).wrapping_add(seed))
        .collect()
}

#[tokio::test]
async fn banniere_posee_pendant_que_l_ami_est_eteint_le_rattrape_au_redemarrage() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let alice = boot(dir_a.path()).await;
    let bob = boot(dir_b.path()).await;
    let alice_pub = alice.node.public_key();
    lier_amis(&alice, &bob).await;

    // Bob s'éteint. Alice pose sa bannière pendant ce temps : l'annonce
    // `Profile` part dans l'outbox (aucune DHT ici — comme sur le terrain où
    // la boîte hors-ligne est inaccessible).
    bob.shutdown();
    alice
        .node
        .profile_update(Some("Alice"), None, None, None, None, None, None, None)
        .unwrap();
    let banner = alice
        .node
        .profile_set_banner(Some(("image/png", image(7))))
        .unwrap()
        .expect("hash de bannière");
    tokio::time::sleep(Duration::from_millis(600)).await;

    // Bob redémarre (même profil, nouvelle adresse) ; les adresses sont
    // ré-échangées comme le font les pairs enregistrés à la main.
    let bob = boot(dir_b.path()).await;
    let bob_pub = bob.node.public_key();
    alice.register_peer(bob_pub, bob.p2p_addr());
    bob.register_peer(alice_pub, alice.p2p_addr());

    // L'annonce en attente doit être livrée, et les octets récupérés.
    assert!(
        attendre(|| matches!(bob.node.files_local_path(&banner), Ok(Some(_)))).await,
        "bannière posée hors-ligne jamais rattrapée après redémarrage"
    );

    alice.shutdown();
    bob.shutdown();
}
