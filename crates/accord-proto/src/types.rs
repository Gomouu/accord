//! Types partagés du protocole : identifiants, adresses, infos de nœud,
//! records DHT (SPEC §2.1, §3, §4).

use crate::limits;
use crate::wire::{DecodeError, Reader, WireDecode, WireEncode, Writer};
use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

/// Identifiant de nœud : SHA-256 de la clé publique Ed25519 (256 bits).
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub [u8; 32]);

impl NodeId {
    /// Distance XOR entre deux identifiants (métrique Kademlia).
    pub fn distance(&self, other: &NodeId) -> [u8; 32] {
        let mut d = [0u8; 32];
        for (i, b) in d.iter_mut().enumerate() {
            *b = self.0[i] ^ other.0[i];
        }
        d
    }

    /// Index du bucket Kademlia pour `other` vu depuis `self` :
    /// position du bit de poids fort de la distance XOR (0..=255),
    /// ou `None` si les identifiants sont égaux.
    pub fn bucket_index(&self, other: &NodeId) -> Option<usize> {
        let d = self.distance(other);
        for (byte_idx, byte) in d.iter().enumerate() {
            if *byte != 0 {
                return Some(255 - (byte_idx * 8 + byte.leading_zeros() as usize));
            }
        }
        None
    }
}

impl fmt::Debug for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for b in &self.0[..6] {
            write!(f, "{b:02x}")?;
        }
        write!(f, "…")
    }
}

impl WireEncode for NodeId {
    fn encode(&self, w: &mut Writer) {
        w.put_arr(&self.0);
    }
}

impl WireDecode for NodeId {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(NodeId(r.arr()?))
    }
}

/// Adresse de transport filaire : `family:u8(4|6) ‖ ip ‖ port:u16`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WireAddr(pub SocketAddr);

impl WireEncode for WireAddr {
    fn encode(&self, w: &mut Writer) {
        match self.0 {
            SocketAddr::V4(a) => {
                w.put_u8(4);
                w.put_arr(&a.ip().octets());
                w.put_u16(a.port());
            }
            SocketAddr::V6(a) => {
                w.put_u8(6);
                w.put_arr(&a.ip().octets());
                w.put_u16(a.port());
            }
        }
    }
}

impl WireDecode for WireAddr {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        match r.u8()? {
            4 => {
                let ip: [u8; 4] = r.arr()?;
                let port = r.u16()?;
                Ok(WireAddr(SocketAddr::new(
                    IpAddr::V4(Ipv4Addr::from(ip)),
                    port,
                )))
            }
            6 => {
                let ip: [u8; 16] = r.arr()?;
                let port = r.u16()?;
                Ok(WireAddr(SocketAddr::new(
                    IpAddr::V6(Ipv6Addr::from(ip)),
                    port,
                )))
            }
            _ => Err(DecodeError::InvalidValue("addr family")),
        }
    }
}

/// Drapeaux de capacité d'un nœud (bitfield u8).
pub mod node_flags {
    /// Le nœud accepte de relayer du trafic (publiquement joignable).
    pub const RELAY: u8 = 0x01;
}

/// Coordonnées complètes d'un nœud, telles qu'échangées dans la DHT.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeInfo {
    /// Identifiant (doit être SHA-256 de `static_pub` — vérifié à la réception).
    pub node_id: NodeId,
    /// Clé publique Ed25519 du nœud.
    pub static_pub: [u8; 32],
    /// Nonce de preuve de travail de l'identité.
    pub pow_nonce: u64,
    /// Drapeaux de capacité (`node_flags`).
    pub flags: u8,
    /// Adresses candidates (≤ 4).
    pub addrs: Vec<WireAddr>,
}

impl WireEncode for NodeInfo {
    fn encode(&self, w: &mut Writer) {
        self.node_id.encode(w);
        w.put_arr(&self.static_pub);
        w.put_u64(self.pow_nonce);
        w.put_u8(self.flags);
        w.put_list(&self.addrs, |w, a| a.encode(w));
    }
}

impl WireDecode for NodeInfo {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(NodeInfo {
            node_id: NodeId::decode(r)?,
            static_pub: r.arr()?,
            pow_nonce: r.u64()?,
            flags: r.u8()?,
            addrs: r.list(limits::MAX_NODE_ADDRS, "nodeinfo.addrs", WireAddr::decode)?,
        })
    }
}

