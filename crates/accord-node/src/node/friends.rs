//! Amis : contacts, demandes, réponses, retrait, blocage et statut de
//! présence local (bloc `impl Node` du domaine `friends.*`).

use std::net::SocketAddr;

use accord_core::db::Contact;
use accord_core::{friends, peer_addr, presence};
use accord_crypto::{node_id_of, FriendCode};
use accord_proto::core_msg::{CoreMsg, CONTACT_STATE_ABSENT, CONTACT_STATE_BLOCKED};
use serde_json::json;

use crate::error::NodeError;
use crate::hex;
use crate::outbound::Outbound;

use super::{now_ms, Node};

/// Amis annoncés au plus dans une passe de rattrapage vers un appareil frère.
///
/// Généreux à dessein : ce plafond n'est pas là pour économiser du réseau mais
/// pour qu'un carnet aberrant ne produise pas une rafale sans fin. Un carnet
/// ordinaire passe entier, et l'appelant journalise le jour où la borne mord —
/// tronquer en silence rendrait l'appareil sourd à certains amis exactement
/// comme avant le correctif, sans que rien ne l'indique.
const MAX_ANNONCE_CARNET: usize = 512;

impl Node {
    /// Liste des contacts.
    pub fn contacts(&self) -> Result<Vec<Contact>, NodeError> {
        self.with_db(|db| Ok(db.contacts()?))
    }

    /// Prépare et route une demande d'ami vers une clé publique.
    pub fn friend_request(
        &self,
        peer_pubkey: &[u8; 32],
        display_name: &str,
    ) -> Result<(), NodeError> {
        let action = self.with_db(|db| {
            Ok(friends::request_friend(
                db,
                peer_pubkey,
                display_name,
                now_ms(),
            )?)
        })?;
        // Nom annoncé au pair : le pseudo de profil s'il est défini, sinon le
        // code ami (D-027).
        let my_name = match self.profile_name()? {
            Some(name) => name,
            None => FriendCode::of_pubkey(&self.identity.public_key()).display(),
        };
        let msg = match action {
            friends::OutgoingAction::SendRequest => CoreMsg::FriendRequest {
                display_name: my_name,
                message: String::new(),
                verify_phrase: None,
            },
            friends::OutgoingAction::SendAccept => CoreMsg::FriendResponse { accepted: true },
        };
        self.outbound.send(Outbound::Core {
            to: *peer_pubkey,
            msg: Box::new(msg),
        });
        // Demandes croisées : amitié établie, annoncer aussi notre pseudo.
        if action == friends::OutgoingAction::SendAccept {
            self.announce_profile_to(peer_pubkey)?;
            self.annoncer_ami(peer_pubkey, display_name, now_ms());
        }
        Ok(())
    }

    /// Répond à une demande entrante. Sur acceptation, annonce aussi notre
    /// pseudo au nouvel ami (D-027).
    pub fn friend_respond(&self, peer_pubkey: &[u8; 32], accept: bool) -> Result<(), NodeError> {
        self.with_db(|db| Ok(friends::respond_friend(db, peer_pubkey, accept)?))?;
        self.outbound.send(Outbound::Core {
            to: *peer_pubkey,
            msg: Box::new(CoreMsg::FriendResponse { accepted: accept }),
        });
        if accept {
            self.announce_profile_to(peer_pubkey)?;
            // Le nom est relu de la base : c'est celui que la demande entrante
            // y a inscrit, pas un paramètre que cet appel porterait.
            let nom = self.nom_du_contact(peer_pubkey);
            self.annoncer_ami(peer_pubkey, &nom, now_ms());
        }
        Ok(())
    }

    /// Removes an established friendship (distinct from a block): the contact
    /// disappears locally, DM history is kept, and the peer is notified
    /// best-effort with a `FriendRemove` wire message (session-authenticated,
    /// never queued offline). Emits `event.friend_removed` so every local UI
    /// client refreshes.
    pub fn friend_remove(&self, peer_pubkey: &[u8; 32]) -> Result<(), NodeError> {
        self.with_db(|db| Ok(friends::remove_friend(db, peer_pubkey)?))?;
        self.outbound.send(Outbound::Core {
            to: *peer_pubkey,
            msg: Box::new(CoreMsg::FriendRemove),
        });
        self.emit(
            "event.friend_removed",
            json!({ "peer": hex::encode(peer_pubkey) }),
        );
        Ok(())
    }

