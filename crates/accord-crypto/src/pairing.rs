//! Appairage d'un nouvel appareil (multi-appareil, jalon 1, lot 1.D).
//!
//! Voir `docs/MULTI_DEVICE.md` §4. En une phrase : l'appareil déjà autorisé
//! affiche un **code court**, le nouvel appareil le saisit, les deux dérivent
//! un canal du code par un **PAKE**, et un humain compare une **empreinte** des
//! deux côtés avant que quoi que ce soit ne soit signé.
//!
//! 🔒 Pourquoi un PAKE et pas simplement le code. Avec un secret partagé en
//! clair, quiconque observe l'échange dérive le canal **hors ligne** et se fait
//! passer pour le nouvel appareil. Le PAKE fait payer chaque essai d'une
//! interaction **en ligne**, que la cadence limitée borne ensuite. C'est toute
//! la différence entre un code à six chiffres acceptable et un trou.
//!
//! 🔒 Pourquoi l'empreinte par-dessus. Le PAKE authentifie le canal auprès de
//! **qui connaît le code**. Si le code fuit — regardé par-dessus l'épaule,
//! copié dans une conversation — la confirmation d'empreinte exige encore
//! d'être *devant l'appareil autorisé*. Elle transforme une fuite de code en
//! tentative échouée. Elle sert aussi de confirmation de clé, que la forme de
//! base de SPAKE2 n'apporte pas (§4.1).

use crate::error::CryptoError;
use rand::rngs::OsRng;
use rand::RngCore;
use sha2::{Digest, Sha256};
use spake2::{Ed25519Group, Identity as PakeId, Password, Spake2};

/// Alphabet du code d'appairage : chiffres et majuscules, **sans les
/// caractères ambigus** `0/O` et `1/I/L`.
///
/// Le code se lit à voix haute ou se recopie d'un écran à l'autre ; une
/// confusion n'est pas une erreur bénigne ici, c'est un appairage qui échoue
/// sans que personne ne comprenne pourquoi.
const ALPHABET: &[u8] = b"23456789ABCDEFGHJKMNPQRSTUVWXYZ";

/// Longueur du code d'appairage, en caractères.
///
/// 8 caractères sur 31 symboles ≈ 39 bits. Beaucoup plus qu'un code à six
/// chiffres, parce que rien n'empêche de l'allonger : il n'est saisi qu'une
/// fois, et c'est le PAKE qui rend chaque essai coûteux, pas sa longueur.
pub const CODE_LEN: usize = 8;

/// Durée de validité d'un code, en millisecondes (5 minutes).
pub const CODE_TTL_MS: u64 = 5 * 60 * 1000;

/// Étiquette de session du PAKE.
///
/// 🔒 Sépare ce PAKE de tout autre usage du même code : un canal dérivé ici ne
/// peut pas être rejoué dans un autre contexte du protocole.
const PAKE_ID: &[u8] = b"accord-device-pairing-v1";

/// Préfixe de domaine de l'empreinte affichée aux deux utilisateurs.
const FINGERPRINT_DOMAIN: &[u8] = b"accord-pairing-fingerprint-v1";

/// Nombre de chiffres de l'empreinte comparée par un humain.
///
/// Six chiffres, en deux groupes de trois. Assez pour qu'une collision ne soit
/// pas trouvable dans les cinq minutes de vie du code ; assez court pour
/// qu'une personne les lise à une autre sans se tromper.
const FINGERPRINT_DIGITS: usize = 6;

/// Un code d'appairage, tel qu'il s'affiche et se saisit.
#[derive(Clone, PartialEq, Eq)]
pub struct PairingCode(String);

impl PairingCode {
    /// Tire un code neuf.
    pub fn generate() -> Self {
        Self::generate_with(&mut OsRng)
    }

    /// [`PairingCode::generate`] avec une source d'aléa explicite (tests).
    pub fn generate_with(rng: &mut impl RngCore) -> Self {
        // Rejet plutôt que modulo : 256 n'est pas multiple de 31, et prendre
        // le reste rendrait les premiers symboles de l'alphabet un peu plus
        // probables. L'écart est minime, le rejet est gratuit.
        let mut out = String::with_capacity(CODE_LEN);
        let limit = (256 / ALPHABET.len() * ALPHABET.len()) as u16;
        while out.len() < CODE_LEN {
            let mut b = [0u8; 1];
            rng.fill_bytes(&mut b);
            if u16::from(b[0]) >= limit {
                continue;
            }
            out.push(ALPHABET[usize::from(b[0]) % ALPHABET.len()] as char);
        }
        Self(out)
    }

