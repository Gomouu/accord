//! Le cœur du jalon multi-appareil : deux appareils d'un même compte tiennent
//! chacun leur session avec un tiers, sans s'évincer.
//!
//! `Endpoint::install_session` applique un invariant — « au plus une session
//! DIRECTE par identité » — et cet invariant est le correctif de la session
//! cadavre (Lot G, cause 4) : il ne doit pas bouger. Son éviction est écrite
//! contre `peer_static`, c'est-à-dire la clé de transport du pair. Tout le
//! jalon multi-appareil repose sur la conséquence énoncée dans
//! `docs/MULTI_DEVICE.md` §3.2.1 : donner une clé PROPRE à chaque appareil
//! transforme cet invariant en « au plus une session par appareil » sans
//! toucher une ligne du transport.
//!
//! Ce fichier prouve cette affirmation au lieu de la supposer, en trois
//! volets qui se tiennent mutuellement :
//!
//! 1. deux clés d'appareil distinctes cohabitent chez le tiers, et chacune
//!    reçoit et émet pour son compte ;
//! 2. la graine PARTAGÉE — l'approche naïve « restaure ta phrase de
//!    récupération sur la deuxième machine » — fait bien s'évincer les deux
//!    machines. Sans ce contrôle négatif, le test 1 ne prouverait rien : il
//!    passerait tout aussi bien si l'éviction n'existait pas du tout ;
//! 3. un appareil qui se reconnecte depuis une nouvelle adresse évince sa
//!    propre session périmée, et ELLE SEULE — le correctif Lot G reste entier
//!    et ne fait pas de dégât collatéral sur l'autre appareil du compte.

use accord_crypto::device::{AccountIdentity, DeviceIdentity};
use accord_crypto::Identity;
use accord_proto::core_msg::CoreMsg;
use accord_proto::plaintext::ChannelMsg;
use accord_proto::ControlMsg;
use accord_transport::clock::ManualClock;
use accord_transport::endpoint::{Endpoint, EndpointConfig, TransportEvent};
use accord_transport::socket::sim::{NetConditions, SimNet};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

const POW: u32 = 4;

fn config() -> EndpointConfig {
    EndpointConfig {
        pow_bits: POW,
        keepalive_ms: 25_000,
        idle_timeout_ms: 120_000,
        cookie_pressure_per_s: 64,
        relay_serving: false,
        capabilities: None,
    }
}

struct Node {
    ep: Arc<Endpoint>,
    events: mpsc::UnboundedReceiver<TransportEvent>,
    addr: SocketAddr,
}

fn spawn_node_avec_identite(
    net: &SimNet,
    clock: &ManualClock,
    addr: &str,
    id: Arc<Identity>,
) -> Node {
    let addr: SocketAddr = addr.parse().unwrap();
    let socket = Arc::new(net.bind(addr));
    let (ep, events) = Endpoint::new(
        socket,
        id,
        Arc::new(clock.clone()) as Arc<dyn accord_transport::Clock>,
        config(),
    );
    ep.spawn();
    Node { ep, events, addr }
}

fn spawn_node(net: &SimNet, clock: &ManualClock, addr: &str) -> Node {
    spawn_node_avec_identite(
        net,
        clock,
        addr,
        Arc::new(Identity::generate_with_pow_bits(POW)),
    )
}

/// Reconstruit l'identité de transport d'un appareil depuis sa graine
/// persistée. C'est littéralement ce que fait la machine à chaque démarrage :
/// la graine d'appareil ne quitte jamais le disque local, et rejouer cette
/// reconstruction est ce qui rend le volet 3 (redémarrage) fidèle.
fn identite_de(appareil: &DeviceIdentity) -> Arc<Identity> {
    Arc::new(Identity::from_seed_with_pow_bits(*appareil.seed(), POW))
}

fn presence(texte: &str) -> ChannelMsg {
    ChannelMsg::Core(CoreMsg::Presence {
        status: 0,
        custom: Some(texte.to_string()),
    })
}

