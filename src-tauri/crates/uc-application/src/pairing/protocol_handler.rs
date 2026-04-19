//! Pairing protocol action handler
//!
//! Executes pairing actions (Send, ShowVerification, PersistPairedDevice, timers, etc.)
//! produced by the state machine. Separated from session lifecycle management.

use anyhow::{Context, Result};
use chrono::Utc;
use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, RwLock};
use tracing::{info_span, Instrument};

use uc_core::network::SessionId;
use uc_core::{DeviceId, TrustedPeerRepositoryPort};

use super::state_machine::{PairingAction, PairingEvent, TimeoutKind};

use super::events::PairingDomainEvent;
use super::session_manager::{PairingPeerInfo, PairingSessionContext};
use crate::trusted_peer::{TrustPeerOrchestrator, TrustVerificationChallenge};

/// Shared alias for the trust-peer orchestrator singleton (D19): one
/// instance per process, dyn-backed so bootstrap can inject the concrete
/// `DieselTrustedPeerRepository` without threading the generic up through
/// `PairingOrchestrator::new`.
pub(crate) type SharedTrustPeerOrchestrator =
    Arc<TrustPeerOrchestrator<dyn TrustedPeerRepositoryPort>>;

/// Handles execution of pairing protocol actions.
///
/// Owns port references needed for protocol operations: trust_peer_orch
/// and action_tx channel. Does NOT own sessions — borrows them via Arc
/// references passed from the orchestrator.
#[derive(Clone)]
pub(crate) struct PairingProtocolHandler {
    /// Action sender (forwarding actions to the network layer)
    action_tx: mpsc::Sender<PairingAction>,
    /// Trust-peer orchestrator singleton (D19). Drives the trust
    /// establishment flow (`initiate → record_session_opened →
    /// confirm_verification`) at the moment the pairing state machine
    /// reaches `PersistPairedDevice`. Replaces the legacy
    /// `PairedDeviceRepositoryPort::upsert` write (retired in phase 4b PR-5)
    /// and the `MemberRepositoryPort::save` dual-write that lived here
    /// during Phase 2; membership is now persisted at the space-access
    /// boundary via `AdmitMemberUseCase`.
    trust_peer_orch: SharedTrustPeerOrchestrator,
    /// Event senders for domain events
    event_senders: Arc<Mutex<Vec<mpsc::Sender<PairingDomainEvent>>>>,
}

impl PairingProtocolHandler {
    /// Create a new protocol handler.
    pub(crate) fn new(
        action_tx: mpsc::Sender<PairingAction>,
        trust_peer_orch: SharedTrustPeerOrchestrator,
        event_senders: Arc<Mutex<Vec<mpsc::Sender<PairingDomainEvent>>>>,
    ) -> Self {
        Self {
            action_tx,
            trust_peer_orch,
            event_senders,
        }
    }

    /// Get a reference to the event senders.
    pub(crate) fn event_senders(&self) -> &Arc<Mutex<Vec<mpsc::Sender<PairingDomainEvent>>>> {
        &self.event_senders
    }

    /// Execute a single action, using the provided session/peer maps.
    pub(crate) async fn execute_action(
        &self,
        session_id: &str,
        _peer_id: &str,
        action: PairingAction,
        sessions: &Arc<RwLock<HashMap<SessionId, PairingSessionContext>>>,
        session_peers: &Arc<RwLock<HashMap<SessionId, PairingPeerInfo>>>,
    ) -> Result<()> {
        Self::execute_action_inner(
            self.action_tx.clone(),
            sessions.clone(),
            session_peers.clone(),
            self.event_senders.clone(),
            self.trust_peer_orch.clone(),
            session_id.to_string(),
            action,
        )
        .await
    }

