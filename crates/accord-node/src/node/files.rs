//! Partage de fichiers : magasin local de blobs, publication (manifeste
//! signé), service des pairs et téléchargements (bloc `impl Node` du domaine
//! `files.*`).
//!
//! Le magasin vit dans `fichiers/` à côté de la base (répertoire de profil) :
//! blobs complets sous `fichiers/<racine hex>`, téléchargements en cours sous
//! `fichiers/<racine hex>.part` (écrits bloc à bloc, repris via la bitmap
//! persistée en base). La boucle réseau ([`crate::runtime`]) pilote les
//! échanges FILE ; ce module fournit la persistance et les E/S disque.

use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use accord_core::db::{FetchIntent, FileEntry};
use accord_core::files::{fec, merkle};
use accord_proto::core_msg::FileRef;
use accord_proto::file_msg::Manifest;
use accord_proto::limits::{FILE_BLOCK_SIZE, MAX_FILE_SIZE};
use accord_proto::wire::{WireDecode, WireEncode};
use serde_json::json;

use crate::error::NodeError;
use crate::hex;

use super::{now_ms, Node};

/// Longueur maximale d'un nom de fichier ou d'un type MIME (bornes filaires).
const MAX_NOM: usize = 255;

/// Intentions adoptées au plus par passe de la boucle réseau : la lecture est
/// bornée (les plus dues d'abord) plutôt que de charger toute la table
/// `file_fetches`. Large devant [`fetch::MAX_FETCHES_ACTIFS`] pour absorber
/// les intentions déjà actives ou déjà complètes que la passe ignore.
const ADOPT_INTENTS_MAX: usize = 64;

/// Vrai si le bit `i` de la bitmap est levé (bit i = bloc i détenu).
fn bit(bitmap: &[u8], i: usize) -> bool {
    bitmap.get(i / 8).is_some_and(|b| b & (1 << (i % 8)) != 0)
}

/// Bitmap pleine pour `count` blocs (fichier complet).
fn bitmap_pleine(count: usize) -> Vec<u8> {
    let mut out = vec![0u8; count.div_ceil(8)];
    for (i, byte) in out.iter_mut().enumerate() {
        let reste = count - i * 8;
        *byte = if reste >= 8 {
            0xFF
        } else {
            (1u16 << reste) as u8 - 1
        };
    }
    out
}

/// Type MIME deviné par l'extension du nom (liste courte de types courants).
pub(crate) fn mime_par_extension(nom: &str) -> &'static str {
    let ext = nom.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
    match ext.as_deref() {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        Some("pdf") => "application/pdf",
        Some("txt" | "log") => "text/plain",
        Some("md") => "text/markdown",
        Some("json") => "application/json",
        Some("mp4") => "video/mp4",
        Some("webm") => "video/webm",
        Some("mp3") => "audio/mpeg",
        Some("ogg" | "opus") => "audio/ogg",
        Some("wav") => "audio/wav",
        Some("zip") => "application/zip",
        _ => "application/octet-stream",
    }
}

/// Valide un nom de fichier proposé (borne, pas de séparateur ni contrôle).
fn valider_nom(nom: &str) -> Result<(), NodeError> {
    if nom.is_empty() || nom.len() > MAX_NOM {
        return Err(NodeError::Invalid("nom de fichier hors bornes"));
    }
    if nom.chars().any(|c| c.is_control() || c == '/' || c == '\\') {
        return Err(NodeError::Invalid("nom de fichier invalide"));
    }
    Ok(())
}

impl Node {
    /// Dossier du magasin de blobs (créé au besoin par les écritures).
    fn files_dir(&self) -> Result<PathBuf, NodeError> {
        self.with_db(|db| Ok(db.files_dir()?))
    }

    /// Chemin du blob complet d'une racine dans le magasin.
    fn blob_path(&self, racine: &[u8; 32]) -> Result<PathBuf, NodeError> {
        Ok(self.files_dir()?.join(hex::encode(racine)))
    }

