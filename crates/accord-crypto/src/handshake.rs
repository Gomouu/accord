//! Handshake 1-RTT mutuellement authentifié (SPEC §2.2–§2.3).
//!
//! HELLO/WELCOME avec DH éphémère X25519, authentification par signatures
//! Ed25519 sur un transcript hash couvrant l'intégralité de l'échange, et
//! protections anti-rejeu (fenêtre ±90 s + cache de nonces).

use crate::error::CryptoError;
use crate::identity::{verify_pow, verify_signature, Identity};
use crate::session::SessionKeys;
use accord_proto::envelope::{Hello, Welcome};
use accord_proto::limits::HANDSHAKE_MAX_SKEW_MS;
use hkdf::Hkdf;
use rand::rngs::OsRng;
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::time::Duration;
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

const HS_DOMAIN: &[u8] = b"accord-hs-v1";

/// Résultat d'un handshake réussi, côté initiateur ou répondeur.
pub struct Established {
    /// Clés de session dérivées (i2r / r2i).
    pub keys: SessionKeys,
    /// Identifiant de session choisi par le répondeur.
    pub session_id: [u8; 8],
    /// Clé publique Ed25519 du pair.
    pub peer_static: [u8; 32],
    /// Nonce PoW du pair (déjà vérifié).
    pub peer_pow_nonce: u64,
    /// Vrai côté initiateur.
    pub is_initiator: bool,
}

impl std::fmt::Debug for Established {
    /// Debug sans matière secrète (les clés ne sont jamais affichées).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Established")
            .field("session_id", &self.session_id)
            .field("is_initiator", &self.is_initiator)
            .finish_non_exhaustive()
    }
}

fn transcript_1(h: &Hello) -> [u8; 32] {
    let mut d = Sha256::new();
    d.update(HS_DOMAIN);
    d.update([accord_proto::PROTOCOL_VERSION]);
    d.update(h.eph_pub);
    d.update(h.static_pub);
    d.update(h.pow_nonce.to_be_bytes());
    d.update(h.timestamp_ms.to_be_bytes());
    d.update(h.nonce);
    d.finalize().into()
}

fn transcript_2(t1: &[u8; 32], w: &Welcome) -> [u8; 32] {
    let mut d = Sha256::new();
    d.update(HS_DOMAIN);
    d.update(t1);
    d.update(w.eph_pub);
    d.update(w.static_pub);
    d.update(w.pow_nonce.to_be_bytes());
    d.update(w.timestamp_ms.to_be_bytes());
    d.update(w.nonce);
    d.update(w.session_id);
    d.finalize().into()
}

fn derive_keys(shared: &[u8; 32], t2: &[u8; 32]) -> SessionKeys {
    let hk = Hkdf::<Sha256>::new(Some(t2), shared);
    let mut k_i2r = Zeroizing::new([0u8; 32]);
    let mut k_r2i = Zeroizing::new([0u8; 32]);
    crate::hkdf_expand_fixe(&hk, b"accord-i2r", k_i2r.as_mut());
    crate::hkdf_expand_fixe(&hk, b"accord-r2i", k_r2i.as_mut());
    SessionKeys::new(*k_i2r, *k_r2i)
}

fn check_freshness(timestamp_ms: u64, now_ms: u64) -> Result<(), CryptoError> {
    if now_ms.abs_diff(timestamp_ms) > HANDSHAKE_MAX_SKEW_MS {
        return Err(CryptoError::ClockSkew);
    }
    Ok(())
}

/// Cache de nonces de handshake déjà vus (anti-rejeu, rétention 5 min).
#[derive(Default)]
pub struct NonceCache {
    seen: HashMap<[u8; 16], u64>,
}

impl NonceCache {
    /// Durée de rétention d'un nonce.
    pub const RETENTION: Duration = Duration::from_secs(300);

    /// Crée un cache vide.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enregistre `nonce` ; erreur s'il a déjà été vu dans la fenêtre.
    pub fn check_and_insert(&mut self, nonce: [u8; 16], now_ms: u64) -> Result<(), CryptoError> {
        let retention = Self::RETENTION.as_millis() as u64;
        self.seen
            .retain(|_, t| now_ms.saturating_sub(*t) < retention);
        if self.seen.insert(nonce, now_ms).is_some() {
            return Err(CryptoError::HandshakeReplay);
        }
        Ok(())
    }
}

