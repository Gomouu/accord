//! Tests d'intégration des boucles de maintenance réseau (D-024), sur le même
//! harnais que `two_node_e2e` : nœuds complets, UDP réel, DHT, intervalles
//! raccourcis. Deux scénarios de reconnexion après redémarrage :
//!
//! 1. présence publiée puis résolue via la DHT, et outbox vidée : un nœud
//!    redémarré (carnet d'adresses en mémoire perdu) met un DM en file
//!    hors-ligne, retrouve l'adresse de son ami par le record de présence et
//!    livre le message — l'accusé applicatif solde la file ;
//! 2. offres `GroupSync` : un membre éteint pendant une opération de groupe
//!    converge à son retour grâce à l'offre périodique (pull anti-entropie).
//!
//! Limites assumées : horloge réelle (pas d'horloge simulée), assertions par
//! attente bornée ; le backoff d'outbox (base 5 s, constante de la base) peut
//! retarder une passe, la borne d'attente en tient compte. Le dépôt et la
//! relève des boîtes aux lettres DHT ne sont pas rejoués ici (chemin couvert
//! unitairement dans `accord-core::offline` et `maintenance`).

use std::time::Duration;

use accord_core::db::ContactState;
use accord_node::{
    identity, run_with_maintenance, MaintenanceConfig, NodeConfig, Paths, RunningNode,
};

const PASSPHRASE: &str = "phrase-de-passe-test";

/// Intervalles raccourcis pour observer plusieurs passes en quelques secondes.
fn fast_maintenance() -> MaintenanceConfig {
    MaintenanceConfig {
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

/// Démarre (ou redémarre : l'identité est restaurée du coffre) un nœud dans
/// `dir` avec la maintenance accélérée.
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
        ..NodeConfig::default()
    };
    run_with_maintenance(unlocked, config, fast_maintenance())
        .await
        .unwrap()
}

