//! Harnais de mesures G2 : trois benchs reproductibles,
//! ignorés par défaut pour ne pas alourdir la suite normale. Lancement :
//!
//! ```sh
//! cargo test -p accord-node --release --test bench_mesures -- \
//!     --ignored --nocapture --test-threads=1
//! ```
//!
//! Chaque bench imprime ses chiffres (`BENCH …`) sur stdout. Le montage
//! réutilise exclusivement l'API publique des crates — aucun code produit
//! n'est modifié. Code de mesure assumé simple : chronométrage `Instant`,
//! percentiles sur échantillons triés.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use accord_crypto::{FriendCode, Identity};
use accord_dht::{DhtConfig, DhtRpc, KademliaNode};
use accord_node::{identity, run, NodeConfig, Paths, RunningNode, VoiceBackend};
use accord_proto::dht_msg::{DhtBody, DhtMessage};
use accord_proto::types::{DhtRecord, NodeId, NodeInfo, RecordKind, WireAddr};
use accord_voice::params::FRAME_SAMPLES;

// ---------------------------------------------------------------------------
// Outillage commun
// ---------------------------------------------------------------------------

/// Démarre un nœud complet (UDP local, API sur port éphémère, voix simulée).
async fn boot(dir: &std::path::Path) -> RunningNode {
    let paths = Paths::new(dir);
    let unlocked = identity::create(&paths, "phrase-de-passe-bench", 1).unwrap();
    let config = NodeConfig {
        paths,
        p2p_addr: "127.0.0.1:0".parse().unwrap(),
        api_port: 0,
        pow_bits: 1,
        voice_backend: VoiceBackend::Simule,
        ..NodeConfig::default()
    };
    run(unlocked, config).await.unwrap()
}

/// Attend qu'une condition asynchrone devienne vraie ; rend `false` au-delà
/// de `deadline`.
async fn eventually<F, Fut>(deadline: Duration, pas: Duration, mut cond: F) -> bool
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let fin = Instant::now() + deadline;
    while Instant::now() < fin {
        if cond().await {
            return true;
        }
        tokio::time::sleep(pas).await;
    }
    cond().await
}

/// Établit l'amitié Alice → Bob (montage identique à `two_node_e2e.rs`).
async fn befriend(alice: &RunningNode, bob: &RunningNode) {
    let alice_pub = alice.node.public_key();
    let bob_pub = bob.node.public_key();
    alice.register_peer(bob_pub, bob.p2p_addr());
    bob.register_peer(alice_pub, alice.p2p_addr());
    alice.node.friend_request(&bob_pub, "Alice").unwrap();
    assert!(
        eventually(SECONDES_10, POLL_COURT, || async {
            bob.node
                .contacts()
                .map(|cs| cs.iter().any(|c| c.pubkey == alice_pub))
                .unwrap_or(false)
        })
        .await,
        "la demande d'ami n'est pas arrivée"
    );
    bob.node.friend_respond(&alice_pub, true).unwrap();
    assert!(
        eventually(SECONDES_10, POLL_COURT, || async {
            use accord_core::db::ContactState;
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
        "l'amitié n'a pas été confirmée"
    );
}

/// Percentile par rang le plus proche sur des durées déjà triées.
fn percentile(tri: &[Duration], p: f64) -> Duration {
    assert!(!tri.is_empty(), "échantillon vide");
    let idx = ((tri.len() - 1) as f64 * p).round() as usize;
    tri[idx]
}

/// Durée en millisecondes lisible.
fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1_000.0
}

const POLL_COURT: Duration = Duration::from_millis(20);
const POLL_FIN: Duration = Duration::from_millis(1);
const SECONDES_10: Duration = Duration::from_secs(10);

// ---------------------------------------------------------------------------
// (a) Débit de messages directs entre 2 nœuds sur UDP local
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "bench G2 : lancer avec --ignored --nocapture (voir en-tête du fichier)"]
async fn bench_debit_dm() {
    const NB_MESSAGES: usize = 200;

    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let alice = boot(dir_a.path()).await;
    let bob = boot(dir_b.path()).await;
    let alice_pub = alice.node.public_key();
    let bob_pub = bob.node.public_key();
    befriend(&alice, &bob).await;

    // Émission en rafale : compose + persistance SQLCipher + mise en file
    // réseau, côté appelant (non bloquant sur la livraison).
    let depart = Instant::now();
    for i in 0..NB_MESSAGES {
        alice
            .node
            .dm_send(&bob_pub, &format!("bench {i}"), None)
            .unwrap();
    }
    let duree_emission = depart.elapsed();

    // Livraison : Bob a historisé les NB_MESSAGES (chiffrement de session,
    // UDP localhost, ingestion + persistance chez Bob).
    let livre = eventually(Duration::from_secs(60), POLL_COURT, || async {
        bob.node
            .dm_history(&alice_pub, u64::MAX, NB_MESSAGES + 8)
            .map(|h| h.iter().filter(|m| m.author == alice_pub).count() >= NB_MESSAGES)
            .unwrap_or(false)
    })
    .await;
    assert!(livre, "tous les messages ne sont pas arrivés chez Bob");
    let duree_livraison = depart.elapsed();

    // Aller-retour applicatif : chaque message est acquitté chez Alice.
    let acquitte = eventually(Duration::from_secs(60), POLL_COURT, || async {
        alice
            .node
            .dm_history(&bob_pub, u64::MAX, NB_MESSAGES + 8)
            .map(|h| {
                h.iter()
                    .filter(|m| m.author == alice_pub && m.acked)
                    .count()
                    >= NB_MESSAGES
            })
            .unwrap_or(false)
    })
    .await;
    assert!(acquitte, "tous les accusés ne sont pas revenus chez Alice");
    let duree_acquittement = depart.elapsed();

    let n = NB_MESSAGES as f64;
    println!("BENCH dm : {NB_MESSAGES} messages, 2 nœuds UDP 127.0.0.1");
    println!(
        "BENCH dm : émission   {:>8.1} ms  ({:>6.0} msg/s côté appelant)",
        ms(duree_emission),
        n / duree_emission.as_secs_f64()
    );
    println!(
        "BENCH dm : livraison  {:>8.1} ms  ({:>6.0} msg/s bout-en-bout)",
        ms(duree_livraison),
        n / duree_livraison.as_secs_f64()
    );
    println!(
        "BENCH dm : acquitté   {:>8.1} ms  ({:>6.0} msg/s aller-retour applicatif)",
        ms(duree_acquittement),
        n / duree_acquittement.as_secs_f64()
    );

    alice.shutdown();
    bob.shutdown();
}

