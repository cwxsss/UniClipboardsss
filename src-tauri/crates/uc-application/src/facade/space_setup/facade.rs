//! `SpaceSetupFacade` — space-lifecycle entry point (A1 + A2 + shutdown).
//!
//! Owns the two use cases plus the network lifecycle port so A1/A2 success
//! auto-triggers `start_network` (F1) and [`on_shutdown`](Self::on_shutdown)
//! mirrors it with `stop_network` (F2).
//!
//! Network errors during auto-start are intentionally non-fatal: the
//! underlying space mutation has already committed and isn't safe to roll
//! back, and the network runtime is retryable from UI. Failures are
//! surfaced through `tracing::warn!` so ops still sees them.

use std::sync::Arc;

use tracing::{instrument, warn};

use uc_core::ports::NetworkControlPort;

use crate::facade::space_setup::commands::{
    InitializeSpaceCommand, InitializeSpaceResult, IssuePairingInvitationResult,
    UnlockSpaceCommand, UnlockSpaceResult,
};
use crate::facade::space_setup::deps::SpaceSetupDeps;
use crate::facade::space_setup::errors::{
    InitializeSpaceError, IssuePairingInvitationError, UnlockSpaceError,
};
use crate::pairing_invitation::InMemoryPairingInvitationHolder;
use crate::usecases::pairing::issue_invitation::IssuePairingInvitationUseCase;
use crate::usecases::setup::initialize_space::InitializeSpaceUseCase;
use crate::usecases::setup::unlock_space::UnlockSpaceUseCase;

/// Space-lifecycle facade (A1 initialise, A2 unlock, B1 issue invitation,
/// F2 shutdown).
pub struct SpaceSetupFacade {
    initialize_space: Arc<InitializeSpaceUseCase>,
    unlock_space: Arc<UnlockSpaceUseCase>,
    issue_pairing_invitation: Arc<IssuePairingInvitationUseCase>,
    network_control: Arc<dyn NetworkControlPort>,
}

impl SpaceSetupFacade {
    /// Wire all use cases from a single [`SpaceSetupDeps`] bundle.
    pub fn new(deps: SpaceSetupDeps) -> Self {
        let SpaceSetupDeps {
            space_access,
            local_identity,
            device_identity,
            member_repo,
            setup_status,
            settings,
            clock,
            network_control,
            pairing_invitation,
        } = deps;

        // Invitation holder is purely an internal flow-state component
        // (§11.4) — construct it here so bootstrap never sees the type.
        let invitation_holder = Arc::new(InMemoryPairingInvitationHolder::new());

        let initialize_space = Arc::new(InitializeSpaceUseCase::new(
            Arc::clone(&space_access),
            local_identity,
            Arc::clone(&device_identity),
            member_repo,
            Arc::clone(&setup_status),
            settings,
            Arc::clone(&clock),
        ));
        let unlock_space = Arc::new(UnlockSpaceUseCase::new(space_access, setup_status));
        let issue_pairing_invitation = Arc::new(IssuePairingInvitationUseCase::new(
            pairing_invitation,
            device_identity,
            clock,
            invitation_holder,
        ));
        Self {
            initialize_space,
            unlock_space,
            issue_pairing_invitation,
            network_control,
        }
    }

    /// A1 · Create the encrypted space on a fresh device. On success the
    /// network runtime is auto-started (F1).
    #[instrument(skip_all)]
    pub async fn initialize_space(
        &self,
        cmd: InitializeSpaceCommand,
    ) -> Result<InitializeSpaceResult, InitializeSpaceError> {
        let out = self.initialize_space.execute(cmd).await?;
        self.auto_start_network().await;
        Ok(out)
    }

