use std::sync::Arc;

use crate::setup::orchestrator::{SetupError, SetupOrchestrator};
use crate::setup::SetupState;

pub(crate) struct CompleteJoinSpaceUseCase {
    orchestrator: Arc<SetupOrchestrator>,
}

impl CompleteJoinSpaceUseCase {
    pub(crate) fn new(orchestrator: Arc<SetupOrchestrator>) -> Self {
        Self { orchestrator }
    }

    pub(crate) async fn execute(&self) -> Result<SetupState, SetupError> {
        self.orchestrator.complete_join_space().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setup::testing::{build_default_harness, seed_state};

    #[tokio::test]
    async fn join_space_succeeded_transitions_to_completed_and_marks_status() {
        let harness = build_default_harness();
        seed_state(
            &harness,
            SetupState::ProcessingJoinSpace {
                message: Some("verifying".to_string()),
            },
        )
        .await;
        let uc = CompleteJoinSpaceUseCase::new(Arc::clone(&harness.orchestrator));

        let state = uc.execute().await.unwrap();

        assert_eq!(state, SetupState::Completed);
        assert_eq!(*harness.app_lifecycle.calls.lock().await, 1);
        assert!(harness.status.snapshot().await.has_completed);
    }
}