    /// Sets the local presence status (`friends.set_status`): persists it in
    /// the meta table then announces it to all confirmed friends (invisible
    /// is announced as plain offline). `custom`: `None` keeps the current
    /// text, an empty string clears it.
    pub fn set_own_presence(
        &self,
        status: presence::OwnStatus,
        custom: Option<&str>,
    ) -> Result<(), NodeError> {
        self.with_db(|db| Ok(presence::set_own_presence(db, status, custom)?))?;
        self.broadcast_presence(true)
    }

    /// Persisted local presence status (`friends.get_status`); defaults to
    /// online without custom text.
    pub fn own_presence(&self) -> Result<(presence::OwnStatus, Option<String>), NodeError> {
        self.with_db(|db| Ok(presence::own_presence(db)?))
    }

    /// Bloque un pair, et l'annonce aux autres machines du compte.
    pub fn friend_block(&self, peer_pubkey: &[u8; 32]) -> Result<(), NodeError> {
        let at_ms = now_ms();
        self.with_db(|db| Ok(friends::block(db, peer_pubkey, at_ms)?))?;
        self.annoncer_etat_contact(peer_pubkey, CONTACT_STATE_BLOCKED, at_ms);
        Ok(())
    }

    /// Débloque un pair, et l'annonce aux autres machines du compte.
    pub fn friend_unblock(&self, peer_pubkey: &[u8; 32]) -> Result<(), NodeError> {
        let at_ms = now_ms();
        self.with_db(|db| Ok(friends::unblock(db, peer_pubkey)?))?;
        self.annoncer_etat_contact(peer_pubkey, CONTACT_STATE_ABSENT, at_ms);
        Ok(())
    }

    /// Annonce aux AUTRES appareils du compte qu'un contact a changé d'état.
    ///
    /// 🔒 Sans effet observable en cas d'échec, et c'est voulu : le blocage
    /// local a déjà pris. Faire échouer `friend_block` parce que l'annonce n'a
    /// pas pu partir laisserait l'utilisateur devant une erreur alors que la
    /// protection qu'il demandait est en place sur la machine qu'il regarde.
    /// L'autre machine rattrapera au prochain blocage, ou restera en retard —
    /// c'est écrit dans `SECURITY.md`.
    fn annoncer_etat_contact(&self, peer_pubkey: &[u8; 32], state: u8, at_ms: u64) {
        self.outbound.send(Outbound::Core {
            // Adressé au COMPTE : la couche réseau développe en un envoi par
            // appareil joignable. Nous pouvons y figurer nous-mêmes — sans
            // conséquence, l'application est idempotente.
            to: self.public_key(),
            msg: Box::new(CoreMsg::SelfContactState {
                peer: *peer_pubkey,
                state,
                at_ms,
            }),
        });
    }

    /// Mémorise la dernière adresse directe connue d'un pair (carnet
    /// PERSISTANT, cf. [`peer_addr`]). Best-effort : appelé à chaque session
    /// établie avec un ami pour permettre une reconnexion rapide au prochain
    /// démarrage, avant la résolution DHT.
    pub fn remember_peer_addr(&self, node_id: [u8; 32], addr: SocketAddr) -> Result<(), NodeError> {
        self.with_db(|db| Ok(peer_addr::remember(db, &node_id, addr, now_ms())?))
    }

