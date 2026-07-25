//! Test d'intégration bout-en-bout de l'**appairage** (jalon 1, lot 1.D) : une
//! machine vierge saisit un code, et ressort membre à part entière du compte —
//! capable d'en signer la liste d'appareils.
//!
//! `crates/accord-node/src/node/tests.rs` couvre déjà la machine à états et les
//! refus, sur des nœuds en mémoire. Ce fichier-ci prouve la moitié qui ne se
//! teste pas là : que la racine reçue **sert vraiment**, ce qui suppose de la
//! sceller dans un coffre sur disque, de rouvrir une base sous la clé qui en
//! dérive, et de refaire tourner un nœud complet dessus. Une graine transmise
//! peut être parfaitement juste au niveau du protocole et ne rien ouvrir du
//! tout.
//!
//! Quatre propriétés, dans l'ordre de ce qu'elles coûteraient si elles
//! tombaient :
//!
//! 1. l'appareil qui rejoint entre dans la liste signée du compte ;
//! 2. 🔒 la racine adoptée **est** celle du compte, et rien d'autre ;
//! 3. 🔒 adopter refuse d'écraser un profil habité — un appairage ne doit
//!    jamais faire perdre une identité dont la phrase de récupération est
//!    peut-être la seule copie ;
//! 4. la machine adoptée signe et publie une liste que le réseau accepte sous
//!    la clé du COMPTE. C'est le point du lot : sans lui, l'appareil appairé
//!    figure dans la liste sans pouvoir en signer une seule version.
//!
//! Aucun réseau ici : les `CoreMsg` passent d'un nœud à l'autre à la main, par
//! les mêmes points d'entrée que le routeur emprunte (`Node::ingest_core`,
//! canal `Outbound`). Ce que le transport ferait de plus — chiffrer, router,
//! retenter — est couvert ailleurs et n'apprendrait rien de plus ici.

use accord_core::Db;
use accord_node::outbound::{Outbound, OutboundSink};
use accord_node::{device, identity, Node, NodeError, Paths};
use accord_proto::core_msg::CoreMsg;
use tokio::sync::mpsc::Receiver;

/// Phrase de passe locale de tous les profils du fichier. Sans conséquence : ce
/// qui distingue les machines est leur coffre, pas le mot qui l'ouvre.
const PASSPHRASE: &str = "phrase-de-passe-appairage";

/// Difficulté de preuve de travail des identités de compte, en test.
///
/// ⚠️ Celle des **appareils** ne se règle pas ici : `device::ensure_local_device`
/// mine à la difficulté réseau, et c'est voulu — la liste publiée est vérifiée
/// par de vrais pairs, qui ne connaissent pas nos raccourcis de test.
const POW_TEST: u32 = 1;

/// Un nœud complet monté sur un profil scellé, avec son canal sortant capturé.
struct Machine {
    node: Node,
    sortant: Receiver<Outbound>,
    paths: Paths,
    /// Clé d'appareil de cette machine — distincte de sa clé de compte depuis
    /// le lot 1.B, et c'est elle que la liste du compte inscrit.
    appareil: [u8; 32],
    _dir: tempfile::TempDir,
}

impl Machine {
    /// Crée un profil neuf (coffre + base + identité d'appareil) et monte un
    /// nœud dessus, comme le fait le démarrage réel.
    fn neuve() -> Self {
        let dir = tempfile::tempdir().expect("répertoire temporaire");
        let paths = Paths::new(dir.path());
        let unlocked = identity::create(&paths, PASSPHRASE, POW_TEST).expect("profil scellé");
        let db = Db::open(&paths.db(), &unlocked.db_key).expect("base ouverte");
        let appareil = device::ensure_local_device(&db).expect("appareil local");
        let (sink, sortant) = OutboundSink::channel(64);
        Self {
            node: Node::new(unlocked.identity, db, sink),
            sortant,
            paths,
            appareil: appareil.public_key(),
            _dir: dir,
        }
    }

    /// Clé sous laquelle les pairs voient cette machine.
    fn cle(&self) -> [u8; 32] {
        self.node.transport_key()
    }

