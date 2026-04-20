//! A2 · `UnlockSpaceUseCase`.
//!
//! Post-setup start-up flow: check `SetupStatus.has_completed`, then
//! forward to `SpaceAccessPort::unlock`. Because A1 is atomic, if we
//! ever reach A2 we can assume the owner `SpaceMember` / identity are
//! already persisted — A2 does not do a "self-member self-heal" round
//! (decision from Slice 1 outside-in session).

use std::sync::Arc;

use tracing::{debug, info, instrument, warn};

use uc_core::ids::SpaceId;
use uc_core::ports::space::{SpaceAccessError, SpaceAccessPort};
use uc_core::ports::SetupStatusPort;

use crate::facade::space_setup::{UnlockSpaceCommand, UnlockSpaceError, UnlockSpaceResult};

pub(crate) struct UnlockSpaceUseCase {
    space_access: Arc<dyn SpaceAccessPort>,
    setup_status: Arc<dyn SetupStatusPort>,
}

impl UnlockSpaceUseCase {
    pub(crate) fn new(
        space_access: Arc<dyn SpaceAccessPort>,
        setup_status: Arc<dyn SetupStatusPort>,
    ) -> Self {
        Self {
            space_access,
            setup_status,
        }
    }

    #[instrument(skip(self, cmd))]
    pub(crate) async fn execute(
        &self,
        cmd: UnlockSpaceCommand,
    ) -> Result<UnlockSpaceResult, UnlockSpaceError> {
        let status = self
            .setup_status
            .get_status()
            .await
            .map_err(|e| UnlockSpaceError::Internal(e.to_string()))?;
        if !status.has_completed {
            debug!("unlock rejected: setup not completed");
            return Err(UnlockSpaceError::SetupNotCompleted);
        }

        // The adapter keys keyslot lookup off the current profile, not
        // this id (see `uc-infra/src/security/space_access_adapter.rs`).
        // A fresh UUID here is fine and matches how A1 derives its id.
        let space_id = SpaceId::new();
        self.space_access
            .unlock(&space_id, &cmd.passphrase)
            .await
            .map_err(map_unlock_err)?;

        info!(%space_id, "space unlocked");
        Ok(UnlockSpaceResult { space_id })
    }
}

