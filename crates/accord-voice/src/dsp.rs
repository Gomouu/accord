//! DSP de capture : suppression de bruit (RNNoise, crate Rust pure
//! `nnnoiseless`) et contrôle automatique de gain (AGC), appliqués à la trame
//! PCM locale AVANT la VAD et l'encodage ([`crate::room::VoiceRoom::capture`]).
//!
//! Les deux étages sont indépendants et débrayables à chaud ; désactivés par
//! défaut (le contrat `voice.*` les expose, l'UI choisit sa politique). Ordre
//! d'application : suppression de bruit puis AGC — le gain s'adapte au signal
//! débarrassé de son bruit de fond, pas au bruit.
//!
//! Coût CPU mesuré (Apple M-series, build release, trame mono 20 ms à
//! 48 kHz) : ≈ 105 µs/trame pour la chaîne complète (RNNoise domine, l'AGC
//! est négligeable), soit ≈ 0,5 % d'un cœur. Mesure reproductible via le test
//! `cpu_cost_measurement` :
//! `cargo test -p accord-voice --release -- --ignored --nocapture`.

use nnnoiseless::DenoiseState;

use crate::gain;
use crate::params::FRAME_SAMPLES;

/// Taille de trame interne de RNNoise (480 échantillons = 10 ms à 48 kHz) :
/// une trame Accord de 20 ms est traitée en deux passes successives.
const DENOISE_CHUNK: usize = DenoiseState::FRAME_SIZE;

/// Niveau RMS cible de l'AGC (≈ −26 dBFS pleine échelle `i16`, niveau de
/// parole usuel en VoIP).
const AGC_TARGET_RMS: f32 = 1_640.0;

/// Plancher d'adaptation de l'AGC (≈ −55 dBFS) : en deçà, la trame est du
/// silence ou du bruit de fond — le gain n'est PAS adapté (sinon l'AGC
/// amplifierait le bruit entre les mots), mais reste appliqué (continuité).
const AGC_GATE_RMS: f32 = 58.0;

/// Gain minimal de l'AGC (−12 dB : atténue une source trop forte).
const AGC_GAIN_MIN: f32 = 0.25;

/// Gain maximal de l'AGC (+12 dB : remonte une source trop faible sans
/// transformer un chuchotement en souffle).
const AGC_GAIN_MAX: f32 = 4.0;

/// Vitesse de montée du gain (par trame de 20 ms) : lente (~1 s pour
/// converger), évite le pompage sur les respirations.
const AGC_RISE: f32 = 0.04;

/// Vitesse de descente du gain (par trame de 20 ms) : rapide (~100 ms), une
/// source soudainement forte est ramenée au niveau cible sans saturer
/// longtemps.
const AGC_FALL: f32 = 0.5;

/// Suppression de bruit RNNoise : deux passes de 480 échantillons par trame
/// de 20 ms, état persistant entre les trames (modèle récurrent).
struct Denoiser {
    state: Box<DenoiseState<'static>>,
    input: [f32; DENOISE_CHUNK],
    output: [f32; DENOISE_CHUNK],
}

impl Denoiser {
    fn new() -> Self {
        Self {
            state: DenoiseState::new(),
            input: [0.0; DENOISE_CHUNK],
            output: [0.0; DENOISE_CHUNK],
        }
    }

    /// Débruite une trame en place (échelle `i16` conservée, RNNoise
    /// travaille en `f32` pleine échelle 16 bits).
    fn process(&mut self, pcm: &mut [i16]) {
        for chunk in pcm.chunks_exact_mut(DENOISE_CHUNK) {
            for (dst, &src) in self.input.iter_mut().zip(chunk.iter()) {
                *dst = f32::from(src);
            }
            self.state.process_frame(&mut self.output, &self.input);
            for (dst, &src) in chunk.iter_mut().zip(self.output.iter()) {
                *dst = src.clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16;
            }
        }
    }
}

/// Contrôle automatique de gain : ramène le niveau RMS de la parole vers
/// [`AGC_TARGET_RMS`], montée lente / descente rapide, gain borné.
#[derive(Debug)]
pub struct Agc {
    gain: f32,
}

impl Default for Agc {
    fn default() -> Self {
        Self { gain: 1.0 }
    }
}