    /// Vrai si `pubkey` est une RELATION : ami confirmé ou demande en cours
    /// (entrante ou sortante). Périmètre de la persistance d'adresse : une
    /// session s'établit souvent AVANT la conclusion de l'amitié — n'écrire
    /// l'adresse que pour les amis raterait exactement cette fenêtre (l'entrée
    /// d'un contact en attente ne devient lisible par [`known_friend_addrs`]
    /// qu'une fois l'amitié conclue ; un bloqué ou un inconnu n'écrit rien).
    pub fn is_relation(&self, pubkey: &[u8; 32]) -> bool {
        use accord_core::db::ContactState;
        self.contacts()
            .map(|cs| {
                cs.iter().any(|c| {
                    c.pubkey == *pubkey
                        && matches!(
                            c.state,
                            ContactState::Friend
                                | ContactState::PendingIn
                                | ContactState::PendingOut
                        )
                })
            })
            .unwrap_or(false)
    }

    /// Amis dont une adresse directe fraîche (TTL par défaut) est mémorisée,
    /// pour un dial immédiat au démarrage. Une entrée périmée ou corrompue est
    /// silencieusement omise (la DHT reprend la main).
    pub fn known_friend_addrs(&self) -> Result<Vec<([u8; 32], SocketAddr)>, NodeError> {
        let friends = self.friend_pubkeys()?;
        self.with_db(|db| {
            let now = now_ms();
            let mut out = Vec::with_capacity(friends.len());
            for pk in friends {
                if let Some(addr) = peer_addr::recall(db, &pk, now, peer_addr::DEFAULT_TTL_MS)? {
                    out.push((pk, addr));
                }
            }
            Ok(out)
        })
    }

