//! Setup action executor.
//!
//! Handles side-effect execution for setup actions, delegating to infrastructure
//! ports. The orchestrator remains a thin dispatcher that coordinates state
//! machine transitions and delegates action execution here.

use std::sync::Arc;

use chrono::{Duration as ChronoDuration, Utc};
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};
use tracing::{debug, error, info, warn};

use uc_core::{
    network::pairing_state_machine::FailureReason,
    ports::space::{PersistencePort, ProofPort, SpaceAccessTransportPort},
    ports::{DiscoveryPort, NetworkControlPort, PairingTransportPort, SetupEventPort, TimerPort},
    security::space_access::{
        event::SpaceAccessEvent,
        state::{DenyReason, SpaceAccessState},
    },
    security::{model::Passphrase, SecretString},
    setup::{SetupAction, SetupError as SetupDomainError, SetupEvent, SetupState},
};

use crate::usecases::pairing::{PairingDomainEvent, PairingEventPort, PairingOrchestrator};
use crate::usecases::setup::context::SetupContext;
use crate::usecases::setup::MarkSetupComplete;
use crate::usecases::space_access::{
    SpaceAccessCryptoFactory, SpaceAccessExecutor, SpaceAccessJoinerOffer, SpaceAccessOrchestrator,
};
use crate::usecases::AppLifecycleCoordinator;
use crate::usecases::InitializeEncryption;

use super::orchestrator::SetupError;

#[cfg(test)]
const JOINER_OFFER_WAIT_TIMEOUT: Duration = Duration::from_millis(300);
#[cfg(not(test))]
const JOINER_OFFER_WAIT_TIMEOUT: Duration = Duration::from_secs(3);
const JOINER_OFFER_POLL_INTERVAL: Duration = Duration::from_millis(20);

/// Executes side-effect actions for setup state transitions.
///
/// Owns port references that were previously held by `SetupOrchestrator`.
/// Session state (selected_peer_id, pairing_session_id, etc.) is passed as
/// method parameters to avoid circular references back to the orchestrator.
pub struct SetupActionExecutor {
    // Use-case ports
    pub(super) initialize_encryption: Arc<InitializeEncryption>,
    pub(super) mark_setup_complete: Arc<MarkSetupComplete>,
    pub(super) app_lifecycle: Arc<AppLifecycleCoordinator>,
    pub(super) setup_event_port: Arc<dyn SetupEventPort>,

    // Infrastructure ports
    pub(super) discovery_port: Arc<dyn DiscoveryPort>,
    pub(super) network_control: Arc<dyn NetworkControlPort>,
    pub(super) crypto_factory: Arc<dyn SpaceAccessCryptoFactory>,
    pub(super) pairing_transport: Arc<dyn PairingTransportPort>,
    pub(super) transport_port: Arc<Mutex<dyn SpaceAccessTransportPort>>,
    pub(super) proof_port: Arc<dyn ProofPort>,
    pub(super) timer_port: Arc<Mutex<dyn TimerPort>>,
    pub(super) persistence_port: Arc<Mutex<dyn PersistencePort>>,

    // Collaborator orchestrators
    pub(super) pairing_orchestrator: Arc<PairingOrchestrator>,
    pub(super) space_access_orchestrator: Arc<SpaceAccessOrchestrator>,
}

