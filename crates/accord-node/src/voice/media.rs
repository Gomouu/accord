//! Fragmentation et réassemblage temps réel des flux vidéo d'un appel :
//! partage d'écran (v5) et caméra (v6).
//!
//! L'émetteur découpe chaque trame vidéo encodée (WebCodecs, côté UI) en
//! tranches ≤ [`CHUNK`], portées par une trame vidéo du canal VOICE — chacune un
//! unique datagramme UDP jamais refragmenté par le transport. Le récepteur
//! réassemble par `frame_id` et ne garde QUE la trame la plus récente : une
//! trame incomplète est abandonnée dès qu'une plus récente arrive (sémantique
//! temps réel), et un fragment perdu jette la trame — on attend la keyframe
//! suivante. Ce réassembleur dédié évite de saturer le réassembleur généraliste
//! du transport (8 slots, timeout 30 s) inadapté à un flux vidéo.
//!
//! Les deux flux sont indépendants de bout en bout (variantes filaires
//! distinctes, réassembleurs distincts) : on peut se montrer ET partager son
//! écran dans le même appel sans que les trames se mélangent.

use accord_proto::plaintext::{VoiceMsg, VIDEO_FLAG_KEYFRAME};

/// Taille cible d'une tranche encodée. Reste sous `MAX_VIDEO_FRAGMENT` du proto
/// (1200) pour que la trame scellée tienne dans un unique datagramme UDP.
const CHUNK: usize = 1000;

/// Borne anti-DoS d'une trame réassemblée (une keyframe raisonnable reste très
/// en dessous).
const MAX_FRAME_BYTES: usize = 512 * 1024;

/// Nombre maximal de fragments accepté pour une trame (borne mémoire du
/// réassembleur, alignée sur [`MAX_FRAME_BYTES`]).
const MAX_FRAGS: u16 = 640;

/// Flux vidéo d'un appel : chacun a sa variante filaire et son réassembleur.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum VideoStream {
    /// Partage d'écran (v5).
    Screen,
    /// Caméra (v6).
    Camera,
}

/// Découpe une trame encodée en trames filaires du flux `stream`, prêtes à
/// émettre. Une trame vide produit un unique fragment vide.
pub(crate) fn fragment(
    stream: VideoStream,
    room: [u8; 16],
    frame_id: u32,
    keyframe: bool,
    encoded: &[u8],
) -> Vec<VoiceMsg> {
    let flags = if keyframe { VIDEO_FLAG_KEYFRAME } else { 0 };
    let slices: Vec<&[u8]> = if encoded.is_empty() {
        vec![&[]]
    } else {
        encoded.chunks(CHUNK).collect()
    };
    let frag_count = slices.len().min(MAX_FRAGS as usize) as u16;
    slices
        .into_iter()
        .take(frag_count as usize)
        .enumerate()
        .map(|(idx, slice)| {
            let frag_idx = idx as u16;
            let payload = slice.to_vec();
            match stream {
                VideoStream::Screen => VoiceMsg::ScreenFrame {
                    room,
                    frame_id,
                    frag_count,
                    frag_idx,
                    flags,
                    payload,
                },
                VideoStream::Camera => VoiceMsg::CameraFrame {
                    room,
                    frame_id,
                    frag_count,
                    frag_idx,
                    flags,
                    payload,
                },
            }
        })
        .collect()
}

/// Vrai si `candidate` est une trame plus récente que `current` (comparaison
/// circulaire sur u32 : tolère le rebouclage du compteur d'émission).
fn is_newer(candidate: u32, current: u32) -> bool {
    candidate != current && candidate.wrapping_sub(current) < u32::MAX / 2
}

/// Trame en cours de réassemblage.
struct Frame {
    /// Identifiant de la trame.
    frame_id: u32,
    /// Nombre de fragments attendus.
    frag_count: u16,
    /// La trame porte au moins un fragment keyframe.
    keyframe: bool,
    /// Fragments reçus (`None` = manquant), indexés par `frag_idx`.
    parts: Vec<Option<Vec<u8>>>,
    /// Nombre de fragments déjà reçus.
    have: u16,
    /// Total d'octets accumulés (borne anti-DoS).
    bytes: usize,
}

/// Réassembleur d'un flux de partage d'écran (une trame en cours au plus).
/// `last` mémorise l'identifiant de la trame la plus récente observée (même
/// après complétion) : les fragments retardataires d'une trame plus ancienne
/// sont ignorés.
#[derive(Default)]
pub(crate) struct Reassembler {
    /// Identifiant de la trame la plus récente observée (complétée ou non).
    last: Option<u32>,
    /// Trame en cours de réassemblage, le cas échéant.
    current: Option<Frame>,
}

