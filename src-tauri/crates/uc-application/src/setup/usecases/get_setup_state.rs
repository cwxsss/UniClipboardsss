use std::sync::Arc;

use crate::setup::orchestrator::SetupOrchestrator;
use crate::setup::SetupState;

pub(crate) struct GetSetupStateQuery {
    orchestrator: Arc<SetupOrchestrator>,
}

impl GetSetupStateQuery {
    pub(crate) fn new(orchestrator: Arc<SetupOrchestrator>) -> Self {
        Self { orchestrator }
    }

    pub(crate) async fn execute(&self) -> SetupState {
        self.orchestrator.get_state().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setup::testing::{build_harness, FakeSetupStatus, HarnessOptions};

    #[tokio::test]
    async fn seeds_completed_when_status_has_completed() {
        let harness = build_harness(HarnessOptions {
            status: FakeSetupStatus::completed(),
            ..HarnessOptions::default()
        });
        let uc = GetSetupStateQuery::new(Arc::clone(&harness.orchestrator));

        assert_eq!(uc.execute().await, SetupState::Completed);
    }

    #[tokio::test]
    async fn seeds_welcome_when_status_not_completed() {
        let harness = build_harness(HarnessOptions::default());
        let uc = GetSetupStateQuery::new(Arc::clone(&harness.orchestrator));

        assert_eq!(uc.execute().await, SetupState::Welcome);
    }
}
