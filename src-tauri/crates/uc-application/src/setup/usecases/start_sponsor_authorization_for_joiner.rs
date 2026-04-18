use std::sync::Arc;

use uc_core::crypto::model::KeySlotFile;
use uc_core::space_access::state::SpaceAccessState;

use crate::setup::orchestrator::{SetupError, SetupOrchestrator};

pub(crate) struct StartSponsorAuthorizationForJoinerUseCase {
    orchestrator: Arc<SetupOrchestrator>,
}

impl StartSponsorAuthorizationForJoinerUseCase {
    pub(crate) fn new(orchestrator: Arc<SetupOrchestrator>) -> Self {
        Self { orchestrator }
    }

    pub(crate) async fn execute(
        &self,
        pairing_session_id: String,
        sponsor_peer_id: String,
        keyslot_file: KeySlotFile,
    ) -> Result<SpaceAccessState, SetupError> {
        self.orchestrator
            .start_completed_host_sponsor_authorization(
                pairing_session_id,
                sponsor_peer_id,
                keyslot_file,
            )
            .await
    }
}
