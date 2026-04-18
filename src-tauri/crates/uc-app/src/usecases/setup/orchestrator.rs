//! Setup orchestrator.
//!
//! This module coordinates the setup state machine transitions and delegates
//! side-effect execution to `SetupActionExecutor`. The orchestrator remains
//! a thin dispatcher that owns session state and the state machine loop.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use rand::RngCore;
use tokio::sync::Mutex;
use tracing::{error, info, info_span, warn, Instrument};

use uc_application::setup::{SetupEvent, SetupEventPort, SetupState, SetupStateMachine};
use uc_core::{
    crypto::{
        model::{KeySlot, KeySlotFile, Passphrase},
        SecretString,
    },
    ids::{SessionId, SpaceId},
    ports::space::CryptoPort,
    ports::space::{PersistencePort, ProofPort, SpaceAccessTransportPort},
    ports::{DiscoveryPort, NetworkControlPort, PairingTransportPort, SetupStatusPort, TimerPort},
    setup::SetupStatus,
    space_access::{
        event::SpaceAccessEvent,
        state::{DenyReason, SpaceAccessState},
        SpaceAccessProofArtifact,
    },
};

use crate::usecases::initialize_encryption::InitializeEncryptionError;
use crate::usecases::setup::action_executor::SetupActionExecutor;
use crate::usecases::setup::context::SetupContext;
use crate::usecases::setup::MarkSetupComplete;
use crate::usecases::space_access::{
    SpaceAccessCryptoFactory, SpaceAccessExecutor, SpaceAccessFacade, SpaceAccessJoinerOffer,
};
use crate::usecases::AppLifecycleCoordinator;
use crate::usecases::InitializeEncryption;
use crate::usecases::SetupPairingFacadePort;

