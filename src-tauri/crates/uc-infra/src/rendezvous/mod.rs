//! Rendezvous service client (Slice 1 · P7a).
//!
//! Backs [`uc_core::ports::PairingInvitationPort`] via the public rendezvous
//! HTTP service. The service is an opaque meeting-point: sponsor posts its
//! iroh address, gets a short code, joiner looks it up out-of-band.

pub mod client;

pub use client::{RendezvousPairingInvitationAdapter, RENDEZVOUS_BASE_URL};