async fn attendre(budget: Duration, mut cond: impl FnMut() -> bool) -> Result<(), ()> {
    let deadline = std::time::Instant::now() + budget;
    while std::time::Instant::now() < deadline {
        if cond() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    if cond() {
        Ok(())
    } else {
        Err(())
    }
}

/// Draine les événements jusqu'à avoir vu un `Presence` de CHACUNE des clés
/// attendues, et rend les statuts reçus indexés par émetteur.
///
/// Un appel séquentiel par émetteur ne conviendrait pas : l'ordre d'arrivée
/// des deux sessions n'est pas garanti, et un drainage naïf jetterait au
/// passage le message de l'autre appareil — le test échouerait alors pour une
/// raison qui n'a rien à voir avec l'invariant sous examen.
async fn presences_recues(
    node: &mut Node,
    attendus: &[[u8; 32]],
) -> HashMap<[u8; 32], Option<String>> {
    let mut vues: HashMap<[u8; 32], Option<String>> = HashMap::new();
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while vues.len() < attendus.len() && std::time::Instant::now() < deadline {
        let recu = tokio::time::timeout(Duration::from_millis(200), node.events.recv()).await;
        let Ok(Some(TransportEvent::Message {
            static_pub, msg, ..
        })) = recu
        else {
            continue;
        };
        if let ChannelMsg::Core(CoreMsg::Presence { custom, .. }) = *msg {
            if attendus.contains(&static_pub) {
                vues.insert(static_pub, custom);
            }
        }
    }
    vues
}

/// LE test du jalon. Deux appareils d'un même compte ouvrent chacun leur
/// session vers un tiers ; le tiers doit tenir les DEUX, et pouvoir parler à
/// chacune séparément.
///
/// Si l'invariant portait sur le compte plutôt que sur la clé de transport,
/// la deuxième installation retirerait la première et le tiers ne verrait
/// plus qu'un seul appareil — sans la moindre erreur nulle part. C'est
/// exactement le mode de panne silencieux décrit en §1 du document de
/// conception, et ce que ce test rend impossible à réintroduire sans bruit.
#[tokio::test]
async fn deux_appareils_du_meme_compte_joignent_un_tiers_sans_s_evincer() {
    let clock = ManualClock::new(1_000_000);
    let net = SimNet::new(21, NetConditions::default());

    // Un compte, deux appareils. La clé de compte ne touche JAMAIS le
    // transport : elle ne sert qu'à signer la liste d'appareils. Ce que le
    // tiers verra sur le fil, ce sont deux clés d'appareil sans lien
    // cryptographique visible entre elles.
    let compte = AccountIdentity::from_identity(Identity::generate_with_pow_bits(POW));
    let portable = compte.new_device();
    let fixe = compte.new_device();
    let cle_portable = portable.public_key();
    let cle_fixe = fixe.public_key();

    // Prémisse explicite : tout ce qui suit est vide de sens si les deux
    // appareils partagent une clé. La génération d'appareil doit rester
    // ALÉATOIRE et jamais dérivée de la graine de compte — une dérivation
    // déterministe redonnerait la même clé sur deux machines restaurées
    // depuis la même phrase, et ramènerait précisément le défaut que ce
    // jalon existe pour corriger (`docs/MULTI_DEVICE.md` §2).
    assert_ne!(
        cle_portable, cle_fixe,
        "deux appareils d'un même compte doivent être deux identités de transport DISTINCTES"
    );
    assert_ne!(
        cle_portable,
        compte.public_key(),
        "la clé d'appareil ne doit jamais être la clé de compte"
    );
    assert_ne!(
        cle_fixe,
        compte.public_key(),
        "la clé d'appareil ne doit jamais être la clé de compte"
    );

    let mut tiers = spawn_node(&net, &clock, "10.0.20.1:4000");
    let mut ap_portable =
        spawn_node_avec_identite(&net, &clock, "10.0.20.2:4000", identite_de(&portable));
    let mut ap_fixe = spawn_node_avec_identite(&net, &clock, "10.0.20.3:4000", identite_de(&fixe));

    // Les deux appareils s'annoncent au tiers « en même temps » : chaque envoi
    // déclenche son handshake et met le message en file jusqu'au WELCOME.
    ap_portable
        .ep
        .send(tiers.addr, &presence("portable"))
        .await
        .unwrap();
    ap_fixe
        .ep
        .send(tiers.addr, &presence("fixe"))
        .await
        .unwrap();

    attendre(Duration::from_secs(3), || tiers.ep.session_count() == 2)
        .await
        .expect("le tiers doit tenir une session par appareil (deux au total)");

    // Les deux sessions sont DIRECTES : c'est bien sur elles que porte
    // l'éviction. Une session relayée est exclue du filtre et ne prouverait
    // donc rien de l'invariant.
    let vues = tiers.ep.session_views();
    assert_eq!(vues.len(), 2, "exactement une session par appareil");
    assert!(
        vues.iter().all(|v| v.relay_circuit.is_none()),
        "les deux sessions doivent être directes : l'invariant d'éviction ne s'applique qu'à celles-là"
    );
    let mut cles_vues: Vec<[u8; 32]> = vues.iter().map(|v| v.peer_static).collect();
    cles_vues.sort_unstable();
    let mut cles_attendues = vec![cle_portable, cle_fixe];
    cles_attendues.sort_unstable();
    assert_eq!(
        cles_vues, cles_attendues,
        "le tiers doit voir les deux clés d'appareil, pas une seule survivante"
    );

    // Assertion décisive pour la LIVRAISON : c'est `direct_session_addr` que
    // la couche nœud interroge pour router. Si les deux appareils s'étaient
    // évincés, l'un des deux appels rendrait `None` ou — pire — l'adresse de
    // l'autre machine : le trou noir silencieux.
    assert_eq!(
        tiers.ep.direct_session_addr(&cle_portable),
        Some(ap_portable.addr),
        "le tiers doit savoir joindre le portable à SON adresse"
    );
    assert_eq!(
        tiers.ep.direct_session_addr(&cle_fixe),
        Some(ap_fixe.addr),
        "le tiers doit savoir joindre le fixe à SON adresse"
    );

    // Sens montant : les deux messages arrivent, attribués au bon appareil.
    // L'attribution compte autant que l'arrivée — c'est elle qui permettra à
    // la couche nœud de remonter les deux appareils au même compte.
    let recus = presences_recues(&mut tiers, &[cle_portable, cle_fixe]).await;
    assert_eq!(
        recus.get(&cle_portable),
        Some(&Some("portable".to_string())),
        "le message du portable n'est pas arrivé au tiers"
    );
    assert_eq!(
        recus.get(&cle_fixe),
        Some(&Some("fixe".to_string())),
        "le message du fixe n'est pas arrivé au tiers"
    );

    // Sens descendant : le tiers scelle une fois PAR APPAREIL (modèle de
    // livraison §5). Chaque machine ne doit recevoir que sa propre copie —
    // c'est ce qui distingue « deux sessions vivantes » de « une session
    // survivante qui répond deux fois ».
    tiers
        .ep
        .send(ap_portable.addr, &presence("pour-le-portable"))
        .await
        .unwrap();
    tiers
        .ep
        .send(ap_fixe.addr, &presence("pour-le-fixe"))
        .await
        .unwrap();

    let vu_portable = attendre_message(&mut ap_portable).await;
    assert_eq!(
        vu_portable,
        Some("pour-le-portable".to_string()),
        "le portable n'a pas reçu sa copie"
    );
    let vu_fixe = attendre_message(&mut ap_fixe).await;
    assert_eq!(
        vu_fixe,
        Some("pour-le-fixe".to_string()),
        "le fixe n'a pas reçu sa copie"
    );

    // Chaque appareil ne connaît que le tiers : aucune session parasite ne
    // vient gonfler les comptes ci-dessus.
    assert_eq!(ap_portable.ep.session_count(), 1);
    assert_eq!(ap_fixe.ep.session_count(), 1);
}

/// Draine les événements d'un appareil jusqu'au premier `Presence` reçu.
async fn attendre_message(node: &mut Node) -> Option<String> {
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while std::time::Instant::now() < deadline {
        let recu = tokio::time::timeout(Duration::from_millis(200), node.events.recv()).await;
        let Ok(Some(TransportEvent::Message { msg, .. })) = recu else {
            continue;
        };
        if let ChannelMsg::Core(CoreMsg::Presence { custom, .. }) = *msg {
            return custom;
        }
    }
    None
}

/// Contrôle négatif, et raison d'être du test précédent : avec une graine
/// PARTAGÉE — l'approche naïve « restaure ta phrase de récupération sur la
/// deuxième machine » — les deux machines sont la même identité sur le fil et
/// s'évincent bel et bien chez le tiers.
///
/// Sans ce test, `deux_appareils_du_meme_compte_joignent_un_tiers_sans_s_evincer`
/// passerait à l'identique dans un transport où l'éviction aurait été
/// silencieusement supprimée — il ne prouverait alors plus rien. Ici on
/// vérifie que le mécanisme est bien ARMÉ dans ce montage précis : la
/// cohabitation observée plus haut vient des clés distinctes, pas d'une règle
/// absente.
#[tokio::test]
async fn la_graine_partagee_fait_bien_s_evincer_les_deux_machines() {
    let clock = ManualClock::new(1_000_000);
    let net = SimNet::new(22, NetConditions::default());
    let tiers = spawn_node(&net, &clock, "10.0.21.1:4000");

    // Une seule identité pour deux machines : c'est exactement ce que produit
    // la restauration de la même phrase de récupération des deux côtés.
    let graine_commune = Arc::new(Identity::generate_with_pow_bits(POW));
    let machine1 =
        spawn_node_avec_identite(&net, &clock, "10.0.21.2:4000", Arc::clone(&graine_commune));
    let machine2 =
        spawn_node_avec_identite(&net, &clock, "10.0.21.3:4000", Arc::clone(&graine_commune));
    let cle = graine_commune.public_key();

    machine1
        .ep
        .send(
            tiers.addr,
            &ChannelMsg::Control(ControlMsg::Ping { token: 1 }),
        )
        .await
        .unwrap();
    attendre(Duration::from_secs(3), || {
        tiers.ep.direct_session_addr(&cle) == Some(machine1.addr)
    })
    .await
    .expect("session initiale avec la première machine");
    assert_eq!(tiers.ep.session_count(), 1);

    // La seconde machine se connecte. Même clé statique ⇒ son installation
    // évince la session de la première.
    machine2
        .ep
        .send(
            tiers.addr,
            &ChannelMsg::Control(ControlMsg::Ping { token: 2 }),
        )
        .await
        .unwrap();
    attendre(Duration::from_secs(3), || {
        tiers
            .ep
            .session_views()
            .iter()
            .any(|v| v.addr == machine2.addr)
    })
    .await
    .expect("session avec la seconde machine");

    // Une seule session survit : la première machine est devenue injoignable
    // pour le tiers, en silence. C'est le mode de panne que le jalon
    // multi-appareil élimine en donnant une clé propre à chaque appareil.
    assert_eq!(
        tiers.ep.session_count(),
        1,
        "graine partagée : l'invariant « au plus une session directe par identité » doit évincer"
    );
    assert_eq!(
        tiers.ep.direct_session_addr(&cle),
        Some(machine2.addr),
        "la session survivante doit être la plus récente"
    );
}

/// Le pendant qui protège le correctif Lot G : l'éviction doit rester
/// EXACTEMENT à sa portée. Un appareil qui redémarre silencieusement et
/// revient depuis une nouvelle adresse évince sa propre session périmée — et
/// laisse intacte celle de l'autre appareil du même compte.
///
/// Les deux moitiés comptent. Perdre la première réintroduit le cadavre (trou
/// noir UDP jusqu'à l'expiration d'inactivité, deux minutes) ; perdre la
/// seconde signifierait qu'on a « corrigé » le multi-appareil en désarmant
/// l'éviction, ce qui recasserait la reconnexion.
#[tokio::test]
async fn un_appareil_qui_se_reconnecte_evince_sa_session_perimee_et_elle_seule() {
    let clock = ManualClock::new(1_000_000);
    let net = SimNet::new(23, NetConditions::default());

    let compte = AccountIdentity::from_identity(Identity::generate_with_pow_bits(POW));
    let portable = compte.new_device();
    let fixe = compte.new_device();
    let cle_portable = portable.public_key();
    let cle_fixe = fixe.public_key();

    let tiers = spawn_node(&net, &clock, "10.0.22.1:4000");
    let portable_v1 =
        spawn_node_avec_identite(&net, &clock, "10.0.22.2:4000", identite_de(&portable));
    let ap_fixe = spawn_node_avec_identite(&net, &clock, "10.0.22.3:4000", identite_de(&fixe));

    for (machine, jeton) in [(&portable_v1, 1u64), (&ap_fixe, 2u64)] {
        machine
            .ep
            .send(
                tiers.addr,
                &ChannelMsg::Control(ControlMsg::Ping { token: jeton }),
            )
            .await
            .unwrap();
    }
    attendre(Duration::from_secs(3), || tiers.ep.session_count() == 2)
        .await
        .expect("les deux appareils du compte doivent être connectés au départ");

    // Le portable s'éteint sans adieu (mort UDP) et redémarre sur un nouveau
    // port avec la MÊME graine d'appareil : le tiers détient encore le
    // cadavre à l'ancienne adresse.
    net.set_down(portable_v1.addr, true);
    let portable_v2 =
        spawn_node_avec_identite(&net, &clock, "10.0.22.4:4000", identite_de(&portable));
    portable_v2
        .ep
        .send(
            tiers.addr,
            &ChannelMsg::Control(ControlMsg::Ping { token: 3 }),
        )
        .await
        .unwrap();
    attendre(Duration::from_secs(3), || {
        tiers
            .ep
            .session_views()
            .iter()
            .any(|v| v.addr == portable_v2.addr)
    })
    .await
    .expect("session avec la seconde incarnation du portable");

    // Toujours deux sessions, pas trois : le cadavre a été retiré à
    // l'installation de la fraîche. Trois signifierait que l'éviction a été
    // désarmée ; une seule, qu'elle mord trop large.
    assert_eq!(
        tiers.ep.session_count(),
        2,
        "le cadavre du portable doit être évincé, et lui seul"
    );
    assert!(
        tiers
            .ep
            .session_views()
            .iter()
            .all(|v| v.addr != portable_v1.addr),
        "aucune session ne doit subsister à l'ancienne adresse du portable"
    );
    assert_eq!(
        tiers.ep.direct_session_addr(&cle_portable),
        Some(portable_v2.addr),
        "le portable doit être joignable à sa NOUVELLE adresse"
    );
    // Le fixe n'a rien fait et n'a rien à subir : la reconnexion d'un
    // appareil ne doit pas déconnecter les autres appareils du compte.
    assert_eq!(
        tiers.ep.direct_session_addr(&cle_fixe),
        Some(ap_fixe.addr),
        "la session du fixe est un dégât collatéral interdit"
    );
}
