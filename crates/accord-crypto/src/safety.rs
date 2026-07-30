//! Safety numbers (Lot E1): human-comparable fingerprint of an identity
//! pair, Signal-style. Two friends compare the displayed number out of band
//! (in person, over a call): if it matches, no intermediary substituted a
//! key (anti-MITM). Purely local derivation: no network byte, no new wire
//! format.
//!
//! Construction: each Ed25519 public key is reduced to a fingerprint by
//! `FINGERPRINT_ITERATIONS` rounds of SHA-512 (`version ‖ pubkey`, then
//! `hash ‖ pubkey`), keeping 30 bytes → 6 groups of 5 digits. The two
//! fingerprints, ordered lexicographically by public key, form 60 digits
//! (12 groups of 5): `safety_number(a, b) == safety_number(b, a)` by
//! construction. An emoji rendering (8 symbols from a fixed 256-entry
//! table) allows a quick glance comparison.

use sha2::{Digest, Sha512};

/// SHA-512 rounds per fingerprint: makes vanity-key searches (forging a key
/// whose fingerprint mimics a victim's) expensive — same order of magnitude
/// as Signal's ~5200 rounds.
const FINGERPRINT_ITERATIONS: u32 = 5200;

/// Fingerprint algorithm version, mixed into the first round: a future
/// construction change yields disjoint numbers instead of silent collisions.
const FINGERPRINT_VERSION: [u8; 2] = [0, 0];

/// Fingerprint bytes kept per key (30 bytes → 6 groups of 5 digits).
const FINGERPRINT_BYTES: usize = 30;

/// Number of emoji in the quick rendering.
const EMOJI_COUNT: usize = 8;

/// Display-ready safety number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SafetyNumber {
    /// 60 ASCII digits, no separator (the UI groups them by 5).
    pub digits: String,
    /// Compact emoji rendering: `EMOJI_COUNT` symbols from the fixed table.
    pub emoji: Vec<&'static str>,
}

/// Iterated fingerprint of one public key (64 SHA-512 bytes).
fn fingerprint(pubkey: &[u8; 32]) -> [u8; 64] {
    let mut hash = [0u8; 64];
    let mut first = Sha512::new();
    first.update(FINGERPRINT_VERSION);
    first.update(pubkey);
    hash.copy_from_slice(&first.finalize());
    for _ in 1..FINGERPRINT_ITERATIONS {
        let mut round = Sha512::new();
        round.update(hash);
        round.update(pubkey);
        hash.copy_from_slice(&round.finalize());
    }
    hash
}

/// 30 decimal digits from the first 30 bytes of a fingerprint (6 slices of
/// 5 bytes, each reduced modulo 100 000).
fn digits_of(hash: &[u8; 64]) -> String {
    let mut out = String::with_capacity(FINGERPRINT_BYTES);
    for chunk in hash[..FINGERPRINT_BYTES].chunks_exact(5) {
        let n = chunk.iter().fold(0u64, |acc, &b| (acc << 8) | u64::from(b));
        out.push_str(&format!("{:05}", n % 100_000));
    }
    out
}

/// Safety number of the conversation between `mine` and `theirs`.
/// Symmetric: both keys are ordered lexicographically before derivation, so
/// both peers display exactly the same number.
pub fn safety_number(mine: &[u8; 32], theirs: &[u8; 32]) -> SafetyNumber {
    let (lo, hi) = if mine <= theirs {
        (mine, theirs)
    } else {
        (theirs, mine)
    };
    let fp_lo = fingerprint(lo);
    let fp_hi = fingerprint(hi);
    let digits = format!("{}{}", digits_of(&fp_lo), digits_of(&fp_hi));
    let mut sel = Sha512::new();
    sel.update(b"accord-safety-emoji");
    sel.update(fp_lo);
    sel.update(fp_hi);
    let emoji = sel
        .finalize()
        .iter()
        .take(EMOJI_COUNT)
        .map(|&b| EMOJI_TABLE[usize::from(b)])
        .collect();
    SafetyNumber { digits, emoji }
}

