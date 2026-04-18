use std::sync::Arc;

use crate::setup::orchestrator::{SetupError, SetupOrchestrator};
use crate::setup::SetupState;

pub(crate) struct ConfirmPeerTrustUseCase {
    orchestrator: Arc<SetupOrchestrator>,
}

impl ConfirmPeerTrustUseCase {
    pub(crate) fn new(orchestrator: Arc<SetupOrchestrator>) -> Self {
        Self { orchestrator }
    }

    pub(crate) async fn execute(&self) -> Result<SetupState, SetupError> {
        self.orchestrator.confirm_peer_trust().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setup::testing::{build_default_harness, seed_pairing_session, seed_state};

    #[tokio::test]
    async fn confirm_moves_from_confirm_peer_to_input_passphrase() {
        let harness = build_default_harness();
        seed_state(
            &harness,
            SetupState::JoinSpaceConfirmPeer {
                short_code: "1234".to_string(),
                peer_fingerprint: Some("fp".to_string()),
                error: None,
            },
        )
        .await;
        seed_pairing_session(&harness, "session-1").await;
        let uc = ConfirmPeerTrustUseCase::new(Arc::clone(&harness.orchestrator));

        let _ = uc.execute().await.unwrap();
        // The action executor emits the terminal JoinSpaceInputPassphrase after
        // the state-machine transition; assert the event trail settled there.
        let emissions = harness.events.snapshot().await;
        assert_eq!(
            emissions.last().map(|(s, _)| s.clone()),
            Some(SetupState::JoinSpaceInputPassphrase { error: None })
        );
    }

    #[tokio::test]
    async fn confirm_without_session_returns_pairing_failed() {
        let harness = build_default_harness();
        seed_state(
            &harness,
            SetupState::JoinSpaceConfirmPeer {
                short_code: "1234".to_string(),
                peer_fingerprint: None,
                error: None,
            },
        )
        .await;
        let uc = ConfirmPeerTrustUseCase::new(Arc::clone(&harness.orchestrator));

        let err = uc.execute().await.unwrap_err();
        assert!(matches!(err, SetupError::PairingFailed));
    }
}