    /// Chemin du fichier partiel d'un téléchargement en cours.
    fn part_path(&self, racine: &[u8; 32]) -> Result<PathBuf, NodeError> {
        Ok(self
            .files_dir()?
            .join(format!("{}.part", hex::encode(racine))))
    }

    /// Publie un fichier local dans le magasin (copie, manifeste signé) et le rend servable aux pairs.
    pub fn files_publish(&self, chemin: &Path) -> Result<FileRef, NodeError> {
        let meta = std::fs::metadata(chemin)?;
        if !meta.is_file() {
            return Err(NodeError::Invalid("chemin : fichier régulier attendu"));
        }
        if meta.len() == 0 || meta.len() > MAX_FILE_SIZE {
            return Err(NodeError::Invalid("taille de fichier hors bornes"));
        }
        let nom = chemin
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .ok_or(NodeError::Invalid("chemin sans nom de fichier"))?;
        let mime = mime_par_extension(&nom);
        let octets = std::fs::read(chemin)?;
        self.files_publish_bytes(&nom, mime, octets)
    }

    /// Publie un blob en mémoire (avatars, images collées) dans le magasin.
    pub fn files_publish_bytes(
        &self,
        nom: &str,
        mime: &str,
        octets: Vec<u8>,
    ) -> Result<FileRef, NodeError> {
        valider_nom(nom)?;
        if mime.is_empty() || mime.len() > MAX_NOM {
            return Err(NodeError::Invalid("type MIME hors bornes"));
        }
        if octets.is_empty() || octets.len() as u64 > MAX_FILE_SIZE {
            return Err(NodeError::Invalid("taille de fichier hors bornes"));
        }
        let manifest = merkle::build_manifest(&self.identity, &octets, nom, mime)?;
        let racine = manifest.merkle_root;
        let chemin = self.blob_path(&racine)?;
        if let Some(parent) = chemin.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Publication idempotente : la racine identifie le contenu exact.
        if !chemin.exists() {
            std::fs::write(&chemin, &octets)?;
        }
        let entry = FileEntry {
            merkle_root: racine,
            name: nom.to_string(),
            size: octets.len() as u64,
            mime: mime.to_string(),
            manifest: manifest.to_bytes(),
            path: Some(chemin.to_string_lossy().into_owned()),
            bitmap: bitmap_pleine(manifest.leaf_hashes.len()),
            complete: true,
            added_ms: now_ms(),
        };
        self.with_db(|db| Ok(db.upsert_file(&entry)?))?;
        Ok(FileRef {
            merkle_root: racine,
            name: entry.name,
            size: entry.size,
            mime: entry.mime,
        })
    }

    /// Chemin local d'un fichier complet du magasin, s'il existe.
    pub fn files_local_path(&self, racine: &[u8; 32]) -> Result<Option<PathBuf>, NodeError> {
        let Some(entry) = self.files_entry(racine)? else {
            return Ok(None);
        };
        let Some(path) = entry.path.filter(|_| entry.complete) else {
            return Ok(None);
        };
        let path = PathBuf::from(path);
        Ok(path.is_file().then_some(path))
    }

    /// Démarre (ou poursuit) le téléchargement d'un fichier depuis les pairs ; `indice` est un pair source probable.
    pub fn files_fetch(
        &self,
        racine: &[u8; 32],
        indice: Option<[u8; 32]>,
    ) -> Result<(), NodeError> {
        // Déjà complet en local : progression finale émise immédiatement.
        if self.files_local_path(racine)?.is_some() {
            if let Some(entry) = self.files_entry(racine)? {
                let total = merkle::block_count(entry.size);
                self.emit_file_progress(racine, total, total, true);
            }
            return Ok(());
        }
        // Intention persistée : la boucle réseau la reprend (même après un
        // redémarrage) et pilote le transfert fenêtré multi-sources.
        self.with_db(|db| Ok(db.upsert_file_fetch(racine, indice.as_ref(), now_ms())?))
    }

    // ---- Accès internes de la boucle réseau et du service API ----

    /// Entrée d'index d'un fichier connu localement.
    pub(crate) fn files_entry(&self, racine: &[u8; 32]) -> Result<Option<FileEntry>, NodeError> {
        self.with_db(|db| Ok(db.file(racine)?))
    }

