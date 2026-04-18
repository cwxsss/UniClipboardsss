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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setup::testing::{build_harness, seed_state, FakeSetupStatus, HarnessOptions};

    #[tokio::test]
    async fn reset_returns_welcome_and_clears_status() {
        let harness = build_harness(HarnessOptions {
            status: FakeSetupStatus::completed(),
            ..HarnessOptions::default()
        });
        seed_state(&harness, SetupState::Completed).await;
        let uc = ResetSetupUseCase::new(Arc::clone(&harness.orchestrator));

        let state = uc.execute().await.unwrap();

        assert_eq!(state, SetupState::Welcome);
        let status = harness.status.snapshot().await;
        assert!(!status.has_completed, "reset must clear has_completed");
        let emissions = harness.events.snapshot().await;
        assert_eq!(emissions.last().map(|(s, _)| s), Some(&SetupState::Welcome));
    }
}