    /// Vide le canal sortant et rend les `CoreMsg` adressés à `dest`.
    fn sortants_vers(&mut self, dest: &[u8; 32]) -> Vec<CoreMsg> {
        let mut out = Vec::new();
        while let Ok(action) = self.sortant.try_recv() {
            if let Outbound::Core { to, msg } = action {
                if to == *dest {
                    out.push(*msg);
                }
            }
        }
        out
    }
}

/// Joue l'appairage complet entre `autorise` et `rejoint`, confirmations
/// humaines comprises, et rend la clé de compte partagée.
///
/// Chaque message emprunte le point d'entrée que le routeur utilise, avec la
/// clé de MACHINE de l'émetteur : à ce stade les deux appareils ne se
/// connaissent pas, il n'y a ni amitié ni liste pour les rattacher à un compte.
fn appairer(autorise: &mut Machine, rejoint: &mut Machine) {
    let cle_autorise = autorise.cle();
    let cle_rejoint = rejoint.cle();

    let code = autorise.node.pairing_start().expect("offre ouverte").code;
    let hello = rejoint.node.pairing_submit(&code).expect("code accepté");

    // L'appareil autorisé répond ; le nouvel appareil, lui, ne répond pas à la
    // réponse (sans quoi les deux s'épuiseraient en trois allers-retours).
    let reponses = autorise
        .node
        .ingest_core(&cle_rejoint, CoreMsg::PairingHello { msg: hello })
        .expect("HELLO ingéré");
    for r in reponses {
        assert!(
            rejoint
                .node
                .ingest_core(&cle_autorise, r)
                .expect("réponse ingérée")
                .is_empty(),
            "le côté qui rejoint ne doit pas relancer l'échange"
        );
    }

    let empreinte = rejoint
        .node
        .pairing_fingerprint()
        .expect("empreinte côté nouveau");
    assert_eq!(
        autorise.node.pairing_fingerprint().as_deref(),
        Some(empreinte.as_str()),
        "les deux écrans doivent afficher le même nombre"
    );

    // Confirmation humaine côté nouveau : il propose son entrée d'appareil.
    rejoint
        .node
        .pairing_confirm()
        .expect("confirmation acceptée");
    for m in rejoint.sortants_vers(&cle_autorise) {
        autorise
            .node
            .ingest_core(&cle_rejoint, m)
            .expect("entrée ingérée");
    }

    // Confirmation humaine côté autorisé : inscription, publication, puis la
    // racine du compte.
    autorise
        .node
        .pairing_confirm()
        .expect("confirmation acceptée");
    for m in autorise.sortants_vers(&cle_rejoint) {
        rejoint
            .node
            .ingest_core(&cle_autorise, m)
            .expect("racine ingérée");
    }
}

