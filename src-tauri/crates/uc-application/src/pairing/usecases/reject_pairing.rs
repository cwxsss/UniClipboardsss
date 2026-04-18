use std::sync::Arc;

use anyhow::Result;

use crate::pairing::orchestrator::PairingOrchestrator;

/// Application-layer use case for "user rejected the short-code" on the
/// pairing flow (phase 0.4.4 / D22 thin wrapper). `pub(crate)`: external
/// consumers drive this via [`super::super::facade::PairingFacade::reject_pairing`].
pub(crate) struct RejectPairingUseCase {
    orchestrator: Arc<PairingOrchestrator>,
}

impl RejectPairingUseCase {
    pub(crate) fn new(orchestrator: Arc<PairingOrchestrator>) -> Self {
        Self { orchestrator }
    }

    pub(crate) async fn execute(&self, session_id: &str) -> Result<()> {
        self.orchestrator.user_reject_pairing(session_id).await
    }
}
