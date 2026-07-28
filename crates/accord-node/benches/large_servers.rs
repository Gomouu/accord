//! Bancs criterion des serveurs à grande échelle (ROADMAP §18.3, jalon 6).
//!
//! ⚠️ Ce jalon **commence par des mesures**, pas par du code : rien ici
//! n'optimise quoi que ce soit. Le banc répond à trois questions chiffrées —
//! combien coûte une jonction, combien pèse l'état matérialisé, combien coûte
//! une diffusion — à 50, 200 et 500 membres.
//!
//! Même standard que `benches/history.rs` : base **chiffrée sur disque**
//! (SQLCipher, le `Db::open` de l'application) et mesure des **méthodes
//! JSON-RPC que l'interface appelle vraiment**, pas de requêtes SQL isolées.
//!
//! Là où le chemin réel passe par le réseau, le banc mesure le point d'entrée
//! du nœud plutôt qu'une abstraction :
//!
//! - **rejoindre** = l'op-log poussé op par op par le pair qui invite, chaque
//!   op traversant [`Node::ingest_core`] comme le fait le routeur du runtime ;
//! - **diffuser** = l'expansion exacte de `Runtime::dispatch_outbound` pour un
//!   `Outbound::GroupCast` : état du groupe, puis, membre par membre, cibles de
//!   livraison et mise en file hors-ligne (le store-and-forward persiste
//!   TOUJOURS un `GroupMsg`, joignable ou non — `Runtime::deliver_core_to_device`).
//!
//! Ce qui suit la socket — scellement de session, datagramme UDP, retransmission —
//! n'est pas mesurable sans réseau et ne l'est donc pas ici : voir
//! `docs/PERFORMANCE.md` §3.5.
//!
//! Exécution : `cargo bench -p accord-node --bench large_servers`.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use accord_api::Service;
use accord_core::db::{Db, LocalMembership};
use accord_core::group::{self, GroupState};
use accord_crypto::Identity;
use accord_node::hex;
use accord_node::node::Node;
use accord_node::outbound::{Outbound, OutboundSink};
use accord_node::service::NodeService;
use accord_proto::core_msg::{perms, ChannelKind, CoreMsg, GroupOp, GroupOpBody};
use accord_proto::WireEncode;
use criterion::{
    black_box, criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput,
};
use serde_json::{json, Value};
use tokio::runtime::Runtime;
use tokio::sync::mpsc::Receiver;

/// Paliers du jalon : « temps de rejoindre, mémoire, à 50 / 200 / 500 membres »
/// (ROADMAP §18.3, tableau de contenu).
const PALIERS: [usize; 3] = [50, 200, 500];

/// Difficulté de preuve de travail des identités du banc : nulle, la PoW
/// réseau coûterait des secondes sans rien mesurer d'utile ici.
const POW_BANC: u32 = 0;

/// Clé de chiffrement de la base du banc (SQLCipher, clé brute).
const CLE_BASE: [u8; 32] = [7u8; 32];

/// Graine de l'identité du fondateur — fixe, pour que deux exécutions
/// produisent le même op-log.
const GRAINE_FONDATEUR: [u8; 32] = [11u8; 32];

/// Graine de l'identité de celui qui rejoint.
const GRAINE_JOIGNANT: [u8; 32] = [13u8; 32];

/// Ancrage temporel des ops (ms) : fixe, pour un corpus reproductible.
const INSTANT_BANC: u64 = 1_700_000_000_000;

/// Salons du serveur type. Un serveur d'amis en a une poignée ; un serveur de
/// 500 membres en a une douzaine, réparties en catégories.
const SALONS: usize = 12;

/// Catégories du serveur type.
const CATEGORIES: usize = 4;

/// Rôles du serveur type.
const ROLES: usize = 6;

/// Une invitation pour dix membres : un serveur ne crée pas un lien par
/// personne, il en réutilise quelques-uns.
const MEMBRES_PAR_INVITATION: usize = 10;

// ---------------------------------------------------------------------------
// Allocateur compteur — la mémoire de l'état matérialisé
// ---------------------------------------------------------------------------

