//! Bancs criterion des budgets « historiques volumineux » (ROADMAP §9.2, §10.2).
//!
//! Les budgets de la feuille de route portaient la mention « à instrumenter » :
//! ce banc mesure le **chemin réel** que suit l'interface à l'ouverture d'une
//! conversation, c'est-à-dire les méthodes JSON-RPC que `DmView` appelle au
//! montage (`dm.history`, `dm.pins`, puis `dm.mark_read` et `friends.list` via
//! le marquage lu), et non une requête SQL isolée. La sérialisation JSON rendue
//! à l'interface est donc comprise dans la mesure.
//!
//! Base sur DISQUE et chiffrée (SQLCipher), comme chez l'utilisateur : une base
//! en mémoire mesurerait un coût de stockage qui n'existe pas. Le banc imprime
//! aussi la taille du fichier et le poids de l'index de recherche, l'autre
//! moitié du budget §9.2 (« taille de la base, coût de la recherche »).
//!
//! Exécution : `cargo bench -p accord-node --bench history`.

use std::sync::Arc;

use accord_api::Service;
use accord_core::db::{Contact, ContactState, Db, DmRecord};
use accord_core::search;
use accord_crypto::{derive_search_key, node_id_of, Identity};
use accord_node::hex;
use accord_node::node::Node;
use accord_node::outbound::OutboundSink;
use accord_node::service::NodeService;
use accord_proto::core_msg::MsgBody;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use serde_json::{json, Value};
use tokio::runtime::Runtime;

/// Volume cible du budget « ouverture d'une conversation » (ROADMAP §9.2).
const VOLUME_CIBLE: u64 = 100_000;

/// Paliers de la courbe d'échelle : le tableau §10.2 écrit le budget à 10 k,
/// §9.2 à 100 k. Mesurer les deux montre la pente entre les deux.
const PALIERS: [u64; 3] = [1_000, 10_000, VOLUME_CIBLE];

/// Taille de page de l'interface (`app/src/lib/history.ts`, `PAGE_SIZE`).
const PAGE: u64 = 50;

/// Difficulté de preuve de travail de l'identité du banc : nulle, la PoW
/// réseau coûterait des secondes sans rien mesurer d'utile ici.
const POW_BANC: u32 = 0;

/// Vocabulaire du corpus. Le coût de la recherche dépend de la distribution des
/// mots : un mot fréquent doit apparaître dans une grande part des messages —
/// c'est exactement le cas qui met l'index en difficulté.
const VOCABULAIRE: &[&str] = &[
    "bonjour",
    "demain",
    "réunion",
    "projet",
    "merci",
    "message",
    "fichier",
    "version",
    "correctif",
    "branche",
    "mesure",
    "budget",
    "index",
    "conversation",
    "historique",
    "pagination",
    "chiffrement",
    "réseau",
    "appareil",
    "sauvegarde",
    "parc",
    "soirée",
];

/// Mot présent dans TOUS les messages du corpus (pire cas de la recherche :
/// l'index rend autant d'identifiants qu'il y a de messages).
const MOT_FREQUENT: &str = "accord";

/// Mot présent dans un message sur mille (cas courant : peu de résultats).
const MOT_RARE: &str = "zyzzyva";

/// Générateur déterministe (xorshift) : corpus reproductible d'une exécution à
/// l'autre, sans dépendance de génération aléatoire.
fn melange(mut x: u64) -> u64 {
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    x
}

/// Identifiant du message `i`. Dispersé, comme le `new_id16` aléatoire du
/// chemin réel : un compteur ordonné donnerait des insertions et des lectures
/// par clé primaire d'une localité que la vraie base n'a pas.
fn id_message(i: u64) -> [u8; 16] {
    let mut id = [0u8; 16];
    id[..8].copy_from_slice(&melange(i.wrapping_mul(0xD6E8_FEB8_6659_FD93)).to_be_bytes());
    id[8..].copy_from_slice(&melange(i ^ 0x9E37_79B9_7F4A_7C15).to_be_bytes());
    id
}