impl SetupActionExecutor {
    /// Execute a list of setup actions and return follow-up events.
    pub async fn execute_actions(
        &self,
        actions: Vec<SetupAction>,
        passphrase: &Arc<Mutex<Option<Passphrase>>>,
        selected_peer_id: &Arc<Mutex<Option<String>>>,
        pairing_session_id: &Arc<Mutex<Option<String>>>,
        joiner_offer: &Arc<Mutex<Option<SpaceAccessJoinerOffer>>>,
        context: &Arc<SetupContext>,
    ) -> Result<Vec<SetupEvent>, SetupError> {
        let mut follow_up_events = Vec::new();
        for action in actions {
            debug!(?action, "setup executing action");
            match action {
                SetupAction::CreateEncryptedSpace => {
                    let pp = Self::take_passphrase(passphrase).await?;
                    self.initialize_encryption.execute(pp).await?;
                    follow_up_events.push(SetupEvent::CreateSpaceSucceeded);
                    debug!("setup action CreateEncryptedSpace completed");
                }
                SetupAction::MarkSetupComplete => {
                    self.app_lifecycle
                        .ensure_ready()
                        .await
                        .map_err(SetupError::LifecycleFailed)?;
                    self.mark_setup_complete.execute().await?;
                    debug!("setup action MarkSetupComplete completed");
                }
                SetupAction::EnsureDiscovery => {
                    info!("setup ensure discovery: requesting network start");
                    self.network_control.start_network().await.map_err(|err| {
                        error!(
                            action = "EnsureDiscovery",
                            step = "start_network",
                            error = %err,
                            "setup ensure discovery failed"
                        );
                        SetupError::PairingFailed
                    })?;

                    let discovered_peers = self
                        .discovery_port
                        .list_discovered_peers()
                        .await
                        .map_err(|err| {
                            error!(
                                action = "EnsureDiscovery",
                                step = "list_discovered_peers",
                                error = %err,
                                "setup ensure discovery failed"
                            );
                            SetupError::PairingFailed
                        })?;

                    info!(
                        discovered_peer_count = discovered_peers.len(),
                        "setup ensure discovery: initial discovered peer snapshot"
                    );

                    debug!("setup action EnsureDiscovery completed");
                }
                SetupAction::EnsurePairing => {
                    self.ensure_pairing_session(
                        selected_peer_id,
                        pairing_session_id,
                        joiner_offer,
                        context,
                    )
                    .await?;
                    debug!("setup action EnsurePairing completed");
                }
                SetupAction::ConfirmPeerTrust => {
                    self.confirm_peer_trust_action(pairing_session_id, context)
                        .await?;
                    debug!("setup action ConfirmPeerTrust completed");
                }
                SetupAction::AbortPairing => {
                    self.abort_pairing_session(pairing_session_id, selected_peer_id)
                        .await;
                    debug!("setup action AbortPairing completed");
                }
                SetupAction::StartJoinSpaceAccess => {
                    self.start_join_space_access(
                        passphrase,
                        pairing_session_id,
                        selected_peer_id,
                        joiner_offer,
                        context,
                    )
                    .await?;
                    debug!("setup action StartJoinSpaceAccess completed");
                }
            }
        }

        Ok(follow_up_events)
    }

    async fn take_passphrase(
        passphrase: &Arc<Mutex<Option<Passphrase>>>,
    ) -> Result<Passphrase, SetupError> {
        let mut guard = passphrase.lock().await;
        guard
            .take()
            .ok_or(SetupError::ActionNotImplemented("CreateEncryptedSpace"))
    }

