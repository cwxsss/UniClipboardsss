use std::sync::Arc;

use tracing::instrument;
use uc_core::ids::SpaceId;
use uc_core::ports::setup::SetupStatusPort;
use uc_core::ports::space::{SpaceAccessError, SpaceAccessPort};

const DEFAULT_SPACE_ID: &str = "space";

#[derive(Clone)]
pub struct EncryptionFacadeDeps {
    pub setup_status: Arc<dyn SetupStatusPort>,
    pub space_access: Arc<dyn SpaceAccessPort>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncryptionStateView {
    pub initialized: bool,
    pub session_ready: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum EncryptionFacadeError {
    #[error("failed to load setup status: {0}")]
    SetupStatus(String),
    #[error("space access failed: {0}")]
    SpaceAccess(String),
}

pub struct EncryptionFacade {
    deps: EncryptionFacadeDeps,
}

impl EncryptionFacade {
    pub fn new(deps: EncryptionFacadeDeps) -> Self {
        Self { deps }
    }

    #[instrument(skip_all)]
    pub async fn state(&self) -> Result<EncryptionStateView, EncryptionFacadeError> {
        let initialized = self
            .deps
            .setup_status
            .get_status()
            .await
            .map(|status| status.has_completed)
            .map_err(|err| EncryptionFacadeError::SetupStatus(err.to_string()))?;
        let space_id = default_space_id();
        let session_ready = if initialized {
            self.deps.space_access.is_unlocked(&space_id).await
        } else {
            false
        };

        Ok(EncryptionStateView {
            initialized,
            session_ready,
        })
    }

    #[instrument(skip_all)]
    pub async fn unlock(&self) -> Result<bool, EncryptionFacadeError> {
        match self
            .deps
            .space_access
            .try_resume_session(&default_space_id())
            .await
        {
            Ok(Some(_)) => Ok(true),
            Ok(None) => Ok(false),
            Err(err) => Err(space_access_error(err)),
        }
    }

    #[instrument(skip_all)]
    pub async fn lock(&self) -> Result<(), EncryptionFacadeError> {
        self.deps
            .space_access
            .lock(&default_space_id())
            .await
            .map_err(space_access_error)
    }

    #[instrument(skip_all)]
    pub async fn verify_keychain_access(&self) -> Result<bool, EncryptionFacadeError> {
        self.deps
            .space_access
            .verify_keychain_access()
            .await
            .map_err(space_access_error)
    }
}

fn default_space_id() -> SpaceId {
    SpaceId::from(DEFAULT_SPACE_ID)
}

fn space_access_error(err: SpaceAccessError) -> EncryptionFacadeError {
    EncryptionFacadeError::SpaceAccess(err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    use async_trait::async_trait;
    use std::sync::Mutex;
    use uc_core::crypto::domain::{ActiveSpace, Passphrase};
    use uc_core::setup::SetupStatus;
    use uc_core::space_access::{JoinOffer, ProofDerivedKey};

    #[derive(Default)]
    struct FakeSetupStatus {
        status: Mutex<SetupStatus>,
    }

    #[async_trait]
    impl SetupStatusPort for FakeSetupStatus {
        async fn get_status(&self) -> anyhow::Result<SetupStatus> {
            Ok(self.status.lock().expect("status lock").clone())
        }

        async fn set_status(&self, status: &SetupStatus) -> anyhow::Result<()> {
            *self.status.lock().expect("status lock") = status.clone();
            Ok(())
        }
    }

    #[derive(Default)]
    struct FakeSpaceAccess {
        unlocked: Mutex<bool>,
        resume_returns_session: Mutex<bool>,
        verify_granted: Mutex<bool>,
        lock_calls: Mutex<u32>,
    }

    #[async_trait]
    impl SpaceAccessPort for FakeSpaceAccess {
        async fn initialize(
            &self,
            space_id: &SpaceId,
            _passphrase: &Passphrase,
        ) -> Result<ActiveSpace, SpaceAccessError> {
            Ok(ActiveSpace::new(space_id.clone()))
        }

        async fn unlock(
            &self,
            space_id: &SpaceId,
            _passphrase: &Passphrase,
        ) -> Result<ActiveSpace, SpaceAccessError> {
            Ok(ActiveSpace::new(space_id.clone()))
        }

        async fn is_unlocked(&self, _space_id: &SpaceId) -> bool {
            *self.unlocked.lock().expect("unlocked lock")
        }

        async fn lock(&self, _space_id: &SpaceId) -> Result<(), SpaceAccessError> {
            *self.unlocked.lock().expect("unlocked lock") = false;
            *self.lock_calls.lock().expect("lock calls lock") += 1;
            Ok(())
        }

        async fn factory_reset(&self, _space_id: &SpaceId) -> Result<(), SpaceAccessError> {
            Ok(())
        }

        async fn try_resume_session(
            &self,
            space_id: &SpaceId,
        ) -> Result<Option<ActiveSpace>, SpaceAccessError> {
            if *self.resume_returns_session.lock().expect("resume lock") {
                *self.unlocked.lock().expect("unlocked lock") = true;
                Ok(Some(ActiveSpace::new(space_id.clone())))
            } else {
                Ok(None)
            }
        }

        async fn verify_keychain_access(&self) -> Result<bool, SpaceAccessError> {
            Ok(*self.verify_granted.lock().expect("verify lock"))
        }

        async fn derive_subkey(
            &self,
            _salt: &[u8],
            _info: &[u8],
        ) -> Result<[u8; 32], SpaceAccessError> {
            Ok([0; 32])
        }

        async fn current_session_proof_key(
            &self,
        ) -> Result<Option<ProofDerivedKey>, SpaceAccessError> {
            Ok(None)
        }

        async fn prepare_join_offer(
            &self,
            space_id: &SpaceId,
            _passphrase: &Passphrase,
        ) -> Result<JoinOffer, SpaceAccessError> {
            Ok(JoinOffer {
                space_id: space_id.clone(),
                keyslot_blob: Vec::new(),
                challenge_nonce: [0; 32],
            })
        }

        async fn derive_master_key_for_proof(
            &self,
            _offer: &JoinOffer,
            _passphrase: &Passphrase,
        ) -> Result<ProofDerivedKey, SpaceAccessError> {
            Ok(ProofDerivedKey::from_bytes([0; 32]))
        }
    }

    fn facade_with(
        completed: bool,
        unlocked: bool,
        resume_returns_session: bool,
        verify_granted: bool,
    ) -> (EncryptionFacade, Arc<FakeSpaceAccess>) {
        let setup_status = Arc::new(FakeSetupStatus::default());
        setup_status
            .status
            .lock()
            .expect("status lock")
            .has_completed = completed;
        let space_access = Arc::new(FakeSpaceAccess::default());
        *space_access.unlocked.lock().expect("unlocked lock") = unlocked;
        *space_access
            .resume_returns_session
            .lock()
            .expect("resume lock") = resume_returns_session;
        *space_access.verify_granted.lock().expect("verify lock") = verify_granted;

        (
            EncryptionFacade::new(EncryptionFacadeDeps {
                setup_status,
                space_access: space_access.clone(),
            }),
            space_access,
        )
    }

    #[tokio::test]
    async fn state_reports_not_ready_when_setup_is_incomplete() {
        let (facade, _) = facade_with(false, true, false, false);

        let state = facade.state().await.expect("state");

        assert_eq!(
            state,
            EncryptionStateView {
                initialized: false,
                session_ready: false
            }
        );
    }

    #[tokio::test]
    async fn state_reports_session_ready_after_completed_setup() {
        let (facade, _) = facade_with(true, true, false, false);

        let state = facade.state().await.expect("state");

        assert_eq!(
            state,
            EncryptionStateView {
                initialized: true,
                session_ready: true
            }
        );
    }

    #[tokio::test]
    async fn unlock_returns_whether_session_was_resumed() {
        let (resumed, _) = facade_with(true, false, true, false);
        let (not_resumed, _) = facade_with(true, false, false, false);

        assert!(resumed.unlock().await.expect("resumed"));
        assert!(!not_resumed.unlock().await.expect("not resumed"));
    }

    #[tokio::test]
    async fn lock_clears_session() {
        let (facade, space_access) = facade_with(true, true, false, false);

        facade.lock().await.expect("lock");

        assert!(!*space_access.unlocked.lock().expect("unlocked lock"));
        assert_eq!(*space_access.lock_calls.lock().expect("lock calls lock"), 1);
    }

    #[tokio::test]
    async fn verify_keychain_access_returns_grant_state() {
        let (facade, _) = facade_with(true, false, false, true);

        assert!(facade
            .verify_keychain_access()
            .await
            .expect("verify keychain"));
    }
}