/// Allocateur global qui compte les octets **vivants** pendant une fenêtre de
/// mesure. Sans lui, « la mémoire tenue par l'état matérialisé » resterait une
/// estimation à la main de la taille des `BTreeMap` ; ici c'est le tas qui
/// répond.
///
/// ⚠️ Le comptage est **éteint par défaut** et n'est allumé que dans
/// [`memoire_de`] : pendant les mesures de temps, chaque allocation ne paie
/// qu'un chargement atomique relâché, pas un incrément. C'est ce qui permet à
/// ce banc de mesurer du temps ET de la mémoire dans le même binaire.
struct Compteur;

/// Comptage actif ? (voir [`Compteur`]).
static COMPTE: AtomicBool = AtomicBool::new(false);

/// Octets vivants alloués depuis l'allumage du compteur.
static VIVANTS: AtomicI64 = AtomicI64::new(0);

// SAFETY: chaque appel délègue à `System` sans modifier le pointeur ni la
// disposition ; le comptage n'ajoute qu'un entier atomique.
unsafe impl GlobalAlloc for Compteur {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if COMPTE.load(Ordering::Relaxed) {
            VIVANTS.fetch_add(layout.size() as i64, Ordering::Relaxed);
        }
        System.alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if COMPTE.load(Ordering::Relaxed) {
            VIVANTS.fetch_sub(layout.size() as i64, Ordering::Relaxed);
        }
        System.dealloc(ptr, layout)
    }
}

#[global_allocator]
static ALLOCATEUR: Compteur = Compteur;

/// Exécute `f` compteur allumé et rend son résultat avec les octets **encore
/// vivants** à la sortie — c'est-à-dire ce que la valeur rendue retient, les
/// tampons temporaires libérés en chemin s'annulant d'eux-mêmes.
fn memoire_de<T>(f: impl FnOnce() -> T) -> (T, i64) {
    COMPTE.store(true, Ordering::SeqCst);
    let avant = VIVANTS.load(Ordering::SeqCst);
    let valeur = f();
    let apres = VIVANTS.load(Ordering::SeqCst);
    COMPTE.store(false, Ordering::SeqCst);
    (valeur, apres - avant)
}

// ---------------------------------------------------------------------------
// Corpus
// ---------------------------------------------------------------------------

/// Générateur déterministe (xorshift) : même corpus d'une exécution à l'autre.
fn melange(mut x: u64) -> u64 {
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    x
}

/// Identifiant de 16 octets dérivé de `graine` (salons, catégories, rôles).
fn id16(graine: u64) -> [u8; 16] {
    let mut id = [0u8; 16];
    id[..8].copy_from_slice(&melange(graine.wrapping_mul(0xD6E8_FEB8_6659_FD93)).to_be_bytes());
    id[8..].copy_from_slice(&melange(graine ^ 0x9E37_79B9_7F4A_7C15).to_be_bytes());
    id
}

/// Clé publique du membre `i`. Des octets dispersés, pas une vraie identité :
/// aucun chemin mesuré ici ne vérifie une signature de MEMBRE (seules les ops
/// du fondateur sont signées, et elles le sont pour de bon), et dériver 500
/// identités Ed25519 ne mesurerait que la génération de clés.
fn cle_membre(i: usize) -> [u8; 32] {
    let mut cle = [0u8; 32];
    for (bloc, mot) in cle.chunks_exact_mut(8).enumerate() {
        mot.copy_from_slice(
            &melange((i as u64 + 1).wrapping_mul(0x100_0000_01B3) ^ bloc as u64).to_be_bytes(),
        );
    }
    cle
}

/// Horloge murale, pour les chemins qui datent ce qu'ils écrivent.
fn maintenant_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(INSTANT_BANC)
}

/// Un serveur peuplé : base chiffrée sur disque, op-log complet, service API.
struct Serveur {
    /// Gardé vivant pour la durée du banc : sa destruction efface la base.
    _dir: tempfile::TempDir,
    /// Chemin de la base, pour rouvrir une connexion à cache froid.
    chemin: PathBuf,
    service: NodeService,
    node: Arc<Node>,
    rt: Runtime,
    /// File des actions réseau du nœud : c'est là qu'atterrit le `GroupCast`
    /// d'un envoi, que le banc dépile pour le diffuser lui-même.
    sortie: RefCell<Receiver<Outbound>>,
    fondateur: [u8; 32],
    group_id: [u8; 16],
    /// Premier salon textuel — celui où le banc envoie.
    salon: [u8; 16],
    /// Op-log complet dans l'ordre d'émission (qui est l'ordre canonique : un
    /// seul auteur, lamports croissants).
    ops: Vec<GroupOp>,
    membres: usize,
}

