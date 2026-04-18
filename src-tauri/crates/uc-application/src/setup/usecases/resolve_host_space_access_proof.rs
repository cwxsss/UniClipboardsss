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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setup::testing::build_default_harness;
    use uc_core::ids::{SessionId, SpaceId};

    #[tokio::test]
    async fn rejects_when_space_access_not_waiting_joiner_proof() {
        let harness = build_default_harness();
        let uc = ResolveHostSpaceAccessProofUseCase::new(Arc::clone(&harness.orchestrator));

        let proof = SpaceAccessProofArtifact {
            pairing_session_id: SessionId::from("session".to_string()),
            space_id: SpaceId::from("space"),
            challenge_nonce: [0u8; 32],
            proof_bytes: Vec::new(),
        };

        let err = uc.execute(proof, None).await.unwrap_err();
        assert!(matches!(err, SetupError::PairingFailed));
    }
}
