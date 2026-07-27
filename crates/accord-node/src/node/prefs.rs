//! Préférences de compte partagées entre les appareils (feuille de route
//! §17.4) : émission, réception et borne d'horloge.
//!
//! La liste blanche des clés et la règle de conflit vivent dans
//! [`accord_core::prefs`] — ici on ne fait que le tour de piste réseau, sur le
//! modèle exact de `Node::annoncer_etat_contact` (`friends.rs`) : un message
//! adressé à NOTRE PROPRE COMPTE, que la couche réseau développe en un envoi
//! par appareil joignable.

use accord_core::prefs::{self, SyncedPref};
use accord_proto::core_msg::CoreMsg;
use serde_json::json;

use crate::error::NodeError;
use crate::outbound::Outbound;

use super::{now_ms, Node};

impl Node {
    /// Toutes les préférences de compte connues de cette machine.
    pub fn synced_prefs(&self) -> Result<Vec<SyncedPref>, NodeError> {
        self.with_db(|db| Ok(prefs::list(db)?))
    }

    /// Enregistre une préférence décidée ICI, et l'annonce aux autres appareils
    /// du compte. Rend l'horodatage retenu, que l'interface conserve pour
    /// savoir, au prochain démarrage, si la valeur du nœud est plus récente que
    /// la sienne.
    pub fn set_synced_pref(&self, key: &str, value: &str) -> Result<u64, NodeError> {
        let at_ms = now_ms();
        self.with_db(|db| Ok(prefs::set_local(db, key, value, at_ms)?))?;
        self.annoncer_preference(key, value, at_ms);
        Ok(at_ms)
    }

    /// Annonce une préférence aux AUTRES appareils du compte.
    ///
    /// Sans effet observable en cas d'échec, pour la même raison que
    /// `annoncer_etat_contact` : le réglage a déjà pris sur la machine que
    /// l'utilisateur regarde, et faire échouer son clic parce qu'une annonce
    /// n'a pas pu partir signalerait un problème là où il n'y en a pas. Un
    /// appareil éteint rattrape à la reconnexion (file hors-ligne par
    /// appareil), ou reste en retard.
    fn annoncer_preference(&self, key: &str, value: &str, at_ms: u64) {
        self.outbound.send(Outbound::Core {
            // Adressé au COMPTE : la couche réseau développe en un envoi par
            // appareil joignable, nous exclus.
            to: self.public_key(),
            msg: Box::new(CoreMsg::SelfPref {
                key: key.as_bytes().to_vec(),
                value: value.as_bytes().to_vec(),
                at_ms,
            }),
        });
    }

    /// Applique une préférence annoncée par un autre appareil du compte.
    ///
    /// 🔒 Trois filtres, dans cet ordre, et chacun compte :
    ///
    /// 1. **`is_own_device`** — la décision se prend sur la clé que la session
    ///    Noise a authentifiée, **jamais** sur quoi que ce soit du contenu.
    ///    Sans lui, n'importe quel ami réécrirait nos réglages en nous envoyant
    ///    quelques octets. Même garde que `SelfContactState`.
    /// 2. **Borne d'horloge sur `at_ms`** — rien au-delà de
    ///    `now + MAX_CLOCK_SKEW_MS`, la tolérance que la DHT applique depuis
    ///    toujours et que `store_peer_device_list` applique désormais aussi. La
    ///    résolution de conflit étant « dernier écrivain gagne », une date
    ///    lointaine ne gagne pas une fois : elle gagne **pour toujours**, et
    ///    plus aucun changement légitime ne peut la dépasser. Il ne faut pas un
    ///    attaquant pour en arriver là — une pile CMOS morte ou un fuseau faux
    ///    produisent exactement le même message (`SECURITY.md` §5, items 16-17).
    /// 3. **Liste blanche** (dans `accord_core::prefs`) — une clé inconnue est
    ///    ignorée en silence, ce qui rend le message compatible avec les
    ///    versions antérieures et borne ce qu'un frère buggé peut écrire.
    ///
    /// Une clé ou une valeur non-UTF-8 tombe dans le même silence : elle ne
    /// peut pas venir de notre interface, et elle ne sera de toute façon
    /// jamais dans la liste blanche.
    pub(super) fn ingest_self_pref(
        &self,
        device_pubkey: &[u8; 32],
        key: &[u8],
        value: &[u8],
        at_ms: u64,
    ) -> Result<(), NodeError> {
        if !self.is_own_device(device_pubkey) {
            return Ok(());
        }
        if at_ms > now_ms().saturating_add(accord_dht::store::MAX_CLOCK_SKEW_MS) {
            tracing::warn!("préférence datée dans le futur : refusée");
            return Ok(());
        }
        let (Ok(key), Ok(value)) = (std::str::from_utf8(key), std::str::from_utf8(value)) else {
            return Ok(());
        };
        if !self.with_db(|db| Ok(prefs::apply_remote(db, key, value, at_ms)?))? {
            return Ok(());
        }
        self.emit(
            "event.self_pref",
            json!({ "key": key, "value": value, "at_ms": at_ms }),
        );
        Ok(())
    }
}
