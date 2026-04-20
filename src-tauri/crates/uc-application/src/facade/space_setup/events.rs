//! Sponsor-side pairing lifecycle events broadcast by
//! [`SpaceSetupFacade::subscribe_pairing_completion`].
//!
//! A single `PairingOutcome` fires per matched invitation: either `Success`
//! after admit + trust + Confirm landed, or `Failure` if any post-match step
//! fails (proof mismatch, persistence failure, Confirm send failure, clock
//! out of range, holder invariant break). Stray connections carrying an
//! unknown or expired code do **not** produce an outcome — the listener
//! should remain valid until their own invitation resolves.

use uc_core::ids::DeviceId;
use uc_core::security::IdentityFingerprint;

/// Terminal result of a sponsor-side inbound pairing handshake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PairingOutcome {
    /// Handshake completed and both `SpaceMember` + `TrustedPeer` rows
    /// landed locally; the joiner received the `Confirm` frame.
    Success {
        peer_device_id: DeviceId,
        peer_device_name: String,
        peer_fingerprint: IdentityFingerprint,
    },
    /// Handshake started (invitation matched) but failed before reaching
    /// a committed confirm. `reason` is a human-readable summary suitable
    /// for display and logging — not a structured error variant.
    Failure { reason: String },
}