    /// Pseudo enregistré pour `peer_pubkey`, ou le code ami à défaut.
    ///
    /// Sans voie de panique : un carnet illisible rend le code ami, qui est
    /// toujours vrai et n'exige aucune base — annoncer une amitié sous un nom
    /// approximatif vaut mieux que ne pas l'annoncer, puisque c'est l'amitié,
    /// pas le nom, qui rend l'autre appareil audible.
    pub(super) fn nom_du_contact(&self, peer_pubkey: &[u8; 32]) -> String {
        self.with_db(|db| Ok(db.contact(&node_id_of(peer_pubkey).0)?))
            .ok()
            .flatten()
            .map(|c| c.display_name)
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| FriendCode::of_pubkey(peer_pubkey).display())
    }

    /// Annonce UNE amitié aux autres appareils du compte.
    ///
    /// 🔴 Sans cela, un appareil fraîchement appairé reste **sourd** : son
    /// carnet est vide, et `accord_core::messaging::ingest_dm` jette tout
    /// message d'un pair qui n'y figure pas. Voir [`CoreMsg::SelfContactAdd`].
    ///
    /// Sans effet observable en cas d'échec, pour la même raison que
    /// [`Node::annoncer_etat_contact`] : l'amitié est déjà nouée ici, et faire
    /// échouer l'action de l'utilisateur pour une annonce qui n'est pas partie
    /// signalerait un problème là où ce qu'il demandait est en place.
    pub(super) fn annoncer_ami(&self, peer_pubkey: &[u8; 32], display_name: &str, added_ms: u64) {
        self.outbound.send(Outbound::Core {
            // Adressé au COMPTE : la couche réseau développe en un envoi par
            // appareil joignable, et nous y figurons — sans conséquence,
            // l'application ne crée que ce qui manque.
            to: self.public_key(),
            msg: Box::new(CoreMsg::SelfContactAdd {
                peer: *peer_pubkey,
                display_name: display_name.to_string(),
                added_ms,
            }),
        });
    }

    /// Annonce TOUT le carnet à un appareil frère qui vient de devenir
    /// joignable — le chemin de rattrapage, quand l'annonce unitaire ci-dessus
    /// est passée pendant qu'il était éteint (ou avant qu'il existe).
    ///
    /// ⚠️ Borné à [`MAX_ANNONCE_CARNET`], et l'appelant journalise si la borne
    /// mord : un carnet tronqué en silence laisserait un appareil sourd à
    /// certains amis sans que rien ne le dise, ce qui est exactement la panne
    /// que ce message existe pour supprimer.
    pub fn self_contact_msgs(&self) -> Result<(Vec<CoreMsg>, usize), NodeError> {
        self.with_db(|db| {
            let amis: Vec<_> = db
                .contacts()?
                .into_iter()
                .filter(|c| c.state == accord_core::db::ContactState::Friend)
                .collect();
            let total = amis.len();
            Ok((
                amis.into_iter()
                    .take(MAX_ANNONCE_CARNET)
                    .map(|c| CoreMsg::SelfContactAdd {
                        peer: c.pubkey,
                        display_name: c.display_name,
                        added_ms: c.added_ms,
                    })
                    .collect(),
                total,
            ))
        })
    }

    /// Clés publiques des amis confirmés (présence, relève des boîtes).
    pub fn friend_pubkeys(&self) -> Result<Vec<[u8; 32]>, NodeError> {
        self.with_db(|db| {
            Ok(db
                .contacts()?
                .into_iter()
                .filter(|c| c.state == accord_core::db::ContactState::Friend)
                .map(|c| c.pubkey)
                .collect())
        })
    }

    /// Clés publiques des demandes d'ami SORTANTES en attente : cibles de la
    /// résolution de présence au même titre que les amis (premier contact —
    /// le destinataire n'est pas encore ami mais doit pouvoir être joint pour
    /// que la demande parte, directement ou via ses relais domicile).
    pub fn pending_out_pubkeys(&self) -> Result<Vec<[u8; 32]>, NodeError> {
        self.with_db(|db| {
            Ok(db
                .contacts()?
                .into_iter()
                .filter(|c| c.state == accord_core::db::ContactState::PendingOut)
                .map(|c| c.pubkey)
                .collect())
        })
    }

    /// Écrit la note privée locale attachée à une clé publique (au plus
    /// [`MAX_NOTE_CHARS`] caractères ; une note vide efface l'entrée). Purement
    /// locale : jamais émise vers le pair ni ailleurs.
    pub fn set_contact_note(&self, pubkey: &[u8; 32], note: &str) -> Result<(), NodeError> {
        let trimmed = note.trim();
        if trimmed.chars().count() > MAX_NOTE_CHARS {
            return Err(NodeError::Invalid("note trop longue (max 4096 caractères)"));
        }
        self.with_db(|db| Ok(db.set_contact_note(pubkey, trimmed)?))
    }

    /// Lit la note privée locale d'une clé publique (`None` si aucune).
    pub fn contact_note(&self, pubkey: &[u8; 32]) -> Result<Option<String>, NodeError> {
        self.with_db(|db| Ok(db.contact_note(pubkey)?))
    }

    // ---- Safety numbers (Lot E1, local-only, no wire byte) ----

    /// Safety number of the conversation with `peer_pubkey`, plus the local
    /// verification state: `(number, verified, key_changed)`. `key_changed`
    /// is true when the contact was verified against a different public key
    /// than the current one ("verification broken").
    pub fn friend_safety_number(
        &self,
        peer_pubkey: &[u8; 32],
    ) -> Result<(accord_crypto::SafetyNumber, bool, bool), NodeError> {
        let number = accord_crypto::safety_number(&self.identity.public_key(), peer_pubkey);
        let contact = self.with_db(|db| Ok(db.contact(&node_id_of(peer_pubkey).0)?))?;
        let (verified, key_changed) = verification_state(contact.as_ref());
        Ok((number, verified, key_changed))
    }

    /// Test fixture: rewrites the pubkey stored at verification time, to
    /// model a key substitution AFTER verification without simulating a
    /// whole re-resolution flow. `#[cfg(test)]` only — never in a real
    /// binary.
    #[cfg(test)]
    pub(crate) fn test_force_verified_pubkey(
        &self,
        peer_pubkey: &[u8; 32],
        seen: &[u8; 32],
    ) -> Result<(), NodeError> {
        self.with_db(|db| {
            Ok(db.set_contact_verified(&node_id_of(peer_pubkey).0, Some((now_ms(), *seen)))?)
        })
    }

    /// Marks (or unmarks) the contact as manually verified. The public key
    /// seen NOW is stored with the flag so a later key substitution is
    /// detectable. Emits `event.friend_verified` for local UI refresh.
    pub fn friend_set_verified(
        &self,
        peer_pubkey: &[u8; 32],
        verified: bool,
    ) -> Result<(), NodeError> {
        let verification = verified.then(|| (now_ms(), *peer_pubkey));
        self.with_db(|db| Ok(db.set_contact_verified(&node_id_of(peer_pubkey).0, verification)?))?;
        self.emit(
            "event.friend_verified",
            json!({ "peer": hex::encode(peer_pubkey), "verified": verified }),
        );
        Ok(())
    }
}

