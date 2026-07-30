//! Token bucket par IP source pour l'anti-abus (SPEC §4, §2.5).

use std::collections::HashMap;
use std::net::IpAddr;

/// Seau à jetons : capacité (rafale) + débit de recharge par seconde.
#[derive(Debug, Clone, Copy)]
pub struct Bucket {
    tokens: f64,
    capacity: f64,
    refill_per_s: f64,
    last_ms: u64,
}

impl Bucket {
    /// Crée un seau plein.
    pub fn new(capacity: f64, refill_per_s: f64, now_ms: u64) -> Self {
        Self {
            tokens: capacity,
            capacity,
            refill_per_s,
            last_ms: now_ms,
        }
    }

    /// Tente de consommer un jeton ; recharge selon le temps écoulé.
    pub fn try_take(&mut self, now_ms: u64) -> bool {
        self.try_take_n(1.0, now_ms)
    }

    /// Tente de consommer `n` jetons (RPC coûteux plus chers).
    pub fn try_take_n(&mut self, n: f64, now_ms: u64) -> bool {
        let elapsed = now_ms.saturating_sub(self.last_ms) as f64 / 1000.0;
        self.tokens = (self.tokens + elapsed * self.refill_per_s).min(self.capacity);
        self.last_ms = now_ms;
        if self.tokens >= n {
            self.tokens -= n;
            true
        } else {
            false
        }
    }
}

/// Intervalle de purge périodique des seaux (ms).
const GC_INTERVAL_MS: u64 = 60_000;

/// Inactivité au-delà de laquelle un seau est oublié (ms).
const BUCKET_IDLE_MS: u64 = 300_000;

/// Nombre maximal de seaux suivis simultanément.
///
/// 🔒 Le limiteur est indexé par IP SOURCE, et sur UDP la source est
/// falsifiable : chaque adresse inédite créait une entrée, et la purge
/// périodique ne passait qu'une fois par minute. Un flot de datagrammes à
/// sources tirées au hasard faisait donc croître la table de plusieurs
/// centaines de milliers d'entrées par minute — le limiteur censé borner
/// l'abus en devenait le vecteur.
///
/// Au plafond, on tente d'abord une purge immédiate (les seaux inactifs sont
/// alors très majoritaires) ; si elle ne libère rien, la requête est REFUSÉE.
/// C'est le choix conservateur : sous inondation, refuser un pair inconnu de
/// plus coûte une retransmission de handshake, tandis que l'accepter coûte de
/// la mémoire sans borne. Les pairs déjà suivis conservent leur seau et ne
/// sont jamais affectés.
const MAX_BUCKETS: usize = 8_192;

/// Ensemble de seaux indexés par IP source, avec purge des inactifs.
pub struct RateLimiter {
    buckets: HashMap<IpAddr, Bucket>,
    capacity: f64,
    refill_per_s: f64,
    last_gc_ms: u64,
}

impl RateLimiter {
    /// Crée un limiteur avec capacité de rafale et débit de recharge donnés.
    pub fn new(capacity: f64, refill_per_s: f64) -> Self {
        Self {
            buckets: HashMap::new(),
            capacity,
            refill_per_s,
            last_gc_ms: 0,
        }
    }

    /// Autorise (ou non) une action de coût `cost` pour l'IP `ip`.
    pub fn check(&mut self, ip: IpAddr, cost: f64, now_ms: u64) -> bool {
        // Purge périodique des seaux pleins et inactifs > 5 min.
        if now_ms.saturating_sub(self.last_gc_ms) > GC_INTERVAL_MS {
            self.gc(now_ms);
        }
        let cap = self.capacity;
        let refill = self.refill_per_s;
        if let Some(bucket) = self.buckets.get_mut(&ip) {
            return bucket.try_take_n(cost, now_ms);
        }
        // Nouvelle IP : borne le nombre de seaux suivis (voir [`MAX_BUCKETS`]).
        // La purge hors calendrier ne se déclenche qu'au plafond : elle n'a
        // aucun coût dans le régime nominal.
        if self.buckets.len() >= MAX_BUCKETS {
            self.gc(now_ms);
            if self.buckets.len() >= MAX_BUCKETS {
                return false;
            }
        }
        self.buckets
            .entry(ip)
            .or_insert_with(|| Bucket::new(cap, refill, now_ms))
            .try_take_n(cost, now_ms)
    }

