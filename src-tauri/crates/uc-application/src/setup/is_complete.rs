//! `IsSetupComplete` — read-side sibling of [`MarkSetupComplete`].
//!
//! Answers "has Space setup completed on this profile?" by reading the
//! persisted `SetupStatus` via `SetupStatusPort`. Used by the CLI
//! `start` command to gate daemon launch without building the full
//! runtime or touching the keychain.
//!
//! Returning a plain `bool` (instead of `SetupStatus`) keeps the
//! external contract tight — callers outside the application layer
//! shouldn't care about the `space_id` or future fields of
//! `SetupStatus`. When more is needed later, add a sibling `Get*`
//! query that returns the full struct.

use std::sync::Arc;

use uc_core::ports::SetupStatusPort;

/// Query use case: is Space setup complete on this profile?
pub struct IsSetupComplete {
    setup_status: Arc<dyn SetupStatusPort>,
}

impl IsSetupComplete {
    pub fn new(setup_status: Arc<dyn SetupStatusPort>) -> Self {
        Self { setup_status }
    }

    /// Accept-any-port alias mirroring `MarkSetupComplete::from_ports`
    /// so callers can follow the same construction pattern.
    pub fn from_ports(setup_status: Arc<dyn SetupStatusPort>) -> Self {
        Self::new(setup_status)
    }

    /// Returns `Ok(true)` when `SetupStatus.has_completed` is `true`.
    /// Propagates the port's `anyhow::Error` on read failure — callers
    /// decide whether to treat I/O error as "not complete" or to bubble
    /// up (CLI `start` bubbles so operators see the real problem).
    pub async fn execute(&self) -> anyhow::Result<bool> {
        Ok(self.setup_status.get_status().await?.has_completed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setup::testing::FakeSetupStatus;

    #[tokio::test]
    async fn returns_true_when_status_completed() {
        let port = FakeSetupStatus::completed();
        let uc = IsSetupComplete::from_ports(port);
        assert!(uc.execute().await.unwrap());
    }

    #[tokio::test]
    async fn returns_false_when_status_default() {
        let port = FakeSetupStatus::default_not_completed();
        let uc = IsSetupComplete::from_ports(port);
        assert!(!uc.execute().await.unwrap());
    }
}
