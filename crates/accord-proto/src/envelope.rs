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
//!
//! # Matériel post-quantique
//!
//! `Hello::pq_ek` et `Welcome::pq_ct` suivent immédiatement `capabilities`, et
//! le bit [`limits::CAP_PQ_HYBRID`] de ce champ est la **seule** source de
//! vérité sur leur présence : bit posé ⇒ exactement
//! [`limits::MLKEM512_EK_BYTES`] (resp. [`limits::MLKEM512_CT_BYTES`]) octets ;
//! bit absent ⇒ rien du tout.
//!
//! 🔒 Aucun préfixe de longueur n'est lu sur le fil. Le handshake se décode
//! **avant** qu'une session existe : rien n'y est authentifié, et une longueur
//! choisie par l'émetteur y ouvrirait une voie d'épuisement mémoire. La taille
//! vient donc du jeu de paramètres du protocole. Corollaire : une structure
//! incohérente (bit posé sans matériel, ou matériel sans bit) produit des
//! octets que le décodeur rejette — l'échec est fermé, jamais approximatif.

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
    /// Clé d'encapsulation ML-KEM-512 de l'initiateur, présente **si et
    /// seulement si** `capabilities` porte [`limits::CAP_PQ_HYBRID`]. Boxée :
    /// un `Hello` classique ne paie alors que 8 octets, pas 800.
    pub pq_ek: Option<Box<[u8; limits::MLKEM512_EK_BYTES]>>,
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
    /// Chiffré d'encapsulation ML-KEM-512 produit vers la clé du HELLO,
    /// présent **si et seulement si** `capabilities` porte
    /// [`limits::CAP_PQ_HYBRID`] (même règle que [`Hello::pq_ek`]).
    pub pq_ct: Option<Box<[u8; limits::MLKEM512_CT_BYTES]>>,
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

/// Vrai si les capacités annoncées engagent l'émetteur à joindre son matériel
/// post-quantique. Encodeur et décodeur passent tous deux par ici : c'est ce
/// qui garantit qu'ils lisent la même chose au même endroit.
fn announces_pq(capabilities: Option<u32>) -> bool {
    capabilities.is_some_and(|caps| caps & limits::CAP_PQ_HYBRID != 0)
}

