//! Fragmentation et réassemblage transparents à l'intérieur d'une session
//! chiffrée (SPEC §13.1).
//!
//! La MTU applicative UDP est de 1 200 octets (SPEC §13) : un plaintext
//! applicatif plus grand que la charge utile d'un datagramme ne peut pas être
//! scellé d'un seul tenant. Cette couche découpe le plaintext *avant*
//! chiffrement en fragments scellés indépendamment (chacun a son propre nonce
//! via la crypto de session), et les réassemble à la réception.
//!
//! Le découpage est un détail INTERNE à la session : il s'applique après
//! déchiffrement du paquet DATA, donc il n'y a pas de compatibilité filaire à
//! gérer — les deux extrémités utilisent la même version. Aucun octet de
//! l'enveloppe DATA n'est modifié.
//!
//! ## Format d'un cadre de session (plaintext scellé)
//!
//! Chaque plaintext scellé commence par un octet de genre :
//!
//! - `0x00` — cadre unique : `[0x00][charge applicative]`. Surcoût 1 octet ;
//!   utilisé dès que le message tient dans un datagramme.
//! - `0x01` — fragment : `[0x01][id: u32 BE][total: u16 BE][index: u16 BE][tranche]`.
//!   Surcoût 9 octets par fragment. `id` identifie le message au sein de la
//!   session, `total` le nombre de fragments, `index` la position (0-based).
//!
//! La perte d'un seul fragment (UDP, pas de retransmission ici) fait échouer le
//! message entier ; les couches hautes (outbox, fenêtres de fichiers)
//! réémettent.

use crate::error::TransportError;
use accord_proto::envelope::DATA_HEADER_LEN;
use accord_proto::limits::{MAX_TCP_FRAME, UDP_MTU};
use std::collections::HashMap;

/// Octet de genre : cadre unique (message tenant dans un datagramme).
const FRAME_SINGLE: u8 = 0x00;
/// Octet de genre : fragment d'un message plus grand que la MTU.
const FRAME_FRAG: u8 = 0x01;

/// Surcoût d'un cadre unique : l'octet de genre.
const SINGLE_HEADER_LEN: usize = 1;
/// Surcoût d'un fragment : genre(1) + id(4) + total(2) + index(2).
const FRAG_HEADER_LEN: usize = 9;

/// Longueur du tag AEAD (Poly1305) ajouté par le scellement de session.
const AEAD_TAG_LEN: usize = 16;

/// Plaintext maximal scellable dans un unique datagramme UDP.
///
/// Taille filaire d'un paquet DATA = [`DATA_HEADER_LEN`] + tag AEAD + plaintext.
/// On borne cette somme à [`UDP_MTU`].
pub(crate) const MAX_SEALED_PLAINTEXT: usize = UDP_MTU - DATA_HEADER_LEN - AEAD_TAG_LEN;

/// Charge applicative maximale d'un cadre unique.
pub(crate) const MAX_SINGLE_PAYLOAD: usize = MAX_SEALED_PLAINTEXT - SINGLE_HEADER_LEN;

/// Taille maximale d'une tranche transportée par un fragment.
pub(crate) const MAX_FRAGMENT_CHUNK: usize = MAX_SEALED_PLAINTEXT - FRAG_HEADER_LEN;

/// Taille maximale d'un message applicatif réassemblé (aligné sur
/// [`MAX_TCP_FRAME`] d'accord-proto : 1 MiB).
pub(crate) const MAX_MESSAGE_LEN: usize = MAX_TCP_FRAME;

/// Nombre maximal de fragments qu'un message légitime peut produire :
/// `ceil(MAX_MESSAGE_LEN / MAX_FRAGMENT_CHUNK)` (≈ 908 avec la MTU du SPEC).
///
/// 🔒 C'est une borne d'ALLOCATION, pas seulement de cohérence. Le champ
/// `total` d'un fragment est un `u16` piloté par le pair : sans cette borne, un
/// unique datagramme de quelques octets annonçant `total = 65535` faisait
/// pré-allouer un `Vec<Option<Vec<u8>>>` de 65 535 cases (~1,5 Mio), et
/// [`MAX_CONCURRENT_REASSEMBLIES`] datagrammes en réservaient une douzaine de
/// mébioctets — une amplification mémoire de plusieurs milliers, invisible pour
/// le compteur [`Reassembler::octets_totaux`] qui ne pèse que les tranches
/// reçues. Aucun émetteur conforme ne dépasse cette borne (`frame` découpe un
/// plaintext déjà borné à [`MAX_MESSAGE_LEN`]) : le format filaire est
/// inchangé.
pub(crate) const MAX_FRAGMENTS: usize = MAX_MESSAGE_LEN.div_ceil(MAX_FRAGMENT_CHUNK);

