//! Invitations entrantes en attente (porte de consentement, D-045).
//!
//! Jamais répliquée : une invitation reçue (`CoreMsg::InviteTicket`) reste
//! purement locale tant que l'utilisateur ne l'a pas explicitement acceptée
//! ou refusée ([`crate::group::invite`]).

use super::{blob, Db};
use crate::error::CoreError;
use rusqlite::params;

/// Plafond d'invitations entrantes en attente par inviteur (anti-abus) : un
/// ticket signature-valide est bon marché à forger (l'attaquant s'auto-signe
/// comme inviteur et fait varier `(group_id, invite_id)`), sans ce plafond il
/// pourrait insérer un nombre illimité de lignes et déclencher un nombre
/// illimité de `event.group_invite_pending`.
pub const MAX_INCOMING_INVITES_PER_INVITER: usize = 20;

/// Invitation reçue, en attente de décision locale (accepter/refuser).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncomingInvite {
    /// Groupe concerné.
    pub group_id: [u8; 16],
    /// Invitation correspondant à l'op `InviteCreate` répliquée.
    pub invite_id: [u8; 16],
    /// Nom du groupe au moment de l'invitation.
    pub group_name: String,
    /// Clé publique de l'inviteur.
    pub inviter: [u8; 32],
    /// Secret d'invitation (préimage du `code_hash` de l'op `InviteCreate`).
    pub secret: [u8; 32],
    /// Expiration murale ms du ticket (0 = jamais).
    pub expires_ms: u64,
    /// Horloge murale locale de réception (affichage seulement).
    pub received_ms: u64,
}

impl Db {
    /// Enregistre (ou remplace) une invitation entrante en attente, sous
    /// réserve du plafond anti-abus [`MAX_INCOMING_INVITES_PER_INVITER`] par
    /// inviteur. Une mise à jour d'une invitation déjà en attente pour la
    /// même clé `(group_id, invite_id)` n'ajoute pas de nouvelle ligne et
    /// n'est donc jamais bloquée par le plafond. Rend `true` si l'invitation
    /// a été enregistrée, `false` si elle a été abandonnée (plafond atteint)
    /// — l'appelant ne doit alors émettre aucun événement.
    pub fn insert_incoming_invite(&self, inv: &IncomingInvite) -> Result<bool, CoreError> {
        let is_new_key = self
            .incoming_invite(&inv.group_id, &inv.invite_id)?
            .is_none();
        if is_new_key {
            let pending_for_inviter: i64 = self.conn().query_row(
                "SELECT COUNT(*) FROM group_invites_incoming WHERE inviter = ?1",
                params![inv.inviter.as_slice()],
                |row| row.get(0),
            )?;
            if pending_for_inviter as usize >= MAX_INCOMING_INVITES_PER_INVITER {
                return Ok(false);
            }
        }
        self.conn().execute(
            "INSERT INTO group_invites_incoming
               (group_id, invite_id, group_name, inviter, secret, expires_ms, received_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(group_id, invite_id) DO UPDATE SET
               group_name = excluded.group_name,
               inviter = excluded.inviter,
               secret = excluded.secret,
               expires_ms = excluded.expires_ms,
               received_ms = excluded.received_ms",
            params![
                inv.group_id.as_slice(),
                inv.invite_id.as_slice(),
                inv.group_name,
                inv.inviter.as_slice(),
                inv.secret.as_slice(),
                inv.expires_ms,
                inv.received_ms,
            ],
        )?;
        Ok(true)
    }

    /// Une invitation entrante précise, si en attente.
    pub fn incoming_invite(
        &self,
        group_id: &[u8; 16],
        invite_id: &[u8; 16],
    ) -> Result<Option<IncomingInvite>, CoreError> {
        let mut stmt = self.conn().prepare(
            "SELECT group_id, invite_id, group_name, inviter, secret, expires_ms, received_ms
             FROM group_invites_incoming WHERE group_id = ?1 AND invite_id = ?2",
        )?;
        let raw = stmt
            .query_map(params![group_id.as_slice(), invite_id.as_slice()], raw_row)?
            .collect::<Result<Vec<_>, _>>()?;
        raw.into_iter().next().map(values_to_invite).transpose()
    }

    /// Toutes les invitations entrantes en attente, plus récentes en premier.
    pub fn incoming_invites(&self) -> Result<Vec<IncomingInvite>, CoreError> {
        let mut stmt = self.conn().prepare(
            "SELECT group_id, invite_id, group_name, inviter, secret, expires_ms, received_ms
             FROM group_invites_incoming ORDER BY received_ms DESC",
        )?;
        let raw = stmt
            .query_map([], raw_row)?
            .collect::<Result<Vec<_>, _>>()?;
        raw.into_iter().map(values_to_invite).collect()
    }

    /// Retire une invitation entrante (acceptée, refusée ou expirée).
    pub fn remove_incoming_invite(
        &self,
        group_id: &[u8; 16],
        invite_id: &[u8; 16],
    ) -> Result<(), CoreError> {
        self.conn().execute(
            "DELETE FROM group_invites_incoming WHERE group_id = ?1 AND invite_id = ?2",
            params![group_id.as_slice(), invite_id.as_slice()],
        )?;
        Ok(())
    }
}

type RawInviteRow = (Vec<u8>, Vec<u8>, String, Vec<u8>, Vec<u8>, u64, u64);

fn raw_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawInviteRow> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
    ))
}

