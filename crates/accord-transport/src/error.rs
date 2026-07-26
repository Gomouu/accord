//! Erreurs typées de la couche transport.

use thiserror::Error;

/// Erreur transport.
#[derive(Debug, Error)]
pub enum TransportError {
    /// Erreur d'entrée/sortie du socket.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// Erreur cryptographique (handshake/session).
    #[error("crypto: {0}")]
    Crypto(#[from] accord_crypto::CryptoError),
    /// Erreur d'encodage/décodage protocolaire.
    #[error("proto: {0}")]
    Proto(#[from] accord_proto::DecodeError),
    /// Aucune session établie et pas d'adresse pour en ouvrir une.
    #[error("pair inconnu")]
    UnknownPeer,
    /// Message trop volumineux, même après fragmentation (> 1 MiB).
    #[error("message trop grand pour la session")]
    TooLarge,
    /// L'identité authentifiée du pair ne correspond pas à la cible attendue :
    /// le handshake (ou la session existante) n'est pas lié à l'identité visée
    /// (liaison d'identité, SPEC §2.2 ; MITM on-path déjoué). La file destinée
    /// à la cible n'est jamais scellée sous cette session.
    #[error("identité du pair inattendue")]
    PeerIdentityMismatch,
    /// Réassemblage de fragments incohérent ou hors des bornes anti-DoS.
    #[error("réassemblage: {0}")]
    Reassembly(&'static str),
    /// Le relais a refusé l'ouverture d'un circuit client (SPEC §10). Le code
    /// est celui du `RelayMsg::Reject` (voir `REJECT_*` dans l'endpoint).
    #[error("ouverture de circuit relais refusée (code {0:#04x})")]
    RelayOpenRejected(u8),
    /// Le relais est resté silencieux : ni `Accept` ni `Reject` reçu dans le
    /// délai imparti. L'ouverture est abandonnée et l'attente purgée (pas de
    /// pendaison ni de fuite d'entrée `pending_relay_open`).
    #[error("ouverture de circuit relais expirée (relais silencieux)")]
    RelayOpenTimeout,
    /// La session négociée est classique alors que la configuration locale
    /// EXIGE l'hybride post-quantique (réglage avancé, lot 2.D). Le handshake
    /// est authentifié et valide : c'est une politique locale qui le refuse,
    /// pas une faute du pair. Aucune session n'est installée.
    #[error("session classique refusée : l'hybride post-quantique est exigé")]
    PostQuantumRequired,
    /// Endpoint en cours d'arrêt.
    #[error("endpoint arrêté")]
    Shutdown,
}