/// Verification state of a contact: `(verified, key_changed)`.
pub(crate) fn verification_state(contact: Option<&Contact>) -> (bool, bool) {
    match contact.and_then(|c| c.verified_pubkey.map(|vp| (c.pubkey, vp))) {
        Some((current, seen)) => (true, current != seen),
        None => (false, false),
    }
}

/// Longueur maximale d'une note privée de contact (caractères Unicode).
const MAX_NOTE_CHARS: usize = 4096;

#[cfg(test)]
mod tests {
    use accord_core::db::Db;
    use accord_core::presence::OwnStatus;
    use accord_crypto::Identity;
    use tokio::sync::mpsc;

    use super::*;
    use crate::outbound::OutboundSink;

    /// Node wired to an outbound channel, with one established friend.
    fn node_with_friend() -> (Node, [u8; 32], mpsc::Receiver<Outbound>) {
        let id = Identity::generate_with_pow_bits(1);
        let db = Db::open_in_memory(&[1u8; 32]).unwrap();
        let (sink, mut rx) = OutboundSink::channel(64);
        let node = Node::new(id, db, sink);
        let peer = Identity::generate_with_pow_bits(1);
        node.friend_request(&peer.public_key(), "Pair").unwrap();
        node.ingest_core(
            &peer.public_key(),
            CoreMsg::FriendResponse { accepted: true },
        )
        .unwrap();
        while rx.try_recv().is_ok() {}
        (node, peer.public_key(), rx)
    }

    #[test]
    fn known_friend_addrs_rend_les_amis_avec_adresse_fraiche() {
        let (node, peer, _rx) = node_with_friend();
        let addr: SocketAddr = "203.0.113.7:48016".parse().unwrap();
        node.remember_peer_addr(peer, addr).unwrap();
        let known = node.known_friend_addrs().unwrap();
        assert_eq!(known, vec![(peer, addr)]);
    }

    #[test]
    fn known_friend_addrs_ignore_les_non_amis() {
        let (node, _peer, _rx) = node_with_friend();
        let stranger = Identity::generate_with_pow_bits(1).public_key();
        let addr: SocketAddr = "203.0.113.9:48016".parse().unwrap();
        node.remember_peer_addr(stranger, addr).unwrap();
        assert!(node.known_friend_addrs().unwrap().is_empty());
    }

    /// Next `CoreMsg` pushed on the outbound channel, with its recipient.
    fn next_core(rx: &mut mpsc::Receiver<Outbound>) -> Option<([u8; 32], CoreMsg)> {
        while let Ok(action) = rx.try_recv() {
            if let Outbound::Core { to, msg } = action {
                return Some((to, *msg));
            }
        }
        None
    }

    #[test]
    fn friend_remove_drops_contact_and_notifies_peer() {
        let (node, peer, mut rx) = node_with_friend();
        node.friend_remove(&peer).unwrap();
        assert!(node.contacts().unwrap().is_empty());
        let (to, msg) = next_core(&mut rx).expect("notification attendue");
        assert_eq!(to, peer);
        assert_eq!(msg, CoreMsg::FriendRemove);
        // Not a friend anymore: a second removal is refused.
        assert!(node.friend_remove(&peer).is_err());
    }

    #[test]
    fn friend_remove_keeps_dm_history() {
        let (node, peer, _rx) = node_with_friend();
        node.dm_send(&peer, "avant retrait", None).unwrap();
        node.friend_remove(&peer).unwrap();
        assert_eq!(node.dm_history(&peer, u64::MAX, 10).unwrap().len(), 1);
        // Sending to a removed friend fails exactly like any non-friend.
        assert!(node.dm_send(&peer, "après retrait", None).is_err());
    }

