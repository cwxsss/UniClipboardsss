//! `SetupFacade` — stable application-layer entry point for setup (phase B.4).
//!
//! # Architecture
//!
//! ```text
//! External (daemon / tauri / cli)
//!         ↓
//!     SetupFacade              ← the only `pub` surface
//!         ↓
//!     SetupOrchestrator        ← pub(crate), hidden from consumers
//!         ↓
//!     Ports (uc-core + setup::ports)
//! ```
//!
//! Per `uc-application/AGENTS.md` §11.4 external consumers never see
//! `SetupOrchestrator` directly. They drive setup exclusively through
//! `SetupFacade`, which composes a private `Arc<SetupOrchestrator>` with one
//! UseCase/Query per user intent or coordination action (B.3). Refactoring
//! the orchestrator's internal signatures is therefore no longer a
//! breaking change for daemon/tauri/cli.

use std::sync::Arc;

use tokio::sync::Mutex;
use uc_core::{
    ids::SpaceId,
    ports::space::{PersistencePort, ProofPort, SpaceAccessTransportPort},
    ports::{DiscoveryPort, NetworkControlPort, PairingTransportPort, SetupStatusPort, TimerPort},
    space_access::{
        state::{DenyReason, SpaceAccessState},
        SpaceAccessProofArtifact,
    },
};

use super::event_port::SetupEventPort;
use super::mark_complete::MarkSetupComplete;
use super::orchestrator::{SetupError, SetupOrchestrator};
use super::pairing_facade::SetupPairingFacadePort;
use super::ports::{SetupAppLifecyclePort, SetupInitializeEncryptionPort};
use super::state::SetupState;
use super::usecases::{
    ApplyJoinerSpaceAccessResultUseCase, CancelSetupUseCase, ClearSetupTransientStateUseCase,
    CompleteJoinSpaceUseCase, ConfirmPeerTrustUseCase, GetSetupStateQuery, ResetSetupUseCase,
    ResolveHostSpaceAccessProofUseCase, SelectJoinPeerUseCase, StartJoinSpaceUseCase,
    StartNewSpaceUseCase, StartSponsorAuthorizationForJoinerUseCase,
    SubmitNewSpacePassphraseUseCase, VerifyJoinPassphraseUseCase,
};
use uc_core::ports::space::SpaceAccessPort;

use crate::space_access::SpaceAccessFacade;

/// Stable application-layer entry point for the setup module.
///
/// Owns the orchestrator and per-intent UseCases/Query. External consumers
/// (daemon / tauri / cli) drive setup exclusively through this type.
pub struct SetupFacade {
    // Kept to forward internal-only calls (none currently expected from
    // outside the crate, but retained for the shim re-export and future
    // in-crate tests). External code must not reach through this.
    #[allow(dead_code)]
    orchestrator: Arc<SetupOrchestrator>,

    start_new_space: StartNewSpaceUseCase,
    start_join_space: StartJoinSpaceUseCase,
    select_join_peer: SelectJoinPeerUseCase,
    confirm_peer_trust: ConfirmPeerTrustUseCase,
    submit_new_space_passphrase: SubmitNewSpacePassphraseUseCase,
    verify_join_passphrase: VerifyJoinPassphraseUseCase,
    complete_join_space: CompleteJoinSpaceUseCase,
    cancel_setup: CancelSetupUseCase,
    reset_setup: ResetSetupUseCase,
    clear_transient_state: ClearSetupTransientStateUseCase,
    get_setup_state: GetSetupStateQuery,
    start_sponsor_auth_for_joiner: StartSponsorAuthorizationForJoinerUseCase,
    resolve_host_space_access_proof: ResolveHostSpaceAccessProofUseCase,
    apply_joiner_space_access_result: ApplyJoinerSpaceAccessResultUseCase,
}

