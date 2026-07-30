//! Annulation d'écho acoustique (AEC, D-051) : supprime de la capture micro
//! le son que le haut-parleur local vient de jouer, pour que les
//! interlocuteurs ne s'entendent pas « en double ».
//!
//! Chaîne pur Rust, sans matériel, testable hors ligne :
//!
//! 1. **estimation du délai global** haut-parleur → micro (files de sortie,
//!    tampons du périphérique, trajet acoustique) par corrélation croisée des
//!    enveloppes d'énergie, à la granularité de la trame (20 ms), sur une
//!    fenêtre glissante ;
//! 2. **filtre adaptatif** NLMS en domaine fréquentiel par partitions
//!    (overlap-save, FFT réelle via `realfft` — déjà dans l'arbre de
//!    dépendances), queue de 160 ms : soustrait l'écho linéaire estimé ;
//! 3. **détection de double-parole** (Geigel) : l'adaptation est gelée quand
//!    la voix locale domine, pour ne jamais apprendre — ni supprimer — la
//!    parole du locuteur ;
//! 4. **suppression résiduelle** (NLP) bornée (−12 dB max), active seulement
//!    quand l'écho seul est présent, avec lissage attaque/relâche.
//!
//! L'hôte pousse chaque trame **réellement envoyée à la sortie** via
//! [`EchoCanceller::push_far`] (une par tick de 20 ms, silence compris — la
//! chronologie de la référence doit avancer au rythme de la lecture), et
//! passe chaque trame micro par [`EchoCanceller::process`]. Sans écoute
//! (casque, silence distant), la référence est muette et l'AEC est
//! transparent.

use realfft::num_complex::Complex;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};
use std::collections::VecDeque;
use std::sync::Arc;

use crate::params::FRAME_SAMPLES;

/// Taille de bloc (une trame de 20 ms à 48 kHz).
const BLOCK: usize = FRAME_SAMPLES;
/// Taille de FFT (overlap-save : deux blocs).
const FFT_SIZE: usize = 2 * BLOCK;
/// Bacs de la FFT réelle.
const BINS: usize = FFT_SIZE / 2 + 1;
/// Partitions du filtre adaptatif (queue couverte : 8 × 20 ms = 160 ms).
const PARTITIONS: usize = 8;
/// Délai global maximal recherché (trames de 20 ms : 25 = 500 ms).
const DELAY_MAX_FRAMES: usize = 25;
/// Fenêtre de corrélation des enveloppes (trames : 64 = 1,28 s).
const ENV_WINDOW: usize = 64;
/// Pas d'adaptation NLMS (normalisé, stable pour μ ≤ 1).
const NLMS_MU: f32 = 0.5;
/// Régularisation de la normalisation NLMS (évite la division par ~0 sur le
/// silence ; négligeable devant l'énergie d'une trame de parole).
const NLMS_EPS: f32 = 1e6;
/// Seuil Geigel de double-parole : pic micro > 90 % du pic far-end récent.
const DTD_THRESHOLD: f32 = 0.9;
/// Gel d'adaptation après détection de double-parole (trames : 8 = 160 ms).
const DTD_HANGOVER_FRAMES: u32 = 8;
/// Enveloppe far-end sous laquelle la référence est considérée muette
/// (échelle RMS `i16` : ≈ −60 dBFS).
const FAR_SILENCE_RMS: f32 = 30.0;
/// Plancher de la suppression résiduelle (0.25 ≈ −12 dB).
const NLP_FLOOR: f32 = 0.25;
/// Corrélation minimale pour adopter un nouveau délai.
const DELAY_MIN_CORR: f32 = 0.5;
/// Trames consécutives de confirmation avant de changer de délai.
const DELAY_STABLE_FRAMES: u32 = 15;

