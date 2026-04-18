use std::sync::Arc;

use uc_core::space_access::state::SpaceAccessState;
use uc_core::space_access::SpaceAccessProofArtifact;

use crate::setup::orchestrator::{SetupError, SetupOrchestrator};

pub(crate) struct ResolveHostSpaceAccessProofUseCase {
    orchestrator: Arc<SetupOrchestrator>,
}

impl ResolveHostSpaceAccessProofUseCase {
    pub(crate) fn new(orchestrator: Arc<SetupOrchestrator>) -> Self {
        Self { orchestrator }
    }

    pub(crate) async fn execute(
        &self,
        proof: SpaceAccessProofArtifact,
        sponsor_peer_id: Option<String>,
    ) -> Result<SpaceAccessState, SetupError> {
        self.orchestrator
            .resolve_host_space_access_proof(proof, sponsor_peer_id)
            .await
    }
}
