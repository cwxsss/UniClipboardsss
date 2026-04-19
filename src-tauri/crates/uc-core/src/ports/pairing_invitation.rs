//! Pairing invitation port.
//!
//! Sponsor-side capability for issuing a short-lived invitation credential
//! that a joiner can redeem to find and dial the sponsor. The concrete
//! adapter (Slice 1: rendezvous HTTP client) owns the TTL policy, the
//! transport to the rendezvous service, and the on-wire code format.
//!
//! Scope of this port intentionally stops at "get me a code I can display"
//! — joiner-side redeem + dial lives on [`PairingSessionPort`] (defined in
//! Slice 1 P8 alongside the iroh adapter).

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use thiserror::Error;

pub use crate::pairing::invitation::InvitationCode;

/// Successfully issued invitation.
#[derive(Debug, Clone)]
pub struct IssuedInvitation {
    /// Code the joiner enters.
    pub code: InvitationCode,
    /// Server-authoritative expiry (decision Q-B1-1).
    pub expires_at: DateTime<Utc>,
}

/// Errors produced while issuing an invitation.
#[derive(Debug, Error)]
pub enum InvitationError {
    /// Adapter couldn't reach its transport (e.g. iroh endpoint not started).
    /// Surfaced to UI as "start network first".
    #[error("network is not started")]
    NetworkNotStarted,

    /// Rendezvous service unreachable / returned a transient failure.
    #[error("pairing invitation service unavailable")]
    ServiceUnavailable,

    /// Unexpected adapter-side failure; message is for logs only.
    #[error("internal invitation error: {0}")]
    Internal(String),
}

/// Sponsor-side invitation issuance.
#[async_trait]
pub trait PairingInvitationPort: Send + Sync {
    /// Request a fresh invitation code. The adapter decides TTL and code
    /// format; callers treat the returned `expires_at` as ground truth.
    async fn issue_invitation(&self) -> Result<IssuedInvitation, InvitationError>;
}
