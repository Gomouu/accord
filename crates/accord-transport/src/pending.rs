//! Handshake initiateur en cours, et file d'attente bornée des messages
//! applicatifs émis vers un pair pas encore joignable.
//!
//! Extrait d'`endpoint.rs` : la file d'attente a ses propres invariants (deux
//! plafonds, une comptabilité d'octets tenue à jour) et se teste sans socket,
//! sans horloge et sans identité — rien de tout cela n'avait besoin des deux
//! mille lignes de l'endpoint autour.

use accord_crypto::handshake::Initiator;

/// Fresh-handshake regenerations attempted for a single pending before it is
/// abandoned. Once the identical retransmissions of one generation are spent, a
/// new HELLO with a fresh nonce is started (recovering a lost WELCOME, which the
/// responder's replay cache would otherwise eat) and the reconnection attempt
/// persists — bounded here, never an unconditional periodic redial (Lot G,
/// causes 1 and 2).
pub(crate) const MAX_HANDSHAKE_GENERATIONS: u32 = 8;

/// Octets de plaintext applicatif mis en file d'attente au plus par handshake
/// en cours, avant que les plus ANCIENS ne soient abandonnés.
///
/// Un [`Pending`] collecte les messages émis vers un pair tant que la session
/// n'est pas établie. Rien ne bornait cette file : un pair injoignable
/// (éteint, filtré, adresse périmée) la laissait grandir jusqu'à l'abandon du
/// handshake — jusqu'à `MAX_HANDSHAKE_GENERATIONS × DHT_RPC_TIMEOUT_MS`, soit
/// une demi-minute pendant laquelle une couche haute qui réémet (outbox,
/// fenêtres de fichiers) peut y verser des mébioctets, par adresse visée.
///
/// Le plafond porte sur les OCTETS et non sur le nombre de messages : ce sont
/// les octets qui pèsent, et un message vaut de dix octets à un mébioctet. Ce
/// sont les plus ANCIENS qui cèdent la place : à la reconnexion, c'est
/// l'annonce de profil la plus fraîche qui compte, et la file entière est de
/// toute façon perdue si le handshake échoue.
pub(crate) const MAX_PENDING_QUEUE_BYTES: usize = 4 * 1024 * 1024;

/// Messages mis en file d'attente au plus par handshake en cours. Le plafond
/// d'octets seul laisserait passer des millions de messages minuscules, dont
/// chacun coûte une allocation et un décalage de vecteur à l'éviction ; celui-ci
/// borne ce nombre, très au-delà de ce qu'une reconnexion légitime accumule.
pub(crate) const MAX_PENDING_QUEUE_MSGS: usize = 4_096;

/// Handshake initiateur en cours vers un pair, avec sa file d'attente.
pub(crate) struct Pending {
    pub(crate) initiator: Initiator,
    /// Identité Ed25519 attendue à cette adresse, si l'ouverture vise un pair
    /// précis (livraison CORE liée). `None` pour un pair quelconque (DHT) :
    /// aucune liaison n'est alors imposée. Le WELCOME établi est refusé si sa
    /// clé statique diffère de cette cible (voir `on_welcome`).
    pub(crate) expected_static: Option<[u8; 32]>,
    pub(crate) queued: Vec<Vec<u8>>,
    /// Somme des longueurs de `queued`, tenue à jour plutôt que recalculée :
    /// la file est plafonnée en OCTETS, et la recalculer à chaque envoi
    /// rendrait quadratique le remplissage d'une file de petits messages.
    pub(crate) queued_bytes: usize,
    pub(crate) attempts: u32,
    pub(crate) last_send_ms: u64,
    /// Fresh-handshake generation of this pending (Lot G): incremented each time
    /// the initiator is restarted with a new nonce after its retransmissions are
    /// spent, and capped by [`MAX_HANDSHAKE_GENERATIONS`].
    pub(crate) generation: u32,
}

