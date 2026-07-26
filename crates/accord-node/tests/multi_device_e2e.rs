//! Test d'intégration bout-en-bout du **multi-appareil** (jalon 1) : un compte
//! porté par DEUX machines complètes reçoit ses messages sur les deux, sans
//! qu'aucune n'évince l'autre.
//!
//! `crates/accord-transport/tests/multi_appareil_e2e.rs` prouve déjà, au
//! niveau du transport, que deux clés statiques distinctes cohabitent. Ce
//! fichier-ci prouve la moitié qui manquait : que le **nœud complet** — liste
//! d'appareils signée, résolution appareil → compte, éventail de livraison,
//! file hors-ligne par appareil — s'en sert réellement. La distinction
//! compte/appareil peut être parfaitement juste unité par unité et ne rien
//! livrer du tout si `deliver_core` ne consulte pas les cibles.
//!
//! Quatre propriétés, dans l'ordre de ce qu'elles coûteraient si elles
//! tombaient :
//!
//! 1. les deux appareils basculés reçoivent le même message direct ;
//! 2. ils tiennent leur session avec l'ami **en même temps** ;
//! 3. l'appareil éteint pendant l'envoi rattrape à son retour, et celui qui
//!    était allumé ne reçoit rien une seconde fois ;
//! 4. 🔒 le parc d'aujourd'hui — aucun appareil basculé — reçoit exactement un
//!    exemplaire à la clé de compte. C'est le garde-fou qui compte le plus :
//!    un éventail trop large ne se voit pas en test unitaire et coupe la
//!    livraison de tout le monde le jour de la sortie.
//!
//! ## Ce que le harnais fabrique, et pourquoi
//!
//! Deux points d'état sont écrits directement dans les bases, avant tout
//! démarrage, parce que le protocole ne permet pas de les obtenir depuis un
//! test de nœud :
//!
//! - **les amitiés**, pour éviter de rejouer une poignée de main déjà couverte
//!   par `two_node_e2e.rs` — et surtout parce qu'un compte à deux machines
//!   devrait la jouer deux fois, ce qui n'a pas de sens du côté de l'ami ;
//! - **la liste à deux appareils**, que l'appairage (lot 1.D) écrit au même
//!   endroit (`Node::store_device_list`) mais dont le transport n'est pas
//!   câblé : `devices.pair_submit` rend les octets PAKE à l'appelant, à
//!   charge pour l'hôte de les acheminer. Aucune API de nœud ne les envoie.
//!
//! Tout le reste passe par le vrai code : la liste est signée par la racine du
//! compte et **vérifiée** par l'ami au travers de son point d'entrée public
//! (`Node::ingest_device_list_record` — publieur, ancrage de clé DHT,
//! signature racine, preuve de travail par appareil, version monotone), et les
//! messages traversent un vrai socket UDP.

use std::time::Duration;

use accord_core::db::{CachedDeviceList, Contact, ContactState};
use accord_core::Db;
use accord_crypto::{node_id_of, DeviceIdentity, Identity};
use accord_node::{
    device, identity, run, run_with_maintenance, MaintenanceConfig, NodeConfig, Paths, RunningNode,
    Unlocked,
};
use accord_proto::device::{DeviceEntry, DeviceList, DEVICE_FLAG_TRANSPORT_KEY};
use accord_proto::WireEncode;

/// Phrase de passe locale de tous les profils du fichier. Sans conséquence :
/// ce qui distingue les deux machines du compte est leur clé d'appareil, pas
/// leur coffre.
const PASSPHRASE: &str = "phrase-de-passe-multi-appareil";

/// Drapeaux d'un appareil dont le transport présente **vraiment** sa propre
/// clé. Jamais posé à la main sans démarrer la machine dans cet état : un
/// drapeau qui ment dirige les messages vers une clé que personne n'écoute.
const TRANSPORT: u32 = DEVICE_FLAG_TRANSPORT_KEY;

// ---------------------------------------------------------------------------
// Harnais
// ---------------------------------------------------------------------------

/// Configuration de nœud des tests : boucle locale, port éphémère, preuve de
/// travail symbolique et mDNS coupé — la vraie difficulté ferait ramper la
/// suite, et l'annonce LAN ferait dépendre le test du réseau de la machine.
///
/// 🔒 `device_key_transport` n'est **pas** forcé : c'est la valeur par défaut
/// qui s'applique, donc celle que l'application livre. Le jalon se démontre sur
/// la configuration réelle ; forcer le drapeau ici rendrait ces tests muets sur
/// un retour en arrière du défaut, qui reste le recours d'urgence.
fn config(paths: Paths) -> NodeConfig {
    NodeConfig {
        paths,
        p2p_addr: "127.0.0.1:0".parse().unwrap(),
        api_port: 0,
        pow_bits: 1,
        mdns_enabled: false,
        ..NodeConfig::default()
    }
}

