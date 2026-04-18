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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setup::testing::{build_harness, seed_state, FakeSetupStatus, HarnessOptions};

    #[tokio::test]
    async fn returns_completed_when_setup_status_marked_completed() {
        let harness = build_harness(HarnessOptions {
            status: FakeSetupStatus::completed(),
            ..HarnessOptions::default()
        });
        seed_state(
            &harness,
            SetupState::JoinSpaceInputPassphrase { error: None },
        )
        .await;
        let uc = ClearSetupTransientStateUseCase::new(Arc::clone(&harness.orchestrator));

        let state = uc.execute().await.unwrap();

        assert_eq!(state, SetupState::Completed);
    }

    #[tokio::test]
    async fn returns_welcome_when_setup_status_not_completed() {
        let harness = build_harness(HarnessOptions::default());
        seed_state(&harness, SetupState::JoinSpaceSelectDevice { error: None }).await;
        let uc = ClearSetupTransientStateUseCase::new(Arc::clone(&harness.orchestrator));

        let state = uc.execute().await.unwrap();

        assert_eq!(state, SetupState::Welcome);
    }
}