/// Mémoire de réassemblage plafonnée par session (anti-DoS).
pub(crate) const MAX_REASSEMBLY_BYTES: usize = 2 * 1024 * 1024;

/// Nombre maximal de messages en cours de réassemblage simultané par session.
pub(crate) const MAX_CONCURRENT_REASSEMBLIES: usize = 8;

/// Délai d'abandon d'un réassemblage partiel (millisecondes).
pub(crate) const REASSEMBLY_TIMEOUT_MS: u64 = 30_000;

/// Découpe un plaintext applicatif en cadres de session prêts à sceller.
///
/// Un message tenant dans un datagramme produit un unique cadre `SINGLE`
/// (surcoût 1 octet). Sinon il est fragmenté ; `next_id` fournit un identifiant
/// de message unique au sein de la session (compteur à repli).
///
/// L'appelant garantit `plaintext.len() <= MAX_MESSAGE_LEN` (vérifié en amont).
pub(crate) fn frame(plaintext: &[u8], next_id: &mut u32) -> Vec<Vec<u8>> {
    if plaintext.len() <= MAX_SINGLE_PAYLOAD {
        let mut cadre = Vec::with_capacity(SINGLE_HEADER_LEN + plaintext.len());
        cadre.push(FRAME_SINGLE);
        cadre.extend_from_slice(plaintext);
        return vec![cadre];
    }

    let msg_id = *next_id;
    *next_id = next_id.wrapping_add(1);

    let tranches: Vec<&[u8]> = plaintext.chunks(MAX_FRAGMENT_CHUNK).collect();
    // `total` tient dans un u16 : MAX_MESSAGE_LEN / MAX_FRAGMENT_CHUNK ≈ 907.
    let total = tranches.len() as u16;
    tranches
        .into_iter()
        .enumerate()
        .map(|(index, tranche)| {
            let mut cadre = Vec::with_capacity(FRAG_HEADER_LEN + tranche.len());
            cadre.push(FRAME_FRAG);
            cadre.extend_from_slice(&msg_id.to_be_bytes());
            cadre.extend_from_slice(&total.to_be_bytes());
            cadre.extend_from_slice(&(index as u16).to_be_bytes());
            cadre.extend_from_slice(tranche);
            cadre
        })
        .collect()
}

/// Message partiellement réassemblé.
struct Partial {
    total: u16,
    tranches: Vec<Option<Vec<u8>>>,
    recus: u16,
    octets: usize,
    premier_vu_ms: u64,
}

/// Réassembleur borné par session (anti-DoS).
#[derive(Default)]
pub(crate) struct Reassembler {
    en_cours: HashMap<u32, Partial>,
    octets_totaux: usize,
}

impl Reassembler {
    /// Crée un réassembleur vide.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Purge les réassemblages partiels dont le premier fragment est plus vieux
    /// que [`REASSEMBLY_TIMEOUT_MS`].
    ///
    /// Les soustractions du compteur global sont saturantes : une dérive de
    /// comptabilité (impossible aujourd'hui, mais le compteur vit à côté de
    /// trois chemins de retrait) déborderait sinon un `usize` en release et
    /// figerait DÉFINITIVEMENT le réassemblage de la session — un pair
    /// n'obtiendrait plus jamais un message fragmenté.
    pub(crate) fn sweep(&mut self, now_ms: u64) {
        let octets_totaux = &mut self.octets_totaux;
        self.en_cours.retain(|_, p| {
            let vivant = now_ms.saturating_sub(p.premier_vu_ms) < REASSEMBLY_TIMEOUT_MS;
            if !vivant {
                *octets_totaux = octets_totaux.saturating_sub(p.octets);
            }
            vivant
        });
    }

