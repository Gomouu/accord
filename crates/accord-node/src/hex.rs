//! Encodage hexadécimal des identifiants pour l'API JSON.

/// Encode des octets en hexadécimal minuscule.
pub fn encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// Décode une chaîne hexadécimale en taille fixe.
pub fn decode<const N: usize>(s: &str) -> Option<[u8; N]> {
    if s.len() != N * 2 || !s.is_ascii() {
        return None;
    }
    let mut out = [0u8; N];
    for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
        let hi = (chunk[0] as char).to_digit(16)?;
        let lo = (chunk[1] as char).to_digit(16)?;
        out[i] = ((hi << 4) | lo) as u8;
    }
    Some(out)
}

/// Décode une chaîne hexadécimale de longueur libre (corps de message
/// réimporté, dont la taille n'est pas connue à la compilation).
pub fn decode_vec(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 || !s.is_ascii() {
        return None;
    }
    let mut out = Vec::with_capacity(s.len() / 2);
    for chunk in s.as_bytes().chunks(2) {
        let hi = (chunk[0] as char).to_digit(16)?;
        let lo = (chunk[1] as char).to_digit(16)?;
        out.push(((hi << 4) | lo) as u8);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_rejects() {
        let bytes = [0x00, 0xff, 0x2a, 0x91];
        let s = encode(&bytes);
        assert_eq!(s, "00ff2a91");
        assert_eq!(decode::<4>(&s), Some(bytes));
        assert_eq!(decode::<4>("00ff2a9"), None);
        assert_eq!(decode::<4>("00ff2a9z"), None);
        assert_eq!(decode::<3>(&s), None);
    }
}
