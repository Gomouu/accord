//! Vidéo sélective (feuille de route §9.1) : ne transmettre un flux vidéo
//! qu'aux participants qui l'affichent réellement.
//!
//! À dix en visio, chacun émettait neuf flux et en recevait neuf ; un flux non
//! peint était quand même reçu, réassemblé et DÉCODÉ. Le récepteur déclare donc
//! à chaque émetteur ce qu'il n'affiche pas de lui
//! ([`accord_proto::plaintext::VoiceMsg::VideoInterest`]), et l'émetteur cesse
//! d'envoyer ces flux-là.
//!
//! Tout ce module tient sur un seul principe : **se taire ne doit jamais couper
//! un flux.** Un pair d'une version antérieure ne déclare rien ; un datagramme
//! best-effort peut se perdre ; l'UI peut n'avoir pas encore rendu. Dans les
//! trois cas le flux doit continuer. D'où :
//!
//! - le défaut, chez l'émetteur, est « j'envoie tout » ([`HiddenByPeer::wants`]
//!   rend `true` en l'absence d'entrée) ;
//! - une déclaration reçue **expire** ([`INTEREST_TTL`]) : elle n'est vraie que
//!   tant qu'on la réaffirme. Une reprise d'affichage dont le message se perd
//!   coûte au pire une expiration de retard, jamais une tuile noire définitive ;
//! - côté récepteur, [`DeclaredHidden`] ne réémet que les masques NON NULS. Ne
//!   plus rien masquer, c'est se taire — et se taire rétablit tout.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use accord_proto::plaintext::{VIDEO_HIDE_CAMERA, VIDEO_HIDE_SCREEN};

use super::media::VideoStream;

/// Durée de validité d'une déclaration d'intérêt reçue. Volontairement
/// plusieurs fois le pas de réaffirmation ([`REFRESH_PERIOD`]) : quelques
/// datagrammes perdus d'affilée ne doivent pas rallumer un flux que l'on
/// masque encore.
pub(crate) const INTEREST_TTL: Duration = Duration::from_secs(10);

/// Pas de réaffirmation des masques non nuls, côté récepteur.
pub(crate) const REFRESH_PERIOD: Duration = Duration::from_secs(3);

/// Bit du masque `hidden` correspondant à un flux.
pub(crate) fn stream_bit(stream: VideoStream) -> u8 {
    match stream {
        VideoStream::Camera => VIDEO_HIDE_CAMERA,
        VideoStream::Screen => VIDEO_HIDE_SCREEN,
    }
}

/// Côté ÉMETTEUR : ce que chaque destinataire a déclaré ne pas afficher de nos
/// flux, avec l'instant de réception (l'entrée expire — voir l'en-tête).
#[derive(Default)]
pub(crate) struct HiddenByPeer {
    entries: HashMap<[u8; 32], (u8, Instant)>,
}

impl HiddenByPeer {
    /// Enregistre la déclaration de `peer`. Un masque nul n'est pas conservé :
    /// « je n'affiche rien de masqué » est exactement l'état par défaut, et le
    /// garder ne ferait que retarder l'oubli.
    pub(crate) fn note(&mut self, peer: [u8; 32], hidden: u8, now: Instant) {
        if hidden == 0 {
            self.entries.remove(&peer);
            return;
        }
        self.entries.insert(peer, (hidden, now));
    }

    /// Vrai si `peer` doit recevoir le flux `stream`. Rend `true` par défaut :
    /// pair inconnu (jamais déclaré, version antérieure) ou déclaration
    /// périmée. C'est le point où le principe « se taire n'éteint rien » est
    /// réellement appliqué.
    pub(crate) fn wants(&self, peer: &[u8; 32], stream: VideoStream, now: Instant) -> bool {
        let Some((hidden, at)) = self.entries.get(peer) else {
            return true;
        };
        if now.duration_since(*at) >= INTEREST_TTL {
            return true;
        }
        hidden & stream_bit(stream) == 0
    }

