//! In-memory [`PairingInvitation`] holder for sponsor-side pending
//! invitations.
//!
//! Slice 1 decision Q-2: invitations live **only** in this holder and are
//! dropped when the process exits. No replacement axis (no "redis holder"
//! etc.) — invitations are intrinsically short-lived (typical TTL ≤ 10
//! minutes) and re-issuing on next launch is acceptable, so we don't need
//! a port here.
//!
//! P7d scope (current phase):
//! * `insert` — called by `IssuePairingInvitationUseCase` after a
//!   successful rendezvous issue.
//!
//! P7e will add a `take_matching(code, now)` path for sponsor-side
//! `Incoming` events to locate and consume the pending aggregate.

use std::collections::HashMap;

use tokio::sync::Mutex;

use uc_core::pairing::invitation::{InvitationCode, PairingInvitation};

/// Process-local map of outstanding [`PairingInvitation`]s keyed by code.
///
/// The code is chosen as the key because the sponsor-side `Incoming`
/// event (P7e) carries only the joiner-echoed code, not the aggregate's
/// internal pointer.
pub(crate) struct InMemoryPairingInvitationHolder {
    by_code: Mutex<HashMap<InvitationCode, PairingInvitation>>,
}

impl InMemoryPairingInvitationHolder {
    pub(crate) fn new() -> Self {
        Self {
            by_code: Mutex::new(HashMap::new()),
        }
    }

    /// Insert (or overwrite) the aggregate keyed by its code.
    ///
    /// Overwrite semantics: a fresh `issue_invitation()` that reuses the
    /// same code (rendezvous adapter decides code uniqueness) replaces the
    /// previous slot. The caller's invariant is "the latest issue wins";
    /// we don't enforce a "single pending per device" rule here because
    /// that's a UI-level policy decision, not a core invariant.
    pub(crate) async fn insert(&self, invitation: PairingInvitation) {
        let code = invitation.code().clone();
        self.by_code.lock().await.insert(code, invitation);
    }

    /// Count of outstanding entries (test-only — not part of the
    /// application-facing surface).
    #[cfg(test)]
    pub(crate) async fn len(&self) -> usize {
        self.by_code.lock().await.len()
    }

    /// Test-only: look up by code without consuming the aggregate.
    #[cfg(test)]
    pub(crate) async fn get_for_test(&self, code: &InvitationCode) -> Option<PairingInvitation> {
        self.by_code.lock().await.get(code).cloned()
    }
}

impl Default for InMemoryPairingInvitationHolder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use chrono::{DateTime, Duration, Utc};

    use uc_core::ids::DeviceId;
    use uc_core::pairing::invitation::InvitationState;

    fn fixed_now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-04-20T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn pending(code: &str) -> PairingInvitation {
        let issued = fixed_now();
        let expires = issued + Duration::minutes(5);
        let (invitation, _) = PairingInvitation::issue(
            InvitationCode::new(code),
            issued,
            expires,
            DeviceId::new("device-1"),
        );
        invitation
    }

    #[tokio::test]
    async fn insert_stores_aggregate_by_code() {
        let holder = InMemoryPairingInvitationHolder::new();
        holder.insert(pending("ABCD-1234")).await;
        assert_eq!(holder.len().await, 1);
        let stored = holder
            .get_for_test(&InvitationCode::new("ABCD-1234"))
            .await
            .expect("aggregate stored");
        assert!(matches!(stored.state(), InvitationState::Pending { .. }));
    }

    #[tokio::test]
    async fn insert_with_same_code_overwrites() {
        let holder = InMemoryPairingInvitationHolder::new();
        holder.insert(pending("SAME")).await;
        holder.insert(pending("SAME")).await;
        assert_eq!(holder.len().await, 1, "overwrite, not duplicate");
    }

    #[tokio::test]
    async fn distinct_codes_coexist() {
        let holder = InMemoryPairingInvitationHolder::new();
        holder.insert(pending("ONE")).await;
        holder.insert(pending("TWO")).await;
        assert_eq!(holder.len().await, 2);
    }
}