/// Fixed table of 256 distinct, visually discriminable emoji (one per byte
/// value). NEVER reorder or replace an entry: any change would invalidate
/// renderings users have already memorised.
static EMOJI_TABLE: [&str; 256] = [
    "🐶", "🐱", "🐭", "🐹", "🐰", "🦊", "🐻", "🐼", "🐨", "🐯", "🦁", "🐮", "🐷", "🐸", "🐵", "🐔",
    "🐧", "🐦", "🐤", "🦆", "🦅", "🦉", "🦇", "🐺", "🐗", "🐴", "🦄", "🐝", "🐛", "🦋", "🐌", "🐞",
    "🐜", "🦟", "🦗", "🕷", "🦂", "🐢", "🐍", "🦎", "🦖", "🦕", "🐙", "🦑", "🦐", "🦞", "🦀", "🐡",
    "🐠", "🐟", "🐬", "🐳", "🐋", "🦈", "🐊", "🐅", "🐆", "🦓", "🦍", "🦧", "🐘", "🦛", "🦏", "🐪",
    "🐫", "🦒", "🦘", "🐃", "🐂", "🐄", "🐎", "🐖", "🐏", "🐑", "🦙", "🐐", "🦌", "🐕", "🐩", "🦮",
    "🐈", "🐓", "🦃", "🦚", "🦜", "🦢", "🦩", "🕊", "🐇", "🦝", "🦨", "🦡", "🦦", "🦥", "🐁", "🐀",
    "🦔", "🌵", "🎄", "🌲", "🌳", "🌴", "🌱", "🌿", "☘️", "🍀", "🎍", "🎋", "🍃", "🍂", "🍁", "🍄",
    "🌾", "💐", "🌷", "🌹", "🥀", "🌺", "🌸", "🌼", "🌻", "🌞", "🌝", "🌛", "🌜", "🌚", "🌕", "🌙",
    "⭐", "🌟", "💫", "✨", "☄️", "🪐", "🌍", "🌈", "☀️", "⛅", "☁️", "⛈", "🌧", "❄️", "⛄", "🌊",
    "💧", "🔥", "🌪", "🍇", "🍈", "🍉", "🍊", "🍋", "🍌", "🍍", "🥭", "🍎", "🍏", "🍐", "🍑", "🍒",
    "🍓", "🥝", "🍅", "🥥", "🥑", "🍆", "🥔", "🥕", "🌽", "🌶", "🥒", "🥬", "🥦", "🧄", "🧅", "🥜",
    "🌰", "🍞", "🥐", "🥖", "🥨", "🥞", "🧇", "🧀", "🍗", "🥩", "🍔", "🍟", "🍕", "🌭", "🥪", "🌮",
    "🌯", "🥗", "🥘", "🍝", "🍜", "🍲", "🍛", "🍣", "🍱", "🥟", "🍤", "🍙", "🍚", "🍘", "🍥", "🥮",
    "🍢", "🍡", "🍧", "🍨", "🍦", "🥧", "🧁", "🍰", "🎂", "🍮", "🍭", "🍬", "🍫", "🍿", "🍩", "🍪",
    "⚽", "🏀", "🏈", "⚾", "🥎", "🎾", "🏐", "🏉", "🥏", "🎱", "🪀", "🏓", "🏸", "🥅", "⛳", "🪁",
    "🏹", "🎣", "🥊", "🛼", "🛹", "⛸", "🎿", "🎯", "🎲", "🧩", "🎮", "🎻", "🎹", "🥁", "🎷", "🎺",
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    const A: [u8; 32] = [1u8; 32];
    const B: [u8; 32] = [2u8; 32];

    #[test]
    fn deterministic_and_symmetric() {
        let ab = safety_number(&A, &B);
        assert_eq!(ab, safety_number(&A, &B));
        assert_eq!(ab, safety_number(&B, &A));
    }

    #[test]
    fn shape_is_60_ascii_digits_and_8_emoji() {
        let sn = safety_number(&A, &B);
        assert_eq!(sn.digits.len(), 60);
        assert!(sn.digits.bytes().all(|b| b.is_ascii_digit()));
        assert_eq!(sn.emoji.len(), 8);
    }

    #[test]
    fn any_key_change_changes_the_number() {
        let base = safety_number(&A, &B);
        let mut b2 = B;
        b2[31] ^= 1;
        assert_ne!(base.digits, safety_number(&A, &b2).digits);
        let mut a2 = A;
        a2[0] ^= 1;
        assert_ne!(base.digits, safety_number(&a2, &B).digits);
    }

    #[test]
    fn emoji_table_has_256_unique_entries() {
        let unique: HashSet<&&str> = EMOJI_TABLE.iter().collect();
        assert_eq!(unique.len(), 256);
        assert!(EMOJI_TABLE.iter().all(|e| !e.is_empty()));
    }

    #[test]
    fn symmetry_holds_at_the_key_space_edges() {
        let sn = safety_number(&[0u8; 32], &[255u8; 32]);
        assert_eq!(sn.digits, safety_number(&[255u8; 32], &[0u8; 32]).digits);
        assert_eq!(sn.digits.len(), 60);
    }
}