    /// A2 · Unlock the encrypted space after a restart. On success the
    /// network runtime is auto-started (F1).
    #[instrument(skip_all)]
    pub async fn unlock_space(
        &self,
        cmd: UnlockSpaceCommand,
    ) -> Result<UnlockSpaceResult, UnlockSpaceError> {
        let out = self.unlock_space.execute(cmd).await?;
        self.auto_start_network().await;
        Ok(out)
    }

    /// B1 · Ask the rendezvous service for a fresh invitation code and
    /// park the resulting aggregate in the application-layer holder.
    ///
    /// Does **not** auto-start the network: the adapter surfaces
    /// [`IssuePairingInvitationError::NetworkNotStarted`] if the runtime
    /// isn't up, letting the UI prompt the user to complete A1/A2 first.
    #[instrument(skip_all)]
    pub async fn issue_pairing_invitation(
        &self,
    ) -> Result<IssuePairingInvitationResult, IssuePairingInvitationError> {
        self.issue_pairing_invitation.execute().await
    }

    /// F2 · Shut the network runtime down cleanly on app exit.
    ///
    /// Best-effort: failures are logged but not returned, since the caller
    /// is on the teardown path and has no recourse.
    #[instrument(skip_all)]
    pub async fn on_shutdown(&self) {
        if let Err(err) = self.network_control.stop_network().await {
            warn!(
                error = %err,
                "stop_network failed during shutdown; proceeding with teardown"
            );
        }
    }

