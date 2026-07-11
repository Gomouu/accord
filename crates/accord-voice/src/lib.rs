//! Voix full-mesh chiffrée d'Accord (SPEC §8).
//!
//! Ce crate contient toute la logique voix en Rust pur, testable sans
//! matériel : tampon de gigue adaptatif, estimation de perte, débit adaptatif,
//! détection d'activité vocale, séquencement et état de salon full-mesh. Le
//! codec passe par le trait [`AudioCodec`] ([`PassthroughCodec`] pour les
//! tests) ; la vraie liaison Opus et la capture/lecture `cpal` s'activent avec
//! la feature `hardware` (voir D-020). Le chiffrement des trames est déjà
//! fourni par les sessions transport (canal VOICE).

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod bitrate;
pub mod codec;
pub mod convert;
pub mod dsp;
pub mod gain;
#[cfg(feature = "hardware")]
pub mod io;
pub mod jitter;
pub mod loss;
pub mod params;
pub mod room;
pub mod vad;

pub use codec::{AudioCodec, CodecError, PassthroughCodec, Pcm8Codec};
pub use jitter::{JitterBuffer, Playout};
pub use loss::LossEstimator;
pub use room::{RoomError, VoiceRoom};
pub use vad::Vad;

#[cfg(feature = "hardware")]
pub use codec::OpusCodec;
#[cfg(feature = "hardware")]
pub use io::{AudioInput, AudioOutput, IoError};