/// Configuration d'un pair **d'avant le basculement** (6.4.0 et antérieurs) :
/// son transport présente encore la clé de compte. C'est le parc qu'on doit
/// continuer de joindre, et le retour en arrière d'urgence.
fn config_avant_bascule(paths: Paths) -> NodeConfig {
    NodeConfig {
        device_key_transport: false,
        ..config(paths)
    }
}

/// Maintenance accélérée, réservée au test de rattrapage : le vidage de
/// l'outbox par défaut (30 s) dépasse la borne d'attente du harnais.
///
/// ⚠️ Les autres tests gardent la cadence par défaut **exprès** : une outbox
/// qui se vide toutes les 200 ms relivrerait un message dont l'envoi direct a
/// échoué, et un test de session vivante ne prouverait plus rien.
fn maintenance_rapide() -> MaintenanceConfig {
    MaintenanceConfig {
        outbox_flush: Duration::from_millis(200),
        ..MaintenanceConfig::default()
    }
}

/// Démarre un nœud sur un profil déjà scellé, dans la configuration LIVRÉE.
async fn boot(paths: &Paths, maintenance: MaintenanceConfig) -> RunningNode {
    let unlocked = identity::unlock(paths, PASSPHRASE).unwrap();
    run_with_maintenance(unlocked, config(paths.clone()), maintenance)
        .await
        .unwrap()
}

/// [`boot`] pour un pair d'avant le basculement.
async fn boot_avant_bascule(paths: &Paths, maintenance: MaintenanceConfig) -> RunningNode {
    let unlocked = identity::unlock(paths, PASSPHRASE).unwrap();
    run_with_maintenance(unlocked, config_avant_bascule(paths.clone()), maintenance)
        .await
        .unwrap()
}

/// Attend qu'une condition devienne vraie (interrogation courte, borne dure).
async fn eventually(cond: impl FnMut() -> bool) -> bool {
    eventually_within(Duration::from_secs(22), cond).await
}

