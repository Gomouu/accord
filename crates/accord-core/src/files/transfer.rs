//! Téléchargement multi-sources avec fenêtrage et reprise (SPEC §9).
//!
//! Machine à états sans E/S : l'appelant (démon) fournit les événements
//! réseau (`on_block`, `on_not_found`) et draine les blocs vérifiés
//! ([`Transfer::take_block`]) vers le disque. Chaque bloc de données est
//! vérifié contre sa feuille du manifest AVANT d'être accepté ; les blocs de
//! parité alimentent la réparation Reed-Solomon quand un groupe est
//! réparable. La reprise s'appuie sur la bitmap persistée (format
//! `FileMsg::Have`).

use std::collections::{BTreeMap, BTreeSet};

use accord_proto::file_msg::Manifest;

use crate::error::CoreError;
use crate::files::{fec, merkle};

/// Blocs simultanément en vol par source (SPEC §9).
pub const WINDOW_PER_SOURCE: usize = 8;

/// Issue de la réception d'un bloc.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockOutcome {
    /// Bloc de données vérifié et accepté.
    Verified,
    /// Bloc de parité retenu pour réparation éventuelle.
    ParityStored,
    /// Hash invalide : bloc rejeté (source suspecte).
    Rejected,
    /// Bloc déjà détenu ou index inconnu : ignoré.
    Ignored,
}

/// État d'une source de téléchargement.
#[derive(Debug, Default)]
struct Source {
    /// Index des blocs demandés à cette source, non encore répondus.
    in_flight: Vec<u32>,
    /// Index refusés par cette source (`NotFound`) : ne plus lui demander.
    refused: Vec<u32>,
}

/// Téléchargement d'un fichier depuis plusieurs sources.
#[derive(Debug)]
pub struct Transfer {
    manifest: Manifest,
    /// Bloc de données i détenu (vérifié, éventuellement déjà drainé).
    have: Vec<bool>,
    /// Blocs vérifiés en attente de drainage vers le disque.
    pending: BTreeMap<u32, Vec<u8>>,
    /// Blocs de parité reçus, par index global.
    parity: BTreeMap<u32, Vec<u8>>,
    /// Sources actives.
    sources: BTreeMap<[u8; 32], Source>,
    /// Index actuellement demandés (toutes sources confondues).
    requested: Vec<u32>,
}

impl Transfer {
    /// Démarre un téléchargement depuis un manifest vérifié.
    pub fn new(manifest: Manifest) -> Result<Self, CoreError> {
        merkle::verify_manifest(&manifest)?;
        let count = manifest.leaf_hashes.len();
        Ok(Self {
            manifest,
            have: vec![false; count],
            pending: BTreeMap::new(),
            parity: BTreeMap::new(),
            sources: BTreeMap::new(),
            requested: Vec::new(),
        })
    }

    /// Reprend un téléchargement interrompu depuis la bitmap persistée
    /// (les blocs déjà détenus sont sur disque, ils ne seront pas redemandés).
    pub fn resume(manifest: Manifest, bitmap: &[u8]) -> Result<Self, CoreError> {
        let mut transfer = Self::new(manifest)?;
        let count = transfer.have.len();
        if bitmap.len() != count.div_ceil(8) {
            return Err(CoreError::Invalid("bitmap de reprise de taille invalide"));
        }
        for i in 0..count {
            transfer.have[i] = bitmap[i / 8] & (1 << (i % 8)) != 0;
        }
        Ok(transfer)
    }

    /// Manifest du transfert.
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Bitmap des blocs détenus (format `FileMsg::Have`, bit i = bloc i).
    pub fn bitmap(&self) -> Vec<u8> {
        let mut out = vec![0u8; self.have.len().div_ceil(8)];
        for (i, &h) in self.have.iter().enumerate() {
            if h {
                out[i / 8] |= 1 << (i % 8);
            }
        }
        out
    }

    /// `(blocs détenus, blocs totaux)`.
    pub fn progress(&self) -> (usize, usize) {
        (self.have.iter().filter(|h| **h).count(), self.have.len())
    }

