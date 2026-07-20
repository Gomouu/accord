//! Sessions chiffrées XChaCha20-Poly1305 avec compteurs stricts par
//! direction, fenêtre anti-rejeu et re-keying par epochs (SPEC §2.4).

use crate::error::CryptoError;
use accord_proto::envelope::DataPacket;
use accord_proto::limits::{REKEY_FRAME_LIMIT, REKEY_MAX_AGE_S};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Paire de clés de session dérivées du handshake (epoch 0).
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct SessionKeys {
    k_i2r: [u8; 32],
    k_r2i: [u8; 32],
}

impl SessionKeys {
    /// Construit la paire (initiateur→répondeur, répondeur→initiateur).
    pub fn new(k_i2r: [u8; 32], k_r2i: [u8; 32]) -> Self {
        Self { k_i2r, k_r2i }
    }

    /// Comparaison de clés pour les tests (non constant-time, tests only).
    pub fn same_keys(&self, other: &Self) -> bool {
        self.k_i2r == other.k_i2r && self.k_r2i == other.k_r2i
    }
}

/// Ratchet d'epoch : `k' = HKDF-Expand(HKDF-Extract(0, k), "accord-rekey", 32)`.
fn next_epoch_key(k: &[u8; 32]) -> [u8; 32] {
    let hk = Hkdf::<Sha256>::new(None, k);
    let mut out = [0u8; 32];
    crate::hkdf_expand_fixe(&hk, b"accord-rekey", &mut out);
    out
}

fn nonce_for(direction: u8, epoch: u8, counter: u64) -> XNonce {
    let mut n = [0u8; 24];
    n[0] = direction;
    n[1] = epoch;
    n[8..16].copy_from_slice(&counter.to_be_bytes());
    *XNonce::from_slice(&n)
}

/// Fenêtre anti-rejeu glissante de 1 024 compteurs (type RFC 6479).
#[derive(Default)]
struct ReplayWindow {
    max: u64,
    seen_any: bool,
    bitmap: [u64; 16], // 1024 bits
}

const WINDOW: u64 = 1024;

impl ReplayWindow {
    fn bit(counter: u64) -> (usize, u64) {
        let idx = (counter % WINDOW) as usize;
        (idx / 64, 1u64 << (idx % 64))
    }

    /// Vérifie sans marquer (avant déchiffrement).
    fn check(&self, counter: u64) -> Result<(), CryptoError> {
        if !self.seen_any {
            return Ok(());
        }
        if counter > self.max {
            return Ok(());
        }
        if self.max - counter >= WINDOW {
            return Err(CryptoError::FrameReplay);
        }
        let (word, mask) = Self::bit(counter);
        if self.bitmap[word] & mask != 0 {
            return Err(CryptoError::FrameReplay);
        }
        Ok(())
    }

    /// Marque un compteur comme vu (après déchiffrement réussi).
    fn mark(&mut self, counter: u64) {
        if !self.seen_any || counter > self.max {
            let start = if self.seen_any { self.max + 1 } else { 0 };
            // Efface les bits entre l'ancien max et le nouveau.
            let span = counter.saturating_sub(start).min(WINDOW);
            for i in 0..=span {
                let (word, mask) = Self::bit(start.saturating_add(i).min(counter));
                self.bitmap[word] &= !mask;
            }
            self.max = counter;
            self.seen_any = true;
        }
        let (word, mask) = Self::bit(counter);
        self.bitmap[word] |= mask;
    }
}

/// État d'une clé de réception pour un epoch donné.
struct RecvEpoch {
    epoch: u8,
    key: [u8; 32],
    window: ReplayWindow,
}

impl Drop for RecvEpoch {
    fn drop(&mut self) {
        self.key.zeroize();
    }
}

/// Chiffreur/déchiffreur d'une session établie.
///
/// Gère les compteurs d'émission, la fenêtre anti-rejeu en réception et le
/// re-keying par epochs. Une instance par session, non partagée entre threads
/// sans verrou externe.
pub struct SessionCrypto {
    session_id: [u8; 8],
    is_initiator: bool,
    send_key: [u8; 32],
    send_epoch: u8,
    send_counter: u64,
    /// Epoch courant + précédent conservés (SPEC : 2 epochs actifs).
    recv: Vec<RecvEpoch>,
    recv_base_epoch: u8,
    created_ms: u64,
}

impl Drop for SessionCrypto {
    fn drop(&mut self) {
        self.send_key.zeroize();
    }
}

