use std::sync::Arc;

use crate::setup::orchestrator::{SetupError, SetupOrchestrator};
use crate::setup::SetupState;

pub(crate) struct CancelSetupUseCase {
    orchestrator: Arc<SetupOrchestrator>,
}

impl CancelSetupUseCase {
    pub(crate) fn new(orchestrator: Arc<SetupOrchestrator>) -> Self {
        Self { orchestrator }
    }

    pub(crate) async fn execute(&self) -> Result<SetupState, SetupError> {
        self.orchestrator.cancel_setup().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setup::testing::{build_default_harness, seed_state};

    #[tokio::test]
    async fn cancel_from_create_space_input_returns_to_welcome() {
        let harness = build_default_harness();
        seed_state(
            &harness,
            SetupState::CreateSpaceInputPassphrase { error: None },
        )
        .await;
        let uc = CancelSetupUseCase::new(Arc::clone(&harness.orchestrator));

        assert_eq!(uc.execute().await.unwrap(), SetupState::Welcome);
    }

    #[tokio::test]
    async fn cancel_from_join_input_passphrase_returns_to_select_device() {
        let harness = build_default_harness();
        seed_state(
            &harness,
            SetupState::JoinSpaceInputPassphrase { error: None },
        )
        .await;
        let uc = CancelSetupUseCase::new(Arc::clone(&harness.orchestrator));

        assert_eq!(
            uc.execute().await.unwrap(),
            SetupState::JoinSpaceSelectDevice { error: None }
        );
    }
}