    /// Oublie la déclaration de `peer` (il (re)joint la session : son UI repart
    /// de zéro, et une suppression héritée de son passage précédent lui vaudrait
    /// une tuile noire jusqu'à expiration).
    pub(crate) fn forget(&mut self, peer: &[u8; 32]) {
        self.entries.remove(peer);
    }

    /// Oublie tout (fin de session).
    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }
}

/// Côté RÉCEPTEUR : ce que l'on a déclaré à chaque émetteur. Ne retient que les
/// masques non nuls — un masque nul, une fois émis, n'a plus rien à maintenir.
#[derive(Default)]
pub(crate) struct DeclaredHidden {
    sent: HashMap<[u8; 32], u8>,
}

impl DeclaredHidden {
    /// Confronte l'état voulu (`wanted`, tel que calculé par l'UI) à ce qui a
    /// déjà été déclaré, et rend les seules déclarations à émettre. Les pairs
    /// qui sortent de `wanted` reçoivent un masque nul explicite : c'est le
    /// chemin RAPIDE du rétablissement (l'expiration en est le filet).
    pub(crate) fn apply(&mut self, wanted: &[([u8; 32], u8)]) -> Vec<([u8; 32], u8)> {
        let mut out = Vec::new();
        for (peer, hidden) in wanted {
            if self.sent.get(peer).copied().unwrap_or(0) != *hidden {
                out.push((*peer, *hidden));
            }
        }
        // Pairs dont on ne masque plus rien : masque nul explicite, puis oubli.
        let cleared: Vec<[u8; 32]> = self
            .sent
            .keys()
            .filter(|peer| !wanted.iter().any(|(p, _)| p == *peer))
            .copied()
            .collect();
        for peer in cleared {
            out.push((peer, 0));
        }
        self.sent = wanted
            .iter()
            .filter(|(_, hidden)| *hidden != 0)
            .map(|(peer, hidden)| (*peer, *hidden))
            .collect();
        out
    }

    /// Masques à réaffirmer périodiquement. Vide dès que l'on n'occulte plus
    /// rien : le silence est alors la déclaration correcte.
    pub(crate) fn refresh(&self) -> Vec<([u8; 32], u8)> {
        self.sent.iter().map(|(p, h)| (*p, *h)).collect()
    }