    fn execute_action_inner(
        action_tx: mpsc::Sender<PairingAction>,
        sessions: Arc<RwLock<HashMap<SessionId, PairingSessionContext>>>,
        session_peers: Arc<RwLock<HashMap<SessionId, PairingPeerInfo>>>,
        event_senders: Arc<Mutex<Vec<mpsc::Sender<PairingDomainEvent>>>>,
        trust_peer_orch: SharedTrustPeerOrchestrator,
        session_id: String,
        action: PairingAction,
    ) -> impl Future<Output = Result<()>> + Send {
        async move {
            let mut queue = VecDeque::from([action]);

            while let Some(action) = queue.pop_front() {
                match action {
                    PairingAction::Send {
                        peer_id: target_peer,
                        message,
                    } => {
                        action_tx
                            .send(PairingAction::Send {
                                peer_id: target_peer,
                                message,
                            })
                            .await
                            .context("Failed to queue send action")?;
                    }
                    PairingAction::ShowVerification {
                        session_id: action_session_id,
                        short_code,
                        local_fingerprint,
                        peer_fingerprint,
                        peer_display_name,
                    } => {
                        let short_code_clone = short_code.clone();
                        let local_fingerprint_clone = local_fingerprint.clone();
                        let peer_fingerprint_clone = peer_fingerprint.clone();
                        let peer_id_for_event = {
                            let peers = session_peers.read().await;
                            peers
                                .get(&action_session_id)
                                .map(|info| info.peer_id.clone())
                        };
                        if let Some(peer_id) = peer_id_for_event {
                            tracing::info!(
                                session_id = %action_session_id,
                                peer_id = %peer_id,
                                has_short_code = !short_code_clone.is_empty(),
                                has_local_fingerprint = !local_fingerprint_clone.is_empty(),
                                has_peer_fingerprint = !peer_fingerprint_clone.is_empty(),
                                "Emitting pairing verification domain event"
                            );
                            Self::emit_event_to_senders(
                                event_senders.clone(),
                                PairingDomainEvent::PairingVerificationRequired {
                                    session_id: action_session_id.clone(),
                                    peer_id,
                                    short_code: short_code_clone,
                                    local_fingerprint: local_fingerprint_clone,
                                    peer_fingerprint: peer_fingerprint_clone,
                                },
                            )
                            .await;
                        } else {
                            tracing::warn!(
                                session_id = %action_session_id,
                                "Pairing verification event missing peer info; domain event not emitted"
                            );
                        }
                        tracing::debug!(
                            session_id = %action_session_id,
                            action = "ShowVerification",
                            "Sending UI action to frontend"
                        );
                        action_tx
                            .send(PairingAction::ShowVerification {
                                session_id: action_session_id,
                                short_code,
                                local_fingerprint,
                                peer_fingerprint,
                                peer_display_name,
                            })
                            .await
                            .context("Failed to queue ui action")?;
                    }
                    PairingAction::ShowVerifying {
                        session_id: verifying_session_id,
                        peer_display_name,
                    } => {
                        let peer_id_for_event = {
                            let peers = session_peers.read().await;
                            peers
                                .get(&verifying_session_id)
                                .map(|info| info.peer_id.clone())
                        };
                        if let Some(peer_id) = peer_id_for_event {
                            tracing::info!(
                                session_id = %verifying_session_id,
                                peer_id = %peer_id,
                                "Emitting pairing verifying domain event"
                            );
                            Self::emit_event_to_senders(
                                event_senders.clone(),
                                PairingDomainEvent::PairingVerifying {
                                    session_id: verifying_session_id.clone(),
                                    peer_id,
                                },
                            )
                            .await;
                        }
                        tracing::debug!(
                            session_id = %verifying_session_id,
                            action = "ShowVerifying",
                            "Sending UI action to frontend"
                        );
                        action_tx
                            .send(PairingAction::ShowVerifying {
                                session_id: verifying_session_id,
                                peer_display_name,
                            })
                            .await
                            .context("Failed to queue ui action")?;
                    }
                    PairingAction::EmitResult {
                        session_id: result_session_id,
                        success,
                        error,
                        abort_reason,
                    } => {
                        let result_session_id_for_send = result_session_id.clone();
                        let error_for_send = error.clone();
                        let abort_reason_for_send = abort_reason.clone();
                        tracing::info!(
                            session_id = %result_session_id,
                            success = %success,
                            error = ?error,
                            abort_reason = ?abort_reason,
                            "Emitting pairing result to frontend"
                        );
                        action_tx
                            .send(PairingAction::EmitResult {
                                session_id: result_session_id_for_send,
                                success,
                                error: error_for_send,
                                abort_reason: abort_reason_for_send,
                            })
                            .await
                            .context("Failed to queue emit result action")?;
                        let peer_id = {
                            let peers = session_peers.read().await;
                            peers
                                .get(&result_session_id)
                                .map(|peer| peer.peer_id.clone())
                                .unwrap_or_default()
                        };
                        if peer_id.is_empty() {
                            tracing::warn!(
                                session_id = %result_session_id,
                                "Pairing result emitted without peer id"
                            );
                        }
                        let event = if success {
                            PairingDomainEvent::PairingSucceeded {
                                session_id: result_session_id.clone(),
                                peer_id,
                            }
                        } else {
                            // Fallback to `ProtocolError` when the state
                            // machine did not attach an abort-reason —
                            // shouldn't happen with the updated emission
                            // sites but keeps the event total.
                            let reason =
                                abort_reason.unwrap_or(uc_core::TrustAbortReason::ProtocolError);
                            PairingDomainEvent::PairingFailed {
                                session_id: result_session_id.clone(),
                                peer_id,
                                reason,
                            }
                        };
                        Self::emit_event_to_senders(event_senders.clone(), event).await;
                    }
                    PairingAction::PersistPairedDevice {
                        session_id: _,
                        outcome,
                    } => {
                        tracing::info!(
                            session_id = %session_id,
                            peer_id = %outcome.peer_id,
                            "Driving trust-peer flow before verification completion"
                        );
                        let peer_id_str = outcome.peer_id.to_string();
                        let peer_device_id = DeviceId::new(peer_id_str.clone());
                        // Short-code is a UI-presentation artefact emitted
                        // earlier in the flow via
                        // `PairingDomainEvent::PairingVerificationRequired`;
                        // the orchestrator state here is not observed
                        // externally in 0.4.2 (D23 defers event publishing
                        // to phase A), so the challenge only needs the
                        // canonical fingerprint for persistence.
                        let challenge = TrustVerificationChallenge {
                            peer_fingerprint: outcome.identity_fingerprint.clone(),
                            short_code: String::new(),
                        };

                        // D19 singleton: reset guarantees a fresh flow
                        // regardless of the previous flow's terminal state.
                        trust_peer_orch.reset().await;
                        let persist_result: Result<()> = async {
                            trust_peer_orch
                                .initiate(peer_device_id.clone())
                                .await
                                .map_err(|err| anyhow::anyhow!("trust initiate failed: {err}"))?;
                            trust_peer_orch
                                .record_session_opened(peer_device_id.clone(), challenge)
                                .await
                                .map_err(|err| {
                                    anyhow::anyhow!("trust record_session_opened failed: {err}")
                                })?;
                            trust_peer_orch
                                .confirm_verification()
                                .await
                                .map_err(|err| {
                                    anyhow::anyhow!("trust confirm_verification failed: {err}")
                                })?;
                            Ok(())
                        }
                        .await;

                        let actions = {
                            let mut sessions = sessions.write().await;
                            if let Some(context) = sessions.get_mut(&session_id) {
                                let event = match persist_result {
                                    Ok(()) => PairingEvent::PersistOk {
                                        session_id: session_id.clone(),
                                        device_id: peer_id_str,
                                    },
                                    Err(err) => PairingEvent::PersistErr {
                                        session_id: session_id.clone(),
                                        error: err.to_string(),
                                    },
                                };
                                let (_state, actions) =
                                    context.state_machine.handle_event(event, Utc::now());
                                tracing::debug!(
                                    session_id = %session_id,
                                    num_actions = actions.len(),
                                    "Persist event generated actions"
                                );
                                actions
                            } else {
                                vec![]
                            }
                        };
                        queue.extend(actions);
                    }
                    PairingAction::StartTimer {
                        session_id: action_session_id,
                        kind,
                        deadline,
                    } => {
                        let sessions_for_timer = sessions.clone();
                        let peers_for_timer = session_peers.clone();
                        let event_senders_for_timer = event_senders.clone();
                        let mut sessions = sessions.write().await;
                        let context = sessions
                            .get_mut(&action_session_id)
                            .context("Session not found")?;
                        {
                            let mut timers = context.timers.lock().await;
                            if let Some(handle) = timers.remove(&kind) {
                                handle.abort();
                            }
                        }

                        let action_tx = action_tx.clone();
                        let sessions = sessions_for_timer;
                        let session_peers = peers_for_timer;
                        let event_senders = event_senders_for_timer;
                        let trust_peer_orch_for_timer = trust_peer_orch.clone();
                        let session_id_for_log = action_session_id.clone();
                        let sleep_duration = deadline
                            .signed_duration_since(Utc::now())
                            .to_std()
                            .unwrap_or_else(|_| std::time::Duration::from_secs(0));
                        let future = async move {
                            tokio::time::sleep(sleep_duration).await;
                            if let Err(error) = Self::handle_timeout(
                                action_tx,
                                sessions,
                                session_peers,
                                event_senders,
                                trust_peer_orch_for_timer,
                                action_session_id,
                                kind,
                            )
                            .await
                            {
                                tracing::error!(
                                    %session_id_for_log,
                                    ?kind,
                                    error = ?error,
                                    "pairing timer handling failed"
                                );
                            }
                        };
                        let future: Pin<Box<dyn Future<Output = ()> + Send>> = Box::pin(future);
                        let handle = tokio::spawn(future);

                        let abort_handle = handle.abort_handle();
                        let mut timers = context.timers.lock().await;
                        timers.insert(kind, abort_handle);
                    }
                    PairingAction::CancelTimer {
                        session_id: action_session_id,
                        kind,
                    } => {
                        let mut sessions = sessions.write().await;
                        let context = sessions
                            .get_mut(&action_session_id)
                            .context("Session not found")?;
                        let mut timers = context.timers.lock().await;
                        if let Some(handle) = timers.remove(&kind) {
                            handle.abort();
                        }
                    }
                    PairingAction::LogTransition { .. } => {
                        // Already logged, no additional action needed
                    }
                    PairingAction::NoOp => {}
                }
            }

            Ok(())
        }
    }