    /// Vrai si tous les blocs de données sont détenus.
    pub fn is_complete(&self) -> bool {
        self.have.iter().all(|h| *h)
    }

    /// Déclare une source disponible (idempotent).
    pub fn add_source(&mut self, node: [u8; 32]) {
        self.sources.entry(node).or_default();
    }

    /// Nombre de sources actives.
    pub fn source_count(&self) -> usize {
        self.sources.len()
    }

    /// Retire une source (panne, départ) ; ses demandes en vol redeviennent
    /// assignables.
    pub fn remove_source(&mut self, node: &[u8; 32]) {
        if let Some(src) = self.sources.remove(node) {
            self.requested.retain(|i| !src.in_flight.contains(i));
        }
    }

    /// Blocs à demander maintenant : données manquantes d'abord, puis parité
    /// des groupes dont un bloc a été refusé. Chaque bloc est assigné à la
    /// source la moins chargée qui ne l'a pas refusé, dans la limite de
    /// 8 blocs en vol par source.
    pub fn next_requests(&mut self) -> Vec<([u8; 32], u32)> {
        // Capacité globale restante : la somme des fenêtres libres. Nulle,
        // aucune demande ne peut partir — et il est inutile de balayer les
        // milliers de blocs manquants d'un gros fichier pour le découvrir.
        // Cette passe est rejouée à CHAQUE bloc reçu.
        let mut capacite: usize = self
            .sources
            .values()
            .map(|s| WINDOW_PER_SOURCE.saturating_sub(s.in_flight.len()))
            .sum();
        if capacite == 0 {
            return Vec::new();
        }
        // Appartenance en O(log n) : `requested` reste un `Vec` (son ordre ne
        // sert à rien ailleurs), mais un `contains` linéaire par bloc manquant
        // rendait la passe quadratique — 8 192 blocs × 8 192 passes.
        let deja_demandes: BTreeSet<u32> = self.requested.iter().copied().collect();
        let wanted: Vec<u32> = self
            .missing_data()
            .into_iter()
            .chain(self.useful_parity())
            .filter(|i| !deja_demandes.contains(i))
            .collect();
        let mut out = Vec::new();
        for index in wanted {
            let candidate = self
                .sources
                .iter_mut()
                .filter(|(_, s)| {
                    s.in_flight.len() < WINDOW_PER_SOURCE && !s.refused.contains(&index)
                })
                .min_by_key(|(_, s)| s.in_flight.len());
            let Some((node, src)) = candidate else {
                continue;
            };
            src.in_flight.push(index);
            self.requested.push(index);
            out.push((*node, index));
            // Fenêtres pleines : les blocs suivants ne trouveraient plus aucun
            // candidat de toute façon. Sortie exactement équivalente, sans le
            // balayage stérile du reste du fichier.
            capacite -= 1;
            if capacite == 0 {
                break;
            }
        }
        out
    }

    /// Réception d'un bloc depuis une source. Vérifie les blocs de données
    /// contre le manifest ; retient la parité ; libère la fenêtre.
    pub fn on_block(&mut self, source: &[u8; 32], index: u32, data: Vec<u8>) -> BlockOutcome {
        let count = self.have.len() as u32;
        // Corrélation demande↔réponse (blocs de DONNÉES) : n'accepter un bloc
        // que d'une source à qui on l'a effectivement demandé (index dans sa
        // fenêtre `in_flight`). Un bloc non sollicité est rejeté à bas coût
        // AVANT `settle` et AVANT toute vérification Merkle — un pair ne peut
        // donc pas nous faire hacher des blocs arbitraires. La livraison
        // légitime correspond toujours à un index poussé par `next_requests`,
        // donc aucun débit honnête n'est bridé.
        if index < count
            && !self
                .sources
                .get(source)
                .is_some_and(|s| s.in_flight.contains(&index))
        {
            return BlockOutcome::Ignored;
        }
        self.settle(source, index);
        if index < count {
            let i = index as usize;
            if self.have[i] {
                return BlockOutcome::Ignored;
            }
            if data.len() != merkle::block_len(self.manifest.size, i)
                || !merkle::verify_block(&self.manifest.leaf_hashes[i], &data)
            {
                return BlockOutcome::Rejected;
            }
            self.accept(index, data);
            BlockOutcome::Verified
        } else if index < count + (fec::group_count(count as usize) * fec::RS_PARITY) as u32 {
            // Parité (envoyée proactivement, donc non corrélable) : bornée par
            // le manifest. Ne déclencher la réparation RS (coûteuse) qu'UNE
            // fois par index — un flot de parités dupliquées est ignoré.
            if self.parity.contains_key(&index) {
                return BlockOutcome::Ignored;
            }
            self.parity.insert(index, data);
            BlockOutcome::ParityStored
        } else {
            BlockOutcome::Ignored
        }
    }

