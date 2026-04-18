use std::sync::Arc;

use crate::setup::orchestrator::{SetupError, SetupOrchestrator};
use crate::setup::SetupState;

pub(crate) struct StartNewSpaceUseCase {
    orchestrator: Arc<SetupOrchestrator>,
}

impl StartNewSpaceUseCase {
    pub(crate) fn new(orchestrator: Arc<SetupOrchestrator>) -> Self {
        Self { orchestrator }
    }

    pub(crate) async fn execute(&self) -> Result<SetupState, SetupError> {
        self.orchestrator.new_space().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setup::testing::build_default_harness;

    #[tokio::test]
    async fn welcome_transitions_to_create_space_input_passphrase() {
        let harness = build_default_harness();
        let uc = StartNewSpaceUseCase::new(Arc::clone(&harness.orchestrator));

        let state = uc.execute().await.unwrap();

        assert_eq!(
            state,
            SetupState::CreateSpaceInputPassphrase { error: None }
        );
        let emissions = harness.events.snapshot().await;
        assert_eq!(emissions.len(), 1);
        assert_eq!(
            emissions[0].0,
            SetupState::CreateSpaceInputPassphrase { error: None }
        );
    }
}