    /// Analyse une saisie utilisateur : espaces et tirets ignorés, casse
    /// indifférente.
    ///
    /// Refuse tout caractère hors alphabet plutôt que de le corriger. « 0 »
    /// pour « O » serait une correction plausible — et le début d'un code
    /// qu'on croit avoir tapé alors qu'on en a tapé un autre.
    pub fn parse(input: &str) -> Result<Self, CryptoError> {
        let cleaned: String = input
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '-')
            .map(|c| c.to_ascii_uppercase())
            .collect();
        if cleaned.len() != CODE_LEN {
            return Err(CryptoError::Pairing("longueur de code"));
        }
        if !cleaned.bytes().all(|b| ALPHABET.contains(&b)) {
            return Err(CryptoError::Pairing("caractère hors alphabet"));
        }
        Ok(Self(cleaned))
    }

    /// Le code tel qu'il s'affiche.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Le code découpé pour l'affichage (`ABCD-EFGH`).
    pub fn display(&self) -> String {
        let (a, b) = self.0.split_at(CODE_LEN / 2);
        format!("{a}-{b}")
    }
}

/// 🔒 `Debug` muet : un code d'appairage dans une trace, c'est le code dans un
/// fichier de journal, donc lisible par qui lit les journaux.
impl std::fmt::Debug for PairingCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PairingCode(…)")
    }
}

/// Un côté de l'échange PAKE, en attente du message d'en face.
///
/// SPAKE2 est **symétrique** ici : les deux côtés connaissent le même code,
/// aucun n'est « le serveur ». C'est la forme qui convient à deux appareils
/// d'un même compte.
pub struct PairingHandshake {
    state: Spake2<Ed25519Group>,
}

/// Canal établi entre les deux appareils, et son empreinte à confirmer.
pub struct PairedChannel {
    key: [u8; 32],
    fingerprint: String,
}

impl PairingHandshake {
    /// Démarre l'échange à partir du code. Rend l'état et le message à envoyer.
    pub fn start(code: &PairingCode) -> (Self, Vec<u8>) {
        let (state, msg) = Spake2::<Ed25519Group>::start_symmetric(
            &Password::new(code.as_str().as_bytes()),
            &PakeId::new(PAKE_ID),
        );
        (Self { state }, msg)
    }

    /// Termine l'échange avec le message d'en face.
    ///
    /// Une erreur signifie que l'autre côté ne connaissait pas le même code —
    /// ou qu'il n'y a personne d'autre au bout. Dans les deux cas, l'appairage
    /// s'arrête ici.
    pub fn finish(self, peer_msg: &[u8]) -> Result<PairedChannel, CryptoError> {
        let shared = self
            .state
            .finish(peer_msg)
            .map_err(|_| CryptoError::Pairing("échange refusé"))?;
        let mut key = [0u8; 32];
        let digest: [u8; 32] = Sha256::digest(&shared).into();
        key.copy_from_slice(&digest);
        let fingerprint = fingerprint_of(&key);
        Ok(PairedChannel { key, fingerprint })
    }
}

impl PairedChannel {
    /// Clé du canal, pour chiffrer l'échange qui suit la confirmation.
    pub fn key(&self) -> &[u8; 32] {
        &self.key
    }

    /// Empreinte à afficher **des deux côtés**, à comparer par un humain.
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }
}

/// 🔒 `Debug` muet : la clé du canal ne doit pas fuir dans une trace.
impl std::fmt::Debug for PairedChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PairedChannel(…)")
    }
}

