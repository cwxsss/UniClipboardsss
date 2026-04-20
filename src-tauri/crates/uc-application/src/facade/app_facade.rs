//! `AppFacade` — Slice 1 single, cross-domain entry point.
//!
//! Per `uc-application/AGENTS.md` §11.4 external consumers reach the
//! application layer exclusively through a facade. `AppFacade` composes
//! cross-domain use cases (Slice 1: A1 `InitializeSpaceUseCase` + A2
//! `UnlockSpaceUseCase`) behind a single surface, and will grow to host
//! B1/B2/F1/F2 entry points in subsequent phases.
//!
//! # Current scope (Slice 1 · P4)
//!
//! * A1 `initialize_space` — first-run flow
//! * A2 `unlock_space` — post-setup start-up flow
//!
//! # Deferred
//!
//! * B1 `issue_pairing_invitation` / B2 `redeem_pairing_invitation` → P7+
//! * F1 `on_startup` / A1 & A2 success → auto-`StartNetwork` → P6
//! * F2 `on_shutdown` → P6
//! * Wiring daemon / tauri / CLI to call `AppFacade` instead of the
//!   existing sub-facades (`SetupFacade`, `PairingFacade`,
//!   `SpaceAccessFacade`) → Slice 1.5 or later
//!
//! Sub-facades remain `pub` — Slice 1 does not break existing entry
//! points.

use std::sync::Arc;

use tracing::instrument;
use uc_core::membership::MemberRepositoryPort;
use uc_core::ports::space::SpaceAccessPort;
use uc_core::ports::{
    ClockPort, DeviceIdentityPort, LocalIdentityPort, SettingsPort, SetupStatusPort,
};

use crate::facade::commands::{
    InitializeSpaceCommand, InitializeSpaceResult, UnlockSpaceCommand, UnlockSpaceResult,
};
use crate::facade::errors::{InitializeSpaceError, UnlockSpaceError};
use crate::usecases::setup::initialize_space::InitializeSpaceUseCase;
use crate::usecases::setup::unlock_space::UnlockSpaceUseCase;

/// Slice 1 Application Facade.
///
/// Owns the per-intent use cases as `Arc` so future methods (F1 bootstrap,
/// B1/B2 pairing) can share them without tearing down the composition.
pub struct AppFacade {
    initialize_space: Arc<InitializeSpaceUseCase>,
    unlock_space: Arc<UnlockSpaceUseCase>,
}

impl AppFacade {
    /// Construct the Slice 1 `AppFacade` from the ports it composes.
    ///
    /// The facade wires both setup use cases internally; external callers
    /// (bootstrap) only supply ports. `SpaceAccessPort` / `SetupStatusPort`
    /// are shared between A1 and A2 because the underlying adapter keeps
    /// the active space / setup-status as process-wide singletons.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        space_access: Arc<dyn SpaceAccessPort>,
        local_identity: Arc<dyn LocalIdentityPort>,
        device_identity: Arc<dyn DeviceIdentityPort>,
        member_repo: Arc<dyn MemberRepositoryPort>,
        setup_status: Arc<dyn SetupStatusPort>,
        settings: Arc<dyn SettingsPort>,
        clock: Arc<dyn ClockPort>,
    ) -> Self {
        let initialize_space = Arc::new(InitializeSpaceUseCase::new(
            Arc::clone(&space_access),
            local_identity,
            device_identity,
            member_repo,
            Arc::clone(&setup_status),
            settings,
            clock,
        ));
        let unlock_space = Arc::new(UnlockSpaceUseCase::new(space_access, setup_status));
        Self {
            initialize_space,
            unlock_space,
        }
    }

    /// A1 · Create the encrypted space on a fresh device.
    ///
    /// Follow-up `StartNetwork` wiring lands with F1 in P6.
    #[instrument(skip_all)]
    pub async fn initialize_space(
        &self,
        cmd: InitializeSpaceCommand,
    ) -> Result<InitializeSpaceResult, InitializeSpaceError> {
        // TODO(P6 · F1): on success, call `StartNetworkUseCase::execute()`.
        self.initialize_space.execute(cmd).await
    }

    /// A2 · Unlock the encrypted space after a restart.
    ///
    /// Follow-up `StartNetwork` wiring lands with F1 in P6.
    #[instrument(skip_all)]
    pub async fn unlock_space(
        &self,
        cmd: UnlockSpaceCommand,
    ) -> Result<UnlockSpaceResult, UnlockSpaceError> {
        // TODO(P6 · F1): on success, call `StartNetworkUseCase::execute()`.
        self.unlock_space.execute(cmd).await
    }
}