/// État de l'initiateur entre l'envoi du HELLO et la réception du WELCOME.
pub struct Initiator {
    eph_secret: x25519_dalek::StaticSecret,
    t1: [u8; 32],
    hello: Hello,
    pow_bits: u32,
    /// Clé statique Ed25519 du pair attendu, si l'appelant vise une identité
    /// précise (liaison d'identité, SPEC §2.2). `None` pour un pair quelconque
    /// (ex. nœud DHT sans amitié établie) : aucune liaison n'est alors imposée.
    expected_static: Option<[u8; 32]>,
}

impl Initiator {
    /// Construit le HELLO et l'état d'attente. `cookie` vient d'un éventuel
    /// défi anti-DoS précédent (vide sinon). `expected_static` lie la future
    /// session à l'identité visée : le WELCOME devra en émaner (voir `finish`).
    pub fn start(
        identity: &Identity,
        now_ms: u64,
        cookie: Vec<u8>,
        pow_bits: u32,
        expected_static: Option<[u8; 32]>,
    ) -> Self {
        let eph_secret = x25519_dalek::StaticSecret::random_from_rng(OsRng);
        let eph_pub = x25519_dalek::PublicKey::from(&eph_secret).to_bytes();
        let mut nonce = [0u8; 16];
        OsRng.fill_bytes(&mut nonce);
        let mut hello = Hello {
            eph_pub,
            static_pub: identity.public_key(),
            pow_nonce: identity.pow_nonce(),
            timestamp_ms: now_ms,
            nonce,
            cookie,
            sig: [0; 64],
        };
        let t1 = transcript_1(&hello);
        hello.sig = identity.sign(&t1);
        Self {
            eph_secret,
            t1,
            hello,
            pow_bits,
            expected_static,
        }
    }

    /// HELLO à émettre (retransmissible tel quel).
    pub fn hello(&self) -> &Hello {
        &self.hello
    }

    /// Valide le WELCOME et dérive les clés de session.
    pub fn finish(self, w: &Welcome, now_ms: u64) -> Result<Established, CryptoError> {
        // Liaison d'identité (SPEC §2.2) : si un pair précis est visé, la clé
        // statique du WELCOME doit lui correspondre (comparaison temps
        // constant). Un MITM on-path qui forge un WELCOME frais et valide signé
        // de SA propre identité est ainsi rejeté avant toute dérivation.
        if let Some(expected) = self.expected_static {
            if !bool::from(w.static_pub.ct_eq(&expected)) {
                return Err(CryptoError::PeerIdentityMismatch);
            }
        }
        check_freshness(w.timestamp_ms, now_ms)?;
        if !verify_pow(&w.static_pub, w.pow_nonce, self.pow_bits) {
            return Err(CryptoError::InvalidPow);
        }
        let t2 = transcript_2(&self.t1, w);
        verify_signature(&w.static_pub, &t2, &w.sig)?;
        let peer_eph = x25519_dalek::PublicKey::from(w.eph_pub);
        let shared = self.eph_secret.diffie_hellman(&peer_eph);
        if !shared.was_contributory() {
            return Err(CryptoError::InvalidPublicKey);
        }
        let keys = derive_keys(shared.as_bytes(), &t2);
        Ok(Established {
            keys,
            session_id: w.session_id,
            peer_static: w.static_pub,
            peer_pow_nonce: w.pow_nonce,
            is_initiator: true,
        })
    }
}

/// Traite un HELLO côté répondeur : valide, répond WELCOME et établit la session.
///
/// `nonce_cache` est partagé entre tous les handshakes entrants du nœud.
pub fn respond(
    identity: &Identity,
    hello: &Hello,
    now_ms: u64,
    nonce_cache: &mut NonceCache,
    pow_bits: u32,
) -> Result<(Welcome, Established), CryptoError> {
    check_freshness(hello.timestamp_ms, now_ms)?;
    if !verify_pow(&hello.static_pub, hello.pow_nonce, pow_bits) {
        return Err(CryptoError::InvalidPow);
    }
    let t1 = transcript_1(hello);
    verify_signature(&hello.static_pub, &t1, &hello.sig)?;
    nonce_cache.check_and_insert(hello.nonce, now_ms)?;

    let eph_secret = x25519_dalek::StaticSecret::random_from_rng(OsRng);
    let eph_pub = x25519_dalek::PublicKey::from(&eph_secret).to_bytes();
    let mut nonce = [0u8; 16];
    OsRng.fill_bytes(&mut nonce);
    let mut session_id = [0u8; 8];
    OsRng.fill_bytes(&mut session_id);

    let mut welcome = Welcome {
        eph_pub,
        static_pub: identity.public_key(),
        pow_nonce: identity.pow_nonce(),
        timestamp_ms: now_ms,
        nonce,
        session_id,
        sig: [0; 64],
    };
    let t2 = transcript_2(&t1, &welcome);
    welcome.sig = identity.sign(&t2);

    let peer_eph = x25519_dalek::PublicKey::from(hello.eph_pub);
    let shared = eph_secret.diffie_hellman(&peer_eph);
    if !shared.was_contributory() {
        return Err(CryptoError::InvalidPublicKey);
    }
    let keys = derive_keys(shared.as_bytes(), &t2);
    Ok((
        welcome,
        Established {
            keys,
            session_id,
            peer_static: hello.static_pub,
            peer_pow_nonce: hello.pow_nonce,
            is_initiator: false,
        },
    ))
}