    async fn start_join_space_access(
        &self,
        passphrase: &Arc<Mutex<Option<Passphrase>>>,
        pairing_session_id: &Arc<Mutex<Option<String>>>,
        selected_peer_id: &Arc<Mutex<Option<String>>>,
        joiner_offer: &Arc<Mutex<Option<SpaceAccessJoinerOffer>>>,
        context: &Arc<SetupContext>,
    ) -> Result<(), SetupError> {
        let passphrase_val = {
            let guard = passphrase.lock().await;
            guard.as_ref().map(|p| SecretString::new(p.0.clone()))
        }
        .ok_or_else(|| {
            error!("start join space access requested without passphrase");
            SetupError::PairingFailed
        })?;

        let session_id = {
            let guard = pairing_session_id.lock().await;
            guard.clone()
        }
        .ok_or_else(|| {
            error!("start join space access requested without pairing session id");
            SetupError::PairingFailed
        })?;

        let peer_id = {
            let guard = selected_peer_id.lock().await;
            guard.clone()
        }
        .ok_or_else(|| {
            error!("start join space access requested without selected peer");
            SetupError::PairingFailed
        })?;

        self.pairing_transport
            .open_pairing_session(peer_id, session_id.clone())
            .await
            .map_err(|err| {
                error!(
                    error = %err,
                    pairing_session_id = %session_id,
                    "failed to reopen pairing session for space access"
                );
                SetupError::PairingFailed
            })?;

        self.start_space_access_result_listener(session_id.clone(), context, pairing_session_id)
            .await;

        let offer = self
            .wait_for_joiner_offer(joiner_offer, &session_id)
            .await
            .ok_or_else(|| {
                error!(
                    pairing_session_id = %session_id,
                    timeout_ms = JOINER_OFFER_WAIT_TIMEOUT.as_millis(),
                    "start join space access requested without received offer"
                );
                SetupError::PairingFailed
            })?;

        {
            let mut guard = joiner_offer.lock().await;
            *guard = Some(offer.clone());
        }

        {
            let sa_context = self.space_access_orchestrator.context();
            let mut guard = sa_context.lock().await;
            guard.joiner_offer = Some(offer.clone());
            guard.joiner_passphrase = Some(SecretString::new(passphrase_val.expose().to_string()));
            guard.sponsor_peer_id = selected_peer_id.lock().await.clone();
        }

        let crypto = self.crypto_factory.build(passphrase_val);
        let mut transport = self.transport_port.lock().await;
        let mut timer = self.timer_port.lock().await;
        let mut store = self.persistence_port.lock().await;
        let mut executor = SpaceAccessExecutor {
            crypto: crypto.as_ref(),
            transport: &mut *transport,
            proof: self.proof_port.as_ref(),
            timer: &mut *timer,
            store: &mut *store,
        };

        let space_id = offer.space_id.clone();

        self.space_access_orchestrator
            .dispatch(
                &mut executor,
                SpaceAccessEvent::JoinRequested {
                    pairing_session_id: session_id.clone(),
                    ttl_secs: 60,
                },
                Some(session_id.clone()),
            )
            .await
            .map_err(|err| {
                error!(
                    error = %err,
                    pairing_session_id = %session_id,
                    "space access join requested failed during setup"
                );
                SetupError::PairingFailed
            })?;

        self.space_access_orchestrator
            .dispatch(
                &mut executor,
                SpaceAccessEvent::OfferAccepted {
                    pairing_session_id: session_id.clone(),
                    space_id: space_id.clone(),
                    expires_at: Utc::now() + ChronoDuration::seconds(60),
                },
                Some(session_id.clone()),
            )
            .await
            .map_err(|err| {
                error!(
                    error = %err,
                    pairing_session_id = %session_id,
                    "space access offer accepted failed during setup"
                );
                SetupError::PairingFailed
            })?;

        self.space_access_orchestrator
            .dispatch(
                &mut executor,
                SpaceAccessEvent::PassphraseSubmitted,
                Some(session_id.clone()),
            )
            .await
            .map_err(|err| {
                error!(
                    error = %err,
                    pairing_session_id = %session_id,
                    "space access passphrase submitted failed during setup"
                );
                SetupError::PairingFailed
            })?;

        Ok(())
    }

    async fn wait_for_joiner_offer(
        &self,
        joiner_offer: &Arc<Mutex<Option<SpaceAccessJoinerOffer>>>,
        pairing_session_id: &str,
    ) -> Option<SpaceAccessJoinerOffer> {
        let mut waited = Duration::ZERO;

        loop {
            if let Some(local_offer) = {
                let guard = joiner_offer.lock().await;
                guard.clone()
            } {
                return Some(local_offer);
            }

            let context_offer = {
                let sa_context = self.space_access_orchestrator.context();
                let guard = sa_context.lock().await;
                guard.joiner_offer.clone()
            };

            if context_offer.is_some() {
                return context_offer;
            }

            if waited >= JOINER_OFFER_WAIT_TIMEOUT {
                return None;
            }

            if waited == Duration::ZERO {
                warn!(
                    pairing_session_id = %pairing_session_id,
                    timeout_ms = JOINER_OFFER_WAIT_TIMEOUT.as_millis(),
                    "waiting for joiner offer before starting join space access"
                );
            }

            sleep(JOINER_OFFER_POLL_INTERVAL).await;
            waited += JOINER_OFFER_POLL_INTERVAL;
        }
    }