#[cfg(test)]
mod tests {
    //! Thin smoke tests — the two use cases themselves are covered
    //! exhaustively in `usecases::setup::{initialize_space,unlock_space}`.
    //! Here we only prove that `AppFacade::new` wires them up and that
    //! the facade methods forward arguments and error codes unchanged.

    use super::*;

    use std::sync::Mutex as StdMutex;

    use async_trait::async_trait;

    use uc_core::crypto::domain::{ActiveSpace, Passphrase};
    use uc_core::ids::{DeviceId, SpaceId};
    use uc_core::membership::{MembershipError, SpaceMember};
    use uc_core::ports::space::{SpaceAccessError, SpaceAccessPort};
    use uc_core::ports::{
        ClockPort, DeviceIdentityPort, LocalIdentityError, LocalIdentityPort, SettingsPort,
        SetupStatusPort,
    };
    use uc_core::security::IdentityFingerprint;
    use uc_core::settings::model::Settings;
    use uc_core::setup::SetupStatus;
    use uc_core::space_access::{JoinOffer, ProofDerivedKey};

    // ── fakes (minimal) ──────────────────────────────────────────────────

    #[derive(Default)]
    struct FakeSpaceAccess {
        unlock_err: StdMutex<Option<SpaceAccessError>>,
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
            unimplemented!("not used by A1/A2")
        }
        async fn derive_master_key_for_proof(
            &self,
            _offer: &JoinOffer,
            _passphrase: &Passphrase,
        ) -> Result<ProofDerivedKey, SpaceAccessError> {
            unimplemented!("not used by A1/A2")
        }
    }

    struct FakeLocalIdentity {
        fp: IdentityFingerprint,
    }
    #[async_trait]
    impl LocalIdentityPort for FakeLocalIdentity {
        async fn create(&self) -> Result<IdentityFingerprint, LocalIdentityError> {
            Ok(self.fp.clone())
        }
        async fn ensure(&self) -> Result<IdentityFingerprint, LocalIdentityError> {
            Ok(self.fp.clone())
        }
        async fn get_current_fingerprint(
            &self,
        ) -> Result<Option<IdentityFingerprint>, LocalIdentityError> {
            Ok(Some(self.fp.clone()))
        }
    }

    struct FixedDeviceIdentity {
        id: DeviceId,
    }
    impl DeviceIdentityPort for FixedDeviceIdentity {
        fn current_device_id(&self) -> DeviceId {
            self.id.clone()
        }
    }

    #[derive(Default)]
    struct InMemoryMemberRepo {
        rows: StdMutex<Vec<SpaceMember>>,
    }
    #[async_trait]
    impl uc_core::membership::MemberRepositoryPort for InMemoryMemberRepo {
        async fn get(&self, device_id: &DeviceId) -> Result<Option<SpaceMember>, MembershipError> {
            Ok(self
                .rows
                .lock()
                .unwrap()
                .iter()
                .find(|m| &m.device_id == device_id)
                .cloned())
        }
        async fn list(&self) -> Result<Vec<SpaceMember>, MembershipError> {
            Ok(self.rows.lock().unwrap().clone())
        }
        async fn save(&self, member: &SpaceMember) -> Result<(), MembershipError> {
            self.rows.lock().unwrap().push(member.clone());
            Ok(())
        }
        async fn remove(&self, _device_id: &DeviceId) -> Result<bool, MembershipError> {
            Ok(true)
        }
    }

    #[derive(Default)]
    struct InMemorySetupStatus {
        status: StdMutex<SetupStatus>,
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

    #[derive(Default)]
    struct InMemorySettings {
        settings: StdMutex<Settings>,
    }
    #[async_trait]
    impl SettingsPort for InMemorySettings {
        async fn load(&self) -> anyhow::Result<Settings> {
            Ok(self.settings.lock().unwrap().clone())
        }
        async fn save(&self, settings: &Settings) -> anyhow::Result<()> {
            *self.settings.lock().unwrap() = settings.clone();
            Ok(())
        }
    }

    struct FixedClock(i64);
    impl ClockPort for FixedClock {
        fn now_ms(&self) -> i64 {
            self.0
        }
    }

    fn default_fingerprint() -> IdentityFingerprint {
        IdentityFingerprint::from_raw_string("ABCDEFGHIJKLMNOP").unwrap()
    }

    fn make_facade(
        space_access: Arc<dyn SpaceAccessPort>,
        setup_status: Arc<dyn SetupStatusPort>,
        settings: Arc<dyn SettingsPort>,
    ) -> AppFacade {
        AppFacade::new(
            space_access,
            Arc::new(FakeLocalIdentity {
                fp: default_fingerprint(),
            }),
            Arc::new(FixedDeviceIdentity {
                id: DeviceId::new("device-1"),
            }),
            Arc::new(InMemoryMemberRepo::default()),
            setup_status,
            settings,
            Arc::new(FixedClock(0)),
        )
    }

    fn settings_with_device_name(name: &str) -> Arc<InMemorySettings> {
        let holder = InMemorySettings::default();
        {
            let mut s = holder.settings.lock().unwrap();
            s.general.device_name = Some(name.to_string());
        }
        Arc::new(holder)
    }

    // ── tests ────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn initialize_space_forwards_happy_path() {
        let facade = make_facade(
            Arc::new(FakeSpaceAccess::default()),
            Arc::new(InMemorySetupStatus::default()),
            settings_with_device_name("mac"),
        );
        let cmd = InitializeSpaceCommand {
            passphrase: Passphrase::new("hunter22hunter22"),
            passphrase_confirm: Passphrase::new("hunter22hunter22"),
            device_name: None,
        };
        let out = facade.initialize_space(cmd).await.expect("A1 ok");
        assert_eq!(out.fingerprint, default_fingerprint());
    }

    #[tokio::test]
    async fn initialize_space_forwards_passphrase_mismatch() {
        let facade = make_facade(
            Arc::new(FakeSpaceAccess::default()),
            Arc::new(InMemorySetupStatus::default()),
            settings_with_device_name("mac"),
        );
        let cmd = InitializeSpaceCommand {
            passphrase: Passphrase::new("hunter22hunter22"),
            passphrase_confirm: Passphrase::new("different22else2"),
            device_name: None,
        };
        let err = facade.initialize_space(cmd).await.unwrap_err();
        assert!(matches!(err, InitializeSpaceError::PassphraseMismatch));
    }

    #[tokio::test]
    async fn unlock_space_forwards_happy_path() {
        let setup_status = InMemorySetupStatus::default();
        *setup_status.status.lock().unwrap() = SetupStatus {
            has_completed: true,
        };
        let facade = make_facade(
            Arc::new(FakeSpaceAccess::default()),
            Arc::new(setup_status),
            Arc::new(InMemorySettings::default()),
        );
        let cmd = UnlockSpaceCommand {
            passphrase: Passphrase::new("hunter22hunter22"),
        };
        facade.unlock_space(cmd).await.expect("A2 ok");
    }

    #[tokio::test]
    async fn unlock_space_forwards_setup_not_completed() {
        let facade = make_facade(
            Arc::new(FakeSpaceAccess::default()),
            Arc::new(InMemorySetupStatus::default()),
            Arc::new(InMemorySettings::default()),
        );
        let cmd = UnlockSpaceCommand {
            passphrase: Passphrase::new("hunter22hunter22"),
        };
        let err = facade.unlock_space(cmd).await.unwrap_err();
        assert!(matches!(err, UnlockSpaceError::SetupNotCompleted));
    }

    #[tokio::test]
    async fn unlock_space_forwards_wrong_passphrase() {
        let setup_status = InMemorySetupStatus::default();
        *setup_status.status.lock().unwrap() = SetupStatus {
            has_completed: true,
        };
        let space_access = FakeSpaceAccess::default();
        *space_access.unlock_err.lock().unwrap() = Some(SpaceAccessError::WrongPassphrase);
        let facade = make_facade(
            Arc::new(space_access),
            Arc::new(setup_status),
            Arc::new(InMemorySettings::default()),
        );
        let cmd = UnlockSpaceCommand {
            passphrase: Passphrase::new("hunter22hunter22"),
        };
        let err = facade.unlock_space(cmd).await.unwrap_err();
        assert!(matches!(err, UnlockSpaceError::WrongPassphrase));
    }
}