impl Serveur {
    /// Appelle une méthode JSON-RPC comme le fait l'interface.
    fn appel(&self, method: &str, params: Value) -> Value {
        self.rt
            .block_on(self.service.call(method, params))
            .unwrap_or_else(|e| panic!("{method} a échoué : {e:?}"))
    }

    /// `groups.state` tel que l'interface l'appelle à l'ouverture d'un serveur
    /// (`app/src/lib/api.ts`, `groupState`) : le `group_id` seul.
    fn etat(&self) -> Value {
        self.appel(
            "groups.state",
            json!({ "group_id": hex::encode(&self.group_id) }),
        )
    }

    /// Vide la file hors-ligne. Appelée entre deux itérations mesurées, hors
    /// chronomètre : chez l'utilisateur, les accusés applicatifs soldent ces
    /// lignes au fil de l'eau, si bien que la file d'un serveur qui tourne
    /// vaut un message en cours, pas des milliers.
    fn purger_file(&self) {
        // Trente jours dans le futur : tout ce qui est en file a dépassé sa
        // rétention de sept jours du point de vue de cet instant.
        let horizon = maintenant_ms() + 30 * 86_400_000;
        self.node.outbox_purge_expired(horizon).expect("purge");
    }
}

/// Compose l'op-log d'un serveur de `membres` membres et rend le tout monté :
/// base sur disque, nœud, service API.
///
/// Les ops sont écrites par le chemin de production ([`group::author_op`], qui
/// valide l'op sur l'état courant avant de la signer et de la persister), pas
/// insérées à la main : un op-log que le moteur n'aurait pas accepté ne
/// mesurerait rien.
fn peupler(membres: usize) -> Serveur {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let chemin = dir.path().join("core.db");
    let identity = Identity::from_seed_with_pow_bits(GRAINE_FONDATEUR, POW_BANC);
    let fondateur = identity.public_key();
    let db = Db::open(&chemin, &CLE_BASE).expect("base");

    let cree =
        group::create_group(&db, &identity, "Serveur du banc", INSTANT_BANC).expect("CREATE");
    let group_id = cree.group_id;
    db.set_group_membership(&group_id, LocalMembership::Joined)
        .expect("appartenance");

    let mut horloge = INSTANT_BANC;
    let mut ecrire = |db: &Db, body: GroupOpBody| {
        horloge += 1_000;
        group::author_op(db, &identity, &group_id, &body, horloge)
            .expect("op refusée par le moteur")
    };

    // Un nom changé une fois : le SetMeta d'un serveur qui a vécu.
    ecrire(
        &db,
        GroupOpBody::SetMeta {
            name: "Serveur du banc".into(),
            icon: None,
            banner_color: Some(0x0058_65F2),
        },
    );

    let categories: Vec<[u8; 16]> = (0..CATEGORIES).map(|i| id16(0x0C00 + i as u64)).collect();
    for (i, cat) in categories.iter().enumerate() {
        ecrire(
            &db,
            GroupOpBody::AddCategory {
                category_id: *cat,
                name: format!("Catégorie {i}"),
                position: i as u16,
            },
        );
    }

    let salons: Vec<[u8; 16]> = (0..SALONS).map(|i| id16(0x5A00 + i as u64)).collect();
    for (i, salon) in salons.iter().enumerate() {
        ecrire(
            &db,
            GroupOpBody::AddChannel {
                channel_id: *salon,
                name: format!("salon-{i}"),
                category: Some(categories[i % CATEGORIES]),
                // Deux salons vocaux, le reste en textuel.
                kind: if i % 6 == 5 {
                    ChannelKind::Voice
                } else {
                    ChannelKind::Text
                },
                position: i as u16,
            },
        );
        ecrire(
            &db,
            GroupOpBody::SetTopic {
                channel_id: *salon,
                topic: format!("Sujet du salon {i}"),
            },
        );
    }

    let roles: Vec<[u8; 16]> = (0..ROLES).map(|i| id16(0x0B00 + i as u64)).collect();
    for (i, role) in roles.iter().enumerate() {
        ecrire(
            &db,
            GroupOpBody::AddRole {
                role_id: *role,
                name: format!("Rôle {i}"),
                color: 0x0000_FF00 + i as u32,
                position: i as u16,
                permissions: perms::VIEW | perms::SEND | perms::INVITE,
            },
        );
        // Un override par rôle sur le premier salon : la configuration fine
        // d'un serveur qui a des règles.
        ecrire(
            &db,
            GroupOpBody::SetChannelPerms {
                channel_id: salons[0],
                role_id: *role,
                allow: perms::VIEW,
                deny: 0,
            },
        );
    }

    let invitations = (membres / MEMBRES_PAR_INVITATION).max(1);
    for i in 0..invitations {
        ecrire(
            &db,
            GroupOpBody::InviteCreate {
                invite_id: id16(0x1_0000 + i as u64),
                code_hash: [0x33u8; 32],
                max_uses: 0,
                expires_ms: 0,
            },
        );
    }

    // Les membres : une admission et une attribution de rôle chacun — c'est ce
    // que produit une invitation rachetée, et c'est le gros de l'op-log.
    let joignant = Identity::from_seed_with_pow_bits(GRAINE_JOIGNANT, POW_BANC).public_key();
    for i in 0..membres {
        // Le dernier membre est celui qui rejoindra : le banc mesure une
        // jonction de membre, pas d'inconnu.
        let membre = if i + 1 == membres {
            joignant
        } else {
            cle_membre(i)
        };
        ecrire(
            &db,
            GroupOpBody::AddMember {
                member: membre,
                invite_id: None,
            },
        );
        ecrire(
            &db,
            GroupOpBody::AssignRole {
                member: membre,
                role_id: roles[i % ROLES],
            },
        );
    }

    let ops = db.group_ops(&group_id).expect("op-log");
    let (sink, sortie) = OutboundSink::channel(64);
    let node = Arc::new(Node::new(identity, db, sink));
    Serveur {
        _dir: dir,
        chemin,
        service: NodeService::new(node.clone()),
        node,
        rt: Runtime::new().expect("runtime tokio"),
        sortie: RefCell::new(sortie),
        fondateur,
        group_id,
        salon: salons[0],
        ops,
        membres,
    }
}

