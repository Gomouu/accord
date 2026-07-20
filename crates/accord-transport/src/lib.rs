//! # accord-transport
//!
//! Couche transport d'Accord (SPEC §1, §2.5, §10, §11) au-dessus d'un socket
//! datagramme abstrait :
//!
//! - [`socket`] : trait [`socket::DatagramSocket`], implémentation UDP réelle
//!   et mesh simulé déterministe pour les tests d'intégration ;
//! - [`endpoint`] : pilote les handshakes, sessions chiffrées, keep-alive,
//!   mobilité réseau et anti-DoS ; remonte des [`endpoint::TransportEvent`] ;
//! - [`ratelimit`] : token buckets par IP source ;
//! - [`relay`] : circuits de relais opaques avec plafond de bande passante ;
//! - [`nat`] : agrégation de candidats et détection de NAT symétrique ;
//! - [`tcp`] : repli TCP (datagrammes encadrés, poinçonnage par ouverture
//!   simultanée, multiplexage transparent avec le socket UDP) ;
//! - [`clock`] : horloge abstraite (réelle / manuelle pour les tests).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod clock;
pub mod endpoint;
pub mod error;
mod frag;
pub mod nat;
pub mod ratelimit;
pub mod relay;
pub mod socket;
pub mod tcp;

pub use clock::{Clock, ManualClock, SystemClock};
pub use endpoint::{Endpoint, EndpointConfig, SessionView, TransportEvent};
pub use error::TransportError;
pub use ratelimit::RateLimiter;
pub use socket::{DatagramSocket, UdpDatagram};
pub use tcp::{MuxSocket, TcpLinks};