// ---------------------------------------------------------------------------
// (b) Lookup DHT sur un réseau simulé en mémoire de 60 nœuds
// ---------------------------------------------------------------------------
// Réplique minimale du montage de `accord-dht/src/testnet.rs` (privé à sa
// crate) : annuaire en mémoire, RPC dispatchés en appel direct, horloge
// logique avancée de 100 ms par RPC pour recharger les token buckets.

/// Client RPC agissant comme `appelant` sur l'annuaire du réseau simulé.
#[derive(Clone)]
struct RpcAnnuaire {
    appelant: NodeInfo,
    annuaire: Arc<HashMap<NodeId, Arc<KademliaNode>>>,
    horloge: Arc<AtomicU64>,
    compteur_rpc: Arc<AtomicU64>,
}

#[async_trait::async_trait]
impl DhtRpc for RpcAnnuaire {
    async fn send_rpc(&self, to: &NodeInfo, body: DhtBody) -> Option<DhtBody> {
        let cible = self.annuaire.get(&to.node_id)?;
        self.compteur_rpc.fetch_add(1, Ordering::Relaxed);
        let now_ms = self.horloge.fetch_add(100, Ordering::Relaxed);
        let msg = DhtMessage {
            rpc_id: [0x42; 20],
            body,
        };
        cible
            .handle_rpc(&self.appelant, msg, now_ms)
            .map(|m| m.body)
    }

    fn local_id(&self) -> NodeId {
        self.appelant.node_id
    }
}