impl Agc {
    /// Gain linéaire courant (exposé pour les tests et l'observabilité).
    pub fn gain(&self) -> f32 {
        self.gain
    }

    /// Applique l'AGC à une trame en place : adapte le gain sur les trames de
    /// parole (RMS au-dessus du plancher), applique le gain courant partout
    /// (continuité), sature aux bornes `i16` (jamais de repli).
    pub fn process(&mut self, pcm: &mut [i16]) {
        let rms = frame_rms(pcm);
        if rms > AGC_GATE_RMS {
            let desired = (AGC_TARGET_RMS / rms).clamp(AGC_GAIN_MIN, AGC_GAIN_MAX);
            let rate = if desired < self.gain {
                AGC_FALL
            } else {
                AGC_RISE
            };
            self.gain += (desired - self.gain) * rate;
        }
        gain::apply_gain(pcm, self.gain);
    }
}

/// RMS d'une trame PCM (échelle `i16`).
fn frame_rms(pcm: &[i16]) -> f32 {
    if pcm.is_empty() {
        return 0.0;
    }
    let sum_sq: f64 = pcm.iter().map(|&s| f64::from(s) * f64::from(s)).sum();
    (sum_sq / pcm.len() as f64).sqrt() as f32
}

/// Chaîne DSP de capture : suppression de bruit puis AGC, chacun débrayable.
/// Les deux étages sont créés/détruits à la bascule (l'état RNNoise repart de
/// zéro à la réactivation — voulu : pas d'état périmé).
#[derive(Default)]
pub struct CaptureDsp {
    denoiser: Option<Denoiser>,
    agc: Option<Agc>,
}

impl CaptureDsp {
    /// Active/désactive la suppression de bruit (idempotent).
    pub fn set_noise_suppression(&mut self, enabled: bool) {
        if enabled && self.denoiser.is_none() {
            self.denoiser = Some(Denoiser::new());
        } else if !enabled {
            self.denoiser = None;
        }
    }

    /// Vrai si la suppression de bruit est active.
    pub fn noise_suppression(&self) -> bool {
        self.denoiser.is_some()
    }

    /// Active/désactive le contrôle automatique de gain (idempotent).
    pub fn set_agc(&mut self, enabled: bool) {
        if enabled && self.agc.is_none() {
            self.agc = Some(Agc::default());
        } else if !enabled {
            self.agc = None;
        }
    }

    /// Vrai si l'AGC est actif.
    pub fn agc(&self) -> bool {
        self.agc.is_some()
    }