/// Errors produced by the setup orchestrator.
#[derive(Debug, thiserror::Error)]
pub enum SetupError {
    #[error("initialize encryption failed: {0}")]
    InitializeEncryption(#[from] InitializeEncryptionError),
    #[error("mark setup complete failed: {0}")]
    MarkSetupComplete(#[from] anyhow::Error),
    /// Failed to load setup status from persistent storage.
    #[error("load setup status failed: {0}")]
    StatusLoadFailed(#[source] anyhow::Error),
    #[error("setup reset failed: {0}")]
    ResetFailed(#[source] anyhow::Error),
    #[error("lifecycle boot failed: {0}")]
    LifecycleFailed(#[source] anyhow::Error),
    #[error("setup action not implemented: {0}")]
    ActionNotImplemented(&'static str),
    #[error("pairing operation failed")]
    PairingFailed,
}

/// Orchestrator that drives setup state transitions and delegates side effects
/// to `SetupActionExecutor`.
pub struct SetupOrchestrator {
    pub(super) context: Arc<SetupContext>,

    // Session state -- borrowed by action executor via method params
    pub(super) selected_peer_id: Arc<Mutex<Option<String>>>,
    pub(super) pairing_session_id: Arc<Mutex<Option<String>>>,
    pub(super) joiner_offer: Arc<Mutex<Option<SpaceAccessJoinerOffer>>>,
    pub(super) passphrase: Arc<Mutex<Option<Passphrase>>>,
    seeded: AtomicBool,
    seed_lock: Mutex<()>,

    // Retained ports (used only by orchestrator dispatch, not by actions)
    setup_status: Arc<dyn SetupStatusPort>,

    // Action executor handles all side-effect execution
    pub(super) action_executor: Arc<SetupActionExecutor>,
}

struct LoadedKeyslotSpaceAccessCrypto {
    keyslot_file: KeySlotFile,
}

#[async_trait::async_trait]
impl CryptoPort for LoadedKeyslotSpaceAccessCrypto {
    async fn generate_nonce32(&self) -> [u8; 32] {
        let mut nonce = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut nonce);
        nonce
    }

    async fn export_keyslot_blob(&self, _space_id: &SpaceId) -> anyhow::Result<KeySlot> {
        Ok(self.keyslot_file.clone().into())
    }

    async fn derive_master_key_from_keyslot(
        &self,
        _keyslot_blob: &[u8],
        _passphrase: SecretString,
    ) -> anyhow::Result<uc_core::crypto::MasterKey> {
        Err(anyhow::anyhow!(
            "loaded keyslot crypto cannot derive master key in sponsor flow"
        ))
    }
}

struct NoopRuntimeSpaceAccessCrypto;

#[async_trait::async_trait]
impl CryptoPort for NoopRuntimeSpaceAccessCrypto {
    async fn generate_nonce32(&self) -> [u8; 32] {
        [0u8; 32]
    }

    async fn export_keyslot_blob(&self, _space_id: &SpaceId) -> anyhow::Result<KeySlot> {
        Err(anyhow::anyhow!(
            "noop runtime space access crypto cannot export keyslot"
        ))
    }

    async fn derive_master_key_from_keyslot(
        &self,
        _keyslot_blob: &[u8],
        _passphrase: SecretString,
    ) -> anyhow::Result<uc_core::crypto::MasterKey> {
        Err(anyhow::anyhow!(
            "noop runtime space access crypto cannot derive master key"
        ))
    }
}

impl SetupOrchestrator {
    pub fn new(
        initialize_encryption: Arc<InitializeEncryption>,
        mark_setup_complete: Arc<MarkSetupComplete>,
        setup_status: Arc<dyn SetupStatusPort>,
        app_lifecycle: Arc<AppLifecycleCoordinator>,
        setup_pairing_facade: Arc<dyn SetupPairingFacadePort>,
        setup_event_port: Arc<dyn SetupEventPort>,
        space_access_facade: Arc<SpaceAccessFacade>,
        discovery_port: Arc<dyn DiscoveryPort>,
        network_control: Arc<dyn NetworkControlPort>,
        crypto_factory: Arc<dyn SpaceAccessCryptoFactory>,
        pairing_transport: Arc<dyn PairingTransportPort>,
        transport_port: Arc<Mutex<dyn SpaceAccessTransportPort>>,
        proof_port: Arc<dyn ProofPort>,
        timer_port: Arc<Mutex<dyn TimerPort>>,
        persistence_port: Arc<Mutex<dyn PersistencePort>>,
    ) -> Self {
        let action_executor = Arc::new(SetupActionExecutor {
            initialize_encryption,
            mark_setup_complete,
            app_lifecycle,
            setup_event_port,
            discovery_port,
            network_control,
            crypto_factory,
            pairing_transport,
            transport_port,
            proof_port,
            timer_port,
            persistence_port,
            setup_pairing_facade,
            space_access_facade,
        });

        Self {
            context: SetupContext::default().arc(),
            selected_peer_id: Arc::new(Mutex::new(None)),
            pairing_session_id: Arc::new(Mutex::new(None)),
            joiner_offer: Arc::new(Mutex::new(None)),
            passphrase: Arc::new(Mutex::new(None)),
            seeded: AtomicBool::new(false),
            seed_lock: Mutex::new(()),
            setup_status,
            action_executor,
        }
    }

    pub async fn new_space(&self) -> Result<SetupState, SetupError> {
        let event = SetupEvent::StartNewSpace;
        self.dispatch(event).await
    }

    pub async fn join_space(&self) -> Result<SetupState, SetupError> {
        let event = SetupEvent::StartJoinSpace;
        self.dispatch(event).await
    }

    pub async fn select_device(&self, peer_id: String) -> Result<SetupState, SetupError> {
        let event = SetupEvent::ChooseJoinPeer { peer_id };
        self.dispatch(event).await
    }

    pub async fn submit_passphrase(
        &self,
        pass1: String,
        _pass2: String,
    ) -> Result<SetupState, SetupError> {
        let event = SetupEvent::SubmitPassphrase {
            passphrase: SecretString::new(pass1),
        };
        self.dispatch(event).await
    }

    pub async fn verify_passphrase(&self, passphrase: String) -> Result<SetupState, SetupError> {
        let event = SetupEvent::VerifyPassphrase {
            passphrase: SecretString::new(passphrase),
        };
        self.dispatch(event).await
    }

    pub async fn confirm_peer_trust(&self) -> Result<SetupState, SetupError> {
        let event = SetupEvent::ConfirmPeerTrust;
        self.dispatch(event).await
    }

    pub async fn cancel_setup(&self) -> Result<SetupState, SetupError> {
        let event = SetupEvent::CancelSetup;
        self.dispatch(event).await
    }

    /// Complete the join space flow. Called by the frontend when the daemon
    /// emits `setup.spaceAccessCompleted` via the WebSocket bridge.
    pub async fn complete_join_space(&self) -> Result<SetupState, SetupError> {
        let event = SetupEvent::JoinSpaceSucceeded;
        self.dispatch(event).await
    }

    pub async fn reset(&self) -> Result<SetupState, SetupError> {
        let _dispatch_guard = self.context.acquire_dispatch_lock().await;

        self.clear_runtime_session_state().await;
        self.setup_status
            .set_status(&SetupStatus::default())
            .await
            .map_err(SetupError::ResetFailed)?;
        SetupActionExecutor::set_state_and_emit(
            &self.context,
            &self.action_executor.setup_event_port,
            SetupState::Welcome,
            None,
        )
        .await;
        self.seeded.store(false, Ordering::SeqCst);

        Ok(SetupState::Welcome)
    }

    /// Clears in-memory setup session state and any active pairing session.
    ///
    /// Unlike [`reset`](Self::reset), this preserves the device's completion status
    /// stored in persistent storage. Returns the base state derived from
    /// `SetupStatus.has_completed`: [`SetupState::Completed`] if the device has
    /// previously completed setup, or [`SetupState::Welcome`] otherwise.
    pub async fn clear_transient_state(&self) -> Result<SetupState, SetupError> {
        let _dispatch_guard = self.context.acquire_dispatch_lock().await;

        self.clear_runtime_session_state().await;

        let status = self
            .setup_status
            .get_status()
            .await
            .map_err(SetupError::StatusLoadFailed)?;
        let base_state = Self::state_from_status(&status);

        SetupActionExecutor::set_state_and_emit(
            &self.context,
            &self.action_executor.setup_event_port,
            base_state.clone(),
            None,
        )
        .await;
        self.seeded.store(true, Ordering::SeqCst);

        Ok(base_state)
    }

    pub async fn get_state(&self) -> SetupState {
        self.seed_state_from_status().await;
        self.context.get_state().await
    }

    pub async fn start_completed_host_sponsor_authorization(
        &self,
        pairing_session_id: String,
        sponsor_peer_id: String,
        keyslot_file: KeySlotFile,
    ) -> Result<SpaceAccessState, SetupError> {
        let current_state = self.get_state().await;
        if !matches!(current_state, SetupState::Completed) {
            return Err(SetupError::PairingFailed);
        }

        self.action_executor
            .space_access_facade
            .set_sponsor_peer_id(Some(sponsor_peer_id))
            .await;

        let space_id = SpaceId::from(keyslot_file.scope.profile_id.as_str());
        let typed_session_id = SessionId::from(pairing_session_id);
        self.dispatch_space_access_event_with_crypto(
            LoadedKeyslotSpaceAccessCrypto { keyslot_file },
            SpaceAccessEvent::SponsorAuthorizationRequested {
                pairing_session_id: typed_session_id.clone(),
                space_id,
                ttl_secs: 60,
            },
            typed_session_id,
        )
        .await
    }

    pub async fn resolve_host_space_access_proof(
        &self,
        proof: SpaceAccessProofArtifact,
        sponsor_peer_id: Option<String>,
    ) -> Result<SpaceAccessState, SetupError> {
        let current_pairing_session_id =
            match self.action_executor.space_access_facade.get_state().await {
                SpaceAccessState::WaitingJoinerProof {
                    pairing_session_id, ..
                } => pairing_session_id,
                _ => return Err(SetupError::PairingFailed),
            };

        if let Some(peer_id) = sponsor_peer_id {
            self.action_executor
                .space_access_facade
                .set_sponsor_peer_id(Some(peer_id))
                .await;
        }
        let Some(expected) = self
            .action_executor
            .space_access_facade
            .peek_prepared_offer()
            .await
        else {
            return Err(SetupError::PairingFailed);
        };

        let event = if proof.pairing_session_id != current_pairing_session_id {
            SpaceAccessEvent::ProofRejected {
                pairing_session_id: proof.pairing_session_id.clone(),
                space_id: proof.space_id.clone(),
                reason: DenyReason::SessionMismatch,
            }
        } else if proof.space_id != expected.space_id {
            SpaceAccessEvent::ProofRejected {
                pairing_session_id: proof.pairing_session_id.clone(),
                space_id: proof.space_id.clone(),
                reason: DenyReason::SpaceMismatch,
            }
        } else {
            let verified = self
                .action_executor
                .proof_port
                .verify_proof(&proof, expected.nonce)
                .await
                .map_err(|_| SetupError::PairingFailed)?;

            if verified {
                SpaceAccessEvent::ProofVerified {
                    pairing_session_id: proof.pairing_session_id.clone(),
                    space_id: proof.space_id.clone(),
                }
            } else {
                SpaceAccessEvent::ProofRejected {
                    pairing_session_id: proof.pairing_session_id.clone(),
                    space_id: proof.space_id.clone(),
                    reason: DenyReason::InvalidProof,
                }
            }
        };

        self.dispatch_space_access_event_with_crypto(
            NoopRuntimeSpaceAccessCrypto,
            event,
            proof.pairing_session_id.clone(),
        )
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
        if let Some(peer_id) = sponsor_peer_id {
            self.action_executor
                .space_access_facade
                .set_sponsor_peer_id(Some(peer_id))
                .await;
        }

        let typed_session_id = SessionId::from(pairing_session_id);
        let event = if success {
            SpaceAccessEvent::AccessGranted {
                pairing_session_id: typed_session_id.clone(),
                space_id,
            }
        } else {
            SpaceAccessEvent::AccessDenied {
                pairing_session_id: typed_session_id.clone(),
                space_id,
                reason: deny_reason.unwrap_or(DenyReason::InternalError),
            }
        };

        self.dispatch_space_access_event_with_crypto(
            NoopRuntimeSpaceAccessCrypto,
            event,
            typed_session_id,
        )
        .await
    }

    async fn dispatch(&self, event: SetupEvent) -> Result<SetupState, SetupError> {
        let event = self.capture_context(event).await;
        let _dispatch_guard = self.context.acquire_dispatch_lock().await;

        let span = info_span!("usecase.setup_orchestrator.dispatch", event = ?event);
        async {
            let mut current = self.context.get_state().await;
            let mut pending_events = vec![event];

            while let Some(event) = pending_events.pop() {
                let from = current.clone();
                let event_name = format!("{:?}", event);
                let (next, actions) = SetupStateMachine::transition(current, event);
                info!(from = ?from, to = ?next, event = %event_name, "setup state transition");
                let follow_up_events = self
                    .action_executor
                    .execute_actions(
                        actions,
                        &self.passphrase,
                        &self.selected_peer_id,
                        &self.pairing_session_id,
                        &self.joiner_offer,
                        &self.context,
                    )
                    .await?;
                SetupActionExecutor::set_state_and_emit(
                    &self.context,
                    &self.action_executor.setup_event_port,
                    next.clone(),
                    self.current_pairing_session_id().await,
                )
                .await;
                current = next;
                pending_events.extend(follow_up_events);
            }

            Ok(current)
        }
        .instrument(span)
        .await
    }

    async fn capture_context(&self, event: SetupEvent) -> SetupEvent {
        match event {
            SetupEvent::ChooseJoinPeer { peer_id } => {
                *self.selected_peer_id.lock().await = Some(peer_id.clone());
                SetupEvent::ChooseJoinPeer { peer_id }
            }
            SetupEvent::SubmitPassphrase { passphrase } => {
                let (event_passphrase, stored_passphrase) = Self::split_passphrase(passphrase);
                *self.passphrase.lock().await = Some(stored_passphrase);
                SetupEvent::SubmitPassphrase {
                    passphrase: event_passphrase,
                }
            }
            SetupEvent::VerifyPassphrase { passphrase } => {
                let (event_passphrase, stored_passphrase) = Self::split_passphrase(passphrase);
                *self.passphrase.lock().await = Some(stored_passphrase);
                SetupEvent::VerifyPassphrase {
                    passphrase: event_passphrase,
                }
            }
            other => other,
        }
    }

    async fn dispatch_space_access_event_with_crypto<C>(
        &self,
        crypto: C,
        event: SpaceAccessEvent,
        pairing_session_id: SessionId,
    ) -> Result<SpaceAccessState, SetupError>
    where
        C: CryptoPort,
    {
        let mut transport = self.action_executor.transport_port.lock().await;
        let mut timer = self.action_executor.timer_port.lock().await;
        let mut store = self.action_executor.persistence_port.lock().await;
        let mut executor = SpaceAccessExecutor {
            crypto: &crypto,
            transport: &mut *transport,
            proof: self.action_executor.proof_port.as_ref(),
            timer: &mut *timer,
            store: &mut *store,
        };

        self.action_executor
            .space_access_facade
            .dispatch(&mut executor, event, Some(pairing_session_id))
            .await
            .map_err(|_| SetupError::PairingFailed)
    }

    fn split_passphrase(passphrase: SecretString) -> (SecretString, Passphrase) {
        let raw = passphrase.into_inner();
        let stored = Passphrase(raw.clone());
        (SecretString::new(raw), stored)
    }

    async fn current_pairing_session_id(&self) -> Option<String> {
        let guard = self.pairing_session_id.lock().await;
        guard.clone()
    }

    /// Clears in-memory session state: selected peer, pairing session, joiner offer,
    /// and passphrase. Rejects any active pairing session.
    async fn clear_runtime_session_state(&self) {
        if let Some(session_id) = self.pairing_session_id.lock().await.take() {
            if let Err(error) = self
                .action_executor
                .setup_pairing_facade
                .reject_pairing(&session_id)
                .await
            {
                warn!(
                    error = %error,
                    session_id = %session_id,
                    "failed to reject setup pairing session during setup state clear"
                );
            }
        }

        self.selected_peer_id.lock().await.take();
        self.joiner_offer.lock().await.take();
        self.passphrase.lock().await.take();
        self.action_executor.space_access_facade.reset().await;
    }

    /// Derives the base [`SetupState`] from persisted [`SetupStatus`].
    ///
    /// Returns [`SetupState::Completed`] if `has_completed` is true,
    /// otherwise [`SetupState::Welcome`].
    fn state_from_status(status: &SetupStatus) -> SetupState {
        if status.has_completed {
            SetupState::Completed
        } else {
            SetupState::Welcome
        }
    }

    async fn seed_state_from_status(&self) {
        if self.seeded.load(Ordering::SeqCst) {
            return;
        }

        let _seed_guard = self.seed_lock.lock().await;
        if self.seeded.load(Ordering::SeqCst) {
            return;
        }

        match self.setup_status.get_status().await {
            Ok(status) => {
                let base_state = Self::state_from_status(&status);
                if matches!(base_state, SetupState::Completed) {
                    SetupActionExecutor::set_state_and_emit(
                        &self.context,
                        &self.action_executor.setup_event_port,
                        base_state,
                        None,
                    )
                    .await;
                }
            }
            Err(err) => {
                error!(error = %err, "failed to load setup status");
            }
        }

        self.seeded.store(true, Ordering::SeqCst);
    }
}
