use std::sync::Arc;

use anyhow::Result;

use crate::pairing::orchestrator::PairingOrchestrator;

/// Application-layer use case for "user confirmed the short-code matches"
/// on the pairing flow (phase 0.4.4 / D22 thin wrapper).
///
/// Internal to the pairing module: external consumers drive this via
/// [`super::super::facade::PairingFacade::accept_pairing`]. Kept as a
/// discrete type so the user intent is named and independently testable,
/// but intentionally `pub(crate)` to enforce the
/// `External → Facade → Orchestrator → Ports` boundary set by AGENTS.md §11.
pub(crate) struct AcceptPairingUseCase {
    orchestrator: Arc<PairingOrchestrator>,
}

impl AcceptPairingUseCase {
    pub(crate) fn new(orchestrator: Arc<PairingOrchestrator>) -> Self {
        Self { orchestrator }
    }

    pub(crate) async fn execute(&self, session_id: &str) -> Result<()> {
        self.orchestrator.user_accept_pairing(session_id).await
    }
}
