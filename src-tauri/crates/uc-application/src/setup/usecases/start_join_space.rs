use std::sync::Arc;

use crate::setup::orchestrator::{SetupError, SetupOrchestrator};
use crate::setup::SetupState;

pub(crate) struct StartJoinSpaceUseCase {
    orchestrator: Arc<SetupOrchestrator>,
}

impl StartJoinSpaceUseCase {
    pub(crate) fn new(orchestrator: Arc<SetupOrchestrator>) -> Self {
        Self { orchestrator }
    }

    pub(crate) async fn execute(&self) -> Result<SetupState, SetupError> {
        self.orchestrator.join_space().await
    }
}
