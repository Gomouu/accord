//! Actions réseau émises par la logique applicative vers la boucle transport.
//!
//! La couche applicative ([`crate::node::Node`]) ne parle jamais directement
//! au réseau : elle pousse des [`Outbound`] sur un canal borné que la boucle
//! [`crate::runtime`] consomme. En l'absence de runtime (tests unitaires), un
//! puits nul absorbe les actions sans effet.

use accord_proto::core_msg::{CoreMsg, GroupOp};
use tokio::sync::mpsc;

/// Action réseau à exécuter par la boucle transport.
#[derive(Debug)]
pub enum Outbound {
    /// Émettre un message CORE vers un pair (clé publique Ed25519).
    Core {
        /// Destinataire.
        to: [u8; 32],
        /// Message à chiffrer et livrer (ou mettre en file si hors ligne).
        msg: Box<CoreMsg>,
    },
    /// Diffuser une op de groupe à tous les membres joignables.
    GroupOp {
        /// Op signée à répliquer.
        op: Box<GroupOp>,
    },
    /// Diffuser un message CORE à tous les membres d'un groupe (sauf soi).
    GroupCast {
        /// Groupe dont les membres sont destinataires.
        group_id: [u8; 16],
        /// Message à livrer à chaque membre.
        msg: Box<CoreMsg>,
    },
    /// (Re)publier un record dans la DHT.
    DhtPublish {
        /// Record signé.
        record: Box<accord_proto::types::DhtRecord>,
    },
    /// Saluer les pairs Accord du réseau local pour un appairage en cours.
    ///
    /// Le message PAKE n'est pas porté ici : c'est le nœud qui le rend, **pair
    /// par pair** ([`crate::node::Node::pairing_hello_for`]), parce que lui
    /// seul tient le compte de qui a déjà été salué. 🔒 Chaque HELLO consomme
    /// une des trois tentatives d'en face ; saluer deux fois le même voisin
    /// brûlerait l'offre qu'un utilisateur regarde à l'écran.
    PairingBroadcast,
}

/// Puits d'actions réseau (extrémité émettrice, clonable).
#[derive(Clone)]
pub struct OutboundSink {
    tx: Option<mpsc::Sender<Outbound>>,
}

impl OutboundSink {
    /// Crée un puits relié à un récepteur borné.
    pub fn channel(capacity: usize) -> (Self, mpsc::Receiver<Outbound>) {
        let (tx, rx) = mpsc::channel(capacity);
        (Self { tx: Some(tx) }, rx)
    }

    /// Puits nul : absorbe tout sans effet (tests sans réseau).
    pub fn null() -> Self {
        Self { tx: None }
    }

    /// Émet une action (best-effort : perdue si le canal est plein ou absent,
    /// la logique applicative a déjà persisté l'état localement).
    pub fn send(&self, action: Outbound) {
        if let Some(tx) = &self.tx {
            if tx.try_send(action).is_err() {
                tracing::warn!("outbound: canal saturé, action réseau différée");
            }
        }
    }
}
