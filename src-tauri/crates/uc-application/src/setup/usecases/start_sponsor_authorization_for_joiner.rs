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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setup::testing::{build_default_harness, seed_state};
    use crate::setup::SetupState;
    use uc_core::crypto::model::{
        EncryptedBlob, EncryptionAlgo, EncryptionFormatVersion, KdfParams, KeyScope, KeySlotFile,
        KeySlotVersion,
    };

    fn sample_keyslot_file() -> KeySlotFile {
        KeySlotFile {
            version: KeySlotVersion::V1,
            scope: KeyScope {
                profile_id: "profile".into(),
            },
            kdf: KdfParams::for_initialization(),
            salt: vec![0u8; 16],
            wrapped_master_key: EncryptedBlob {
                version: EncryptionFormatVersion::V1,
                aead: EncryptionAlgo::XChaCha20Poly1305,
                nonce: vec![0u8; 24],
                ciphertext: vec![0u8; 8],
                aad_fingerprint: None,
            },
            created_at: None,
            updated_at: None,
        }
    }

    #[tokio::test]
    async fn rejects_when_setup_not_completed() {
        let harness = build_default_harness();
        seed_state(&harness, SetupState::Welcome).await;
        let uc = StartSponsorAuthorizationForJoinerUseCase::new(Arc::clone(&harness.orchestrator));

        let err = uc
            .execute("session".into(), "sponsor".into(), sample_keyslot_file())
            .await
            .unwrap_err();
        assert!(matches!(err, SetupError::PairingFailed));
    }
}