/// Service API branché sur une **connexion neuve** à la même base : le cache
/// d'état de groupe vit dans l'instance de `Db`, donc une base rouverte replie
/// l'op-log au premier `groups.state`. C'est l'état de l'application au
/// démarrage — et, à un `Db` près, celui d'après chaque op reçue, qui invalide
/// le cache.
fn service_froid(chemin: &Path) -> NodeService {
    let db = Db::open(chemin, &CLE_BASE).expect("base");
    let identity = Identity::from_seed_with_pow_bits(GRAINE_FONDATEUR, POW_BANC);
    NodeService::new(Arc::new(Node::new(identity, db, OutboundSink::null())))
}

// ---------------------------------------------------------------------------
// Rejoindre
// ---------------------------------------------------------------------------

/// Le nœud de celui qui rejoint : base vierge, invitation acceptée localement
/// (la porte de consentement de D-045), aucun op encore reçu.
struct Joignant {
    _dir: tempfile::TempDir,
    chemin: PathBuf,
    node: Arc<Node>,
}

/// Monte un joignant vierge pour `group_id`.
fn joignant_vierge(group_id: &[u8; 16]) -> Joignant {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let chemin = dir.path().join("core.db");
    let db = Db::open(&chemin, &CLE_BASE).expect("base");
    // Sans cette marque, tout op-log poussé serait ignoré en silence.
    db.set_group_membership(group_id, LocalMembership::Accepted)
        .expect("appartenance");
    let identity = Identity::from_seed_with_pow_bits(GRAINE_JOIGNANT, POW_BANC);
    Joignant {
        _dir: dir,
        chemin,
        node: Arc::new(Node::new(identity, db, OutboundSink::null())),
    }
}