    /// La source ne détient pas ce bloc : libère la fenêtre et évite de le
    /// lui redemander.
    pub fn on_not_found(&mut self, source: &[u8; 32], index: u32) {
        self.settle(source, index);
        if let Some(src) = self.sources.get_mut(source) {
            src.refused.push(index);
        }
    }

    /// Tente la réparation Reed-Solomon de chaque groupe réparable avec la
    /// parité reçue. Rend les index de blocs récupérés. Les blocs réparés
    /// sont vérifiés contre le manifest ; en cas d'échec (parité corrompue),
    /// la parité du groupe est jetée.
    ///
    /// `disk_block` fournit les blocs déjà drainés (reprise) : appelé pour
    /// tout bloc détenu absent de la file interne.
    pub fn try_repair(
        &mut self,
        mut disk_block: impl FnMut(u32) -> Option<Vec<u8>>,
    ) -> Result<Vec<u32>, CoreError> {
        let count = self.have.len();
        let mut repaired = Vec::new();
        for group in 0..fec::group_count(count) {
            let first = group * fec::RS_DATA;
            let real = (count - first).min(fec::RS_DATA);
            let missing: Vec<usize> = (first..first + real).filter(|&i| !self.have[i]).collect();
            if missing.is_empty() {
                continue;
            }
            if missing.len() > fec::RS_PARITY {
                continue;
            }
            // Parts disponibles : données détenues + parité reçue.
            let mut shards: Vec<Option<Vec<u8>>> = vec![None; fec::RS_DATA + fec::RS_PARITY];
            for i in first..first + real {
                if self.have[i] {
                    shards[i - first] = self
                        .pending
                        .get(&(i as u32))
                        .cloned()
                        .or_else(|| disk_block(i as u32));
                }
            }
            for j in 0..fec::RS_PARITY {
                let idx = fec::parity_index(count, group, j);
                if let Some(p) = self.parity.get(&idx) {
                    shards[fec::RS_DATA + j] = Some(p.clone());
                }
            }
            // Les blocs virtuels (nuls) d'un groupe incomplet comptent comme
            // disponibles : la reconstruction exige 10 parts sur 14.
            let available = shards.iter().flatten().count() + (fec::RS_DATA - real);
            if available < fec::RS_DATA {
                continue;
            }
            let sizes: Vec<usize> = (first..first + real)
                .map(|i| merkle::block_len(self.manifest.size, i))
                .collect();
            let Ok(blocks) = fec::reconstruct_group(&sizes, &shards) else {
                continue;
            };
            // Vérification systématique des blocs réparés contre le manifest.
            let mut group_ok = true;
            for &i in &missing {
                if !merkle::verify_block(&self.manifest.leaf_hashes[i], &blocks[i - first]) {
                    group_ok = false;
                    break;
                }
            }
            if !group_ok {
                // Parité corrompue quelque part : on la jette entièrement.
                for j in 0..fec::RS_PARITY {
                    self.parity.remove(&fec::parity_index(count, group, j));
                }
                continue;
            }
            for &i in &missing {
                self.accept(i as u32, blocks[i - first].clone());
                repaired.push(i as u32);
            }
        }
        Ok(repaired)
    }

    /// Draine un bloc vérifié vers l'appelant (écriture disque).
    pub fn take_block(&mut self, index: u32) -> Option<Vec<u8>> {
        self.pending.remove(&index)
    }

