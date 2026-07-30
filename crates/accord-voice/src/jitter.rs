//! Tampon de gigue adaptatif (SPEC §8).
//!
//! Réordonne les trames par `seq`, absorbe la gigue réseau et signale les
//! trous au décodeur (PLC). La profondeur cible suit le p95 des intervalles
//! d'inter-arrivée, plus une trame de marge, bornée à [40 ms, 200 ms]
//! (2 à 10 trames de 20 ms). Une trame en retard au-delà du tampon est
//! abandonnée (jouer tard est pire que dissimuler).

use std::collections::{BTreeMap, VecDeque};

use crate::params::{JITTER_MAX_FRAMES, JITTER_MIN_FRAMES};

/// Fenêtre d'estimation du p95 (nombre d'inter-arrivées conservées).
const RTT_WINDOW: usize = 128;

/// Nombre minimal d'inter-arrivées avant d'estimer un p95.
const RTT_MIN_SAMPLES: usize = 8;

/// Trames conservées au plus dans le tampon (≈ 1,6 s de son).
///
/// 🔒 Le tampon était borné par la seule discipline de l'émetteur : les trames
/// sont indexées par un `seq` sur 16 bits fourni par le pair, et seules celles
/// DÉJÀ JOUÉES étaient écartées. Un participant hostile — ou une pile qui
/// déraille — pouvait donc y verser jusqu'à 32 767 trames d'avance, chacune
/// portant une charge utile de l'ordre du kibioctet : plusieurs dizaines de
/// mébioctets PAR participant, sans qu'aucune ne soit jamais jouée (le trou en
/// tête bloque la lecture). Pendant l'amorçage, où aucune séquence de référence
/// n'existe encore, toute trame était acceptée sans condition.
///
/// Le plafond vaut huit fois la profondeur cible maximale : très au-delà de
/// toute gigue ou de tout désordre réels, et sans commune mesure avec ce qu'un
/// flot hostile visait.
pub const MAX_BUFFERED_FRAMES: usize = 8 * JITTER_MAX_FRAMES;

/// Distance signée `b − a` en arithmétique circulaire 16 bits.
fn seq_after(a: u16, b: u16) -> bool {
    (b.wrapping_sub(a) as i16) > 0
}

/// Résultat de la lecture d'une trame de sortie.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Playout {
    /// Trame disponible : paquet encodé à décoder.
    Frame(Vec<u8>),
    /// Trou : le décodeur doit produire une dissimulation (PLC).
    Conceal,
    /// Tampon en amorçage : rien à jouer encore.
    Starved,
}

/// Tampon de gigue pour un flux entrant.
#[derive(Debug)]
pub struct JitterBuffer {
    frames: BTreeMap<u16, Vec<u8>>,
    next_seq: Option<u16>,
    target_frames: usize,
    interarrivals: VecDeque<u32>,
    /// Tampon de travail du calcul de p95, réutilisé d'une trame à l'autre :
    /// le chemin voix passe ici à chaque paquet reçu et par participant, une
    /// allocation par paquet n'y a rien à faire.
    p95_scratch: Vec<u32>,
    last_arrival_ms: Option<u32>,
    priming: bool,
}

impl Default for JitterBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl JitterBuffer {
    /// Nouveau tampon vide en phase d'amorçage.
    pub fn new() -> Self {
        Self {
            frames: BTreeMap::new(),
            next_seq: None,
            target_frames: JITTER_MIN_FRAMES,
            interarrivals: VecDeque::with_capacity(RTT_WINDOW),
            p95_scratch: Vec::with_capacity(RTT_WINDOW),
            last_arrival_ms: None,
            priming: true,
        }
    }

    /// Profondeur cible courante (trames).
    pub fn target_frames(&self) -> usize {
        self.target_frames
    }

    /// Nombre de trames actuellement en tampon.
    pub fn buffered(&self) -> usize {
        self.frames.len()
    }

