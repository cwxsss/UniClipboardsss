use std::sync::Arc;

use uc_core::ids::SpaceId;
use uc_core::space_access::state::{DenyReason, SpaceAccessState};

use crate::setup::orchestrator::{SetupError, SetupOrchestrator};

pub(crate) struct ApplyJoinerSpaceAccessResultUseCase {
    orchestrator: Arc<SetupOrchestrator>,
}

impl ApplyJoinerSpaceAccessResultUseCase {
    pub(crate) fn new(orchestrator: Arc<SetupOrchestrator>) -> Self {
        Self { orchestrator }
    }

    pub(crate) async fn execute(
        &self,
        pairing_session_id: String,
        space_id: SpaceId,
        sponsor_peer_id: Option<String>,
        success: bool,
        deny_reason: Option<DenyReason>,
    ) -> Result<SpaceAccessState, SetupError> {
        self.orchestrator
            .apply_joiner_space_access_result(
                pairing_session_id,
                space_id,
                sponsor_peer_id,
                success,
                deny_reason,
            )
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setup::testing::build_default_harness;

    /// Verifies the UseCase delegates to the orchestrator and propagates the
    /// resulting `SpaceAccessState` without panicking even when the
    /// state-machine cannot apply the event (Idle + `AccessDenied` is a
    /// no-op transition in the current flow).
    #[tokio::test]
    async fn deny_path_propagates_current_space_access_state() {
        let harness = build_default_harness();
        let uc = ApplyJoinerSpaceAccessResultUseCase::new(Arc::clone(&harness.orchestrator));

        let state = uc
            .execute(
                "session-1".to_string(),
                SpaceId::from("space"),
                Some("sponsor".to_string()),
                false,
                Some(DenyReason::InternalError),
            )
            .await
            .unwrap();
        // Accepts any state the orchestrator returns; what we assert is the
        // call did not bubble a `SetupError`.
        let _ = state;
    }
}
