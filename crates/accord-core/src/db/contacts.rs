//! Contacts : amis, demandes en attente, blocages.

use super::{blob, Db};
use crate::error::CoreError;
use rusqlite::params;

/// État relationnel d'un contact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ContactState {
    /// Demande envoyée, en attente de réponse.
    PendingOut = 0,
    /// Demande reçue, en attente de notre décision.
    PendingIn = 1,
    /// Ami confirmé.
    Friend = 2,
    /// Bloqué (tout trafic entrant ignoré).
    Blocked = 3,
}

impl ContactState {
    /// Décode l'état depuis la base.
    pub fn from_u8(v: u8) -> Result<Self, CoreError> {
        match v {
            0 => Ok(Self::PendingOut),
            1 => Ok(Self::PendingIn),
            2 => Ok(Self::Friend),
            3 => Ok(Self::Blocked),
            _ => Err(CoreError::Invalid("état de contact")),
        }
    }
}

/// Contact connu (la clé publique fait foi ; le nom est déclaratif).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contact {
    /// Identifiant réseau (SHA-256 de la clé publique).
    pub node_id: [u8; 32],
    /// Clé publique Ed25519.
    pub pubkey: [u8; 32],
    /// Pseudo affiché (fourni par le pair, non authentifié).
    pub display_name: String,
    /// État relationnel.
    pub state: ContactState,
    /// Date d'ajout (ms).
    pub added_ms: u64,
    /// Dernière activité vue (ms).
    pub last_seen_ms: u64,
}

/// Colonnes brutes d'un contact, avant validation des tailles de blobs.
type RawContact = (Vec<u8>, Vec<u8>, String, u8, u64, u64);

fn row_to_contact(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawContact> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
    ))
}

fn build(raw: RawContact) -> Result<Contact, CoreError> {
    Ok(Contact {
        node_id: blob(raw.0)?,
        pubkey: blob(raw.1)?,
        display_name: raw.2,
        state: ContactState::from_u8(raw.3)?,
        added_ms: raw.4,
        last_seen_ms: raw.5,
    })
}

const COLS: &str = "node_id, pubkey, display_name, state, added_ms, last_seen_ms";

impl Db {
    /// Insère ou met à jour un contact.
    pub fn upsert_contact(&self, c: &Contact) -> Result<(), CoreError> {
        self.conn().execute(
            "INSERT INTO contacts (node_id, pubkey, display_name, state, added_ms, last_seen_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(node_id) DO UPDATE SET
               display_name = excluded.display_name,
               state        = excluded.state,
               last_seen_ms = excluded.last_seen_ms",
            params![
                c.node_id,
                c.pubkey,
                c.display_name,
                c.state as u8,
                c.added_ms,
                c.last_seen_ms
            ],
        )?;
        Ok(())
    }

    /// Contact par identifiant réseau.
    pub fn contact(&self, node_id: &[u8; 32]) -> Result<Option<Contact>, CoreError> {
        let mut stmt = self
            .conn()
            .prepare(&format!("SELECT {COLS} FROM contacts WHERE node_id = ?1"))?;
        let mut rows = stmt.query([node_id.as_slice()])?;
        match rows.next()? {
            Some(row) => Ok(Some(build(row_to_contact(row)?)?)),
            None => Ok(None),
        }
    }

    /// Tous les contacts, amis d'abord puis par nom.
    pub fn contacts(&self) -> Result<Vec<Contact>, CoreError> {
        let mut stmt = self.conn().prepare(&format!(
            "SELECT {COLS} FROM contacts ORDER BY state DESC, display_name ASC"
        ))?;
        let raws = stmt
            .query_map([], row_to_contact)?
            .collect::<Result<Vec<_>, _>>()?;
        raws.into_iter().map(build).collect()
    }

    /// Change l'état relationnel d'un contact existant.
    pub fn set_contact_state(
        &self,
        node_id: &[u8; 32],
        state: ContactState,
    ) -> Result<(), CoreError> {
        let n = self.conn().execute(
            "UPDATE contacts SET state = ?2 WHERE node_id = ?1",
            params![node_id, state as u8],
        )?;
        if n == 0 {
            return Err(CoreError::NotFound("contact"));
        }
        Ok(())
    }

    /// Met à jour le pseudo affiché d'un contact (annoncé par le pair) et sa
    /// dernière activité vue.
    pub fn set_contact_name(
        &self,
        node_id: &[u8; 32],
        display_name: &str,
        now_ms: u64,
    ) -> Result<(), CoreError> {
        let n = self.conn().execute(
            "UPDATE contacts SET display_name = ?2, last_seen_ms = ?3 WHERE node_id = ?1",
            params![node_id, display_name, now_ms],
        )?;
        if n == 0 {
            return Err(CoreError::NotFound("contact"));
        }
        Ok(())
    }

