//! Canal FILE (0x04) : transfert de fichiers fragmentés (SPEC §9).

use crate::limits;
use crate::wire::{DecodeError, Reader, WireDecode, WireEncode, Writer};

const MAX_NAME: usize = 256;
/// Nombre maximal de feuilles d'un manifest (2 GiB / 256 KiB = 8192).
///
/// 🔒 Plus large que [`limits::MAX_LIST`], et c'est voulu : le nombre de
/// feuilles n'est pas un choix de l'émetteur mais une conséquence arithmétique
/// de `size`, elle-même bornée par [`limits::MAX_FILE_SIZE`] — le décodage
/// vérifie d'ailleurs l'égalité des deux (`manifest.leaf_count`). Un émetteur ne
/// peut donc pas déclarer plus de feuilles qu'il n'annonce d'octets, ni en
/// annoncer sans fournir les 32 octets de chacune.
const MAX_LEAVES: usize = 8192;

/// Manifest signé décrivant un fichier partagé.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    /// Racine de l'arbre de Merkle (identifiant du fichier).
    pub merkle_root: [u8; 32],
    /// Taille exacte du fichier en octets.
    pub size: u64,
    /// Nom de fichier proposé.
    pub name: String,
    /// Type MIME déclaré.
    pub mime: String,
    /// Hash SHA-256 de chaque bloc de 256 KiB, dans l'ordre.
    pub leaf_hashes: Vec<[u8; 32]>,
    /// Clé publique Ed25519 du publieur.
    pub publisher: [u8; 32],
    /// Signature Ed25519 sur [`Manifest::signable_bytes`].
    pub sig: [u8; 64],
}

impl Manifest {
    /// Octets couverts par la signature du manifest.
    pub fn signable_bytes(&self) -> Vec<u8> {
        let mut w = Writer::with_capacity(64 + self.leaf_hashes.len() * 32);
        w.put_raw(b"accord-manifest-v1");
        w.put_arr(&self.merkle_root);
        w.put_u64(self.size);
        w.put_str(&self.name);
        w.put_str(&self.mime);
        w.put_list(&self.leaf_hashes, |w, h| w.put_arr(h));
        w.into_bytes()
    }
}

impl WireEncode for Manifest {
    fn encode(&self, w: &mut Writer) {
        w.put_arr(&self.merkle_root);
        w.put_u64(self.size);
        w.put_str(&self.name);
        w.put_str(&self.mime);
        w.put_list(&self.leaf_hashes, |w, h| w.put_arr(h));
        w.put_arr(&self.publisher);
        w.put_arr(&self.sig);
    }
}

impl WireDecode for Manifest {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        let merkle_root = r.arr()?;
        let size = r.u64()?;
        if size == 0 || size > limits::MAX_FILE_SIZE {
            return Err(DecodeError::TooLarge("manifest.size"));
        }
        let name = r.str(MAX_NAME, "manifest.name")?;
        let mime = r.str(MAX_NAME, "manifest.mime")?;
        let leaf_hashes = r.list(MAX_LEAVES, "manifest.leaves", |r| r.arr())?;
        let expected = size.div_ceil(limits::FILE_BLOCK_SIZE as u64) as usize;
        if leaf_hashes.len() != expected {
            return Err(DecodeError::InvalidValue("manifest.leaf_count"));
        }
        Ok(Manifest {
            merkle_root,
            size,
            name,
            mime,
            leaf_hashes,
            publisher: r.arr()?,
            sig: r.arr()?,
        })
    }
}

/// Message du canal FILE.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileMsg {
    /// 0x01 — Demande du manifest d'un fichier.
    GetManifest {
        /// Racine Merkle du fichier.
        root: [u8; 32],
    },
    /// 0x02 — Manifest signé.
    ManifestMsg {
        /// Manifest complet.
        manifest: Manifest,
    },
    /// 0x03 — Demande d'un bloc.
    GetBlock {
        /// Racine Merkle du fichier.
        root: [u8; 32],
        /// Index du bloc (données 0..n, parité n..n+p).
        index: u32,
    },
    /// 0x04 — Contenu d'un bloc.
    Block {
        /// Racine Merkle du fichier.
        root: [u8; 32],
        /// Index du bloc.
        index: u32,
        /// Données (≤ 256 KiB).
        data: Vec<u8>,
    },
    /// 0x05 — Bitmap des blocs détenus (1 bit par bloc).
    Have {
        /// Racine Merkle du fichier.
        root: [u8; 32],
        /// Bitmap little-endian par octet, bit i = bloc i détenu.
        bitmap: Vec<u8>,
    },
    /// 0x06 — Bloc introuvable ou refusé.
    NotFound {
        /// Racine Merkle du fichier.
        root: [u8; 32],
        /// Index demandé.
        index: u32,
    },
}