    /// Libère toutes les demandes en vol (relance après pertes réseau) : les
    /// blocs non répondus redeviennent assignables ; les refus (`NotFound`)
    /// sont conservés.
    pub fn release_in_flight(&mut self) {
        for src in self.sources.values_mut() {
            src.in_flight.clear();
        }
        self.requested.clear();
    }

    /// Index des blocs vérifiés en attente de drainage.
    pub fn pending_blocks(&self) -> Vec<u32> {
        self.pending.keys().copied().collect()
    }

    fn accept(&mut self, index: u32, data: Vec<u8>) {
        self.have[index as usize] = true;
        self.pending.insert(index, data);
        self.requested.retain(|i| *i != index);
    }

    fn settle(&mut self, source: &[u8; 32], index: u32) {
        if let Some(src) = self.sources.get_mut(source) {
            src.in_flight.retain(|i| *i != index);
        }
        self.requested.retain(|i| *i != index);
    }

    fn missing_data(&self) -> Vec<u32> {
        (0..self.have.len() as u32)
            .filter(|&i| !self.have[i as usize])
            .collect()
    }

    /// Parité utile : celle des groupes dont un bloc manquant a été refusé
    /// par au moins une source (la parité n'est pas demandée tant que les
    /// données semblent obtenables directement).
    fn useful_parity(&self) -> Vec<u32> {
        let count = self.have.len();
        // Ensemble plutôt que liste : les refus sont testés une fois par bloc
        // de données du fichier, et un `Vec::contains` y coûtait le produit des
        // deux tailles.
        let refused: BTreeSet<u32> = self
            .sources
            .values()
            .flat_map(|s| s.refused.iter().copied())
            .collect();
        let mut out = Vec::new();
        for group in 0..fec::group_count(count) {
            let first = group * fec::RS_DATA;
            let real = (count - first).min(fec::RS_DATA);
            let group_stuck =
                (first..first + real).any(|i| !self.have[i] && refused.contains(&(i as u32)));
            if !group_stuck {
                continue;
            }
            for j in 0..fec::RS_PARITY {
                let idx = fec::parity_index(count, group, j);
                if !self.parity.contains_key(&idx) {
                    out.push(idx);
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use accord_crypto::Identity;
    use accord_proto::limits::FILE_BLOCK_SIZE;

    fn fixture(blocks: usize, tail: usize) -> (Manifest, Vec<Vec<u8>>) {
        let publisher = Identity::generate_with_pow_bits(1);
        let mut data = Vec::new();
        for i in 0..blocks {
            let len = if i + 1 == blocks {
                tail
            } else {
                FILE_BLOCK_SIZE
            };
            data.extend(std::iter::repeat_n((i + 1) as u8, len));
        }
        let manifest = merkle::build_manifest(&publisher, &data, "f.bin", "app/bin").unwrap();
        let chunks = data.chunks(FILE_BLOCK_SIZE).map(<[u8]>::to_vec).collect();
        (manifest, chunks)
    }

    #[test]
    fn windowed_download_completes_and_verifies() {
        let (manifest, blocks) = fixture(3, 500);
        let mut t = Transfer::new(manifest).unwrap();
        let src = [1u8; 32];
        t.add_source(src);
        let reqs = t.next_requests();
        assert_eq!(reqs.len(), 3); // 3 données + parité non demandée (< fenêtre ? si)
        for (node, index) in reqs {
            if (index as usize) < blocks.len() {
                assert_eq!(
                    t.on_block(&node, index, blocks[index as usize].clone()),
                    BlockOutcome::Verified
                );
            }
        }
        assert!(t.is_complete());
        assert_eq!(t.progress(), (3, 3));
        // Drainage : chaque bloc sort une seule fois.
        assert_eq!(t.take_block(0).unwrap(), blocks[0]);
        assert!(t.take_block(0).is_none());
    }

    #[test]
    fn bloc_de_donnees_non_sollicite_est_ignore_sans_verification() {
        // Régression (audit 1.0) : un pair ne peut pas nous faire vérifier
        // (hacher) des blocs de données non demandés. Sans demande préalable,
        // le bloc n'est pas en fenêtre → ignoré à bas coût.
        let (manifest, blocks) = fixture(2, 300);
        let mut t = Transfer::new(manifest).unwrap();
        let src = [1u8; 32];
        t.add_source(src);
        assert_eq!(
            t.on_block(&src, 0, blocks[0].clone()),
            BlockOutcome::Ignored
        );
        // Une fois demandé (dans la fenêtre de sa source), il est accepté.
        let (node, index) = t
            .next_requests()
            .into_iter()
            .find(|(_, i)| *i == 0)
            .unwrap();
        assert_eq!(
            t.on_block(&node, index, blocks[index as usize].clone()),
            BlockOutcome::Verified
        );
    }

    #[test]
    fn parite_dupliquee_ne_relance_pas_la_reparation() {
        // Régression (audit 1.0) : un flot de blocs de parité identiques ne
        // déclenche la réparation RS (coûteuse) qu'une seule fois par index.
        let (manifest, _) = fixture(2, 300);
        let mut t = Transfer::new(manifest).unwrap();
        let src = [1u8; 32];
        t.add_source(src);
        let parite = 2u32; // premier index de parité (au-delà des 2 données)
        assert_eq!(
            t.on_block(&src, parite, vec![0u8; 300]),
            BlockOutcome::ParityStored
        );
        assert_eq!(
            t.on_block(&src, parite, vec![0u8; 300]),
            BlockOutcome::Ignored
        );
    }

    #[test]
    fn corrupted_block_is_rejected_and_window_freed() {
        let (manifest, blocks) = fixture(2, 100);
        let mut t = Transfer::new(manifest).unwrap();
        let src = [1u8; 32];
        t.add_source(src);
        t.next_requests();
        assert_eq!(
            t.on_block(&src, 0, vec![0xFF; blocks[0].len()]),
            BlockOutcome::Rejected
        );
        assert!(!t.is_complete());
        // Le bloc redevient demandable.
        assert!(t.next_requests().iter().any(|(_, i)| *i == 0));
    }

    #[test]
    fn window_caps_in_flight_per_source() {
        let (manifest, _) = fixture(12, 100);
        let mut t = Transfer::new(manifest).unwrap();
        t.add_source([1u8; 32]);
        let reqs = t.next_requests();
        assert_eq!(reqs.len(), WINDOW_PER_SOURCE);
        // Deuxième source : fenêtre propre.
        t.add_source([2u8; 32]);
        let more = t.next_requests();
        assert!(more.iter().all(|(n, _)| *n == [2u8; 32]));
        assert!(!more.is_empty());
        // Aucun index demandé deux fois.
        let mut all: Vec<u32> = reqs.iter().chain(&more).map(|(_, i)| *i).collect();
        all.sort_unstable();
        all.dedup();
        assert_eq!(all.len(), reqs.len() + more.len());
    }

    #[test]
    fn not_found_reroutes_to_other_source() {
        let (manifest, blocks) = fixture(1, 300);
        let mut t = Transfer::new(manifest).unwrap();
        let a = [1u8; 32];
        let b = [2u8; 32];
        t.add_source(a);
        let reqs = t.next_requests();
        let data_req = reqs.iter().find(|(_, i)| *i == 0).unwrap();
        t.on_not_found(&data_req.0, 0);
        t.add_source(b);
        // Le bloc 0 doit être réassigné à b, pas à a.
        let reqs = t.next_requests();
        assert!(reqs.contains(&(b, 0)));
        assert_eq!(t.on_block(&b, 0, blocks[0].clone()), BlockOutcome::Verified);
        assert!(t.is_complete());
    }

    #[test]
    fn release_in_flight_makes_blocks_reassignable_but_keeps_refusals() {
        let (manifest, blocks) = fixture(1, 300);
        let mut t = Transfer::new(manifest).unwrap();
        let a = [1u8; 32];
        t.add_source(a);
        assert_eq!(t.next_requests(), vec![(a, 0)]);
        // Demande perdue sur le réseau : rien n'est redemandé sans libération.
        assert!(t.next_requests().iter().all(|(_, i)| *i != 0));
        t.release_in_flight();
        // Le bloc 0 redevient demandable auprès de `a`.
        assert!(t.next_requests().contains(&(a, 0)));
        // Un refus survit à la libération : le bloc n'est plus demandé à `a`,
        // mais l'est à une nouvelle source.
        t.on_not_found(&a, 0);
        t.release_in_flight();
        assert!(!t.next_requests().contains(&(a, 0)));
        let b = [2u8; 32];
        t.add_source(b);
        assert!(t.next_requests().contains(&(b, 0)));
        assert_eq!(t.on_block(&b, 0, blocks[0].clone()), BlockOutcome::Verified);
    }

    #[test]
    fn fec_repairs_missing_block_from_parity() {
        let (manifest, blocks) = fixture(3, 400);
        let refs: Vec<&[u8]> = blocks.iter().map(Vec::as_slice).collect();
        let parity = fec::parity_for_group(&refs).unwrap();

        let mut t = Transfer::new(manifest).unwrap();
        let src = [1u8; 32];
        t.add_source(src);
        t.next_requests();
        // Reçoit blocs 0 et 2 ; le bloc 1 est introuvable partout.
        t.on_block(&src, 0, blocks[0].clone());
        t.on_block(&src, 2, blocks[2].clone());
        t.on_not_found(&src, 1);
        // La parité du groupe est demandée puis reçue.
        let parity_reqs: Vec<u32> = t.next_requests().iter().map(|(_, i)| *i).collect();
        assert!(parity_reqs.contains(&3));
        assert_eq!(
            t.on_block(&src, 3, parity[0].clone()),
            BlockOutcome::ParityStored
        );
        let repaired = t.try_repair(|_| None).unwrap();
        assert_eq!(repaired, vec![1]);
        assert!(t.is_complete());
        assert_eq!(t.take_block(1).unwrap(), blocks[1]);
    }

    #[test]
    fn corrupted_parity_is_discarded_not_trusted() {
        let (manifest, blocks) = fixture(3, 400);
        let mut t = Transfer::new(manifest).unwrap();
        let src = [1u8; 32];
        t.add_source(src);
        t.next_requests();
        t.on_block(&src, 0, blocks[0].clone());
        t.on_block(&src, 2, blocks[2].clone());
        // Parité forgée de la bonne taille.
        t.on_block(&src, 3, vec![0xEE; blocks[0].len()]);
        let repaired = t.try_repair(|_| None).unwrap();
        assert!(repaired.is_empty());
        assert!(!t.is_complete());
    }

    #[test]
    fn resume_from_bitmap_skips_held_blocks_and_repairs_from_disk() {
        let (manifest, blocks) = fixture(3, 400);
        let refs: Vec<&[u8]> = blocks.iter().map(Vec::as_slice).collect();
        let parity = fec::parity_for_group(&refs).unwrap();

        // Première session : blocs 0 et 2 obtenus puis drainés sur disque.
        let mut t1 = Transfer::new(manifest.clone()).unwrap();
        let src = [1u8; 32];
        t1.add_source(src);
        t1.next_requests();
        t1.on_block(&src, 0, blocks[0].clone());
        t1.on_block(&src, 2, blocks[2].clone());
        t1.take_block(0).unwrap();
        t1.take_block(2).unwrap();
        let bitmap = t1.bitmap();

        // Reprise : seul le bloc 1 est manquant.
        let mut t2 = Transfer::resume(manifest, &bitmap).unwrap();
        t2.add_source(src);
        let reqs = t2.next_requests();
        assert!(reqs.iter().any(|(_, i)| *i == 1));
        assert!(!reqs.iter().any(|(_, i)| *i == 0 || *i == 2));
        // Réparation avec parité + blocs relus du disque.
        t2.on_block(&src, 3, parity[0].clone());
        let repaired = t2.try_repair(|i| blocks.get(i as usize).cloned()).unwrap();
        assert_eq!(repaired, vec![1]);
        assert!(t2.is_complete());
    }
}
