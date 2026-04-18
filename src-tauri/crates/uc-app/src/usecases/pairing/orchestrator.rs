//! Pairing protocol orchestrator
//!
//! This module coordinates the pairing state machine by converting network events,
//! user inputs, and timer events into state machine events, then executing the
//! resulting actions.
//!
//! # Architecture
//!
//! ```text
//! Network/User/Timer Events
//!   |
//! PairingOrchestrator (thin coordinator)
//!   |--- PairingSessionManager (session lifecycle)
//!   |--- PairingProtocolHandler (action execution)
//!   |
//! PairingStateMachine (pure state transitions)
//!   |
//! PairingActions (executed by protocol handler)
//!   |
//! Network/User/Persistence side effects
//! ```

use anyhow::Result;

use super::crypto::PairingCryptoPorts;
use super::{PairingDomainEvent, PairingEventPort, PairingFacade};
use crate::usecases::pairing::staged_paired_device_store::StagedPairedDeviceStore;
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tracing::{info_span, Instrument};

use super::protocol_handler::PairingProtocolHandler;
use super::session_manager::{LocalDeviceInfo, PairingSessionContext, PairingSessionManager};

use uc_core::{
    network::{
        protocol::{
            PairingChallenge, PairingChallengeResponse, PairingConfirm, PairingKeyslotOffer,
            PairingRequest,
        },
        SessionId,
    },
    pairing::PairingRole,
    ports::PairedDeviceRepositoryPort,
    settings::model::Settings,
    MemberRepositoryPort,
};

use super::state_machine::{PairingAction, PairingEvent, PairingState};

/// Pairing orchestrator configuration
#[derive(Debug, Clone)]
pub struct PairingConfig {
    /// Step timeout (seconds)
    pub step_timeout_secs: i64,
    /// User verification timeout (seconds)
    pub user_verification_timeout_secs: i64,
    /// Session timeout (seconds)
    pub session_timeout_secs: i64,
    /// Max retries
    pub max_retries: u8,
    /// Protocol version
    pub protocol_version: String,
}

impl Default for PairingConfig {
    fn default() -> Self {
        Self::from_settings(&Settings::default())
    }
}

impl PairingConfig {
    pub fn from_settings(settings: &Settings) -> Self {
        let pairing = &settings.pairing;
        let step = pairing.step_timeout.as_secs().min(i64::MAX as u64) as i64;
        let verify = pairing
            .user_verification_timeout
            .as_secs()
            .min(i64::MAX as u64) as i64;
        let session = pairing.session_timeout.as_secs().min(i64::MAX as u64) as i64;

        Self {
            step_timeout_secs: step.max(1),
            user_verification_timeout_secs: verify.max(1),
            session_timeout_secs: session.max(1),
            max_retries: pairing.max_retries.max(1),
            protocol_version: pairing.protocol_version.clone(),
        }
    }
}

/// Pairing orchestrator -- thin coordinator delegating to session manager and protocol handler.
#[derive(Clone)]
pub struct PairingOrchestrator {
    /// Session lifecycle manager
    session_manager: PairingSessionManager,
    /// Protocol action handler
    protocol_handler: PairingProtocolHandler,
}

/// Re-export PairingPeerInfo as a public type for API compatibility.
pub use super::session_manager::PairingPeerInfo;