/// [`eventually`] à borne explicite.
///
/// La borne est parfois une **assertion** et non un simple garde-fou : voir
/// `les_deux_appareils_tiennent_leur_session_en_meme_temps`, où arriver vite
/// est précisément ce qui distingue une session vivante d'un rattrapage par
/// la file hors-ligne.
async fn eventually_within(limite: Duration, mut cond: impl FnMut() -> bool) -> bool {
    let fin = tokio::time::Instant::now() + limite;
    while tokio::time::Instant::now() < fin {
        if cond() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    cond()
}

/// Horloge murale, dans l'unité des listes d'appareils.
fn maintenant_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

/// Nombre d'exemplaires de `texte` reçus de `auteur` dans l'historique direct.
///
/// Compter, et pas seulement constater la présence : c'est la seule façon de
/// distinguer « livré » de « livré deux fois ».
fn copies_recues(node: &RunningNode, auteur: &[u8; 32], texte: &str) -> usize {
    use accord_proto::core_msg::MsgBody;
    node.node
        .dm_history(auteur, u64::MAX, 100)
        .map(|h| {
            h.iter()
                .filter(|m| {
                    m.author == *auteur
                        && matches!(
                            MsgBody::decode_body(m.kind, &m.body),
                            Ok(MsgBody::Text { ref text, .. }) if *text == texte
                        )
                })
                .count()
        })
        .unwrap_or(0)
}

/// Messages mis en file par ce nœud depuis son démarrage.
///
/// 🔒 C'est **le** compteur qui attrape un éventail trop large. Un message
/// direct est durable, donc persisté dans l'outbox pour CHAQUE cible, même
/// quand l'envoi direct aboutit ; le compteur vaut donc exactement le nombre
/// de cibles retenues. L'historique du destinataire, lui, ne peut rien dire :
/// deux exemplaires du même `msg_id` y sont dédupliqués à l'ingestion, si bien
/// qu'un doublon d'émission y serait parfaitement invisible.
fn mises_en_file(node: &RunningNode) -> u64 {
    node.diagnostics_counters().outbox.enqueued
}

/// Inscrit `peer` comme ami confirmé dans une base encore fermée au nœud.
fn inscrire_ami(db: &Db, peer: &[u8; 32], nom: &str) {
    db.upsert_contact(&Contact {
        node_id: node_id_of(peer).0,
        pubkey: *peer,
        display_name: nom.to_string(),
        state: ContactState::Friend,
        added_ms: 1,
        last_seen_ms: 1,
        verified_at: None,
        verified_pubkey: None,
    })
    .unwrap();
}

/// Liste d'appareils signée par la racine du compte.
///
/// Reproduit ce que signe un appareil déjà autorisé à la fin d'un appairage :
/// même structure, même domaine de signature, mêmes nonces de preuve de
/// travail que les appareils réellement générés — la vérification de l'ami
/// s'exécute donc pour de bon, elle n'est pas contournée.
fn liste_signee(root: &Identity, devices: &[(&DeviceIdentity, &str, u32)], now: u64) -> DeviceList {
    let mut list = DeviceList {
        account: root.public_key(),
        version: accord_crypto::version_for(now),
        issued_ms: now,
        valid_for_s: device::DEVICE_LIST_VALID_S,
        devices: devices
            .iter()
            .map(|(d, nom, flags)| DeviceEntry {
                pubkey: d.public_key(),
                pow_nonce: d.pow_nonce(),
                name: (*nom).to_string(),
                added_ms: now,
                flags: *flags,
            })
            .collect(),
        revoked: Vec::new(),
        sig: [0u8; 64],
    };
    accord_crypto::sign_device_list_with_root(root, &mut list);
    list
}

/// Écrit la liste du compte dans une base, comme le fait l'appairage une fois
/// l'empreinte confirmée.
fn stocker_liste(db: &Db, list: &DeviceList) {
    let mut w = accord_proto::Writer::new();
    list.encode(&mut w);
    db.cache_device_list(&CachedDeviceList {
        account: list.account,
        version: list.version,
        encoded: w.into_bytes(),
        fetched_ms: maintenant_ms(),
    })
    .unwrap();
}

/// Un compte à deux machines et l'ami qui leur écrit, préparés sur disque mais
/// pas encore démarrés.
struct Profils {
    _dirs: Vec<tempfile::TempDir>,
    paths_a: Paths,
    paths_b: Paths,
    paths_f: Paths,
    /// Clé racine du compte : la seule capable de signer une liste.
    root: Identity,
    compte: [u8; 32],
    ami: [u8; 32],
    device_a: DeviceIdentity,
    device_b: DeviceIdentity,
}

/// Prépare deux profils d'un **même compte** et celui d'un ami.
///
/// La seconde machine restaure la phrase de récupération de la première :
/// c'est le geste exact qui, avant ce jalon, faisait s'évincer les deux
/// machines chez chacun de leurs amis (`docs/MULTI_DEVICE.md` §1). Les
/// identités d'appareil sont générées ici plutôt qu'au démarrage — même appel
/// (`device::ensure_local_device`, idempotent), mais le test a besoin de leurs
/// nonces de preuve de travail pour signer une liste que l'ami acceptera.
fn preparer() -> Profils {
    let dirs: Vec<tempfile::TempDir> = (0..3).map(|_| tempfile::tempdir().unwrap()).collect();
    let paths_a = Paths::new(dirs[0].path());
    let paths_b = Paths::new(dirs[1].path());
    let paths_f = Paths::new(dirs[2].path());

    let (unlocked_a, phrase) = identity::create_with_phrase(&paths_a, PASSPHRASE, 1).unwrap();
    let Unlocked {
        identity: root,
        db_key: cle_a,
    } = unlocked_a;
    let unlocked_b = identity::restore_from_phrase(&paths_b, &phrase, PASSPHRASE, 1).unwrap();
    let unlocked_f = identity::create(&paths_f, PASSPHRASE, 1).unwrap();

    let compte = root.public_key();
    let ami = unlocked_f.identity.public_key();
    assert_eq!(
        unlocked_b.identity.public_key(),
        compte,
        "les deux machines doivent porter le MÊME compte"
    );
    assert_ne!(ami, compte, "l'ami est un tiers, pas une machine du compte");

    let db_a = Db::open(&paths_a.db(), &cle_a).unwrap();
    let db_b = Db::open(&paths_b.db(), &unlocked_b.db_key).unwrap();
    let db_f = Db::open(&paths_f.db(), &unlocked_f.db_key).unwrap();

    let device_a = device::ensure_local_device(&db_a).unwrap();
    let device_b = device::ensure_local_device(&db_b).unwrap();
    // 🔒 Le cœur du jalon. Deux machines restaurées depuis la même phrase
    // partagent tout SAUF ceci ; si les clés d'appareil se confondaient, tout
    // ce fichier passerait en parlant deux fois à la même machine.
    assert_ne!(
        device_a.public_key(),
        device_b.public_key(),
        "deux machines d'un même compte doivent avoir des clés d'appareil DISTINCTES"
    );
    assert_ne!(device_a.public_key(), compte);
    assert_ne!(device_b.public_key(), compte);

    inscrire_ami(&db_a, &ami, "Ami");
    inscrire_ami(&db_b, &ami, "Ami");
    inscrire_ami(&db_f, &compte, "Compte");

    Profils {
        _dirs: dirs,
        paths_a,
        paths_b,
        paths_f,
        root,
        compte,
        ami,
        device_a,
        device_b,
    }
}

impl Profils {
    /// Écrit la même liste dans les bases des deux machines du compte : chacune
    /// doit connaître la flotte entière, pas seulement elle-même.
    fn stocker_sur_le_compte(&self, list: &DeviceList) {
        for (paths, cle) in [
            (&self.paths_a, identity::unlock(&self.paths_a, PASSPHRASE)),
            (&self.paths_b, identity::unlock(&self.paths_b, PASSPHRASE)),
        ] {
            let db = Db::open(&paths.db(), &cle.unwrap().db_key).unwrap();
            stocker_liste(&db, list);
        }
    }

    /// Fait apprendre à l'ami la liste d'appareils du compte, par le point
    /// d'entrée public que la résolution DHT emprunte.
    ///
    /// 🔒 Rien n'est écrit dans la base de l'ami : le record traverse
    /// `verify_device_list_record`, qui refuse un publieur qui n'est pas le
    /// compte, une clé DHT qui ne dérive pas de lui, une signature invalide,
    /// une preuve de travail d'appareil insuffisante et une version déjà vue.
    /// Une liste forgée n'entrerait pas ici.
    fn faire_connaitre(&self, f: &RunningNode, list: &DeviceList) {
        let record = device::device_list_record_with_root(&self.root, list, maintenant_ms());
        f.node
            .ingest_device_list_record(&self.compte, &record)
            .expect("l'ami doit accepter la liste signée du compte");
    }
}

// ---------------------------------------------------------------------------
// 1. Le jalon : les deux appareils reçoivent
// ---------------------------------------------------------------------------

#[tokio::test]
async fn les_deux_appareils_dun_compte_recoivent_le_meme_message() {
    let p = preparer();
    let liste = liste_signee(
        &p.root,
        &[
            (&p.device_a, "Portable", TRANSPORT),
            (&p.device_b, "Fixe", TRANSPORT),
        ],
        maintenant_ms(),
    );
    p.stocker_sur_le_compte(&liste);

    // Les trois machines démarrent dans la configuration LIVRÉE : rien n'est
    // forcé, le basculement est le défaut. C'est ce qui fait de ce test
    // l'assertion du jalon plutôt qu'une démonstration en laboratoire.
    let a = boot(&p.paths_a, MaintenanceConfig::default()).await;
    let b = boot(&p.paths_b, MaintenanceConfig::default()).await;
    let f = boot(&p.paths_f, MaintenanceConfig::default()).await;

    // 🔒 Le garde-fou qui rend tout le reste concluant : sans lui, un retour
    // du défaut à `false` ferait passer ce fichier en parlant deux fois à la
    // même machine, et le jalon serait perdu sans qu'un test ne bronche.
    assert_ne!(
        a.node.transport_key(),
        a.node.public_key(),
        "la configuration par défaut doit présenter la clé d'APPAREIL"
    );
    // La clé présentée par chaque machine est bien SA clé d'appareil, pas la
    // clé du compte : sans ça les deux sessions retomberaient sur une seule
    // identité de transport et s'évinceraient.
    assert_eq!(a.node.transport_key(), p.device_a.public_key());
    assert_eq!(b.node.transport_key(), p.device_b.public_key());
    assert_ne!(a.node.transport_key(), b.node.transport_key());
    assert_eq!(
        a.node.public_key(),
        p.compte,
        "le compte, lui, ne bouge pas"
    );
    assert_eq!(b.node.public_key(), p.compte);

    p.faire_connaitre(&f, &liste);

    // Amorçage de présence : l'ami connaît une adresse PAR APPAREIL, et les
    // machines apprennent de l'ami sa liste et l'adresse qu'elle désigne — lui
    // aussi est basculé, sa clé de compte ne mène nulle part.
    f.register_peer(p.device_a.public_key(), a.p2p_addr());
    f.register_peer(p.device_b.public_key(), b.p2p_addr());
    a.learn_peer(&f).unwrap();
    b.learn_peer(&f).unwrap();

    // L'éventail lui-même, avant toute livraison : deux cibles, les deux clés
    // d'appareil, et surtout PAS la clé de compte — plus personne ne l'écoute.
    let cibles = f.node.delivery_targets(&p.compte);
    assert_eq!(
        cibles.len(),
        2,
        "une cible par appareil basculé : {cibles:?}"
    );
    assert!(cibles.contains(&p.device_a.public_key()));
    assert!(cibles.contains(&p.device_b.public_key()));
    assert!(
        !cibles.contains(&p.compte),
        "aucune machine n'écoute la clé de compte une fois tout basculé"
    );

    const TEXTE: &str = "un seul message, deux machines";
    let avant = mises_en_file(&f);
    f.node.dm_send(&p.compte, TEXTE, None).unwrap();

    assert!(
        eventually(|| copies_recues(&a, &p.ami, TEXTE) == 1).await,
        "le portable n'a pas reçu le message (reçu {} fois)",
        copies_recues(&a, &p.ami, TEXTE)
    );
    assert!(
        eventually(|| copies_recues(&b, &p.ami, TEXTE) == 1).await,
        "le fixe n'a pas reçu le message (reçu {} fois)",
        copies_recues(&b, &p.ami, TEXTE)
    );

    // Deux remises, pas trois : la clé de compte ne doit pas s'ajouter aux
    // deux appareils, sinon un message serait déposé pour toujours en file
    // pour un destinataire qui n'existe plus.
    assert_eq!(
        mises_en_file(&f) - avant,
        2,
        "un message vers un compte à deux appareils basculés = exactement deux remises"
    );

    a.shutdown();
    b.shutdown();
    f.shutdown();
}

// ---------------------------------------------------------------------------
// 2. Pas d'éviction : les deux sessions vivent en même temps
// ---------------------------------------------------------------------------

#[tokio::test]
async fn les_deux_appareils_tiennent_leur_session_en_meme_temps() {
    let p = preparer();
    let liste = liste_signee(
        &p.root,
        &[
            (&p.device_a, "Portable", TRANSPORT),
            (&p.device_b, "Fixe", TRANSPORT),
        ],
        maintenant_ms(),
    );
    p.stocker_sur_le_compte(&liste);

    // Cadence de maintenance PAR DÉFAUT, délibérément : l'outbox ne se vide
    // qu'après 30 s, donc rien de ce qui arrive dans les secondes qui suivent
    // ne peut venir d'un rattrapage. C'est ce qui rend la borne serrée du
    // second tour concluante.
    let a = boot(&p.paths_a, MaintenanceConfig::default()).await;
    let b = boot(&p.paths_b, MaintenanceConfig::default()).await;
    let f = boot(&p.paths_f, MaintenanceConfig::default()).await;
    assert_ne!(
        a.node.transport_key(),
        a.node.public_key(),
        "la configuration par défaut doit présenter la clé d'APPAREIL"
    );

    p.faire_connaitre(&f, &liste);
    f.register_peer(p.device_a.public_key(), a.p2p_addr());
    f.register_peer(p.device_b.public_key(), b.p2p_addr());
    a.learn_peer(&f).unwrap();
    b.learn_peer(&f).unwrap();

    // Premier tour : les deux sessions s'établissent (poignée de main incluse,
    // d'où la borne large).
    const TOUR1: &str = "premier tour";
    f.node.dm_send(&p.compte, TOUR1, None).unwrap();
    assert!(
        eventually(|| copies_recues(&a, &p.ami, TOUR1) == 1).await,
        "le portable n'a pas ouvert sa session"
    );
    assert!(
        eventually(|| copies_recues(&b, &p.ami, TOUR1) == 1).await,
        "le fixe n'a pas ouvert sa session"
    );

    // Sens retour, depuis les deux machines : l'ami reçoit les deux, ce qui
    // suppose deux sessions entrantes distinctes chez lui — l'ancienne
    // approche à graine partagée n'aurait pu en tenir qu'une.
    const REPONSE_A: &str = "ici le portable";
    const REPONSE_B: &str = "ici le fixe";
    a.node.dm_send(&p.ami, REPONSE_A, None).unwrap();
    b.node.dm_send(&p.ami, REPONSE_B, None).unwrap();
    assert!(
        eventually(|| copies_recues(&f, &p.compte, REPONSE_A) == 1).await,
        "la réponse du portable n'est pas arrivée"
    );
    assert!(
        eventually(|| copies_recues(&f, &p.compte, REPONSE_B) == 1).await,
        "la réponse du fixe n'est pas arrivée"
    );

    // 🔒 Le contrôle qui mord. Les deux sessions viennent d'être utilisées ;
    // si l'une avait évincé l'autre, la machine évincée ne recevrait ce second
    // message qu'au prochain vidage d'outbox — 30 s plus tard. Arriver dans
    // les trois secondes ne peut donc s'expliquer que par une session
    // directe encore vivante, des deux côtés à la fois.
    const TOUR2: &str = "second tour";
    f.node.dm_send(&p.compte, TOUR2, None).unwrap();
    let vite = Duration::from_secs(3);
    assert!(
        eventually_within(vite, || copies_recues(&a, &p.ami, TOUR2) == 1).await,
        "la session du portable a été évincée"
    );
    assert!(
        eventually_within(vite, || copies_recues(&b, &p.ami, TOUR2) == 1).await,
        "la session du fixe a été évincée"
    );

    a.shutdown();
    b.shutdown();
    f.shutdown();
}

// ---------------------------------------------------------------------------
// 3. L'appareil éteint rattrape, l'autre ne reçoit pas deux fois
// ---------------------------------------------------------------------------

#[tokio::test]
async fn un_appareil_eteint_rattrape_a_son_retour() {
    let p = preparer();
    let liste = liste_signee(
        &p.root,
        &[
            (&p.device_a, "Portable", TRANSPORT),
            (&p.device_b, "Fixe", TRANSPORT),
        ],
        maintenant_ms(),
    );
    p.stocker_sur_le_compte(&liste);

    // Ici la cadence rapide est nécessaire : c'est la file hors-ligne qui
    // porte tout le rattrapage.
    let a = boot(&p.paths_a, maintenance_rapide()).await;
    let f = boot(&p.paths_f, maintenance_rapide()).await;

    p.faire_connaitre(&f, &liste);
    f.register_peer(p.device_a.public_key(), a.p2p_addr());
    a.learn_peer(&f).unwrap();

    // Le fixe est éteint : l'ami vise quand même les deux appareils, parce que
    // la liste dit qui existe, pas qui est allumé.
    assert_eq!(f.node.delivery_targets(&p.compte).len(), 2);

    const TEXTE: &str = "envoyé pendant que le fixe dormait";
    f.node.dm_send(&p.compte, TEXTE, None).unwrap();
    assert!(
        eventually(|| copies_recues(&a, &p.ami, TEXTE) == 1).await,
        "le portable, lui, était joignable"
    );

    // Le fixe s'allume. Son adresse P2P est réapprise — c'est ce que fait la
    // résolution de présence en production.
    let b = boot(&p.paths_b, maintenance_rapide()).await;
    f.register_peer(p.device_b.public_key(), b.p2p_addr());
    b.learn_peer(&f).unwrap();
    // Un mot du fixe ouvre la session : à la connexion, l'ami vide la file de
    // CETTE machine (`flush_peer`), indépendamment du backoff en cours.
    b.node.dm_send(&p.ami, "de retour", None).unwrap();

    assert!(
        eventually(|| copies_recues(&b, &p.ami, TEXTE) == 1).await,
        "le fixe n'a pas rattrapé le message manqué"
    );

    // 🔒 Et le portable n'a pas été resservi au passage : la file est indexée
    // par appareil, pas par compte. Une file partagée aurait relivré ici.
    assert_eq!(
        copies_recues(&a, &p.ami, TEXTE),
        1,
        "le portable a reçu le message une seconde fois"
    );
    // 🔒 Et rien d'ÉTRANGER n'a atterri dans cette conversation.
    //
    // Cette assertion comptait les messages et en exigeait UN. Elle ne tenait
    // qu'en gagnant une course : « de retour », envoyé par le fixe juste
    // au-dessus, se synchronise vers le portable — c'est le multi-appareil qui
    // fait son travail, pas une anomalie. Le test finissait simplement avant.
    // Sous charge il perdait la course, et son message d'échec envoyait
    // chercher une double livraison qui n'existe pas.
    //
    // On vérifie donc ce qui était réellement visé : aucun corps inattendu.
    // Le non-doublon de TEXTE est déjà épinglé juste au-dessus, et cette
    // formulation est vraie que la synchro soit arrivée ou non.
    let attendus = [TEXTE, "de retour"];
    for m in a.node.dm_history(&p.ami, u64::MAX, 100).unwrap() {
        let texte = match accord_proto::core_msg::MsgBody::decode_body(m.kind, &m.body) {
            Ok(accord_proto::core_msg::MsgBody::Text { text, .. }) => text,
            autre => panic!("corps inattendu dans la conversation du portable : {autre:?}"),
        };
        assert!(
            attendus.contains(&texte.as_str()),
            "message étranger dans la conversation du portable : {texte:?}"
        );
    }

    a.shutdown();
    b.shutdown();
    f.shutdown();
}

// ---------------------------------------------------------------------------
// 4. Le parc d'avant le basculement, inchangé
// ---------------------------------------------------------------------------

/// Démarre un nœud tout neuf **d'avant le basculement**, tel qu'il tourne
/// encore chez tous ceux qui n'ont pas mis à jour.
async fn boot_avant_bascule_neuf(dir: &std::path::Path) -> RunningNode {
    let paths = Paths::new(dir);
    let unlocked = identity::create(&paths, PASSPHRASE, 1).unwrap();
    run(unlocked, config_avant_bascule(paths)).await.unwrap()
}

#[tokio::test]
async fn un_ami_sans_liste_dappareils_recoit_un_seul_exemplaire() {
    // 🔒 Le garde-fou de régression. Deux pairs d'avant le basculement, dont
    // aucun ne connaît de liste d'appareils : si l'éventail se trompait ne
    // serait-ce que d'une cible, tout le parc pas encore mis à jour cesserait
    // de recevoir. Ce test rejoue donc le chemin d'avant le jalon, poignée de
    // main comprise.
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let alice = boot_avant_bascule_neuf(dir_a.path()).await;
    let bob = boot_avant_bascule_neuf(dir_b.path()).await;

    let alice_pub = alice.node.public_key();
    let bob_pub = bob.node.public_key();
    // Sans basculement, la clé de transport EST la clé de compte : c'est ce
    // que voit encore tout le parc pas mis à jour, et c'est ce qui rend
    // l'inscription d'une adresse sous une clé de compte suffisante ici.
    assert_eq!(alice.node.transport_key(), alice_pub);
    assert_eq!(bob.node.transport_key(), bob_pub);

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
        "Bob n'a pas reçu la demande d'ami"
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
        "l'amitié n'a pas été confirmée"
    );

    // Une seule cible, la clé de compte : le comportement d'avant, à l'octet.
    assert_eq!(
        alice.node.delivery_targets(&bob_pub),
        vec![bob_pub],
        "sans liste connue, on vise le compte et rien d'autre"
    );

    const TEXTE: &str = "message au parc d'avant le basculement";
    let avant = mises_en_file(&alice);
    alice.node.dm_send(&bob_pub, TEXTE, None).unwrap();
    assert!(
        eventually(|| copies_recues(&bob, &alice_pub, TEXTE) == 1).await,
        "Bob n'a pas reçu exactement un exemplaire (reçu {} fois)",
        copies_recues(&bob, &alice_pub, TEXTE)
    );
    assert_eq!(
        mises_en_file(&alice) - avant,
        1,
        "un message vers un ami sans liste = exactement une remise"
    );

    alice.shutdown();
    bob.shutdown();
}

