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
//!
//! 🔒 **La racine du compte voyage** ([`AccountSeed`]), et c'est une décision,
//! pas un oubli. Un appareil appairé qui garderait sa propre racine figurerait
//! dans la liste du compte sans pouvoir en signer une seule version : ni
//! inscrire l'appareil suivant, ni publier un profil, ni signer une op de
//! groupe au nom du compte. Le produit livre déjà une phrase de récupération de
//! douze mots qui pose la racine sur n'importe quelle machine qui la saisit :
//! refuser de la faire passer par un canal PAKE dont un humain a comparé
//! l'empreinte ne protégerait rien qui ne soit pas déjà plus facile à obtenir.
//! Elle ne part **jamais** en clair, jamais avant la confirmation, et jamais
//! dans l'autre sens.

use crate::error::CryptoError;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use rand::rngs::OsRng;
use rand::RngCore;
use sha2::{Digest, Sha256};
use spake2::{Ed25519Group, Identity as PakeId, Password, Spake2};
use zeroize::{Zeroize, ZeroizeOnDrop};

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

/// 🔒 La clé du canal est effacée à la libération. C'est elle qui protège la
/// racine du compte pendant l'appairage ([`PairedChannel::seal_account_seed`]) :
/// la laisser traîner dans une pile réutilisée serait laisser traîner de quoi
/// ouvrir la charge la plus sensible du protocole. Même geste que
/// [`AccountSeed`] et que les clés de session.
impl Drop for PairedChannel {
    fn drop(&mut self) {
        self.key.zeroize();
    }
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

/// Taille du nonce XChaCha20-Poly1305, en octets.
const NONCE_LEN: usize = 24;

impl PairedChannel {
    /// Clé du canal, pour chiffrer l'échange qui suit la confirmation.
    pub fn key(&self) -> &[u8; 32] {
        &self.key
    }

    /// Chiffre une charge pour l'autre appareil : `nonce ‖ chiffré`.
    ///
    /// 🔒 Nonce **aléatoire**, et c'est sûr ici précisément parce que le canal
    /// est jetable : il porte deux ou trois messages avant de disparaître, là
    /// où un compteur serait indispensable sur un canal durable. Le nonce
    /// XChaCha fait 192 bits — la probabilité d'en retirer deux fois le même
    /// sur si peu de messages est hors de portée.
    pub fn seal(&self, plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let mut nonce = [0u8; NONCE_LEN];
        OsRng.fill_bytes(&mut nonce);
        let cipher = XChaCha20Poly1305::new((&self.key).into());
        let mut out = nonce.to_vec();
        out.extend(
            cipher
                .encrypt(XNonce::from_slice(&nonce), plaintext)
                .map_err(|_| CryptoError::Pairing("chiffrement du canal"))?,
        );
        Ok(out)
    }

