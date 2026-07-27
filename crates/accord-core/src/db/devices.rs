//! Persistance du compte et de ses appareils (multi-appareil, jalon 1).
//!
//! Deux tables, créées par la migration 13 (voir [`super::MIGRATIONS`]) :
//!
//! - `local_device` — **notre** appareil sur cette machine : sa graine privée.
//!   Une seule ligne. La graine ne quitte jamais cette base.
//! - `device_lists` — les listes d'appareils **des autres**, mises en cache par
//!   compte, avec leur version pour rejeter un rejeu.
//!
//! La liste de notre propre compte se recalcule depuis la clé racine ; seule
//! sa version courante est mémorisée, pour qu'une réémission dépasse toujours
//! la précédente.
//!
//! Une troisième, `device_seen` (migration 17), retient la dernière fois que
//! chaque appareil du compte s'est manifesté ICI, et par quelle route.
//! 🔒 Elle est purement locale : rien de ce qu'elle contient ne circule.

use super::Db;
use crate::error::CoreError;
use std::collections::HashMap;

/// Graine de l'appareil local et sa preuve de travail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalDevice {
    /// Graine Ed25519 privée de cet appareil. 🔒 Ne sort jamais de la base.
    pub seed: [u8; 32],
    /// Nonce de preuve de travail associé (évite de le recalculer au
    /// démarrage : c'est plusieurs secondes de CPU à la difficulté réelle).
    pub pow_nonce: u64,
    /// Nom lisible affiché aux autres appareils du compte.
    pub name: String,
}

/// Liste d'appareils d'un compte distant, telle que reçue et vérifiée.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedDeviceList {
    /// Clé publique racine du compte.
    pub account: [u8; 32],
    /// Version de la liste mise en cache.
    pub version: u64,
    /// Encodage filaire complet, resservi tel quel (signature comprise) :
    /// re-signer ou reconstruire serait à la fois inutile et faux, puisque
    /// seule la racine peut signer.
    pub encoded: Vec<u8>,
    /// Date de réception locale (ms), pour la fraîcheur du cache.
    pub fetched_ms: u64,
}

/// Dernier contact observé avec un appareil du compte, vu de CETTE machine.
///
/// 🔒 Ni adresse ni lieu : la route, et rien de plus. Voir la migration 17
/// pour le raisonnement complet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceSeen {
    /// Date du dernier contact (ms epoch).
    pub last_ms: u64,
    /// Vrai si ce dernier contact passait par un circuit relais, faux s'il
    /// était direct.
    pub relayed: bool,
}