/// Rejoint : l'op-log arrive op par op depuis le pair qui invite, exactement
/// comme le routeur du runtime les remet (`CoreMsg::GroupOpMsg`). Rend le
/// nombre de membres de l'état obtenu — le contrôle que l'état est utilisable.
fn rejoindre(
    joignant: &Joignant,
    fondateur: &[u8; 32],
    group_id: &[u8; 16],
    ops: &[GroupOp],
) -> usize {
    for op in ops {
        joignant
            .node
            .ingest_core(fondateur, CoreMsg::GroupOpMsg { op: op.clone() })
            .expect("ingestion d'op");
    }
    joignant
        .node
        .group_state(group_id)
        .expect("état après jonction")
        .members
        .len()
}

// ---------------------------------------------------------------------------
// Diffusion en étoile
// ---------------------------------------------------------------------------

/// Diffuse `msg` à tous les membres : la boucle de
/// `Runtime::dispatch_outbound` pour un `Outbound::GroupCast`, moins la
/// socket. Rend le nombre de remises effectuées.
fn diffuser(serveur: &Serveur, msg: &CoreMsg) -> usize {
    let etat = serveur.node.group_state(&serveur.group_id).expect("état");
    let moi = serveur.node.public_key();
    let ma_cle = serveur.node.transport_key();
    let mut remises = 0;
    for membre in etat.members.keys() {
        if *membre == moi {
            continue;
        }
        // Un compte se résout en une cible par appareil (lot 1.E) : une
        // requête en base par membre, puis une mise en file par cible.
        let cibles =
            accord_node::device::without_self(serveur.node.delivery_targets(membre), &ma_cle);
        for cible in cibles {
            serveur.node.outbox_enqueue(&cible, msg).expect("file");
            remises += 1;
        }
    }
    remises
}

/// Envoie un message par `groups.send` puis le diffuse : la chaîne complète
/// d'un message de salon, du JSON-RPC de l'interface à la dernière mise en
/// file.
fn envoyer_et_diffuser(serveur: &Serveur) -> usize {
    serveur.appel(
        "groups.send",
        json!({
            "group_id": hex::encode(&serveur.group_id),
            "channel_id": hex::encode(&serveur.salon),
            "text": "Message du banc de mesure",
        }),
    );
    let action = serveur
        .sortie
        .borrow_mut()
        .try_recv()
        .expect("diffusion attendue en file");
    let Outbound::GroupCast { msg, .. } = action else {
        panic!("GroupCast attendu");
    };
    diffuser(serveur, &msg)
}

// ---------------------------------------------------------------------------
// Mesures hors chronomètre
// ---------------------------------------------------------------------------

/// Taille du fichier `chemin` sur disque.
fn octets_fichier(chemin: &Path) -> u64 {
    std::fs::metadata(chemin).map(|m| m.len()).unwrap_or(0)
}

/// Taille d'une base **fermée** : en WAL, une base encore ouverte laisse
/// l'essentiel de ce qui vient d'être écrit dans son journal, et le fichier
/// principal ne mesure alors que la position du dernier point de contrôle. La
/// fermeture replie le journal — c'est ce chiffre-là qui est la taille de
/// l'op-log sur le disque de l'utilisateur.
fn octets_base_fermee(joignant: Joignant) -> u64 {
    let Joignant { _dir, chemin, node } = joignant;
    drop(node);
    let octets = octets_fichier(&chemin);
    drop(_dir);
    octets
}

/// Taille d'une base neuve : le schéma seul, sans un seul op. À retrancher de
/// [`octets_base_fermee`] pour lire ce que pèse VRAIMENT l'op-log.
fn octets_base_vide() -> u64 {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let chemin = dir.path().join("core.db");
    drop(Db::open(&chemin, &CLE_BASE).expect("base"));
    octets_fichier(&chemin)
}