impl Pending {
    /// Met un plaintext applicatif en file d'attente, en respectant les
    /// plafonds [`MAX_PENDING_QUEUE_BYTES`] et [`MAX_PENDING_QUEUE_MSGS`] : les
    /// messages les plus anciens sont abandonnés d'abord. Le message qu'on
    /// vient d'accepter n'est jamais évincé — il a déjà passé le contrôle
    /// `frag::MAX_MESSAGE_LEN` (1 MiB), il tient donc largement seul.
    pub(crate) fn push_queued(&mut self, plaintext: Vec<u8>) {
        self.queued_bytes = self.queued_bytes.saturating_add(plaintext.len());
        self.queued.push(plaintext);
        let mut abandonnes = 0usize;
        while (self.queued_bytes > MAX_PENDING_QUEUE_BYTES
            || self.queued.len() > MAX_PENDING_QUEUE_MSGS)
            && self.queued.len() > 1
        {
            let perdu = self.queued.remove(0).len();
            self.queued_bytes = self.queued_bytes.saturating_sub(perdu);
            abandonnes += 1;
        }
        if abandonnes > 0 {
            tracing::debug!(
                abandonnes,
                "file de handshake saturée : messages les plus anciens abandonnés"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use accord_crypto::Identity;

    fn pending() -> Pending {
        let identity = Identity::generate_with_pow_bits(1);
        Pending {
            initiator: Initiator::start(&identity, 0, Vec::new(), 1, None, None),
            expected_static: None,
            queued: Vec::new(),
            queued_bytes: 0,
            attempts: 0,
            last_send_ms: 0,
            generation: 0,
        }
    }

    /// Somme réelle des longueurs, pour éprouver la comptabilité incrémentale.
    fn octets(p: &Pending) -> usize {
        p.queued.iter().map(Vec::len).sum()
    }

    #[test]
    fn la_file_reste_sous_le_plafond_d_octets() {
        // Un pair injoignable et une couche haute qui réémet : sans plafond,
        // la file grandissait jusqu'à l'abandon du handshake.
        let mut p = pending();
        let gros = 64 * 1024;
        for _ in 0..200 {
            p.push_queued(vec![0u8; gros]);
        }
        assert!(
            p.queued_bytes <= MAX_PENDING_QUEUE_BYTES,
            "file non bornée : {} octets",
            p.queued_bytes
        );
        assert_eq!(p.queued_bytes, octets(&p), "comptabilité désynchronisée");
    }

    #[test]
    fn la_file_reste_sous_le_plafond_de_messages() {
        // Le plafond d'octets seul laisserait passer des millions de messages
        // minuscules, chacun coûtant une allocation.
        let mut p = pending();
        for _ in 0..(MAX_PENDING_QUEUE_MSGS + 500) {
            p.push_queued(vec![1u8]);
        }
        assert!(p.queued.len() <= MAX_PENDING_QUEUE_MSGS);
        assert_eq!(p.queued_bytes, octets(&p));
    }

    #[test]
    fn ce_sont_les_plus_anciens_qui_cedent_la_place() {
        // À la reconnexion, c'est l'annonce la plus FRAÎCHE qui compte.
        let mut p = pending();
        for i in 0..(MAX_PENDING_QUEUE_MSGS as u32 + 3) {
            p.push_queued(i.to_be_bytes().to_vec());
        }
        let dernier = (MAX_PENDING_QUEUE_MSGS as u32 + 2).to_be_bytes().to_vec();
        assert_eq!(p.queued.last(), Some(&dernier), "le plus récent a survécu");
        assert!(!p.queued.contains(&0u32.to_be_bytes().to_vec()));
    }

    #[test]
    fn un_message_seul_n_est_jamais_evince() {
        // Un plaintext peut atteindre 1 MiB (borne de fragmentation) : il tient
        // largement sous le plafond, mais la boucle d'éviction ne doit en aucun
        // cas vider la file du message qu'elle vient d'accepter.
        let mut p = pending();
        p.push_queued(vec![7u8; MAX_PENDING_QUEUE_BYTES + 1]);
        assert_eq!(p.queued.len(), 1);
        assert_eq!(p.queued_bytes, octets(&p));
    }
}
