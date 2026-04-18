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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setup::testing::{build_default_harness, seed_state};

    #[tokio::test]
    async fn submit_drives_create_space_flow_to_completed() {
        let harness = build_default_harness();
        seed_state(
            &harness,
            SetupState::CreateSpaceInputPassphrase { error: None },
        )
        .await;
        let uc = SubmitNewSpacePassphraseUseCase::new(Arc::clone(&harness.orchestrator));

        let final_state = uc
            .execute("correct horse".to_string(), "correct horse".to_string())
            .await
            .unwrap();

        assert_eq!(final_state, SetupState::Completed);
        assert_eq!(*harness.initialize_encryption.calls.lock().await, 1);
        assert_eq!(*harness.app_lifecycle.calls.lock().await, 1);
        assert!(harness.status.snapshot().await.has_completed);
    }
}