    /// Oublie tout (fin de session) : la session suivante repart d'un état où
    /// rien n'est masqué.
    pub(crate) fn clear(&mut self) {
        self.sent.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: [u8; 32] = [0xAA; 32];
    const B: [u8; 32] = [0xBB; 32];

    #[test]
    fn an_undeclared_peer_receives_everything() {
        // Le cœur du contrat : un pair qui n'a jamais rien dit (client plus
        // ancien, message perdu, UI pas encore rendue) reçoit tout.
        let table = HiddenByPeer::default();
        let now = Instant::now();
        assert!(table.wants(&A, VideoStream::Camera, now));
        assert!(table.wants(&A, VideoStream::Screen, now));
    }

    #[test]
    fn hiding_one_stream_leaves_the_other_flowing() {
        // Cas de l'épinglage : on regarde l'écran de quelqu'un, pas sa caméra.
        let mut table = HiddenByPeer::default();
        let now = Instant::now();
        table.note(A, VIDEO_HIDE_CAMERA, now);
        assert!(!table.wants(&A, VideoStream::Camera, now));
        assert!(table.wants(&A, VideoStream::Screen, now));
    }

    #[test]
    fn hiding_never_leaks_to_another_peer() {
        let mut table = HiddenByPeer::default();
        let now = Instant::now();
        table.note(A, VIDEO_HIDE_CAMERA | VIDEO_HIDE_SCREEN, now);
        assert!(!table.wants(&A, VideoStream::Camera, now));
        assert!(table.wants(&B, VideoStream::Camera, now));
    }

    #[test]
    fn a_declaration_expires_and_the_stream_comes_back_by_itself() {
        // Filet de sécurité : si le message de reprise se perd, l'expiration
        // rallume le flux toute seule.
        let mut table = HiddenByPeer::default();
        let start = Instant::now();
        table.note(A, VIDEO_HIDE_CAMERA, start);
        assert!(!table.wants(&A, VideoStream::Camera, start + INTEREST_TTL / 2));
        assert!(table.wants(&A, VideoStream::Camera, start + INTEREST_TTL));
    }

    #[test]
    fn a_refreshed_declaration_keeps_the_stream_off() {
        let mut table = HiddenByPeer::default();
        let start = Instant::now();
        table.note(A, VIDEO_HIDE_CAMERA, start);
        let later = start + INTEREST_TTL - REFRESH_PERIOD;
        table.note(A, VIDEO_HIDE_CAMERA, later);
        // Passé le TTL de la PREMIÈRE déclaration, la réaffirmation tient.
        assert!(!table.wants(&A, VideoStream::Camera, start + INTEREST_TTL));
    }

    #[test]
    fn a_zero_mask_restores_everything_immediately() {
        let mut table = HiddenByPeer::default();
        let now = Instant::now();
        table.note(A, VIDEO_HIDE_CAMERA, now);
        table.note(A, 0, now);
        assert!(table.wants(&A, VideoStream::Camera, now));
    }

    #[test]
    fn unknown_mask_bits_hide_nothing_known() {
        // Un bit d'un flux futur ne doit pas éteindre la caméra ni l'écran.
        let mut table = HiddenByPeer::default();
        let now = Instant::now();
        table.note(A, 0x80, now);
        assert!(table.wants(&A, VideoStream::Camera, now));
        assert!(table.wants(&A, VideoStream::Screen, now));
    }

    #[test]
    fn rejoining_peer_is_forgotten() {
        let mut table = HiddenByPeer::default();
        let now = Instant::now();
        table.note(A, VIDEO_HIDE_CAMERA, now);
        table.forget(&A);
        assert!(table.wants(&A, VideoStream::Camera, now));
    }

    #[test]
    fn declarations_are_sent_once_then_only_on_change() {
        let mut declared = DeclaredHidden::default();
        assert_eq!(
            declared.apply(&[(A, VIDEO_HIDE_CAMERA)]),
            vec![(A, VIDEO_HIDE_CAMERA)]
        );
        // Même état : rien à réémettre (les trames arrivent à 24 Hz, on ne
        // déclare pas à chaque re-rendu).
        assert!(declared.apply(&[(A, VIDEO_HIDE_CAMERA)]).is_empty());
        // Changement : une seule déclaration.
        assert_eq!(
            declared.apply(&[(A, VIDEO_HIDE_SCREEN)]),
            vec![(A, VIDEO_HIDE_SCREEN)]
        );
    }

    #[test]
    fn dropping_a_peer_from_the_wanted_set_sends_an_explicit_zero() {
        // Rétablissement RAPIDE : on ne se contente pas d'attendre l'expiration.
        let mut declared = DeclaredHidden::default();
        declared.apply(&[(A, VIDEO_HIDE_CAMERA)]);
        assert_eq!(declared.apply(&[]), vec![(A, 0)]);
        // Et l'on cesse de réaffirmer : le silence vaut « envoie-moi tout ».
        assert!(declared.refresh().is_empty());
    }

    #[test]
    fn only_non_zero_masks_are_refreshed() {
        let mut declared = DeclaredHidden::default();
        declared.apply(&[(A, VIDEO_HIDE_CAMERA), (B, 0)]);
        assert_eq!(declared.refresh(), vec![(A, VIDEO_HIDE_CAMERA)]);
    }

    #[test]
    fn clearing_forgets_every_declaration() {
        let mut declared = DeclaredHidden::default();
        declared.apply(&[(A, VIDEO_HIDE_CAMERA)]);
        declared.clear();
        assert!(declared.refresh().is_empty());
        // Après une fin de session, la même déclaration est de nouveau émise.
        assert_eq!(
            declared.apply(&[(A, VIDEO_HIDE_CAMERA)]),
            vec![(A, VIDEO_HIDE_CAMERA)]
        );
    }
}