    /// Handle a timer timeout by feeding the timeout event into the state machine.
    fn handle_timeout(
        action_tx: mpsc::Sender<PairingAction>,
        sessions: Arc<RwLock<HashMap<SessionId, PairingSessionContext>>>,
        session_peers: Arc<RwLock<HashMap<SessionId, PairingPeerInfo>>>,
        event_senders: Arc<Mutex<Vec<mpsc::Sender<PairingDomainEvent>>>>,
        trust_peer_orch: SharedTrustPeerOrchestrator,
        session_id: String,
        kind: TimeoutKind,
    ) -> impl Future<Output = Result<()>> + Send {
        async move {
            let span = info_span!(
                "pairing.handle_timeout",
                session_id = %session_id,
                kind = ?kind
            );
            async {
                let actions = {
                    let mut sessions = sessions.write().await;
                    let context = sessions.get_mut(&session_id).context("Session not found")?;
                    {
                        let mut timers = context.timers.lock().await;
                        timers.remove(&kind);
                    }
                    let (_state, actions) = context.state_machine.handle_event(
                        PairingEvent::Timeout {
                            session_id: session_id.clone(),
                            kind,
                        },
                        Utc::now(),
                    );
                    actions
                };

                for action in actions {
                    Self::execute_action_inner(
                        action_tx.clone(),
                        sessions.clone(),
                        session_peers.clone(),
                        event_senders.clone(),
                        trust_peer_orch.clone(),
                        session_id.clone(),
                        action,
                    )
                    .await?;
                }

                Ok(())
            }
            .instrument(span)
            .await
        }
    }

