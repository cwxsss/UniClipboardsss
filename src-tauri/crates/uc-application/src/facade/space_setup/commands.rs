//! Command / result payloads for the Slice 1 application facade.
//!
//! Each pair models one external-facing application action; keep them free
//! of cross-cutting domain types and do not add query shape here.

use chrono::{DateTime, Utc};

use uc_core::crypto::domain::Passphrase;
use uc_core::ids::{DeviceId, SpaceId};
use uc_core::pairing::InvitationCode;
use uc_core::security::IdentityFingerprint;

// ---------------------------------------------------------------------------
// A1 · InitializeSpace
// ---------------------------------------------------------------------------

/// Input to [`crate::usecases::setup::initialize_space::InitializeSpaceUseCase`].
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

/// Input to [`crate::usecases::setup::unlock_space::UnlockSpaceUseCase`].
#[derive(Debug)]
pub struct UnlockSpaceCommand {
    pub passphrase: Passphrase,
}

/// Output of a successful A2 unlock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnlockSpaceResult {
    pub space_id: SpaceId,
}

// ---------------------------------------------------------------------------
// B1 · IssuePairingInvitation
// ---------------------------------------------------------------------------

/// Output of a successful B1 invitation issuance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssuePairingInvitationResult {
    /// Short human-typable code the sponsor shows to the joiner.
    pub code: InvitationCode,
    /// Server-authoritative expiry; UI should display a countdown from
    /// this value rather than computing its own.
    pub expires_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// B2 · RedeemPairingInvitation  (joiner side)
// ---------------------------------------------------------------------------

/// Input to [`crate::usecases::pairing::redeem_invitation::RedeemPairingInvitationUseCase`].
///
/// Joiner-side UX gathers both fields up front: the user types the
/// invitation code the sponsor shared and the space passphrase the sponsor
/// chose during A1. Slice 1 does not support a two-step flow where the
/// passphrase is entered after receiving the keyslot offer.
#[derive(Debug)]
pub struct RedeemPairingInvitationCommand {
    /// Invitation code the user typed (or scanned from the sponsor's UI).
    pub code: InvitationCode,
    /// Same passphrase the sponsor used in A1 `InitializeSpace`.
    pub passphrase: Passphrase,
}

// ---------------------------------------------------------------------------
// Setup state query (Slice4 P3 T3.2)
// ---------------------------------------------------------------------------

/// Read-only view of setup state surfaced by
/// [`crate::facade::space_setup::SpaceSetupFacade::query_setup_state`].
///
/// Replaces the legacy stateful FSM snapshot exposed via
/// `SetupFacade::get_state`. The new shape carries only what the
/// stateless v2 UI flow needs: whether onboarding is done, what
/// invitation (if any) is currently parked on the sponsor side, and
/// the local device name to prefill confirmation copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupStateView {
    /// `true` when this device has completed A1 (`InitializeSpace`) or
    /// B2 (`RedeemPairingInvitation`); `false` on a fresh install.
    pub has_completed: bool,
    /// `Some(_)` when the sponsor has a Pending invitation parked in
    /// the in-memory holder; `None` when there is no in-flight code.
    /// Multi-pending policy is "earliest-expiring wins".
    pub current_invitation: Option<CurrentInvitation>,
    /// Display name persisted in `Settings.general.device_name`, or
    /// `None` when unset on a fresh install.
    pub device_name: Option<String>,
}

/// Companion to [`SetupStateView::current_invitation`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrentInvitation {
    pub code: InvitationCode,
    pub expires_at: DateTime<Utc>,
}

/// Output of a successful B2 redemption.
///
/// Returned fields let the UI show a "you are connected to X" confirmation
/// without having to re-read the freshly-persisted `SpaceMember` /
/// `TrustedPeer` rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedeemPairingInvitationResult {
    /// Sponsor device now persisted locally as both `SpaceMember` and
    /// `TrustedPeer`.
    pub sponsor_device_id: DeviceId,
    /// Sponsor's stable identity fingerprint (F-036 concept 2).
    pub sponsor_identity_fingerprint: IdentityFingerprint,
    /// Sponsor's space id, adopted as the joiner's local space id.
    pub space_id: SpaceId,
    /// This device's own id, as persisted on the sponsor side through the
    /// in-flight `JoinerRequest` — surfaced here so the UI does not need
    /// to query `DeviceIdentityPort` separately for the confirmation
    /// screen.
    pub self_device_id: DeviceId,
    /// This device's stable identity fingerprint.
    pub self_identity_fingerprint: IdentityFingerprint,
}
