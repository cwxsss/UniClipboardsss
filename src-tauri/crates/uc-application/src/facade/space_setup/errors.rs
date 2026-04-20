//! Application-layer errors for the Slice 1 facade.

use thiserror::Error;

/// Failure modes of A1 `InitializeSpaceUseCase`.
///
/// Kept narrower than the ports' native error types so callers can branch
/// on **what action to take next** (ask user again / surface a support
/// message / crash-logs) without having to understand cryptographic
/// details.
#[derive(Debug, Error)]
pub enum InitializeSpaceError {
    /// `passphrase` and `passphrase_confirm` differed. UI should keep the
    /// user on the current form.
    #[error("passphrase and confirmation do not match")]
    PassphraseMismatch,

    /// No device name available — neither in the command nor in
    /// `Settings.general.device_name`.
    #[error("device name is required but not provided")]
    DeviceNameRequired,

    /// The local space has already been initialised. User should unlock
    /// (A2) instead, or run a factory reset first.
    #[error("space is already initialised")]
    AlreadyInitialized,

    /// A local identity already exists (previous A1/B2 run left state).
    /// Current policy is loud failure so data inconsistencies are caught;
    /// the joiner path uses `ensure()` where retry is expected.
    #[error("local identity already exists")]
    IdentityAlreadyExists,

    /// Failed to read or persist settings / membership / setup-status —
    /// message carries adapter-level context for logs.
    #[error("storage failure: {0}")]
    StorageFailed(String),

    /// Any other uncategorised failure (adapter internal / infra-layer
    /// bug). Treat as fatal for the current action.
    #[error("internal error: {0}")]
    Internal(String),
}

/// Failure modes of B1 `IssuePairingInvitationUseCase`.
///
/// Mirrors
/// [`uc_core::ports::pairing_invitation::InvitationError`] at the
/// application boundary, keeping the upstream-port variant names so UI
/// can branch on intent ("start network" vs. "retry later") without
/// having to import the infra-port enum.
#[derive(Debug, Error)]
pub enum IssuePairingInvitationError {
    /// Underlying network runtime has not been started. UI should surface
    /// "start network first" (A1/A2 completing auto-starts it, so this
    /// typically means startup failed earlier and the user needs to retry).
    #[error("network is not started")]
    NetworkNotStarted,

    /// Rendezvous service unreachable / transient failure. UI may offer a
    /// manual retry.
    #[error("pairing invitation service unavailable")]
    ServiceUnavailable,

    /// Uncategorised adapter-side failure; message for logs only.
    #[error("internal error: {0}")]
    Internal(String),
}

/// Failure modes of A2 `UnlockSpaceUseCase`.
#[derive(Debug, Error)]
pub enum UnlockSpaceError {
    /// Setup has not been completed — there is no space to unlock yet.
    #[error("setup has not been completed")]
    SetupNotCompleted,

    /// Space exists only logically (setup marked complete) but the
    /// underlying keyslot is missing / corrupted.
    #[error("space is not initialised")]
    SpaceNotInitialized,

    /// Passphrase did not unwrap the stored master key.
    #[error("wrong passphrase")]
    WrongPassphrase,

    /// Stored keyslot was corrupted or in an unsupported format.
    #[error("space key material corrupted")]
    CorruptedKeyMaterial,

    /// Uncategorised infra / adapter failure.
    #[error("internal error: {0}")]
    Internal(String),
}
