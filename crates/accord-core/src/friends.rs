//! Codes amis et machine à états des demandes d'ami (SPEC §5).
//!
//! Résolution : un code ami (64 bits + mots BIP39) pointe vers un record DHT
//! d'identité auto-publié `payload(8) ‖ pubkey(32)`. La vérification croisée
//! (code ↔ payload ↔ clé ↔ publieur ↔ signature) empêche un tiers de faire
//! résoudre le code de quelqu'un d'autre vers sa propre clé.
//!
//! États d'un contact : `PendingOut` (demande envoyée), `PendingIn` (demande
//! reçue), `Friend`, `Blocked`. Les demandes croisées s'auto-acceptent ; un
//! contact bloqué est ignoré silencieusement (aucune réponse observable).

use accord_crypto::{node_id_of, verify_signature, FriendCode, Identity, FRIENDCODE_PAYLOAD_LEN};
use accord_proto::types::{DhtRecord, RecordKind};

use crate::db::{Contact, ContactState, Db};
use crate::error::CoreError;

/// Longueur d'un record d'identité : `payload(8) ‖ pubkey(32)`.
const IDENTITY_VALUE_LEN: usize = FRIENDCODE_PAYLOAD_LEN + 32;
/// Longueur maximale d'un pseudo affiché, en octets : couvre un pseudo de
/// profil de 32 caractères × 4 octets UTF-8 (D-027).
const MAX_DISPLAY_NAME: usize = 128;
/// Durée de vie d'un record d'identité publié (1 h, republication continue).
const IDENTITY_EXPIRY_S: u32 = 3600;

/// Action à entreprendre après une demande sortante.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutgoingAction {
    /// Envoyer `CoreMsg::FriendRequest` au pair.
    SendRequest,
    /// Demandes croisées : amitié établie, envoyer
    /// `CoreMsg::FriendResponse { accepted: true }`.
    SendAccept,
}

/// Issue de l'ingestion d'une demande d'ami entrante.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncomingOutcome {
    /// Demande enregistrée, en attente de décision de l'utilisateur.
    Pending,
    /// Demandes croisées : amitié établie, répondre `accepted = true`.
    AutoAccepted,
    /// Déjà amis : répondre `accepted = true` (idempotence).
    AlreadyFriend,
    /// Expéditeur bloqué : ne rien répondre.
    Ignored,
}

/// Construit le record DHT d'identité de l'utilisateur local (à republier
/// périodiquement pour rester résoluble par code ami).
pub fn identity_record(identity: &Identity, now_ms: u64) -> DhtRecord {
    let code = FriendCode::of_pubkey(&identity.public_key());
    let mut value = code.payload().to_vec();
    value.extend_from_slice(&identity.public_key());
    let mut record = DhtRecord {
        key: code.dht_key(),
        kind: RecordKind::Identity,
        value,
        publisher: identity.public_key(),
        timestamp_ms: now_ms,
        expiry_s: IDENTITY_EXPIRY_S,
        sig: [0u8; 64],
    };
    record.sig = identity.sign(&record.signable_bytes());
    record
}

/// Vérifie qu'un record DHT résout bien `code` et rend la clé publique du
/// titulaire. Toutes les liaisons sont contrôlées : clé DHT, payload, code,
/// publieur et signature.
pub fn verify_identity_record(
    code: &FriendCode,
    record: &DhtRecord,
) -> Result<[u8; 32], CoreError> {
    if record.kind != RecordKind::Identity {
        return Err(CoreError::Invalid("record d'identité de nature inattendue"));
    }
    if record.key != code.dht_key() {
        return Err(CoreError::Invalid("clé DHT sans rapport avec le code ami"));
    }
    if record.value.len() != IDENTITY_VALUE_LEN
        || record.value[..FRIENDCODE_PAYLOAD_LEN] != *code.payload()
    {
        return Err(CoreError::Invalid("payload du record d'identité invalide"));
    }
    let mut pubkey = [0u8; 32];
    pubkey.copy_from_slice(&record.value[FRIENDCODE_PAYLOAD_LEN..]);
    if !code.matches_pubkey(&pubkey) {
        return Err(CoreError::Invalid("clé publique sans rapport avec le code"));
    }
    if record.publisher != pubkey {
        return Err(CoreError::Invalid("record d'identité non auto-publié"));
    }
    verify_signature(&record.publisher, &record.signable_bytes(), &record.sig)?;
    Ok(pubkey)
}

/// Valide un pseudo affiché (fourni par un pair, non authentifié).
fn validate_display_name(name: &str) -> Result<(), CoreError> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_DISPLAY_NAME {
        return Err(CoreError::Invalid("pseudo vide ou trop long"));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(CoreError::Invalid("pseudo avec caractères de contrôle"));
    }
    Ok(())
}