impl SessionCrypto {
    /// Construit l'état de session à partir du résultat du handshake.
    pub fn new(keys: &SessionKeys, session_id: [u8; 8], is_initiator: bool, now_ms: u64) -> Self {
        let (send_key, recv_key) = if is_initiator {
            (keys.k_i2r, keys.k_r2i)
        } else {
            (keys.k_r2i, keys.k_i2r)
        };
        Self {
            session_id,
            is_initiator,
            send_key,
            send_epoch: 0,
            send_counter: 0,
            recv: vec![RecvEpoch {
                epoch: 0,
                key: recv_key,
                window: ReplayWindow::default(),
            }],
            recv_base_epoch: 0,
            created_ms: now_ms,
        }
    }

    /// Identifiant de session.
    pub fn session_id(&self) -> [u8; 8] {
        self.session_id
    }

    /// Direction d'émission (octet de nonce).
    fn send_direction(&self) -> u8 {
        u8::from(!self.is_initiator)
    }

    /// Vrai quand l'émetteur doit passer à l'epoch suivant (SPEC §2.4).
    pub fn needs_rekey(&self, now_ms: u64) -> bool {
        self.send_counter >= REKEY_FRAME_LIMIT
            || now_ms.saturating_sub(self.created_ms) >= REKEY_MAX_AGE_S * 1000
    }

    /// Epoch d'émission courant.
    pub fn send_epoch(&self) -> u8 {
        self.send_epoch
    }

    /// Passe l'émission à l'epoch suivant. Erreur à l'épuisement de l'espace
    /// d'epochs (255) : la session doit être re-négociée par handshake.
    pub fn advance_send_epoch(&mut self, now_ms: u64) -> Result<u8, CryptoError> {
        if self.send_epoch == u8::MAX {
            return Err(CryptoError::RekeyRequired);
        }
        let new_key = next_epoch_key(&self.send_key);
        self.send_key.zeroize();
        self.send_key = new_key;
        self.send_epoch += 1;
        self.send_counter = 0;
        self.created_ms = now_ms;
        Ok(self.send_epoch)
    }

    /// Chiffre un plaintext en paquet DATA.
    pub fn seal(&mut self, plaintext: &[u8]) -> Result<DataPacket, CryptoError> {
        if self.send_counter >= REKEY_FRAME_LIMIT.saturating_mul(2) {
            // Garde-fou absolu : l'appelant aurait dû re-keyer à 1M.
            return Err(CryptoError::RekeyRequired);
        }
        let counter = self.send_counter;
        self.send_counter += 1;
        let mut packet = DataPacket {
            session_id: self.session_id,
            epoch: self.send_epoch,
            counter,
            ciphertext: Vec::new(),
        };
        let aad = packet.aad();
        let cipher = XChaCha20Poly1305::new((&self.send_key).into());
        packet.ciphertext = cipher
            .encrypt(
                &nonce_for(self.send_direction(), self.send_epoch, counter),
                Payload {
                    msg: plaintext,
                    aad: &aad,
                },
            )
            .map_err(|_| CryptoError::DecryptFailed)?;
        Ok(packet)
    }

    fn recv_epoch_index(&mut self, epoch: u8) -> Result<usize, CryptoError> {
        if let Some(i) = self.recv.iter().position(|e| e.epoch == epoch) {
            return Ok(i);
        }
        // Epoch immédiatement suivant : dérive la clé et rote (garde 2 epochs).
        let newest = self.recv_base_epoch;
        if epoch == newest.wrapping_add(1) && newest < u8::MAX {
            let latest_key = self
                .recv
                .iter()
                .find(|e| e.epoch == newest)
                .map(|e| e.key)
                .ok_or(CryptoError::UnknownEpoch(epoch))?;
            let key = next_epoch_key(&latest_key);
            self.recv.push(RecvEpoch {
                epoch,
                key,
                window: ReplayWindow::default(),
            });
            self.recv_base_epoch = epoch;
            // Ne conserve que les 2 epochs les plus récents.
            while self.recv.len() > 2 {
                self.recv.remove(0);
            }
            return Ok(self.recv.len() - 1);
        }
        Err(CryptoError::UnknownEpoch(epoch))
    }

