//! Protocole d'invitation à consentement explicite (D-045).
//!
//! Remplace l'ancien force-join (`AddMember` + op-log complet + clé poussés
//! sans demande ni accord) par un aller-retour en deux temps :
//!
//! 1. L'inviteur autorise une invitation (op `InviteCreate`, déjà répliquée
//!    et vérifiée au repli, [`super::state::GroupState::apply`]) puis envoie
//!    un `CoreMsg::InviteTicket` signé à UN invité précis (transport
//!    point-à-point, jamais répliqué).
//! 2. L'invité vérifie la signature du ticket, le stocke localement en
//!    attente, et — seulement si l'utilisateur accepte explicitement — répond
//!    par `CoreMsg::InviteAccept` prouvant qu'il détient le secret. L'inviteur
//!    ré-vérifie alors l'invitation (droits, expiration, secret) avant
//!    d'admettre le membre et de lui pousser l'op-log et la clé de groupe.
//!
//! La porte de consentement côté invité (matérialisation locale non
//! répliquée, [`crate::db::LocalMembership`]) est appliquée par l'appelant
//! réseau (`accord-node`) ; ce module ne fait que fournir les primitives
//! cryptographiques et l'autorité de décision côté inviteur.

use accord_crypto::{verify_signature, Identity};
use accord_proto::core_msg::{invite_ticket_signable_bytes, perms, CoreMsg, GroupOp, GroupOpBody};
use rand::RngCore;
use sha2::{Digest, Sha256};

use crate::db::Db;
use crate::error::CoreError;

use super::crypt::SEALED_KEY_LEN;
use super::{author_op, group_state, new_id16, ops_for_pull, seal_current_key_for};

/// Durée de vie minimale d'un ticket d'invitation (1 minute) : borne basse
/// déterministe, évite une expiration immédiate par erreur d'appel.
pub const MIN_INVITE_TTL_MS: u64 = 60_000;
/// Durée de vie maximale d'un ticket d'invitation (30 jours).
pub const MAX_INVITE_TTL_MS: u64 = 30 * 24 * 60 * 60 * 1000;
/// Durée de vie par défaut d'un ticket d'invitation (7 jours), utilisée par
/// la frontière API quand l'appelant n'en précise pas.
pub const DEFAULT_INVITE_TTL_MS: u64 = 7 * 24 * 60 * 60 * 1000;

/// Invitation à usage unique fraîchement autorisée : op signée à répliquer,
/// identifiant, secret en clair (à transporter via [`build_invite_ticket`],
/// jamais persisté tel quel) et expiration effective.
#[derive(Debug)]
pub struct AuthoredInvite {
    /// Op `InviteCreate` signée, à diffuser aux membres actuels.
    pub op: GroupOp,
    /// Identifiant de l'invitation.
    pub invite_id: [u8; 16],
    /// Secret en clair (32 octets aléatoires) : préimage de `code_hash`.
    pub secret: [u8; 32],
    /// Expiration murale ms effective (jamais 0 : toujours bornée).
    pub expires_ms: u64,
}

/// Génère un secret d'invitation aléatoire et son empreinte SHA-256.
fn new_invite_secret() -> ([u8; 32], [u8; 32]) {
    let mut secret = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut secret);
    let hash: [u8; 32] = Sha256::digest(secret).into();
    (secret, hash)
}

/// Autorise une invitation à usage unique (op `InviteCreate`) pour `group_id`.
/// `ttl_ms` est bornée à `[MIN_INVITE_TTL_MS, MAX_INVITE_TTL_MS]`. Échoue
/// comme tout op locale si l'identité ne détient pas `INVITE`
/// ([`author_op`] rejoue l'op sur l'état courant).
pub fn author_invite_create(
    db: &Db,
    identity: &Identity,
    group_id: &[u8; 16],
    now_ms: u64,
    ttl_ms: u64,
) -> Result<AuthoredInvite, CoreError> {
    let ttl_ms = ttl_ms.clamp(MIN_INVITE_TTL_MS, MAX_INVITE_TTL_MS);
    let expires_ms = now_ms.saturating_add(ttl_ms);
    let invite_id = new_id16();
    let (secret, code_hash) = new_invite_secret();
    let op = author_op(
        db,
        identity,
        group_id,
        &GroupOpBody::InviteCreate {
            invite_id,
            code_hash,
            max_uses: 1,
            expires_ms,
        },
        now_ms,
    )?;
    Ok(AuthoredInvite {
        op,
        invite_id,
        secret,
        expires_ms,
    })
}

