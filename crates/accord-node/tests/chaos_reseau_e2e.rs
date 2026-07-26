//! Campagnes de chaos réseau (feuille de route §9.3) : la messagerie sous
//! perte, sous gigue, et à travers une coupure franche.
//!
//! **Pourquoi ce fichier existe.** Le mesh UDP simulé
//! ([`accord_transport::socket::sim`]) porte depuis longtemps une perte de
//! datagrammes, une latence variable et une mise hors service par nœud — et
//! chacun de ses appelants passait `NetConditions::default()`, c'est-à-dire
//! zéro perte et zéro latence. Les boutons existaient ; personne ne les
//! tournait. Tout ce que le projet savait de son comportement en réseau
//! dégradé, il le savait par déduction.
//!
//! Ce que chaque test éprouve, et qui ne se voit pas sur un réseau parfait :
//!
//! - **Perte** : la file hors-ligne et les reprises. Sur un lien qui perd un
//!   datagramme sur trois, un message ne passe qu'en étant réémis.
//! - **Gigue** : le simulateur tire un délai par datagramme et le livre depuis
//!   une tâche à part, donc rien ne garantit l'ordre d'arrivée. C'est le
//!   réordonnancement demandé par le §9.3, et il éprouve l'horloge de Lamport :
//!   l'ordre d'ARRIVÉE ne doit pas décider de l'ordre AFFICHÉ.
//!
//!   ⚠️ Ce que ce test ne prouve pas : qu'un réordonnancement a bien eu lieu
//!   dans une exécution donnée. Il met en place les conditions qui le
//!   permettent et vérifie que l'invariant tient ; il ne compte pas les
//!   inversions. Le prouver demanderait au simulateur de rapporter l'ordre de
//!   livraison, ce qu'il ne fait pas.
//! - **Coupure franche** : `set_down` fait disparaître les datagrammes sans
//!   RST, sans FIN, sans erreur — le pair d'en face ne l'apprend pas, il cesse
//!   simplement de recevoir. C'est ce que fait un Wi-Fi qui tombe, et c'est
//!   très différent d'un arrêt propre du nœud, seul cas couvert jusqu'ici par
//!   `reconnexion_e2e`.
//!
//! ⚠️ Les bornes d'attente sont larges à dessein. Un test de chaos qui échoue
//! une fois sur vingt ne mesure rien : il pollue la campagne
//! `./reconnexion-30.sh`, dont tout l'intérêt est qu'un échec y signifie
//! quelque chose. Les graines du simulateur sont fixes, pour que deux
//! exécutions voient la même séquence de pertes.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use accord_core::db::ContactState;
use accord_node::{identity, run_with_socket, MaintenanceConfig, NodeConfig, Paths, RunningNode};
use accord_proto::core_msg::MsgBody;
use accord_transport::socket::sim::{NetConditions, SimNet};

const PASSPHRASE: &str = "phrase-de-passe-test";