/// Tampons de travail de [`EchoCanceller::process`], alloués une fois pour
/// toutes.
///
/// Le traitement tourne à la cadence média (une trame de 20 ms par session
/// active) et il allouait treize vecteurs par trame — quatre FFT, une
/// normalisation, deux enveloppes centrées, la référence retardée clonée…
/// Toutes ces tailles sont des CONSTANTES du module ; rien ne justifiait de
/// les redemander à l'allocateur cinquante fois par seconde sur le chemin
/// temps réel. Les longueurs ne changent jamais après la construction : le
/// traitement écrase, il ne redimensionne pas.
struct Scratch {
    /// Trame micro en `f32` (longueur `BLOCK`).
    mic: Vec<f32>,
    /// Référence far-end retardée du délai global (longueur `BLOCK`).
    delayed: Vec<f32>,
    /// Bloc overlap-save `[précédent | courant]` (longueur `FFT_SIZE`).
    block: Vec<f32>,
    /// Spectre de l'écho estimé, ACCUMULÉ : remis à zéro à chaque trame.
    echo_spec: Vec<Complex<f32>>,
    /// Écho estimé en temps (longueur `FFT_SIZE`).
    echo_time: Vec<f32>,
    /// Erreur `micro − écho` (longueur `BLOCK`).
    err: Vec<f32>,
    /// Erreur zéro-remplie à gauche pour la FFT (longueur `FFT_SIZE`). Seule
    /// la moitié droite est jamais écrite ; la gauche reste nulle.
    err_block: Vec<f32>,
    /// Spectre de l'erreur (longueur `BINS`).
    err_spec: Vec<Complex<f32>>,
    /// Normalisation NLMS par bac, INITIALISÉE à `NLMS_EPS` à chaque trame.
    norm: Vec<f32>,
    /// Copie de la partition à contraindre (longueur `BINS`).
    w_spec: Vec<Complex<f32>>,
    /// Partition contrainte en temps (longueur `FFT_SIZE`).
    w_time: Vec<f32>,
    /// Enveloppe micro centrée (longueur `ENV_WINDOW`).
    mic_c: Vec<f32>,
    /// Enveloppe far-end centrée (longueur `ENV_WINDOW`).
    far_c: Vec<f32>,
}

impl Scratch {
    fn new() -> Self {
        Self {
            mic: vec![0.0; BLOCK],
            delayed: vec![0.0; BLOCK],
            block: vec![0.0; FFT_SIZE],
            echo_spec: vec![Complex::default(); BINS],
            echo_time: vec![0.0; FFT_SIZE],
            err: vec![0.0; BLOCK],
            err_block: vec![0.0; FFT_SIZE],
            err_spec: vec![Complex::default(); BINS],
            norm: vec![NLMS_EPS; BINS],
            w_spec: vec![Complex::default(); BINS],
            w_time: vec![0.0; FFT_SIZE],
            mic_c: Vec::with_capacity(ENV_WINDOW),
            far_c: Vec::with_capacity(ENV_WINDOW),
        }
    }
}

/// Annuleur d'écho acoustique : une instance par session audio active.
pub struct EchoCanceller {
    fft: Arc<dyn RealToComplex<f32>>,
    ifft: Arc<dyn ComplexToReal<f32>>,
    /// Trames far-end brutes, la plus récente en tête (référence retardée et
    /// fenêtre du détecteur de double-parole).
    far_ring: VecDeque<Vec<f32>>,
    /// Enveloppes RMS par trame (chronologie sortie), la plus ancienne en tête.
    far_env: VecDeque<f32>,
    /// Enveloppes RMS par trame (chronologie micro), la plus ancienne en tête.
    mic_env: VecDeque<f32>,
    /// Corrélation lissée par retard candidat.
    corr: [f32; DELAY_MAX_FRAMES + 1],
    /// Délai global courant (trames).
    delay: usize,
    /// Trames consécutives où un autre délai domine.
    delay_votes: u32,
    /// Candidat au remplacement du délai courant.
    delay_candidate: usize,
    /// Spectres far-end des dernières partitions, le plus récent en tête.
    far_spectra: VecDeque<Vec<Complex<f32>>>,
    /// Bloc far-end retardé précédent (moitié gauche de l'overlap-save).
    prev_far: Vec<f32>,
    /// Partitions du filtre (domaine fréquentiel).
    weights: Vec<Vec<Complex<f32>>>,
    /// Prochaine partition à contraindre (projection du gradient, tournante).
    constrain_next: usize,
    /// Gel d'adaptation restant (double-parole).
    dtd_hangover: u32,
    /// Gain de suppression résiduelle lissé.
    nlp_gain: f32,
    /// Tampons de travail réutilisés d'une trame à l'autre.
    scratch: Scratch,
}

impl Default for EchoCanceller {
    fn default() -> Self {
        Self::new()
    }
}