/// Tout ce qui se mesure une fois, sans chronomètre : taille de l'op-log,
/// mémoire de l'état matérialisé, poids du JSON rendu à l'interface, et taille
/// sur disque de la base d'un membre qui vient de rejoindre.
fn mesures_statiques(serveur: &Serveur) {
    let ops = serveur.ops.len();
    let filaire: usize = serveur.ops.iter().map(|op| op.to_bytes().len()).sum();

    // Mémoire tenue par l'état matérialisé : le repli complet de l'op-log,
    // compteur allumé. Les ops sont déjà en main, donc seul ce que l'état
    // retient est compté.
    let (etat, octets_etat) = memoire_de(|| GroupState::fold(&serveur.ops));
    assert_eq!(
        etat.members.len(),
        serveur.membres + 1,
        "membres attendus (fondateur compris)"
    );
    assert_eq!(etat.channels.len(), SALONS, "salons attendus");

    let json = serde_json::to_vec(&serveur.etat()).expect("JSON");

    // Ce qu'un seul message écrit sur le disque de l'ÉMETTEUR : la file
    // hors-ligne est indexée par destinataire et garde une copie entière du
    // `CoreMsg` encodé pour chacun — le corps n'est chiffré qu'une fois (clé
    // d'epoch du groupe), mais il est recopié autant de fois qu'il y a de
    // membres.
    serveur.appel(
        "groups.send",
        json!({
            "group_id": hex::encode(&serveur.group_id),
            "channel_id": hex::encode(&serveur.salon),
            "text": "Message du banc de mesure",
        }),
    );
    let action = serveur
        .sortie
        .borrow_mut()
        .try_recv()
        .expect("diffusion attendue en file");
    let Outbound::GroupCast { msg, .. } = action else {
        panic!("GroupCast attendu");
    };
    let charge = accord_node::maintenance::encode_core(&msg).len();

    // Base d'un membre qui vient de rejoindre : elle ne contient que cet
    // op-log et son appartenance locale.
    let joignant = joignant_vierge(&serveur.group_id);
    let membres = rejoindre(
        &joignant,
        &serveur.fondateur,
        &serveur.group_id,
        &serveur.ops,
    );
    assert_eq!(
        membres,
        serveur.membres + 1,
        "état incomplet après jonction"
    );
    let base = octets_base_fermee(joignant);
    let vide = octets_base_vide();

    let kio = |o: f64| o / 1024.0;
    println!(
        "\n-- serveur {} membres : {ops} ops rejouées, {:.1} Kio filaires \
         ({:.0} o/op) | état matérialisé {:.1} Kio ({:.0} o/membre) | \
         groups.state {:.1} Kio de JSON | base du joignant {:.1} Kio \
         (dont {:.1} Kio de schéma vide) | un message en file : {charge} o \
         x {} destinataires = {:.1} Kio --",
        serveur.membres,
        kio(filaire as f64),
        filaire as f64 / ops as f64,
        kio(octets_etat as f64),
        octets_etat as f64 / (serveur.membres + 1) as f64,
        kio(json.len() as f64),
        kio(base as f64),
        kio(vide as f64),
        serveur.membres,
        kio((charge * serveur.membres) as f64),
    );
}

// ---------------------------------------------------------------------------
// Bancs
// ---------------------------------------------------------------------------

/// Un seul point d'entrée, et un seul corpus par palier : peupler un serveur
/// de 500 membres écrit un millier d'ops signées et validées une à une, ce qui
/// se paie en secondes — le refaire pour chaque groupe de mesure triplerait le
/// temps d'exécution sans rien changer aux chiffres.
#[path = "large_servers/mesures.rs"]
mod mesures;
use mesures::{mesure_cinq_serveurs, mesure_journal_long};