/// Intervalles raccourcis : sans cela, une reprise après perte se compte en
/// dizaines de secondes et le test dure plus que la campagne entière.
fn fast_maintenance() -> MaintenanceConfig {
    MaintenanceConfig {
        dht_republish: Duration::from_secs(3600),
        enabled: true,
        identity_republish: Duration::from_millis(500),
        presence_publish: Duration::from_millis(200),
        presence_resolve: Duration::from_millis(300),
        outbox_flush: Duration::from_millis(300),
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

/// Attend qu'une condition devienne vraie, au plus `secs` secondes.
async fn eventually(secs: u64, mut cond: impl FnMut() -> bool) -> bool {
    for _ in 0..(secs * 10) {
        if cond() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    false
}

/// Lie deux nœuds en amis. Fait AVANT d'allumer le chaos : ce fichier éprouve
/// la messagerie sous conditions dégradées, pas le premier contact — celui-ci
/// a ses propres tests (`nat_first_contact_e2e`).
async fn lier_amis(a: &RunningNode, b: &RunningNode) {
    let a_pub = a.node.public_key();
    let b_pub = b.node.public_key();
    a.learn_peer(b).unwrap();
    b.learn_peer(a).unwrap();
    a.node.friend_request(&b_pub, "Alice").unwrap();
    assert!(
        eventually(30, || b
            .node
            .contacts()
            .map(|cs| cs.iter().any(|c| c.pubkey == a_pub))
            .unwrap_or(false))
        .await,
        "demande d'ami non reçue avant le chaos"
    );
    b.node.friend_respond(&a_pub, true).unwrap();
    assert!(
        eventually(30, || a
            .node
            .contacts()
            .map(|cs| cs
                .iter()
                .any(|c| c.pubkey == b_pub && c.state == ContactState::Friend))
            .unwrap_or(false))
        .await,
        "amitié non confirmée avant le chaos"
    );
}

/// Textes des messages reçus par `qui` de la part de `de`, du plus ancien au
/// plus récent (`dm_history` rend l'inverse).
///
/// `DmRecord::body` n'est PAS le texte : c'est le corps encodé, discriminé par
/// `kind`. Le lire comme une chaîne rend `"\0\u{6}coucou\0\0\0"` — une première
/// version de ce fichier l'a fait, et les trois tests ont échoué en accusant le
/// réseau alors que les messages arrivaient en quelques millisecondes.
/// On décode donc comme le fait le service, et on ne garde que les messages
/// texte : sous chaos, la conversation porte aussi des accusés de lecture.
fn textes_recus(qui: &RunningNode, de: &[u8; 32]) -> Vec<String> {
    let mut h = qui.node.dm_history(de, u64::MAX, 100).unwrap();
    h.reverse();
    h.into_iter()
        .filter_map(|m| match MsgBody::decode_body(m.kind, &m.body) {
            Ok(MsgBody::Text { text, .. }) => Some(text),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn un_message_traverse_un_lien_qui_perd_un_datagramme_sur_trois() {
    let net = SimNet::new(20_260_726, NetConditions::default());
    let a_addr: SocketAddr = "127.20.0.1:5001".parse().unwrap();
    let b_addr: SocketAddr = "127.20.0.2:5002".parse().unwrap();

    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let a = boot_sim(dir_a.path(), &net, a_addr).await;
    let b = boot_sim(dir_b.path(), &net, b_addr).await;
    lier_amis(&a, &b).await;

    // Le chaos ne s'allume qu'ici : l'amitié est établie sur un lien sain,
    // seule la messagerie est mise à l'épreuve.
    let perte = NetConditions {
        loss: 0.33,
        latency_min_ms: 0,
        latency_max_ms: 0,
    };
    net.set_conditions(a_addr, perte);
    net.set_conditions(b_addr, perte);

    let b_pub = b.node.public_key();
    let a_pub = a.node.public_key();
    a.node
        .dm_send(&b_pub, "un message sous la pluie", None)
        .unwrap();

    assert!(
        eventually(60, || textes_recus(&b, &a_pub)
            .iter()
            .any(|c| c == "un message sous la pluie"))
        .await,
        "un message n'a pas traversé un lien perdant un datagramme sur trois : \
         les reprises ne rattrapent pas la perte"
    );
}

#[tokio::test]
async fn des_messages_reordonnes_par_le_reseau_restent_dans_l_ordre_a_l_ecran() {
    // 🔒 Le cœur du test n'est pas que les messages arrivent — c'est qu'ils
    // s'affichent dans l'ordre où ils ont été ÉCRITS, quel que soit l'ordre où
    // ils sont ARRIVÉS. Le simulateur tire un délai par datagramme entre 5 et
    // 120 ms et le livre depuis une tâche à part : une salve émise en séquence
    // arrive nécessairement mêlée. Sans horloge de Lamport, l'historique
    // afficherait l'ordre d'arrivée, et une conversation deviendrait
    // incompréhensible sur un réseau lent.
    let net = SimNet::new(20_260_727, NetConditions::default());
    let a_addr: SocketAddr = "127.21.0.1:5001".parse().unwrap();
    let b_addr: SocketAddr = "127.21.0.2:5002".parse().unwrap();

    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let a = boot_sim(dir_a.path(), &net, a_addr).await;
    let b = boot_sim(dir_b.path(), &net, b_addr).await;
    lier_amis(&a, &b).await;

    let gigue = NetConditions {
        loss: 0.0,
        latency_min_ms: 5,
        latency_max_ms: 120,
    };
    net.set_conditions(a_addr, gigue);
    net.set_conditions(b_addr, gigue);

    let b_pub = b.node.public_key();
    let a_pub = a.node.public_key();
    let attendus: Vec<String> = (0..8).map(|n| format!("message {n}")).collect();
    for texte in &attendus {
        a.node.dm_send(&b_pub, texte, None).unwrap();
    }

    assert!(
        eventually(60, || textes_recus(&b, &a_pub).len() >= attendus.len()).await,
        "les huit messages ne sont pas tous arrivés sous gigue"
    );
    assert_eq!(
        textes_recus(&b, &a_pub),
        attendus,
        "les messages sont arrivés dans le désordre ET s'affichent dans le \
         désordre : l'ordre d'arrivée ne doit jamais décider de l'ordre affiché"
    );
}

#[tokio::test]
async fn une_coupure_franche_ne_perd_pas_les_messages_ecrits_pendant() {
    // Une coupure FRANCHE, pas un arrêt propre : les datagrammes disparaissent
    // sans RST, sans FIN, sans erreur. Le pair d'en face ne l'apprend pas, il
    // cesse de recevoir — c'est un Wi-Fi qui tombe. `reconnexion_e2e` couvre
    // l'arrêt propre du nœud, qui laisse au moins l'occasion de fermer.
    let net = SimNet::new(20_260_728, NetConditions::default());
    let a_addr: SocketAddr = "127.22.0.1:5001".parse().unwrap();
    let b_addr: SocketAddr = "127.22.0.2:5002".parse().unwrap();

    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let a = boot_sim(dir_a.path(), &net, a_addr).await;
    let b = boot_sim(dir_b.path(), &net, b_addr).await;
    lier_amis(&a, &b).await;

    let b_pub = b.node.public_key();
    let a_pub = a.node.public_key();

    // Un premier message passe : le lien fonctionne, et l'échec éventuel qui
    // suivra ne pourra pas être imputé à une liaison jamais établie.
    a.node.dm_send(&b_pub, "avant la coupure", None).unwrap();
    assert!(
        eventually(30, || textes_recus(&b, &a_pub)
            .iter()
            .any(|c| c == "avant la coupure"))
        .await,
        "le lien ne marchait pas avant même la coupure"
    );

    // Coupure. B disparaît du réseau sans prévenir personne.
    net.set_down(b_addr, true);
    a.node.dm_send(&b_pub, "pendant la coupure", None).unwrap();

    // Le message ne doit surtout pas être perdu côté A : il part en file.
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(
        !textes_recus(&b, &a_pub)
            .iter()
            .any(|c| c == "pendant la coupure"),
        "B a reçu un message alors que son réseau était coupé : la coupure \
         simulée ne coupe rien, et le test ne prouverait rien"
    );

    // Retour du réseau.
    net.set_down(b_addr, false);
    assert!(
        eventually(90, || textes_recus(&b, &a_pub)
            .iter()
            .any(|c| c == "pendant la coupure"))
        .await,
        "le message écrit pendant la coupure n'est jamais arrivé après le \
         retour du réseau : la file hors-ligne ne rattrape pas une coupure \
         franche"
    );
}