    /// Accepte un cadre de session déchiffré.
    ///
    /// Rend `Ok(Some(message))` quand un message applicatif complet est
    /// disponible, `Ok(None)` si le cadre était un fragment partiel (ou un
    /// doublon), et `Err` si le cadre est incohérent ou dépasse une borne
    /// anti-DoS (le message fautif est abandonné, la session reste ouverte).
    pub(crate) fn accept(
        &mut self,
        cadre: &[u8],
        now_ms: u64,
    ) -> Result<Option<Vec<u8>>, TransportError> {
        let (&genre, corps) = cadre
            .split_first()
            .ok_or(TransportError::Reassembly("cadre vide"))?;
        match genre {
            FRAME_SINGLE => {
                if corps.len() > MAX_MESSAGE_LEN {
                    return Err(TransportError::Reassembly("cadre unique trop grand"));
                }
                Ok(Some(corps.to_vec()))
            }
            FRAME_FRAG => self.accept_fragment(corps, now_ms),
            _ => Err(TransportError::Reassembly("genre de cadre inconnu")),
        }
    }

    fn accept_fragment(
        &mut self,
        corps: &[u8],
        now_ms: u64,
    ) -> Result<Option<Vec<u8>>, TransportError> {
        // En-tête fragment (hors octet de genre déjà consommé) : 8 octets.
        if corps.len() < FRAG_HEADER_LEN - 1 {
            return Err(TransportError::Reassembly("en-tête fragment tronqué"));
        }
        let msg_id = u32::from_be_bytes([corps[0], corps[1], corps[2], corps[3]]);
        let total = u16::from_be_bytes([corps[4], corps[5]]);
        let index = u16::from_be_bytes([corps[6], corps[7]]);
        let tranche = &corps[8..];

        if total == 0 || index >= total {
            return Err(TransportError::Reassembly("indices de fragment invalides"));
        }
        // Borne d'allocation AVANT toute réservation (voir [`MAX_FRAGMENTS`]).
        if total as usize > MAX_FRAGMENTS {
            return Err(TransportError::Reassembly("trop de fragments annoncés"));
        }
        if tranche.len() > MAX_FRAGMENT_CHUNK {
            return Err(TransportError::Reassembly(
                "tranche de fragment trop grande",
            ));
        }

        // Purge des réassemblages expirés avant toute allocation.
        self.sweep(now_ms);

        // Message d'un seul fragment : livraison directe, sans état.
        if total == 1 {
            return Ok(Some(tranche.to_vec()));
        }

        // Cohérence avec un réassemblage existant (borrow relâché aussitôt).
        // On n'indexe `p.tranches[index]` QUE si le total concorde : `index`
        // n'a été borné que par le `total` du fragment ENTRANT (l. 183), pas
        // par la longueur du partiel existant (dimensionnée à son propre
        // total). Sans cette garde, un pair pouvait envoyer un 2ᵉ fragment de
        // même msg_id avec un total plus grand et un index hors des bornes du
        // partiel → panique d'indexation (Vec), sous le verrou d'état →
        // empoisonnement du Mutex → arrêt de toute la messagerie.
        let (existe, meme_total, deja_recu) = match self.en_cours.get(&msg_id) {
            Some(p) if p.total == total => (true, true, p.tranches[index as usize].is_some()),
            Some(_) => (true, false, false),
            None => (false, true, false),
        };
        if existe && !meme_total {
            self.retirer(msg_id);
            return Err(TransportError::Reassembly("total de fragments incohérent"));
        }
        if deja_recu {
            return Ok(None); // doublon : ignoré
        }

        // Borne mémoire globale de la session.
        if self.octets_totaux.saturating_add(tranche.len()) > MAX_REASSEMBLY_BYTES {
            return Err(TransportError::Reassembly(
                "mémoire de réassemblage saturée",
            ));
        }

        // Création d'un nouveau réassemblage : borne le nombre de messages
        // simultanés.
        if !existe && self.en_cours.len() >= MAX_CONCURRENT_REASSEMBLIES {
            return Err(TransportError::Reassembly(
                "trop de réassemblages simultanés",
            ));
        }

        let partiel = self.en_cours.entry(msg_id).or_insert_with(|| Partial {
            total,
            tranches: (0..total).map(|_| None).collect(),
            recus: 0,
            octets: 0,
            premier_vu_ms: now_ms,
        });

        partiel.tranches[index as usize] = Some(tranche.to_vec());
        partiel.recus += 1;
        partiel.octets += tranche.len();
        self.octets_totaux += tranche.len();

        if partiel.octets > MAX_MESSAGE_LEN {
            self.retirer(msg_id);
            return Err(TransportError::Reassembly("message réassemblé trop grand"));
        }

        if partiel.recus < total {
            return Ok(None);
        }

        // Complet : concaténation dans l'ordre des index.
        let Some(partiel) = self.en_cours.remove(&msg_id) else {
            return Ok(None);
        };
        self.octets_totaux = self.octets_totaux.saturating_sub(partiel.octets);
        let mut message = Vec::with_capacity(partiel.octets);
        for tranche in partiel.tranches {
            match tranche {
                Some(t) => message.extend_from_slice(&t),
                None => return Err(TransportError::Reassembly("fragment manquant")),
            }
        }
        if message.len() > MAX_MESSAGE_LEN {
            return Err(TransportError::Reassembly("message réassemblé trop grand"));
        }
        Ok(Some(message))
    }

