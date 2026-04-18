use std::sync::Arc;

use crate::setup::orchestrator::{SetupError, SetupOrchestrator};
use crate::setup::SetupState;

pub(crate) struct ResetSetupUseCase {
    orchestrator: Arc<SetupOrchestrator>,
}

impl ResetSetupUseCase {
    pub(crate) fn new(orchestrator: Arc<SetupOrchestrator>) -> Self {
        Self { orchestrator }
    }

    pub(crate) async fn execute(&self) -> Result<SetupState, SetupError> {
        self.orchestrator.reset().await
    }
}
