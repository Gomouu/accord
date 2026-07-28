//! Benchmarks criterion — anti-régression perf des primitives chaudes
//! (D33) : handshake complet (classique et hybride post-quantique) et
//! scellement/ouverture AEAD par session.
//!
//! Exécution : `cargo bench -p accord-crypto`. Ces mesures ne sont PAS un gate
//! (le matériel varie) : elles servent à comparer avant/après un changement.

use accord_crypto::handshake::{respond, Initiator, NonceCache};
use accord_crypto::{Identity, SessionCrypto, SessionKeys};
use accord_proto::limits::CAP_PQ_HYBRID;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

/// PoW minimal : on mesure le coût cryptographique du handshake, pas celui de
/// la preuve de travail d'identité (générée hors boucle).
const POW: u32 = 1;

/// Un handshake complet, des deux côtés, sous les capacités données.
///
/// `caps` pilote le mode : `None` pour le classique, `Some(CAP_PQ_HYBRID)` pour
/// l'hybride. Les deux pairs reçoivent la même valeur — un handshake où un seul
/// côté annonce l'hybride retombe en classique et ne mesurerait pas ce qu'on
/// croit mesurer.
fn handshake_complet(alice: &Identity, bob: &Identity, now: u64, caps: Option<u32>) -> [u8; 8] {
    let init = Initiator::start(black_box(alice), now, vec![], POW, None, caps);
    let mut cache = NonceCache::new();
    let (welcome, _est_b) = respond(
        black_box(bob),
        init.hello(),
        now + 20,
        &mut cache,
        POW,
        caps,
    )
    .unwrap();
    let est_a = init.finish(&welcome, now + 40).unwrap();
    est_a.session_id
}

/// Handshake classique contre handshake hybride, dans un même groupe.
///
/// **Ce que ce chiffre sert à trancher** (jalon 2, §7.6) : le surcoût de
/// l'hybride ML-KEM se paie une fois par session, pas par message. Le comparer
/// au coût d'établissement complet — et non à zéro — est la seule lecture
/// honnête : les deux barres incluent la signature Ed25519, la vérification de
/// PoW et les deux dérivations HKDF, qui dominent déjà le classique.
///
/// ⚠️ Ce n'est PAS un gate : le matériel varie d'une machine à l'autre. La
/// mesure sert à comparer avant/après un changement, et à nourrir
/// `docs/PERFORMANCE.md`.
fn bench_handshake(c: &mut Criterion) {
    let alice = Identity::generate_with_pow_bits(POW);
    let bob = Identity::generate_with_pow_bits(POW);
    let now = 1_000_000u64;

    let mut group = c.benchmark_group("handshake_complet");
    group.bench_function("x25519", |b| {
        b.iter(|| black_box(handshake_complet(&alice, &bob, now, None)))
    });
    group.bench_function("hybride_x25519_ml_kem", |b| {
        b.iter(|| black_box(handshake_complet(&alice, &bob, now, Some(CAP_PQ_HYBRID))))
    });
    group.finish();
}

fn session_pair() -> (SessionCrypto, SessionCrypto) {
    // Clés symétriques partagées (le handshake est benché séparément).
    let keys = SessionKeys::new([7u8; 32], [9u8; 32]);
    let sid = [1u8; 8];
    (
        SessionCrypto::new(&keys, sid, true, 0),
        SessionCrypto::new(&keys, sid, false, 0),
    )
}

fn bench_aead(c: &mut Criterion) {
    let mut group = c.benchmark_group("aead_session");
    for taille in [64usize, 1024, 16 * 1024] {
        let plaintext = vec![0xABu8; taille];
        group.throughput(Throughput::Bytes(taille as u64));

        group.bench_with_input(BenchmarkId::new("seal", taille), &plaintext, |b, pt| {
            let (mut init, _) = session_pair();
            b.iter(|| black_box(init.seal(black_box(pt)).unwrap()));
        });

        group.bench_with_input(BenchmarkId::new("open", taille), &plaintext, |b, pt| {
            // Ouverture : l'initiateur scelle avec le sens i2r, le répondeur
            // ouvre avec le même sens. Compteur/epoch neufs à chaque itération
            // via une paire fraîche (l'anti-rejeu refuserait un doublon).
            b.iter_batched(
                || {
                    let (mut i, r) = session_pair();
                    let packet = i.seal(pt).unwrap();
                    (r, packet)
                },
                |(mut r, packet)| black_box(r.open(black_box(&packet)).unwrap()),
                criterion::BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

criterion_group!(benches, bench_handshake, bench_aead);
criterion_main!(benches);