#[test]
fn un_appareil_appaire_adopte_le_compte_et_peut_en_signer_la_liste() {
    let mut autorise = Machine::neuve();
    let mut rejoint = Machine::neuve();
    let compte = autorise.node.public_key();
    let appareil_rejoint = rejoint.appareil;
    assert_ne!(
        rejoint.node.public_key(),
        compte,
        "avant l'appairage, la machine qui rejoint a son propre compte"
    );

    appairer(&mut autorise, &mut rejoint);

    // 1. L'appareil figure dans la liste signée du compte.
    let publiee = autorise
        .node
        .device_list_record()
        .expect("liste publiable après appairage");
    let liste = device::verify_device_list_record(&compte, &publiee, 0)
        .expect("le réseau doit accepter la liste du compte");
    assert!(
        liste.authorises(&appareil_rejoint),
        "l'appareil appairé doit figurer dans la liste du compte"
    );

    // 🔒 Avant adoption, cette machine ne peut RIEN signer au nom du compte :
    // c'est exactement le manque que le lot comble.
    if let Some(avant) = rejoint.node.device_list_record() {
        assert!(
            device::verify_device_list_record(&compte, &avant, 0).is_err(),
            "sans la racine, la machine qui rejoint ne signe pas pour le compte"
        );
    }

    // 2. Ce que la machine emporte : la racine ET la clé d'appareil que
    //    l'appareil autorisé vient d'inscrire.
    let adoption = rejoint
        .node
        .pairing_take_adoption()
        .expect("compte à adopter");
    assert!(
        !rejoint.node.pairing_adopted_ready(),
        "la racine ne se reprend qu'une fois"
    );

    // 3. 🔒 Le profil de la machine qui rejoint a déjà un coffre : refuser,
    //    plutôt qu'écraser une identité dont la phrase de récupération est
    //    peut-être la seule copie au monde.
    assert!(
        matches!(
            identity::adopt_account_seed(&rejoint.paths, &adoption, PASSPHRASE, POW_TEST),
            Err(NodeError::AlreadyExists)
        ),
        "adopter ne doit jamais écraser un profil habité"
    );

    // Adoption sur un profil vierge — ce que fait l'hôte : le nœud d'appairage
    // tourne sans coffre, et c'est ici que le compte s'installe.
    let vierge = tempfile::tempdir().expect("répertoire temporaire");
    let paths = Paths::new(vierge.path());
    let adopte =
        identity::adopt_account_seed(&paths, &adoption, PASSPHRASE, POW_TEST).expect("adoption");
    assert_eq!(
        adopte.identity.public_key(),
        compte,
        "la racine adoptée EST celle du compte"
    );

    // 4. La machine adoptée signe et publie une liste que le réseau accepte
    //    sous la clé du COMPTE — et elle s'y inscrit sous la clé d'appareil
    //    qui avait été enrôlée, pas sous une clé neuve.
    let db = Db::open(&paths.db(), &adopte.db_key).expect("base ouverte sous la clé adoptée");
    assert_eq!(
        device::ensure_local_device(&db)
            .expect("appareil local")
            .public_key(),
        appareil_rejoint,
        "la clé d'appareil inscrite doit survivre à l'adoption"
    );
    let node = Node::new(adopte.identity, db, OutboundSink::null());
    let record = node
        .device_list_record()
        .expect("la machine adoptée doit pouvoir publier");
    let signee = device::verify_device_list_record(&compte, &record, 0)
        .expect("le réseau doit accepter une liste signée par la machine adoptée");
    assert_eq!(record.publisher, compte);
    assert_eq!(signee.account, compte);
    assert!(
        signee.authorises(&appareil_rejoint),
        "la machine adoptée doit se lister elle-même"
    );
}

#[test]
fn le_coffre_adopte_se_rouvre_avec_la_phrase_de_passe_locale() {
    // ⚠️ La clé de base dérive de la GRAINE, pas de la phrase de passe : une
    // base laissée en place par le nœud d'appairage ne s'ouvrirait plus jamais.
    // `adopt_account_seed` la retire ; sans cela le démarrage suivant échouerait
    // sur une erreur de déchiffrement que rien à l'écran n'expliquerait.
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let paths = Paths::new(dir.path());

    // Un nœud d'appairage a tourné ici : une base existe, chiffrée sous une
    // clé qui n'est pas celle du compte adopté.
    let brouillon = Db::open(&paths.db(), &[0xEE; 32]).expect("base de brouillon");
    let appareil = device::ensure_local_device(&brouillon).expect("appareil local");
    let adoption = accord_node::pairing::AccountAdoption::new(
        accord_crypto::pairing::AccountSeed::new([0x31; 32]),
        accord_core::db::LocalDevice {
            seed: *appareil.seed(),
            pow_nonce: appareil.pow_nonce(),
            name: "Portable".into(),
        },
    );
    drop(brouillon);
    assert!(paths.db().exists());

    let adopte =
        identity::adopt_account_seed(&paths, &adoption, PASSPHRASE, POW_TEST).expect("adoption");
    Db::open(&paths.db(), &adopte.db_key).expect("la base doit s'ouvrir sous la clé adoptée");

    // Et le coffre se rouvre au démarrage suivant, sur la même identité.
    let rouvert = identity::unlock(&paths, PASSPHRASE).expect("coffre rouvert");
    assert_eq!(
        rouvert.identity.public_key(),
        adopte.identity.public_key(),
        "le coffre adopté doit rendre la même identité"
    );
    assert_eq!(*rouvert.db_key, *adopte.db_key);

    // Et la clé d'appareil enrôlée est bien celle qui repart, pas une neuve.
    let db = Db::open(&paths.db(), &rouvert.db_key).expect("base rouverte");
    assert_eq!(
        device::ensure_local_device(&db)
            .expect("appareil local")
            .public_key(),
        appareil.public_key()
    );
}
