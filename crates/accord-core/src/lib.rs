//! Logique applicative d'Accord (SPEC §5-§9).
//!
//! Ce crate ne parle pas au réseau : il transforme des événements (messages
//! reçus, actions utilisateur) en état persistant (base SQLCipher) et en
//! messages à émettre. Les couches transport/DHT injectent et consomment.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod db;
pub mod error;
pub mod files;
pub mod friends;
pub mod group;
pub mod mentions;
pub mod messaging;
pub mod offline;
pub mod presence;
pub mod profile;
pub mod search;

pub use db::Db;
pub use error::CoreError;