#[tokio::test]
async fn une_liste_sans_appareil_bascule_vise_encore_le_compte() {
    // 🔒 L'état de TOUT le parc pendant la phase 1 : la liste est publiée,
    // signée et fraîche — et aucun transport n'a encore basculé. Viser les
    // appareils ici enverrait chaque message à une clé que personne n'écoute,
    // et la panne serait silencieuse des deux côtés.
    let p = preparer();
    let liste = liste_signee(
        &p.root,
        &[(&p.device_a, "Portable", 0), (&p.device_b, "Fixe", 0)],
        maintenant_ms(),
    );
    p.stocker_sur_le_compte(&liste);

    // `device_key_transport: false` — les machines présentent bien la clé de
    // compte, en accord avec les drapeaux à zéro de la liste. Seule la
    // première est allumée : deux machines sur la même clé s'évinceraient,
    // et c'est justement le blocage que la phase 2 lève.
    let a = boot_avant_bascule(&p.paths_a, MaintenanceConfig::default()).await;
    let f = boot_avant_bascule(&p.paths_f, MaintenanceConfig::default()).await;
    assert_eq!(
        a.node.transport_key(),
        p.compte,
        "en phase 1 le transport présente la clé de COMPTE"
    );

    p.faire_connaitre(&f, &liste);
    f.register_peer(p.compte, a.p2p_addr());
    a.register_peer(p.ami, f.p2p_addr());

    // L'ami connaît deux appareils et vise pourtant une seule clé : celle du
    // compte, que les deux machines présentent.
    assert_eq!(
        f.node.delivery_targets(&p.compte),
        vec![p.compte],
        "une liste dont aucun appareil n'a basculé se joint par le compte"
    );

    const TEXTE: &str = "message au parc en phase 1";
    let avant = mises_en_file(&f);
    f.node.dm_send(&p.compte, TEXTE, None).unwrap();
    assert!(
        eventually(|| copies_recues(&a, &p.ami, TEXTE) == 1).await,
        "la machine en phase 1 n'a pas reçu son exemplaire unique (reçu {} fois)",
        copies_recues(&a, &p.ami, TEXTE)
    );
    assert_eq!(
        mises_en_file(&f) - avant,
        1,
        "une liste non basculée ne doit produire qu'une seule remise"
    );

    a.shutdown();
    f.shutdown();
}