    /// Insère une trame reçue. Les trames trop en retard (déjà jouées) sont
    /// ignorées, et le tampon reste borné à [`MAX_BUFFERED_FRAMES`] : au
    /// plafond, la trame entrante est simplement jetée.
    ///
    /// Jeter l'entrante plutôt qu'évincer une trame déjà en place est le choix
    /// conservateur, et il n'a aucun coût d'équité : le tampon est PROPRE à un
    /// participant, personne d'autre ne peut le remplir. Un pair qui inonde ne
    /// dégrade donc que sa propre voix — et sa mémoire chez nous est bornée.
    pub fn push(&mut self, seq: u16, payload: Vec<u8>, now_ms: u32) {
        self.update_target(now_ms);
        if let Some(next) = self.next_seq {
            // Déjà passée : trop tard pour être jouée.
            if seq != next && !seq_after(next.wrapping_sub(1), seq) {
                return;
            }
        }
        if self.frames.len() >= MAX_BUFFERED_FRAMES && !self.frames.contains_key(&seq) {
            return;
        }
        self.frames.insert(seq, payload);
    }

    /// Produit la prochaine trame de sortie (cadence de 20 ms). Reste en
    /// amorçage tant que la profondeur cible n'est pas atteinte.
    pub fn pop(&mut self) -> Playout {
        if self.priming {
            if self.frames.len() < self.target_frames {
                return Playout::Starved;
            }
            self.priming = false;
            // Démarre la lecture à la plus petite séquence disponible.
            self.next_seq = self.frames.keys().next().copied();
        }
        let Some(next) = self.next_seq else {
            return Playout::Starved;
        };
        self.next_seq = Some(next.wrapping_add(1));
        match self.frames.remove(&next) {
            Some(payload) => Playout::Frame(payload),
            None => {
                // Trou : si rien n'arrive et que le tampon se vide, on
                // ré-amorce pour se resynchroniser.
                if self.frames.is_empty() {
                    self.priming = true;
                }
                Playout::Conceal
            }
        }
    }

    /// Paquet de la PROCHAINE séquence à jouer, s'il est déjà en tampon.
    /// Après un [`Playout::Conceal`] sur la trame `n`, c'est le paquet `n+1` :
    /// sa FEC in-band permet de reconstruire la trame perdue (le paquet reste
    /// en tampon et sera joué normalement au tour suivant).
    pub fn peek_next(&self) -> Option<&[u8]> {
        self.frames
            .get(&self.next_seq?)
            .map(|payload| payload.as_slice())
    }

