//! Enveloppe externe des paquets (SPEC §1) : HELLO, WELCOME, DATA, COOKIE.
//!
//! # Compatibilité du champ `capabilities`
//!
//! `Hello::capabilities` et `Welcome::capabilities` sont des champs **additifs
//! de fin de structure** : présents, ils occupent exactement 4 octets après la
//! signature ; absents, la structure est identique à celle des versions
//! antérieures. Le décodeur d'une version antérieure rejette tout octet
//! excédentaire (`TrailingBytes`) — **émettre le champ vers un pair trop
//! ancien lui rend le handshake indécodable**. La politique d'émission est donc
//! décidée par la couche transport, pas ici : voir
//! `accord_transport::EndpointConfig::capabilities`.

use crate::limits::{self, PROTOCOL_VERSION};
use crate::wire::{DecodeError, Reader, WireDecode, WireEncode, Writer};

/// Taille de l'en-tête AAD d'un paquet DATA : version(1) + class(1) +
/// session_id(8) + epoch(1) + counter(8).
pub const DATA_HEADER_LEN: usize = 19;

/// Message HELLO du handshake (initiateur → répondeur), classe 0x01.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hello {
    /// Clé publique X25519 éphémère de l'initiateur.
    pub eph_pub: [u8; 32],
    /// Clé publique Ed25519 statique de l'initiateur.
    pub static_pub: [u8; 32],
    /// Nonce de preuve de travail de l'identité.
    pub pow_nonce: u64,
    /// Horloge murale UNIX en millisecondes.
    pub timestamp_ms: u64,
    /// Nonce anti-rejeu du handshake.
    pub nonce: [u8; 16],
    /// Cookie anti-DoS (vide en régime normal).
    pub cookie: Vec<u8>,
    /// Signature Ed25519 du transcript_1.
    pub sig: [u8; 64],
    /// Bitmask de capacités de l'émetteur, **champ additif de fin de
    /// structure** : `None` chez un émetteur antérieur à son introduction.
    /// Voir [`crate::limits::CAP_KNOWN`] et la note de compatibilité de ce
    /// module.
    pub capabilities: Option<u32>,
}

/// Message WELCOME du handshake (répondeur → initiateur), classe 0x02.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Welcome {
    /// Clé publique X25519 éphémère du répondeur.
    pub eph_pub: [u8; 32],
    /// Clé publique Ed25519 statique du répondeur.
    pub static_pub: [u8; 32],
    /// Nonce de preuve de travail de l'identité.
    pub pow_nonce: u64,
    /// Horloge murale UNIX en millisecondes.
    pub timestamp_ms: u64,
    /// Nonce anti-rejeu du handshake.
    pub nonce: [u8; 16],
    /// Identifiant de session choisi par le répondeur.
    pub session_id: [u8; 8],
    /// Signature Ed25519 du transcript_2.
    pub sig: [u8; 64],
    /// Bitmask de capacités de l'émetteur, **champ additif de fin de
    /// structure** (même règle que [`Hello::capabilities`]).
    pub capabilities: Option<u32>,
}

/// Paquet DATA chiffré, classe 0x03. Tout le protocole applicatif y transite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataPacket {
    /// Identifiant de session.
    pub session_id: [u8; 8],
    /// Génération de clé (re-keying).
    pub epoch: u8,
    /// Compteur d'émission strictement croissant par direction.
    pub counter: u64,
    /// Charge chiffrée XChaCha20-Poly1305.
    pub ciphertext: Vec<u8>,
}

impl DataPacket {
    /// En-tête servant d'AAD à l'AEAD (les 19 premiers octets du paquet).
    pub fn aad(&self) -> [u8; DATA_HEADER_LEN] {
        let mut aad = [0u8; DATA_HEADER_LEN];
        aad[0] = PROTOCOL_VERSION;
        aad[1] = 0x03;
        aad[2..10].copy_from_slice(&self.session_id);
        aad[10] = self.epoch;
        aad[11..19].copy_from_slice(&self.counter.to_be_bytes());
        aad
    }
}

/// Paquet COOKIE anti-DoS, classe 0x04 (SPEC §2.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CookiePacket {
    /// Cookie opaque à renvoyer dans le HELLO suivant.
    pub cookie: Vec<u8>,
}

/// Paquet externe décodé.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Packet {
    /// Handshake initiateur.
    Hello(Hello),
    /// Handshake répondeur.
    Welcome(Welcome),
    /// Trame chiffrée de session.
    Data(DataPacket),
    /// Défi anti-DoS.
    Cookie(CookiePacket),
}

const MAX_COOKIE: usize = 64;

/// Lit le champ additif `capabilities` en fin de HELLO/WELCOME : absent si le
/// tampon est épuisé, sinon exactement 4 octets. Un reliquat de 1 à 3 octets
/// reste une erreur (structure malformée, pas une extension future).
fn read_capabilities(r: &mut Reader<'_>) -> Result<Option<u32>, DecodeError> {
    if r.remaining() == 0 {
        return Ok(None);
    }
    Ok(Some(r.u32()?))
}

