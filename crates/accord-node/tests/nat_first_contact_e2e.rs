//! Premier contact entrant SANS ouverture de port (SPEC §11.3, voir
//! `docs/NAT-FIRST-CONTACT.md`) : A et B sont chacun derrière un NAT
//! SYMÉTRIQUE simulé (mapping par destination, filtrage entrant strict,
//! aucun entrant non sollicité) — le pire cas des box FAI. R est le seul
//! nœud public (pair ordinaire, pas de serveur : il devient relais domicile
//! par dérivation déterministe).
//!
//! Scénario : A ne connaît que la CLÉ PUBLIQUE de B (et l'adresse d'amorçage
//! de R). Ni A ni B n'ont jamais communiqué, ni ouvert de port, ni de mapping
//! UPnP. La demande d'ami doit atteindre B via le rendez-vous relais
//! domicile ; l'amitié se confirme dans les deux sens ; un DM aller-retour
//! prouve la messagerie sur le lien (relayé).
//!
//! Ce test ÉCHOUAIT avant le correctif (diagnostic D1-D4 du doc : tables de
//! routage jamais peuplées en conditions réelles, repli relais conditionné à
//! la présence, outbox aveugle au circuit) et passe après.

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

/// Démarre un nœud complet sur le mesh simulé, à l'adresse donnée.
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

/// Attend qu'une condition devienne vraie (borne dure `secs` secondes : le
/// repli relais n'est tenté que `PUNCH_FALLBACK_MS` après chaque passe de
/// résolution, la borne en tient compte).
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
async fn premier_contact_derriere_nat_symetrique_sans_port_ouvert() {
    let net = SimNet::new(1, NetConditions::default());
    let r_addr: SocketAddr = "127.1.0.1:4000".parse().unwrap();
    let a_addr: SocketAddr = "10.0.0.2:4001".parse().unwrap();
    let b_addr: SocketAddr = "10.0.1.2:4002".parse().unwrap();
    // Deux NAT symétriques DISTINCTS : mapping par destination, aucun entrant
    // non sollicité — l'adresse interne n'est jamais joignable de l'extérieur.
    net.set_symmetric_nat(a_addr, "127.8.0.1".parse().unwrap());
    net.set_symmetric_nat(b_addr, "127.9.0.1".parse().unwrap());

    let dir_r = tempfile::tempdir().unwrap();
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let r = boot_sim(dir_r.path(), &net, r_addr).await;
    let a = boot_sim(dir_a.path(), &net, a_addr).await;
    let b = boot_sim(dir_b.path(), &net, b_addr).await;

    // Amorçage par ADRESSE SEULE (le cas réel) : aucun NodeInfo hors-bande,
    // aucun `register_peer`. L'apprentissage DHT passe par NODE_ANNOUNCE.
    a.add_bootstrap_peer(r_addr).await.unwrap();
    b.add_bootstrap_peer(r_addr).await.unwrap();

    let a_pub = a.node.public_key();
    let b_pub = b.node.public_key();

    // Premier contact : A n'a que la clé publique de B. La demande part en
    // file hors-ligne et doit aboutir par le rendez-vous relais domicile (R).
    a.node.friend_request(&b_pub, "Alice").unwrap();

    assert!(
        eventually(120, || {
            b.node
                .contacts()
                .map(|cs| cs.iter().any(|c| c.pubkey == a_pub))
                .unwrap_or(false)
        })
        .await,
        "B n'a jamais reçu la demande d'ami : le premier contact entrant \
         derrière un NAT symétrique exige encore une ouverture de port"
    );

    // B accepte : la réponse doit revenir vers A par le même chemin (circuit).
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
        "l'amitié n'a pas été confirmée chez A (réponse perdue sur le circuit)"
    );

    // Messagerie sur le lien établi, dans les deux sens.
    a.node
        .dm_send(&b_pub, "premier message sans port ouvert", None)
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
    b.node
        .dm_send(&a_pub, "réponse à travers le relais", None)
        .unwrap();
    assert!(
        eventually(60, || {
            a.node
                .dm_history(&b_pub, u64::MAX, 10)
                .map(|h| h.iter().any(|m| m.author == b_pub))
                .unwrap_or(false)
        })
        .await,
        "A n'a pas reçu la réponse de B"
    );

    a.shutdown();
    b.shutdown();
    r.shutdown();
}