/// Construit et signe le ticket transportable à envoyer à l'invité
/// (`CoreMsg::InviteTicket`), point-à-point, hors op-log.
pub fn build_invite_ticket(
    identity: &Identity,
    group_id: &[u8; 16],
    invite_id: &[u8; 16],
    group_name: &str,
    secret: &[u8; 32],
    expires_ms: u64,
) -> CoreMsg {
    let inviter = identity.public_key();
    let bytes = invite_ticket_signable_bytes(
        group_id, invite_id, group_name, &inviter, secret, expires_ms,
    );
    let sig = identity.sign(&bytes);
    CoreMsg::InviteTicket {
        group_id: *group_id,
        invite_id: *invite_id,
        group_name: group_name.to_string(),
        inviter,
        secret: *secret,
        expires_ms,
        sig,
    }
}

/// Vérifie la signature d'un `InviteTicket` reçu et son expiration de
/// transport (indépendante de l'expiration de l'op `InviteCreate`,
/// revérifiée côté inviteur au moment de l'acceptation). Ne matérialise
/// jamais le groupe : le seul effet attendu est « ticket digne d'être
/// affiché à l'utilisateur ».
#[allow(clippy::too_many_arguments)]
pub fn verify_invite_ticket(
    group_id: &[u8; 16],
    invite_id: &[u8; 16],
    group_name: &str,
    inviter: &[u8; 32],
    secret: &[u8; 32],
    expires_ms: u64,
    sig: &[u8; 64],
    now_ms: u64,
) -> Result<(), CoreError> {
    if expires_ms != 0 && now_ms > expires_ms {
        return Err(CoreError::Invalid("ticket d'invitation expiré"));
    }
    let bytes =
        invite_ticket_signable_bytes(group_id, invite_id, group_name, inviter, secret, expires_ms);
    verify_signature(inviter, &bytes, sig)?;
    Ok(())
}

/// Résultat d'une acceptation finalisée côté inviteur : op `AddMember`
/// signée, journal complet à rejouer chez le nouveau membre, et clé de
/// groupe courante scellée pour lui — exactement ce que l'ancien force-join
/// poussait sans demande, désormais gated par une acceptation prouvée.
#[derive(Debug)]
pub struct FinalizedInvite {
    /// Op `AddMember` signée, à diffuser aux membres actuels.
    pub add_op: GroupOp,
    /// Journal complet du groupe (ordre canonique), à envoyer au nouveau
    /// membre pour qu'il matérialise l'état.
    pub ops: Vec<GroupOp>,
    /// Epoch de la clé de groupe scellée.
    pub key_epoch: u32,
    /// Clé de groupe courante, scellée pour le nouveau membre.
    pub sealed_key: [u8; SEALED_KEY_LEN],
}