    /// Manifest décodé d'un fichier connu localement.
    pub(crate) fn files_manifest(&self, racine: &[u8; 32]) -> Result<Option<Manifest>, NodeError> {
        let Some(entry) = self.files_entry(racine)? else {
            return Ok(None);
        };
        Ok(Some(Manifest::from_bytes(&entry.manifest).map_err(
            |_| NodeError::Invalid("manifest illisible dans l'index local"),
        )?))
    }

    /// Intentions de téléchargement à adopter à cette passe (persistées),
    /// bornées à [`ADOPT_INTENTS_MAX`] et priorisant les plus dues : la boucle
    /// réseau ne charge pas toute la table `file_fetches` toutes les 250 ms
    /// (latence de tick sous un grand nombre d'intentions).
    pub(crate) fn files_fetch_intents(&self) -> Result<Vec<FetchIntent>, NodeError> {
        self.with_db(|db| Ok(db.file_fetches_a_adopter(ADOPT_INTENTS_MAX)?))
    }

    /// Solde une intention de téléchargement (terminée : le fichier est là).
    pub(crate) fn files_fetch_clear(&self, racine: &[u8; 32]) -> Result<(), NodeError> {
        self.with_db(|db| Ok(db.remove_file_fetch(racine)?))
    }

    /// Reporte une intention après un abandon (backoff par racine) : elle
    /// reste persistée et sera ré-adoptée par la boucle réseau à l'échéance.
    pub(crate) fn files_fetch_defer(&self, racine: &[u8; 32], now: u64) -> Result<(), NodeError> {
        self.with_db(|db| Ok(db.defer_file_fetch(racine, now)?))
    }

    /// Réarme (relance immédiate) les intentions dont l'indice est ce pair —
    /// appelé à sa reconnexion. Rend le nombre d'intentions réarmées.
    pub(crate) fn files_retry_hinted(&self, indice: &[u8; 32]) -> Result<usize, NodeError> {
        self.with_db(|db| Ok(db.retry_file_fetches_hinted(indice)?))
    }

    /// Indexe le manifest d'un téléchargement qui démarre (entrée incomplète,
    /// bitmap vide). Sans effet si la racine est déjà indexée.
    pub(crate) fn files_store_manifest(&self, manifest: &Manifest) -> Result<(), NodeError> {
        if self.files_entry(&manifest.merkle_root)?.is_some() {
            return Ok(());
        }
        let entry = FileEntry {
            merkle_root: manifest.merkle_root,
            name: manifest.name.clone(),
            size: manifest.size,
            mime: manifest.mime.clone(),
            manifest: manifest.to_bytes(),
            path: None,
            bitmap: vec![0u8; manifest.leaf_hashes.len().div_ceil(8)],
            complete: false,
            added_ms: now_ms(),
        };
        self.with_db(|db| Ok(db.upsert_file(&entry)?))
    }