/// Empreinte lisible d'une clé de canal : six chiffres, en deux groupes.
fn fingerprint_of(key: &[u8; 32]) -> String {
    let mut d = Sha256::new();
    d.update(FINGERPRINT_DOMAIN);
    d.update(key);
    let h: [u8; 32] = d.finalize().into();
    let n = u32::from_be_bytes([h[0], h[1], h[2], h[3]]) % 10u32.pow(FINGERPRINT_DIGITS as u32);
    let s = format!("{n:0width$}", width = FINGERPRINT_DIGITS);
    format!("{} {}", &s[..3], &s[3..])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Joue l'échange complet entre deux côtés qui connaissent `code`.
    fn echange(a: &PairingCode, b: &PairingCode) -> (PairedChannel, PairedChannel) {
        let (ha, ma) = PairingHandshake::start(a);
        let (hb, mb) = PairingHandshake::start(b);
        (ha.finish(&mb).unwrap(), hb.finish(&ma).unwrap())
    }

    #[test]
    fn les_deux_cotes_derivent_la_meme_cle_et_la_meme_empreinte() {
        let code = PairingCode::generate();
        let (a, b) = echange(&code, &code);
        assert_eq!(a.key(), b.key());
        assert_eq!(a.fingerprint(), b.fingerprint());
    }

    #[test]
    fn un_code_different_ne_derive_pas_le_meme_canal() {
        // 🔒 Le cœur du PAKE. Celui qui a mal saisi — ou celui qui devine —
        // n'obtient pas le canal, il obtient un échec ou une clé étrangère.
        let vrai = PairingCode::parse("ABCDEFGH").unwrap();
        let faux = PairingCode::parse("ABCDEFGJ").unwrap();
        let (ha, ma) = PairingHandshake::start(&vrai);
        let (hb, mb) = PairingHandshake::start(&faux);

        // Un refus franc convient ; si l'échange aboutit malgré tout, les
        // deux clés doivent différer.
        if let (Ok(a), Ok(b)) = (ha.finish(&mb), hb.finish(&ma)) {
            assert_ne!(
                a.key(),
                b.key(),
                "deux codes différents ne doivent jamais donner la même clé"
            );
        }
    }

    #[test]
    fn un_message_intercepte_ailleurs_ne_donne_pas_le_canal() {
        // L'observateur qui rejoue un message capté sur un AUTRE appairage ne
        // connaît pas ce code-ci : il n'obtient pas le canal commun.
        let code = PairingCode::generate();
        let ailleurs = PairingCode::generate();
        let (ha, _) = PairingHandshake::start(&code);
        let (_, message_etranger) = PairingHandshake::start(&ailleurs);

        let (attendu, _) = echange(&code, &code);
        if let Ok(usurpe) = ha.finish(&message_etranger) {
            assert_ne!(
                usurpe.key(),
                attendu.key(),
                "un message d'un autre appairage ne doit pas ouvrir ce canal"
            );
        }
    }

    #[test]
    fn un_code_genere_a_la_bonne_forme() {
        for _ in 0..50 {
            let c = PairingCode::generate();
            assert_eq!(c.as_str().len(), CODE_LEN);
            assert!(c.as_str().bytes().all(|b| ALPHABET.contains(&b)));
            assert_eq!(c.display().len(), CODE_LEN + 1);
        }
    }

    #[test]
    fn lalphabet_exclut_les_caracteres_ambigus() {
        // 🔒 Un code se recopie d'un écran à l'autre, parfois à voix haute.
        // « 0 » pour « O » n'est pas une faute bénigne : c'est un appairage
        // qui échoue sans que personne ne sache pourquoi.
        for ambigu in *b"0O1IL" {
            assert!(
                !ALPHABET.contains(&ambigu),
                "{} ne doit pas être dans l'alphabet",
                ambigu as char
            );
        }
    }

    #[test]
    fn la_saisie_tolere_espaces_tirets_et_minuscules() {
        let attendu = PairingCode::parse("ABCDEFGH").unwrap();
        for saisie in ["abcdefgh", "ABCD-EFGH", " abcd efgh ", "a b c d e f g h"] {
            assert_eq!(PairingCode::parse(saisie).unwrap(), attendu);
        }
    }

    #[test]
    fn la_saisie_refuse_un_caractere_ambigu_au_lieu_de_le_corriger() {
        // Corriger « 0 » en « O » serait plausible — et le début d'un code
        // qu'on croit avoir tapé alors qu'on en a tapé un autre.
        for mauvais in ["ABCDEFG0", "ABCDEFGO", "ABCDEFG1", "ABCDEFG!"] {
            assert!(
                PairingCode::parse(mauvais).is_err(),
                "{mauvais} doit être refusé"
            );
        }
    }

    #[test]
    fn la_saisie_refuse_une_longueur_incorrecte() {
        for mauvais in ["", "ABCDEFG", "ABCDEFGHJ"] {
            assert!(PairingCode::parse(mauvais).is_err());
        }
    }

    #[test]
    fn lempreinte_a_six_chiffres_en_deux_groupes() {
        let code = PairingCode::generate();
        let (a, _) = echange(&code, &code);
        let f = a.fingerprint();
        assert_eq!(f.len(), FINGERPRINT_DIGITS + 1);
        assert_eq!(f.chars().filter(char::is_ascii_digit).count(), 6);
        assert_eq!(f.matches(' ').count(), 1);
    }

    #[test]
    fn deux_appairages_distincts_ont_des_empreintes_distinctes() {
        // Sinon la comparaison humaine ne prouverait rien : deux sessions
        // différentes afficheraient le même nombre.
        let mut vues = std::collections::HashSet::new();
        for _ in 0..20 {
            let code = PairingCode::generate();
            let (a, _) = echange(&code, &code);
            vues.insert(a.fingerprint().to_string());
        }
        assert!(vues.len() > 15, "empreintes trop peu variées : {vues:?}");
    }

    #[test]
    fn ni_le_code_ni_la_cle_ne_fuient_dans_une_trace() {
        // 🔒 Un `Debug` bavard met le secret dans les journaux, où il survit
        // au code lui-même.
        let code = PairingCode::generate();
        let trace = format!("{code:?}");
        assert!(!trace.contains(code.as_str()), "le code fuit : {trace}");

        let (a, _) = echange(&code, &code);
        let trace = format!("{a:?}");
        let cle_hex: String = a.key().iter().map(|b| format!("{b:02x}")).collect();
        assert!(!trace.contains(&cle_hex), "la clé fuit : {trace}");
    }
}