impl WireEncode for Packet {
    fn encode(&self, w: &mut Writer) {
        w.put_u8(PROTOCOL_VERSION);
        match self {
            Packet::Hello(h) => {
                w.put_u8(0x01);
                w.put_arr(&h.eph_pub);
                w.put_arr(&h.static_pub);
                w.put_u64(h.pow_nonce);
                w.put_u64(h.timestamp_ms);
                w.put_arr(&h.nonce);
                w.put_vbytes(&h.cookie);
                w.put_arr(&h.sig);
                if let Some(caps) = h.capabilities {
                    w.put_u32(caps);
                }
            }
            Packet::Welcome(m) => {
                w.put_u8(0x02);
                w.put_arr(&m.eph_pub);
                w.put_arr(&m.static_pub);
                w.put_u64(m.pow_nonce);
                w.put_u64(m.timestamp_ms);
                w.put_arr(&m.nonce);
                w.put_arr(&m.session_id);
                w.put_arr(&m.sig);
                if let Some(caps) = m.capabilities {
                    w.put_u32(caps);
                }
            }
            Packet::Data(d) => {
                w.put_u8(0x03);
                w.put_arr(&d.session_id);
                w.put_u8(d.epoch);
                w.put_u64(d.counter);
                w.put_raw(&d.ciphertext);
            }
            Packet::Cookie(c) => {
                w.put_u8(0x04);
                w.put_vbytes(&c.cookie);
            }
        }
    }
}

impl WireDecode for Packet {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        let version = r.u8()?;
        if version == 0 {
            return Err(DecodeError::InvalidValue("version 0"));
        }
        if version > PROTOCOL_VERSION {
            return Err(DecodeError::UnsupportedVersion(version));
        }
        match r.u8()? {
            0x01 => Ok(Packet::Hello(Hello {
                eph_pub: r.arr()?,
                static_pub: r.arr()?,
                pow_nonce: r.u64()?,
                timestamp_ms: r.u64()?,
                nonce: r.arr()?,
                cookie: r.vbytes(MAX_COOKIE, "hello.cookie")?,
                sig: r.arr()?,
                capabilities: read_capabilities(r)?,
            })),
            0x02 => Ok(Packet::Welcome(Welcome {
                eph_pub: r.arr()?,
                static_pub: r.arr()?,
                pow_nonce: r.u64()?,
                timestamp_ms: r.u64()?,
                nonce: r.arr()?,
                session_id: r.arr()?,
                sig: r.arr()?,
                capabilities: read_capabilities(r)?,
            })),
            0x03 => {
                let session_id = r.arr()?;
                let epoch = r.u8()?;
                let counter = r.u64()?;
                let ciphertext = r.rest().to_vec();
                if ciphertext.len() > limits::MAX_TCP_FRAME {
                    return Err(DecodeError::TooLarge("data.ciphertext"));
                }
                Ok(Packet::Data(DataPacket {
                    session_id,
                    epoch,
                    counter,
                    ciphertext,
                }))
            }
            0x04 => Ok(Packet::Cookie(CookiePacket {
                cookie: r.vbytes(MAX_COOKIE, "cookie.cookie")?,
            })),
            _ => Err(DecodeError::InvalidValue("packet_class")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hello(capabilities: Option<u32>) -> Hello {
        Hello {
            eph_pub: [1; 32],
            static_pub: [2; 32],
            pow_nonce: 7,
            timestamp_ms: 1_700_000_000_000,
            nonce: [3; 16],
            cookie: vec![9; 16],
            sig: [4; 64],
            capabilities,
        }
    }

    fn welcome(capabilities: Option<u32>) -> Welcome {
        Welcome {
            eph_pub: [5; 32],
            static_pub: [6; 32],
            pow_nonce: 11,
            timestamp_ms: 1_700_000_000_000,
            nonce: [7; 16],
            session_id: [8; 8],
            sig: [9; 64],
            capabilities,
        }
    }

    #[test]
    fn hello_without_capabilities_roundtrips_byte_identically() {
        let packet = Packet::Hello(hello(None));
        let bytes = packet.to_bytes();
        assert_eq!(Packet::from_bytes(&bytes).unwrap(), packet);
    }

    #[test]
    fn hello_with_capabilities_costs_exactly_four_bytes() {
        let sans = Packet::Hello(hello(None)).to_bytes();
        let avec = Packet::Hello(hello(Some(0x0000_0005))).to_bytes();
        assert_eq!(avec.len(), sans.len() + 4);
        assert_eq!(avec[..sans.len()], sans[..]);
        let decoded = Packet::from_bytes(&avec).unwrap();
        let Packet::Hello(h) = decoded else {
            panic!("attendu un HELLO");
        };
        assert_eq!(h.capabilities, Some(5));
    }

    #[test]
    fn welcome_capabilities_roundtrip() {
        for caps in [None, Some(0), Some(u32::MAX)] {
            let packet = Packet::Welcome(welcome(caps));
            assert_eq!(Packet::from_bytes(&packet.to_bytes()).unwrap(), packet);
        }
    }

    #[test]
    fn absent_and_zero_capabilities_are_distinct_on_the_wire() {
        assert_ne!(
            Packet::Hello(hello(None)).to_bytes(),
            Packet::Hello(hello(Some(0))).to_bytes()
        );
    }

    #[test]
    fn truncated_capabilities_field_is_rejected() {
        let mut bytes = Packet::Hello(hello(Some(1))).to_bytes();
        bytes.pop();
        assert_eq!(
            Packet::from_bytes(&bytes).unwrap_err(),
            DecodeError::UnexpectedEof
        );
    }

    #[test]
    fn unknown_capability_bits_survive_decoding() {
        // Les bits inconnus doivent traverser le décodage intacts : c'est aux
        // couches hautes de les ignorer, jamais au décodeur de les rejeter.
        let caps = 0xdead_beef;
        let bytes = Packet::Hello(hello(Some(caps))).to_bytes();
        let Packet::Hello(h) = Packet::from_bytes(&bytes).unwrap() else {
            panic!("attendu un HELLO");
        };
        assert_eq!(h.capabilities, Some(caps));
        assert_ne!(caps & !crate::limits::CAP_KNOWN, 0);
    }
}