    /// Applique la chaîne active à une trame de capture en place. Une trame
    /// de taille inattendue traverse inchangée (jamais de panique : la taille
    /// vient de l'hôte, pas du réseau, mais la défense ne coûte rien).
    pub fn process(&mut self, pcm: &mut [i16]) {
        if pcm.len() != FRAME_SAMPLES {
            return;
        }
        if let Some(denoiser) = &mut self.denoiser {
            denoiser.process(pcm);
        }
        if let Some(agc) = &mut self.agc {
            agc.process(pcm);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Trame de « parole » synthétique : alternance ±amplitude (au-dessus du
    /// seuil VAD, RMS = amplitude).
    fn tone(amplitude: i16) -> Vec<i16> {
        (0..FRAME_SAMPLES)
            .map(|i| if i % 2 == 0 { amplitude } else { -amplitude })
            .collect()
    }

    /// Bruit blanc déterministe (LCG, sans dépendance) d'amplitude bornée.
    fn white_noise(seed: &mut u64, amplitude: i32) -> Vec<i16> {
        (0..FRAME_SAMPLES)
            .map(|_| {
                *seed = seed
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                let r = ((*seed >> 33) as i32 % (2 * amplitude + 1)) - amplitude;
                r as i16
            })
            .collect()
    }

    #[test]
    fn agc_boosts_quiet_speech_toward_target() {
        let mut agc = Agc::default();
        let mut last_rms = 0.0;
        for _ in 0..400 {
            let mut frame = tone(400); // RMS 400, bien sous la cible.
            agc.process(&mut frame);
            last_rms = frame_rms(&frame);
        }
        assert!(
            (AGC_TARGET_RMS * 0.8..=AGC_TARGET_RMS * 1.2).contains(&last_rms),
            "RMS final {last_rms} loin de la cible {AGC_TARGET_RMS}"
        );
        assert!(agc.gain() > 1.0);
    }

    #[test]
    fn agc_attenuates_loud_input_quickly() {
        let mut agc = Agc::default();
        let mut last_rms = f32::MAX;
        // Descente rapide : une dizaine de trames (200 ms) suffit.
        for _ in 0..10 {
            let mut frame = tone(20_000);
            agc.process(&mut frame);
            last_rms = frame_rms(&frame);
        }
        assert!(
            last_rms < 8_000.0,
            "source forte insuffisamment atténuée : RMS {last_rms}"
        );
        assert!(agc.gain() < 1.0);
    }

    #[test]
    fn agc_gain_is_bounded_and_silence_does_not_adapt() {
        let mut agc = Agc::default();
        // Une source extrêmement faible ne pousse jamais le gain au-delà de
        // la borne (+12 dB).
        for _ in 0..1_000 {
            let mut frame = tone(80);
            agc.process(&mut frame);
        }
        assert!(agc.gain() <= AGC_GAIN_MAX + f32::EPSILON);
        let boosted = agc.gain();
        // Le silence n'adapte pas le gain (pas d'amplification du bruit de
        // fond entre les mots).
        for _ in 0..200 {
            let mut frame = vec![0i16; FRAME_SAMPLES];
            agc.process(&mut frame);
        }
        assert_eq!(agc.gain(), boosted);
    }

    #[test]
    fn denoiser_attenuates_stationary_white_noise() {
        let mut dsp = CaptureDsp::default();
        dsp.set_noise_suppression(true);
        let mut seed = 42u64;
        let input_amplitude = 3_000i32;
        // Chauffe du modèle récurrent (il apprend le profil du bruit) puis
        // mesure sur les dernières trames : une atténuation nette est exigée
        // (le seuil reste prudent, RNNoise est entraîné sur des bruits réels,
        // pas sur un blanc uniforme synthétique).
        let mut out_rms = 0.0;
        let mut in_rms = 0.0;
        for i in 0..300 {
            let mut frame = white_noise(&mut seed, input_amplitude);
            let before = frame_rms(&frame);
            dsp.process(&mut frame);
            if i >= 200 {
                in_rms += before;
                out_rms += frame_rms(&frame);
            }
        }
        assert!(
            out_rms < in_rms * 0.7,
            "bruit blanc insuffisamment supprimé : {out_rms} vs {in_rms}"
        );
    }

    #[test]
    fn disabled_dsp_is_a_no_op_and_toggles_are_idempotent() {
        let mut dsp = CaptureDsp::default();
        assert!(!dsp.noise_suppression() && !dsp.agc());
        let mut frame = tone(12_345);
        let original = frame.clone();
        dsp.process(&mut frame);
        assert_eq!(frame, original, "chaîne vide : trame inchangée");

        dsp.set_agc(true);
        dsp.set_agc(true);
        assert!(dsp.agc());
        dsp.set_noise_suppression(true);
        dsp.set_noise_suppression(false);
        assert!(!dsp.noise_suppression());

        // Taille de trame inattendue : traversée inchangée, pas de panique.
        let mut short = vec![100i16; 7];
        dsp.process(&mut short);
        assert_eq!(short, vec![100i16; 7]);
    }

    /// Mesure du coût CPU (documentée dans l'en-tête du module) :
    /// `cargo test -p accord-voice --release -- --ignored --nocapture`.
    #[test]
    #[ignore = "mesure manuelle du coût CPU (release)"]
    fn cpu_cost_measurement() {
        let mut dsp = CaptureDsp::default();
        dsp.set_noise_suppression(true);
        dsp.set_agc(true);
        let mut seed = 7u64;
        let frames: Vec<Vec<i16>> = (0..1_000).map(|_| white_noise(&mut seed, 5_000)).collect();
        let start = std::time::Instant::now();
        let mut sink = 0i64;
        for frame in &frames {
            let mut f = frame.clone();
            dsp.process(&mut f);
            sink += i64::from(f[0]);
        }
        let elapsed = start.elapsed();
        let per_frame = elapsed / 1_000;
        // Une trame dure 20 ms : le budget temps réel est très large.
        println!("DSP complet : {per_frame:?}/trame (budget 20 ms), sink={sink}");
        assert!(per_frame < std::time::Duration::from_millis(20));
    }
}
