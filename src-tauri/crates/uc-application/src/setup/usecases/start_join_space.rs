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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setup::testing::build_default_harness;

    #[tokio::test]
    async fn welcome_transitions_to_join_space_select_device() {
        let harness = build_default_harness();
        let uc = StartJoinSpaceUseCase::new(Arc::clone(&harness.orchestrator));

        let state = uc.execute().await.unwrap();

        assert_eq!(state, SetupState::JoinSpaceSelectDevice { error: None });
        let emissions = harness.events.snapshot().await;
        assert_eq!(emissions.len(), 1);
        assert_eq!(
            emissions[0].0,
            SetupState::JoinSpaceSelectDevice { error: None }
        );
    }
}