    /// Best-effort network startup after a successful space-lifecycle
    /// action. Does not propagate errors: A1/A2 already committed the
    /// space mutation and rolling that back is worse than leaving the
    /// network offline until the user retries.
    async fn auto_start_network(&self) {
        if let Err(err) = self.network_control.start_network().await {
            warn!(
                error = %err,
                "start_network failed after space-lifecycle action; space state is \
                 committed, user must retry network via manual reconnect"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    //! Thin smoke tests — the two use cases themselves are covered
    //! exhaustively in `usecases::setup::{initialize_space,unlock_space}`.
    //! Here we only prove that `SpaceSetupFacade` wires them up and
    //! forwards arguments and error codes unchanged.

    use super::*;

    use std::sync::Mutex as StdMutex;

    use async_trait::async_trait;

    use chrono::{DateTime, Utc};

    use uc_core::crypto::domain::{ActiveSpace, Passphrase};
    use uc_core::ids::{DeviceId, SpaceId};
    use uc_core::membership::{MembershipError, SpaceMember};
    use uc_core::pairing::invitation::InvitationCode;
    use uc_core::ports::pairing_invitation::{
        InvitationError, IssuedInvitation, PairingInvitationPort,
    };
    use uc_core::ports::space::{SpaceAccessError, SpaceAccessPort};
    use uc_core::ports::{
        ClockPort, DeviceIdentityPort, LocalIdentityError, LocalIdentityPort, NetworkControlPort,
        SettingsPort, SetupStatusPort,
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

    #[derive(Default)]
    struct FakeNetworkControl {
        start_calls: StdMutex<u32>,
        stop_calls: StdMutex<u32>,
        start_err: StdMutex<Option<String>>,
    }

    impl FakeNetworkControl {
        fn start_calls(&self) -> u32 {
            *self.start_calls.lock().unwrap()
        }
        fn stop_calls(&self) -> u32 {
            *self.stop_calls.lock().unwrap()
        }
    }

    #[async_trait]
    impl NetworkControlPort for FakeNetworkControl {
        async fn start_network(&self) -> anyhow::Result<()> {
            *self.start_calls.lock().unwrap() += 1;
            if let Some(msg) = self.start_err.lock().unwrap().take() {
                return Err(anyhow::anyhow!(msg));
            }
            Ok(())
        }
        async fn stop_network(&self) -> anyhow::Result<()> {
            *self.stop_calls.lock().unwrap() += 1;
            Ok(())
        }
    }

    #[derive(Default)]
    struct FakeInvitationPort {
        calls: StdMutex<u32>,
        next_err: StdMutex<Option<InvitationError>>,
    }

    #[async_trait]
    impl PairingInvitationPort for FakeInvitationPort {
        async fn issue_invitation(&self) -> Result<IssuedInvitation, InvitationError> {
            *self.calls.lock().unwrap() += 1;
            if let Some(err) = self.next_err.lock().unwrap().take() {
                return Err(err);
            }
            Ok(IssuedInvitation {
                code: InvitationCode::new("SMOKE-0001"),
                expires_at: DateTime::parse_from_rfc3339("2026-04-20T10:05:00Z")
                    .unwrap()
                    .with_timezone(&Utc),
            })
        }
    }

    fn default_fingerprint() -> IdentityFingerprint {
        IdentityFingerprint::from_raw_string("ABCDEFGHIJKLMNOP").unwrap()
    }

    fn make_facade(
        space_access: Arc<dyn SpaceAccessPort>,
        setup_status: Arc<dyn SetupStatusPort>,
        settings: Arc<dyn SettingsPort>,
    ) -> (
        SpaceSetupFacade,
        Arc<FakeNetworkControl>,
        Arc<FakeInvitationPort>,
    ) {
        let network_control = Arc::new(FakeNetworkControl::default());
        let pairing_invitation = Arc::new(FakeInvitationPort::default());
        let facade = SpaceSetupFacade::new(SpaceSetupDeps {
            space_access,
            local_identity: Arc::new(FakeLocalIdentity {
                fp: default_fingerprint(),
            }),
            device_identity: Arc::new(FixedDeviceIdentity {
                id: DeviceId::new("device-1"),
            }),
            member_repo: Arc::new(InMemoryMemberRepo::default()),
            setup_status,
            settings,
            clock: Arc::new(FixedClock(0)),
            network_control: network_control.clone(),
            pairing_invitation: pairing_invitation.clone(),
        });
        (facade, network_control, pairing_invitation)
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
        let (facade, _net, _inv) = make_facade(
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
        let (facade, _net, _inv) = make_facade(
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
        let (facade, _net, _inv) = make_facade(
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
        let (facade, _net, _inv) = make_facade(
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
        let (facade, _net, _inv) = make_facade(
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

    // ── P6 · F1/F2 network wiring ────────────────────────────────────────

    #[tokio::test]
    async fn initialize_space_success_starts_network() {
        let (facade, net, _inv) = make_facade(
            Arc::new(FakeSpaceAccess::default()),
            Arc::new(InMemorySetupStatus::default()),
            settings_with_device_name("mac"),
        );
        let cmd = InitializeSpaceCommand {
            passphrase: Passphrase::new("hunter22hunter22"),
            passphrase_confirm: Passphrase::new("hunter22hunter22"),
            device_name: None,
        };
        facade.initialize_space(cmd).await.expect("A1 ok");
        assert_eq!(net.start_calls(), 1);
        assert_eq!(net.stop_calls(), 0);
    }

    #[tokio::test]
    async fn initialize_space_failure_does_not_start_network() {
        let (facade, net, _inv) = make_facade(
            Arc::new(FakeSpaceAccess::default()),
            Arc::new(InMemorySetupStatus::default()),
            settings_with_device_name("mac"),
        );
        let cmd = InitializeSpaceCommand {
            passphrase: Passphrase::new("hunter22hunter22"),
            passphrase_confirm: Passphrase::new("different22else2"),
            device_name: None,
        };
        let _ = facade.initialize_space(cmd).await.unwrap_err();
        assert_eq!(
            net.start_calls(),
            0,
            "A1 failure must not touch network runtime"
        );
    }

    #[tokio::test]
    async fn unlock_space_success_starts_network() {
        let setup_status = InMemorySetupStatus::default();
        *setup_status.status.lock().unwrap() = SetupStatus {
            has_completed: true,
        };
        let (facade, net, _inv) = make_facade(
            Arc::new(FakeSpaceAccess::default()),
            Arc::new(setup_status),
            Arc::new(InMemorySettings::default()),
        );
        let cmd = UnlockSpaceCommand {
            passphrase: Passphrase::new("hunter22hunter22"),
        };
        facade.unlock_space(cmd).await.expect("A2 ok");
        assert_eq!(net.start_calls(), 1);
    }

    #[tokio::test]
    async fn unlock_space_failure_does_not_start_network() {
        let (facade, net, _inv) = make_facade(
            Arc::new(FakeSpaceAccess::default()),
            Arc::new(InMemorySetupStatus::default()),
            Arc::new(InMemorySettings::default()),
        );
        let cmd = UnlockSpaceCommand {
            passphrase: Passphrase::new("hunter22hunter22"),
        };
        let _ = facade.unlock_space(cmd).await.unwrap_err();
        assert_eq!(
            net.start_calls(),
            0,
            "A2 failure (SetupNotCompleted) must not touch network runtime"
        );
    }

    #[tokio::test]
    async fn start_network_failure_does_not_fail_initialize_space() {
        let (facade, net, _inv) = make_facade(
            Arc::new(FakeSpaceAccess::default()),
            Arc::new(InMemorySetupStatus::default()),
            settings_with_device_name("mac"),
        );
        *net.start_err.lock().unwrap() = Some("bind failed".to_string());
        let cmd = InitializeSpaceCommand {
            passphrase: Passphrase::new("hunter22hunter22"),
            passphrase_confirm: Passphrase::new("hunter22hunter22"),
            device_name: None,
        };
        let out = facade
            .initialize_space(cmd)
            .await
            .expect("A1 result must not reflect network failure");
        assert_eq!(out.fingerprint, default_fingerprint());
        assert_eq!(net.start_calls(), 1);
    }

    #[tokio::test]
    async fn on_shutdown_stops_network() {
        let (facade, net, _inv) = make_facade(
            Arc::new(FakeSpaceAccess::default()),
            Arc::new(InMemorySetupStatus::default()),
            Arc::new(InMemorySettings::default()),
        );
        facade.on_shutdown().await;
        assert_eq!(net.stop_calls(), 1);
        assert_eq!(net.start_calls(), 0);
    }

    // ── B1 · issue pairing invitation wiring ─────────────────────────────

    #[tokio::test]
    async fn issue_pairing_invitation_forwards_happy_path() {
        let (facade, _net, inv) = make_facade(
            Arc::new(FakeSpaceAccess::default()),
            Arc::new(InMemorySetupStatus::default()),
            Arc::new(InMemorySettings::default()),
        );
        let out = facade.issue_pairing_invitation().await.expect("B1 ok");
        assert_eq!(out.code.as_str(), "SMOKE-0001");
        assert_eq!(*inv.calls.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn issue_pairing_invitation_forwards_network_not_started() {
        let (facade, _net, inv) = make_facade(
            Arc::new(FakeSpaceAccess::default()),
            Arc::new(InMemorySetupStatus::default()),
            Arc::new(InMemorySettings::default()),
        );
        *inv.next_err.lock().unwrap() = Some(InvitationError::NetworkNotStarted);
        let err = facade.issue_pairing_invitation().await.unwrap_err();
        assert!(matches!(
            err,
            IssuePairingInvitationError::NetworkNotStarted
        ));
    }

    #[tokio::test]
    async fn issue_pairing_invitation_does_not_auto_start_network() {
        let (facade, net, _inv) = make_facade(
            Arc::new(FakeSpaceAccess::default()),
            Arc::new(InMemorySetupStatus::default()),
            Arc::new(InMemorySettings::default()),
        );
        facade.issue_pairing_invitation().await.expect("B1 ok");
        assert_eq!(
            net.start_calls(),
            0,
            "B1 is not a space-lifecycle action and must not touch network runtime",
        );
    }
}