    /// Emit a domain event to all subscribers.
    pub(crate) async fn emit_event(&self, event: PairingDomainEvent) {
        Self::emit_event_to_senders(self.event_senders.clone(), event).await;
    }

    /// Emit a domain event to all senders (static version for use in action execution).
    async fn emit_event_to_senders(
        event_senders: Arc<Mutex<Vec<mpsc::Sender<PairingDomainEvent>>>>,
        event: PairingDomainEvent,
    ) {
        let senders = { event_senders.lock().await.clone() };
        for sender in senders {
            if sender.send(event.clone()).await.is_err() {
                tracing::debug!("Pairing event receiver dropped");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    //! 0.4.2 migration note: the four Phase-2 `dual_write_*` unit tests
    //! that lived here have been replaced by trusted-peer assertions that
    //! mirror the new write-path contract:
    //!
    //! - `trust_flow_persists_trusted_peer_on_success`
    //! - `trust_flow_rejects_second_pairing_for_same_peer`
    //! - `trust_flow_reset_allows_pairing_another_peer`
    //! - `trust_flow_peer_device_id_matches_peer_id_string` (D5)
    //!
    //! The tests exercise the `TrustPeerOrchestrator` driver sequence that
    //! the `PersistPairedDevice` branch now runs, using the same
    //! `InMemoryTrustedPeerRepository` test double that the orchestrator's
    //! own unit tests use.

    use crate::trusted_peer::testing::InMemoryTrustedPeerRepository;
    use crate::trusted_peer::{
        TrustPeerOrchestrator, TrustState, TrustVerificationChallenge, TrustedPeerApplicationError,
    };
    use std::sync::Arc;
    use uc_core::security::IdentityFingerprint;
    use uc_core::{DeviceId, TrustedPeerRepositoryPort};

    fn build_orch(
        local: &str,
    ) -> (
        Arc<InMemoryTrustedPeerRepository>,
        TrustPeerOrchestrator<InMemoryTrustedPeerRepository>,
    ) {
        let repo = Arc::new(InMemoryTrustedPeerRepository::new());
        let orch = TrustPeerOrchestrator::new(repo.clone(), DeviceId::new(local));
        (repo, orch)
    }

    /// Pad a short seed into a valid 16-char alphanumeric fingerprint.
    fn fp(seed: &str) -> IdentityFingerprint {
        let mut raw: String = seed.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
        raw.make_ascii_uppercase();
        while raw.len() < 16 {
            raw.push('A');
        }
        IdentityFingerprint::from_raw_string(&raw[..16]).unwrap()
    }

    fn challenge(peer_fingerprint: IdentityFingerprint) -> TrustVerificationChallenge {
        TrustVerificationChallenge {
            peer_fingerprint,
            short_code: String::new(),
        }
    }

    async fn drive_persist_flow(
        orch: &TrustPeerOrchestrator<InMemoryTrustedPeerRepository>,
        peer_device_id: DeviceId,
        fingerprint: IdentityFingerprint,
    ) -> Result<TrustState, TrustedPeerApplicationError> {
        orch.reset().await;
        orch.initiate(peer_device_id.clone()).await?;
        orch.record_session_opened(peer_device_id, challenge(fingerprint))
            .await?;
        orch.confirm_verification().await
    }

    #[tokio::test]
    async fn trust_flow_persists_trusted_peer_on_success() {
        // Replaces `dual_write_persists_member_on_success`: the PersistPairedDevice
        // branch now drives the trust orchestrator's full Idle→Trusted sequence.
        let (repo, orch) = build_orch("local-1");

        let expected_fp = fp("FPXYZ");
        let final_state = drive_persist_flow(&orch, DeviceId::new("peer-xyz"), expected_fp.clone())
            .await
            .unwrap();

        match final_state {
            TrustState::Trusted { trusted_peer } => {
                assert_eq!(trusted_peer.local_device_id.as_str(), "local-1");
                assert_eq!(trusted_peer.peer_device_id.as_str(), "peer-xyz");
                assert_eq!(trusted_peer.peer_fingerprint, expected_fp);
            }
            other => panic!("expected Trusted, got {other:?}"),
        }

        let saved = repo.get(&DeviceId::new("peer-xyz")).await.unwrap();
        assert!(saved.is_some());
        assert_eq!(saved.unwrap().peer_fingerprint, expected_fp);
    }

    #[tokio::test]
    async fn trust_flow_rejects_second_pairing_for_same_peer() {
        // Replaces `dual_write_swallows_already_admitted_errors`: under the
        // new model re-pairing the same peer without an explicit distrust
        // returns `AlreadyTrusted` (D21) rather than silently overwriting.
        let (_repo, orch) = build_orch("local-1");

        drive_persist_flow(&orch, DeviceId::new("peer-xyz"), fp("FPXYZ"))
            .await
            .unwrap();

        let err = drive_persist_flow(&orch, DeviceId::new("peer-xyz"), fp("FPXYZROTATED"))
            .await
            .unwrap_err();
        assert_eq!(
            err,
            TrustedPeerApplicationError::AlreadyTrusted(DeviceId::new("peer-xyz"))
        );
    }

    #[tokio::test]
    async fn trust_flow_reset_allows_pairing_another_peer() {
        // Replaces `dual_write_swallows_repository_errors`: proves the
        // singleton-orchestrator reset contract — after one flow reaches
        // the `Trusted` terminal state, the next flow with a different
        // peer still works.
        let (repo, orch) = build_orch("local-1");

        drive_persist_flow(&orch, DeviceId::new("peer-a"), fp("FPA"))
            .await
            .unwrap();
        drive_persist_flow(&orch, DeviceId::new("peer-b"), fp("FPB"))
            .await
            .unwrap();

        assert!(repo.get(&DeviceId::new("peer-a")).await.unwrap().is_some());
        assert!(repo.get(&DeviceId::new("peer-b")).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn trust_flow_peer_device_id_matches_peer_id_string() {
        // Replaces `space_member_from_paired_device_maps_core_fields`:
        // validates D5 (`DeviceId == peer_id` string reuse) at the
        // trust-peer boundary — the identifier that reaches the repository
        // is exactly the peer-id string that the pairing protocol supplied.
        let (repo, orch) = build_orch("local-1");
        let peer_id_str = "long-peer-id-with-specific-bytes-xyz";

        drive_persist_flow(&orch, DeviceId::new(peer_id_str), fp("FP"))
            .await
            .unwrap();

        let saved = repo
            .get(&DeviceId::new(peer_id_str))
            .await
            .unwrap()
            .expect("expected trusted peer to be persisted under the peer-id device id");
        assert_eq!(saved.peer_device_id.as_str(), peer_id_str);
    }
}
