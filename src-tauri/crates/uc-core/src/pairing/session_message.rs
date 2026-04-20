//! Slice 1 pairing session-level domain messages.
//!
//! Pure domain types carried by [`PairingSessionPort`] and surfaced by
//! [`PairingEventPort`]. Adapters own wire encoding — these types have no
//! `serde` derives, no protocol ids, no libp2p / iroh leakage.
//!
//! Shape tracks the Slice 1 handshake:
//!
//! ```text
//!   Joiner → Sponsor : Request
//!   Sponsor → Joiner : KeyslotOffer
//!   Joiner → Sponsor : ChallengeResponse
//!   Sponsor → Joiner : Confirm      (or Reject at any step, either side)
//! ```
//!
//! Legacy libp2p-era equivalents live in [`crate::network::protocol::pairing`]
//! and carry a different — PIN-based, `peer_id`-leaky — shape. Slice 5 will
//! delete that module together with the libp2p adapter.
//!
//! [`PairingSessionPort`]: crate::ports::pairing::PairingSessionPort
//! [`PairingEventPort`]: crate::ports::pairing::PairingEventPort

use super::invitation::InvitationCode;
use crate::ids::{DeviceId, SpaceId};
use crate::ports::pairing::PairingSessionId;
use crate::security::IdentityFingerprint;

/// All pairing session-level messages for the Slice 1 iroh-native flow.
#[derive(Debug, Clone)]
pub enum PairingSessionMessage {
    Request(JoinerRequest),
    KeyslotOffer(SponsorKeyslotOffer),
    ChallengeResponse(JoinerChallengeResponse),
    Confirm(SponsorConfirm),
    Reject(PairingReject),
}

/// Joiner → sponsor. First message on the bi-stream (B2 step 5).
#[derive(Debug, Clone)]
pub struct JoinerRequest {
    /// Code the joiner redeemed. Sponsor orchestrator matches it against
    /// the in-memory pending invitation (Q-B1-3 / F-041).
    pub invitation_code: InvitationCode,
    /// Joiner's stable business device id (F-036 concept 1).
    pub device_id: DeviceId,
    /// Joiner's device name for sponsor-side UI / persistence.
    pub device_name: String,
    /// Joiner's identity fingerprint (F-036 concept 2). Derived at the
    /// adapter from the Ed25519 pubkey used by the session's transport.
    pub identity_fingerprint: IdentityFingerprint,
    /// Handshake transcript nonce.
    pub nonce: Vec<u8>,
}

/// Sponsor → joiner. Hands the joiner an offer they can unseal with the
/// shared passphrase (B2 step 6).
#[derive(Debug, Clone)]
pub struct SponsorKeyslotOffer {
    /// The space this offer belongs to.
    pub space_id: SpaceId,
    /// Opaque keyslot payload. Infra serializes the historical
    /// `KeySlotFile` JSON here; core treats the blob as bytes.
    pub keyslot_blob: Vec<u8>,
    /// 32-byte challenge nonce the joiner combines with the derived
    /// master key and `pairing_session_id` to compute an HMAC proof
    /// ([`ProofPort::build_proof`](crate::ports::space::ProofPort)).
    /// Sponsor keeps a copy in per-session state and feeds the same
    /// value to `verify_proof` on receipt.
    pub challenge: Vec<u8>,
    /// Sponsor-minted session identifier replayed verbatim into the
    /// joiner's proof payload so the sponsor-side `verify_proof` can
    /// bind the HMAC to the live pairing session (replay defence).
    pub pairing_session_id: PairingSessionId,
}

/// Joiner → sponsor. Challenge decrypt proof (B2 step 8).
#[derive(Debug, Clone)]
pub struct JoinerChallengeResponse {
    pub encrypted_challenge: Vec<u8>,
}

/// Sponsor → joiner. Final success message + sponsor identity facts the
/// joiner persists as a `SpaceMember` + `TrustedPeer` (B2 step 9/10).
#[derive(Debug, Clone)]
pub struct SponsorConfirm {
    pub space_id: SpaceId,
    pub sender_device_id: DeviceId,
    pub sender_device_name: String,
    pub sender_identity_fingerprint: IdentityFingerprint,
}

/// Either side → other. Terminal message with a structured reason so the
/// orchestrator can pick the right UI error / `PairingError` variant.
#[derive(Debug, Clone)]
pub struct PairingReject {
    pub reason: PairingRejectReason,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PairingRejectReason {
    /// Sponsor: incoming code didn't match any pending invitation (stale
    /// rendezvous entry or attacker replay).
    InvitationMismatch,
    /// Sponsor: joiner's challenge response didn't decrypt — wrong
    /// passphrase.
    PassphraseMismatch,
    /// Sponsor: user declined (reserved; Slice 1 doesn't surface an
    /// approval prompt but the enum leaves room for it).
    UserRejected,
    /// Protocol-level violation; message is for logs only.
    Internal(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reject_reason_equality_is_structural() {
        assert_eq!(
            PairingRejectReason::InvitationMismatch,
            PairingRejectReason::InvitationMismatch
        );
        assert_ne!(
            PairingRejectReason::Internal("a".into()),
            PairingRejectReason::Internal("b".into())
        );
    }
}
