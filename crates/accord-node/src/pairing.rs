//! État d'appairage côté nœud (multi-appareil, jalon 1, lot 1.D).
//!
//! La cryptographie vit dans `accord_crypto::pairing` ; ici vivent les règles
//! qui l'entourent et sans lesquelles elle ne protège rien : **usage unique**,
//! **expiration**, **cadence limitée**.
//!
//! 🔒 Ces trois règles sont ce qui rend un code court acceptable. Le PAKE fait
//! payer chaque essai d'une interaction en ligne ; encore faut-il que le
//! nombre d'interactions soit borné. Un code réutilisable, éternel ou
//! devinable en rafale annulerait tout le bénéfice du PAKE.
//!
//! Machine à états pure, sans horloge ni réseau : le temps arrive en
//! paramètre. C'est ce qui rend l'expiration et la cadence testables sans
//! attendre cinq minutes.

use accord_crypto::pairing::{PairedChannel, PairingCode, PairingHandshake, CODE_TTL_MS};

/// Tentatives d'appairage acceptées par fenêtre.
///
/// Trois essais : de quoi se tromper deux fois en recopiant un code, pas de
/// quoi explorer un espace de 39 bits.
pub const MAX_ATTEMPTS: u32 = 3;

/// Pourquoi une tentative d'appairage a été refusée.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairingRefusal {
    /// Aucun appairage n'est en cours sur cet appareil.
    NoOffer,
    /// Le code a dépassé ses cinq minutes.
    Expired,
    /// Le code a déjà servi. Un appairage, un code.
    AlreadyUsed,
    /// Trop de tentatives : l'offre est brûlée.
    TooManyAttempts,
    /// L'échange PAKE n'aboutit pas — l'autre côté n'a pas le même code.
    BadExchange,
}

/// Une offre d'appairage en cours sur l'appareil déjà autorisé.
///
/// Créée quand l'utilisateur demande « Ajouter un appareil », consommée par la
/// première tentative qui aboutit, détruite dans tous les autres cas.
pub struct PairingOffer {
    code: PairingCode,
    handshake: Option<PairingHandshake>,
    /// Message PAKE à transmettre au nouvel appareil.
    outgoing: Vec<u8>,
    created_ms: u64,
    attempts: u32,
    used: bool,
}

impl PairingOffer {
    /// Ouvre une offre : tire un code et prépare notre moitié de l'échange.
    pub fn open(now_ms: u64) -> Self {
        Self::open_with_code(PairingCode::generate(), now_ms)
    }

    /// [`PairingOffer::open`] avec un code imposé (tests).
    pub fn open_with_code(code: PairingCode, now_ms: u64) -> Self {
        let (handshake, outgoing) = PairingHandshake::start(&code);
        Self {
            code,
            handshake: Some(handshake),
            outgoing,
            created_ms: now_ms,
            attempts: 0,
            used: false,
        }
    }

    /// Le code à afficher (et à encoder en QR).
    pub fn code(&self) -> &PairingCode {
        &self.code
    }

    /// Notre message PAKE, à transmettre au nouvel appareil.
    pub fn outgoing(&self) -> &[u8] {
        &self.outgoing
    }

    /// Instant d'expiration de l'offre (ms epoch).
    pub fn expires_ms(&self) -> u64 {
        self.created_ms.saturating_add(CODE_TTL_MS)
    }

    /// Vrai si l'offre ne peut plus rien accepter — expirée, consommée ou
    /// épuisée. L'écran s'en sert pour cesser d'afficher le code.
    pub fn is_spent(&self, now_ms: u64) -> bool {
        self.used || self.attempts >= MAX_ATTEMPTS || now_ms >= self.expires_ms()
    }

    /// Traite le message PAKE du nouvel appareil.
    ///
    /// 🔒 L'ordre des contrôles compte. L'expiration et l'usage se vérifient
    /// **avant** de consommer une tentative : une offre déjà morte ne doit pas
    /// voir son compteur bouger, sinon l'état de l'écran dépendrait de
    /// sollicitations extérieures.
    ///
    /// La tentative est **comptée**, qu'elle aboutisse ou non. C'est tout
    /// l'intérêt : chaque essai coûte une interaction, et il n'y en a que
    /// trois.
    ///
    /// 🔒 **Un canal rendu ici n'est pas un appairage.** En SPAKE2 symétrique,
    /// `finish` réussit des deux côtés même quand les codes diffèrent — les
    /// clés dérivées sont simplement différentes. Une erreur ne signale qu'un
    /// message mal formé, jamais « mauvais code ». C'est pourquoi l'offre
    /// n'est **pas** consommée ici : elle l'est à [`PairingOffer::confirm`],
    /// après que deux humains ont comparé l'empreinte. Marquer l'offre
    /// consommée dès l'échange laisserait n'importe qui la détruire en
    /// envoyant un message bien formé.
    pub fn accept(
        &mut self,
        peer_msg: &[u8],
        now_ms: u64,
    ) -> Result<PairedChannel, PairingRefusal> {
        if self.used {
            return Err(PairingRefusal::AlreadyUsed);
        }
        if now_ms >= self.expires_ms() {
            return Err(PairingRefusal::Expired);
        }
        if self.attempts >= MAX_ATTEMPTS {
            return Err(PairingRefusal::TooManyAttempts);
        }
        self.attempts += 1;

        // `finish` consomme l'état SPAKE2 : on le reprend, et on en repose un
        // neuf pour l'essai suivant. 🔒 Jamais le même : rejouer un état SPAKE2
        // sur plusieurs essais donnerait à l'attaquant plusieurs observations
        // du même secret.
        let handshake = self.handshake.take().ok_or(PairingRefusal::AlreadyUsed)?;
        let issue = handshake.finish(peer_msg);
        let (fresh, outgoing) = PairingHandshake::start(&self.code);
        self.handshake = Some(fresh);
        self.outgoing = outgoing;
        issue.map_err(|_| PairingRefusal::BadExchange)
    }