    /// Déchiffre un paquet DATA reçu, avec contrôle d'anti-rejeu.
    pub fn open(&mut self, packet: &DataPacket) -> Result<Vec<u8>, CryptoError> {
        if packet.session_id != self.session_id {
            return Err(CryptoError::DecryptFailed);
        }
        let direction = u8::from(self.is_initiator); // direction du pair
        let idx = self.recv_epoch_index(packet.epoch)?;
        self.recv[idx].window.check(packet.counter)?;
        let aad = packet.aad();
        let cipher = XChaCha20Poly1305::new((&self.recv[idx].key).into());
        let plaintext = cipher
            .decrypt(
                &nonce_for(direction, packet.epoch, packet.counter),
                Payload {
                    msg: &packet.ciphertext,
                    aad: &aad,
                },
            )
            .map_err(|_| CryptoError::DecryptFailed)?;
        self.recv[idx].window.mark(packet.counter);
        Ok(plaintext)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn linked_pair() -> (SessionCrypto, SessionCrypto) {
        let keys = SessionKeys::new([1; 32], [2; 32]);
        let a = SessionCrypto::new(&keys, [9; 8], true, 0);
        let b = SessionCrypto::new(&keys, [9; 8], false, 0);
        (a, b)
    }

    #[test]
    fn seal_open_roundtrip_both_directions() {
        let (mut a, mut b) = linked_pair();
        let p1 = a.seal(b"bonjour").unwrap();
        assert_eq!(b.open(&p1).unwrap(), b"bonjour");
        let p2 = b.seal(b"salut").unwrap();
        assert_eq!(a.open(&p2).unwrap(), b"salut");
    }

    #[test]
    fn replay_rejected() {
        let (mut a, mut b) = linked_pair();
        let p = a.seal(b"x").unwrap();
        b.open(&p).unwrap();
        assert_eq!(b.open(&p).unwrap_err(), CryptoError::FrameReplay);
    }

    #[test]
    fn reordering_within_window_accepted() {
        let (mut a, mut b) = linked_pair();
        let p0 = a.seal(b"0").unwrap();
        let p1 = a.seal(b"1").unwrap();
        let p2 = a.seal(b"2").unwrap();
        b.open(&p2).unwrap();
        b.open(&p0).unwrap();
        b.open(&p1).unwrap();
        assert_eq!(b.open(&p1).unwrap_err(), CryptoError::FrameReplay);
    }

    #[test]
    fn tampered_ciphertext_rejected() {
        let (mut a, mut b) = linked_pair();
        let mut p = a.seal(b"secret").unwrap();
        p.ciphertext[0] ^= 1;
        assert_eq!(b.open(&p).unwrap_err(), CryptoError::DecryptFailed);
    }

    #[test]
    fn tampered_header_rejected_via_aad() {
        let (mut a, mut b) = linked_pair();
        let mut p = a.seal(b"secret").unwrap();
        p.counter = 5; // altération de l'en-tête ⇒ AAD et nonce changent
        assert!(b.open(&p).is_err());
    }

    #[test]
    fn wrong_direction_rejected() {
        // Un paquet réfléchi vers son émetteur ne se déchiffre pas.
        let (mut a, _) = linked_pair();
        let p = a.seal(b"boomerang").unwrap();
        assert!(a.open(&p).is_err());
    }

    #[test]
    fn rekey_epoch_transition() {
        let (mut a, mut b) = linked_pair();
        let before = a.seal(b"avant").unwrap();
        a.advance_send_epoch(0).unwrap();
        let after = a.seal(b"apres").unwrap();
        assert_eq!(after.epoch, 1);
        assert_eq!(after.counter, 0);
        // Le récepteur suit l'epoch n+1 et lit encore l'ancien.
        assert_eq!(b.open(&after).unwrap(), b"apres");
        assert_eq!(b.open(&before).unwrap(), b"avant");
        // Un saut d'epoch (n+3) est rejeté.
        a.advance_send_epoch(0).unwrap();
        a.advance_send_epoch(0).unwrap();
        let far = a.seal(b"loin").unwrap();
        assert!(matches!(
            b.open(&far).unwrap_err(),
            CryptoError::UnknownEpoch(3)
        ));
    }

    #[test]
    fn needs_rekey_on_age() {
        let keys = SessionKeys::new([1; 32], [2; 32]);
        let a = SessionCrypto::new(&keys, [9; 8], true, 0);
        assert!(!a.needs_rekey(1000));
        assert!(a.needs_rekey(REKEY_MAX_AGE_S * 1000 + 1));
    }

    #[test]
    fn old_window_replay_rejected() {
        let (mut a, mut b) = linked_pair();
        let first = a.seal(b"0").unwrap();
        b.open(&first).unwrap();
        // Avance le compteur au-delà de la fenêtre de 1024.
        let mut last = None;
        for _ in 0..1100 {
            last = Some(a.seal(b"x").unwrap());
        }
        b.open(&last.unwrap()).unwrap();
        assert_eq!(b.open(&first).unwrap_err(), CryptoError::FrameReplay);
    }
}