    /// Écrit un bloc vérifié dans le fichier partiel (à son décalage).
    pub(crate) fn files_write_block(
        &self,
        racine: &[u8; 32],
        index: u32,
        data: &[u8],
    ) -> Result<(), NodeError> {
        let chemin = self.part_path(racine)?;
        if let Some(parent) = chemin.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&chemin)?;
        f.seek(SeekFrom::Start(index as u64 * FILE_BLOCK_SIZE as u64))?;
        f.write_all(data)?;
        Ok(())
    }

    /// Persiste la progression d'un téléchargement ; à la complétion, le
    /// fichier partiel devient le blob définitif et l'index est mis à jour.
    pub(crate) fn files_save_progress(
        &self,
        racine: &[u8; 32],
        bitmap: &[u8],
        complete: bool,
    ) -> Result<(), NodeError> {
        if !complete {
            return self.with_db(|db| Ok(db.set_file_progress(racine, bitmap, false)?));
        }
        let Some(mut entry) = self.files_entry(racine)? else {
            return Err(NodeError::NotFound("fichier"));
        };
        let blob = self.blob_path(racine)?;
        let part = self.part_path(racine)?;
        if part.is_file() {
            std::fs::rename(&part, &blob)?;
        } else if !blob.is_file() {
            return Err(NodeError::NotFound("contenu téléchargé"));
        }
        entry.path = Some(blob.to_string_lossy().into_owned());
        entry.bitmap = bitmap.to_vec();
        entry.complete = true;
        self.with_db(|db| Ok(db.upsert_file(&entry)?))
    }

    /// Lit un bloc de données détenu (blob complet ou fichier partiel).
    pub(crate) fn files_read_block(
        &self,
        racine: &[u8; 32],
        index: u32,
    ) -> Result<Option<Vec<u8>>, NodeError> {
        let Some(entry) = self.files_entry(racine)? else {
            return Ok(None);
        };
        let len = merkle::block_len(entry.size, index as usize);
        if len == 0 {
            return Ok(None);
        }
        let chemin = if entry.complete {
            match entry.path {
                Some(p) => PathBuf::from(p),
                None => return Ok(None),
            }
        } else {
            if !bit(&entry.bitmap, index as usize) {
                return Ok(None);
            }
            self.part_path(racine)?
        };
        let Ok(mut f) = std::fs::File::open(&chemin) else {
            return Ok(None);
        };
        f.seek(SeekFrom::Start(index as u64 * FILE_BLOCK_SIZE as u64))?;
        let mut data = vec![0u8; len];
        f.read_exact(&mut data)?;
        Ok(Some(data))
    }

    /// Manifest à servir à un pair qui le demande.
    pub(crate) fn files_serve_manifest(
        &self,
        racine: &[u8; 32],
    ) -> Result<Option<Manifest>, NodeError> {
        self.files_manifest(racine)
    }

    /// Bloc à servir à un pair : données détenues, ou parité Reed-Solomon
    /// calculée à la volée (fichiers complets seulement). `None` si l'index
    /// est hors bornes ou le bloc absent.
    pub(crate) fn files_serve_block(
        &self,
        racine: &[u8; 32],
        index: u32,
    ) -> Result<Option<Vec<u8>>, NodeError> {
        let Some(entry) = self.files_entry(racine)? else {
            return Ok(None);
        };
        let count = merkle::block_count(entry.size);
        if (index as usize) < count {
            return self.files_read_block(racine, index);
        }
        // Parité : uniquement depuis un blob complet (le groupe entier est lu).
        let parite_totale = fec::group_count(count) * fec::RS_PARITY;
        if !entry.complete || (index as usize) >= count + parite_totale {
            return Ok(None);
        }
        let groupe = (index as usize - count) / fec::RS_PARITY;
        let j = (index as usize - count) % fec::RS_PARITY;
        let premier = groupe * fec::RS_DATA;
        let reels = (count - premier).min(fec::RS_DATA);
        let mut blocs = Vec::with_capacity(reels);
        for i in premier..premier + reels {
            match self.files_read_block(racine, i as u32)? {
                Some(b) => blocs.push(b),
                None => return Ok(None),
            }
        }
        let refs: Vec<&[u8]> = blocs.iter().map(Vec::as_slice).collect();
        let mut parite = fec::parity_for_group(&refs)?;
        Ok(Some(parite.swap_remove(j)))
    }

    /// Émet la progression d'un téléchargement vers l'UI.
    pub(crate) fn emit_file_progress(
        &self,
        racine: &[u8; 32],
        done: usize,
        total: usize,
        complete: bool,
    ) {
        self.emit(
            "event.file_progress",
            json!({
                "merkle_root": hex::encode(racine),
                "done": done,
                "total": total,
                "complete": complete,
            }),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outbound::OutboundSink;
    use accord_core::db::Db;
    use accord_crypto::Identity;

    /// Nœud adossé à une base sur disque (le magasin exige un profil réel).
    fn node_sur_disque() -> (Node, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("accord.db"), &[1u8; 32]).unwrap();
        let id = Identity::generate_with_pow_bits(1);
        (Node::new(id, db, OutboundSink::null()), dir)
    }

    #[test]
    fn publication_octets_cree_blob_et_index() {
        let (n, dir) = node_sur_disque();
        let contenu = vec![7u8; 3000];
        let f = n
            .files_publish_bytes("photo.png", "image/png", contenu.clone())
            .unwrap();
        assert_eq!(f.size, 3000);
        assert_eq!(f.mime, "image/png");
        // Blob écrit sous fichiers/<racine hex>.
        let chemin = n.files_local_path(&f.merkle_root).unwrap().unwrap();
        assert_eq!(std::fs::read(&chemin).unwrap(), contenu);
        assert!(chemin.starts_with(dir.path().canonicalize().unwrap()));
        // Index complet, manifest relisible et vérifiable.
        let entry = n.files_entry(&f.merkle_root).unwrap().unwrap();
        assert!(entry.complete);
        let manifest = n.files_manifest(&f.merkle_root).unwrap().unwrap();
        merkle::verify_manifest(&manifest).unwrap();
        assert_eq!(manifest.publisher, n.public_key());
        // Publication idempotente.
        let f2 = n
            .files_publish_bytes("photo.png", "image/png", contenu)
            .unwrap();
        assert_eq!(f2.merkle_root, f.merkle_root);
    }

    #[test]
    fn publication_depuis_un_chemin_devine_le_mime() {
        let (n, dir) = node_sur_disque();
        let source = dir.path().join("notes.md");
        std::fs::write(&source, b"# titre").unwrap();
        let f = n.files_publish(&source).unwrap();
        assert_eq!(f.name, "notes.md");
        assert_eq!(f.mime, "text/markdown");
        assert_eq!(mime_par_extension("SON.OPUS"), "audio/ogg");
        assert_eq!(
            mime_par_extension("sans-extension"),
            "application/octet-stream"
        );
    }

    #[test]
    fn publication_refuse_les_entrees_invalides() {
        let (n, dir) = node_sur_disque();
        assert!(n.files_publish_bytes("f.bin", "app/bin", vec![]).is_err());
        assert!(n.files_publish_bytes("", "app/bin", vec![1]).is_err());
        assert!(n.files_publish_bytes("a/../b", "app/bin", vec![1]).is_err());
        assert!(n.files_publish_bytes("f.bin", "", vec![1]).is_err());
        let vide = dir.path().join("vide.bin");
        std::fs::write(&vide, b"").unwrap();
        assert!(n.files_publish(&vide).is_err());
        assert!(n.files_publish(dir.path()).is_err());
    }

    #[test]
    fn intention_de_telechargement_persistee_puis_soldee() {
        let (n, _dir) = node_sur_disque();
        let racine = [3u8; 32];
        n.files_fetch(&racine, Some([9u8; 32])).unwrap();
        assert_eq!(
            n.files_fetch_intents().unwrap(),
            vec![FetchIntent {
                merkle_root: racine,
                hint: Some([9u8; 32]),
                next_attempt_ms: 0,
                attempts: 0,
            }]
        );
        n.files_fetch_clear(&racine).unwrap();
        assert!(n.files_fetch_intents().unwrap().is_empty());
        // Fichier déjà complet : aucune intention créée.
        let f = n
            .files_publish_bytes("f.bin", "app/bin", vec![1, 2, 3])
            .unwrap();
        n.files_fetch(&f.merkle_root, None).unwrap();
        assert!(n.files_fetch_intents().unwrap().is_empty());
    }

    #[test]
    fn intention_survit_a_l_abandon_puis_reprend_a_la_reconnexion_du_pair() {
        let (n, _dir) = node_sur_disque();
        let racine = [3u8; 32];
        let indice = [9u8; 32];
        n.files_fetch(&racine, Some(indice)).unwrap();
        // Abandon : l'intention est REPORTÉE (backoff), jamais supprimée.
        n.files_fetch_defer(&racine, 1_000).unwrap();
        let intents = n.files_fetch_intents().unwrap();
        assert_eq!(intents.len(), 1, "intention perdue après abandon");
        assert_eq!(intents[0].attempts, 1);
        assert!(intents[0].next_attempt_ms > 1_000);
        // Le pair indice se reconnecte : relance immédiate, backoff réarmé.
        assert_eq!(n.files_retry_hinted(&indice).unwrap(), 1);
        let intents = n.files_fetch_intents().unwrap();
        assert_eq!(intents[0].next_attempt_ms, 0);
        assert_eq!(intents[0].attempts, 0);
    }

    #[test]
    fn telechargement_bloc_a_bloc_puis_finalisation() {
        // Le « pair source » est un second magasin local.
        let (source, _d1) = node_sur_disque();
        let contenu: Vec<u8> = (0..FILE_BLOCK_SIZE + 1234).map(|i| i as u8).collect();
        let f = source
            .files_publish_bytes("gros.bin", "app/bin", contenu.clone())
            .unwrap();
        let manifest = source.files_manifest(&f.merkle_root).unwrap().unwrap();

        let (n, _d2) = node_sur_disque();
        n.files_store_manifest(&manifest).unwrap();
        assert!(n.files_local_path(&f.merkle_root).unwrap().is_none());
        // Blocs servis par la source, écrits chez le destinataire.
        for i in 0..2u32 {
            let bloc = source
                .files_serve_block(&f.merkle_root, i)
                .unwrap()
                .unwrap();
            n.files_write_block(&f.merkle_root, i, &bloc).unwrap();
        }
        // Progression partielle : le bloc 1 est relisible, pas le bloc 0.
        n.files_save_progress(&f.merkle_root, &[0b10], false)
            .unwrap();
        assert!(n.files_read_block(&f.merkle_root, 0).unwrap().is_none());
        assert!(n.files_read_block(&f.merkle_root, 1).unwrap().is_some());
        // Complétion : renommage en blob définitif, contenu intact.
        n.files_save_progress(&f.merkle_root, &[0b11], true)
            .unwrap();
        let chemin = n.files_local_path(&f.merkle_root).unwrap().unwrap();
        assert_eq!(std::fs::read(chemin).unwrap(), contenu);
        assert!(!n.part_path(&f.merkle_root).unwrap().exists());
    }

    #[test]
    fn service_des_blocs_donnees_et_parite() {
        let (n, _dir) = node_sur_disque();
        let contenu: Vec<u8> = (0..2 * FILE_BLOCK_SIZE + 400)
            .map(|i| (i / 7) as u8)
            .collect();
        let f = n
            .files_publish_bytes("f.bin", "app/bin", contenu.clone())
            .unwrap();
        // Données : chaque bloc correspond au contenu.
        let bloc0 = n.files_serve_block(&f.merkle_root, 0).unwrap().unwrap();
        assert_eq!(bloc0, contenu[..FILE_BLOCK_SIZE]);
        let bloc2 = n.files_serve_block(&f.merkle_root, 2).unwrap().unwrap();
        assert_eq!(bloc2, contenu[2 * FILE_BLOCK_SIZE..]);
        // Parité : identique au calcul direct du groupe.
        let refs: Vec<&[u8]> = contenu.chunks(FILE_BLOCK_SIZE).collect();
        let attendu = fec::parity_for_group(&refs).unwrap();
        for j in 0..fec::RS_PARITY as u32 {
            let p = n.files_serve_block(&f.merkle_root, 3 + j).unwrap().unwrap();
            assert_eq!(p, attendu[j as usize]);
        }
        // Hors bornes : refusé proprement.
        assert!(n.files_serve_block(&f.merkle_root, 7).unwrap().is_none());
        assert!(n.files_serve_block(&[0u8; 32], 0).unwrap().is_none());
    }

    #[test]
    fn bitmaps_et_compteurs() {
        assert_eq!(bitmap_pleine(1), vec![0b1]);
        assert_eq!(bitmap_pleine(8), vec![0xFF]);
        assert_eq!(bitmap_pleine(11), vec![0xFF, 0b111]);
        assert!(bit(&[0b10], 1));
        assert!(!bit(&[0b10], 0));
        assert!(!bit(&[0b10], 99));
    }
}