    async fn start_space_access_result_listener(
        &self,
        session_id: String,
        context: &Arc<SetupContext>,
        pairing_session_id: &Arc<Mutex<Option<String>>>,
    ) {
        let context = Arc::clone(context);
        let setup_event_port = Arc::clone(&self.setup_event_port);
        let mark_setup_complete = Arc::clone(&self.mark_setup_complete);
        let app_lifecycle = Arc::clone(&self.app_lifecycle);
        let pairing_session_id = Arc::clone(pairing_session_id);
        let space_access_orchestrator = Arc::clone(&self.space_access_orchestrator);

        tokio::spawn(async move {
            loop {
                if !Self::pairing_session_matches(&pairing_session_id, &session_id).await {
                    break;
                }

                let space_access_state = space_access_orchestrator.get_state().await;
                let Some(setup_event) =
                    Self::map_setup_event_from_space_access_state(&space_access_state, &session_id)
                else {
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    continue;
                };

                let _dispatch_guard = context.acquire_dispatch_lock().await;
                let current = context.get_state().await;
                let (next, actions) =
                    uc_core::setup::SetupStateMachine::transition(current, setup_event);

                for action in actions {
                    if matches!(action, SetupAction::MarkSetupComplete) {
                        if let Err(err) = app_lifecycle.ensure_ready().await {
                            error!(
                                error = %err,
                                session_id = %session_id,
                                "lifecycle ensure_ready failed after space access completion"
                            );
                            let failed_state = SetupState::JoinSpaceSelectDevice {
                                error: Some(SetupDomainError::PairingFailed),
                            };
                            context.set_state(failed_state.clone()).await;
                            setup_event_port
                                .emit_setup_state_changed(failed_state, Some(session_id.clone()))
                                .await;
                            return;
                        }

                        if let Err(err) = mark_setup_complete.execute().await {
                            error!(
                                error = %err,
                                session_id = %session_id,
                                "mark setup complete failed after space access completion"
                            );
                            let failed_state = SetupState::JoinSpaceSelectDevice {
                                error: Some(SetupDomainError::PairingFailed),
                            };
                            context.set_state(failed_state.clone()).await;
                            setup_event_port
                                .emit_setup_state_changed(failed_state, Some(session_id.clone()))
                                .await;
                            return;
                        }
                    }
                }

                context.set_state(next.clone()).await;
                setup_event_port
                    .emit_setup_state_changed(next, Some(session_id.clone()))
                    .await;
                break;
            }
        });
    }

    fn map_setup_event_from_space_access_state(
        space_access_state: &SpaceAccessState,
        session_id: &str,
    ) -> Option<SetupEvent> {
        match space_access_state {
            SpaceAccessState::Granted {
                pairing_session_id, ..
            } if pairing_session_id == session_id => Some(SetupEvent::JoinSpaceSucceeded),
            SpaceAccessState::Denied {
                pairing_session_id,
                reason,
                ..
            } if pairing_session_id == session_id => Some(SetupEvent::JoinSpaceFailed {
                error: Self::map_space_access_deny_reason(reason),
            }),
            SpaceAccessState::Cancelled {
                pairing_session_id, ..
            } if pairing_session_id == session_id => Some(SetupEvent::JoinSpaceFailed {
                error: SetupDomainError::PairingFailed,
            }),
            _ => None,
        }
    }

    fn map_space_access_deny_reason(reason: &DenyReason) -> SetupDomainError {
        match reason {
            DenyReason::InvalidProof => SetupDomainError::PassphraseInvalidOrMismatch,
            DenyReason::Expired
            | DenyReason::SpaceMismatch
            | DenyReason::SessionMismatch
            | DenyReason::InternalError => SetupDomainError::PairingFailed,
        }
    }

