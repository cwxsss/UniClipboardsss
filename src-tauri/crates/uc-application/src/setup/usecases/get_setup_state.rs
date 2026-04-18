use std::sync::Arc;

use crate::setup::orchestrator::SetupOrchestrator;
use crate::setup::SetupState;

pub(crate) struct GetSetupStateQuery {
    orchestrator: Arc<SetupOrchestrator>,
}

impl GetSetupStateQuery {
    pub(crate) fn new(orchestrator: Arc<SetupOrchestrator>) -> Self {
        Self { orchestrator }
    }

    pub(crate) async fn execute(&self) -> SetupState {
        self.orchestrator.get_state().await
    }
}