fn values_to_invite(v: RawInviteRow) -> Result<IncomingInvite, CoreError> {
    Ok(IncomingInvite {
        group_id: blob(v.0)?,
        invite_id: blob(v.1)?,
        group_name: v.2,
        inviter: blob(v.3)?,
        secret: blob(v.4)?,
        expires_ms: v.5,
        received_ms: v.6,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(group_id: [u8; 16], invite_id: [u8; 16]) -> IncomingInvite {
        IncomingInvite {
            group_id,
            invite_id,
            group_name: "Guilde".into(),
            inviter: [7; 32],
            secret: [8; 32],
            expires_ms: 1_000,
            received_ms: 500,
        }
    }

    #[test]
    fn insert_get_list_and_remove_roundtrip() {
        let db = Db::open_in_memory(&[1; 32]).unwrap();
        assert!(db.incoming_invite(&[1; 16], &[2; 16]).unwrap().is_none());

        let inv = sample([1; 16], [2; 16]);
        db.insert_incoming_invite(&inv).unwrap();
        assert_eq!(
            db.incoming_invite(&[1; 16], &[2; 16]).unwrap(),
            Some(inv.clone())
        );
        assert_eq!(db.incoming_invites().unwrap(), vec![inv.clone()]);

        // Un second insert avec la même clé (group_id, invite_id) remplace
        // les champs (ex : nouveau ticket pour la même invitation).
        let mut updated = inv.clone();
        updated.group_name = "Renommée".into();
        db.insert_incoming_invite(&updated).unwrap();
        assert_eq!(
            db.incoming_invite(&[1; 16], &[2; 16]).unwrap(),
            Some(updated)
        );

        db.remove_incoming_invite(&[1; 16], &[2; 16]).unwrap();
        assert!(db.incoming_invite(&[1; 16], &[2; 16]).unwrap().is_none());
        assert!(db.incoming_invites().unwrap().is_empty());
    }

    #[test]
    fn multiple_invites_are_independent() {
        let db = Db::open_in_memory(&[1; 32]).unwrap();
        db.insert_incoming_invite(&sample([1; 16], [2; 16]))
            .unwrap();
        db.insert_incoming_invite(&sample([1; 16], [3; 16]))
            .unwrap();
        db.insert_incoming_invite(&sample([9; 16], [2; 16]))
            .unwrap();
        assert_eq!(db.incoming_invites().unwrap().len(), 3);
        db.remove_incoming_invite(&[1; 16], &[2; 16]).unwrap();
        assert_eq!(db.incoming_invites().unwrap().len(), 2);
    }

    #[test]
    fn insert_incoming_invite_enforces_per_inviter_cap() {
        let db = Db::open_in_memory(&[1; 32]).unwrap();
        let attacker = [42u8; 32];

        // The first MAX_INCOMING_INVITES_PER_INVITER tickets from the same
        // self-signed inviter are all kept.
        for i in 0..MAX_INCOMING_INVITES_PER_INVITER as u8 {
            let mut inv = sample([i; 16], [i; 16]);
            inv.inviter = attacker;
            assert!(db.insert_incoming_invite(&inv).unwrap(), "ticket {i} kept");
        }
        assert_eq!(
            db.incoming_invites().unwrap().len(),
            MAX_INCOMING_INVITES_PER_INVITER
        );

        // A 21st ticket from the SAME inviter (new group_id/invite_id pair)
        // is dropped: no row inserted, no room for the caller to emit.
        let mut over_cap = sample([21; 16], [21; 16]);
        over_cap.inviter = attacker;
        assert!(!db.insert_incoming_invite(&over_cap).unwrap());
        assert_eq!(
            db.incoming_invites().unwrap().len(),
            MAX_INCOMING_INVITES_PER_INVITER
        );
        assert!(db.incoming_invite(&[21; 16], &[21; 16]).unwrap().is_none());

        // Updating an already-pending ticket for the same inviter (same
        // group_id/invite_id key) is not a new row, so it is never blocked
        // by the cap even while at capacity.
        let mut update = sample([0; 16], [0; 16]);
        update.inviter = attacker;
        update.group_name = "Renommée".into();
        assert!(db.insert_incoming_invite(&update).unwrap());
        assert_eq!(
            db.incoming_invites().unwrap().len(),
            MAX_INCOMING_INVITES_PER_INVITER
        );

        // A ticket from a DIFFERENT inviter is unaffected by the first
        // inviter's cap and is accepted.
        let other = sample([99; 16], [99; 16]);
        assert!(db.insert_incoming_invite(&other).unwrap());
        assert_eq!(
            db.incoming_invites().unwrap().len(),
            MAX_INCOMING_INVITES_PER_INVITER + 1
        );
    }
}
