//! # accord-proto
//!
//! Types de paquets, encodage binaire strict, framing et versionnage du
//! protocole filaire Accord. Ce crate est l'implémentation de référence de
//! `SPEC.md` : il ne contient aucune I/O ni cryptographie, uniquement des
//! types purs et leur (dé)sérialisation, ce qui le rend testable
//! exhaustivement et fuzzable.
//!
//! - [`wire`] : primitives d'encodage strictes (`Writer`/`Reader`).
//! - [`envelope`] : paquets externes HELLO/WELCOME/DATA/COOKIE.
//! - [`plaintext`] : contenu déchiffré des DATA, démultiplexé par canal.
//! - [`types`], [`dht_msg`], [`core_msg`], [`file_msg`] : structures métier.
//! - [`limits`] : bornes de décodage et constantes protocolaires.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod core_msg;
pub mod device;
pub mod dht_msg;
pub mod envelope;
pub mod file_msg;
pub mod limits;
pub mod plaintext;
pub mod types;
pub mod wire;

pub use device::{DeviceEntry, DeviceList, RevokedEntry};
pub use envelope::{CookiePacket, DataPacket, Hello, Packet, Welcome};
pub use limits::PROTOCOL_VERSION;
pub use plaintext::{ChannelMsg, ControlMsg, RelayMsg, VoiceMsg};
pub use types::{DhtRecord, NodeId, NodeInfo, RecordKind, WireAddr};
pub use wire::{DecodeError, Reader, WireDecode, WireEncode, Writer};

/// Encode un paquet pour un flux TCP : préfixe de longueur `u32` big-endian.
///
/// Erreur si le paquet dépasse [`limits::MAX_TCP_FRAME`].
pub fn tcp_frame(packet_bytes: &[u8]) -> Result<Vec<u8>, DecodeError> {
    if packet_bytes.len() > limits::MAX_TCP_FRAME {
        return Err(DecodeError::TooLarge("tcp frame"));
    }
    let mut out = Vec::with_capacity(4 + packet_bytes.len());
    out.extend_from_slice(&(packet_bytes.len() as u32).to_be_bytes());
    out.extend_from_slice(packet_bytes);
    Ok(out)
}

/// Extrait le prochain frame TCP complet d'un tampon de réception.
///
/// Retourne `Ok(None)` si le tampon est incomplet ; retourne le frame et le
/// nombre d'octets consommés sinon. Longueur > 1 MiB ⇒ erreur (connexion à
/// fermer).
pub fn tcp_deframe(buf: &[u8]) -> Result<Option<(Vec<u8>, usize)>, DecodeError> {
    if buf.len() < 4 {
        return Ok(None);
    }
    let len = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
    if len > limits::MAX_TCP_FRAME {
        return Err(DecodeError::TooLarge("tcp frame"));
    }
    if buf.len() < 4 + len {
        return Ok(None);
    }
    Ok(Some((buf[4..4 + len].to_vec(), 4 + len)))
}
