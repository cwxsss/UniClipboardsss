//! Slice 1 iroh-native pairing infrastructure.
//!
//! * [`wire`] — binary codec for [`PairingSessionMessage`] using postcard.
//!
//! Future children (P7c.2 / P7c.3):
//! * `session` — `IrohPairingSessionAdapter` implementing `PairingSessionPort`
//!   and `PairingEventPort` on top of an iroh `Endpoint`.
//!
//! [`PairingSessionMessage`]: uc_core::pairing::PairingSessionMessage

pub mod wire;

pub use wire::{decode, encode, WireDecodeError, WireEncodeError};
