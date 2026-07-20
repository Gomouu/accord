//! Benchmarks criterion — anti-régression perf des requêtes DB chaudes (D33).
//!
//! L'historique de conversation est lu à chaque ouverture de fil et à chaque
//! pagination : sa requête (index `peer, lamport DESC`) doit rester bon marché
//! même quand la conversation grossit. On mesure aussi l'insertion d'un DM
//! (chemin de réception) et la relève d'une page profonde (scroll arrière).
//!
//! Base EN MÉMOIRE (aucune I/O disque) : on isole le coût requête/matérialisa-
//! tion, pas celui du stockage. Exécution : `cargo bench -p accord-core`.

use accord_core::db::{Db, DmRecord};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

fn dm(peer: [u8; 32], lamport: u64) -> DmRecord {
    let mut msg_id = [0u8; 16];
    msg_id[..8].copy_from_slice(&lamport.to_be_bytes());
    DmRecord {
        msg_id,
        peer,
        author: peer,
        lamport,
        sent_ms: 1_700_000_000_000 + lamport,
        kind: 0,
        body: b"message d'historique de longueur realiste pour le bench".to_vec(),
        acked: true,
        deleted: false,
        edited: None,
    }
}

/// Base peuplée de `n` messages dans une conversation.
fn db_avec_historique(n: u64) -> (Db, [u8; 32]) {
    let db = Db::open_in_memory(&[7u8; 32]).expect("db mémoire");
    let peer = [42u8; 32];
    for lamport in 1..=n {
        db.insert_dm(&dm(peer, lamport)).expect("insert");
    }
    (db, peer)
}

fn bench_dm_history(c: &mut Criterion) {
    let mut group = c.benchmark_group("dm_history_page_recente");
    for taille in [100u64, 1_000, 10_000] {
        let (db, peer) = db_avec_historique(taille);
        group.bench_with_input(BenchmarkId::from_parameter(taille), &taille, |b, _| {
            // Page la plus récente (50 messages), cas d'ouverture de fil.
            b.iter(|| black_box(db.dm_history(black_box(&peer), u64::MAX, 50).unwrap()))
        });
    }
    group.finish();
}

fn bench_dm_history_deep(c: &mut Criterion) {
    // Pagination PROFONDE (scroll arrière au milieu d'un gros historique) :
    // l'index doit éviter un scan complet.
    let (db, peer) = db_avec_historique(10_000);
    c.bench_function("dm_history_page_profonde_10k", |b| {
        b.iter(|| black_box(db.dm_history(black_box(&peer), 5_000, 50).unwrap()))
    });
}

fn bench_insert_dm(c: &mut Criterion) {
    // Insertion sur une conversation déjà volumineuse (maintien d'index).
    c.bench_function("insert_dm_sur_conversation_10k", |b| {
        b.iter_batched(
            || db_avec_historique(10_000),
            |(db, peer)| {
                black_box(db.insert_dm(&dm(peer, 10_001)).unwrap());
            },
            criterion::BatchSize::LargeInput,
        )
    });
}

criterion_group!(
    benches,
    bench_dm_history,
    bench_dm_history_deep,
    bench_insert_dm
);
criterion_main!(benches);
