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
use uc_core::ports::PairedDeviceRepositoryPort;
use uc_core::{
    DeviceId, MemberRepositoryPort, MemberSyncPreferences, MembershipError, SpaceMember,
};

use super::state_machine::{FailureReason, PairingAction, PairingEvent, TimeoutKind};

use super::events::PairingDomainEvent;
use super::session_manager::{PairingPeerInfo, PairingSessionContext};
use super::staged_paired_device_store::StagedPairedDeviceStore;

/// Handles execution of pairing protocol actions.
///
/// Owns port references needed for protocol operations: device_repo,
/// staged_store, and action_tx channel. Does NOT own sessions — borrows
/// them via Arc references passed from the orchestrator.
#[derive(Clone)]
pub(crate) struct PairingProtocolHandler {
    /// Action sender (forwarding actions to the network layer)
    action_tx: mpsc::Sender<PairingAction>,
    /// Paired device repository (authoritative write during Phase 2 migration).
    device_repo: Arc<dyn PairedDeviceRepositoryPort + Send + Sync + 'static>,
    /// Space member repository (shadow write during Phase 2 migration).
    member_repo: Arc<dyn MemberRepositoryPort + Send + Sync + 'static>,
    /// Staged paired device store
    staged_store: Arc<StagedPairedDeviceStore>,
    /// Event senders for domain events
    event_senders: Arc<Mutex<Vec<mpsc::Sender<PairingDomainEvent>>>>,
}