    /// Met à jour la profondeur cible d'après le p95 des inter-arrivées.
    fn update_target(&mut self, now_ms: u32) {
        if let Some(last) = self.last_arrival_ms {
            let delta = now_ms.saturating_sub(last);
            if self.interarrivals.len() == RTT_WINDOW {
                self.interarrivals.pop_front();
            }
            self.interarrivals.push_back(delta);
        }
        self.last_arrival_ms = Some(now_ms);

        if self.interarrivals.len() < RTT_MIN_SAMPLES {
            return;
        }
        // Tampon réutilisé + sélection partielle : `select_nth_unstable` place
        // à `idx` l'élément qu'un tri complet y aurait mis, donc le p95 est
        // identique au centile près, sans trier la fenêtre entière ni allouer.
        self.p95_scratch.clear();
        self.p95_scratch.extend(self.interarrivals.iter().copied());
        let idx = (self.p95_scratch.len() * 95 / 100).min(self.p95_scratch.len() - 1);
        let (_, &mut p95, _) = self.p95_scratch.select_nth_unstable(idx);
        // Cible = p95 + une trame de marge, exprimée en trames de 20 ms.
        let frames = (p95 as usize / crate::params::FRAME_MS as usize) + 1;
        self.target_frames = frames.clamp(JITTER_MIN_FRAMES, JITTER_MAX_FRAMES);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pkt(n: u8) -> Vec<u8> {
        vec![n; 4]
    }

    #[test]
    fn primes_then_plays_in_order() {
        let mut jb = JitterBuffer::new();
        assert_eq!(jb.pop(), Playout::Starved);
        jb.push(0, pkt(0), 0);
        jb.push(1, pkt(1), 20);
        // Profondeur cible min = 2 trames : dès 2 en tampon, lecture démarre.
        assert_eq!(jb.pop(), Playout::Frame(pkt(0)));
        assert_eq!(jb.pop(), Playout::Frame(pkt(1)));
    }

    #[test]
    fn reorders_out_of_order_arrivals() {
        let mut jb = JitterBuffer::new();
        jb.push(1, pkt(1), 0);
        jb.push(0, pkt(0), 5);
        assert_eq!(jb.pop(), Playout::Frame(pkt(0)));
        assert_eq!(jb.pop(), Playout::Frame(pkt(1)));
    }

    #[test]
    fn missing_frame_yields_conceal() {
        let mut jb = JitterBuffer::new();
        jb.push(0, pkt(0), 0);
        jb.push(2, pkt(2), 20);
        assert_eq!(jb.pop(), Playout::Frame(pkt(0)));
        // seq 1 manquant → dissimulation.
        assert_eq!(jb.pop(), Playout::Conceal);
        assert_eq!(jb.pop(), Playout::Frame(pkt(2)));
    }

    #[test]
    fn peek_next_rend_le_successeur_du_trou_sans_le_consommer() {
        let mut jb = JitterBuffer::new();
        jb.push(0, pkt(0), 0);
        jb.push(2, pkt(2), 20);
        assert_eq!(jb.pop(), Playout::Frame(pkt(0)));
        assert_eq!(jb.pop(), Playout::Conceal);
        // Le paquet 2 (successeur du trou) est visible pour la FEC…
        assert_eq!(jb.peek_next(), Some(pkt(2).as_slice()));
        // …et reste en tampon : il est joué normalement ensuite.
        assert_eq!(jb.pop(), Playout::Frame(pkt(2)));
        assert_eq!(jb.peek_next(), None, "plus rien en tampon");
    }

    #[test]
    fn late_frame_is_dropped() {
        let mut jb = JitterBuffer::new();
        jb.push(5, pkt(5), 0);
        jb.push(6, pkt(6), 20);
        assert_eq!(jb.pop(), Playout::Frame(pkt(5)));
        // seq 4 arrive trop tard (déjà après la lecture de 5).
        jb.push(4, pkt(4), 40);
        assert!(!jb.frames.contains_key(&4));
    }

    #[test]
    fn un_flot_de_sequences_lointaines_ne_fait_pas_enfler_le_tampon() {
        // 🔒 Le tampon est indexé par un `seq` sur 16 bits fourni par le pair.
        // Sans plafond, un participant hostile y versait jusqu'à 32 767 trames
        // d'avance — plusieurs dizaines de mébioctets qui n'étaient jamais
        // jouées, le trou en tête bloquant la lecture.
        let mut jb = JitterBuffer::new();
        for seq in 0..20_000u16 {
            jb.push(seq, vec![0u8; 1_000], seq as u32 * 20);
        }
        assert!(
            jb.buffered() <= MAX_BUFFERED_FRAMES,
            "tampon non borné : {} trames",
            jb.buffered()
        );

        // Le trafic nominal reste intact : la lecture démarre et se poursuit
        // dans l'ordre malgré l'inondation qui précède.
        let mut jb = JitterBuffer::new();
        for seq in 0..5u16 {
            jb.push(seq, pkt(seq as u8), seq as u32 * 20);
        }
        assert_eq!(jb.pop(), Playout::Frame(pkt(0)));
        assert_eq!(jb.pop(), Playout::Frame(pkt(1)));
    }

    #[test]
    fn target_depth_grows_with_jitter_and_stays_bounded() {
        let mut jb = JitterBuffer::new();
        // Inter-arrivées très irrégulières (jusqu'à 160 ms).
        for i in 0..40u16 {
            let now = i as u32 * 160;
            jb.push(i, pkt(i as u8), now);
        }
        assert!(jb.target_frames() >= JITTER_MIN_FRAMES);
        assert!(jb.target_frames() <= JITTER_MAX_FRAMES);
        assert!(
            jb.target_frames() > JITTER_MIN_FRAMES,
            "gigue forte ⇒ tampon plus profond"
        );
    }
}
