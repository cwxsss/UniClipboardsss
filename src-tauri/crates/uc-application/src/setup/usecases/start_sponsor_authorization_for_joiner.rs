use std::sync::Arc;

use uc_core::ids::SpaceId;
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
        space_id: SpaceId,
    ) -> Result<SpaceAccessState, SetupError> {
        self.orchestrator
            .start_completed_host_sponsor_authorization(
                pairing_session_id,
                sponsor_peer_id,
                space_id,
            )
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setup::testing::{build_default_harness, seed_state};
    use crate::setup::SetupState;

    #[tokio::test]
    async fn rejects_when_setup_not_completed() {
        let harness = build_default_harness();
        seed_state(&harness, SetupState::Welcome).await;
        let uc = StartSponsorAuthorizationForJoinerUseCase::new(Arc::clone(&harness.orchestrator));

        let err = uc
            .execute("session".into(), "sponsor".into(), SpaceId::from("profile"))
            .await
            .unwrap_err();
        assert!(matches!(err, SetupError::PairingFailed));
    }
}
