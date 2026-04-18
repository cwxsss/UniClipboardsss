use std::sync::Arc;

use crate::setup::orchestrator::{SetupError, SetupOrchestrator};
use crate::setup::SetupState;

pub(crate) struct VerifyJoinPassphraseUseCase {
    orchestrator: Arc<SetupOrchestrator>,
}

impl VerifyJoinPassphraseUseCase {
    pub(crate) fn new(orchestrator: Arc<SetupOrchestrator>) -> Self {
        Self { orchestrator }
    }

    pub(crate) async fn execute(&self, passphrase: String) -> Result<SetupState, SetupError> {
        self.orchestrator.verify_passphrase(passphrase).await
    }
}
