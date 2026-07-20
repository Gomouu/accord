//! Tests d'intégration de la mise en réseau réelle (B2) : port P2P stable
//! réutilisé entre deux lancements, et amorçage de bout en bout — un second
//! nœud démarré avec le premier en pair d'amorçage résout son code ami par la
//! DHT (routage initialement vide), noue l'amitié puis échange un DM sur UDP
//! réel.

use std::time::Duration;

use accord_crypto::FriendCode;
use accord_node::{
    identity, run_with_maintenance, MaintenanceConfig, NodeConfig, Paths, RunningNode,
};

/// Démarre un nœud sur `dir` (création d'identité) avec une republication
/// d'identité rapide (pour que le record d'identité soit stocké localement
/// quasi immédiatement, condition de la résolution par un pair d'amorçage).
async fn boot(dir: &std::path::Path) -> RunningNode {
    boot_port(dir, 0).await
}

/// Comme [`boot`] mais avec un port P2P explicite (`0` : stratégie stable).
async fn boot_port(dir: &std::path::Path, port: u16) -> RunningNode {
    let paths = Paths::new(dir);
    let unlocked = identity::create(&paths, "phrase-de-passe-test", 1).unwrap();
    demarrer(paths, unlocked, port).await
}

/// Assemble la config commune et démarre le nœud.
async fn demarrer(paths: Paths, unlocked: identity::Unlocked, port: u16) -> RunningNode {
    let config = NodeConfig {
        paths,
        p2p_addr: format!("127.0.0.1:{port}").parse().unwrap(),
        api_port: 0,
        pow_bits: 1,
        ..NodeConfig::default()
    };
    let maintenance = MaintenanceConfig {
        identity_republish: Duration::from_millis(150),
        ..MaintenanceConfig::default()
    };
    run_with_maintenance(unlocked, config, maintenance)
        .await
        .unwrap()
}