impl EchoCanceller {
    /// Crée un annuleur neutre (filtre nul, délai 0).
    pub fn new() -> Self {
        let mut planner = RealFftPlanner::<f32>::new();
        Self {
            fft: planner.plan_fft_forward(FFT_SIZE),
            ifft: planner.plan_fft_inverse(FFT_SIZE),
            far_ring: VecDeque::with_capacity(DELAY_MAX_FRAMES + PARTITIONS + 2),
            far_env: VecDeque::with_capacity(ENV_WINDOW),
            mic_env: VecDeque::with_capacity(ENV_WINDOW),
            corr: [0.0; DELAY_MAX_FRAMES + 1],
            delay: 0,
            delay_votes: 0,
            delay_candidate: 0,
            far_spectra: VecDeque::with_capacity(PARTITIONS),
            prev_far: vec![0.0; BLOCK],
            weights: vec![vec![Complex::default(); BINS]; PARTITIONS],
            constrain_next: 0,
            dtd_hangover: 0,
            nlp_gain: 1.0,
            scratch: Scratch::new(),
        }
    }

    /// Délai global courant estimé (trames de 20 ms) — observabilité et tests.
    pub fn delay_frames(&self) -> usize {
        self.delay
    }

    /// Enregistre une trame envoyée à la sortie audio (une par tick de 20 ms,
    /// **silence compris** : la chronologie doit suivre la lecture réelle).
    /// Une trame de taille inattendue est traitée comme du silence.
    pub fn push_far(&mut self, frame: &[i16]) {
        let far: Vec<f32> = if frame.len() == FRAME_SAMPLES {
            frame.iter().map(|&s| f32::from(s)).collect()
        } else {
            vec![0.0; FRAME_SAMPLES]
        };
        if self.far_env.len() == ENV_WINDOW {
            self.far_env.pop_front();
        }
        self.far_env.push_back(rms(&far));
        self.far_ring.push_front(far);
        self.far_ring.truncate(DELAY_MAX_FRAMES + PARTITIONS + 2);
    }