#[tokio::test]
#[ignore = "bench G2 : lancer avec --ignored --nocapture (voir en-tête du fichier)"]
async fn bench_lookup_dht() {
    const NB_NOEUDS: usize = 60;
    const POW_BITS: u32 = 8;
    const NB_RECORDS: usize = 20;

    // Construction du réseau : 60 identités PoW, un /24 distinct par nœud
    // (limite de diversité IP de la table de routage).
    let depart_montage = Instant::now();
    let mut infos = Vec::with_capacity(NB_NOEUDS);
    let mut noeuds = Vec::with_capacity(NB_NOEUDS);
    for i in 0..NB_NOEUDS {
        let id = Identity::generate_with_pow_bits(POW_BITS);
        let addr: SocketAddr = format!("10.{i}.0.1:4433").parse().unwrap();
        let info = NodeInfo {
            node_id: id.node_id(),
            static_pub: id.public_key(),
            pow_nonce: id.pow_nonce(),
            flags: 0,
            addrs: vec![WireAddr(addr)],
        };
        let config = DhtConfig {
            pow_bits: POW_BITS,
            ..DhtConfig::default()
        };
        noeuds.push(Arc::new(KademliaNode::new(info.clone(), config)));
        infos.push(info);
    }
    let annuaire: Arc<HashMap<NodeId, Arc<KademliaNode>>> = Arc::new(
        noeuds
            .iter()
            .map(|n| (n.node_id(), Arc::clone(n)))
            .collect(),
    );
    let horloge = Arc::new(AtomicU64::new(1_000));
    let compteur_rpc = Arc::new(AtomicU64::new(0));
    let client = |i: usize| RpcAnnuaire {
        appelant: noeuds[i].local().clone(),
        annuaire: Arc::clone(&annuaire),
        horloge: Arc::clone(&horloge),
        compteur_rpc: Arc::clone(&compteur_rpc),
    };

    // Amorçage en deux passes via le nœud 0 (join puis rafraîchissement),
    // comme le fait le réseau de test d'accord-dht.
    for _ in 0..2 {
        for (i, noeud) in noeuds.iter().enumerate() {
            let now = horloge.load(Ordering::Relaxed);
            noeud
                .bootstrap(&client(i), vec![infos[0].clone()], now)
                .await;
        }
    }
    let duree_montage = depart_montage.elapsed();

    // Publication de NB_RECORDS records IDENTITY signés, via des nœuds variés.
    let mut cles = Vec::with_capacity(NB_RECORDS);
    for i in 0..NB_RECORDS {
        let publieur = Identity::generate_with_pow_bits(POW_BITS);
        let code = FriendCode::of_pubkey(&publieur.public_key());
        let cle = code.dht_key();
        let mut valeur = code.payload().to_vec();
        valeur.extend_from_slice(&publieur.public_key());
        let now = horloge.load(Ordering::Relaxed);
        let mut record = DhtRecord {
            key: cle,
            kind: RecordKind::Identity,
            value: valeur,
            publisher: publieur.public_key(),
            timestamp_ms: now,
            expiry_s: 3_600,
            sig: [0; 64],
        };
        record.sig = publieur.sign(&record.signable_bytes());
        let via = (i * 3) % NB_NOEUDS;
        let stocke = noeuds[via].put(&client(via), record, now).await;
        assert!(stocke > 0, "le record {i} n'a atteint aucun pair");
        cles.push(cle);
    }

    // Mesure : un lookup `get` par nœud (60 lookups, clés en rotation).
    // `get` regarde d'abord le magasin local : les vias déjà détenteurs
    // (k = 20 copies sur 60 nœuds) répondent sans RPC — comptés à part.
    let mut durees_reseau = Vec::new();
    let mut locaux = 0usize;
    let mut rpc_reseau = 0u64;
    for (i, noeud) in noeuds.iter().enumerate() {
        let cle = cles[i % NB_RECORDS];
        let rpc = client(i);
        let avant = compteur_rpc.load(Ordering::Relaxed);
        let now = horloge.load(Ordering::Relaxed);
        let depart = Instant::now();
        let trouve = noeud.get(&rpc, cle, now).await;
        let duree = depart.elapsed();
        assert!(trouve.is_some(), "lookup {i} : record introuvable");
        let nb_rpc = compteur_rpc.load(Ordering::Relaxed) - avant;
        if nb_rpc == 0 {
            locaux += 1;
        } else {
            durees_reseau.push(duree);
            rpc_reseau += nb_rpc;
        }
    }
    durees_reseau.sort_unstable();

    println!(
        "BENCH dht : réseau simulé de {NB_NOEUDS} nœuds (PoW {POW_BITS} bits), \
         montage {:.1} ms",
        ms(duree_montage)
    );
    println!(
        "BENCH dht : {} lookups réseau — médiane {:.3} ms, p95 {:.3} ms, \
         {:.1} RPC/lookup en moyenne",
        durees_reseau.len(),
        ms(percentile(&durees_reseau, 0.50)),
        ms(percentile(&durees_reseau, 0.95)),
        rpc_reseau as f64 / durees_reseau.len() as f64
    );
    println!("BENCH dht : {locaux} lookups résolus sur le magasin local (sans RPC)");
}