    /// Déchiffre une charge reçue sur le canal.
    ///
    /// 🔒 Un échec signifie que l'émetteur n'a pas la clé du canal — donc
    /// qu'il ne connaissait pas le code. C'est le seul endroit du protocole
    /// d'appairage où un échec cryptographique prouve quelque chose : le PAKE
    /// lui-même, en forme symétrique, ne dit rien (voir `PairingOffer`).
    pub fn open(&self, sealed: &[u8]) -> Result<Vec<u8>, CryptoError> {
        if sealed.len() <= NONCE_LEN {
            return Err(CryptoError::Pairing("charge du canal tronquée"));
        }
        let (nonce, body) = sealed.split_at(NONCE_LEN);
        let cipher = XChaCha20Poly1305::new((&self.key).into());
        cipher
            .decrypt(XNonce::from_slice(nonce), body)
            .map_err(|_| CryptoError::Pairing("déchiffrement du canal"))
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

/// Étiquette du contenu d'une charge scellée portant la racine du compte.
///
/// 🔒 La même clé de canal scelle deux choses de sens opposé : l'entrée
/// d'appareil que le nouvel appareil propose, et la racine que l'appareil
/// autorisé lui renvoie. Sans marqueur de contenu, une charge réfléchie
/// pourrait être relue comme l'autre — le genre de confusion qui ne se voit
/// pas à la lecture et qui ne pardonne pas ici.
const SEED_TAG: u8 = 0x01;

/// Taille exacte de la charge en clair d'une racine scellée : étiquette + 32.
const SEED_PLAINTEXT_LEN: usize = 1 + 32;

/// La racine d'un compte, en transit vers un appareil qui l'adopte.
///
/// 🔒 Ce type existe pour une seule raison : rendre difficile de manipuler
/// trente-deux octets qui *sont le compte* comme s'ils étaient des données.
/// Effacé à la libération, `Debug` muet, et aucune conversion implicite vers
/// un tableau nu — [`AccountSeed::expose`] se voit à la relecture.
#[derive(Zeroize, ZeroizeOnDrop, Clone)]
pub struct AccountSeed([u8; 32]);

impl AccountSeed {
    /// Enveloppe une graine de compte.
    pub fn new(seed: [u8; 32]) -> Self {
        Self(seed)
    }

    /// Les octets de la graine. Le nom est délibérément désagréable : chaque
    /// appel est un endroit où la racine du compte sort de son enveloppe.
    pub fn expose(&self) -> &[u8; 32] {
        &self.0
    }
}

/// 🔒 `Debug` muet : la racine du compte dans une trace, c'est le compte dans
/// un fichier de journal — et un journal survit au processus qui l'a écrit.
impl std::fmt::Debug for AccountSeed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AccountSeed(…)")
    }
}

impl PairedChannel {
    /// Scelle la racine du compte pour l'appareil d'en face.
    ///
    /// 🔒 À n'appeler qu'après la confirmation d'empreinte du côté **déjà
    /// autorisé**, et jamais dans l'autre sens. Un échange SPAKE2 abouti ne
    /// prouve rien (§4.2) ; ce qui autorise cet envoi est un humain devant la
    /// machine qui détient déjà le compte.
    pub fn seal_account_seed(&self, seed: &AccountSeed) -> Result<Vec<u8>, CryptoError> {
        let mut clear = Vec::with_capacity(SEED_PLAINTEXT_LEN);
        clear.push(SEED_TAG);
        clear.extend_from_slice(seed.expose());
        let sealed = self.seal(&clear);
        clear.zeroize();
        sealed
    }

