use std::sync::Arc;

use crate::setup::orchestrator::{SetupError, SetupOrchestrator};
use crate::setup::SetupState;

pub(crate) struct ClearSetupTransientStateUseCase {
    orchestrator: Arc<SetupOrchestrator>,
}

impl ClearSetupTransientStateUseCase {
    pub(crate) fn new(orchestrator: Arc<SetupOrchestrator>) -> Self {
        Self { orchestrator }
    }

    pub(crate) async fn execute(&self) -> Result<SetupState, SetupError> {
        self.orchestrator.clear_transient_state().await
    }
}
