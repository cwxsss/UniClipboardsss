//! Application-layer workflows for UniClipboard.

pub mod clipboard_capture;
pub mod facade;
pub mod file_transfer;
pub mod membership;
pub mod pairing;
pub(crate) mod pairing_inbound;
pub(crate) mod pairing_invitation;
pub(crate) mod pairing_outbound;
pub mod setup;
pub mod space_access;
pub mod trusted_peer;
pub(crate) mod usecases;
