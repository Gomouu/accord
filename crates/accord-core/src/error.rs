//! Erreurs typées de la couche cœur.

use accord_proto::wire::DecodeError;

/// Erreur de la logique applicative.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// Erreur de base de données locale.
    #[error("base locale : {0}")]
    Db(#[from] rusqlite::Error),
    /// Erreur cryptographique (scellement, clé de groupe, signature).
    #[error("cryptographie : {0}")]
    Crypto(#[from] accord_crypto::CryptoError),
    /// Décodage filaire invalide.
    #[error("décodage : {0}")]
    Decode(#[from] DecodeError),
    /// Opération de groupe rejetée (permissions ou état incohérent).
    #[error("opération de groupe rejetée : {0}")]
    OpRejected(&'static str),
    /// Entité introuvable (groupe, contact, message, fichier).
    #[error("introuvable : {0}")]
    NotFound(&'static str),
    /// Entrée invalide (bornes, format).
    #[error("entrée invalide : {0}")]
    Invalid(&'static str),
    /// Erreur d'entrées/sorties (fichiers partagés).
    #[error("e/s : {0}")]
    Io(#[from] std::io::Error),
    /// Erreur de codage Reed-Solomon.
    #[error("reed-solomon : {0}")]
    Fec(String),
    /// La base locale a été écrite par une version plus récente de
    /// l'application. Démarrer dessus corromprait des données que ce binaire
    /// ne sait pas interpréter : on refuse plutôt que d'essayer.
    #[error(
        "cette base a été créée par une version plus récente d'Accord \
         (schéma {found}, ce binaire gère {supported}) : mettez l'application à jour"
    )]
    SchemaTooNew {
        /// Version de schéma trouvée dans la base.
        found: i64,
        /// Version maximale que ce binaire sait lire.
        supported: i64,
    },
}