    #[test]
    fn ingested_friend_remove_drops_friendship_only() {
        let (node, peer, _rx) = node_with_friend();
        let replies = node.ingest_core(&peer, CoreMsg::FriendRemove).unwrap();
        assert!(replies.is_empty());
        assert!(node.contacts().unwrap().is_empty());
        // Replay: idempotent, still no reply.
        assert!(node
            .ingest_core(&peer, CoreMsg::FriendRemove)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn own_presence_persists_and_broadcasts_status() {
        let (node, peer, mut rx) = node_with_friend();
        node.set_own_presence(OwnStatus::Dnd, Some("focus"))
            .unwrap();
        assert_eq!(
            node.own_presence().unwrap(),
            (OwnStatus::Dnd, Some("focus".into()))
        );
        let (to, msg) = next_core(&mut rx).expect("annonce attendue");
        assert_eq!(to, peer);
        assert_eq!(
            msg,
            CoreMsg::Presence {
                status: 2,
                custom: Some("focus".into())
            }
        );
    }

    #[test]
    fn invisible_broadcasts_offline_without_custom_text() {
        let (node, _peer, mut rx) = node_with_friend();
        node.set_own_presence(OwnStatus::Invisible, Some("caché"))
            .unwrap();
        let (_, msg) = next_core(&mut rx).expect("annonce attendue");
        assert_eq!(
            msg,
            CoreMsg::Presence {
                status: 3,
                custom: None
            }
        );
        // The status (and its text) stay persisted locally.
        assert_eq!(
            node.own_presence().unwrap(),
            (OwnStatus::Invisible, Some("caché".into()))
        );
        // A clean-shutdown broadcast stays offline too.
        node.broadcast_presence(false).unwrap();
        let (_, msg) = next_core(&mut rx).expect("annonce d'arrêt attendue");
        assert!(matches!(msg, CoreMsg::Presence { status: 3, .. }));
    }

    #[test]
    fn rich_presence_from_friend_is_tracked_and_cleared() {
        let (node, peer, _rx) = node_with_friend();
        node.ingest_core(
            &peer,
            CoreMsg::Presence {
                status: 1,
                custom: Some("afk".into()),
            },
        )
        .unwrap();
        assert_eq!(node.peer_presence(&peer), (1, Some("afk".into())));
        assert!(node.is_online(&peer));
        // Backward compatibility: a bare offline announcement clears all.
        node.ingest_core(
            &peer,
            CoreMsg::Presence {
                status: 3,
                custom: None,
            },
        )
        .unwrap();
        assert_eq!(node.peer_presence(&peer), (3, None));
        assert!(!node.is_online(&peer));
    }

    #[test]
    fn plain_reachability_does_not_override_explicit_status() {
        let (node, peer, _rx) = node_with_friend();
        node.ingest_core(
            &peer,
            CoreMsg::Presence {
                status: 2,
                custom: None,
            },
        )
        .unwrap();
        // Any later message keeps the explicit do-not-disturb status.
        node.ingest_core(
            &peer,
            CoreMsg::Profile {
                display_name: "Pair".into(),
                bio: String::new(),
                avatar: None,
                banner: None,
                pronouns: None,
                accent_color: None,
                banner_color: None,
                avatar_decoration: None,
                profile_effect: None,
                profile_frame: None,
            },
        )
        .unwrap();
        assert_eq!(node.peer_presence(&peer), (2, None));
    }

    #[test]
    fn rich_presence_from_non_friend_only_tracks_reachability() {
        let (node, _peer, _rx) = node_with_friend();
        let stranger = Identity::generate_with_pow_bits(1);
        node.ingest_core(
            &stranger.public_key(),
            CoreMsg::Presence {
                status: 2,
                custom: Some("spam".into()),
            },
        )
        .unwrap();
        // Reachable, but no rich status is stored for strangers.
        assert!(node.is_online(&stranger.public_key()));
        assert_eq!(node.peer_presence(&stranger.public_key()), (0, None));
    }
}