    /// Consomme l'offre, une fois l'empreinte confirmée par un humain des
    /// **deux** côtés.
    ///
    /// 🔒 C'est ici, et nulle part avant, que l'appairage devient définitif.
    /// L'appelant ne doit appeler cette méthode qu'après confirmation
    /// explicite — pas après un simple échange réussi (voir
    /// [`PairingOffer::accept`]).
    pub fn confirm(&mut self, now_ms: u64) -> Result<(), PairingRefusal> {
        if self.used {
            return Err(PairingRefusal::AlreadyUsed);
        }
        if now_ms >= self.expires_ms() {
            return Err(PairingRefusal::Expired);
        }
        self.used = true;
        Ok(())
    }
}

/// 🔒 `Debug` muet : le code vit ici, et une offre dans une trace, c'est le
/// code dans un fichier de journal.
impl std::fmt::Debug for PairingOffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PairingOffer")
            .field("attempts", &self.attempts)
            .field("used", &self.used)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: u64 = 1_700_000_000_000;

    fn code(s: &str) -> PairingCode {
        PairingCode::parse(s).expect("code de test valide")
    }

    /// Le message PAKE qu'un nouvel appareil produirait avec `c`.
    fn message_du_nouvel_appareil(c: &PairingCode) -> Vec<u8> {
        PairingHandshake::start(c).1
    }

    #[test]
    fn un_appairage_nominal_aboutit_des_deux_cotes() {
        let c = code("ABCDEFGH");
        let mut offre = PairingOffer::open_with_code(c.clone(), T0);

        // Le nouvel appareil joue sa moitié avec le même code.
        let (nouveau, msg_nouveau) = PairingHandshake::start(&c);
        // ⚠️ `outgoing()` est relu AVANT `accept`, qui en repose un neuf pour
        // l'essai suivant : le nouvel appareil doit voir le message de CET
        // échange, pas celui du suivant.
        let msg_autorise = offre.outgoing().to_vec();
        let cote_autorise = offre.accept(&msg_nouveau, T0).expect("échange abouti");
        let cote_nouveau = nouveau.finish(&msg_autorise).expect("échange abouti");

        assert_eq!(cote_autorise.key(), cote_nouveau.key());
        assert_eq!(cote_autorise.fingerprint(), cote_nouveau.fingerprint());

        // L'offre n'est consommée qu'à la confirmation de l'empreinte.
        assert!(!offre.is_spent(T0));
        offre.confirm(T0).expect("confirmation acceptée");
        assert!(offre.is_spent(T0));
    }

    #[test]
    fn un_code_ne_sert_quune_fois() {
        // 🔒 Sans ça, un code capté servirait à appairer un second appareil
        // après le légitime, sans que personne ne le remarque.
        let c = code("ABCDEFGH");
        let mut offre = PairingOffer::open_with_code(c.clone(), T0);
        offre
            .accept(&message_du_nouvel_appareil(&c), T0)
            .expect("échange abouti");
        offre.confirm(T0).expect("première confirmation acceptée");

        assert_eq!(
            offre.accept(&message_du_nouvel_appareil(&c), T0).err(),
            Some(PairingRefusal::AlreadyUsed)
        );
        assert_eq!(offre.confirm(T0).err(), Some(PairingRefusal::AlreadyUsed));
        assert!(offre.is_spent(T0));
    }

    #[test]
    fn un_code_expire_apres_cinq_minutes() {
        let c = code("ABCDEFGH");
        let mut offre = PairingOffer::open_with_code(c.clone(), T0);

        // Juste avant l'échéance : encore bon.
        let juste_avant = T0 + CODE_TTL_MS - 1;
        assert!(!offre.is_spent(juste_avant));

        // À l'échéance pile : fini. La borne est inclusive du côté du refus,
        // pour qu'un code affiché « 0:00 » ne marche déjà plus.
        let echeance = T0 + CODE_TTL_MS;
        assert!(offre.is_spent(echeance));
        assert_eq!(
            offre
                .accept(&message_du_nouvel_appareil(&c), echeance)
                .err(),
            Some(PairingRefusal::Expired)
        );
    }

    #[test]
    fn une_offre_expiree_ne_consomme_pas_de_tentative() {
        // L'état affiché ne doit pas dépendre de sollicitations extérieures :
        // un tiers qui frappe à la porte après l'expiration ne change rien.
        let c = code("ABCDEFGH");
        let mut offre = PairingOffer::open_with_code(c.clone(), T0);
        let apres = T0 + CODE_TTL_MS;

        for _ in 0..10 {
            assert_eq!(
                offre.accept(&message_du_nouvel_appareil(&c), apres).err(),
                Some(PairingRefusal::Expired)
            );
        }
        assert_eq!(offre.attempts, 0);
    }

    #[test]
    fn trois_essais_ratés_brûlent_loffre() {
        // 🔒 Le cœur de la cadence limitée. Le PAKE rend chaque essai coûteux ;
        // ce compteur borne leur nombre. Sans lui, 39 bits finiraient par
        // tomber.
        let bon = code("ABCDEFGH");
        let mauvais = code("ABCDEFGJ");
        let mut offre = PairingOffer::open_with_code(bon.clone(), T0);

        for _ in 0..MAX_ATTEMPTS {
            // ⚠️ Un mauvais code n'échoue PAS : SPAKE2 symétrique dérive une
            // clé de toute façon, simplement différente. C'est le compteur —
            // pas une erreur — qui borne les essais.
            let _ = offre.accept(&message_du_nouvel_appareil(&mauvais), T0);
        }

        assert!(offre.is_spent(T0));
        // Même le BON code ne passe plus : l'offre est brûlée, il faut en
        // rouvrir une — donc repasser devant l'appareil autorisé.
        assert_eq!(
            offre.accept(&message_du_nouvel_appareil(&bon), T0).err(),
            Some(PairingRefusal::TooManyAttempts)
        );
    }

    #[test]
    fn un_essai_rate_repart_dun_alea_neuf() {
        // 🔒 Rejouer le même état SPAKE2 sur plusieurs essais donnerait à
        // l'attaquant plusieurs observations du même secret.
        let bon = code("ABCDEFGH");
        let mauvais = code("ABCDEFGJ");
        let mut offre = PairingOffer::open_with_code(bon, T0);

        let premier = offre.outgoing().to_vec();
        let _ = offre.accept(&message_du_nouvel_appareil(&mauvais), T0);
        assert_ne!(
            offre.outgoing(),
            premier.as_slice(),
            "un nouvel essai doit repartir d'un message neuf"
        );
    }

    #[test]
    fn un_echange_abouti_ne_consomme_pas_loffre_a_lui_seul() {
        // 🔒 La correction qui compte. `finish` réussit même avec un mauvais
        // code — il ne dit rien d'autre que « message bien formé ». Consommer
        // l'offre là-dessus laisserait n'importe qui la détruire à distance,
        // et l'utilisateur légitime devrait recommencer sans comprendre.
        let bon = code("ABCDEFGH");
        let mauvais = code("ABCDEFGJ");
        let mut offre = PairingOffer::open_with_code(bon.clone(), T0);

        let _ = offre.accept(&message_du_nouvel_appareil(&mauvais), T0);
        assert!(
            !offre.is_spent(T0),
            "un intrus ne doit pas pouvoir consommer l'offre"
        );

        // Le vrai appareil peut encore aboutir.
        offre
            .accept(&message_du_nouvel_appareil(&bon), T0)
            .expect("échange abouti");
        offre.confirm(T0).expect("confirmation acceptée");
    }

    #[test]
    fn une_empreinte_non_confirmee_laisse_loffre_expirer_sans_appairer() {
        // L'utilisateur voit deux nombres différents et n'appuie sur rien :
        // rien ne doit être signé, et l'offre doit mourir d'elle-même.
        let c = code("ABCDEFGH");
        let mut offre = PairingOffer::open_with_code(c.clone(), T0);
        offre
            .accept(&message_du_nouvel_appareil(&c), T0)
            .expect("échange abouti");

        let apres = T0 + CODE_TTL_MS;
        assert_eq!(offre.confirm(apres).err(), Some(PairingRefusal::Expired));
        assert!(offre.is_spent(apres));
    }

    #[test]
    fn le_code_ne_fuit_pas_dans_une_trace() {
        let c = code("ABCDEFGH");
        let offre = PairingOffer::open_with_code(c, T0);
        let trace = format!("{offre:?}");
        assert!(!trace.contains("ABCDEFGH"), "le code fuit : {trace}");
    }

    #[test]
    fn lexpiration_ne_deborde_pas_sur_une_horloge_absurde() {
        // Une horloge système déréglée ne doit pas faire paniquer un calcul
        // d'échéance — d'où `saturating_add`.
        let offre = PairingOffer::open_with_code(code("ABCDEFGH"), u64::MAX);
        assert_eq!(offre.expires_ms(), u64::MAX);
        assert!(offre.is_spent(u64::MAX));
    }
}