    pub(super) fn map_pairing_failure_reason(reason: &FailureReason) -> SetupDomainError {
        match reason {
            FailureReason::Other(message) => {
                let message_lower = message.to_ascii_lowercase();
                if message_lower.contains("rejected") {
                    return SetupDomainError::PairingRejected;
                }
                if message_lower.contains("timeout") {
                    return SetupDomainError::NetworkTimeout;
                }
                SetupDomainError::PairingFailed
            }
            _ => SetupDomainError::PairingFailed,
        }
    }

    async fn ensure_pairing_session(
        &self,
        selected_peer_id: &Arc<Mutex<Option<String>>>,
        pairing_session_id: &Arc<Mutex<Option<String>>>,
        joiner_offer: &Arc<Mutex<Option<SpaceAccessJoinerOffer>>>,
        context: &Arc<SetupContext>,
    ) -> Result<(), SetupError> {
        let peer_id = {
            let guard = selected_peer_id.lock().await;
            guard.clone()
        }
        .ok_or_else(|| {
            error!("ensure pairing requested without selected peer");
            SetupError::PairingFailed
        })?;

        // Subscribe to pairing domain events BEFORE initiating the session.
        //
        // `initiate_pairing` may emit `PairingVerificationRequired` or
        // `PairingFailed` synchronously (e.g. on the same device / low latency
        // path).  If we subscribed after the initiation we would miss those
        // first events and the setup state machine would stall forever in
        // `ProcessingJoinSpace`.
        let event_rx = match self.pairing_orchestrator.subscribe().await {
            Ok(rx) => rx,
            Err(err) => {
                error!(
                    error = %err,
                    peer_id = %peer_id,
                    "failed to subscribe pairing events before initiating pairing"
                );
                return Err(SetupError::PairingFailed);
            }
        };

        let session_id = self
            .pairing_orchestrator
            .initiate_pairing(peer_id.clone())
            .await
            .map_err(|err| {
                error!(error = %err, peer_id = %peer_id, "failed to initiate pairing session");
                SetupError::PairingFailed
            })?;

        {
            let mut guard = pairing_session_id.lock().await;
            *guard = Some(session_id.clone());
        }

        self.start_pairing_verification_listener_with_rx(
            session_id,
            event_rx,
            pairing_session_id,
            joiner_offer,
            context,
        )
        .await;
        Ok(())
    }

    async fn confirm_peer_trust_action(
        &self,
        pairing_session_id: &Arc<Mutex<Option<String>>>,
        context: &Arc<SetupContext>,
    ) -> Result<(), SetupError> {
        let session_id = {
            let guard = pairing_session_id.lock().await;
            guard.clone()
        }
        .ok_or_else(|| {
            error!("confirm peer trust requested without active session");
            SetupError::PairingFailed
        })?;

        self.pairing_orchestrator
            .user_accept_pairing(&session_id)
            .await
            .map_err(|err| {
                error!(error = %err, session_id = %session_id, "failed to accept pairing session");
                SetupError::PairingFailed
            })?;

        Self::set_state_and_emit(
            context,
            &self.setup_event_port,
            SetupState::JoinSpaceInputPassphrase { error: None },
            Some(session_id),
        )
        .await;
        Ok(())
    }

    async fn abort_pairing_session(
        &self,
        pairing_session_id: &Arc<Mutex<Option<String>>>,
        selected_peer_id: &Arc<Mutex<Option<String>>>,
    ) {
        let session_id = {
            let mut guard = pairing_session_id.lock().await;
            guard.take()
        };
        if let Some(session_id) = session_id {
            if let Err(err) = self
                .pairing_orchestrator
                .user_reject_pairing(&session_id)
                .await
            {
                warn!(error = %err, session_id = %session_id, "failed to reject pairing session");
            }
        }

        {
            let mut guard = selected_peer_id.lock().await;
            guard.take();
        }
    }