    /// Met à jour la dernière activité vue.
    pub fn touch_contact(&self, node_id: &[u8; 32], now_ms: u64) -> Result<(), CoreError> {
        self.conn().execute(
            "UPDATE contacts SET last_seen_ms = ?2 WHERE node_id = ?1",
            params![node_id, now_ms],
        )?;
        Ok(())
    }

    /// Supprime un contact (et rien d'autre : l'historique reste).
    pub fn remove_contact(&self, node_id: &[u8; 32]) -> Result<(), CoreError> {
        self.conn().execute(
            "DELETE FROM contacts WHERE node_id = ?1",
            [node_id.as_slice()],
        )?;
        Ok(())
    }

    // ---- Notes privées de contact (locales, jamais émises) ----

    /// Écrit la note privée attachée à une clé publique. Une note vide efface
    /// l'entrée. La note ne quitte jamais l'appareil (aucun message filaire).
    pub fn set_contact_note(&self, pubkey: &[u8; 32], note: &str) -> Result<(), CoreError> {
        if note.is_empty() {
            self.conn().execute(
                "DELETE FROM contact_notes WHERE pubkey = ?1",
                [pubkey.as_slice()],
            )?;
        } else {
            self.conn().execute(
                "INSERT INTO contact_notes (pubkey, note) VALUES (?1, ?2)
                 ON CONFLICT(pubkey) DO UPDATE SET note = excluded.note",
                params![pubkey, note],
            )?;
        }
        Ok(())
    }

    /// Lit la note privée d'une clé publique (`None` si aucune).
    pub fn contact_note(&self, pubkey: &[u8; 32]) -> Result<Option<String>, CoreError> {
        let mut stmt = self
            .conn()
            .prepare("SELECT note FROM contact_notes WHERE pubkey = ?1")?;
        let mut rows = stmt.query([pubkey.as_slice()])?;
        match rows.next()? {
            Some(row) => Ok(Some(row.get(0)?)),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contact(id: u8, state: ContactState) -> Contact {
        Contact {
            node_id: [id; 32],
            pubkey: [id; 32],
            display_name: format!("pair-{id}"),
            state,
            added_ms: 1000,
            last_seen_ms: 0,
        }
    }

    #[test]
    fn upsert_and_fetch_roundtrip() {
        let db = Db::open_in_memory(&[1; 32]).unwrap();
        let c = contact(9, ContactState::PendingOut);
        db.upsert_contact(&c).unwrap();
        assert_eq!(db.contact(&[9; 32]).unwrap(), Some(c.clone()));
        // L'upsert met à jour l'état sans dupliquer.
        let mut c2 = c;
        c2.state = ContactState::Friend;
        db.upsert_contact(&c2).unwrap();
        assert_eq!(db.contacts().unwrap().len(), 1);
        assert_eq!(
            db.contact(&[9; 32]).unwrap().unwrap().state,
            ContactState::Friend
        );
    }

    #[test]
    fn set_contact_name_updates_name_and_last_seen() {
        let db = Db::open_in_memory(&[1; 32]).unwrap();
        assert!(matches!(
            db.set_contact_name(&[9; 32], "Anna", 5),
            Err(CoreError::NotFound(_))
        ));
        db.upsert_contact(&contact(9, ContactState::Friend))
            .unwrap();
        db.set_contact_name(&[9; 32], "Anna", 5).unwrap();
        let c = db.contact(&[9; 32]).unwrap().unwrap();
        assert_eq!(c.display_name, "Anna");
        assert_eq!(c.last_seen_ms, 5);
    }

    #[test]
    fn state_change_requires_existing_contact() {
        let db = Db::open_in_memory(&[1; 32]).unwrap();
        assert!(matches!(
            db.set_contact_state(&[5; 32], ContactState::Blocked),
            Err(CoreError::NotFound(_))
        ));
        db.upsert_contact(&contact(5, ContactState::PendingIn))
            .unwrap();
        db.set_contact_state(&[5; 32], ContactState::Blocked)
            .unwrap();
        assert_eq!(
            db.contact(&[5; 32]).unwrap().unwrap().state,
            ContactState::Blocked
        );
    }

    #[test]
    fn contact_note_set_get_update_and_clear() {
        let db = Db::open_in_memory(&[1; 32]).unwrap();
        // A note is keyed by pubkey, independent of contact existence.
        assert_eq!(db.contact_note(&[9; 32]).unwrap(), None);
        db.set_contact_note(&[9; 32], "note privée").unwrap();
        assert_eq!(
            db.contact_note(&[9; 32]).unwrap(),
            Some("note privée".into())
        );
        // Upsert overwrites.
        db.set_contact_note(&[9; 32], "révisée").unwrap();
        assert_eq!(db.contact_note(&[9; 32]).unwrap(), Some("révisée".into()));
        // An empty note deletes the row.
        db.set_contact_note(&[9; 32], "").unwrap();
        assert_eq!(db.contact_note(&[9; 32]).unwrap(), None);
    }
}