    /// Retire un réassemblage et rend le nombre d'octets libérés.
    fn retirer(&mut self, msg_id: u32) -> usize {
        match self.en_cours.remove(&msg_id) {
            Some(p) => {
                self.octets_totaux = self.octets_totaux.saturating_sub(p.octets);
                p.octets
            }
            None => 0,
        }
    }

    /// Nombre de réassemblages en cours (observabilité/tests).
    #[cfg(test)]
    pub(crate) fn en_cours(&self) -> usize {
        self.en_cours.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Réassemble une suite de cadres, rendant le premier message complet.
    fn reassemble(cadres: &[Vec<u8>], now: u64) -> Option<Vec<u8>> {
        let mut r = Reassembler::new();
        for c in cadres {
            if let Ok(Some(msg)) = r.accept(c, now) {
                return Some(msg);
            }
        }
        None
    }

    #[test]
    fn petit_message_tient_dans_un_cadre_unique() {
        let plaintext = vec![7u8; 500];
        let mut id = 0;
        let cadres = frame(&plaintext, &mut id);
        assert_eq!(cadres.len(), 1);
        assert_eq!(cadres[0][0], FRAME_SINGLE);
        assert_eq!(cadres[0].len(), plaintext.len() + SINGLE_HEADER_LEN);
        // L'identifiant n'est pas consommé pour un cadre unique.
        assert_eq!(id, 0);
        assert_eq!(reassemble(&cadres, 0), Some(plaintext));
    }

    #[test]
    fn message_a_la_limite_reste_un_cadre_unique() {
        let plaintext = vec![1u8; MAX_SINGLE_PAYLOAD];
        let mut id = 0;
        let cadres = frame(&plaintext, &mut id);
        assert_eq!(cadres.len(), 1);
        assert!(cadres[0].len() <= MAX_SEALED_PLAINTEXT);
        assert_eq!(reassemble(&cadres, 0), Some(plaintext));
    }

    #[test]
    fn gros_message_est_fragmente_et_reassemble() {
        let plaintext: Vec<u8> = (0..600_000u32).map(|i| (i % 256) as u8).collect();
        let mut id = 42;
        let cadres = frame(&plaintext, &mut id);
        assert!(cadres.len() > 1);
        assert_eq!(id, 43); // un identifiant consommé
        for c in &cadres {
            assert_eq!(c[0], FRAME_FRAG);
            assert!(c.len() <= MAX_SEALED_PLAINTEXT);
        }
        assert_eq!(reassemble(&cadres, 0), Some(plaintext));
    }

    #[test]
    fn reassemblage_resiste_au_desordre() {
        let plaintext: Vec<u8> = (0..300_000u32).map(|i| (i % 256) as u8).collect();
        let mut id = 0;
        let mut cadres = frame(&plaintext, &mut id);
        cadres.reverse();
        assert_eq!(reassemble(&cadres, 0), Some(plaintext));
    }

    #[test]
    fn perte_d_un_fragment_ne_livre_jamais_le_message() {
        let plaintext = vec![9u8; 200_000];
        let mut id = 0;
        let mut cadres = frame(&plaintext, &mut id);
        cadres.remove(3); // perd un fragment
        let mut r = Reassembler::new();
        for c in &cadres {
            assert_eq!(r.accept(c, 0).unwrap(), None);
        }
        assert_eq!(r.en_cours(), 1); // reste partiel
    }

    #[test]
    fn doublon_est_ignore_sans_erreur() {
        let plaintext = vec![5u8; 4000];
        let mut id = 0;
        let cadres = frame(&plaintext, &mut id);
        let mut r = Reassembler::new();
        assert_eq!(r.accept(&cadres[0], 0).unwrap(), None);
        // Rejoue le même fragment : ignoré, pas d'erreur.
        assert_eq!(r.accept(&cadres[0], 0).unwrap(), None);
    }

    #[test]
    fn timeout_abandonne_les_reassemblages_partiels() {
        let plaintext = vec![3u8; 100_000];
        let mut id = 0;
        let cadres = frame(&plaintext, &mut id);
        let mut r = Reassembler::new();
        assert_eq!(r.accept(&cadres[0], 0).unwrap(), None);
        assert_eq!(r.en_cours(), 1);
        // Un fragment tardif au-delà du timeout : l'ancien est purgé.
        r.sweep(REASSEMBLY_TIMEOUT_MS + 1);
        assert_eq!(r.en_cours(), 0);
    }

    #[test]
    fn total_incoherent_est_rejete() {
        // Deux fragments prétendant des totaux différents pour le même id.
        let mut c1 = vec![FRAME_FRAG];
        c1.extend_from_slice(&7u32.to_be_bytes());
        c1.extend_from_slice(&3u16.to_be_bytes());
        c1.extend_from_slice(&0u16.to_be_bytes());
        c1.extend_from_slice(&[1, 2, 3]);
        let mut c2 = vec![FRAME_FRAG];
        c2.extend_from_slice(&7u32.to_be_bytes());
        c2.extend_from_slice(&4u16.to_be_bytes()); // total différent
        c2.extend_from_slice(&1u16.to_be_bytes());
        c2.extend_from_slice(&[4, 5, 6]);
        let mut r = Reassembler::new();
        assert_eq!(r.accept(&c1, 0).unwrap(), None);
        assert!(r.accept(&c2, 0).is_err());
        assert_eq!(r.en_cours(), 0); // le réassemblage fautif est abandonné
    }

    #[test]
    fn total_croissant_avec_index_hors_bornes_du_partiel_ne_panique_pas() {
        // Régression (audit 1.0, CRITICAL) : un 1er fragment fixe le partiel à
        // un total de 2 (tranches de longueur 2), puis un 2e fragment de même
        // id annonce un total plus grand ET un index >= 2 — valide vis-à-vis du
        // NOUVEAU total, mais hors des bornes du partiel existant. Avant le
        // correctif, `p.tranches[index]` paniquait (empoisonnement du Mutex
        // d'état). Doit désormais être rejeté proprement comme incohérent.
        let mut c1 = vec![FRAME_FRAG];
        c1.extend_from_slice(&5u32.to_be_bytes());
        c1.extend_from_slice(&2u16.to_be_bytes()); // total = 2 → tranches len 2
        c1.extend_from_slice(&0u16.to_be_bytes());
        c1.extend_from_slice(&[1, 2, 3]);
        let mut c2 = vec![FRAME_FRAG];
        c2.extend_from_slice(&5u32.to_be_bytes());
        c2.extend_from_slice(&10u16.to_be_bytes()); // total plus grand
        c2.extend_from_slice(&9u16.to_be_bytes()); // index 9 : hors du partiel len 2
        c2.extend_from_slice(&[4, 5, 6]);
        let mut r = Reassembler::new();
        assert_eq!(r.accept(&c1, 0).unwrap(), None);
        assert!(r.accept(&c2, 0).is_err()); // rejeté, PAS de panique
        assert_eq!(r.en_cours(), 0); // partiel incohérent abandonné
    }

    #[test]
    fn indices_invalides_rejetes() {
        // index >= total.
        let mut c = vec![FRAME_FRAG];
        c.extend_from_slice(&1u32.to_be_bytes());
        c.extend_from_slice(&2u16.to_be_bytes());
        c.extend_from_slice(&5u16.to_be_bytes()); // index hors bornes
        c.extend_from_slice(&[0]);
        let mut r = Reassembler::new();
        assert!(r.accept(&c, 0).is_err());
    }

    #[test]
    fn borne_de_messages_simultanes() {
        let mut r = Reassembler::new();
        // Ouvre MAX_CONCURRENT_REASSEMBLIES réassemblages distincts (1 fragment
        // chacun d'un message à 2 fragments).
        for id in 0..MAX_CONCURRENT_REASSEMBLIES as u32 {
            let mut c = vec![FRAME_FRAG];
            c.extend_from_slice(&id.to_be_bytes());
            c.extend_from_slice(&2u16.to_be_bytes());
            c.extend_from_slice(&0u16.to_be_bytes());
            c.extend_from_slice(&[0u8; 10]);
            assert_eq!(r.accept(&c, 0).unwrap(), None);
        }
        assert_eq!(r.en_cours(), MAX_CONCURRENT_REASSEMBLIES);
        // Un réassemblage de plus est refusé.
        let mut c = vec![FRAME_FRAG];
        c.extend_from_slice(&999u32.to_be_bytes());
        c.extend_from_slice(&2u16.to_be_bytes());
        c.extend_from_slice(&0u16.to_be_bytes());
        c.extend_from_slice(&[0u8; 10]);
        assert!(r.accept(&c, 0).is_err());
    }

    #[test]
    fn borne_memoire_de_reassemblage() {
        let mut r = Reassembler::new();
        // Force le dépassement du plafond mémoire : quelques messages avec des
        // fragments proches du maximum. Chaque message annonce le nombre
        // MAXIMAL de fragments admissible pour ne jamais se compléter — rester
        // sous [`MAX_FRAGMENTS`] est ce qui garantit que c'est bien la borne
        // MÉMOIRE qui mord ici, et pas la borne de nombre de fragments.
        let mut sature = None;
        'saturation: for id in 0..MAX_CONCURRENT_REASSEMBLIES as u32 {
            for index in 0..300u16 {
                let mut c = vec![FRAME_FRAG];
                c.extend_from_slice(&id.to_be_bytes());
                c.extend_from_slice(&(MAX_FRAGMENTS as u16).to_be_bytes());
                c.extend_from_slice(&index.to_be_bytes());
                c.extend_from_slice(&vec![0u8; MAX_FRAGMENT_CHUNK]);
                if let Err(e) = r.accept(&c, 0) {
                    sature = Some(e);
                    break 'saturation;
                }
            }
        }
        let Some(TransportError::Reassembly(motif)) = sature else {
            panic!("le plafond mémoire aurait dû être atteint");
        };
        assert_eq!(motif, "mémoire de réassemblage saturée");
    }