fn map_unlock_err(err: SpaceAccessError) -> UnlockSpaceError {
    match err {
        SpaceAccessError::NotInitialized => UnlockSpaceError::SpaceNotInitialized,
        SpaceAccessError::WrongPassphrase => UnlockSpaceError::WrongPassphrase,
        SpaceAccessError::CorruptedKeyMaterial => UnlockSpaceError::CorruptedKeyMaterial,
        SpaceAccessError::Internal(m) => UnlockSpaceError::Internal(m),
        other => {
            warn!(error = %other, "unexpected space-access error during unlock");
            UnlockSpaceError::Internal(other.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Mutex;

    use async_trait::async_trait;

    use uc_core::crypto::domain::{ActiveSpace, Passphrase};
    use uc_core::ids::SpaceId;
    use uc_core::ports::space::{SpaceAccessError, SpaceAccessPort};
    use uc_core::setup::SetupStatus;
    use uc_core::space_access::{JoinOffer, ProofDerivedKey};

    #[derive(Default)]
    struct FakeSpaceAccess {
        unlock_err: Mutex<Option<SpaceAccessError>>,
        unlock_calls: Mutex<u32>,
    }
    #[async_trait]
    impl SpaceAccessPort for FakeSpaceAccess {
        async fn initialize(
            &self,
            _space_id: &SpaceId,
            _passphrase: &Passphrase,
        ) -> Result<ActiveSpace, SpaceAccessError> {
            unimplemented!("A2 test does not touch initialize")
        }
        async fn unlock(
            &self,
            space_id: &SpaceId,
            _passphrase: &Passphrase,
        ) -> Result<ActiveSpace, SpaceAccessError> {
            *self.unlock_calls.lock().unwrap() += 1;
            if let Some(err) = self.unlock_err.lock().unwrap().take() {
                return Err(err);
            }
            Ok(ActiveSpace::new(space_id.clone()))
        }
        async fn is_unlocked(&self, _space_id: &SpaceId) -> bool {
            true
        }
        async fn lock(&self, _space_id: &SpaceId) -> Result<(), SpaceAccessError> {
            Ok(())
        }
        async fn factory_reset(&self, _space_id: &SpaceId) -> Result<(), SpaceAccessError> {
            Ok(())
        }
        async fn try_resume_session(
            &self,
            _space_id: &SpaceId,
        ) -> Result<Option<ActiveSpace>, SpaceAccessError> {
            Ok(None)
        }
        async fn verify_keychain_access(&self) -> Result<bool, SpaceAccessError> {
            Ok(true)
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
            _space_id: &SpaceId,
            _passphrase: &Passphrase,
        ) -> Result<JoinOffer, SpaceAccessError> {
            unimplemented!()
        }
        async fn derive_master_key_for_proof(
            &self,
            _offer: &JoinOffer,
            _passphrase: &Passphrase,
        ) -> Result<ProofDerivedKey, SpaceAccessError> {
            unimplemented!()
        }
    }

    #[derive(Default)]
    struct InMemorySetupStatus {
        status: Mutex<SetupStatus>,
    }
    #[async_trait]
    impl SetupStatusPort for InMemorySetupStatus {
        async fn get_status(&self) -> anyhow::Result<SetupStatus> {
            Ok(self.status.lock().unwrap().clone())
        }
        async fn set_status(&self, status: &SetupStatus) -> anyhow::Result<()> {
            *self.status.lock().unwrap() = status.clone();
            Ok(())
        }
    }

    fn build(
        setup_completed: bool,
        unlock_err: Option<SpaceAccessError>,
    ) -> (UnlockSpaceUseCase, Arc<FakeSpaceAccess>) {
        let space_access = Arc::new(FakeSpaceAccess::default());
        *space_access.unlock_err.lock().unwrap() = unlock_err;
        let setup_status = Arc::new(InMemorySetupStatus::default());
        if setup_completed {
            setup_status.status.lock().unwrap().has_completed = true;
        }
        let uc = UnlockSpaceUseCase::new(space_access.clone(), setup_status);
        (uc, space_access)
    }

    fn cmd(pass: &str) -> UnlockSpaceCommand {
        UnlockSpaceCommand {
            passphrase: Passphrase::new(pass),
        }
    }

    #[tokio::test]
    async fn happy_path_returns_result_and_calls_unlock() {
        let (uc, sa) = build(true, None);
        let r = uc.execute(cmd("pass")).await.unwrap();
        // Result carries a freshly minted SpaceId — not empty.
        assert!(!r.space_id.inner().is_empty());
        assert_eq!(*sa.unlock_calls.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn setup_not_completed_short_circuits_before_unlock() {
        let (uc, sa) = build(false, None);
        let err = uc.execute(cmd("pass")).await.unwrap_err();
        assert!(matches!(err, UnlockSpaceError::SetupNotCompleted));
        assert_eq!(*sa.unlock_calls.lock().unwrap(), 0);
    }

    #[tokio::test]
    async fn wrong_passphrase_maps_to_specific_variant() {
        let (uc, _sa) = build(true, Some(SpaceAccessError::WrongPassphrase));
        let err = uc.execute(cmd("wrong")).await.unwrap_err();
        assert!(matches!(err, UnlockSpaceError::WrongPassphrase));
    }

    #[tokio::test]
    async fn not_initialized_maps_to_space_not_initialized() {
        let (uc, _sa) = build(true, Some(SpaceAccessError::NotInitialized));
        let err = uc.execute(cmd("pass")).await.unwrap_err();
        assert!(matches!(err, UnlockSpaceError::SpaceNotInitialized));
    }

    #[tokio::test]
    async fn corrupted_key_material_maps_to_specific_variant() {
        let (uc, _sa) = build(true, Some(SpaceAccessError::CorruptedKeyMaterial));
        let err = uc.execute(cmd("pass")).await.unwrap_err();
        assert!(matches!(err, UnlockSpaceError::CorruptedKeyMaterial));
    }

    #[tokio::test]
    async fn internal_error_passthrough() {
        let (uc, _sa) = build(true, Some(SpaceAccessError::Internal("boom".into())));
        let err = uc.execute(cmd("pass")).await.unwrap_err();
        match err {
            UnlockSpaceError::Internal(m) => assert_eq!(m, "boom"),
            other => panic!("unexpected error: {other:?}"),
        }
    }
}