impl SetupFacade {
    /// Construct a fully wired `SetupFacade`.
    ///
    /// Internally builds `MarkSetupComplete` from `setup_status` and
    /// composes `SetupOrchestrator` with all 14 UseCases/Query. Bootstrap
    /// now passes the setup-status port plus the ports/adapters that the
    /// orchestrator still needs.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        initialize_encryption: Arc<dyn SetupInitializeEncryptionPort>,
        setup_status: Arc<dyn SetupStatusPort>,
        app_lifecycle: Arc<dyn SetupAppLifecyclePort>,
        setup_pairing_facade: Arc<dyn SetupPairingFacadePort>,
        setup_event_port: Arc<dyn SetupEventPort>,
        space_access_facade: Arc<SpaceAccessFacade>,
        discovery_port: Arc<dyn DiscoveryPort>,
        network_control: Arc<dyn NetworkControlPort>,
        space_access_port: Arc<dyn SpaceAccessPort>,
        pairing_transport: Arc<dyn PairingTransportPort>,
        transport_port: Arc<Mutex<dyn SpaceAccessTransportPort>>,
        proof_port: Arc<dyn ProofPort>,
        timer_port: Arc<Mutex<dyn TimerPort>>,
        persistence_port: Arc<Mutex<dyn PersistencePort>>,
    ) -> Self {
        let mark_setup_complete = Arc::new(MarkSetupComplete::from_ports(setup_status.clone()));

        let orchestrator = Arc::new(SetupOrchestrator::new(
            initialize_encryption,
            mark_setup_complete,
            setup_status,
            app_lifecycle,
            setup_pairing_facade,
            setup_event_port,
            space_access_facade,
            discovery_port,
            network_control,
            space_access_port,
            pairing_transport,
            transport_port,
            proof_port,
            timer_port,
            persistence_port,
        ));

        Self {
            start_new_space: StartNewSpaceUseCase::new(Arc::clone(&orchestrator)),
            start_join_space: StartJoinSpaceUseCase::new(Arc::clone(&orchestrator)),
            select_join_peer: SelectJoinPeerUseCase::new(Arc::clone(&orchestrator)),
            confirm_peer_trust: ConfirmPeerTrustUseCase::new(Arc::clone(&orchestrator)),
            submit_new_space_passphrase: SubmitNewSpacePassphraseUseCase::new(Arc::clone(
                &orchestrator,
            )),
            verify_join_passphrase: VerifyJoinPassphraseUseCase::new(Arc::clone(&orchestrator)),
            complete_join_space: CompleteJoinSpaceUseCase::new(Arc::clone(&orchestrator)),
            cancel_setup: CancelSetupUseCase::new(Arc::clone(&orchestrator)),
            reset_setup: ResetSetupUseCase::new(Arc::clone(&orchestrator)),
            clear_transient_state: ClearSetupTransientStateUseCase::new(Arc::clone(&orchestrator)),
            get_setup_state: GetSetupStateQuery::new(Arc::clone(&orchestrator)),
            start_sponsor_auth_for_joiner: StartSponsorAuthorizationForJoinerUseCase::new(
                Arc::clone(&orchestrator),
            ),
            resolve_host_space_access_proof: ResolveHostSpaceAccessProofUseCase::new(Arc::clone(
                &orchestrator,
            )),
            apply_joiner_space_access_result: ApplyJoinerSpaceAccessResultUseCase::new(Arc::clone(
                &orchestrator,
            )),
            orchestrator,
        }
    }

    // ── User-intent actions (routed through UseCases) ────────────────

    pub async fn new_space(&self) -> Result<SetupState, SetupError> {
        self.start_new_space.execute().await
    }

    pub async fn join_space(&self) -> Result<SetupState, SetupError> {
        self.start_join_space.execute().await
    }

    pub async fn select_device(&self, peer_id: String) -> Result<SetupState, SetupError> {
        self.select_join_peer.execute(peer_id).await
    }

    pub async fn confirm_peer_trust(&self) -> Result<SetupState, SetupError> {
        self.confirm_peer_trust.execute().await
    }

    pub async fn submit_passphrase(
        &self,
        passphrase: String,
        confirm: String,
    ) -> Result<SetupState, SetupError> {
        self.submit_new_space_passphrase
            .execute(passphrase, confirm)
            .await
    }

    pub async fn verify_passphrase(&self, passphrase: String) -> Result<SetupState, SetupError> {
        self.verify_join_passphrase.execute(passphrase).await
    }

    pub async fn complete_join_space(&self) -> Result<SetupState, SetupError> {
        self.complete_join_space.execute().await
    }

    pub async fn cancel_setup(&self) -> Result<SetupState, SetupError> {
        self.cancel_setup.execute().await
    }

    pub async fn reset(&self) -> Result<SetupState, SetupError> {
        self.reset_setup.execute().await
    }

    pub async fn clear_transient_state(&self) -> Result<SetupState, SetupError> {
        self.clear_transient_state.execute().await
    }

    // ── Query ────────────────────────────────────────────────────────

    pub async fn get_state(&self) -> SetupState {
        self.get_setup_state.execute().await
    }

    // ── Space-access coordination (routed through UseCases) ──────────

    pub async fn start_completed_host_sponsor_authorization(
        &self,
        pairing_session_id: String,
        sponsor_peer_id: String,
        space_id: SpaceId,
    ) -> Result<SpaceAccessState, SetupError> {
        self.start_sponsor_auth_for_joiner
            .execute(pairing_session_id, sponsor_peer_id, space_id)
            .await
    }

    pub async fn resolve_host_space_access_proof(
        &self,
        proof: SpaceAccessProofArtifact,
        sponsor_peer_id: Option<String>,
    ) -> Result<SpaceAccessState, SetupError> {
        self.resolve_host_space_access_proof
            .execute(proof, sponsor_peer_id)
            .await
    }

    pub async fn apply_joiner_space_access_result(
        &self,
        pairing_session_id: String,
        space_id: SpaceId,
        sponsor_peer_id: Option<String>,
        success: bool,
        deny_reason: Option<DenyReason>,
    ) -> Result<SpaceAccessState, SetupError> {
        self.apply_joiner_space_access_result
            .execute(
                pairing_session_id,
                space_id,
                sponsor_peer_id,
                success,
                deny_reason,
            )
            .await
    }
}