/// Secret rotatif pour les cookies anti-DoS sans état (SPEC §2.5).
pub struct CookieJar {
    secret: [u8; 32],
    rotated_at_ms: u64,
}

impl CookieJar {
    /// Période de rotation du secret (2 minutes) ; l'ancien cookie reste
    /// implicitement valable jusqu'à une période après rotation côté client.
    pub const ROTATION_MS: u64 = 120_000;

    /// Crée un pot à cookies avec un secret aléatoire.
    pub fn new(now_ms: u64) -> Self {
        let mut secret = [0u8; 32];
        OsRng.fill_bytes(&mut secret);
        Self {
            secret,
            rotated_at_ms: now_ms,
        }
    }

    // SÛRETÉ (D23) : `Hmac::new_from_slice` est infaillible — HMAC accepte
    // toute taille de clé par construction (RFC 2104). Allow ciblé plutôt
    // qu'un repli silencieux qui fabriquerait un cookie sous une clé nulle.
    #[allow(clippy::expect_used)]
    fn mac(&self, addr: &str, static_pub: &[u8; 32]) -> [u8; 16] {
        use hmac::{Hmac, Mac};
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&self.secret)
            .expect("HMAC accepte toute taille de clé");
        mac.update(addr.as_bytes());
        mac.update(static_pub);
        let out = mac.finalize().into_bytes();
        let mut cookie = [0u8; 16];
        cookie.copy_from_slice(&out[..16]);
        cookie
    }

    /// Fabrique le cookie pour une source donnée, en rotant le secret si dû.
    pub fn issue(&mut self, addr: &str, static_pub: &[u8; 32], now_ms: u64) -> [u8; 16] {
        if now_ms.saturating_sub(self.rotated_at_ms) >= Self::ROTATION_MS {
            OsRng.fill_bytes(&mut self.secret);
            self.rotated_at_ms = now_ms;
        }
        self.mac(addr, static_pub)
    }

    /// Vérifie un cookie présenté dans un HELLO.
    pub fn verify(&self, addr: &str, static_pub: &[u8; 32], cookie: &[u8]) -> bool {
        if cookie.len() != 16 {
            return false;
        }
        let expected = self.mac(addr, static_pub);
        expected.ct_eq(cookie).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const POW: u32 = 4;

    fn pair() -> (Identity, Identity) {
        (
            Identity::generate_with_pow_bits(POW),
            Identity::generate_with_pow_bits(POW),
        )
    }

    #[test]
    fn full_handshake_derives_same_keys() {
        let (alice, bob) = pair();
        let now = 1_000_000;
        let init = Initiator::start(&alice, now, vec![], POW, None);
        let mut cache = NonceCache::new();
        let (welcome, est_b) = respond(&bob, init.hello(), now + 20, &mut cache, POW).unwrap();
        let est_a = init.finish(&welcome, now + 40).unwrap();
        assert_eq!(est_a.session_id, est_b.session_id);
        assert_eq!(est_a.peer_static, bob.public_key());
        assert_eq!(est_b.peer_static, alice.public_key());
        assert!(est_a.keys.same_keys(&est_b.keys));
        assert!(est_a.is_initiator && !est_b.is_initiator);
    }

    #[test]
    fn expected_identity_binding_accepts_real_peer() {
        // Cas nominal : Alice vise Bob et c'est bien Bob qui répond.
        let (alice, bob) = pair();
        let now = 1_000_000;
        let init = Initiator::start(&alice, now, vec![], POW, Some(bob.public_key()));
        let mut cache = NonceCache::new();
        let (welcome, est_b) = respond(&bob, init.hello(), now + 20, &mut cache, POW).unwrap();
        let est_a = init.finish(&welcome, now + 40).unwrap();
        assert_eq!(est_a.peer_static, bob.public_key());
        assert!(est_a.keys.same_keys(&est_b.keys));
    }

    #[test]
    fn welcome_from_wrong_identity_rejected() {
        // MITM on-path : Alice vise Bob mais Mallory répond avec un WELCOME
        // frais, valide et signé de SA propre identité. La liaison doit le
        // rejeter avant toute dérivation de session.
        let (alice, bob) = pair();
        let mallory = Identity::generate_with_pow_bits(POW);
        let now = 1_000_000;
        let init = Initiator::start(&alice, now, vec![], POW, Some(bob.public_key()));
        let mut cache = NonceCache::new();
        let (welcome, _) = respond(&mallory, init.hello(), now + 20, &mut cache, POW).unwrap();
        assert_ne!(mallory.public_key(), bob.public_key());
        assert_eq!(
            init.finish(&welcome, now + 40).unwrap_err(),
            CryptoError::PeerIdentityMismatch
        );
    }

    #[test]
    fn replayed_hello_rejected() {
        let (alice, bob) = pair();
        let now = 1_000_000;
        let init = Initiator::start(&alice, now, vec![], POW, None);
        let mut cache = NonceCache::new();
        respond(&bob, init.hello(), now, &mut cache, POW).unwrap();
        assert_eq!(
            respond(&bob, init.hello(), now + 10, &mut cache, POW).unwrap_err(),
            CryptoError::HandshakeReplay
        );
    }

    #[test]
    fn stale_timestamp_rejected() {
        let (alice, bob) = pair();
        let init = Initiator::start(&alice, 1_000_000, vec![], POW, None);
        let mut cache = NonceCache::new();
        assert_eq!(
            respond(&bob, init.hello(), 1_000_000 + 91_000, &mut cache, POW).unwrap_err(),
            CryptoError::ClockSkew
        );
    }

    #[test]
    fn tampered_hello_rejected() {
        let (alice, bob) = pair();
        let now = 1_000_000;
        let init = Initiator::start(&alice, now, vec![], POW, None);
        let mut hello = init.hello().clone();
        hello.eph_pub[0] ^= 1; // MITM remplace la clé éphémère
        let mut cache = NonceCache::new();
        assert_eq!(
            respond(&bob, &hello, now, &mut cache, POW).unwrap_err(),
            CryptoError::InvalidSignature
        );
    }

    #[test]
    fn tampered_welcome_rejected() {
        let (alice, bob) = pair();
        let now = 1_000_000;
        let init = Initiator::start(&alice, now, vec![], POW, None);
        let mut cache = NonceCache::new();
        let (mut welcome, _) = respond(&bob, init.hello(), now, &mut cache, POW).unwrap();
        welcome.eph_pub[0] ^= 1;
        assert_eq!(
            init.finish(&welcome, now).unwrap_err(),
            CryptoError::InvalidSignature
        );
    }

    #[test]
    fn insufficient_pow_rejected() {
        let (alice, bob) = pair();
        let now = 1_000_000;
        let init = Initiator::start(&alice, now, vec![], POW, None);
        let mut cache = NonceCache::new();
        // Le répondeur exige 30 bits : l'identité 4 bits d'alice échoue
        // (probabilité de passage accidentel ~ 2^-26).
        assert_eq!(
            respond(&bob, init.hello(), now, &mut cache, 30).unwrap_err(),
            CryptoError::InvalidPow
        );
    }

    #[test]
    fn cookie_roundtrip_and_rotation() {
        let mut jar = CookieJar::new(0);
        let pk = [7u8; 32];
        let c = jar.issue("1.2.3.4:5", &pk, 0);
        assert!(jar.verify("1.2.3.4:5", &pk, &c));
        assert!(!jar.verify("1.2.3.4:6", &pk, &c));
        assert!(!jar.verify("1.2.3.4:5", &[8u8; 32], &c));
        // Après rotation, l'ancien cookie ne vérifie plus.
        let _ = jar.issue("1.2.3.4:5", &pk, CookieJar::ROTATION_MS + 1);
        assert!(!jar.verify("1.2.3.4:5", &pk, &c));
    }
}
