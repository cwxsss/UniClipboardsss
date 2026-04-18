//! Space access orchestrator.
//!
//! Coordinates space access state machine and side effects.

use std::sync::Arc;

use chrono::Utc;
use tokio::sync::{mpsc, Mutex};
use tracing::{info_span, Instrument};

use uc_core::ids::{SessionId, SpaceId};
use uc_core::space_access::action::SpaceAccessAction;
use uc_core::space_access::deny_reason_to_code;
use uc_core::space_access::event::SpaceAccessEvent;
use uc_core::space_access::state::{CancelReason, DenyReason, SpaceAccessState};
use uc_core::space_access::state_machine::SpaceAccessStateMachine;

use super::context::{SpaceAccessContext, SpaceAccessOffer};
use super::events::{SpaceAccessCompletedEvent, SpaceAccessEventPort};
use super::executor::SpaceAccessExecutor;

/// Errors produced by space access orchestrator.
#[derive(Debug, thiserror::Error)]
pub enum SpaceAccessError {
    #[error("space access action not implemented: {0}")]
    ActionNotImplemented(&'static str),
    #[error("space access missing pairing session id")]
    MissingPairingSessionId,
    #[error("space access missing context: {0}")]
    MissingContext(&'static str),
    #[error("space access crypto failed: {0}")]
    Crypto(#[from] anyhow::Error),
    #[error("space access timer failed: {0}")]
    Timer(#[source] anyhow::Error),
    #[error("space access persistence failed: {0}")]
    Persistence(#[source] anyhow::Error),
}

/// Orchestrator that drives space access state and side effects.
pub struct SpaceAccessOrchestrator {
    context: Arc<Mutex<SpaceAccessContext>>,
    state: Arc<Mutex<SpaceAccessState>>,
    dispatch_lock: Arc<Mutex<()>>,
    event_senders: Arc<Mutex<Vec<mpsc::Sender<SpaceAccessCompletedEvent>>>>,
}

impl SpaceAccessOrchestrator {
    pub fn new() -> Self {
        Self::with_context(SpaceAccessContext::default())
    }

    pub fn with_context(context: SpaceAccessContext) -> Self {
        Self {
            context: Arc::new(Mutex::new(context)),
            state: Arc::new(Mutex::new(SpaceAccessState::Idle)),
            dispatch_lock: Arc::new(Mutex::new(())),
            event_senders: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub async fn start_sponsor_authorization(
        &self,
        executor: &mut SpaceAccessExecutor<'_>,
        pairing_session_id: SessionId,
        space_id: SpaceId,
        ttl_secs: u64,
    ) -> Result<SpaceAccessState, SpaceAccessError> {
        let event = SpaceAccessEvent::SponsorAuthorizationRequested {
            pairing_session_id: pairing_session_id.clone(),
            space_id,
            ttl_secs,
        };
        self.dispatch(executor, event, Some(pairing_session_id))
            .await
    }

    pub async fn get_state(&self) -> SpaceAccessState {
        self.state.lock().await.clone()
    }

    pub fn context(&self) -> Arc<Mutex<SpaceAccessContext>> {
        Arc::clone(&self.context)
    }

    pub async fn reset(&self) {
        let _dispatch_guard = self.dispatch_lock.lock().await;
        *self.context.lock().await = SpaceAccessContext::default();
        *self.state.lock().await = SpaceAccessState::Idle;
    }

    pub async fn dispatch(
        &self,
        executor: &mut SpaceAccessExecutor<'_>,
        event: SpaceAccessEvent,
        pairing_session_id: Option<SessionId>,
    ) -> Result<SpaceAccessState, SpaceAccessError> {
        let _dispatch_guard = self.dispatch_lock.lock().await;

        let span = info_span!("usecase.space_access_orchestrator.dispatch", event = ?event);
        async {
            let current = self.state.lock().await.clone();

            // When re-entering from any non-Idle state (e.g. sponsor handling a
            // second joiner after the first completed, or a stale
            // WaitingJoinerProof from a failed pairing), clear stale context so
            // the new session starts with a clean slate.
            let restarting = !matches!(current, SpaceAccessState::Idle)
                && matches!(
                    event,
                    SpaceAccessEvent::SponsorAuthorizationRequested { .. }
                );
            if restarting {
                let mut context = self.context.lock().await;
                context.prepared_offer = None;
                context.joiner_offer = None;
                context.joiner_passphrase = None;
                context.proof_artifact = None;
                context.result_success = None;
                context.result_deny_reason = None;
                // sponsor_peer_id is set by wiring before dispatch — keep it.
            }

            let (next, actions) = SpaceAccessStateMachine::transition(current.clone(), event);
            let is_responder_flow = matches!(
                current,
                SpaceAccessState::WaitingJoinerProof {
                    pairing_session_id: _,
                    space_id: _,
                    expires_at: _,
                }
            );

            {
                let mut context = self.context.lock().await;
                match &next {
                    SpaceAccessState::Granted { .. } => {
                        context.result_success = Some(true);
                        context.result_deny_reason = None;
                    }
                    SpaceAccessState::Denied { reason, .. } => {
                        context.result_success = Some(false);
                        context.result_deny_reason = Some(reason.clone());
                    }
                    _ => {
                        context.result_success = None;
                        context.result_deny_reason = None;
                    }
                }
            }

            let sponsor_persisted = match self
                .execute_actions(executor, pairing_session_id.as_ref(), actions)
                .await
            {
                Ok(persisted) => persisted,
                Err(err) => {
                    if is_responder_flow {
                        self.emit_responder_completion(
                            &next,
                            false,
                            Some(err.to_string()),
                            pairing_session_id.as_ref(),
                        )
                        .await;
                    }
                    return Err(err);
                }
            };

            if is_responder_flow {
                self.emit_responder_completion(
                    &next,
                    sponsor_persisted,
                    None,
                    pairing_session_id.as_ref(),
                )
                .await;
            }

            let mut guard = self.state.lock().await;
            *guard = next.clone();
            Ok(next)
        }
        .instrument(span)
        .await
    }

    async fn emit_responder_completion(
        &self,
        next: &SpaceAccessState,
        sponsor_persisted: bool,
        action_error_reason: Option<String>,
        fallback_session_id: Option<&SessionId>,
    ) {
        let session_id = Self::resolve_session_id(next, fallback_session_id);
        let Some(session_id) = session_id else {
            return;
        };

        if let Some(reason) = action_error_reason {
            self.emit_completion(session_id.as_str(), false, Some(reason))
                .await;
            return;
        }

        match next {
            SpaceAccessState::Granted { .. } => {
                if sponsor_persisted {
                    self.emit_completion(session_id.as_str(), true, None).await;
                } else {
                    self.emit_completion(
                        session_id.as_str(),
                        false,
                        Some("sponsor_persist_not_executed".to_string()),
                    )
                    .await;
                }
            }
            SpaceAccessState::Denied { reason, .. } => {
                self.emit_completion(
                    session_id.as_str(),
                    false,
                    Some(Self::deny_reason_code(reason)),
                )
                .await;
            }
            SpaceAccessState::Cancelled { reason, .. } => {
                self.emit_completion(
                    session_id.as_str(),
                    false,
                    Some(Self::cancel_reason_code(reason)),
                )
                .await;
            }
            _ => {}
        }
    }

    fn resolve_session_id(
        state: &SpaceAccessState,
        fallback_session_id: Option<&SessionId>,
    ) -> Option<SessionId> {
        match state {
            SpaceAccessState::WaitingOffer {
                pairing_session_id, ..
            }
            | SpaceAccessState::WaitingUserPassphrase {
                pairing_session_id, ..
            }
            | SpaceAccessState::WaitingDecision {
                pairing_session_id, ..
            }
            | SpaceAccessState::WaitingJoinerProof {
                pairing_session_id, ..
            }
            | SpaceAccessState::Granted {
                pairing_session_id, ..
            }
            | SpaceAccessState::Denied {
                pairing_session_id, ..
            }
            | SpaceAccessState::Cancelled {
                pairing_session_id, ..
            } => Some(pairing_session_id.clone()),
            SpaceAccessState::Idle => fallback_session_id.cloned(),
        }
    }

    fn deny_reason_code(reason: &DenyReason) -> String {
        deny_reason_to_code(reason).to_string()
    }

    fn cancel_reason_code(reason: &CancelReason) -> String {
        match reason {
            CancelReason::UserCancelled => "user_cancelled",
            CancelReason::Timeout => "timeout",
            CancelReason::SessionClosed => "session_closed",
        }
        .to_string()
    }

    async fn emit_completion(&self, session_id: &str, success: bool, reason: Option<String>) {
        let peer_id = {
            let context = self.context.lock().await;
            context
                .sponsor_peer_id
                .clone()
                .unwrap_or_else(|| "unknown".to_string())
        };

        let senders_count = self.event_senders.lock().await.len();
        tracing::info!(
            session_id,
            success,
            ?reason,
            peer_id = %peer_id,
            senders_count,
            "emit_completion called"
        );

        let event = SpaceAccessCompletedEvent {
            session_id: session_id.to_string(),
            peer_id,
            success,
            reason,
            ts: Utc::now().timestamp_millis(),
        };

        let senders = { self.event_senders.lock().await.clone() };
        for sender in senders {
            if sender.send(event.clone()).await.is_err() {
                tracing::debug!("space access completion receiver dropped");
            }
        }
    }

    async fn execute_actions(
        &self,
        executor: &mut SpaceAccessExecutor<'_>,
        pairing_session_id: Option<&SessionId>,
        actions: Vec<SpaceAccessAction>,
    ) -> Result<bool, SpaceAccessError> {
        let mut sponsor_persisted = false;
        for action in actions {
            match action {
                SpaceAccessAction::RequestOfferPreparation {
                    pairing_session_id,
                    space_id,
                    expires_at: _,
                } => {
                    let keyslot = executor.crypto.export_keyslot_blob(&space_id).await?;
                    let nonce = executor.crypto.generate_nonce32().await;
                    let offer = SpaceAccessOffer {
                        space_id: space_id.clone(),
                        keyslot,
                        nonce,
                    };
                    let mut context = self.context.lock().await;
                    context.prepared_offer = Some(offer);
                    let _ = pairing_session_id;
                }
                SpaceAccessAction::SendOffer => {
                    let session_id =
                        pairing_session_id.ok_or(SpaceAccessError::MissingPairingSessionId)?;
                    executor.transport.send_offer(session_id).await?;
                }
                SpaceAccessAction::StartTimer { ttl_secs } => {
                    let session_id =
                        pairing_session_id.ok_or(SpaceAccessError::MissingPairingSessionId)?;
                    executor
                        .timer
                        .start(session_id, ttl_secs)
                        .await
                        .map_err(SpaceAccessError::Timer)?;
                }
                SpaceAccessAction::StopTimer => {
                    let session_id =
                        pairing_session_id.ok_or(SpaceAccessError::MissingPairingSessionId)?;
                    executor
                        .timer
                        .stop(session_id)
                        .await
                        .map_err(SpaceAccessError::Timer)?;
                }
                SpaceAccessAction::RequestSpaceKeyDerivation { space_id } => {
                    let session_id =
                        pairing_session_id.ok_or(SpaceAccessError::MissingPairingSessionId)?;
                    let (offer, passphrase) = {
                        let mut context = self.context.lock().await;
                        let offer = context
                            .joiner_offer
                            .as_ref()
                            .ok_or(SpaceAccessError::MissingContext("joiner offer"))?
                            .clone();

                        if offer.space_id != space_id {
                            return Err(SpaceAccessError::MissingContext(
                                "joiner offer space mismatch",
                            ));
                        }

                        let passphrase = context
                            .joiner_passphrase
                            .take()
                            .ok_or(SpaceAccessError::MissingContext("joiner passphrase"))?;

                        (offer, passphrase)
                    };

                    let master_key = executor
                        .crypto
                        .derive_master_key_from_keyslot(&offer.keyslot_blob, passphrase)
                        .await?;

                    let proof = executor
                        .proof
                        .build_proof(session_id, &space_id, offer.challenge_nonce, &master_key)
                        .await?;

                    let mut context = self.context.lock().await;
                    context.proof_artifact = Some(proof);
                }
                SpaceAccessAction::SendProof => {
                    let session_id =
                        pairing_session_id.ok_or(SpaceAccessError::MissingPairingSessionId)?;
                    executor.transport.send_proof(session_id).await?;
                }
                SpaceAccessAction::SendResult => {
                    let session_id =
                        pairing_session_id.ok_or(SpaceAccessError::MissingPairingSessionId)?;
                    executor.transport.send_result(session_id).await?;
                }
                SpaceAccessAction::PersistJoinerAccess { space_id } => {
                    let peer_id = {
                        let context = self.context.lock().await;
                        context
                            .sponsor_peer_id
                            .as_ref()
                            .cloned()
                            .ok_or(SpaceAccessError::MissingContext("sponsor peer id"))?
                    };
                    executor
                        .store
                        .persist_joiner_access(&space_id, &peer_id)
                        .await
                        .map_err(SpaceAccessError::Persistence)?;
                }
                SpaceAccessAction::PersistSponsorAccess { space_id } => {
                    let peer_id = {
                        let context = self.context.lock().await;
                        context
                            .sponsor_peer_id
                            .as_ref()
                            .cloned()
                            .ok_or(SpaceAccessError::MissingContext("sponsor peer id"))?
                    };

                    executor
                        .store
                        .persist_sponsor_access(&space_id, &peer_id)
                        .await
                        .map_err(SpaceAccessError::Persistence)?;
                    sponsor_persisted = true;
                }
            }
        }

        Ok(sponsor_persisted)
    }
}

#[async_trait::async_trait]
impl SpaceAccessEventPort for SpaceAccessOrchestrator {
    async fn subscribe(&self) -> anyhow::Result<mpsc::Receiver<SpaceAccessCompletedEvent>> {
        let (event_tx, event_rx) = mpsc::channel(100);
        let mut senders = self.event_senders.lock().await;
        senders.push(event_tx);
        Ok(event_rx)
    }
}