fn new_contact(pubkey: [u8; 32], display_name: &str, state: ContactState, now_ms: u64) -> Contact {
    Contact {
        node_id: node_id_of(&pubkey).0,
        pubkey,
        display_name: display_name.trim().to_string(),
        state,
        added_ms: now_ms,
        last_seen_ms: now_ms,
    }
}

/// Prépare une demande d'ami sortante vers une clé résolue par code ami.
///
/// Refuse si le pair est bloqué ou déjà ami ; si une demande entrante de ce
/// pair est en attente, l'amitié est établie directement (demandes croisées).
pub fn request_friend(
    db: &Db,
    peer_pubkey: &[u8; 32],
    display_name: &str,
    now_ms: u64,
) -> Result<OutgoingAction, CoreError> {
    validate_display_name(display_name)?;
    let node_id = node_id_of(peer_pubkey).0;
    match db.contact(&node_id)?.map(|c| c.state) {
        Some(ContactState::Blocked) => Err(CoreError::OpRejected("contact bloqué")),
        Some(ContactState::Friend) => Err(CoreError::OpRejected("déjà ami")),
        Some(ContactState::PendingOut) => Ok(OutgoingAction::SendRequest),
        Some(ContactState::PendingIn) => {
            db.set_contact_state(&node_id, ContactState::Friend)?;
            Ok(OutgoingAction::SendAccept)
        }
        None => {
            db.upsert_contact(&new_contact(
                *peer_pubkey,
                display_name,
                ContactState::PendingOut,
                now_ms,
            ))?;
            Ok(OutgoingAction::SendRequest)
        }
    }
}

/// Ingère une demande d'ami entrante (après authentification transport du
/// pair : `peer_pubkey` est la clé de la session chiffrée, pas un champ).
pub fn ingest_friend_request(
    db: &Db,
    peer_pubkey: &[u8; 32],
    display_name: &str,
    now_ms: u64,
) -> Result<IncomingOutcome, CoreError> {
    validate_display_name(display_name)?;
    let node_id = node_id_of(peer_pubkey).0;
    match db.contact(&node_id)?.map(|c| c.state) {
        Some(ContactState::Blocked) => Ok(IncomingOutcome::Ignored),
        Some(ContactState::Friend) => {
            db.touch_contact(&node_id, now_ms)?;
            Ok(IncomingOutcome::AlreadyFriend)
        }
        Some(ContactState::PendingOut) => {
            db.set_contact_state(&node_id, ContactState::Friend)?;
            db.touch_contact(&node_id, now_ms)?;
            Ok(IncomingOutcome::AutoAccepted)
        }
        Some(ContactState::PendingIn) => Ok(IncomingOutcome::Pending),
        None => {
            db.upsert_contact(&new_contact(
                *peer_pubkey,
                display_name,
                ContactState::PendingIn,
                now_ms,
            ))?;
            Ok(IncomingOutcome::Pending)
        }
    }
}

/// Répond à une demande entrante. `accept = false` efface le contact
/// (une nouvelle demande restera possible).
pub fn respond_friend(db: &Db, peer_pubkey: &[u8; 32], accept: bool) -> Result<(), CoreError> {
    let node_id = node_id_of(peer_pubkey).0;
    let contact = db
        .contact(&node_id)?
        .ok_or(CoreError::NotFound("aucune demande de ce pair"))?;
    if contact.state != ContactState::PendingIn {
        return Err(CoreError::OpRejected("aucune demande entrante en attente"));
    }
    if accept {
        db.set_contact_state(&node_id, ContactState::Friend)
    } else {
        db.remove_contact(&node_id)
    }
}

/// Ingère la réponse du pair à notre demande sortante. Rend `true` si
/// l'amitié est établie.
pub fn ingest_friend_response(
    db: &Db,
    peer_pubkey: &[u8; 32],
    accepted: bool,
    now_ms: u64,
) -> Result<bool, CoreError> {
    let node_id = node_id_of(peer_pubkey).0;
    let Some(contact) = db.contact(&node_id)? else {
        return Ok(false); // réponse non sollicitée : ignorée
    };
    match contact.state {
        ContactState::PendingOut => {
            if accepted {
                db.set_contact_state(&node_id, ContactState::Friend)?;
                db.touch_contact(&node_id, now_ms)?;
            } else {
                db.remove_contact(&node_id)?;
            }
            Ok(accepted)
        }
        // Demandes croisées déjà auto-acceptées, ou réponse rejouée.
        ContactState::Friend => Ok(accepted),
        _ => Ok(false),
    }
}