    /// Start listening for pairing domain events using a pre-subscribed receiver.
    ///
    /// The caller must subscribe to pairing events **before** calling
    /// `initiate_pairing` to avoid missing events that are emitted in the
    /// same async cycle as the initiation (low-latency / same-device scenario).
    async fn start_pairing_verification_listener_with_rx(
        &self,
        session_id: String,
        event_rx: tokio::sync::mpsc::Receiver<PairingDomainEvent>,
        pairing_session_id: &Arc<Mutex<Option<String>>>,
        joiner_offer: &Arc<Mutex<Option<SpaceAccessJoinerOffer>>>,
        context: &Arc<SetupContext>,
    ) {
        let mut event_rx = event_rx;
        let context = Arc::clone(context);
        let pairing_session_id = Arc::clone(pairing_session_id);
        let joiner_offer = Arc::clone(joiner_offer);
        let setup_event_port = Arc::clone(&self.setup_event_port);

        tokio::spawn(async move {
            while let Some(event) = event_rx.recv().await {
                if !Self::pairing_session_matches(&pairing_session_id, &session_id).await {
                    break;
                }
                match event {
                    PairingDomainEvent::PairingVerificationRequired {
                        session_id: event_session_id,
                        short_code,
                        peer_fingerprint,
                        ..
                    } if event_session_id == session_id => {
                        let next_state = SetupState::JoinSpaceConfirmPeer {
                            short_code,
                            peer_fingerprint: Some(peer_fingerprint),
                            error: None,
                        };
                        context.set_state(next_state.clone()).await;
                        setup_event_port
                            .emit_setup_state_changed(next_state, Some(session_id.clone()))
                            .await;
                    }
                    PairingDomainEvent::PairingFailed {
                        session_id: event_session_id,
                        reason,
                        ..
                    } if event_session_id == session_id => {
                        let mapped_error = Self::map_pairing_failure_reason(&reason);
                        let next_state = SetupState::JoinSpaceSelectDevice {
                            error: Some(mapped_error),
                        };
                        context.set_state(next_state.clone()).await;
                        setup_event_port
                            .emit_setup_state_changed(next_state, Some(session_id.clone()))
                            .await;
                        break;
                    }
                    PairingDomainEvent::PairingSucceeded {
                        session_id: event_session_id,
                        ..
                    } if event_session_id == session_id => {
                        continue;
                    }
                    PairingDomainEvent::KeyslotReceived {
                        session_id: event_session_id,
                        keyslot_file,
                        challenge,
                        ..
                    } if event_session_id == session_id => {
                        let challenge_len = challenge.len();
                        let challenge_nonce: [u8; 32] = match challenge.try_into() {
                            Ok(nonce) => nonce,
                            Err(_) => {
                                warn!(
                                    session_id = %session_id,
                                    challenge_len,
                                    "received invalid keyslot challenge length"
                                );
                                continue;
                            }
                        };
                        let keyslot: uc_core::security::model::KeySlot =
                            keyslot_file.clone().into();
                        let keyslot_blob = match serde_json::to_vec(&keyslot) {
                            Ok(blob) => blob,
                            Err(err) => {
                                warn!(
                                    error = %err,
                                    session_id = %session_id,
                                    "failed to serialize keyslot for space access"
                                );
                                continue;
                            }
                        };
                        let offer = SpaceAccessJoinerOffer {
                            space_id: uc_core::ids::SpaceId::from(
                                keyslot_file.scope.profile_id.as_str(),
                            ),
                            keyslot_blob,
                            challenge_nonce,
                        };

                        *joiner_offer.lock().await = Some(offer);
                    }
                    _ => {}
                }
            }
        });
    }

    pub(super) async fn set_state_and_emit(
        context: &Arc<SetupContext>,
        setup_event_port: &Arc<dyn SetupEventPort>,
        state: SetupState,
        session_id: Option<String>,
    ) {
        context.set_state(state.clone()).await;
        setup_event_port
            .emit_setup_state_changed(state, session_id)
            .await;
    }

    async fn pairing_session_matches(
        pairing_session: &Arc<Mutex<Option<String>>>,
        target_session: &str,
    ) -> bool {
        let guard = pairing_session.lock().await;
        guard
            .as_ref()
            .map(|current| current == target_session)
            .unwrap_or(false)
    }
}
