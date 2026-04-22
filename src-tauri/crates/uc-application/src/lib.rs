//! Application-layer workflows for UniClipboard.

pub mod clipboard_capture;
pub mod clipboard_write;
pub mod facade;

// Slice 2 Phase 3 · T4 — public use case consumed directly by daemon
// `InboundClipboardSyncWorker` (T8). Lives inside `usecases::clipboard_sync`
// (which is `pub(crate)`) so Phase 2 internals stay encapsulated; we
// re-export only the small public surface here.
pub use usecases::clipboard_sync::{
    ApplyInboundClipboardUseCase, ApplyInboundError, ApplyInboundInput, ApplyOutcome,
    InboundCapture, InboundWrite,
};
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
