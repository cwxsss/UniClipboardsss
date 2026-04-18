use std::sync::Arc;

use anyhow::Result;

use crate::pairing::orchestrator::PairingOrchestrator;

/// Application-layer use case for "user cancelled the pairing flow"
/// (phase 0.4.4 / D22 thin wrapper). `pub(crate)`: external consumers
/// drive this via [`super::super::facade::PairingFacade::cancel_pairing`].
pub(crate) struct CancelPairingUseCase {
    orchestrator: Arc<PairingOrchestrator>,
}

impl CancelPairingUseCase {
    pub(crate) fn new(orchestrator: Arc<PairingOrchestrator>) -> Self {
        Self { orchestrator }
    }

    pub(crate) async fn execute(&self, session_id: &str) -> Result<()> {
        self.orchestrator.user_cancel_pairing(session_id).await
    }
}