impl PairingOrchestrator {
    /// Create a new pairing orchestrator.
    pub fn new(
        config: PairingConfig,
        device_repo: Arc<dyn PairedDeviceRepositoryPort + Send + Sync + 'static>,
        member_repo: Arc<dyn MemberRepositoryPort + Send + Sync + 'static>,
        local_device_name: String,
        local_device_id: String,
        local_peer_id: String,
        local_identity_pubkey: Vec<u8>,
        staged_store: Arc<StagedPairedDeviceStore>,
        crypto: Arc<PairingCryptoPorts>,
    ) -> (Self, mpsc::Receiver<PairingAction>) {
        let (action_tx, action_rx) = mpsc::channel(100);
        let event_senders: Arc<Mutex<Vec<mpsc::Sender<PairingDomainEvent>>>> =
            Arc::new(Mutex::new(Vec::new()));

        let local_identity = LocalDeviceInfo {
            device_name: local_device_name,
            device_id: local_device_id,
            identity_pubkey: local_identity_pubkey,
            peer_id: local_peer_id,
        };

        let session_manager = PairingSessionManager::new(config, local_identity, crypto);
        let protocol_handler = PairingProtocolHandler::new(
            action_tx,
            device_repo,
            member_repo,
            staged_store,
            event_senders,
        );

        let orchestrator = Self {
            session_manager,
            protocol_handler,
        };

        (orchestrator, action_rx)
    }

    /// Initiate pairing (Initiator role).
    pub async fn initiate_pairing(&self, peer_id: String) -> Result<SessionId> {
        let span = info_span!("pairing.initiate", peer_id = %peer_id);
        async {
            let mut state_machine = self.session_manager.new_state_machine();
            let (state, actions) = state_machine.handle_event(
                PairingEvent::StartPairing {
                    role: PairingRole::Initiator,
                    peer_id: peer_id.clone(),
                },
                Utc::now(),
            );

            let session_id = match state {
                PairingState::RequestSent { session_id } => session_id,
                _ => {
                    return Err(anyhow::anyhow!(
                        "unexpected state after StartPairing: {:?}",
                        state
                    ))
                }
            };
            self.session_manager
                .record_session_peer(&session_id, peer_id.clone(), None)
                .await;

            let context = PairingSessionContext {
                state_machine,
                created_at: Utc::now(),
                timers: tokio::sync::Mutex::new(HashMap::new()),
            };

            self.session_manager
                .insert_session(session_id.clone(), context)
                .await;

            for action in actions {
                self.execute_action(&session_id, &peer_id, action).await?;
            }

            Ok(session_id)
        }
        .instrument(span)
        .await
    }

    /// Handle incoming pairing request (Responder role).
    pub async fn handle_incoming_request(
        &self,
        peer_id: String,
        request: PairingRequest,
    ) -> Result<()> {
        let expected_local_peer_id = self.session_manager.local_peer_id().to_string();
        if request.peer_id != expected_local_peer_id {
            tracing::warn!(
                session_id = %request.session_id,
                sender_peer_id = %peer_id,
                request_target_peer_id = %request.peer_id,
                expected_local_peer_id = %expected_local_peer_id,
                request_device_id = %request.device_id,
                request_device_name = %request.device_name,
                "incoming pairing request target peer_id mismatch"
            );
            return Err(anyhow::anyhow!(
                "Request target peer_id mismatch: expected {}, got {}",
                expected_local_peer_id,
                request.peer_id
            ));
        }

        let session_id = request.session_id.clone();
        let span = info_span!(
            "pairing.handle_request",
            session_id = %session_id,
            peer_id = %peer_id
        );
        async {
            tracing::info!(
                session_id = %session_id,
                sender_peer_id = %peer_id,
                request_target_peer_id = %request.peer_id,
                expected_local_peer_id = %expected_local_peer_id,
                request_device_id = %request.device_id,
                request_device_name = %request.device_name,
                "validated inbound pairing request target peer_id"
            );
            self.session_manager
                .record_session_peer(
                    &session_id,
                    peer_id.clone(),
                    Some(request.device_name.clone()),
                )
                .await;

            let mut state_machine = self.session_manager.new_state_machine();
            let (_state, actions) = state_machine.handle_event(
                PairingEvent::RecvRequest {
                    session_id: session_id.clone(),
                    sender_peer_id: peer_id.clone(),
                    request,
                },
                Utc::now(),
            );

            let context = PairingSessionContext {
                state_machine,
                created_at: Utc::now(),
                timers: tokio::sync::Mutex::new(HashMap::new()),
            };

            self.session_manager
                .insert_session(session_id.clone(), context)
                .await;

            for action in actions {
                self.execute_action(&session_id, &peer_id, action).await?;
            }

            Ok(())
        }
        .instrument(span)
        .await
    }