/// Lit le matériel post-quantique qui suit `capabilities` : exactement `N`
/// octets si le bit est posé, rien sinon (voir la note de module).
fn read_pq_material<const N: usize>(
    r: &mut Reader<'_>,
    capabilities: Option<u32>,
) -> Result<Option<Box<[u8; N]>>, DecodeError> {
    if !announces_pq(capabilities) {
        return Ok(None);
    }
    Ok(Some(Box::new(r.arr::<N>()?)))
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
                // Miroir exact de `read_pq_material` : le bit commande, le
                // champ suit. Un `Hello` incohérent produit donc des octets
                // que le décodeur rejette plutôt qu'une lecture décalée.
                if announces_pq(h.capabilities) {
                    if let Some(ek) = &h.pq_ek {
                        w.put_raw(&ek[..]);
                    }
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
                if announces_pq(m.capabilities) {
                    if let Some(ct) = &m.pq_ct {
                        w.put_raw(&ct[..]);
                    }
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
            0x01 => {
                let eph_pub = r.arr()?;
                let static_pub = r.arr()?;
                let pow_nonce = r.u64()?;
                let timestamp_ms = r.u64()?;
                let nonce = r.arr()?;
                let cookie = r.vbytes(MAX_COOKIE, "hello.cookie")?;
                let sig = r.arr()?;
                // Ordre imposé : les capacités décrivent ce qui les suit, elles
                // doivent donc être lues avant.
                let capabilities = read_capabilities(r)?;
                let pq_ek = read_pq_material(r, capabilities)?;
                Ok(Packet::Hello(Hello {
                    eph_pub,
                    static_pub,
                    pow_nonce,
                    timestamp_ms,
                    nonce,
                    cookie,
                    sig,
                    capabilities,
                    pq_ek,
                }))
            }
            0x02 => {
                let eph_pub = r.arr()?;
                let static_pub = r.arr()?;
                let pow_nonce = r.u64()?;
                let timestamp_ms = r.u64()?;
                let nonce = r.arr()?;
                let session_id = r.arr()?;
                let sig = r.arr()?;
                let capabilities = read_capabilities(r)?;
                let pq_ct = read_pq_material(r, capabilities)?;
                Ok(Packet::Welcome(Welcome {
                    eph_pub,
                    static_pub,
                    pow_nonce,
                    timestamp_ms,
                    nonce,
                    session_id,
                    sig,
                    capabilities,
                    pq_ct,
                }))
            }
            0x03 => {
                let session_id = r.arr()?;
                let epoch = r.u8()?;
                let counter = r.u64()?;
                // 🔒 Borne vérifiée AVANT la copie : le tampon vient du réseau
                // et `rest()` en rend tout ce qui reste. Copier d'abord pour
                // refuser ensuite faisait payer l'allocation entière à un
                // paquet qu'on allait de toute façon jeter.
                if r.remaining() > limits::MAX_TCP_FRAME {
                    return Err(DecodeError::TooLarge("data.ciphertext"));
                }
                let ciphertext = r.rest().to_vec();
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
            // Cohérent par construction : matériel présent ssi le bit l'est.
            pq_ek: announces_pq(capabilities).then(|| Box::new([0xAB; limits::MLKEM512_EK_BYTES])),
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
            pq_ct: announces_pq(capabilities).then(|| Box::new([0xCD; limits::MLKEM512_CT_BYTES])),
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
    fn pq_material_costs_exactly_its_parameter_set_size() {
        let sans = Packet::Hello(hello(Some(0))).to_bytes();
        let avec = Packet::Hello(hello(Some(limits::CAP_PQ_HYBRID))).to_bytes();
        assert_eq!(avec.len(), sans.len() + limits::MLKEM512_EK_BYTES);
        // Le matériel s'ajoute en fin : tout ce qui précède les 4 octets de
        // capacités (seule autre différence entre les deux paquets) est intact.
        let avant_capacites = sans.len() - 4;
        assert_eq!(avec[..avant_capacites], sans[..avant_capacites]);

        let sans = Packet::Welcome(welcome(Some(0))).to_bytes();
        let avec = Packet::Welcome(welcome(Some(limits::CAP_PQ_HYBRID))).to_bytes();
        assert_eq!(avec.len(), sans.len() + limits::MLKEM512_CT_BYTES);
        let avant_capacites = sans.len() - 4;
        assert_eq!(avec[..avant_capacites], sans[..avant_capacites]);
    }

    #[test]
    fn pq_hello_and_welcome_roundtrip() {
        for packet in [
            Packet::Hello(hello(Some(limits::CAP_PQ_HYBRID))),
            Packet::Welcome(welcome(Some(
                limits::CAP_PQ_HYBRID | limits::CAP_DEVICE_KEYS,
            ))),
        ] {
            assert_eq!(Packet::from_bytes(&packet.to_bytes()).unwrap(), packet);
        }
    }

    #[test]
    fn pq_bit_without_material_is_rejected() {
        // 🔒 Le bit engage l'émetteur. Un HELLO qui l'annonce sans joindre les
        // octets se termine trop tôt : on refuse plutôt que de deviner.
        let mut h = hello(Some(limits::CAP_PQ_HYBRID));
        h.pq_ek = None;
        assert_eq!(
            Packet::from_bytes(&Packet::Hello(h).to_bytes()).unwrap_err(),
            DecodeError::UnexpectedEof
        );
    }

    #[test]
    fn pq_material_without_the_bit_is_rejected() {
        // 🔒 Réciproque : des octets non annoncés restent des octets de trop.
        // Un attaquant qui efface le bit en laissant le matériel n'obtient pas
        // une lecture décalée, il obtient un rejet.
        let mut bytes = Packet::Hello(hello(Some(limits::CAP_PQ_HYBRID))).to_bytes();
        let bit_offset = bytes.len() - limits::MLKEM512_EK_BYTES - 4;
        bytes[bit_offset..bit_offset + 4].copy_from_slice(&0u32.to_be_bytes());
        assert_eq!(
            Packet::from_bytes(&bytes).unwrap_err(),
            DecodeError::TrailingBytes
        );
    }

    #[test]
    fn truncated_pq_material_is_rejected() {
        let mut bytes = Packet::Hello(hello(Some(limits::CAP_PQ_HYBRID))).to_bytes();
        bytes.pop();
        assert_eq!(
            Packet::from_bytes(&bytes).unwrap_err(),
            DecodeError::UnexpectedEof
        );
    }

    #[test]
    fn pq_hello_stays_under_the_udp_mtu() {
        // 🔒 Non-régression de taille (ROADMAP §7.4) : le handshake hybride doit
        // tenir dans UN datagramme. Le réassemblage précède l'établissement de
        // session, donc il n'est pas authentifié — fragmenter le HELLO ouvrirait
        // une surface d'épuisement mémoire. Le cookie anti-DoS (16 o) est compté
        // ici parce qu'il est présent au pire moment, sous pression.
        let mut h = hello(Some(limits::CAP_PQ_HYBRID | limits::CAP_KNOWN));
        h.cookie = vec![0; 16];
        let hello_len = Packet::Hello(h).to_bytes().len();
        let welcome_len = Packet::Welcome(welcome(Some(limits::CAP_PQ_HYBRID)))
            .to_bytes()
            .len();
        assert!(
            hello_len <= limits::UDP_MTU,
            "HELLO hybride de {hello_len} o dépasse UDP_MTU ({})",
            limits::UDP_MTU
        );
        assert!(
            welcome_len <= limits::UDP_MTU,
            "WELCOME hybride de {welcome_len} o dépasse UDP_MTU ({})",
            limits::UDP_MTU
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