    /// Ouvre une racine de compte reçue sur le canal.
    ///
    /// 🔒 Trois refus, et aucun n'est décoratif : une charge qui ne s'ouvre pas
    /// vient de quelqu'un qui n'avait pas le code ; une étiquette qui n'est pas
    /// celle d'une racine est une charge d'un autre sens rejouée dans celui-ci ;
    /// une longueur qui n'est pas exactement la bonne n'est pas une graine.
    pub fn open_account_seed(&self, sealed: &[u8]) -> Result<AccountSeed, CryptoError> {
        let mut clear = self.open(sealed)?;
        let issue = match clear.split_first() {
            Some((&SEED_TAG, rest)) if rest.len() == 32 => {
                let mut seed = [0u8; 32];
                seed.copy_from_slice(rest);
                Ok(AccountSeed::new(seed))
            }
            _ => Err(CryptoError::Pairing("charge de racine mal formée")),
        };
        clear.zeroize();
        issue
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

    #[test]
    fn le_canal_chiffre_et_dechiffre_entre_les_deux_cotes() {
        let code = PairingCode::generate();
        let (a, b) = echange(&code, &code);

        let scelle = a.seal(b"clef d'appareil").unwrap();
        assert_eq!(b.open(&scelle).unwrap(), b"clef d'appareil");
    }

    #[test]
    fn un_canal_etranger_ne_peut_pas_ouvrir_la_charge() {
        // 🔒 C'est le SEUL endroit de l'appairage où un échec cryptographique
        // prouve quelque chose : celui qui ne peut pas ouvrir n'avait pas le
        // code. Le PAKE symétrique, lui, ne dit rien (voir `PairingOffer`).
        let (a, _) = echange(&PairingCode::generate(), &PairingCode::generate());
        let (etranger, _) = echange(&PairingCode::generate(), &PairingCode::generate());

        let scelle = a.seal(b"clef d'appareil").unwrap();
        assert!(etranger.open(&scelle).is_err());
    }

    #[test]
    fn une_charge_alteree_est_refusee() {
        let code = PairingCode::generate();
        let (a, b) = echange(&code, &code);
        let mut scelle = a.seal(b"clef d'appareil").unwrap();

        // Un octet retourné dans le chiffré, puis dans le nonce.
        let dernier = scelle.len() - 1;
        scelle[dernier] ^= 1;
        assert!(b.open(&scelle).is_err());
        scelle[dernier] ^= 1;
        scelle[0] ^= 1;
        assert!(b.open(&scelle).is_err());
    }

    #[test]
    fn une_charge_tronquee_est_refusee_sans_paniquer() {
        // 🔒 La charge vient du réseau : un découpage naïf sur un tampon plus
        // court que le nonce paniquerait, et une panique en production est
        // interdite par le lint anti-panic du dépôt.
        let code = PairingCode::generate();
        let (a, _) = echange(&code, &code);
        for n in 0..=NONCE_LEN {
            assert!(a.open(&vec![0u8; n]).is_err(), "longueur {n}");
        }
    }

    #[test]
    fn la_racine_traverse_le_canal_et_revient_identique() {
        let code = PairingCode::generate();
        let (autorise, nouveau) = echange(&code, &code);
        let graine = AccountSeed::new([7u8; 32]);

        let scelle = autorise.seal_account_seed(&graine).unwrap();
        assert_eq!(
            nouveau.open_account_seed(&scelle).unwrap().expose(),
            &[7; 32]
        );
    }

    #[test]
    fn la_racine_ne_voyage_jamais_en_clair() {
        // 🔒 Le contrôle le plus bête et le plus utile : les octets qui
        // partent sur le fil ne doivent contenir la graine nulle part.
        let code = PairingCode::generate();
        let (a, _) = echange(&code, &code);
        let graine = AccountSeed::new([0xAB; 32]);
        let scelle = a.seal_account_seed(&graine).unwrap();
        assert!(
            !scelle.windows(32).any(|w| w == graine.expose()),
            "la graine apparaît telle quelle dans la charge scellée"
        );
    }

    #[test]
    fn la_racine_ne_fuit_pas_dans_une_trace() {
        // 🔒 Un `Debug` bavard met le compte entier dans les journaux, où il
        // survit au processus qui l'a écrit.
        let graine = AccountSeed::new([0x5A; 32]);
        let trace = format!("{graine:?}");
        let hex: String = graine.expose().iter().map(|b| format!("{b:02x}")).collect();
        assert!(!trace.contains(&hex), "la racine fuit : {trace}");
        assert!(!trace.contains("90"), "la racine fuit en décimal : {trace}");
    }

    #[test]
    fn un_canal_etranger_ne_peut_pas_ouvrir_la_racine() {
        // 🔒 Qui n'avait pas le code n'obtient pas la racine, même en
        // interceptant la charge : c'est l'AEAD du canal qui le dit.
        let code = PairingCode::generate();
        let (a, _) = echange(&code, &code);
        let (etranger, _) = echange(&PairingCode::generate(), &PairingCode::generate());

        let scelle = a.seal_account_seed(&AccountSeed::new([1u8; 32])).unwrap();
        assert!(etranger.open_account_seed(&scelle).is_err());
    }

    #[test]
    fn une_charge_dappareil_nest_pas_relue_comme_une_racine() {
        // 🔒 Le même canal scelle l'entrée d'appareil dans un sens et la
        // racine dans l'autre. Sans étiquette de contenu, une charge réfléchie
        // se relirait comme l'autre — et trente-deux octets d'entrée
        // d'appareil deviendraient « une graine ».
        let code = PairingCode::generate();
        let (a, b) = echange(&code, &code);
        let entree = a.seal(&[0u8; 33]).unwrap();
        assert!(b.open_account_seed(&entree).is_err());
    }

    #[test]
    fn une_racine_de_longueur_fausse_est_refusee() {
        let code = PairingCode::generate();
        let (a, b) = echange(&code, &code);
        // Longueurs de graine plausibles mais fausses : la seule acceptée est
        // exactement 32 octets après l'étiquette.
        for n in [0usize, 1, 31, 33, 64] {
            let mut clair = vec![SEED_TAG];
            clair.resize(1 + n, 0u8);
            let scelle = a.seal(&clair).unwrap();
            assert!(
                b.open_account_seed(&scelle).is_err(),
                "une charge de {n} octets ne doit pas passer pour une graine"
            );
        }
    }

    #[test]
    fn deux_scellements_du_meme_texte_different() {
        // Nonce aléatoire : deux envois identiques ne doivent pas produire le
        // même chiffré, sans quoi un observateur les reconnaîtrait.
        let code = PairingCode::generate();
        let (a, _) = echange(&code, &code);
        assert_ne!(
            a.seal(b"meme texte").unwrap(),
            a.seal(b"meme texte").unwrap()
        );
    }
}