    /// Soustrait l'écho estimé d'une trame micro, en place. Une trame de
    /// taille inattendue traverse inchangée.
    pub fn process(&mut self, pcm: &mut [i16]) {
        if pcm.len() != FRAME_SAMPLES {
            return;
        }
        self.scratch.mic.clear();
        self.scratch.mic.extend(pcm.iter().map(|&s| f32::from(s)));
        self.track_delay();

        // Référence retardée du délai global estimé (silence si l'historique
        // est encore trop court).
        match self.far_ring.get(self.delay) {
            Some(f) => self.scratch.delayed.copy_from_slice(f),
            None => self.scratch.delayed.fill(0.0),
        }

        // Spectre du bloc overlap-save [précédent | courant].
        self.scratch.block[..BLOCK].copy_from_slice(&self.prev_far);
        self.scratch.block[BLOCK..].copy_from_slice(&self.scratch.delayed);
        self.prev_far.copy_from_slice(&self.scratch.delayed);
        // Recycle le spectre le plus ancien : la file de partitions garde des
        // vecteurs de longueur constante, autant réutiliser celui qui sort.
        let mut far_spec = match self.far_spectra.len() {
            n if n >= PARTITIONS => self
                .far_spectra
                .pop_back()
                .unwrap_or_else(|| vec![Complex::default(); BINS]),
            _ => vec![Complex::default(); BINS],
        };
        let _ = self.fft.process(&mut self.scratch.block, &mut far_spec);
        self.far_spectra.push_front(far_spec);

        // Écho linéaire estimé : somme des partitions, retour temporel,
        // moitié droite (overlap-save). L'accumulateur est remis à zéro.
        self.scratch.echo_spec.fill(Complex::default());
        for (part, spec) in self.weights.iter().zip(self.far_spectra.iter()) {
            for ((acc, w), x) in self
                .scratch
                .echo_spec
                .iter_mut()
                .zip(part.iter())
                .zip(spec.iter())
            {
                *acc += w * x;
            }
        }
        let _ = self
            .ifft
            .process(&mut self.scratch.echo_spec, &mut self.scratch.echo_time);
        let scale = 1.0 / FFT_SIZE as f32;

        // Erreur = micro − écho estimé (moitié droite de l'overlap-save).
        for ((e, &m), &t) in self
            .scratch
            .err
            .iter_mut()
            .zip(self.scratch.mic.iter())
            .zip(self.scratch.echo_time[BLOCK..].iter())
        {
            *e = m - t * scale;
        }

        // Double-parole (Geigel) : pic micro comparé au pic far-end sur la
        // fenêtre couverte par la queue du filtre.
        let far_peak = self
            .far_ring
            .iter()
            .skip(self.delay)
            .take(PARTITIONS)
            .flat_map(|f| f.iter())
            .fold(0.0f32, |acc, &v| acc.max(v.abs()));
        let mic_peak = self
            .scratch
            .mic
            .iter()
            .fold(0.0f32, |acc, &v| acc.max(v.abs()));
        if mic_peak > DTD_THRESHOLD * far_peak && far_peak > 0.0 {
            self.dtd_hangover = DTD_HANGOVER_FRAMES;
        } else {
            self.dtd_hangover = self.dtd_hangover.saturating_sub(1);
        }

        // Adaptation NLMS (gelée en double-parole ou référence muette).
        let far_energy: f32 = self
            .far_spectra
            .iter()
            .flat_map(|s| s.iter())
            .map(|c| c.norm_sqr())
            .sum();
        if self.dtd_hangover == 0 && far_energy > NLMS_EPS {
            // La moitié GAUCHE d'`err_block` doit rester nulle (zero-padding de
            // l'overlap-save) : seule la droite est réécrite à chaque trame.
            self.scratch.err_block[..BLOCK].fill(0.0);
            self.scratch.err_block[BLOCK..].copy_from_slice(&self.scratch.err);
            let _ = self
                .fft
                .process(&mut self.scratch.err_block, &mut self.scratch.err_spec);
            // Normalisation par bac : énergie far-end cumulée des partitions.
            self.scratch.norm.fill(NLMS_EPS);
            for spec in &self.far_spectra {
                for (n, x) in self.scratch.norm.iter_mut().zip(spec.iter()) {
                    *n += x.norm_sqr();
                }
            }
            for (part, spec) in self.weights.iter_mut().zip(self.far_spectra.iter()) {
                for ((w, x), (e, n)) in part
                    .iter_mut()
                    .zip(spec.iter())
                    .zip(self.scratch.err_spec.iter().zip(self.scratch.norm.iter()))
                {
                    *w += x.conj() * e * (NLMS_MU / n);
                }
            }
            // Projection du gradient (anti-repliement circulaire), une
            // partition par trame en tournante : coût borné.
            let p = self.constrain_next;
            self.constrain_next = (p + 1) % PARTITIONS;
            self.scratch.w_spec.copy_from_slice(&self.weights[p]);
            let _ = self
                .ifft
                .process(&mut self.scratch.w_spec, &mut self.scratch.w_time);
            for v in self.scratch.w_time.iter_mut() {
                *v *= scale;
            }
            for v in self.scratch.w_time[BLOCK..].iter_mut() {
                *v = 0.0;
            }
            let _ = self
                .fft
                .process(&mut self.scratch.w_time, &mut self.weights[p]);
        }

        // Suppression résiduelle bornée, uniquement écho seul (référence
        // active, pas de double-parole).
        let far_active = self
            .far_env
            .len()
            .checked_sub(1 + self.delay)
            .and_then(|i| self.far_env.get(i))
            .is_some_and(|&e| e > FAR_SILENCE_RMS);
        let target = if far_active && self.dtd_hangover == 0 {
            let ratio = rms(&self.scratch.err) / (rms(&self.scratch.mic) + 1.0);
            ratio.clamp(NLP_FLOOR, 1.0)
        } else {
            1.0
        };
        let rate = if target < self.nlp_gain { 0.5 } else { 0.25 };
        self.nlp_gain += (target - self.nlp_gain) * rate;
        let nlp_gain = self.nlp_gain;
        for (dst, &src) in pcm.iter_mut().zip(self.scratch.err.iter()) {
            *dst = (src * nlp_gain).clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16;
        }
    }

