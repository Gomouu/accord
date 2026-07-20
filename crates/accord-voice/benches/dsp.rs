//! Benchmarks criterion — anti-régression perf du DSP voix (D30/D33).
//!
//! Ces routines tournent sur la boucle 20 ms de CHAQUE participant : mixage
//! full-mesh (jusqu'à N flux additionnés + soft-limiter) et détection d'activité
//! vocale (VAD). Une régression y ajoute de la latence audio et de la charge
//! CPU multipliée par le nombre de pairs. Chemin PUR (aucun matériel : le codec
//! Opus réel est derrière la feature `hardware`).
//!
//! Exécution : `cargo bench -p accord-voice --bench dsp`.

use accord_voice::mix::mix_frames;
use accord_voice::params::{FRAME_SAMPLES, VAD_THRESHOLD_DBFS};
use accord_voice::vad::Vad;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

/// Trame PCM 20 ms déterministe (onde en dents de scie bornée).
fn frame(offset: i16) -> Vec<i16> {
    (0..FRAME_SAMPLES)
        .map(|i| ((i as i16).wrapping_mul(7).wrapping_add(offset)) % 8_000 - 4_000)
        .collect()
}

fn bench_mix(c: &mut Criterion) {
    let mut group = c.benchmark_group("mix_frames_full_mesh");
    // Taille du salon vocal : nombre de flux à additionner par trame.
    for pairs in [2usize, 4, 8] {
        let frames: Vec<Vec<i16>> = (0..pairs).map(|i| frame(i as i16 * 137)).collect();
        group.bench_with_input(BenchmarkId::from_parameter(pairs), &frames, |b, frames| {
            b.iter(|| black_box(mix_frames(black_box(frames.clone()))))
        });
    }
    group.finish();
}

fn bench_vad(c: &mut Criterion) {
    let parole = frame(1);
    let silence = vec![0i16; FRAME_SAMPLES];
    c.bench_function("vad_trame_active", |b| {
        let mut vad = Vad::new(VAD_THRESHOLD_DBFS);
        b.iter(|| black_box(vad.is_active(black_box(&parole))))
    });
    c.bench_function("vad_trame_silence", |b| {
        let mut vad = Vad::new(VAD_THRESHOLD_DBFS);
        b.iter(|| black_box(vad.is_active(black_box(&silence))))
    });
}

criterion_group!(benches, bench_mix, bench_vad);
criterion_main!(benches);