fn bench_grande_echelle(c: &mut Criterion) {
    mesure_cinq_serveurs();
    mesure_journal_long();

    for membres in PALIERS {
        let serveur = peupler(membres);
        mesures_statiques(&serveur);

        // 1. Rejoindre : l'op-log rejoué jusqu'à un état utilisable. Base
        //    neuve à chaque itération (rejouer deux fois le même log ne
        //    mesurerait plus que la déduplication par `op_id`).
        let mut groupe = c.benchmark_group("rejoindre_serveur");
        groupe.sample_size(10);
        groupe.throughput(Throughput::Elements(serveur.ops.len() as u64));
        groupe.bench_with_input(BenchmarkId::from_parameter(membres), &membres, |b, _| {
            b.iter_batched(
                || joignant_vierge(&serveur.group_id),
                |joignant| {
                    black_box(rejoindre(
                        &joignant,
                        &serveur.fondateur,
                        &serveur.group_id,
                        &serveur.ops,
                    ))
                },
                BatchSize::PerIteration,
            );
        });
        groupe.finish();

        // 2. Décomposition de la jonction. Sans ces deux repères, le chiffre
        //    ci-dessus ne dit pas OÙ il passe — et « où est le goulot exact »
        //    est la question même du jalon (ROADMAP §18.3).
        //
        //    - `repli_unique` : replier l'op-log UNE fois, ce que coûterait le
        //      rattrapage si l'état n'était matérialisé qu'à la fin ;
        //    - `verification_signatures` : les N vérifications Ed25519, coût
        //      irréductible d'un log signé (aucun instantané ne les supprime
        //      pour les ops qui restent).
        //
        //    Ce qui manque à l'appel entre leur somme et la jonction complète
        //    est ce que coûte la re-matérialisation à CHAQUE op.
        let mut groupe = c.benchmark_group("jonction_decomposition");
        groupe.sample_size(10);
        groupe.bench_with_input(
            BenchmarkId::new("repli_unique", membres),
            &membres,
            |b, _| {
                b.iter(|| black_box(GroupState::fold(&serveur.ops)));
            },
        );
        groupe.bench_with_input(
            BenchmarkId::new("verification_signatures", membres),
            &membres,
            |b, _| {
                b.iter(|| {
                    for op in &serveur.ops {
                        black_box(
                            accord_crypto::verify_signature(
                                &op.author,
                                &op.signable_bytes(),
                                &op.sig,
                            )
                            .is_ok(),
                        );
                    }
                });
            },
        );
        groupe.finish();

        // 3. `groups.state` : ce que l'interface appelle à l'ouverture d'un
        //    serveur. Deux cas, parce qu'ils n'ont rien à voir — le cache
        //    d'état rend le second appel gratuit, mais toute op reçue le vide.
        let mut groupe = c.benchmark_group("groups_state");
        groupe.bench_with_input(
            BenchmarkId::new("cache_chaud", membres),
            &membres,
            |b, _| {
                b.iter(|| black_box(serveur.etat()));
            },
        );
        groupe.bench_with_input(
            BenchmarkId::new("cache_froid", membres),
            &membres,
            |b, _| {
                b.iter_batched(
                    || service_froid(&serveur.chemin),
                    |service| {
                        black_box(
                            serveur
                                .rt
                                .block_on(service.call(
                                    "groups.state",
                                    json!({ "group_id": hex::encode(&serveur.group_id) }),
                                ))
                                .expect("groups.state"),
                        )
                    },
                    BatchSize::PerIteration,
                );
            },
        );
        groupe.finish();

        // 4. Diffusion en étoile : un message envoyé, puis remis membre par
        //    membre. `iter_custom` — et non `iter` — pour tenir le chronomètre
        //    soi-même et vider la file hors-ligne ENTRE deux itérations sans
        //    la compter.
        let mut groupe = c.benchmark_group("diffusion_etoile");
        groupe.sample_size(10);
        groupe.throughput(Throughput::Elements(membres as u64));
        groupe.bench_with_input(BenchmarkId::from_parameter(membres), &membres, |b, _| {
            b.iter_custom(|iterations| {
                let mut cumul = Duration::ZERO;
                for _ in 0..iterations {
                    let debut = Instant::now();
                    black_box(envoyer_et_diffuser(&serveur));
                    cumul += debut.elapsed();
                    // Hors chronomètre, entre deux itérations : la file
                    // retrouve sa taille de régime (les accusés la vident au
                    // fil de l'eau chez l'utilisateur). Sans cela, chaque
                    // itération laisserait M lignes derrière elle et l'on
                    // finirait par mesurer l'insertion dans une table de
                    // cent mille lignes — ce que personne n'a jamais.
                    serveur.purger_file();
                }
                cumul
            });
        });
        groupe.finish();
    }
}

criterion_group!(benches, bench_grande_echelle);
criterion_main!(benches);
