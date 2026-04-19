//! Command / result payloads for the Slice 1 application facade.
//!
//! Each pair models one external-facing application action; keep them free
//! of cross-cutting domain types and do not add query shape here.

use uc_core::crypto::domain::Passphrase;
use uc_core::ids::{DeviceId, SpaceId};
use uc_core::security::IdentityFingerprint;

// ---------------------------------------------------------------------------
// A1 · InitializeSpace
// ---------------------------------------------------------------------------

/// Input to [`crate::facade::usecases::InitializeSpaceUseCase`].
#[derive(Debug)]
pub struct InitializeSpaceCommand {
    /// User-entered passphrase protecting the new space.
    pub passphrase: Passphrase,
    /// Confirmation copy — must equal [`passphrase`](Self::passphrase).
    pub passphrase_confirm: Passphrase,
    /// Display name for this device as seen by future members.
    ///
    /// * `Some(name)` — persist to `Settings.general.device_name` and use
    ///   for the owner `SpaceMember` record.
    /// * `None` — fall back to the currently-persisted `device_name`;
    ///   caller-level UI must have collected it beforehand.
    pub device_name: Option<String>,
}

/// Output of a successful A1 initialise.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitializeSpaceResult {
    pub space_id: SpaceId,
    pub self_device_id: DeviceId,
    pub fingerprint: IdentityFingerprint,
}

// ---------------------------------------------------------------------------
// A2 · UnlockSpace
// ---------------------------------------------------------------------------

/// Input to [`crate::facade::usecases::UnlockSpaceUseCase`].
#[derive(Debug)]
pub struct UnlockSpaceCommand {
    pub passphrase: Passphrase,
}

/// Output of a successful A2 unlock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnlockSpaceResult {
    pub space_id: SpaceId,
}
