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