/// Finalise une acceptation d'invitation (`CoreMsg::InviteAccept`) côté
/// inviteur : re-vérifie que l'identité locale détient toujours `INVITE`,
/// que l'invitation est encore valide (non révoquée/expirée/épuisée, selon
/// l'état replié — [`super::state::GroupState::invites`]) et que `secret`
/// correspond bien à `code_hash` (seul le destinataire du ticket original
/// peut le prouver), puis admet le membre.
///
/// N'échoue jamais par panique sur une entrée attaquant-contrôlée : toute
/// invitation/secret invalide rend une erreur explicite, à ignorer
/// silencieusement par l'appelant réseau (pas de canal d'erreur vers un pair
/// non authentifié sur cette invitation).
pub fn finalize_invite_accept(
    db: &Db,
    identity: &Identity,
    group_id: &[u8; 16],
    invite_id: &[u8; 16],
    secret: &[u8; 32],
    invitee: &[u8; 32],
    now_ms: u64,
) -> Result<FinalizedInvite, CoreError> {
    let state = group_state(db, group_id)?;
    if !state.can(&identity.public_key(), perms::INVITE) {
        return Err(CoreError::OpRejected("droit d'invitation révoqué"));
    }
    let invite = state
        .invites
        .get(invite_id)
        .ok_or(CoreError::NotFound("invitation inconnue"))?;
    if invite.revoked
        || (invite.max_uses > 0 && invite.uses >= invite.max_uses)
        || (invite.expires_ms > 0 && now_ms > invite.expires_ms)
    {
        return Err(CoreError::OpRejected(
            "invitation révoquée, expirée ou épuisée",
        ));
    }
    let hash: [u8; 32] = Sha256::digest(secret).into();
    if hash != invite.code_hash {
        return Err(CoreError::OpRejected("secret d'invitation invalide"));
    }
    let add_op = author_op(
        db,
        identity,
        group_id,
        &GroupOpBody::AddMember {
            member: *invitee,
            invite_id: Some(*invite_id),
        },
        now_ms,
    )?;
    let ops = ops_for_pull(db, group_id, 0)?;
    let (key_epoch, sealed_key) = seal_current_key_for(db, group_id, invitee)?;
    Ok(FinalizedInvite {
        add_op,
        ops,
        key_epoch,
        sealed_key,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::group::create_group;

    fn setup() -> (Db, Identity) {
        (
            Db::open_in_memory(&[21u8; 32]).unwrap(),
            Identity::generate_with_pow_bits(1),
        )
    }

    #[test]
    fn ttl_is_clamped_and_never_zero() {
        let (db, founder) = setup();
        let created = create_group(&db, &founder, "G", 0).unwrap();
        let inv = author_invite_create(&db, &founder, &created.group_id, 1_000, 0).unwrap();
        assert_eq!(inv.expires_ms, 1_000 + MIN_INVITE_TTL_MS);

        let inv2 = author_invite_create(&db, &founder, &created.group_id, 1_000, u64::MAX).unwrap();
        assert_eq!(inv2.expires_ms, 1_000 + MAX_INVITE_TTL_MS);
    }

    #[test]
    fn ticket_roundtrips_signature_and_rejects_tampering() {
        let (db, founder) = setup();
        let created = create_group(&db, &founder, "Guilde", 0).unwrap();
        let inv = author_invite_create(&db, &founder, &created.group_id, 0, 0).unwrap();
        let ticket = build_invite_ticket(
            &founder,
            &created.group_id,
            &inv.invite_id,
            "Guilde",
            &inv.secret,
            inv.expires_ms,
        );
        let CoreMsg::InviteTicket {
            group_id,
            invite_id,
            group_name,
            inviter,
            secret,
            expires_ms,
            sig,
        } = ticket
        else {
            unreachable!("build_invite_ticket produit un InviteTicket");
        };
        verify_invite_ticket(
            &group_id,
            &invite_id,
            &group_name,
            &inviter,
            &secret,
            expires_ms,
            &sig,
            0,
        )
        .unwrap();

        // Signature forgée (secret altéré après coup) : rejetée.
        assert!(verify_invite_ticket(
            &group_id,
            &invite_id,
            &group_name,
            &inviter,
            &[0xFF; 32],
            expires_ms,
            &sig,
            0,
        )
        .is_err());

        // Ticket déjà expiré au moment de la vérification : rejeté.
        assert!(verify_invite_ticket(
            &group_id,
            &invite_id,
            &group_name,
            &inviter,
            &secret,
            1_000,
            &sig,
            2_000,
        )
        .is_err());
    }

    #[test]
    fn finalize_requires_correct_secret_and_is_single_use() {
        let (db, founder) = setup();
        let bob = Identity::generate_with_pow_bits(1);
        let created = create_group(&db, &founder, "Guilde", 0).unwrap();
        let inv = author_invite_create(&db, &founder, &created.group_id, 0, 0).unwrap();

        // Mauvais secret : refusé, personne n'est admis.
        assert!(finalize_invite_accept(
            &db,
            &founder,
            &created.group_id,
            &inv.invite_id,
            &[0xAA; 32],
            &bob.public_key(),
            1,
        )
        .is_err());
        assert!(!group_state(&db, &created.group_id)
            .unwrap()
            .is_member(&bob.public_key()));

        // Bon secret : admis, la clé lui est scellée.
        let finalized = finalize_invite_accept(
            &db,
            &founder,
            &created.group_id,
            &inv.invite_id,
            &inv.secret,
            &bob.public_key(),
            2,
        )
        .unwrap();
        assert!(finalized.ops.iter().any(|o| o.kind == 0x07));
        assert!(group_state(&db, &created.group_id)
            .unwrap()
            .is_member(&bob.public_key()));

        // Rejeu (invitation à usage unique déjà consommée) : refusé.
        let carol = Identity::generate_with_pow_bits(1);
        assert!(finalize_invite_accept(
            &db,
            &founder,
            &created.group_id,
            &inv.invite_id,
            &inv.secret,
            &carol.public_key(),
            3,
        )
        .is_err());
    }

    #[test]
    fn finalize_rejects_unknown_invite() {
        let (db, founder) = setup();
        let bob = Identity::generate_with_pow_bits(1);
        let created = create_group(&db, &founder, "Guilde", 0).unwrap();
        assert!(finalize_invite_accept(
            &db,
            &founder,
            &created.group_id,
            &[0xEE; 16],
            &[0; 32],
            &bob.public_key(),
            1,
        )
        .is_err());
    }
}
