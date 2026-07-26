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
//! - **Changement d'adresse en cours de session** : `SimNet::rebind` déplace un
//!   nœud vers une nouvelle adresse SANS rien détruire d'autre — il garde sa
//!   file de réception, donc ses sessions et son état. C'est le portable qui
//!   passe du Wi-Fi à la 4G, et c'est exactement ce qui le distingue d'un
//!   redémarrage : couper puis relier ailleurs, c'est un nœud neuf.
//!
//!   ⚠️ Ce que ce test ne prouve pas : que la session a SURVÉCU. Au niveau
//!   applicatif, un handshake neuf vers l'adresse apprise du message entrant
//!   mène au même état ; le détail est dans le test, et l'épinglage du
//!   mécanisme lui-même vit dans `accord-transport`
//!   (`reconnexion_transport_e2e`).
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

#[tokio::test]
async fn un_pair_qui_change_d_adresse_en_pleine_session_reste_joignable() {
    // Le portable qui passe du Wi-Fi à la 4G, ou le NAT qui refait son mapping :
    // l'adresse d'un pair change PENDANT qu'une session est vivante. Rien n'est
    // fermé, rien n'est renégocié — les clés, le numéro de session, l'horloge
    // restent ceux d'avant ; seule l'enveloppe UDP change de source. C'est le
    // contraire d'un redémarrage, et c'est pourquoi `set_down` ne l'exprimait
    // pas : couper puis relier ailleurs, c'est un nœud neuf.
    //
    // L'apprentissage est PASSIF : le pair d'en face ne découvre la nouvelle
    // adresse qu'en RECEVANT un datagramme qui en provient. Ce test suit donc
    // cet ordre — le nœud déplacé parle d'abord — puis vérifie le RETOUR, seul
    // sens qui engage quelque chose : un message ne repart correctement que si
    // la nouvelle adresse a traversé toute la pile jusqu'au chemin d'émission.
    // ⚠️ Ce n'est PAS la même chose que prouver que la session a survécu ; la
    // note en fin de fonction dit pourquoi, et où cette preuve-là vit.
    let net = SimNet::new(20_260_729, NetConditions::default());
    let a_addr: SocketAddr = "127.23.0.1:5001".parse().unwrap();
    let b_addr: SocketAddr = "127.23.0.2:5002".parse().unwrap();
    // Nouvelle adresse de A : IP **et** port changent, comme un basculement
    // Wi-Fi → 4G. Un simple changement de port ne prouverait pas grand-chose.
    let a_apres: SocketAddr = "127.23.9.1:6001".parse().unwrap();

    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let a = boot_sim(dir_a.path(), &net, a_addr).await;
    let b = boot_sim(dir_b.path(), &net, b_addr).await;
    lier_amis(&a, &b).await;

    let a_pub = a.node.public_key();
    let b_pub = b.node.public_key();

    // Les DEUX sens fonctionnent avant le déménagement : sans cela, un échec
    // plus bas ne dirait pas si la mobilité est en cause ou si le lien n'a
    // jamais porté ce sens-là.
    a.node
        .dm_send(&b_pub, "avant le déménagement", None)
        .unwrap();
    assert!(
        eventually(30, || textes_recus(&b, &a_pub)
            .iter()
            .any(|c| c == "avant le déménagement"))
        .await,
        "le sens A vers B ne marchait pas avant même le déménagement"
    );
    b.node
        .dm_send(&a_pub, "reçu, avant le déménagement", None)
        .unwrap();
    assert!(
        eventually(30, || textes_recus(&a, &b_pub)
            .iter()
            .any(|c| c == "reçu, avant le déménagement"))
        .await,
        "le sens B vers A ne marchait pas avant même le déménagement"
    );

    // Le déménagement. La session n'est ni fermée ni notifiée : B continue de
    // croire A à son ancienne adresse, et tout ce qu'il y enverra tombera dans
    // le vide jusqu'à ce qu'il apprenne le contraire.
    assert!(
        net.rebind(a_addr, a_apres),
        "le déplacement n'a pas eu lieu : le test qui suit ne prouverait rien"
    );
    // Garde anti-test-vide : le nœud lui-même doit voir sa nouvelle adresse.
    // Si le socket restait sur l'ancienne, tout ce qui suit passerait sans que
    // rien n'ait bougé.
    assert_eq!(
        a.p2p_addr(),
        a_apres,
        "le nœud déplacé n'a pas suivi son socket"
    );

    // Le nœud déplacé écrit : son datagramme part de la nouvelle adresse, et
    // c'est ce que le transport d'en face doit reconnaître comme « même
    // session, autre adresse » plutôt que comme un inconnu.
    a.node
        .dm_send(&b_pub, "après le déménagement", None)
        .unwrap();
    assert!(
        eventually(60, || textes_recus(&b, &a_pub)
            .iter()
            .any(|c| c == "après le déménagement"))
        .await,
        "un message émis après un changement d'adresse n'arrive pas : la \
         session ne survit pas à la mobilité de son pair"
    );

    // 🔒 Le cœur du test. B répond, donc B doit VISER la nouvelle adresse :
    // celle-ci ne lui a jamais été annoncée, il ne peut la tenir que de la
    // SOURCE du datagramme précédent. Tant que cette source ne remonte pas
    // jusqu'au chemin d'émission, le retour part vers une adresse que plus
    // personne n'écoute et n'arrive jamais.
    b.node
        .dm_send(&a_pub, "et le retour après le déménagement", None)
        .unwrap();
    assert!(
        eventually(60, || textes_recus(&a, &b_pub)
            .iter()
            .any(|c| c == "et le retour après le déménagement"))
        .await,
        "le retour ne suit pas le pair qui a déménagé : la nouvelle adresse \
         n'a pas été propagée jusqu'au chemin d'émission"
    );

    // ⚠️ Ce que ce test ne prouve PAS :
    //
    // - Que la SESSION a survécu au déplacement. C'est la limite la plus
    //   importante de ce test, et elle a été mesurée, pas devinée : deux
    //   mécanismes indépendants rendent le pair déplacé de nouveau joignable,
    //   et ils aboutissent au même état. (1) le transport réaiguille la session
    //   vivante (`Endpoint::on_data`) ; (2) le carnet d'adresses de la couche
    //   nœud apprend la nouvelle adresse par l'événement `Message` et la
    //   compose — un handshake neuf, dont `install_session` évince ensuite le
    //   cadavre. Neutraliser (1) laisse donc CE test vert. Ce qui épingle
    //   vraiment la mobilité, c'est
    //   `une_session_directe_suit_son_pair_qui_change_d_adresse`
    //   (`accord-transport/tests/reconnexion_transport_e2e.rs`) : là où il n'y
    //   a pas de carnet, il vérifie qu'aucune seconde session n'a été négociée.
    //   Ce que ce test-ci vaut, c'est la chaîne complète — base, file
    //   hors-ligne, routage, transport — sur un pair qui a changé d'adresse.
    // - Qu'un pair déplacé reste joignable s'il se TAIT. La mobilité est
    //   passive : tant que le nœud déplacé n'émet rien, l'autre côté écrit vers
    //   une adresse morte. Le rattrapage existe (keep-alive toutes les 25 s,
    //   puis reprise de la file), mais il n'est pas éprouvé ici — l'éprouver
    //   demanderait d'attendre ce keep-alive, et l'attente dominerait la
    //   campagne entière.
    // - Que la mobilité fonctionne à travers un RELAIS : `on_data` ne réaiguille
    //   que les sessions directes, une session relayée gardant l'adresse du
    //   relais. Un pair déplacé dont le trafic passe par un tunnel est un autre
    //   scénario, non couvert.
    // - Que le déplacement est réaliste dans sa forme. Il est ici instantané et
    //   sans perte : rien n'est en vol au moment où l'adresse change. Un vrai
    //   basculement Wi-Fi → 4G perd des paquets pendant plusieurs secondes.
    // - Que l'ancienne adresse est libérée proprement chez tous les tiers (DHT,
    //   carnet persistant d'autres pairs). Ce test n'a que deux nœuds.
}