    #[test]
    fn total_au_dela_du_maximum_est_rejete_avant_toute_allocation() {
        // Un `total` de 65 535 (valeur maximale du champ) faisait pré-allouer
        // 65 535 cases pour un fragment de trois octets : ~1,5 Mio par
        // datagramme, hors du compteur d'octets reçus. Aucun émetteur conforme
        // ne dépasse [`MAX_FRAGMENTS`], le format filaire est inchangé.
        let mut c = vec![FRAME_FRAG];
        c.extend_from_slice(&1u32.to_be_bytes());
        c.extend_from_slice(&u16::MAX.to_be_bytes());
        c.extend_from_slice(&0u16.to_be_bytes());
        c.extend_from_slice(&[1, 2, 3]);
        let mut r = Reassembler::new();
        assert!(r.accept(&c, 0).is_err());
        assert_eq!(r.en_cours(), 0, "aucune réservation n'a eu lieu");

        // Le plus grand message légitime reste, lui, parfaitement réassemblable.
        let plaintext = vec![4u8; MAX_MESSAGE_LEN];
        let mut id = 0;
        let cadres = frame(&plaintext, &mut id);
        assert!(cadres.len() <= MAX_FRAGMENTS);
        assert_eq!(reassemble(&cadres, 0), Some(plaintext));
    }
}