/// Removes an established friendship on our side (distinct from a block: the
/// peer is neither blocked nor prevented from sending a new friend request).
/// DM history is untouched — only the contact entry disappears.
pub fn remove_friend(db: &Db, peer_pubkey: &[u8; 32]) -> Result<(), CoreError> {
    let node_id = node_id_of(peer_pubkey).0;
    match db.contact(&node_id)?.map(|c| c.state) {
        Some(ContactState::Friend) => db.remove_contact(&node_id),
        Some(_) => Err(CoreError::OpRejected("contact non ami")),
        None => Err(CoreError::NotFound("contact inconnu")),
    }
}

/// Ingests a peer-side friendship removal (`CoreMsg::FriendRemove`, sender
/// authenticated by the encrypted session). Returns `true` if a friendship
/// was actually removed; any other contact state (pending, blocked, unknown)
/// is left untouched — a stranger cannot mutate our contact list.
pub fn ingest_friend_remove(db: &Db, peer_pubkey: &[u8; 32]) -> Result<bool, CoreError> {
    let node_id = node_id_of(peer_pubkey).0;
    match db.contact(&node_id)?.map(|c| c.state) {
        Some(ContactState::Friend) => {
            db.remove_contact(&node_id)?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

/// Bloque un pair (existant ou non) : plus aucune demande ni message accepté.
pub fn block(db: &Db, peer_pubkey: &[u8; 32], now_ms: u64) -> Result<(), CoreError> {
    let node_id = node_id_of(peer_pubkey).0;
    match db.contact(&node_id)? {
        Some(_) => db.set_contact_state(&node_id, ContactState::Blocked),
        None => db.upsert_contact(&new_contact(
            *peer_pubkey,
            "bloqué",
            ContactState::Blocked,
            now_ms,
        )),
    }
}

/// Débloque un pair en effaçant le contact.
pub fn unblock(db: &Db, peer_pubkey: &[u8; 32]) -> Result<(), CoreError> {
    let node_id = node_id_of(peer_pubkey).0;
    match db.contact(&node_id)? {
        Some(c) if c.state == ContactState::Blocked => db.remove_contact(&node_id),
        Some(_) => Err(CoreError::OpRejected("contact non bloqué")),
        None => Err(CoreError::NotFound("contact inconnu")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (Db, Identity, Identity) {
        (
            Db::open_in_memory(&[5u8; 32]).unwrap(),
            Identity::generate_with_pow_bits(1),
            Identity::generate_with_pow_bits(1),
        )
    }

    #[test]
    fn identity_record_verifies_and_rejects_substitution() {
        let (_, alice, mallory) = setup();
        let code = FriendCode::of_pubkey(&alice.public_key());
        let record = identity_record(&alice, 1_000);
        assert_eq!(
            verify_identity_record(&code, &record).unwrap(),
            alice.public_key()
        );

        // Mallory republie le record d'Alice avec sa propre clé en valeur.
        let mut forged = record.clone();
        forged.value[FRIENDCODE_PAYLOAD_LEN..].copy_from_slice(&mallory.public_key());
        forged.sig = mallory.sign(&forged.signable_bytes());
        assert!(verify_identity_record(&code, &forged).is_err());

        // Mauvais code pour un record valide : refus aussi.
        let other_code = FriendCode::of_pubkey(&mallory.public_key());
        if other_code != code {
            assert!(verify_identity_record(&other_code, &record).is_err());
        }

        // Signature altérée.
        let mut bad_sig = record.clone();
        bad_sig.sig[0] ^= 1;
        assert!(verify_identity_record(&code, &bad_sig).is_err());
    }

    #[test]
    fn request_then_accept_establishes_friendship() {
        let (db, _, bob) = setup();
        let action = request_friend(&db, &bob.public_key(), "Bob", 1).unwrap();
        assert_eq!(action, OutgoingAction::SendRequest);
        assert!(ingest_friend_response(&db, &bob.public_key(), true, 2).unwrap());
        let contact = db
            .contact(&node_id_of(&bob.public_key()).0)
            .unwrap()
            .unwrap();
        assert_eq!(contact.state, ContactState::Friend);
    }

    #[test]
    fn refusal_erases_contact_and_allows_retry() {
        let (db, _, bob) = setup();
        request_friend(&db, &bob.public_key(), "Bob", 1).unwrap();
        assert!(!ingest_friend_response(&db, &bob.public_key(), false, 2).unwrap());
        assert!(db
            .contact(&node_id_of(&bob.public_key()).0)
            .unwrap()
            .is_none());
        // Nouvelle tentative possible.
        assert_eq!(
            request_friend(&db, &bob.public_key(), "Bob", 3).unwrap(),
            OutgoingAction::SendRequest
        );
    }

    #[test]
    fn crossed_requests_auto_accept() {
        let (db, _, bob) = setup();
        request_friend(&db, &bob.public_key(), "Bob", 1).unwrap();
        // Bob nous demande aussi : auto-acceptation.
        assert_eq!(
            ingest_friend_request(&db, &bob.public_key(), "Bob", 2).unwrap(),
            IncomingOutcome::AutoAccepted
        );
        let contact = db
            .contact(&node_id_of(&bob.public_key()).0)
            .unwrap()
            .unwrap();
        assert_eq!(contact.state, ContactState::Friend);
    }

    #[test]
    fn incoming_request_then_user_decision() {
        let (db, _, bob) = setup();
        assert_eq!(
            ingest_friend_request(&db, &bob.public_key(), "Bob", 1).unwrap(),
            IncomingOutcome::Pending
        );
        // Rejouée : toujours en attente, pas de doublon.
        assert_eq!(
            ingest_friend_request(&db, &bob.public_key(), "Bob", 2).unwrap(),
            IncomingOutcome::Pending
        );
        respond_friend(&db, &bob.public_key(), true).unwrap();
        assert_eq!(
            ingest_friend_request(&db, &bob.public_key(), "Bob", 3).unwrap(),
            IncomingOutcome::AlreadyFriend
        );
    }

    #[test]
    fn blocked_peer_is_silently_ignored() {
        let (db, _, bob) = setup();
        block(&db, &bob.public_key(), 1).unwrap();
        assert_eq!(
            ingest_friend_request(&db, &bob.public_key(), "Bob", 2).unwrap(),
            IncomingOutcome::Ignored
        );
        assert!(matches!(
            request_friend(&db, &bob.public_key(), "Bob", 3),
            Err(CoreError::OpRejected(_))
        ));
        unblock(&db, &bob.public_key()).unwrap();
        assert_eq!(
            ingest_friend_request(&db, &bob.public_key(), "Bob", 4).unwrap(),
            IncomingOutcome::Pending
        );
    }

    #[test]
    fn remove_friend_erases_contact_and_allows_new_request() {
        let (db, _, bob) = setup();
        request_friend(&db, &bob.public_key(), "Bob", 1).unwrap();
        ingest_friend_response(&db, &bob.public_key(), true, 2).unwrap();

        remove_friend(&db, &bob.public_key()).unwrap();
        assert!(db
            .contact(&node_id_of(&bob.public_key()).0)
            .unwrap()
            .is_none());
        // Unlike a block, a fresh friend request stays possible.
        assert_eq!(
            request_friend(&db, &bob.public_key(), "Bob", 3).unwrap(),
            OutgoingAction::SendRequest
        );
    }

    #[test]
    fn remove_friend_rejects_non_friend_states() {
        let (db, _, bob) = setup();
        assert!(matches!(
            remove_friend(&db, &bob.public_key()),
            Err(CoreError::NotFound(_))
        ));
        block(&db, &bob.public_key(), 1).unwrap();
        assert!(matches!(
            remove_friend(&db, &bob.public_key()),
            Err(CoreError::OpRejected(_))
        ));
    }

    #[test]
    fn ingest_friend_remove_only_drops_established_friendships() {
        let (db, _, bob) = setup();
        // Unknown peer: nothing to remove.
        assert!(!ingest_friend_remove(&db, &bob.public_key()).unwrap());
        // Blocked peer: the block survives a removal attempt.
        block(&db, &bob.public_key(), 1).unwrap();
        assert!(!ingest_friend_remove(&db, &bob.public_key()).unwrap());
        unblock(&db, &bob.public_key()).unwrap();
        // Established friendship: removed on ingestion.
        request_friend(&db, &bob.public_key(), "Bob", 2).unwrap();
        ingest_friend_response(&db, &bob.public_key(), true, 3).unwrap();
        assert!(ingest_friend_remove(&db, &bob.public_key()).unwrap());
        assert!(db
            .contact(&node_id_of(&bob.public_key()).0)
            .unwrap()
            .is_none());
    }

    #[test]
    fn display_names_are_validated() {
        let (db, _, bob) = setup();
        assert!(request_friend(&db, &bob.public_key(), "  ", 1).is_err());
        assert!(request_friend(&db, &bob.public_key(), "a\u{0007}b", 1).is_err());
        let long = "x".repeat(129);
        assert!(request_friend(&db, &bob.public_key(), &long, 1).is_err());
    }
}
