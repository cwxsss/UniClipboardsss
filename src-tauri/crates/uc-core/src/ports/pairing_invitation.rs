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
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Opaque invitation credential shown to the joiner (short code / ticket).
///
/// Format and validation rules live in the adapter (Slice 1 decision Q-ε).
/// `uc-core` only treats it as an identifier passed between sponsor and
/// joiner.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InvitationCode(String);

impl InvitationCode {
    /// Wrap an adapter-provided string without performing format validation.
    ///
    /// Core trusts the adapter to have validated the code on the wire; this
    /// constructor exists so use cases can carry the value through domain
    /// types without reaching for a `String`.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl std::fmt::Display for InvitationCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

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