impl Reassembler {
    /// Ingère un fragment. Rend `Some((trame_encodée, keyframe))` dès que la
    /// trame courante est complète ; `None` sinon (fragment ignoré, doublon,
    /// trame incomplète, ou fragment d'une trame plus ancienne que la dernière
    /// observée).
    pub(crate) fn push(
        &mut self,
        frame_id: u32,
        frag_count: u16,
        frag_idx: u16,
        keyframe: bool,
        payload: Vec<u8>,
    ) -> Option<(Vec<u8>, bool)> {
        if frag_count == 0 || frag_count > MAX_FRAGS || frag_idx >= frag_count {
            return None;
        }

        let start_new = match self.last {
            None => true,
            Some(last) if frame_id == last => match &self.current {
                // Reprise de la trame la plus récente si encore en cours et
                // cohérente ; sinon (déjà complétée, ou `frag_count` incohérent)
                // le fragment est ignoré.
                Some(cur) if cur.frame_id == frame_id && cur.frag_count == frag_count => false,
                _ => return None,
            },
            // Trame strictement plus récente : on repart de zéro.
            Some(last) if is_newer(frame_id, last) => true,
            // Fragment retardataire d'une trame plus ancienne : ignoré.
            Some(_) => return None,
        };

        if start_new {
            self.current = Some(Frame {
                frame_id,
                frag_count,
                keyframe: false,
                parts: (0..frag_count as usize).map(|_| None).collect(),
                have: 0,
                bytes: 0,
            });
            self.last = Some(frame_id);
        }

        let cur = self.current.as_mut()?;
        let idx = frag_idx as usize;
        match cur.parts.get(idx) {
            // Doublon : on note une éventuelle keyframe mais on ne recompte pas.
            Some(Some(_)) => {
                cur.keyframe |= keyframe;
                return None;
            }
            Some(None) => {}
            None => return None,
        }

        // Borne anti-DoS sur la taille cumulée de la trame.
        if cur.bytes.saturating_add(payload.len()) > MAX_FRAME_BYTES {
            self.current = None;
            return None;
        }

        cur.bytes += payload.len();
        cur.have += 1;
        cur.keyframe |= keyframe;
        if let Some(slot) = cur.parts.get_mut(idx) {
            *slot = Some(payload);
        }

        if cur.have == cur.frag_count {
            let keyframe = cur.keyframe;
            let mut out = Vec::with_capacity(cur.bytes);
            for slot in std::mem::take(&mut cur.parts) {
                match slot {
                    Some(bytes) => out.extend_from_slice(&bytes),
                    // Ne peut pas arriver (`have == frag_count`) ; par prudence,
                    // on abandonne proprement plutôt que de paniquer.
                    None => {
                        self.current = None;
                        return None;
                    }
                }
            }
            // Trame complétée ; `last` conservé pour ignorer les retardataires.
            self.current = None;
            return Some((out, keyframe));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts_of(msgs: &[VoiceMsg]) -> Vec<(u32, u16, u16, bool, Vec<u8>)> {
        msgs.iter()
            .map(|m| match m {
                VoiceMsg::ScreenFrame {
                    frame_id,
                    frag_count,
                    frag_idx,
                    flags,
                    payload,
                    ..
                }
                | VoiceMsg::CameraFrame {
                    frame_id,
                    frag_count,
                    frag_idx,
                    flags,
                    payload,
                    ..
                } => (
                    *frame_id,
                    *frag_count,
                    *frag_idx,
                    flags & VIDEO_FLAG_KEYFRAME != 0,
                    payload.clone(),
                ),
                _ => panic!("fragment attendu"),
            })
            .collect()
    }

    #[test]
    fn fragment_then_reassemble_roundtrips() {
        let data: Vec<u8> = (0..2500u32).map(|i| i as u8).collect();
        let msgs = fragment(VideoStream::Screen, [7; 16], 1, true, &data);
        // 2500 / 1000 = 3 fragments.
        assert_eq!(msgs.len(), 3);
        let mut r = Reassembler::default();
        let mut done = None;
        for (fid, fc, fi, kf, pl) in parts_of(&msgs) {
            if let Some(out) = r.push(fid, fc, fi, kf, pl) {
                done = Some(out);
            }
        }
        assert_eq!(done, Some((data, true)));
    }

    #[test]
    fn single_fragment_frame_completes_immediately() {
        let msgs = fragment(VideoStream::Screen, [0; 16], 5, false, b"hello");
        assert_eq!(msgs.len(), 1);
        let (fid, fc, fi, kf, pl) = parts_of(&msgs).remove(0);
        let mut r = Reassembler::default();
        assert_eq!(
            r.push(fid, fc, fi, kf, pl),
            Some((b"hello".to_vec(), false))
        );
    }

    #[test]
    fn lost_fragment_drops_the_frame() {
        let data: Vec<u8> = (0..2500u32).map(|i| i as u8).collect();
        let msgs = parts_of(&fragment(VideoStream::Screen, [1; 16], 10, true, &data));
        let mut r = Reassembler::default();
        // On saute le fragment du milieu (idx 1) : la trame ne se complète pas.
        assert_eq!(
            r.push(
                msgs[0].0,
                msgs[0].1,
                msgs[0].2,
                msgs[0].3,
                msgs[0].4.clone()
            ),
            None
        );
        assert_eq!(
            r.push(
                msgs[2].0,
                msgs[2].1,
                msgs[2].2,
                msgs[2].3,
                msgs[2].4.clone()
            ),
            None
        );
    }

    #[test]
    fn newer_frame_supersedes_incomplete_older_frame() {
        let a = parts_of(&fragment(
            VideoStream::Screen,
            [1; 16],
            10,
            true,
            &vec![0xAA; 2500],
        ));
        let b = parts_of(&fragment(
            VideoStream::Screen,
            [1; 16],
            11,
            true,
            &vec![0xBB; 1500],
        ));
        let mut r = Reassembler::default();
        // Trame 10 partielle (un seul fragment).
        assert_eq!(r.push(a[0].0, a[0].1, a[0].2, a[0].3, a[0].4.clone()), None);
        // La trame 11 arrive : elle remplace la 10 et se complète.
        let mut out = None;
        for p in &b {
            if let Some(done) = r.push(p.0, p.1, p.2, p.3, p.4.clone()) {
                out = Some(done);
            }
        }
        assert_eq!(out, Some((vec![0xBB; 1500], true)));
    }

    #[test]
    fn stale_older_frame_is_ignored() {
        let newer = parts_of(&fragment(VideoStream::Screen, [2; 16], 100, false, b"new"));
        let older = parts_of(&fragment(VideoStream::Screen, [2; 16], 99, false, b"old"));
        let mut r = Reassembler::default();
        // On complète d'abord la trame 100.
        assert!(r
            .push(
                newer[0].0,
                newer[0].1,
                newer[0].2,
                newer[0].3,
                newer[0].4.clone()
            )
            .is_some());
        // Un fragment retardataire de la trame 99 est ignoré.
        assert_eq!(
            r.push(
                older[0].0,
                older[0].1,
                older[0].2,
                older[0].3,
                older[0].4.clone()
            ),
            None
        );
    }

    #[test]
    fn invalid_fragments_are_rejected() {
        let mut r = Reassembler::default();
        // frag_count nul.
        assert_eq!(r.push(1, 0, 0, false, vec![1]), None);
        // frag_idx hors borne.
        assert_eq!(r.push(1, 2, 5, false, vec![1]), None);
        // frag_count délirant (> MAX_FRAGS).
        assert_eq!(r.push(1, MAX_FRAGS + 1, 0, false, vec![1]), None);
    }

    #[test]
    fn frame_id_wraparound_is_handled() {
        // Rebouclage du compteur : u32::MAX puis 0 doit compter comme « plus
        // récent », pas comme un retardataire.
        let a = parts_of(&fragment(
            VideoStream::Screen,
            [3; 16],
            u32::MAX,
            false,
            b"a",
        ));
        let b = parts_of(&fragment(VideoStream::Screen, [3; 16], 0, false, b"b"));
        let mut r = Reassembler::default();
        assert!(r
            .push(a[0].0, a[0].1, a[0].2, a[0].3, a[0].4.clone())
            .is_some());
        assert_eq!(
            r.push(b[0].0, b[0].1, b[0].2, b[0].3, b[0].4.clone()),
            Some((b"b".to_vec(), false))
        );
    }

    #[test]
    fn camera_and_screen_produce_distinct_wire_variants() {
        let screen = fragment(VideoStream::Screen, [1; 16], 0, true, b"x");
        let camera = fragment(VideoStream::Camera, [1; 16], 0, true, b"x");
        assert!(matches!(screen.first(), Some(VoiceMsg::ScreenFrame { .. })));
        assert!(matches!(camera.first(), Some(VoiceMsg::CameraFrame { .. })));
    }

    #[test]
    fn separate_reassemblers_keep_the_two_streams_independent() {
        // Un réassembleur par flux : les identifiants de trame de la caméra ne
        // font jamais passer une trame d'écran pour une retardataire (et
        // réciproquement) — c'est ce qui permet de partager son écran ET de se
        // montrer en même temps.
        let screen = parts_of(&fragment(VideoStream::Screen, [1; 16], 100, true, b"ecran"));
        let camera = parts_of(&fragment(VideoStream::Camera, [1; 16], 5, true, b"camera"));
        let (mut rs, mut rc) = (Reassembler::default(), Reassembler::default());

        let s = rs.push(
            screen[0].0,
            screen[0].1,
            screen[0].2,
            screen[0].3,
            screen[0].4.clone(),
        );
        assert_eq!(s, Some((b"ecran".to_vec(), true)));
        // La trame caméra porte un `frame_id` PLUS PETIT : dans un réassembleur
        // partagé elle serait jetée comme retardataire. Ici elle passe.
        let c = rc.push(
            camera[0].0,
            camera[0].1,
            camera[0].2,
            camera[0].3,
            camera[0].4.clone(),
        );
        assert_eq!(c, Some((b"camera".to_vec(), true)));
    }
}