    /// Handle received Challenge (Initiator).
    pub async fn handle_challenge(
        &self,
        session_id: &str,
        peer_id: &str,
        challenge: PairingChallenge,
    ) -> Result<()> {
        let span = info_span!(
            "pairing.handle_challenge",
            session_id = %session_id,
            peer_id = %peer_id
        );
        async {
            self.session_manager
                .record_session_peer(
                    session_id,
                    peer_id.to_string(),
                    Some(challenge.device_name.clone()),
                )
                .await;
            let actions = self
                .session_manager
                .process_event(
                    session_id,
                    PairingEvent::RecvChallenge {
                        session_id: session_id.to_string(),
                        challenge,
                    },
                )
                .await?;

            for action in actions {
                self.execute_action(session_id, peer_id, action).await?;
            }

            Ok(())
        }
        .instrument(span)
        .await
    }

    /// Handle received KeyslotOffer (Initiator).
    pub async fn handle_keyslot_offer(
        &self,
        session_id: &str,
        peer_id: &str,
        offer: PairingKeyslotOffer,
    ) -> Result<()> {
        let span = info_span!(
            "pairing.handle_keyslot_offer",
            session_id = %session_id,
            peer_id = %peer_id
        );
        async {
            let has_keyslot = offer.keyslot_file.as_ref().is_some();
            let has_challenge = offer.challenge.as_ref().is_some();
            tracing::info!(
                session_id = %session_id,
                peer_id = %peer_id,
                has_keyslot,
                has_challenge,
                "Handling pairing keyslot offer"
            );
            let keyslot_file = match offer.keyslot_file {
                Some(keyslot_file) => keyslot_file,
                None => {
                    tracing::warn!(
                        session_id = %session_id,
                        peer_id = %peer_id,
                        "Keyslot offer missing keyslot file"
                    );
                    return Ok(());
                }
            };
            let challenge = match offer.challenge {
                Some(challenge) => challenge,
                None => {
                    tracing::warn!(
                        session_id = %session_id,
                        peer_id = %peer_id,
                        "Keyslot offer missing challenge"
                    );
                    return Ok(());
                }
            };
            self.protocol_handler
                .emit_event(PairingDomainEvent::KeyslotReceived {
                    session_id: session_id.to_string(),
                    peer_id: peer_id.to_string(),
                    keyslot_file,
                    challenge,
                })
                .await;
            Ok(())
        }
        .instrument(span)
        .await
    }

    /// Handle received ChallengeResponse (Responder).
    pub async fn handle_challenge_response(
        &self,
        session_id: &str,
        peer_id: &str,
        response: PairingChallengeResponse,
    ) -> Result<()> {
        let span = info_span!(
            "pairing.handle_challenge_response",
            session_id = %session_id,
            peer_id = %peer_id
        );
        async {
            let has_encrypted_challenge = response.encrypted_challenge.as_ref().is_some();
            tracing::info!(
                session_id = %session_id,
                peer_id = %peer_id,
                has_encrypted_challenge,
                "Handling pairing challenge response"
            );
            Ok(())
        }
        .instrument(span)
        .await
    }

    /// Handle received Response (Responder).
    pub async fn handle_response(
        &self,
        session_id: &str,
        peer_id: &str,
        response: uc_core::network::protocol::PairingResponse,
    ) -> Result<()> {
        let span = info_span!(
            "pairing.handle_response",
            session_id = %session_id,
            peer_id = %peer_id
        );
        async {
            tracing::info!(
                session_id = %session_id,
                peer_id = %peer_id,
                accepted = %response.accepted,
                "Handling pairing response from initiator"
            );
            let actions = self
                .session_manager
                .process_event(
                    session_id,
                    PairingEvent::RecvResponse {
                        session_id: session_id.to_string(),
                        response,
                    },
                )
                .await?;

            for action in actions {
                self.execute_action(session_id, peer_id, action).await?;
            }

            Ok(())
        }
        .instrument(span)
        .await
    }

