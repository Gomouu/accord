//! Property tests (proptest) des codes amis (SPEC §2.4).
//!
//! Trois invariants sur des clés publiques et des saisies générées :
//! 1. **Round-trip** : `parse(display(code)) == code` — la forme canonique se
//!    relit sans perte, somme de contrôle comprise.
//! 2. **Tolérance de saisie** : casse, espaces et tirets superflus n'empêchent
//!    pas la relecture (détection de faute de frappe sans rigidité de forme).
//! 3. **Robustesse** : analyser une chaîne ARBITRAIRE ne panique jamais.

use accord_crypto::FriendCode;
use proptest::prelude::*;

proptest! {
    /// La forme canonique d'un code dérivé d'une clé se relit à l'identique.
    #[test]
    fn display_puis_parse_round_trip(pubkey in any::<[u8; 32]>()) {
        let code = FriendCode::of_pubkey(&pubkey);
        let parsed = FriendCode::parse(&code.display()).expect("code canonique relu");
        prop_assert_eq!(parsed, code);
        // La clé DHT de résolution est stable et déterministe.
        prop_assert_eq!(parsed.dht_key(), code.dht_key());
        prop_assert!(code.matches_pubkey(&pubkey));
    }

    /// La relecture tolère casse, espaces et séparateurs superflus.
    #[test]
    fn parse_tolere_casse_et_espaces(pubkey in any::<[u8; 32]>()) {
        let canonique = FriendCode::of_pubkey(&pubkey).display();
        let bruyant = format!("  {}  ", canonique.to_lowercase().replace('-', " - "));
        let relu = FriendCode::parse(&bruyant).expect("relecture tolérante");
        prop_assert_eq!(relu, FriendCode::of_pubkey(&pubkey));
    }

    /// Analyser une chaîne arbitraire ne panique jamais (Ok ou Err).
    #[test]
    fn parse_arbitraire_ne_panique_pas(s in ".{0,80}") {
        let _ = FriendCode::parse(&s);
    }

    /// Round-trip par payload : `from_payload(payload).payload() == payload`.
    #[test]
    fn payload_round_trip(payload in any::<[u8; 8]>()) {
        let code = FriendCode::from_payload(payload);
        prop_assert_eq!(code.payload(), &payload);
        // La forme canonique de ce code se relit aussi sans perte.
        let relu = FriendCode::parse(&code.display()).expect("relu");
        prop_assert_eq!(relu.payload(), &payload);
    }
}
