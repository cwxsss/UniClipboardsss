use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};
use uc_app::runtime::CoreRuntime;
use uc_app::usecases::pairing::{PairingDomainEvent, PairingEventPort, PairingOrchestrator};
use uc_app::usecases::space_access::SpaceAccessOrchestrator;
use uc_core::network::pairing_state_machine::{CancellationBy, PairingAction, PairingRole};
use uc_core::network::{NetworkEvent, PairingBusy, PairingCancel, PairingMessage, PairingRequest};
use uc_infra::fs::key_slot_store::KeySlotStore;

use crate::pairing::session_projection::{mark_pairing_session_terminal, upsert_pairing_snapshot};
use crate::state::RuntimeState;

const PAIRING_EVENTS_SUBSCRIBE_BACKOFF_INITIAL_MS: u64 = 250;
const PAIRING_EVENTS_SUBSCRIBE_BACKOFF_MAX_MS: u64 = 30_000;
const SESSION_SWEEP_INTERVAL_SECS: u64 = 15;
const DEFAULT_CONTROL_LEASE_TTL_MS: u64 = 30_000;

#[derive(Debug, Clone, Copy)]
struct LeaseRegistration {
    expires_at_ms: i64,
}

#[derive(Debug, Default)]
struct LeaseRegistry {
    active: AtomicBool,
    leases: RwLock<HashMap<String, LeaseRegistration>>,
}

impl LeaseRegistry {
    async fn set(&self, client_kind: String, enabled: bool, lease_ttl_ms: Option<u64>) {
        let mut leases = self.leases.write().await;
        prune_expired_leases(&mut leases);
        if enabled {
            let ttl_ms = lease_ttl_ms.unwrap_or(DEFAULT_CONTROL_LEASE_TTL_MS);
            let expires_at_ms = now_ms().saturating_add(ttl_ms.min(i64::MAX as u64) as i64);
            leases.insert(client_kind, LeaseRegistration { expires_at_ms });
        } else {
            leases.remove(&client_kind);
        }
        self.active.store(!leases.is_empty(), Ordering::SeqCst);
    }