// ---------------------------------------------------------------------------
// 5. Le jour du basculement : les deux versions se parlent
// ---------------------------------------------------------------------------

/// Prépare deux profils SANS lien préalable : un pair resté sur la version
/// d'avant, un pair basculé. Aucune amitié n'est écrite dans les bases —
/// c'est la poignée de main qui doit la nouer, et c'est là que se joue tout
/// l'intérêt de ce test.
fn preparer_deux_versions() -> (tempfile::TempDir, tempfile::TempDir, Paths, Paths) {
    let dir_v = tempfile::tempdir().unwrap();
    let dir_n = tempfile::tempdir().unwrap();
    let paths_v = Paths::new(dir_v.path());
    let paths_n = Paths::new(dir_n.path());
    identity::create(&paths_v, PASSPHRASE, 1).unwrap();
    identity::create(&paths_n, PASSPHRASE, 1).unwrap();
    (dir_v, dir_n, paths_v, paths_n)
}

#[tokio::test]
async fn un_pair_davant_et_un_pair_bascule_se_parlent_encore() {
    // 🔒 **Le test qui décide si le basculement est déployable.** Le parc ne
    // bascule pas d'un bloc : pendant des semaines, chaque conversation aura
    // un pair sur chaque version. Si les deux ne s'entendent plus, la mise à
    // jour coupe les gens de leurs amis restés en arrière — et la panne est
    // silencieuse des deux côtés, puisqu'un message qui n'aboutit pas se
    // contente d'attendre en file.
    //
    // Le premier contact part du pair **d'avant** : c'est le sens que le code
    // livré en 6.3.0 sait tenir seul, et donc le seul dont ce test puisse
    // honnêtement affirmer quelque chose. (Le sens inverse — un pair basculé
    // qui demande l'amitié à un pair d'avant — dépend, chez le destinataire,
    // d'un rattachement que 6.3.0 ne sait pas faire : il enregistrerait la
    // relation au nom de notre machine. Rien ici ne peut le corriger, c'est
    // son binaire ; c'est une limite du jour du basculement, pas de ce test.)
    let (_dir_v, _dir_n, paths_v, paths_n) = preparer_deux_versions();
    let ancien = boot_avant_bascule(&paths_v, maintenance_rapide()).await;
    let neuf = boot(&paths_n, maintenance_rapide()).await;

    let ancien_pub = ancien.node.public_key();
    let neuf_pub = neuf.node.public_key();

    // Les deux versions sont bien celles annoncées.
    assert_eq!(
        ancien.node.transport_key(),
        ancien_pub,
        "le pair d'avant présente encore sa clé de compte"
    );
    assert_ne!(
        neuf.node.transport_key(),
        neuf_pub,
        "le pair basculé présente sa clé d'appareil"
    );

    // Ce que la DHT rend à chacun en production : la liste de l'autre, puis
    // l'adresse de la machine qu'elle désigne.
    ancien.learn_peer(&neuf).unwrap();
    neuf.learn_peer(&ancien).unwrap();

    // 🔒 L'éventail, dans les deux sens, AVANT le moindre envoi. C'est la
    // moitié que le code de la phase 1 doit déjà savoir tenir : lire la liste
    // d'un pair basculé et viser sa machine.
    assert_eq!(
        ancien.node.delivery_targets(&neuf_pub),
        vec![neuf.node.transport_key()],
        "le pair d'avant doit viser la MACHINE du pair basculé"
    );
    assert_eq!(
        neuf.node.delivery_targets(&ancien_pub),
        vec![ancien_pub],
        "le pair basculé doit viser la clé de COMPTE du pair d'avant"
    );

    // Premier contact, du pair d'avant vers le pair basculé.
    ancien.node.friend_request(&neuf_pub, "Ancien").unwrap();
    assert!(
        eventually(|| neuf
            .node
            .contacts()
            .map(|cs| cs.iter().any(|c| c.pubkey == ancien_pub))
            .unwrap_or(false))
        .await,
        "le pair basculé n'a pas reçu la demande d'ami du pair d'avant"
    );
    neuf.node.friend_respond(&ancien_pub, true).unwrap();
    assert!(
        eventually(|| ancien
            .node
            .contacts()
            .map(|cs| cs
                .iter()
                .any(|c| c.pubkey == neuf_pub && c.state == ContactState::Friend))
            .unwrap_or(false))
        .await,
        "l'amitié n'a pas été confirmée chez le pair d'avant"
    );

    // 🔒 Et la relation porte bien le nom d'une PERSONNE de part et d'autre.
    // C'est ce qui distingue « ça marche » de « ça marche en apparence » : un
    // contact enregistré sous une clé d'appareil échangerait des messages tout
    // aussi bien, jusqu'au jour où un second appareil apparaîtrait comme un
    // inconnu et où le code ami ne désignerait plus personne.
    assert!(
        neuf.node
            .contacts()
            .unwrap()
            .iter()
            .all(|c| c.pubkey != ancien.node.transport_key() || c.pubkey == ancien_pub),
        "le pair basculé a enregistré une machine au lieu d'une personne"
    );
    assert!(
        ancien
            .node
            .contacts()
            .unwrap()
            .iter()
            .all(|c| c.pubkey != neuf.node.transport_key()),
        "le pair d'avant a enregistré la machine du pair basculé au lieu de son compte"
    );

    // Messagerie dans les deux sens sur le lien établi.
    const DE_L_ANCIEN: &str = "je n'ai pas encore mis à jour";
    const DU_NEUF: &str = "moi si, et on se parle toujours";
    ancien.node.dm_send(&neuf_pub, DE_L_ANCIEN, None).unwrap();
    assert!(
        eventually(|| copies_recues(&neuf, &ancien_pub, DE_L_ANCIEN) == 1).await,
        "le pair basculé n'a pas reçu le message du pair d'avant (reçu {} fois)",
        copies_recues(&neuf, &ancien_pub, DE_L_ANCIEN)
    );
    neuf.node.dm_send(&ancien_pub, DU_NEUF, None).unwrap();
    assert!(
        eventually(|| copies_recues(&ancien, &neuf_pub, DU_NEUF) == 1).await,
        "le pair d'avant n'a pas reçu le message du pair basculé (reçu {} fois)",
        copies_recues(&ancien, &neuf_pub, DU_NEUF)
    );

    ancien.shutdown();
    neuf.shutdown();
}