    /// Suit le délai global par corrélation croisée des enveloppes (centrées)
    /// micro/far-end ; changement adopté après confirmation répétée, avec
    /// remise à zéro du filtre (l'écho re-converge en ~0,5 s).
    fn track_delay(&mut self) {
        if self.mic_env.len() == ENV_WINDOW {
            self.mic_env.pop_front();
        }
        self.mic_env.push_back(rms(&self.scratch.mic));
        if self.mic_env.len() < ENV_WINDOW || self.far_env.len() < ENV_WINDOW {
            return;
        }
        // Référence muette sur toute la fenêtre : aucune corrélation fiable.
        if self.far_env.iter().all(|&e| e < FAR_SILENCE_RMS) {
            return;
        }
        let mic_mean: f32 = self.mic_env.iter().sum::<f32>() / ENV_WINDOW as f32;
        let far_mean: f32 = self.far_env.iter().sum::<f32>() / ENV_WINDOW as f32;
        let mic_c = &mut self.scratch.mic_c;
        mic_c.clear();
        mic_c.extend(self.mic_env.iter().map(|&v| v - mic_mean));
        let far_c = &mut self.scratch.far_c;
        far_c.clear();
        far_c.extend(self.far_env.iter().map(|&v| v - far_mean));
        let (mic_c, far_c) = (&self.scratch.mic_c, &self.scratch.far_c);
        for lag in 0..=DELAY_MAX_FRAMES {
            let mut num = 0.0f32;
            let mut mic_sq = 0.0f32;
            let mut far_sq = 0.0f32;
            for t in lag..ENV_WINDOW {
                num += mic_c[t] * far_c[t - lag];
                mic_sq += mic_c[t] * mic_c[t];
                far_sq += far_c[t - lag] * far_c[t - lag];
            }
            let denom = (mic_sq * far_sq).sqrt();
            let c = if denom > f32::EPSILON {
                num / denom
            } else {
                0.0
            };
            self.corr[lag] += (c - self.corr[lag]) * 0.2;
        }
        let (best, &best_corr) = self
            .corr
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .unwrap_or((0, &0.0));
        if best == self.delay || best_corr < DELAY_MIN_CORR {
            self.delay_votes = 0;
            return;
        }
        if best == self.delay_candidate {
            self.delay_votes += 1;
        } else {
            self.delay_candidate = best;
            self.delay_votes = 1;
        }
        if self.delay_votes >= DELAY_STABLE_FRAMES {
            self.delay = best;
            self.delay_votes = 0;
            for part in &mut self.weights {
                part.fill(Complex::default());
            }
            self.far_spectra.clear();
            self.prev_far.fill(0.0);
            self.nlp_gain = 1.0;
        }
    }
}

