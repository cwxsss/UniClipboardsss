use std::sync::Arc;

use crate::setup::orchestrator::{SetupError, SetupOrchestrator};
use crate::setup::SetupState;

pub(crate) struct VerifyJoinPassphraseUseCase {
    orchestrator: Arc<SetupOrchestrator>,
}

impl VerifyJoinPassphraseUseCase {
    pub(crate) fn new(orchestrator: Arc<SetupOrchestrator>) -> Self {
        Self { orchestrator }
    }

    pub(crate) async fn execute(&self, passphrase: String) -> Result<SetupState, SetupError> {
        self.orchestrator.verify_passphrase(passphrase).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setup::testing::{build_default_harness, seed_state};

    #[tokio::test]
    async fn verify_without_session_returns_pairing_failed() {
        let harness = build_default_harness();
        seed_state(
            &harness,
            SetupState::JoinSpaceInputPassphrase { error: None },
        )
        .await;
        let uc = VerifyJoinPassphraseUseCase::new(Arc::clone(&harness.orchestrator));

        let err = uc.execute("any".to_string()).await.unwrap_err();
        // `StartJoinSpaceAccess` bails early when session/peer context is empty.
        assert!(matches!(err, SetupError::PairingFailed));
    }
}