/// Attend qu'une condition devienne vraie (borne dure ~20 s : le backoff
/// d'outbox de 5 s et les intervalles jitterés peuvent différer une passe).
async fn eventually(mut cond: impl FnMut() -> bool) -> bool {
    for _ in 0..200 {
        if cond() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    cond()
}

/// Établit l'amitié entre deux nœuds démarrés (adresses amorcées à la main).
async fn befriend(alice: &RunningNode, bob: &RunningNode) {
    let alice_pub = alice.node.public_key();
    let bob_pub = bob.node.public_key();
    alice.register_peer(bob_pub, bob.p2p_addr());
    bob.register_peer(alice_pub, alice.p2p_addr());

    alice.node.friend_request(&bob_pub, "Alice").unwrap();
    assert!(
        eventually(|| {
            bob.node
                .contacts()
                .map(|cs| cs.iter().any(|c| c.pubkey == alice_pub))
                .unwrap_or(false)
        })
        .await,
        "Bob n'a pas reçu la demande d'ami"
    );
    bob.node.friend_respond(&alice_pub, true).unwrap();
    assert!(
        eventually(|| {
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
        "l'amitié n'a pas été confirmée chez Alice"
    );
}

/// Présence publiée puis résolue, et outbox vidée après reconnexion :
/// Alice redémarre (carnet d'adresses perdu), envoie un DM sans adresse
/// connue (mis en file hors-ligne), retrouve Bob par son record de présence
/// DHT et livre le message ; l'accusé de Bob solde la file.
#[tokio::test]
async fn presence_resolue_et_outbox_videe_apres_redemarrage() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let alice = boot(dir_a.path()).await;
    let bob = boot(dir_b.path()).await;
    let bob_pub = bob.node.public_key();

    befriend(&alice, &bob).await;

    // Alice s'éteint : son carnet d'adresses (en mémoire) est perdu.
    let alice_pub = alice.node.public_key();
    alice.shutdown();
    drop(alice);

    // Alice redémarre sur un nouveau port, table DHT amorcée sur Bob.
    let alice = boot(dir_a.path()).await;
    assert_eq!(
        alice.node.public_key(),
        alice_pub,
        "l'identité doit être restaurée du coffre"
    );
    alice.dht_bootstrap(vec![bob.node_info()]).await;

    // Sans adresse connue pour Bob, le DM part en file hors-ligne ; la
    // résolution de présence puis le vidage d'outbox doivent le livrer.
    alice
        .node
        .dm_send(&bob_pub, "envoyé pendant que je te cherchais", None)
        .unwrap();
    assert!(
        eventually(|| {
            bob.node
                .dm_history(&alice_pub, u64::MAX, 10)
                .map(|h| h.iter().any(|m| m.author == alice_pub))
                .unwrap_or(false)
        })
        .await,
        "Bob n'a pas reçu le DM (présence non résolue ou outbox non vidée)"
    );

    // L'accusé applicatif de Bob solde l'élément d'outbox chez Alice.
    assert!(
        eventually(|| {
            alice
                .node
                .outbox_for(&bob_pub)
                .map(|items| items.is_empty())
                .unwrap_or(false)
        })
        .await,
        "l'outbox d'Alice n'a pas été soldée par l'accusé de Bob"
    );

    alice.shutdown();
    bob.shutdown();
}

/// Convergence par offres `GroupSync` : Bob rate une opération de groupe
/// (éteint), redémarre sur un nouveau port, et l'offre périodique d'Alice —
/// adressée grâce à la présence republiée par Bob — déclenche le pull
/// anti-entropie qui rejoue l'opération manquée.
#[tokio::test]
async fn group_sync_converge_apres_redemarrage() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let alice = boot(dir_a.path()).await;
    let bob = boot(dir_b.path()).await;
    let alice_pub = alice.node.public_key();
    let bob_pub = bob.node.public_key();

    befriend(&alice, &bob).await;

    // Groupe avec un salon ; Bob matérialise l'état complet.
    let gid_hex = alice.node.group_create("Guilde").unwrap();
    let gid: [u8; 16] = accord_node::hex::decode(&gid_hex).unwrap();
    alice.node.group_add_channel(&gid, "général").unwrap();
    let invite_id_hex = alice.node.group_invite_create(&gid, &bob_pub).unwrap();
    let invite_id: [u8; 16] = accord_node::hex::decode(&invite_id_hex).unwrap();
    assert!(
        eventually(|| {
            bob.node
                .group_invites_list()
                .map(|invites| invites.iter().any(|i| i.invite_id == invite_id))
                .unwrap_or(false)
        })
        .await,
        "Bob n'a pas reçu le ticket d'invitation"
    );
    bob.node.group_invite_accept(&gid, &invite_id).unwrap();
    assert!(
        eventually(|| {
            bob.node
                .group_state(&gid)
                .map(|s| s.is_member(&bob_pub) && s.channels.len() == 1)
                .unwrap_or(false)
        })
        .await,
        "Bob n'a pas matérialisé l'état initial du groupe"
    );

    // Bob s'éteint ; Alice ajoute un salon — l'op part vers une adresse
    // morte et se perd (le transport l'accepte en file de handshake).
    bob.shutdown();
    drop(bob);
    alice.node.group_add_channel(&gid, "annonces").unwrap();

    // Bob redémarre sur un nouveau port et republie sa présence ; Alice la
    // résout, adresse son offre GroupSync, et Bob tire l'op manquée.
    let bob = boot(dir_b.path()).await;
    assert_eq!(bob.node.public_key(), bob_pub, "identité de Bob restaurée");
    bob.dht_bootstrap(vec![alice.node_info()]).await;
    assert!(
        eventually(|| {
            bob.node
                .group_state(&gid)
                .map(|s| s.channels.len() == 2)
                .unwrap_or(false)
        })
        .await,
        "Bob n'a pas convergé via GroupSync après son redémarrage"
    );
    // Sanité : Alice voit toujours Bob membre et 2 salons.
    let state = alice.node.group_state(&gid).unwrap();
    assert!(state.is_member(&alice_pub) && state.channels.len() == 2);

    alice.shutdown();
    bob.shutdown();
}