impl PairingProtocolHandler {
    /// Create a new protocol handler.
    pub(crate) fn new(
        action_tx: mpsc::Sender<PairingAction>,
        device_repo: Arc<dyn PairedDeviceRepositoryPort + Send + Sync + 'static>,
        member_repo: Arc<dyn MemberRepositoryPort + Send + Sync + 'static>,
        staged_store: Arc<StagedPairedDeviceStore>,
        event_senders: Arc<Mutex<Vec<mpsc::Sender<PairingDomainEvent>>>>,
    ) -> Self {
        Self {
            action_tx,
            device_repo,
            member_repo,
            staged_store,
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
            self.device_repo.clone(),
            self.member_repo.clone(),
            self.staged_store.clone(),
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
        device_repo: Arc<dyn PairedDeviceRepositoryPort + Send + Sync + 'static>,
        member_repo: Arc<dyn MemberRepositoryPort + Send + Sync + 'static>,
        staged_store: Arc<StagedPairedDeviceStore>,
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
                    } => {
                        let result_session_id_for_send = result_session_id.clone();
                        let error_for_send = error.clone();
                        tracing::info!(
                            session_id = %result_session_id,
                            success = %success,
                            error = ?error,
                            "Emitting pairing result to frontend"
                        );
                        action_tx
                            .send(PairingAction::EmitResult {
                                session_id: result_session_id_for_send,
                                success,
                                error: error_for_send,
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
                            let reason =
                                error.clone().map(FailureReason::Other).unwrap_or_else(|| {
                                    FailureReason::Other("Pairing failed".to_string())
                                });
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
                        device,
                    } => {
                        tracing::info!(
                            session_id = %session_id,
                            peer_id = %device.peer_id,
                            "Persisting paired device before verification completion"
                        );
                        let peer_id = device.peer_id.to_string();
                        staged_store.stage(&session_id, device.clone());

                        // Snapshot the fields we need for the shadow membership
                        // write before `device` is moved into `upsert`.
                        let member_snapshot = space_member_from_paired_device(&device);

                        let persist_result = device_repo.upsert(device).await;

                        if persist_result.is_ok() {
                            dual_write_member(member_repo.as_ref(), &session_id, member_snapshot)
                                .await;
                        }

                        let actions = {
                            let mut sessions = sessions.write().await;
                            if let Some(context) = sessions.get_mut(&session_id) {
                                let event = match persist_result {
                                    Ok(()) => PairingEvent::PersistOk {
                                        session_id: session_id.clone(),
                                        device_id: peer_id,
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
                        let device_repo = device_repo.clone();
                        let member_repo_for_timer = member_repo.clone();
                        let staged_store_for_timer = staged_store.clone();
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
                                device_repo,
                                member_repo_for_timer,
                                staged_store_for_timer,
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
        device_repo: Arc<dyn PairedDeviceRepositoryPort + Send + Sync + 'static>,
        member_repo: Arc<dyn MemberRepositoryPort + Send + Sync + 'static>,
        staged_store: Arc<StagedPairedDeviceStore>,
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
                        device_repo.clone(),
                        member_repo.clone(),
                        staged_store.clone(),
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

/// Build a [`SpaceMember`] snapshot from a [`PairedDevice`] for the Phase 2
/// shadow write to the new membership table.
///
/// During the paired_device → membership migration the pairing protocol
/// produces a `PairedDevice` as its persisted artefact. The membership
/// table stores a narrower view: no pairing state (removal = revocation),
/// no `last_seen_at` (moved to the network layer), and `device_id` reuses
/// the peer id string directly by design.
fn space_member_from_paired_device(device: &uc_core::pairing::PairedDevice) -> SpaceMember {
    SpaceMember {
        device_id: DeviceId::new(device.peer_id.as_str()),
        device_name: device.device_name.clone(),
        identity_fingerprint: device.identity_fingerprint.clone(),
        joined_at: device.paired_at,
        sync_preferences: MemberSyncPreferences::default(),
    }
}

/// Shadow-write a member record alongside the authoritative paired_device
/// upsert. Failures are logged but do not fail the pairing flow — during
/// Phase 2 the paired_device table is still the source of truth.
async fn dual_write_member(
    member_repo: &(dyn MemberRepositoryPort + Send + Sync),
    session_id: &str,
    member: SpaceMember,
) {
    if let Err(err) = member_repo.save(&member).await {
        let kind = match err {
            MembershipError::AlreadyAdmitted(_) => "already_admitted",
            MembershipError::NotFound(_) => "not_found",
            MembershipError::Repository(_) => "repository",
        };
        tracing::warn!(
            session_id = %session_id,
            device_id = %member.device_id,
            error = %err,
            kind,
            "Phase 2 shadow write to space_member failed; paired_device succeeded"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::{TimeZone, Utc};
    use std::sync::Mutex as StdMutex;
    use uc_core::pairing::{PairedDevice, PairingState};
    use uc_core::PeerId;

    struct FakeMemberRepo {
        saved: StdMutex<Vec<SpaceMember>>,
        fail_with: Option<MembershipError>,
    }

    impl FakeMemberRepo {
        fn new() -> Self {
            Self {
                saved: StdMutex::new(Vec::new()),
                fail_with: None,
            }
        }

        fn failing(err: MembershipError) -> Self {
            Self {
                saved: StdMutex::new(Vec::new()),
                fail_with: Some(err),
            }
        }

        fn saved(&self) -> Vec<SpaceMember> {
            self.saved.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl MemberRepositoryPort for FakeMemberRepo {
        async fn get(&self, _device_id: &DeviceId) -> Result<Option<SpaceMember>, MembershipError> {
            Ok(None)
        }

        async fn list(&self) -> Result<Vec<SpaceMember>, MembershipError> {
            Ok(self.saved())
        }

        async fn save(&self, member: &SpaceMember) -> Result<(), MembershipError> {
            if let Some(err) = &self.fail_with {
                return Err(match err {
                    MembershipError::AlreadyAdmitted(id) => {
                        MembershipError::AlreadyAdmitted(id.clone())
                    }
                    MembershipError::NotFound(id) => MembershipError::NotFound(id.clone()),
                    MembershipError::Repository(msg) => MembershipError::Repository(msg.clone()),
                });
            }
            self.saved.lock().unwrap().push(member.clone());
            Ok(())
        }

        async fn remove(&self, _device_id: &DeviceId) -> Result<bool, MembershipError> {
            Ok(false)
        }
    }

    fn sample_device() -> PairedDevice {
        PairedDevice {
            peer_id: PeerId::from("peer-xyz"),
            pairing_state: PairingState::Trusted,
            identity_fingerprint: "fp-xyz".to_string(),
            paired_at: Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
            last_seen_at: None,
            device_name: "Laptop-A".to_string(),
            sync_settings: None,
        }
    }

    #[test]
    fn space_member_from_paired_device_maps_core_fields() {
        let device = sample_device();
        let member = space_member_from_paired_device(&device);

        assert_eq!(member.device_id.as_str(), "peer-xyz");
        assert_eq!(member.device_name, "Laptop-A");
        assert_eq!(member.identity_fingerprint, "fp-xyz");
        assert_eq!(member.joined_at, device.paired_at);
        assert_eq!(member.sync_preferences, MemberSyncPreferences::default());
    }

    #[tokio::test]
    async fn dual_write_persists_member_on_success() {
        let repo = FakeMemberRepo::new();
        let member = space_member_from_paired_device(&sample_device());

        dual_write_member(&repo, "session-1", member.clone()).await;

        let saved = repo.saved();
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0], member);
    }

    #[tokio::test]
    async fn dual_write_swallows_repository_errors() {
        // Phase 2 contract: a failure in the shadow write must not propagate;
        // the paired_device upsert is authoritative.
        let repo = FakeMemberRepo::failing(MembershipError::Repository(
            "simulated db outage".to_string(),
        ));
        let member = space_member_from_paired_device(&sample_device());

        dual_write_member(&repo, "session-1", member).await;

        assert!(repo.saved().is_empty());
    }

    #[tokio::test]
    async fn dual_write_swallows_already_admitted_errors() {
        // A repeated admit during dual-write (e.g. reconnect with existing
        // paired_device row) must not surface as a pairing failure.
        let repo =
            FakeMemberRepo::failing(MembershipError::AlreadyAdmitted(DeviceId::new("peer-xyz")));
        let member = space_member_from_paired_device(&sample_device());

        dual_write_member(&repo, "session-1", member).await;

        assert!(repo.saved().is_empty());
    }
}