impl WireEncode for FileMsg {
    fn encode(&self, w: &mut Writer) {
        match self {
            FileMsg::GetManifest { root } => {
                w.put_u8(0x01);
                w.put_arr(root);
            }
            FileMsg::ManifestMsg { manifest } => {
                w.put_u8(0x02);
                manifest.encode(w);
            }
            FileMsg::GetBlock { root, index } => {
                w.put_u8(0x03);
                w.put_arr(root);
                w.put_u32(*index);
            }
            FileMsg::Block { root, index, data } => {
                w.put_u8(0x04);
                w.put_arr(root);
                w.put_u32(*index);
                w.put_lbytes(data);
            }
            FileMsg::Have { root, bitmap } => {
                w.put_u8(0x05);
                w.put_arr(root);
                w.put_lbytes(bitmap);
            }
            FileMsg::NotFound { root, index } => {
                w.put_u8(0x06);
                w.put_arr(root);
                w.put_u32(*index);
            }
        }
    }
}

impl WireDecode for FileMsg {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        match r.u8()? {
            0x01 => Ok(FileMsg::GetManifest { root: r.arr()? }),
            0x02 => Ok(FileMsg::ManifestMsg {
                manifest: Manifest::decode(r)?,
            }),
            0x03 => Ok(FileMsg::GetBlock {
                root: r.arr()?,
                index: r.u32()?,
            }),
            0x04 => Ok(FileMsg::Block {
                root: r.arr()?,
                index: r.u32()?,
                data: r.lbytes(limits::FILE_BLOCK_SIZE, "block.data")?,
            }),
            0x05 => Ok(FileMsg::Have {
                root: r.arr()?,
                bitmap: r.lbytes(MAX_LEAVES / 8 + 1, "have.bitmap")?,
            }),
            0x06 => Ok(FileMsg::NotFound {
                root: r.arr()?,
                index: r.u32()?,
            }),
            _ => Err(DecodeError::InvalidValue("file kind")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Manifest cohérent de `leaves` feuilles (la taille suit le nombre de
    /// feuilles, comme l'exige le décodage).
    fn manifest(leaves: usize) -> Manifest {
        Manifest {
            merkle_root: [1; 32],
            size: (leaves as u64) * limits::FILE_BLOCK_SIZE as u64,
            name: "archive.bin".into(),
            mime: "application/octet-stream".into(),
            leaf_hashes: vec![[2; 32]; leaves],
            publisher: [3; 32],
            sig: [4; 64],
        }
    }

    #[test]
    fn un_manifest_de_plus_dun_gibioctet_fait_laller_retour() {
        // 🔒 Non-régression. Un fichier de 1,5 Gio compte 6144 feuilles : au-delà
        // de l'ancienne borne par défaut des listes (4096), en deçà de la borne
        // réelle du champ (8192 = MAX_FILE_SIZE / FILE_BLOCK_SIZE). Il
        // s'encodait sans erreur et ne se décodait chez personne.
        let m = manifest(6144);
        assert!(m.leaf_hashes.len() > limits::MAX_LIST);
        assert_eq!(Manifest::from_bytes(&m.to_bytes()).unwrap(), m);
        // Et sa préimage de signature s'écrit sans paniquer en profil debug.
        assert!(!m.signable_bytes().is_empty());
    }

    #[test]
    fn un_manifest_au_maximum_du_format_est_accepte() {
        let m = manifest(MAX_LEAVES);
        assert_eq!(m.size, limits::MAX_FILE_SIZE);
        assert_eq!(Manifest::from_bytes(&m.to_bytes()).unwrap(), m);
    }

    #[test]
    fn au_dela_du_maximum_le_manifest_est_refuse_au_decodage() {
        // La borne du champ reste appliquée : une feuille de plus et la taille
        // dépasse MAX_FILE_SIZE, refusée avant même la liste.
        let m = manifest(MAX_LEAVES + 1);
        assert_eq!(
            Manifest::from_bytes(&m.to_bytes()),
            Err(DecodeError::TooLarge("manifest.size"))
        );
    }

    #[test]
    fn un_nombre_de_feuilles_incoherent_avec_la_taille_est_refuse() {
        // 🔒 C'est ce contrôle qui interdit d'annoncer 8192 feuilles pour trois
        // octets de fichier : le nombre de feuilles n'est jamais un choix libre.
        let mut m = manifest(4);
        m.size = limits::FILE_BLOCK_SIZE as u64;
        assert_eq!(
            Manifest::from_bytes(&m.to_bytes()),
            Err(DecodeError::InvalidValue("manifest.leaf_count"))
        );
    }
}