    /// User accepts pairing (verification short code match).
    pub async fn user_accept_pairing(&self, session_id: &str) -> Result<()> {
        let span = info_span!("pairing.user_accept", session_id = %session_id);
        async {
            let actions = self
                .session_manager
                .process_event(
                    session_id,
                    PairingEvent::UserAccept {
                        session_id: session_id.to_string(),
                    },
                )
                .await?;

            for action in actions {
                self.execute_action(session_id, "", action).await?;
            }

            Ok(())
        }
        .instrument(span)
        .await
    }

    /// User rejects pairing.
    pub async fn user_reject_pairing(&self, session_id: &str) -> Result<()> {
        let span = info_span!("pairing.user_reject", session_id = %session_id);
        async {
            let actions = self
                .session_manager
                .process_event(
                    session_id,
                    PairingEvent::UserReject {
                        session_id: session_id.to_string(),
                    },
                )
                .await?;

            for action in actions {
                self.execute_action(session_id, "", action).await?;
            }

            Ok(())
        }
        .instrument(span)
        .await
    }

    /// User cancels pairing.
    pub async fn user_cancel_pairing(&self, session_id: &str) -> Result<()> {
        let span = info_span!("pairing.user_cancel", session_id = %session_id);
        async {
            let actions = self
                .session_manager
                .process_event(
                    session_id,
                    PairingEvent::UserCancel {
                        session_id: session_id.to_string(),
                    },
                )
                .await?;

            for action in actions {
                self.execute_action(session_id, "", action).await?;
            }

            Ok(())
        }
        .instrument(span)
        .await
    }

    /// Handle received Confirm.
    pub async fn handle_confirm(
        &self,
        session_id: &str,
        peer_id: &str,
        confirm: PairingConfirm,
    ) -> Result<()> {
        let span = info_span!(
            "pairing.handle_confirm",
            session_id = %session_id,
            peer_id = %peer_id
        );
        async {
            tracing::info!(
                session_id = %session_id,
                peer_id = %peer_id,
                success = %confirm.success,
                error = ?confirm.error,
                "Handling pairing confirm message"
            );
            let actions = self
                .session_manager
                .process_event(
                    session_id,
                    PairingEvent::RecvConfirm {
                        session_id: session_id.to_string(),
                        confirm,
                    },
                )
                .await?;

            for action in actions {
                self.execute_action(session_id, peer_id, action).await?;
            }

            Ok(())
        }
        .instrument(span)
        .await
    }

    /// Handle received Reject.
    pub async fn handle_reject(&self, session_id: &str, peer_id: &str) -> Result<()> {
        let span = info_span!(
            "pairing.handle_reject",
            session_id = %session_id,
            peer_id = %peer_id
        );
        async {
            let actions = self
                .session_manager
                .process_event(
                    session_id,
                    PairingEvent::RecvReject {
                        session_id: session_id.to_string(),
                    },
                )
                .await?;

            for action in actions {
                self.execute_action(session_id, peer_id, action).await?;
            }

            Ok(())
        }
        .instrument(span)
        .await
    }

    /// Handle received Cancel.
    pub async fn handle_cancel(&self, session_id: &str, peer_id: &str) -> Result<()> {
        let span = info_span!(
            "pairing.handle_cancel",
            session_id = %session_id,
            peer_id = %peer_id
        );
        async {
            let actions = self
                .session_manager
                .process_event(
                    session_id,
                    PairingEvent::RecvCancel {
                        session_id: session_id.to_string(),
                    },
                )
                .await?;

            for action in actions {
                self.execute_action(session_id, peer_id, action).await?;
            }

            Ok(())
        }
        .instrument(span)
        .await
    }