    /// Oublie les seaux inactifs depuis plus de [`BUCKET_IDLE_MS`].
    fn gc(&mut self, now_ms: u64) {
        self.buckets
            .retain(|_, b| now_ms.saturating_sub(b.last_ms) < BUCKET_IDLE_MS);
        self.last_gc_ms = now_ms;
    }

    /// Nombre de seaux suivis (observabilité).
    pub fn tracked(&self) -> usize {
        self.buckets.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn ip(n: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(10, 0, 0, n))
    }

    #[test]
    fn burst_then_throttle() {
        let mut rl = RateLimiter::new(4.0, 10.0);
        // 4 en rafale immédiate.
        for _ in 0..4 {
            assert!(rl.check(ip(1), 1.0, 0));
        }
        assert!(!rl.check(ip(1), 1.0, 0));
        // Après 100 ms, 1 jeton rechargé (10/s).
        assert!(rl.check(ip(1), 1.0, 100));
        assert!(!rl.check(ip(1), 1.0, 100));
    }

    #[test]
    fn per_ip_isolation() {
        let mut rl = RateLimiter::new(2.0, 1.0);
        assert!(rl.check(ip(1), 2.0, 0));
        assert!(!rl.check(ip(1), 1.0, 0));
        // Une autre IP a son propre seau.
        assert!(rl.check(ip(2), 2.0, 0));
    }

    #[test]
    fn expensive_rpc_costs_more() {
        let mut rl = RateLimiter::new(8.0, 1.0);
        // Un STORE coûte 4.
        assert!(rl.check(ip(1), 4.0, 0));
        assert!(rl.check(ip(1), 4.0, 0));
        assert!(!rl.check(ip(1), 4.0, 0));
    }

    /// 🔒 Sur UDP l'IP source est falsifiable : sans plafond, chaque adresse
    /// inédite créait une entrée et la table enflait sans borne entre deux
    /// purges. Le plafond tient, et un pair DÉJÀ suivi n'est jamais évincé au
    /// profit d'un inconnu.
    #[test]
    fn table_bornee_face_a_des_sources_usurpees() {
        let mut rl = RateLimiter::new(4.0, 1.0);
        let habitue = IpAddr::V4(Ipv4Addr::new(198, 51, 100, 1));
        assert!(rl.check(habitue, 1.0, 0));

        // Inondation depuis un espace d'adresses bien plus grand que le plafond.
        for n in 0..(MAX_BUCKETS as u32 * 2) {
            let octets = n.to_be_bytes();
            rl.check(
                IpAddr::V4(Ipv4Addr::new(10, octets[1], octets[2], octets[3])),
                1.0,
                0,
            );
        }
        assert!(
            rl.tracked() <= MAX_BUCKETS,
            "table non bornée : {} seaux",
            rl.tracked()
        );
        // Le pair légitime a gardé son seau (et donc ses jetons restants).
        assert!(rl.check(habitue, 3.0, 0));
        assert!(!rl.check(habitue, 1.0, 0));
    }

    #[test]
    fn les_seaux_inactifs_sont_oublies() {
        let mut rl = RateLimiter::new(4.0, 1.0);
        for n in 0..50u8 {
            rl.check(ip(n), 1.0, 0);
        }
        assert_eq!(rl.tracked(), 50);
        // Bien après la fenêtre d'inactivité, la purge périodique nettoie tout
        // sauf le seau qui vient d'être touché.
        assert!(rl.check(ip(200), 1.0, GC_INTERVAL_MS + BUCKET_IDLE_MS + 1));
        assert_eq!(rl.tracked(), 1);
    }

    #[test]
    fn refill_caps_at_capacity() {
        let mut rl = RateLimiter::new(4.0, 10.0);
        assert!(rl.check(ip(1), 4.0, 0));
        // Longue attente : le seau ne dépasse pas la capacité.
        assert!(rl.check(ip(1), 4.0, 10_000));
        assert!(!rl.check(ip(1), 1.0, 10_000));
    }
}
