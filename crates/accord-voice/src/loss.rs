//! Estimation de perte par trous de séquence sur fenêtre glissante (SPEC §8).
//!
//! Chaque trame reçue porte un `seq: u16` monotone (avec bouclage). Sur une
//! fenêtre de 5 s, la perte = `1 − reçues / attendues`, où « attendues » est
//! l'étendue entre le plus petit et le plus grand `seq` observés. Le bouclage
//! 16 bits est géré par comparaison en distance signée.

use std::collections::VecDeque;

/// Fenêtre de mesure (ms).
const WINDOW_MS: u32 = 5_000;

/// Distance signée `b − a` en arithmétique circulaire 16 bits.
fn seq_diff(a: u16, b: u16) -> i32 {
    (b.wrapping_sub(a)) as i16 as i32
}

/// Estimateur de perte pour un flux entrant.
#[derive(Debug, Default)]
pub struct LossEstimator {
    /// `(seq, arrivée_ms)` dans la fenêtre, ordre d'arrivée.
    seen: VecDeque<(u16, u32)>,
}

impl LossEstimator {
    /// Nouvel estimateur vide.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enregistre l'arrivée d'une trame et purge la fenêtre.
    pub fn observe(&mut self, seq: u16, now_ms: u32) {
        self.seen.push_back((seq, now_ms));
        let horizon = now_ms.saturating_sub(WINDOW_MS);
        while let Some(&(_, t)) = self.seen.front() {
            if t < horizon {
                self.seen.pop_front();
            } else {
                break;
            }
        }
    }

    /// Perte estimée en pourcent (0–100) sur la fenêtre courante.
    pub fn loss_pct(&self) -> u8 {
        if self.seen.len() < 2 {
            return 0;
        }
        // Étendue = distance max entre séquences observées, en suivant l'ordre
        // d'arrivée pour rester robuste au bouclage.
        let mut min_off: i32 = 0;
        let mut max_off: i32 = 0;
        // `seen.len() >= 2` vérifié ci-dessus : repli neutre plutôt que
        // panique dans le chemin voix (D23).
        let Some(&(base, _)) = self.seen.front() else {
            return 0;
        };
        for &(seq, _) in &self.seen {
            let off = seq_diff(base, seq);
            min_off = min_off.min(off);
            max_off = max_off.max(off);
        }
        let expected = (max_off - min_off + 1) as usize;
        let received = self.seen.len();
        if expected <= received {
            return 0;
        }
        let lost = expected - received;
        ((lost * 100) / expected).min(100) as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_loss_when_contiguous() {
        let mut est = LossEstimator::new();
        for seq in 0..50u16 {
            est.observe(seq, seq as u32 * 20);
        }
        assert_eq!(est.loss_pct(), 0);
    }

    #[test]
    fn detects_dropped_frames() {
        let mut est = LossEstimator::new();
        // Reçoit 1 trame sur 2 : ~50 % de perte.
        for i in 0..50u16 {
            est.observe(i * 2, (i as u32) * 40);
        }
        let loss = est.loss_pct();
        assert!((45..=55).contains(&loss), "perte estimée = {loss}");
    }

    #[test]
    fn window_forgets_old_frames() {
        let mut est = LossEstimator::new();
        est.observe(0, 0);
        est.observe(100, 100); // gros trou mais ancien
                               // Bien après la fenêtre, une rafale propre.
        for i in 0..10u16 {
            est.observe(200 + i, 10_000 + i as u32 * 20);
        }
        assert_eq!(est.loss_pct(), 0);
    }

    #[test]
    fn handles_seq_wraparound() {
        let mut est = LossEstimator::new();
        let seqs = [65_533u16, 65_534, 65_535, 0, 1, 2];
        for (i, &s) in seqs.iter().enumerate() {
            est.observe(s, i as u32 * 20);
        }
        assert_eq!(est.loss_pct(), 0);
    }
}