    /// Handle received Busy.
    pub async fn handle_busy(
        &self,
        session_id: &str,
        peer_id: &str,
        reason: Option<String>,
    ) -> Result<()> {
        let span = info_span!(
            "pairing.handle_busy",
            session_id = %session_id,
            peer_id = %peer_id
        );
        async {
            let actions = self
                .session_manager
                .process_event(
                    session_id,
                    PairingEvent::RecvBusy {
                        session_id: session_id.to_string(),
                        reason,
                    },
                )
                .await?;

            for action in actions {
                self.execute_action(session_id, peer_id, action).await?;
            }

            Ok(())
        }
        .instrument(span)
        .await
    }

    /// Handle transport error.
    pub async fn handle_transport_error(
        &self,
        session_id: &str,
        peer_id: &str,
        error: String,
    ) -> Result<()> {
        let span = info_span!(
            "pairing.handle_transport_error",
            session_id = %session_id,
            peer_id = %peer_id,
            error = %error
        );
        async {
            let actions = self
                .session_manager
                .process_event_if_exists(
                    session_id,
                    PairingEvent::TransportError {
                        session_id: session_id.to_string(),
                        error: error.clone(),
                    },
                )
                .await;

            for action in actions {
                self.execute_action(session_id, peer_id, action).await?;
            }

            Ok(())
        }
        .instrument(span)
        .await
    }

    /// Get peer info for a session.
    pub async fn get_session_peer(
        &self,
        session_id: &str,
    ) -> Option<super::session_manager::PairingPeerInfo> {
        self.session_manager.get_session_peer(session_id).await
    }

    /// Get role for a session.
    pub async fn get_session_role(&self, session_id: &str) -> Option<PairingRole> {
        self.session_manager.get_session_role(session_id).await
    }

    /// Return whether a session currently exists in the orchestrator.
    pub async fn has_session(&self, session_id: &str) -> bool {
        self.session_manager.has_session(session_id).await
    }

    /// Return whether a session exists **and** is still in a non-terminal state.
    pub async fn has_active_session(&self, session_id: &str) -> bool {
        self.session_manager.has_active_session(session_id).await
    }

    /// Cleanup expired sessions.
    pub async fn cleanup_expired_sessions(&self) {
        self.session_manager.cleanup_expired_sessions().await
    }

    /// Execute a single action (delegates to protocol handler).
    async fn execute_action(
        &self,
        session_id: &str,
        peer_id: &str,
        action: PairingAction,
    ) -> Result<()> {
        self.protocol_handler
            .execute_action(
                session_id,
                peer_id,
                action,
                self.session_manager.sessions(),
                self.session_manager.session_peers(),
            )
            .await
    }
}

#[async_trait::async_trait]
impl PairingFacade for PairingOrchestrator {
    async fn initiate_pairing(&self, peer_id: String) -> anyhow::Result<SessionId> {
        Self::initiate_pairing(self, peer_id).await
    }

    async fn user_accept_pairing(&self, session_id: &str) -> anyhow::Result<()> {
        Self::user_accept_pairing(self, session_id).await
    }

    async fn user_reject_pairing(&self, session_id: &str) -> anyhow::Result<()> {
        Self::user_reject_pairing(self, session_id).await
    }

    async fn handle_challenge_response(
        &self,
        session_id: &str,
        peer_id: &str,
        response: PairingChallengeResponse,
    ) -> anyhow::Result<()> {
        Self::handle_challenge_response(self, session_id, peer_id, response).await
    }
}

#[async_trait::async_trait]
impl PairingEventPort for PairingOrchestrator {
    async fn subscribe(&self) -> anyhow::Result<mpsc::Receiver<PairingDomainEvent>> {
        let (event_tx, event_rx) = mpsc::channel(100);
        let mut senders = self.protocol_handler.event_senders().lock().await;
        senders.push(event_tx);
        Ok(event_rx)
    }
}
