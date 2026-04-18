use std::sync::Arc;

use crate::setup::orchestrator::{SetupError, SetupOrchestrator};
use crate::setup::SetupState;

pub(crate) struct SelectJoinPeerUseCase {
    orchestrator: Arc<SetupOrchestrator>,
}

impl SelectJoinPeerUseCase {
    pub(crate) fn new(orchestrator: Arc<SetupOrchestrator>) -> Self {
        Self { orchestrator }
    }

    pub(crate) async fn execute(&self, peer_id: String) -> Result<SetupState, SetupError> {
        self.orchestrator.select_device(peer_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setup::testing::{
        build_default_harness, build_harness, seed_state, FakePairingFacade, HarnessOptions,
    };

    #[tokio::test]
    async fn select_peer_transitions_from_select_device_to_processing() {
        let harness = build_default_harness();
        seed_state(&harness, SetupState::JoinSpaceSelectDevice { error: None }).await;
        let uc = SelectJoinPeerUseCase::new(Arc::clone(&harness.orchestrator));

        let state = uc.execute("peer-a".to_string()).await.unwrap();

        assert!(
            matches!(state, SetupState::ProcessingJoinSpace { .. }),
            "expected ProcessingJoinSpace, got {state:?}"
        );
    }

    #[tokio::test]
    async fn select_peer_surfaces_pairing_initiation_failure() {
        let harness = build_harness(HarnessOptions {
            pairing_facade: FakePairingFacade::failing_initiate(),
            ..HarnessOptions::default()
        });
        seed_state(&harness, SetupState::JoinSpaceSelectDevice { error: None }).await;
        let uc = SelectJoinPeerUseCase::new(Arc::clone(&harness.orchestrator));

        let err = uc.execute("peer-a".to_string()).await.unwrap_err();
        assert!(matches!(err, SetupError::PairingFailed));
    }
}