// ---------------------------------------------------------------------------
// (c) Latence d'une trame voix bout-en-bout (mode simulé)
// ---------------------------------------------------------------------------
// Mesure observable la plus fine sans toucher au code produit : délai entre
// l'injection de parole chez Alice et l'ouverture de l'indicateur « parle »
// d'Alice chez Bob (l'indicateur s'ouvre à la PREMIÈRE trame reçue). Le
// chemin couvert : attente du tick 20 ms d'Alice, VAD + encodage PCM 8 bits,
// session chiffrée UDP localhost, ingestion moteur chez Bob, plus la
// granularité du sondage (1 ms).

/// Trame de parole : alternance pleine d'amplitude, au-dessus du seuil VAD.
fn tone() -> Vec<i16> {
    (0..FRAME_SAMPLES)
        .map(|i| if i % 2 == 0 { 20_000 } else { -20_000 })
        .collect()
}

#[tokio::test]
#[ignore = "bench G2 : lancer avec --ignored --nocapture (voir en-tête du fichier)"]
async fn bench_latence_voix() {
    const ITERATIONS: usize = 12;
    const TRAMES_PAR_RAFALE: usize = 5;

    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let alice = boot(dir_a.path()).await;
    let bob = boot(dir_b.path()).await;
    let alice_pub = alice.node.public_key();
    let bob_pub = bob.node.public_key();
    befriend(&alice, &bob).await;

    // Groupe partagé ; salon vocal par défaut (channel_id == group_id).
    let gid_hex = alice.node.group_create("Bench").unwrap();
    let gid: [u8; 16] = accord_node::hex::decode(&gid_hex).unwrap();
    let invite_id_hex = alice.node.group_invite_create(&gid, &bob_pub).unwrap();
    let invite_id: [u8; 16] = accord_node::hex::decode(&invite_id_hex).unwrap();
    assert!(
        eventually(SECONDES_10, POLL_COURT, || async {
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
        eventually(SECONDES_10, POLL_COURT, || async {
            bob.node
                .group_state(&gid)
                .map(|s| s.is_member(&bob_pub))
                .unwrap_or(false)
        })
        .await,
        "Bob n'a pas matérialisé le groupe"
    );

    alice.voice().join(gid, gid).await.unwrap();
    bob.voice().join(gid, gid).await.unwrap();
    let bob_voit_alice = |parle: bool| {
        let bob = &bob;
        async move {
            match bob.voice().status().await {
                Ok(Some(s)) => s
                    .participants
                    .iter()
                    .any(|p| p.pubkey == alice_pub && p.speaking == parle),
                _ => false,
            }
        }
    };
    assert!(
        eventually(SECONDES_10, POLL_COURT, || bob_voit_alice(false)).await,
        "Bob ne voit pas Alice dans le salon"
    );

    let mut durees = Vec::with_capacity(ITERATIONS);
    for iteration in 0..ITERATIONS {
        // Indicateur fermé (hystérésis de 400 ms après la dernière trame).
        assert!(
            eventually(SECONDES_10, Duration::from_millis(5), || bob_voit_alice(
                false
            ))
            .await,
            "itération {iteration} : l'indicateur ne s'est pas refermé"
        );
        let depart = Instant::now();
        for _ in 0..TRAMES_PAR_RAFALE {
            alice.voice().inject_pcm(tone());
        }
        assert!(
            eventually(SECONDES_10, POLL_FIN, || bob_voit_alice(true)).await,
            "itération {iteration} : la parole n'est pas arrivée chez Bob"
        );
        durees.push(depart.elapsed());
    }
    durees.sort_unstable();

    println!(
        "BENCH voix : {ITERATIONS} rafales de {TRAMES_PAR_RAFALE} trames (20 ms), \
         2 nœuds UDP 127.0.0.1, codec simulé PCM 8 bits"
    );
    println!(
        "BENCH voix : injection → « parle » chez le pair — médiane {:.1} ms, \
         p95 {:.1} ms, min {:.1} ms, max {:.1} ms",
        ms(percentile(&durees, 0.50)),
        ms(percentile(&durees, 0.95)),
        ms(durees[0]),
        ms(durees[durees.len() - 1])
    );

    alice.shutdown();
    bob.shutdown();
}
