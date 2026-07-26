//! État du chiffrement, tel que l'utilisateur peut le voir et le régler
//! (jalon 2, lots 2.C et 2.D).
//!
//! Deux choses seulement vivent ici : la préférence persistée « exiger
//! l'hybride post-quantique » et la lecture de ce que le nœud constate
//! réellement. Tout le reste — négociation, transcript, dérivation — est dans
//! `accord-crypto` et `accord-transport` ; ce module ne fait que rendre lisible
//! ce qu'ils décident.
//!
//! 🔒 Rien de ce que ce module produit ne quitte la machine. La proportion de
//! sessions hybrides est un compteur LOCAL : elle répond à « où en suis-je ? »
//! sans qu'aucun pair, aucun relais et aucun serveur n'apprenne quoi que ce
//! soit. Il n'y a pas de télémétrie à désactiver, parce qu'il n'y en a pas.

use serde::Serialize;

use crate::error::NodeError;

use super::Node;

/// Clé `meta` de la préférence d'exigence post-quantique.
pub(crate) const META_REQUIRE_PQ: &str = "security.require_pq";

/// Contrat JSON de `security.state` (champs additifs uniquement).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SecurityState {
    /// Ce nœud sait-il négocier l'hybride post-quantique ? Constante à vrai
    /// depuis le jalon 2 ; exposée quand même, pour que l'interface n'ait pas à
    /// déduire une capacité d'un numéro de version.
    pub hybrid_supported: bool,
    /// Ce nœud REFUSE-t-il les sessions classiques (réglage avancé) ?
    pub require_hybrid: bool,
    /// Sessions hybrides établies depuis le démarrage du nœud.
    pub hybrid_sessions: u64,
    /// Sessions classiques établies depuis le démarrage du nœud.
    pub classic_sessions: u64,
}

/// Lit la préférence persistée. Absente — cas de tout profil antérieur au
/// jalon 2 — vaut faux : le défaut est « accepter les deux, préférer
/// l'hybride », et il ne doit pas dépendre d'un enregistrement présent.
pub(crate) fn read_require_pq(db: &accord_core::db::Db) -> Result<bool, NodeError> {
    Ok(db
        .meta(META_REQUIRE_PQ)?
        .is_some_and(|v| v.first().copied() == Some(1)))
}

impl Node {
    /// État du chiffrement pour l'écran Sécurité.
    pub fn security_state(&self) -> Result<SecurityState, NodeError> {
        // La vérité courante de l'exigence est celle du TRANSPORT, pas celle de
        // la base : le réglage prend effet à chaud, et les deux ne coïncident
        // qu'après une écriture réussie. Sans runtime réseau (outils, tests),
        // la préférence persistée est la seule réponse possible.
        let (require_hybrid, hybrid_sessions, classic_sessions) = match self.network_control() {
            Some(ctrl) => {
                let c = ctrl.counters().handshake;
                (ctrl.requires_post_quantum(), c.hybrid, c.classic)
            }
            None => (self.with_db(read_require_pq)?, 0, 0),
        };
        Ok(SecurityState {
            hybrid_supported: true,
            require_hybrid,
            hybrid_sessions,
            classic_sessions,
        })
    }

    /// Pose (ou lève) l'exigence d'hybride et rend l'état à jour.
    ///
    /// Persiste D'ABORD, applique ensuite : si l'écriture échoue, le transport
    /// n'est pas touché et l'utilisateur voit l'échec, plutôt qu'un réglage qui
    /// paraît actif et disparaît au redémarrage suivant.
    pub fn set_require_hybrid(&self, require: bool) -> Result<SecurityState, NodeError> {
        self.with_db(|db| Ok(db.set_meta(META_REQUIRE_PQ, &[u8::from(require)])?))?;
        if let Some(ctrl) = self.network_control() {
            ctrl.set_require_post_quantum(require);
        }
        self.security_state()
    }
}
