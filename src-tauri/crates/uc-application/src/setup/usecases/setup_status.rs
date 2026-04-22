//! Setup-status query + command use cases.
//!
//! Two thin wrappers over [`SetupStatusPort`] that answer the domain-level
//! questions "has Space setup completed?" ([`IsSetupCompleteUsecase`]) and
//! "mark it as complete" ([`MarkSetupCompleteUsecase`]). Kept together in
//! one file because they share the same port and neither warrants its own
//! module; splitting them invited drift between the sibling use cases.
//!
//! Per `uc-application/AGENTS.md` §11.4 both are `pub(crate)` — external
//! callers (bootstrap / CLI / future GUI) reach them exclusively through
//! [`crate::facade::SetupStatusFacade`].

use std::sync::Arc;

use uc_core::ports::SetupStatusPort;

/// Query: is Space setup complete on this profile?
///
/// Returning a plain `bool` (instead of `SetupStatus`) keeps the external
/// contract tight — callers outside the application layer shouldn't care
/// about `space_id` or future `SetupStatus` fields. When more is needed
/// later, add a sibling `Get*` query that returns the full struct.
pub(crate) struct IsSetupCompleteUsecase {
    setup_status: Arc<dyn SetupStatusPort>,
}

impl IsSetupCompleteUsecase {
    pub(crate) fn new(setup_status: Arc<dyn SetupStatusPort>) -> Self {
        Self { setup_status }
    }

    /// Returns `Ok(true)` when `SetupStatus.has_completed` is `true`.
    /// Propagates the port's `anyhow::Error` on read failure — callers
    /// decide whether to treat I/O error as "not complete" or to bubble
    /// up (CLI `start` bubbles so operators see the real problem).
    pub(crate) async fn execute(&self) -> anyhow::Result<bool> {
        Ok(self.setup_status.get_status().await?.has_completed)
    }
}

/// Command: flip `SetupStatus.has_completed` to `true`.
///
/// Read-modify-write against [`SetupStatusPort`] to preserve any other
/// fields the port already persists (e.g. `space_id`).
pub(crate) struct MarkSetupCompleteUsecase {
    setup_status: Arc<dyn SetupStatusPort>,
}

impl MarkSetupCompleteUsecase {
    pub(crate) fn new(setup_status: Arc<dyn SetupStatusPort>) -> Self {
        Self { setup_status }
    }

    pub(crate) async fn execute(&self) -> anyhow::Result<()> {
        let mut status = self.setup_status.get_status().await?;
        status.has_completed = true;
        self.setup_status.set_status(&status).await
    }
}