    async fn is_active(&self) -> bool {
        let mut leases = self.leases.write().await;
        prune_expired_leases(&mut leases);
        let active = !leases.is_empty();
        self.active.store(active, Ordering::SeqCst);
        active
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DaemonPairingHostError {
    ActivePairingSessionExists,
    HostNotDiscoverable,
    NoLocalPairingParticipantReady,
    SessionNotFound(String),
    Internal(String),
}

impl std::fmt::Display for DaemonPairingHostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ActivePairingSessionExists => f.write_str("active pairing session exists"),
            Self::HostNotDiscoverable => f.write_str("host not discoverable"),
            Self::NoLocalPairingParticipantReady => {
                f.write_str("no local pairing participant ready")
            }
            Self::SessionNotFound(session_id) => {
                write!(f, "pairing session not found: {session_id}")
            }
            Self::Internal(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for DaemonPairingHostError {}

pub struct DaemonPairingHost {
    runtime: Arc<CoreRuntime>,
    pairing_orchestrator: Arc<PairingOrchestrator>,
    pairing_action_rx: Mutex<Option<mpsc::Receiver<PairingAction>>>,
    state: Arc<RwLock<RuntimeState>>,
    space_access_orchestrator: Arc<SpaceAccessOrchestrator>,
    key_slot_store: Arc<dyn KeySlotStore>,
    discoverability: Arc<LeaseRegistry>,
    participant_readiness: Arc<LeaseRegistry>,
    active_session_id: Arc<RwLock<Option<String>>>,
}

impl DaemonPairingHost {
    pub fn new(
        runtime: Arc<CoreRuntime>,
        pairing_orchestrator: Arc<PairingOrchestrator>,
        pairing_action_rx: mpsc::Receiver<PairingAction>,
        state: Arc<RwLock<RuntimeState>>,
        space_access_orchestrator: Arc<SpaceAccessOrchestrator>,
        key_slot_store: Arc<dyn KeySlotStore>,
    ) -> Self {
        Self {
            runtime,
            pairing_orchestrator,
            pairing_action_rx: Mutex::new(Some(pairing_action_rx)),
            state,
            space_access_orchestrator,
            key_slot_store,
            discoverability: Arc::new(LeaseRegistry::default()),
            participant_readiness: Arc::new(LeaseRegistry::default()),
            active_session_id: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn discoverable(&self) -> bool {
        self.discoverability.is_active().await
    }

    pub async fn participant_ready(&self) -> bool {
        self.participant_readiness.is_active().await
    }

    pub async fn set_discoverability(
        &self,
        client_kind: String,
        discoverable: bool,
        lease_ttl_ms: Option<u64>,
    ) {
        self.discoverability
            .set(client_kind, discoverable, lease_ttl_ms)
            .await;
    }

    pub async fn set_participant_ready(
        &self,
        client_kind: String,
        ready: bool,
        lease_ttl_ms: Option<u64>,
    ) {
        self.participant_readiness
            .set(client_kind, ready, lease_ttl_ms)
            .await;
    }

    pub async fn active_session_id(&self) -> Option<String> {
        self.active_session_id.read().await.clone()
    }

    pub async fn initiate_pairing(
        &self,
        peer_id: String,
    ) -> Result<String, DaemonPairingHostError> {
        if !self.discoverability.is_active().await {
            return Err(DaemonPairingHostError::HostNotDiscoverable);
        }
        if !self.participant_readiness.is_active().await {
            return Err(DaemonPairingHostError::NoLocalPairingParticipantReady);
        }
        self.reserve_session_slot(None).await?;

        match self
            .pairing_orchestrator
            .initiate_pairing(peer_id.clone())
            .await
            .map_err(|err| DaemonPairingHostError::Internal(err.to_string()))
        {
            Ok(session_id) => {
                self.bind_active_session(session_id.clone()).await;
                upsert_pairing_snapshot(
                    &self.state,
                    session_id.clone(),
                    Some(peer_id),
                    None,
                    "request",
                    now_ms(),
                )
                .await;
                Ok(session_id)
            }
            Err(err) => {
                self.clear_active_session(None).await;
                Err(err)
            }
        }
    }

    pub async fn accept_pairing(&self, session_id: &str) -> Result<(), DaemonPairingHostError> {
        self.require_session(session_id).await?;
        self.pairing_orchestrator
            .user_accept_pairing(session_id)
            .await
            .map_err(|err| DaemonPairingHostError::Internal(err.to_string()))?;
        upsert_pairing_snapshot(
            &self.state,
            session_id.to_string(),
            self.session_peer_id(session_id).await,
            self.session_device_name(session_id).await,
            "verifying",
            now_ms(),
        )
        .await;
        Ok(())
    }

    pub async fn reject_pairing(&self, session_id: &str) -> Result<(), DaemonPairingHostError> {
        self.require_session(session_id).await?;
        self.pairing_orchestrator
            .user_reject_pairing(session_id)
            .await
            .map_err(|err| DaemonPairingHostError::Internal(err.to_string()))?;
        mark_pairing_session_terminal(
            &self.state,
            session_id.to_string(),
            self.session_peer_id(session_id).await,
            self.session_device_name(session_id).await,
            "failed",
            now_ms(),
        )
        .await;
        self.clear_active_session(Some(session_id)).await;
        Ok(())
    }

    pub async fn cancel_pairing(&self, session_id: &str) -> Result<(), DaemonPairingHostError> {
        let peer_id = self
            .session_peer_id(session_id)
            .await
            .ok_or_else(|| DaemonPairingHostError::SessionNotFound(session_id.to_string()))?;
        let pairing_transport = self.runtime.wiring_deps().network_ports.pairing.clone();

        pairing_transport
            .open_pairing_session(peer_id.clone(), session_id.to_string())
            .await
            .map_err(|err| DaemonPairingHostError::Internal(err.to_string()))?;
        pairing_transport
            .send_pairing_on_session(PairingMessage::Cancel(PairingCancel {
                session_id: session_id.to_string(),
                reason: None,
            }))
            .await
            .map_err(|err| DaemonPairingHostError::Internal(err.to_string()))?;

        mark_pairing_session_terminal(
            &self.state,
            session_id.to_string(),
            Some(peer_id),
            self.session_device_name(session_id).await,
            cancellation_state_label(CancellationBy::LocalUser),
            now_ms(),
        )
        .await;
        self.clear_active_session(Some(session_id)).await;
        Ok(())
    }

    pub async fn verify_pairing(
        &self,
        session_id: &str,
        pin_matches: bool,
    ) -> Result<(), DaemonPairingHostError> {
        if pin_matches {
            self.accept_pairing(session_id).await
        } else {
            self.reject_pairing(session_id).await
        }
    }

    pub async fn handle_incoming_request(
        &self,
        peer_id: String,
        request: PairingRequest,
    ) -> Result<(), DaemonPairingHostError> {
        let session_id = request.session_id.clone();
        self.ensure_inbound_admitted(&peer_id, &session_id).await?;
        self.bind_active_session(session_id.clone()).await;

        upsert_pairing_snapshot(
            &self.state,
            session_id.clone(),
            Some(peer_id.clone()),
            Some(request.device_name.clone()),
            "request",
            now_ms(),
        )
        .await;

        match self
            .pairing_orchestrator
            .handle_incoming_request(peer_id, request)
            .await
        {
            Ok(()) => Ok(()),
            Err(err) => {
                self.clear_active_session(Some(&session_id)).await;
                Err(DaemonPairingHostError::Internal(err.to_string()))
            }
        }
    }

    pub async fn run(self: Arc<Self>, cancel: CancellationToken) -> anyhow::Result<()> {
        let pairing_action_rx = self
            .pairing_action_rx
            .lock()
            .await
            .take()
            .context("daemon pairing host already running")?;
        let domain_events = PairingEventPort::subscribe(self.pairing_orchestrator.as_ref())
            .await
            .context("failed to subscribe to pairing domain events")?;

        let mut tasks = JoinSet::new();

        tasks.spawn(run_pairing_action_loop(
            self.runtime.clone(),
            self.pairing_orchestrator.clone(),
            self.space_access_orchestrator.clone(),
            self.key_slot_store.clone(),
            self.state.clone(),
            self.active_session_id.clone(),
            pairing_action_rx,
            cancel.child_token(),
        ));

        tasks.spawn(run_pairing_domain_event_loop(
            self.pairing_orchestrator.clone(),
            self.state.clone(),
            self.active_session_id.clone(),
            domain_events,
            cancel.child_token(),
        ));

        tasks.spawn(run_pairing_network_event_loop(
            self.runtime.clone(),
            self.pairing_orchestrator.clone(),
            self.state.clone(),
            self.active_session_id.clone(),
            self.discoverability.clone(),
            self.participant_readiness.clone(),
            cancel.child_token(),
        ));

        tasks.spawn(run_pairing_session_sweep_loop(
            self.pairing_orchestrator.clone(),
            self.discoverability.clone(),
            self.participant_readiness.clone(),
            cancel.child_token(),
        ));

        tokio::select! {
            _ = cancel.cancelled() => {
                info!("daemon pairing host received shutdown");
            }
            Some(result) = tasks.join_next() => {
                match result {
                    Ok(Ok(())) => {
                        warn!("daemon pairing host subtask exited unexpectedly");
                    }
                    Ok(Err(err)) => return Err(err),
                    Err(err) => return Err(anyhow::anyhow!("daemon pairing host task join failed: {}", err)),
                }
            }
        }

        while tasks.join_next().await.is_some() {}
        Ok(())
    }

    async fn reserve_session_slot(
        &self,
        session_id: Option<&str>,
    ) -> Result<(), DaemonPairingHostError> {
        let mut guard = self.active_session_id.write().await;
        if let Some(active) = guard.as_ref() {
            if session_id.is_none() || Some(active.as_str()) != session_id {
                return Err(DaemonPairingHostError::ActivePairingSessionExists);
            }
        }

        if let Some(session_id) = session_id {
            *guard = Some(session_id.to_string());
        }
        Ok(())
    }

    async fn bind_active_session(&self, session_id: String) {
        *self.active_session_id.write().await = Some(session_id);
    }

    async fn clear_active_session(&self, session_id: Option<&str>) {
        let mut guard = self.active_session_id.write().await;
        let should_clear = match (guard.as_ref(), session_id) {
            (_, None) => true,
            (Some(active), Some(expected)) => active == expected,
            (None, Some(_)) => false,
        };

        if should_clear {
            *guard = None;
        }
    }

    async fn ensure_inbound_admitted(
        &self,
        peer_id: &str,
        session_id: &str,
    ) -> Result<(), DaemonPairingHostError> {
        if !self.discoverability.is_active().await {
            self.reject_inbound_request(peer_id, session_id, "host_not_discoverable")
                .await;
            return Err(DaemonPairingHostError::HostNotDiscoverable);
        }
        if !self.participant_readiness.is_active().await {
            self.reject_inbound_request(peer_id, session_id, "no_local_pairing_participant_ready")
                .await;
            return Err(DaemonPairingHostError::NoLocalPairingParticipantReady);
        }

        self.reserve_session_slot(Some(session_id)).await
    }

    async fn reject_inbound_request(&self, peer_id: &str, session_id: &str, reason: &str) {
        let pairing = self.runtime.wiring_deps().network_ports.pairing.clone();
        if let Err(err) = pairing
            .open_pairing_session(peer_id.to_string(), session_id.to_string())
            .await
        {
            debug!(
                error = %err,
                peer_id = %peer_id,
                session_id = %session_id,
                "failed to open busy response session"
            );
            return;
        }

        if let Err(err) = pairing
            .send_pairing_on_session(PairingMessage::Busy(PairingBusy {
                session_id: session_id.to_string(),
                reason: Some(reason.to_string()),
            }))
            .await
        {
            debug!(
                error = %err,
                peer_id = %peer_id,
                session_id = %session_id,
                "failed to send busy pairing response"
            );
        }
    }

    async fn require_session(&self, session_id: &str) -> Result<(), DaemonPairingHostError> {
        if self
            .state
            .read()
            .await
            .pairing_session(session_id)
            .is_none()
        {
            return Err(DaemonPairingHostError::SessionNotFound(
                session_id.to_string(),
            ));
        }
        Ok(())
    }

    async fn session_peer_id(&self, session_id: &str) -> Option<String> {
        if let Some(peer) = self.pairing_orchestrator.get_session_peer(session_id).await {
            return Some(peer.peer_id);
        }
        self.state
            .read()
            .await
            .pairing_session(session_id)
            .and_then(|snapshot| snapshot.peer_id.clone())
    }

    async fn session_device_name(&self, session_id: &str) -> Option<String> {
        if let Some(peer) = self.pairing_orchestrator.get_session_peer(session_id).await {
            return peer.device_name;
        }
        self.state
            .read()
            .await
            .pairing_session(session_id)
            .and_then(|snapshot| snapshot.device_name.clone())
    }
}

async fn run_pairing_action_loop(
    runtime: Arc<CoreRuntime>,
    pairing_orchestrator: Arc<PairingOrchestrator>,
    space_access_orchestrator: Arc<SpaceAccessOrchestrator>,
    key_slot_store: Arc<dyn KeySlotStore>,
    state: Arc<RwLock<RuntimeState>>,
    active_session_id: Arc<RwLock<Option<String>>>,
    mut action_rx: mpsc::Receiver<PairingAction>,
    cancel: CancellationToken,
) -> anyhow::Result<()> {
    let pairing_transport = runtime.wiring_deps().network_ports.pairing.clone();

    loop {
        tokio::select! {
            _ = cancel.cancelled() => return Ok(()),
            maybe_action = action_rx.recv() => {
                let Some(action) = maybe_action else {
                    return Ok(());
                };

                match action {
                    PairingAction::Send { peer_id, message } => {
                        let session_id = message.session_id().to_string();
                        if let Err(err) = pairing_transport
                            .open_pairing_session(peer_id.clone(), session_id.clone())
                            .await
                        {
                            signal_pairing_transport_failure(
                                pairing_orchestrator.as_ref(),
                                &state,
                                &active_session_id,
                                &session_id,
                                &peer_id,
                                err.to_string(),
                            )
                            .await?;
                            continue;
                        }

                        if let Err(err) = pairing_transport.send_pairing_on_session(message).await {
                            signal_pairing_transport_failure(
                                pairing_orchestrator.as_ref(),
                                &state,
                                &active_session_id,
                                &session_id,
                                &peer_id,
                                err.to_string(),
                            )
                            .await?;
                        }
                    }
                    PairingAction::ShowVerification {
                        session_id,
                        short_code: _,
                        local_fingerprint: _,
                        peer_fingerprint: _,
                        peer_display_name,
                    } => {
                        upsert_pairing_snapshot(
                            &state,
                            session_id,
                            None,
                            Some(peer_display_name),
                            "verification",
                            now_ms(),
                        )
                        .await;
                    }
                    PairingAction::ShowVerifying {
                        session_id,
                        peer_display_name,
                    } => {
                        upsert_pairing_snapshot(
                            &state,
                            session_id,
                            None,
                            Some(peer_display_name),
                            "verifying",
                            now_ms(),
                        )
                        .await;
                    }
                    PairingAction::EmitResult {
                        session_id,
                        success,
                        error,
                    } => {
                        let peer_info = pairing_orchestrator.get_session_peer(&session_id).await;
                        let role = pairing_orchestrator.get_session_role(&session_id).await;

                        if !success {
                            if let Err(err) = pairing_transport
                                .close_pairing_session(session_id.clone(), error.clone())
                                .await
                            {
                                warn!(error = %err, session_id = %session_id, "failed to close pairing session");
                            }
                        } else if role == Some(PairingRole::Responder) {
                            if let Some(peer) = peer_info.as_ref() {
                                let context = space_access_orchestrator.context();
                                context.lock().await.sponsor_peer_id = Some(peer.peer_id.clone());
                            }

                            if let Err(err) = key_slot_store.load().await {
                                debug!(
                                    error = %err,
                                    session_id = %session_id,
                                    "key slot store unavailable for responder-side follow-up"
                                );
                            }
                        }

                        let (peer_id, device_name) = match peer_info {
                            Some(peer) => (Some(peer.peer_id), peer.device_name),
                            None => (None, None),
                        };

                        let state_label = if success { "complete" } else { "failed" };
                        mark_pairing_session_terminal(
                            &state,
                            session_id.clone(),
                            peer_id,
                            device_name,
                            state_label,
                            now_ms(),
                        )
                        .await;
                        clear_active_session_slot(&active_session_id, &session_id).await;
                    }
                    other => {
                        debug!(action = ?other, "daemon pairing host ignored unsupported action");
                    }
                }
            }
        }
    }
}

async fn run_pairing_domain_event_loop(
    pairing_orchestrator: Arc<PairingOrchestrator>,
    state: Arc<RwLock<RuntimeState>>,
    active_session_id: Arc<RwLock<Option<String>>>,
    mut domain_events: mpsc::Receiver<PairingDomainEvent>,
    cancel: CancellationToken,
) -> anyhow::Result<()> {
    loop {
        tokio::select! {
            _ = cancel.cancelled() => return Ok(()),
            maybe_event = domain_events.recv() => {
                let Some(event) = maybe_event else {
                    return Ok(());
                };

                match event {
                    PairingDomainEvent::PairingVerificationRequired {
                        session_id,
                        peer_id,
                        short_code: _,
                        local_fingerprint: _,
                        peer_fingerprint: _,
                    } => {
                        let device_name = pairing_orchestrator
                            .get_session_peer(&session_id)
                            .await
                            .and_then(|peer| peer.device_name);
                        upsert_pairing_snapshot(
                            &state,
                            session_id,
                            Some(peer_id),
                            device_name,
                            "verification",
                            now_ms(),
                        )
                        .await;
                    }
                    PairingDomainEvent::KeyslotReceived {
                        session_id,
                        peer_id,
                        keyslot_file: _,
                        challenge: _,
                    } => {
                        upsert_pairing_snapshot(
                            &state,
                            session_id,
                            Some(peer_id),
                            None,
                            "verifying",
                            now_ms(),
                        )
                        .await;
                    }
                    PairingDomainEvent::PairingSucceeded { session_id, peer_id } => {
                        let device_name = pairing_orchestrator
                            .get_session_peer(&session_id)
                            .await
                            .and_then(|peer| peer.device_name);
                        mark_pairing_session_terminal(
                            &state,
                            session_id.clone(),
                            Some(peer_id),
                            device_name,
                            "complete",
                            now_ms(),
                        )
                        .await;
                        clear_active_session_slot(&active_session_id, &session_id).await;
                    }
                    PairingDomainEvent::PairingFailed {
                        session_id,
                        peer_id,
                        reason: _,
                    } => {
                        let device_name = pairing_orchestrator
                            .get_session_peer(&session_id)
                            .await
                            .and_then(|peer| peer.device_name);
                        mark_pairing_session_terminal(
                            &state,
                            session_id.clone(),
                            Some(peer_id),
                            device_name,
                            "failed",
                            now_ms(),
                        )
                        .await;
                        clear_active_session_slot(&active_session_id, &session_id).await;
                    }
                }
            }
        }
    }
}

async fn run_pairing_network_event_loop(
    runtime: Arc<CoreRuntime>,
    pairing_orchestrator: Arc<PairingOrchestrator>,
    state: Arc<RwLock<RuntimeState>>,
    active_session_id: Arc<RwLock<Option<String>>>,
    discoverability: Arc<LeaseRegistry>,
    participant_readiness: Arc<LeaseRegistry>,
    cancel: CancellationToken,
) -> anyhow::Result<()> {
    let network_events = runtime.wiring_deps().network_ports.events.clone();
    let pairing_transport = runtime.wiring_deps().network_ports.pairing.clone();

    let mut subscribe_attempt: u32 = 0;
    loop {
        let subscribe_result = tokio::select! {
            _ = cancel.cancelled() => return Ok(()),
            result = network_events.subscribe_events() => result,
        };

        match subscribe_result {
            Ok(mut event_rx) => {
                subscribe_attempt = 0;
                loop {
                    tokio::select! {
                        _ = cancel.cancelled() => return Ok(()),
                        maybe_event = event_rx.recv() => {
                            let Some(event) = maybe_event else {
                                break;
                            };

                            match event {
                                NetworkEvent::PairingMessageReceived { peer_id, message } => {
                                    handle_pairing_message(
                                        pairing_orchestrator.as_ref(),
                                        &state,
                                        &active_session_id,
                                        &pairing_transport,
                                        &discoverability,
                                        &participant_readiness,
                                        peer_id,
                                        message,
                                    )
                                    .await?;
                                }
                                NetworkEvent::PairingFailed { session_id, peer_id, error } => {
                                    signal_pairing_transport_failure(
                                        pairing_orchestrator.as_ref(),
                                        &state,
                                        &active_session_id,
                                        &session_id,
                                        &peer_id,
                                        error,
                                    )
                                    .await?;
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
            Err(err) => {
                subscribe_attempt = subscribe_attempt.saturating_add(1);
                let retry_in_ms = pairing_events_subscribe_backoff_ms(subscribe_attempt);
                warn!(
                    error = %err,
                    attempt = subscribe_attempt,
                    retry_in_ms,
                    "failed to subscribe to daemon pairing network events"
                );
            }
        }

        let backoff = Duration::from_millis(pairing_events_subscribe_backoff_ms(subscribe_attempt));
        tokio::select! {
            _ = cancel.cancelled() => return Ok(()),
            _ = tokio::time::sleep(backoff) => {}
        }
    }
}

async fn run_pairing_session_sweep_loop(
    pairing_orchestrator: Arc<PairingOrchestrator>,
    discoverability: Arc<LeaseRegistry>,
    participant_readiness: Arc<LeaseRegistry>,
    cancel: CancellationToken,
) -> anyhow::Result<()> {
    let mut interval = tokio::time::interval(Duration::from_secs(SESSION_SWEEP_INTERVAL_SECS));

    loop {
        tokio::select! {
            _ = cancel.cancelled() => return Ok(()),
            _ = interval.tick() => {
                pairing_orchestrator.cleanup_expired_sessions().await;
                let _ = discoverability.is_active().await;
                let _ = participant_readiness.is_active().await;
            }
        }
    }
}

async fn handle_pairing_message(
    pairing_orchestrator: &PairingOrchestrator,
    state: &Arc<RwLock<RuntimeState>>,
    active_session_id: &Arc<RwLock<Option<String>>>,
    pairing_transport: &Arc<dyn uc_core::ports::PairingTransportPort>,
    discoverability: &Arc<LeaseRegistry>,
    participant_readiness: &Arc<LeaseRegistry>,
    peer_id: String,
    message: PairingMessage,
) -> anyhow::Result<()> {
    match message {
        PairingMessage::Request(request) => {
            if !discoverability.is_active().await {
                reject_inbound_request(
                    pairing_transport,
                    &peer_id,
                    &request.session_id,
                    "host_not_discoverable",
                )
                .await;
                return Ok(());
            }

            if !participant_readiness.is_active().await {
                reject_inbound_request(
                    pairing_transport,
                    &peer_id,
                    &request.session_id,
                    "no_local_pairing_participant_ready",
                )
                .await;
                return Ok(());
            }

            {
                let mut guard = active_session_id.write().await;
                if let Some(active) = guard.as_ref() {
                    if active != &request.session_id {
                        reject_inbound_request(
                            pairing_transport,
                            &peer_id,
                            &request.session_id,
                            "busy",
                        )
                        .await;
                        return Ok(());
                    }
                } else {
                    *guard = Some(request.session_id.clone());
                }
            }

            upsert_pairing_snapshot(
                state,
                request.session_id.clone(),
                Some(peer_id.clone()),
                Some(request.device_name.clone()),
                "request",
                now_ms(),
            )
            .await;

            pairing_orchestrator
                .handle_incoming_request(peer_id, request)
                .await?;
        }
        PairingMessage::Challenge(challenge) => {
            let session_id = challenge.session_id.clone();
            pairing_orchestrator
                .handle_challenge(&session_id, &peer_id, challenge)
                .await?;
        }
        PairingMessage::KeyslotOffer(offer) => {
            let session_id = offer.session_id.clone();
            pairing_orchestrator
                .handle_keyslot_offer(&session_id, &peer_id, offer)
                .await?;
        }
        PairingMessage::ChallengeResponse(response) => {
            let session_id = response.session_id.clone();
            pairing_orchestrator
                .handle_challenge_response(&session_id, &peer_id, response)
                .await?;
        }
        PairingMessage::Response(response) => {
            let session_id = response.session_id.clone();
            pairing_orchestrator
                .handle_response(&session_id, &peer_id, response)
                .await?;
        }
        PairingMessage::Confirm(confirm) => {
            let session_id = confirm.session_id.clone();
            pairing_orchestrator
                .handle_confirm(&session_id, &peer_id, confirm)
                .await?;
        }
        PairingMessage::Reject(reject) => {
            let session_id = reject.session_id.clone();
            pairing_orchestrator
                .handle_reject(&session_id, &peer_id)
                .await?;
        }
        PairingMessage::Cancel(cancel) => {
            let session_id = cancel.session_id.clone();
            pairing_orchestrator
                .handle_cancel(&session_id, &peer_id)
                .await?;
        }
        PairingMessage::Busy(busy) => {
            let session_id = busy.session_id.clone();
            pairing_orchestrator
                .handle_busy(&session_id, &peer_id)
                .await?;
        }
    }

    Ok(())
}

async fn reject_inbound_request(
    pairing_transport: &Arc<dyn uc_core::ports::PairingTransportPort>,
    peer_id: &str,
    session_id: &str,
    reason: &str,
) {
    if let Err(err) = pairing_transport
        .open_pairing_session(peer_id.to_string(), session_id.to_string())
        .await
    {
        debug!(error = %err, peer_id = %peer_id, session_id = %session_id, "failed to open busy pairing session");
        return;
    }

    if let Err(err) = pairing_transport
        .send_pairing_on_session(PairingMessage::Busy(PairingBusy {
            session_id: session_id.to_string(),
            reason: Some(reason.to_string()),
        }))
        .await
    {
        debug!(error = %err, peer_id = %peer_id, session_id = %session_id, "failed to send busy pairing message");
    }
}

async fn signal_pairing_transport_failure(
    pairing_orchestrator: &PairingOrchestrator,
    state: &Arc<RwLock<RuntimeState>>,
    active_session_id: &Arc<RwLock<Option<String>>>,
    session_id: &str,
    peer_id: &str,
    reason: String,
) -> anyhow::Result<()> {
    mark_pairing_session_terminal(
        state,
        session_id.to_string(),
        Some(peer_id.to_string()),
        None,
        "failed",
        now_ms(),
    )
    .await;
    clear_active_session_slot(active_session_id, session_id).await;
    pairing_orchestrator
        .handle_transport_error(session_id, peer_id, reason)
        .await?;
    Ok(())
}

async fn clear_active_session_slot(
    active_session_id: &Arc<RwLock<Option<String>>>,
    session_id: &str,
) {
    let mut guard = active_session_id.write().await;
    if guard.as_deref() == Some(session_id) {
        *guard = None;
    }
}

fn prune_expired_leases(leases: &mut HashMap<String, LeaseRegistration>) {
    let now = now_ms();
    leases.retain(|_, lease| lease.expires_at_ms > now);
}

fn cancellation_state_label(cancelled_by: CancellationBy) -> &'static str {
    match cancelled_by {
        CancellationBy::LocalUser | CancellationBy::RemoteUser | CancellationBy::System => "failed",
    }
}

fn pairing_events_subscribe_backoff_ms(attempt: u32) -> u64 {
    let exponent = attempt.saturating_sub(1).min(16);
    let factor = 1u64 << exponent;
    PAIRING_EVENTS_SUBSCRIBE_BACKOFF_INITIAL_MS
        .saturating_mul(factor)
        .min(PAIRING_EVENTS_SUBSCRIBE_BACKOFF_MAX_MS)
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
