use std::sync::Arc;

use crate::setup::orchestrator::{SetupError, SetupOrchestrator};
use crate::setup::SetupState;

pub(crate) struct SubmitNewSpacePassphraseUseCase {
    orchestrator: Arc<SetupOrchestrator>,
}

impl SubmitNewSpacePassphraseUseCase {
    pub(crate) fn new(orchestrator: Arc<SetupOrchestrator>) -> Self {
        Self { orchestrator }
    }

    pub(crate) async fn execute(
        &self,
        passphrase: String,
        confirm: String,
    ) -> Result<SetupState, SetupError> {
        self.orchestrator
            .submit_passphrase(passphrase, confirm)
            .await
    }
}