/// Attend qu'une condition devienne vraie (interrogation courte, borne dure).
async fn eventually(mut cond: impl FnMut() -> bool) -> bool {
    for _ in 0..120 {
        if cond() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    cond()
}

#[tokio::test]
async fn port_p2p_stable_persiste_pour_reutilisation() {
    let dir = tempfile::tempdir().unwrap();

    // Lancement sur un port explicite peu banal (évite la contention de la
    // plage par défaut avec les autres tests parallèles).
    let port1 = 47137;
    let first = boot_port(dir.path(), port1).await;
    assert_eq!(first.p2p_addr().port(), port1, "port explicite lié");
    assert_eq!(first.network_status().p2p_port, port1);

    // Le port lié est persisté dans la base (meta `network.port`) : il sera
    // préféré au prochain lancement (préférence vérifiée par le test unitaire
    // `candidate_ports`). On le relit via une connexion indépendante.
    let paths = Paths::new(dir.path());
    let unlocked = identity::unlock(&paths, "phrase-de-passe-test").unwrap();
    let db = accord_core::db::Db::open(&paths.db(), &unlocked.db_key).unwrap();
    let stored = db.meta("network.port").unwrap().expect("port P2P persisté");
    let bytes: [u8; 2] = stored.as_slice().try_into().expect("2 octets");
    assert_eq!(
        u16::from_be_bytes(bytes),
        port1,
        "le port lié doit être persisté pour réutilisation"
    );

    first.shutdown();
}

#[tokio::test]
async fn statut_reseau_expose_les_champs_additifs_sans_mapping() {
    // Nœud en écoute loopback : le mapping de port et la découverte mDNS sont
    // ignorés (rien à exposer ni à annoncer). Le statut expose alors le repli
    // honnête : pas d'adresse externe, méthode "aucun", zéro pair LAN — sans
    // casser les champs historiques.
    let dir = tempfile::tempdir().unwrap();
    let node = boot(dir.path()).await;

    let status = node.network_status();
    assert!(
        status.external_addr.is_none(),
        "aucun mapping attendu en loopback"
    );
    assert_eq!(
        status.port_mapping.as_str(),
        "aucun",
        "méthode de mapping par défaut"
    );
    assert_eq!(status.lan_peers, 0, "aucun pair LAN découvert");
    // Champs historiques toujours présents.
    assert!(status.p2p_port != 0);

    node.shutdown();
}

#[tokio::test]
async fn amorcage_resout_code_ami_puis_amitie_et_dm() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let alice = boot(dir_a.path()).await;
    let bob = boot(dir_b.path()).await;

    let alice_pub = alice.node.public_key();
    let bob_pub = bob.node.public_key();

    // Code ami d'Alice (ce qu'elle communiquerait à Bob).
    let alice_code = FriendCode::parse(&alice.node.self_profile().unwrap().friend_code).unwrap();

    // Bob ne connaît qu'une adresse : celle d'Alice, ajoutée en pair
    // d'amorçage (persistée + connexion + ensemencement DHT immédiats).
    let status = bob.add_bootstrap_peer(alice.p2p_addr()).await.unwrap();
    assert!(
        status.bootstrap.contains(&alice.p2p_addr().to_string()),
        "l'adresse d'amorçage doit figurer dans le statut réseau"
    );

    // Bob résout le code ami d'Alice par la DHT (table de routage vide au
    // départ : le repli d'amorçage interroge directement Alice).
    let mut resolved = None;
    for _ in 0..120 {
        if let Ok(pk) = bob.resolve_friend_code(&alice_code).await {
            resolved = Some(pk);
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(
        resolved,
        Some(alice_pub),
        "Bob n'a pas résolu le code ami d'Alice par la DHT"
    );

    // Amitié : Bob demande Alice, la demande traverse le réseau.
    bob.node.friend_request(&alice_pub, "Bob").unwrap();
    let alice_saw = eventually(|| {
        alice
            .node
            .contacts()
            .map(|cs| cs.iter().any(|c| c.pubkey == bob_pub))
            .unwrap_or(false)
    })
    .await;
    assert!(alice_saw, "Alice n'a pas reçu la demande d'ami de Bob");

    // Alice accepte ; la réponse revient à Bob.
    alice.node.friend_respond(&bob_pub, true).unwrap();
    let bob_is_friends = eventually(|| {
        use accord_core::db::ContactState;
        bob.node
            .contacts()
            .map(|cs| {
                cs.iter()
                    .any(|c| c.pubkey == alice_pub && c.state == ContactState::Friend)
            })
            .unwrap_or(false)
    })
    .await;
    assert!(bob_is_friends, "l'amitié n'a pas été confirmée chez Bob");

    // DM aller : Bob → Alice.
    bob.node
        .dm_send(&alice_pub, "salut Alice, trouvée par la DHT", None)
        .unwrap();
    let alice_got = eventually(|| {
        alice
            .node
            .dm_history(&bob_pub, u64::MAX, 10)
            .map(|h| h.iter().any(|m| m.author == bob_pub))
            .unwrap_or(false)
    })
    .await;
    assert!(alice_got, "Alice n'a pas reçu le DM de Bob");

    // DM retour : Alice → Bob.
    alice
        .node
        .dm_send(&bob_pub, "bien reçue, Bob", None)
        .unwrap();
    let bob_got = eventually(|| {
        bob.node
            .dm_history(&alice_pub, u64::MAX, 10)
            .map(|h| h.iter().any(|m| m.author == alice_pub))
            .unwrap_or(false)
    })
    .await;
    assert!(bob_got, "Bob n'a pas reçu la réponse d'Alice");

    // Le statut réseau de Bob reflète au moins un pair connu.
    assert!(
        bob.network_status().connected_peers >= 1,
        "Bob devrait avoir au moins un pair dans son carnet"
    );

    alice.shutdown();
    bob.shutdown();
}

/// Lot D (D4/D35/D36) : diagnostic de connectivité de bout en bout sur UDP
/// réel — lien par pair (`network.peers` enrichi), compteurs locaux et
/// auto-test réseau avec sonde d'amorçage.
#[tokio::test]
async fn diagnostics_par_pair_compteurs_et_autotest() {
    use accord_node::LinkTransport;

    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let alice = boot(dir_a.path()).await;
    let bob = boot(dir_b.path()).await;

    let alice_pub = alice.node.public_key();
    let bob_pub = bob.node.public_key();

    // Amitié directe (adresses échangées à la main, comme two_node_e2e).
    bob.add_bootstrap_peer(alice.p2p_addr()).await.unwrap();
    alice.register_peer(bob_pub, bob.p2p_addr());
    bob.node.friend_request(&alice_pub, "Bob").unwrap();
    assert!(
        eventually(|| {
            alice
                .node
                .contacts()
                .map(|cs| cs.iter().any(|c| c.pubkey == bob_pub))
                .unwrap_or(false)
        })
        .await,
        "demande d'ami non reçue"
    );
    alice.node.friend_respond(&bob_pub, true).unwrap();
    assert!(
        eventually(|| {
            use accord_core::db::ContactState;
            bob.node
                .contacts()
                .map(|cs| {
                    cs.iter()
                        .any(|c| c.pubkey == alice_pub && c.state == ContactState::Friend)
                })
                .unwrap_or(false)
        })
        .await,
        "amitié non confirmée chez Bob"
    );
    bob.node.dm_send(&alice_pub, "diagnostic", None).unwrap();
    assert!(
        eventually(|| {
            alice
                .node
                .dm_history(&bob_pub, u64::MAX, 10)
                .map(|h| h.iter().any(|m| m.author == bob_pub))
                .unwrap_or(false)
        })
        .await,
        "DM non livré"
    );

    // Lien par pair : session DIRECTE vers Alice, trafic entrant frais, une
    // remise réussie horodatée. La latence keep-alive peut ne pas encore
    // avoir un cycle complet (première échéance à 25 s) : non exigée ici.
    assert!(
        eventually(|| {
            bob.peer_links().iter().any(|l| {
                l.pubkey == accord_node::hex::encode(&alice_pub)
                    && l.live
                    && l.transport == LinkTransport::Direct
                    && l.relay.is_none()
                    && l.last_recv_age_ms.is_some()
                    && l.last_delivery_ms.is_some()
            })
        })
        .await,
        "network.peers doit exposer un lien direct vivant vers Alice: {:?}",
        bob.peer_links()
    );

    // Auto-test de Bob : la sonde du pair d'amorçage (Alice) doit aboutir —
    // la session existe déjà, la sonde est un no-op positif.
    let rapport = bob.self_test().await;
    assert_eq!(rapport.p2p_port, bob.p2p_addr().port());
    assert!(
        rapport
            .bootstrap
            .iter()
            .any(|p| p.addr == alice.p2p_addr().to_string() && p.ok),
        "la sonde d'amorçage vers Alice doit réussir: {rapport:?}"
    );

    // Compteurs : photographie lisible ; rien d'exigé sur les valeurs (le
    // chemin direct n'a besoin ni de poinçonnage ni de relais ici).
    let compteurs = bob.diagnostics_counters();
    assert_eq!(compteurs.punch.requested + compteurs.punch.received, 0);

    alice.shutdown();
    bob.shutdown();
}