/// Nature d'un record DHT (SPEC §4).
///
/// 🔒 **Un genre inconnu ne doit jamais faire échouer un décodage.** Les
/// records voyagent par listes dans les réponses de lookup : si un seul genre
/// non reconnu rejetait la structure, l'ajout d'un genre dans une version
/// future ferait perdre **toute la réponse** aux nœuds plus anciens — pas
/// seulement le record en trop. D'où la variante [`RecordKind::Unknown`] :
/// le décodage préserve l'octet tel quel, et c'est la couche de stockage qui
/// refuse d'accueillir ce qu'elle ne sait pas valider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordKind {
    /// Code ami → identité complète signée.
    Identity,
    /// Adresses courantes signées d'un nœud.
    Presence,
    /// Fragment de boîte aux lettres chiffrée.
    MailboxHint,
    /// Annonce de disponibilité d'un fichier.
    FileProvider,
    /// Genre introduit par une version plus récente. Transporté intact —
    /// la signature du record couvre cet octet — mais jamais stocké.
    Unknown(u8),
}

impl RecordKind {
    /// Décode le discriminant filaire. Infaillible par construction : un
    /// discriminant inconnu devient [`RecordKind::Unknown`].
    pub fn from_u8(v: u8) -> Self {
        match v {
            0x01 => Self::Identity,
            0x02 => Self::Presence,
            0x03 => Self::MailboxHint,
            0x04 => Self::FileProvider,
            other => Self::Unknown(other),
        }
    }

    /// Discriminant filaire. Rend l'octet d'origine pour un genre inconnu,
    /// sans quoi un record relayé verrait sa signature invalidée.
    pub fn to_u8(self) -> u8 {
        match self {
            Self::Identity => 0x01,
            Self::Presence => 0x02,
            Self::MailboxHint => 0x03,
            Self::FileProvider => 0x04,
            Self::Unknown(v) => v,
        }
    }
}

/// Record signé stocké dans la DHT.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DhtRecord {
    /// Clé de stockage (256 bits).
    pub key: [u8; 32],
    /// Nature du record.
    pub kind: RecordKind,
    /// Valeur opaque (≤ 8 KiB).
    pub value: Vec<u8>,
    /// Clé publique Ed25519 du publieur.
    pub publisher: [u8; 32],
    /// Horloge murale de publication (ms).
    pub timestamp_ms: u64,
    /// Durée de vie en secondes (≤ 7 jours).
    pub expiry_s: u32,
    /// Signature Ed25519 du publieur sur [`DhtRecord::signable_bytes`].
    pub sig: [u8; 64],
}

impl DhtRecord {
    /// Octets couverts par la signature du record :
    /// `key ‖ kind ‖ value ‖ timestamp_ms ‖ expiry_s`.
    pub fn signable_bytes(&self) -> Vec<u8> {
        let mut w = Writer::with_capacity(self.value.len() + 64);
        w.put_arr(&self.key);
        w.put_u8(self.kind.to_u8());
        w.put_lbytes(&self.value);
        w.put_u64(self.timestamp_ms);
        w.put_u32(self.expiry_s);
        w.into_bytes()
    }
}

impl WireEncode for DhtRecord {
    fn encode(&self, w: &mut Writer) {
        w.put_arr(&self.key);
        w.put_u8(self.kind.to_u8());
        w.put_lbytes(&self.value);
        w.put_arr(&self.publisher);
        w.put_u64(self.timestamp_ms);
        w.put_u32(self.expiry_s);
        w.put_arr(&self.sig);
    }
}

impl WireDecode for DhtRecord {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        let key = r.arr()?;
        let kind = RecordKind::from_u8(r.u8()?);
        let value = r.lbytes(limits::MAX_DHT_VALUE, "record.value")?;
        let publisher = r.arr()?;
        let timestamp_ms = r.u64()?;
        let expiry_s = r.u32()?;
        if expiry_s > limits::DHT_MAX_EXPIRY_S {
            return Err(DecodeError::TooLarge("record.expiry"));
        }
        let sig = r.arr()?;
        Ok(DhtRecord {
            key,
            kind,
            value,
            publisher,
            timestamp_ms,
            expiry_s,
            sig,
        })
    }
}