impl Db {
    /// Appareil local, s'il a déjà été créé.
    pub fn local_device(&self) -> Result<Option<LocalDevice>, CoreError> {
        let mut stmt = self
            .conn
            .prepare("SELECT seed, pow_nonce, name FROM local_device WHERE id = 1")?;
        let mut rows = stmt.query([])?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };
        Ok(Some(LocalDevice {
            seed: super::blob(row.get::<_, Vec<u8>>(0)?)?,
            pow_nonce: row.get::<_, i64>(1)? as u64,
            name: row.get(2)?,
        }))
    }

    /// Écrit (ou remplace) l'appareil local.
    pub fn set_local_device(&self, device: &LocalDevice) -> Result<(), CoreError> {
        self.conn.execute(
            "INSERT INTO local_device(id, seed, pow_nonce, name) VALUES (1, ?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET
               seed = excluded.seed,
               pow_nonce = excluded.pow_nonce,
               name = excluded.name",
            rusqlite::params![&device.seed[..], device.pow_nonce as i64, device.name],
        )?;
        Ok(())
    }

    /// Efface l'appareil local : la base retrouve l'état d'une machine qui
    /// n'en a pas encore, et le prochain démarrage lui en forge un neuf.
    ///
    /// 🔒 Existe pour l'import de sauvegarde. Une archive contient la base,
    /// donc cette graine ; la restaurer telle quelle sur une seconde machine y
    /// réinstallerait la clé d'appareil de la première, et le transport ne
    /// garde qu'une session directe par clé statique — les deux machines
    /// s'évinceraient l'une l'autre chez chacun de leurs amis.
    ///
    /// Effacer plutôt qu'écrire une graine neuve ici : la création d'une
    /// identité d'appareil (aléa, preuve de travail, difficulté) est une
    /// décision qui appartient au démarrage, et la dupliquer laisserait deux
    /// endroits capables de diverger.
    pub fn clear_local_device(&self) -> Result<(), CoreError> {
        self.conn.execute("DELETE FROM local_device", [])?;
        Ok(())
    }

    /// Renomme l'appareil local. Sans effet s'il n'existe pas encore.
    pub fn rename_local_device(&self, name: &str) -> Result<(), CoreError> {
        self.conn
            .execute("UPDATE local_device SET name = ?1 WHERE id = 1", [name])?;
        Ok(())
    }

    /// Liste d'appareils en cache pour un compte.
    pub fn device_list(&self, account: &[u8; 32]) -> Result<Option<CachedDeviceList>, CoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT account, version, encoded, fetched_ms FROM device_lists WHERE account = ?1",
        )?;
        let mut rows = stmt.query([&account[..]])?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };
        Ok(Some(CachedDeviceList {
            account: super::blob(row.get::<_, Vec<u8>>(0)?)?,
            version: row.get::<_, i64>(1)? as u64,
            encoded: row.get(2)?,
            fetched_ms: row.get::<_, i64>(3)? as u64,
        }))
    }

    /// Met en cache une liste d'appareils **si elle est plus récente** que
    /// celle déjà connue. Rend `true` si le cache a changé.
    ///
    /// 🔒 La comparaison de version vit ici, au plus près de l'écriture. La
    /// placer chez l'appelant laisserait un chemin d'écriture non protégé le
    /// jour où un second appelant apparaît — et ce chemin ressusciterait un
    /// appareil révoqué.
    pub fn cache_device_list(&self, list: &CachedDeviceList) -> Result<bool, CoreError> {
        if let Some(current) = self.device_list(&list.account)? {
            if list.version <= current.version {
                return Ok(false);
            }
        }
        self.conn.execute(
            "INSERT INTO device_lists(account, version, encoded, fetched_ms)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(account) DO UPDATE SET
               version = excluded.version,
               encoded = excluded.encoded,
               fetched_ms = excluded.fetched_ms",
            rusqlite::params![
                &list.account[..],
                list.version as i64,
                list.encoded,
                list.fetched_ms as i64
            ],
        )?;
        Ok(true)
    }

    /// Note qu'un appareil du compte vient de se manifester sur cette machine.
    ///
    /// **Dernier écrivain gagne**, volontairement : chaque appel décrit un
    /// contact qui a lieu MAINTENANT, et le champ répond à « quand pour la
    /// dernière fois ? ». Un garde de monotonie protégerait d'un cas qui
    /// n'existe pas (personne n'écrit de contact passé) et en créerait un
    /// vrai : une horloge locale qui recule d'une heure — changement d'heure,
    /// resynchronisation NTP — figerait la ligne sur une date future, et
    /// l'écran afficherait pour toujours une machine « vue » demain.
    pub fn note_device_seen(&self, device: &[u8; 32], seen: DeviceSeen) -> Result<(), CoreError> {
        self.conn.execute(
            "INSERT INTO device_seen(device, last_ms, relayed) VALUES (?1, ?2, ?3)
             ON CONFLICT(device) DO UPDATE SET
               last_ms = excluded.last_ms,
               relayed = excluded.relayed",
            rusqlite::params![&device[..], seen.last_ms as i64, seen.relayed as i64],
        )?;
        Ok(())
    }

    /// Derniers contacts connus, indexés par clé d'appareil.
    ///
    /// Toute la table d'un coup : l'écran « Mes appareils » les veut tous, et
    /// une requête par ligne d'une liste bornée à huit appareils serait du
    /// travail en plus pour le même résultat. Un appareil absent de la table
    /// n'a simplement jamais été joint depuis cette machine — l'appelant le
    /// distingue d'une date à zéro, qui serait un mensonge.
    pub fn devices_seen(&self) -> Result<HashMap<[u8; 32], DeviceSeen>, CoreError> {
        let mut stmt = self
            .conn
            .prepare("SELECT device, last_ms, relayed FROM device_seen")?;
        let mut rows = stmt.query([])?;
        let mut out = HashMap::new();
        while let Some(row) = rows.next()? {
            out.insert(
                super::blob(row.get::<_, Vec<u8>>(0)?)?,
                DeviceSeen {
                    last_ms: row.get::<_, i64>(1)? as u64,
                    relayed: row.get::<_, i64>(2)? != 0,
                },
            );
        }
        Ok(out)
    }

    /// Oublie la liste d'un compte (retrait d'ami, purge).
    pub fn forget_device_list(&self, account: &[u8; 32]) -> Result<(), CoreError> {
        self.conn.execute(
            "DELETE FROM device_lists WHERE account = ?1",
            [&account[..]],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Db {
        Db::open_in_memory(&[3u8; 32]).unwrap()
    }

    fn cached(account: u8, version: u64) -> CachedDeviceList {
        CachedDeviceList {
            account: [account; 32],
            version,
            encoded: vec![account; 16],
            fetched_ms: 1_700_000_000_000,
        }
    }

    #[test]
    fn no_local_device_on_a_fresh_database() {
        assert_eq!(db().local_device().unwrap(), None);
    }

    #[test]
    fn local_device_roundtrips() {
        let db = db();
        let device = LocalDevice {
            seed: [7; 32],
            pow_nonce: 42,
            name: "Portable".into(),
        };
        db.set_local_device(&device).unwrap();
        assert_eq!(db.local_device().unwrap(), Some(device));
    }

    #[test]
    fn writing_twice_replaces_rather_than_duplicates() {
        // Une seconde ligne signifierait deux identités de transport sur la
        // même machine — le problème exact que ce jalon corrige.
        let db = db();
        for seed in [[1u8; 32], [2u8; 32]] {
            db.set_local_device(&LocalDevice {
                seed,
                pow_nonce: 1,
                name: "X".into(),
            })
            .unwrap();
        }
        assert_eq!(db.local_device().unwrap().unwrap().seed, [2u8; 32]);
        let n: i64 = db
            .conn
            .query_row("SELECT count(*) FROM local_device", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn renaming_keeps_the_seed() {
        let db = db();
        db.set_local_device(&LocalDevice {
            seed: [5; 32],
            pow_nonce: 9,
            name: "Ancien".into(),
        })
        .unwrap();
        db.rename_local_device("Nouveau").unwrap();
        let device = db.local_device().unwrap().unwrap();
        assert_eq!(device.name, "Nouveau");
        assert_eq!(device.seed, [5; 32]);
        assert_eq!(device.pow_nonce, 9);
    }

    #[test]
    fn caching_a_newer_list_replaces_the_older_one() {
        let db = db();
        assert!(db.cache_device_list(&cached(1, 10)).unwrap());
        assert!(db.cache_device_list(&cached(1, 11)).unwrap());
        assert_eq!(db.device_list(&[1; 32]).unwrap().unwrap().version, 11);
    }

    #[test]
    fn an_older_or_equal_list_is_refused() {
        // 🔒 Sans ce refus, rejouer une liste ancienne ressusciterait un
        // appareil révoqué.
        let db = db();
        db.cache_device_list(&cached(1, 10)).unwrap();
        assert!(!db.cache_device_list(&cached(1, 10)).unwrap());
        assert!(!db.cache_device_list(&cached(1, 9)).unwrap());
        assert_eq!(db.device_list(&[1; 32]).unwrap().unwrap().version, 10);
    }

    #[test]
    fn accounts_do_not_share_a_cache_entry() {
        let db = db();
        db.cache_device_list(&cached(1, 5)).unwrap();
        db.cache_device_list(&cached(2, 3)).unwrap();
        assert_eq!(db.device_list(&[1; 32]).unwrap().unwrap().version, 5);
        assert_eq!(db.device_list(&[2; 32]).unwrap().unwrap().version, 3);
    }

    #[test]
    fn forgetting_removes_only_the_named_account() {
        let db = db();
        db.cache_device_list(&cached(1, 5)).unwrap();
        db.cache_device_list(&cached(2, 5)).unwrap();
        db.forget_device_list(&[1; 32]).unwrap();
        assert!(db.device_list(&[1; 32]).unwrap().is_none());
        assert!(db.device_list(&[2; 32]).unwrap().is_some());
    }

    #[test]
    fn an_unseen_device_has_no_entry_at_all() {
        // Une ligne absente et une date à zéro ne disent pas la même chose :
        // « jamais joint depuis cette machine » n'est pas « joint le 1er
        // janvier 1970 ». L'écran s'appuie sur cette distinction.
        assert!(db().devices_seen().unwrap().is_empty());
    }

    #[test]
    fn a_noted_contact_roundtrips_with_its_route() {
        let db = db();
        db.note_device_seen(
            &[9; 32],
            DeviceSeen {
                last_ms: 1_700_000_000_000,
                relayed: true,
            },
        )
        .unwrap();
        assert_eq!(
            db.devices_seen().unwrap().get(&[9u8; 32]),
            Some(&DeviceSeen {
                last_ms: 1_700_000_000_000,
                relayed: true,
            })
        );
    }

    #[test]
    fn a_later_contact_replaces_the_previous_one_route_included() {
        // Le champ répond à « quand pour la dernière fois, et comment » : une
        // seconde ligne, ou une route figée sur le premier contact, seraient
        // deux façons de répondre à une autre question.
        let db = db();
        for seen in [
            DeviceSeen {
                last_ms: 10,
                relayed: true,
            },
            DeviceSeen {
                last_ms: 20,
                relayed: false,
            },
        ] {
            db.note_device_seen(&[4; 32], seen).unwrap();
        }
        assert_eq!(
            db.devices_seen().unwrap().get(&[4u8; 32]),
            Some(&DeviceSeen {
                last_ms: 20,
                relayed: false,
            })
        );
        let n: i64 = db
            .conn
            .query_row("SELECT count(*) FROM device_seen", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn devices_do_not_share_a_seen_entry() {
        let db = db();
        db.note_device_seen(
            &[1; 32],
            DeviceSeen {
                last_ms: 5,
                relayed: false,
            },
        )
        .unwrap();
        db.note_device_seen(
            &[2; 32],
            DeviceSeen {
                last_ms: 7,
                relayed: true,
            },
        )
        .unwrap();
        let seen = db.devices_seen().unwrap();
        assert_eq!(
            seen.get(&[1u8; 32]).map(|s| (s.last_ms, s.relayed)),
            Some((5, false))
        );
        assert_eq!(
            seen.get(&[2u8; 32]).map(|s| (s.last_ms, s.relayed)),
            Some((7, true))
        );
    }

    #[test]
    fn the_encoded_list_is_returned_byte_for_byte() {
        // Elle est resservie telle quelle, signature comprise : la
        // reconstruire serait faux, seule la racine peut signer.
        let db = db();
        let mut list = cached(1, 1);
        list.encoded = (0u8..=255).collect();
        db.cache_device_list(&list).unwrap();
        assert_eq!(
            db.device_list(&[1; 32]).unwrap().unwrap().encoded,
            list.encoded
        );
    }
}