/// Ligne de corpus du message `i` : une dizaine de mots du vocabulaire, plus
/// [`MOT_FREQUENT`] partout et [`MOT_RARE`] de loin en loin.
fn ligne(i: u64) -> String {
    let mut graine = melange(i.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let mut mots: Vec<&str> = Vec::with_capacity(12);
    mots.push(MOT_FREQUENT);
    for _ in 0..9 {
        graine = melange(graine);
        mots.push(VOCABULAIRE[(graine % VOCABULAIRE.len() as u64) as usize]);
    }
    if i % 1_000 == 0 {
        mots.push(MOT_RARE);
    }
    mots.join(" ")
}

/// Un banc : base chiffrée sur disque, peuplée, exposée par le service API.
struct Banc {
    /// Gardé vivant pour la durée du banc : sa destruction efface la base.
    _dir: tempfile::TempDir,
    service: NodeService,
    rt: Runtime,
    peer: String,
    messages: u64,
}

impl Banc {
    /// Appelle une méthode JSON-RPC comme le fait l'interface.
    fn appel(&self, method: &str, params: Value) -> Value {
        self.rt
            .block_on(self.service.call(method, params))
            .unwrap_or_else(|e| panic!("{method} a échoué : {e:?}"))
    }

    /// Séquence exacte du montage de `DmView` sur une conversation directe.
    fn ouvrir_conversation(&self) -> Value {
        let page = self.appel("dm.history", json!({ "pubkey": self.peer, "limit": PAGE }));
        self.appel("dm.pins", json!({ "pubkey": self.peer }));
        page
    }

    /// Position de lecture locale, d'où découle le compteur de non-lus que
    /// `friends.list` recalcule à chaque appel.
    fn marquer_lu(&self, lamport: u64) {
        self.appel(
            "dm.mark_read",
            json!({ "pubkey": self.peer, "lamport": lamport }),
        );
    }
}

/// Peuple une base neuve de `n` messages directs dans une seule conversation,
/// indexés pour la recherche comme le fait le chemin d'envoi/réception.
fn peupler(n: u64) -> Banc {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let identity = Identity::from_seed_with_pow_bits([3u8; 32], POW_BANC);
    let search_key = derive_search_key(identity.seed());
    let db = Db::open(&dir.path().join("core.db"), &[7u8; 32]).expect("base");
    let peer = [42u8; 32];

    db.upsert_contact(&Contact {
        node_id: node_id_of(&peer).0,
        pubkey: peer,
        display_name: "pair du banc".into(),
        state: ContactState::Friend,
        added_ms: 0,
        last_seen_ms: 0,
        verified_at: None,
        verified_pubkey: None,
        state_changed_ms: 0,
    })
    .expect("contact");

    let moi = identity.public_key();
    for i in 1..=n {
        let msg_id = id_message(i);
        let texte = ligne(i);
        let body = MsgBody::Text {
            text: texte.clone(),
            reply_to: None,
            attachments: Vec::new(),
        };
        // Un message sur deux vient du pair : les compteurs de non-lus et les
        // accusés de lecture ne portent que sur les messages entrants.
        let author = if i % 2 == 0 { peer } else { moi };
        db.insert_dm(&DmRecord {
            msg_id,
            peer,
            author,
            lamport: i,
            sent_ms: 1_700_000_000_000 + i * 1_000,
            kind: body.kind(),
            body: body.encode_body(),
            acked: true,
            deleted: false,
            edited: None,
        })
        .expect("insertion");
        search::index_message(&db, &search_key, &msg_id, &texte).expect("indexation");
    }

    let stats = db.storage_stats().expect("statistiques");
    let jetons = db.search_index_rows().expect("lignes d'index");
    println!(
        "\n-- corpus {n} messages : base {:.1} Mio, {jetons} lignes d'index de recherche ({:.0} par message) --",
        stats.db_bytes.unwrap_or(0) as f64 / (1024.0 * 1024.0),
        jetons as f64 / n as f64
    );

    let node = Arc::new(Node::new(identity, db, OutboundSink::null()));
    Banc {
        _dir: dir,
        service: NodeService::new(node),
        rt: Runtime::new().expect("runtime tokio"),
        peer: hex::encode(&peer),
        messages: n,
    }
}

/// Ouverture d'une conversation : la séquence complète de `DmView`, du 1 k au
/// volume cible. C'est ce chiffre que le budget de la feuille de route vise.
fn bench_ouverture(c: &mut Criterion) {
    let mut groupe = c.benchmark_group("ouverture_conversation");
    groupe.sample_size(20);
    for taille in PALIERS {
        let banc = peupler(taille);
        // Contrôle : une page pleine, sinon la mesure ne porte sur rien.
        let page = banc.ouvrir_conversation();
        let recus = page["messages"].as_array().map_or(0, Vec::len) as u64;
        assert_eq!(recus, PAGE.min(taille), "page d'historique incomplète");
        groupe.bench_with_input(BenchmarkId::from_parameter(taille), &taille, |b, _| {
            b.iter(|| black_box(banc.ouvrir_conversation()));
        });
    }
    groupe.finish();
}

/// Tout ce qui se mesure au volume cible, sur UN seul corpus peuplé (chaque
/// peuplement de 100 000 messages coûte une vingtaine de secondes).
fn bench_volume_cible(c: &mut Criterion) {
    let banc = peupler(VOLUME_CIBLE);
    let mut groupe = c.benchmark_group("volume_cible_100k");
    groupe.sample_size(10);

    // Défilement vers le haut au milieu de l'historique : l'index doit éviter
    // un balayage complet.
    groupe.bench_function("pagination_profonde", |b| {
        b.iter(|| {
            black_box(banc.appel(
                "dm.history",
                json!({ "pubkey": banc.peer, "limit": PAGE, "before_lamport": banc.messages / 2 }),
            ))
        });
    });

    // Seconde moitié du montage de `DmView` : `markRead` déclenche
    // `friends.list`, qui recompte les non-lus de chaque contact. Deux cas,
    // parce que le coût suit le nombre de NON-LUS et non la taille de
    // l'historique — les mesurer séparément le prouve.
    banc.marquer_lu(VOLUME_CIBLE);
    groupe.bench_function("friends_list_tout_lu", |b| {
        b.iter(|| black_box(banc.appel("friends.list", json!({}))));
    });
    banc.marquer_lu(1);
    groupe.bench_function("friends_list_50k_non_lus", |b| {
        b.iter(|| black_box(banc.appel("friends.list", json!({}))));
    });

    // Recherche : mot rare (une centaine de résultats), mot fréquent (présent
    // dans les 100 000 messages), et recherche filtrée sans mot-clé (chemin
    // « candidats récents », déjà borné).
    for (nom, requete) in [
        ("recherche_mot_rare", MOT_RARE),
        ("recherche_mot_frequent", MOT_FREQUENT),
        ("recherche_filtre_sans_mot_cle", "has:file"),
    ] {
        groupe.bench_function(nom, |b| {
            b.iter(|| black_box(banc.appel("search.query", json!({ "query": requete }))));
        });
    }
    groupe.finish();
}

criterion_group!(benches, bench_ouverture, bench_volume_cible);
criterion_main!(benches);