/// RMS d'une trame (échelle `i16`, calcul f64 pour la stabilité).
fn rms(frame: &[f32]) -> f32 {
    if frame.is_empty() {
        return 0.0;
    }
    let sum: f64 = frame.iter().map(|&v| f64::from(v) * f64::from(v)).sum();
    (sum / frame.len() as f64).sqrt() as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bruit pseudo-aléatoire déterministe (LCG), amplitude bornée.
    fn noise(seed: &mut u64, amplitude: i32) -> Vec<i16> {
        (0..FRAME_SAMPLES)
            .map(|_| {
                *seed = seed
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                (((*seed >> 33) as i32 % (2 * amplitude + 1)) - amplitude) as i16
            })
            .collect()
    }

    /// Chemin d'écho synthétique : délai de `delay` trames, gain 0.5, petit
    /// FIR de coloration [0.7, 0.2, 0.1] (échantillons successifs).
    struct EchoPath {
        history: VecDeque<Vec<i16>>,
        delay: usize,
        tail: [f32; 3],
    }

    impl EchoPath {
        fn new(delay: usize) -> Self {
            Self {
                history: VecDeque::new(),
                delay,
                tail: [0.7, 0.2, 0.1],
            }
        }

        /// Pousse la trame jouée, rend l'écho capté par le micro ce tick.
        fn tick(&mut self, played: &[i16]) -> Vec<i16> {
            self.history.push_front(played.to_vec());
            self.history.truncate(self.delay + 2);
            let Some(src) = self.history.get(self.delay) else {
                return vec![0; FRAME_SAMPLES];
            };
            let prev = self.history.get(self.delay + 1);
            (0..FRAME_SAMPLES)
                .map(|i| {
                    let mut acc = 0.0f32;
                    for (k, &g) in self.tail.iter().enumerate() {
                        let v = if i >= k {
                            f32::from(src[i - k])
                        } else if let Some(prev) = prev {
                            f32::from(prev[FRAME_SAMPLES + i - k])
                        } else {
                            0.0
                        };
                        acc += 0.5 * g * v;
                    }
                    acc as i16
                })
                .collect()
        }
    }

    fn frame_rms_i16(pcm: &[i16]) -> f32 {
        rms(&pcm.iter().map(|&s| f32::from(s)).collect::<Vec<_>>())
    }

    #[test]
    fn cancels_delayed_echo_after_convergence() {
        let mut aec = EchoCanceller::new();
        let mut path = EchoPath::new(6);
        let mut seed = 11u64;
        let mut echo_rms = 0.0f32;
        let mut out_rms = 0.0f32;
        for t in 0..400 {
            let far = noise(&mut seed, 6_000);
            let mut mic = path.tick(&far);
            let before = frame_rms_i16(&mic);
            aec.process(&mut mic);
            aec.push_far(&far);
            if t >= 300 {
                echo_rms += before;
                out_rms += frame_rms_i16(&mic);
            }
        }
        // ERLE exigée après convergence : ≥ 12 dB (facteur 4 en RMS).
        assert!(
            out_rms < echo_rms / 4.0,
            "écho insuffisamment annulé : {out_rms} vs {echo_rms}"
        );
    }

    #[test]
    fn estimates_the_bulk_delay() {
        let mut aec = EchoCanceller::new();
        let mut path = EchoPath::new(9);
        let mut seed = 23u64;
        for _ in 0..200 {
            // Référence en rafales (l'enveloppe doit varier pour corréler).
            let burst = (seed >> 8) % 3 != 0;
            let far = if burst {
                noise(&mut seed, 8_000)
            } else {
                seed = seed.wrapping_mul(48271).wrapping_add(1);
                vec![0i16; FRAME_SAMPLES]
            };
            let mut mic = path.tick(&far);
            aec.process(&mut mic);
            aec.push_far(&far);
        }
        // La trame poussée au tick t est captée au tick t+9 mais l'AEC la
        // range dès t (process avant push_far) : décalage attendu 9 − 1.
        let d = aec.delay_frames();
        assert!(
            (8..=10).contains(&d),
            "délai estimé {d}, attendu autour de 8-9"
        );
    }

    #[test]
    fn double_talk_preserves_near_speech() {
        let mut aec = EchoCanceller::new();
        let mut path = EchoPath::new(4);
        let mut seed = 5u64;
        // Convergence sur écho seul.
        for _ in 0..300 {
            let far = noise(&mut seed, 6_000);
            let mut mic = path.tick(&far);
            aec.process(&mut mic);
            aec.push_far(&far);
        }
        // Double-parole : voix locale (ton carré 2 kHz, RMS 4 000) + écho.
        let near: Vec<i16> = (0..FRAME_SAMPLES)
            .map(|i| if (i / 12) % 2 == 0 { 4_000 } else { -4_000 })
            .collect();
        let near_rms = frame_rms_i16(&near);
        let mut out_rms = 0.0f32;
        let rounds = 50;
        for _ in 0..rounds {
            let far = noise(&mut seed, 6_000);
            let echo = path.tick(&far);
            let mut mic: Vec<i16> = near
                .iter()
                .zip(echo.iter())
                .map(|(&n, &e)| n.saturating_add(e))
                .collect();
            aec.process(&mut mic);
            aec.push_far(&far);
            out_rms += frame_rms_i16(&mic);
        }
        out_rms /= rounds as f32;
        // La voix locale survit : au moins 70 % de son niveau (−3 dB), et pas
        // d'explosion (l'adaptation gelée n'a pas divergé).
        assert!(
            out_rms > near_rms * 0.7,
            "voix locale écrasée en double-parole : {out_rms} vs {near_rms}"
        );
        assert!(out_rms < near_rms * 1.6, "sortie divergente : {out_rms}");
    }

    #[test]
    fn no_far_end_is_transparent() {
        let mut aec = EchoCanceller::new();
        let speech: Vec<i16> = (0..FRAME_SAMPLES)
            .map(|i| if (i / 24) % 2 == 0 { 3_000 } else { -3_000 })
            .collect();
        for _ in 0..50 {
            let mut mic = speech.clone();
            aec.process(&mut mic);
            aec.push_far(&[0i16; FRAME_SAMPLES]);
            assert_eq!(
                mic, speech,
                "sans référence, la capture doit traverser intacte"
            );
        }
    }

    #[test]
    fn unexpected_sizes_never_panic() {
        let mut aec = EchoCanceller::new();
        let mut short = vec![100i16; 7];
        aec.process(&mut short);
        assert_eq!(short, vec![100i16; 7]);
        aec.push_far(&[1i16; 3]);
        let mut ok = vec![0i16; FRAME_SAMPLES];
        aec.process(&mut ok);
    }
}
