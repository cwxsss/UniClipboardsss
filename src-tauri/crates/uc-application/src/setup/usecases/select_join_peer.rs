use std::sync::Arc;

use crate::setup::orchestrator::{SetupError, SetupOrchestrator};
use crate::setup::SetupState;

pub(crate) struct SelectJoinPeerUseCase {
    orchestrator: Arc<SetupOrchestrator>,
}

impl SelectJoinPeerUseCase {
    pub(crate) fn new(orchestrator: Arc<SetupOrchestrator>) -> Self {
        Self { orchestrator }
    }

    pub(crate) async fn execute(&self, peer_id: String) -> Result<SetupState, SetupError> {
        self.orchestrator.select_device(peer_id).await
    }
}
