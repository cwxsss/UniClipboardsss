//! Space access orchestrator.
//!
//! Coordinates space access state machine and side effects.

use std::sync::Arc;

use chrono::Utc;
use tokio::sync::{mpsc, Mutex};
use tracing::{info_span, warn, Instrument};

use uc_core::ids::{DeviceId, SessionId, SpaceId};
use uc_core::space_access::action::SpaceAccessAction;
use uc_core::space_access::deny_reason_to_code;
use uc_core::space_access::event::SpaceAccessEvent;
use uc_core::space_access::state::{CancelReason, DenyReason, SpaceAccessState};
use uc_core::space_access::state_machine::SpaceAccessStateMachine;
use uc_core::{MemberRepositoryPort, MemberSyncPreferences};

use crate::membership::usecases::{AdmitMember, AdmitMemberUseCase};

use super::context::SpaceAccessContext;
use super::events::{SpaceAccessCompletedEvent, SpaceAccessEventPort};
use super::executor::SpaceAccessExecutor;

/// Admit member use case bound by a dyn repository so `SpaceAccessOrchestrator`
/// stays non-generic across its many consumers (bootstrap / daemon / setup /
/// tauri). Matches D28 shape for `TrustPeerOrchestrator`. The `Send + Sync`
/// bound is implied by the `MemberRepositoryPort` supertrait, so the dyn form
/// mirrors the `Arc<dyn MemberRepositoryPort>` already used in `DevicePorts`.
pub type AdmitMemberUseCaseDyn = AdmitMemberUseCase<dyn MemberRepositoryPort>;

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
    /// 可选 admit 入口。若注入，则任一角色到达 `Granted` 时把对端登记为
    /// 本机空间成员。成员关系是本地自治的 —— 两侧都必须把对端纳入
    /// `member_repo`，否则策略解析器（`ResolveConnectionPolicy` 查 member_repo）
    /// 会把刚配对完的对端判为 `Untrusted`，导致 business 协议被拒。
    admit_member: Option<Arc<AdmitMemberUseCaseDyn>>,
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
            admit_member: None,
        }
    }

    /// Inject the `AdmitMemberUseCase` so that a successful `Granted`
    /// transition — on either sponsor or joiner side — also registers the
    /// remote peer as a local space member. Without this, `Granted` only
    /// persists the trust relationship and the business protocol will be
    /// denied by policy at the first inbound stream (the resolver reads
    /// `member_repo`).
    pub fn with_admit_member(mut self, admit_member: Arc<AdmitMemberUseCaseDyn>) -> Self {
        self.admit_member = Some(admit_member);
        self
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
            if matches!(next, SpaceAccessState::Granted { .. }) {
                // Both sides register the remote peer as a local member so
                // the policy resolver (reads `member_repo`) will allow the
                // business protocol immediately after pairing. Failure only
                // warns — it must not block `Granted` itself.
                self.try_admit_peer_as_member().await;
            }

            let mut guard = self.state.lock().await;
            *guard = next.clone();
            Ok(next)
        }
        .instrument(span)
        .await
    }

    /// `Granted` 后把对端（sponsor 或 joiner，取决于本机角色）登记为本机
    /// 空间成员。两侧都必须调用，否则 `ResolveConnectionPolicy`（查 member_repo）
    /// 会把刚配对完的对端判为 `Untrusted`，接收端的 business stream handler
    /// 会直接 "denied by policy" 拒绝，导致剪贴板/文件元数据同步无法进行。
    ///
    /// 语义对齐 `dual_write_member`（Phase 2）：admit 失败只记 WARN，不阻塞
    /// `Granted` 本身 —— 成员关系是本地自治的，pairing / space_access 的成功
    /// 不应被 admit 失败翻盘。必需上下文（peer_id / device_name / fingerprint）
    /// 任一缺失都会 WARN 跳过。
    async fn try_admit_peer_as_member(&self) {
        let Some(admit) = self.admit_member.clone() else {
            return;
        };

        let (peer_id, device_name, fingerprint) = {
            let context = self.context.lock().await;
            (
                context.sponsor_peer_id.clone(),
                context.peer_device_name.clone(),
                context.peer_fingerprint.clone(),
            )
        };

        let Some(peer_id) = peer_id else {
            warn!("space_access Granted without sponsor_peer_id; skipping admit_member");
            return;
        };
        let Some(device_name) = device_name else {
            warn!(
                peer_id = %peer_id,
                "space_access Granted without peer_device_name; skipping admit_member"
            );
            return;
        };
        let Some(fingerprint) = fingerprint else {
            warn!(
                peer_id = %peer_id,
                "space_access Granted without peer_fingerprint; skipping admit_member"
            );
            return;
        };

        let input = AdmitMember {
            device_id: DeviceId::new(peer_id.clone()),
            device_name,
            identity_fingerprint: fingerprint,
            joined_at: Utc::now(),
            sync_preferences: MemberSyncPreferences::default(),
        };

        match admit.execute(input).await {
            Ok(member) => {
                tracing::info!(
                    peer_id = %peer_id,
                    device_id = %member.device_id,
                    "admit_member succeeded at space_access Granted"
                );
            }
            Err(err) => {
                warn!(
                    peer_id = %peer_id,
                    error = %err,
                    "admit_member failed at space_access Granted; continuing (local-only effect)"
                );
            }
        }
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
                    let offer = executor
                        .space_access
                        .prepare_join_offer(&space_id, executor.passphrase)
                        .await
                        .map_err(|e| SpaceAccessError::Crypto(anyhow::anyhow!(e)))?;
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

                    let domain_passphrase =
                        uc_core::crypto::domain::Passphrase::new(passphrase.expose().to_string());
                    let join_offer = uc_core::space_access::JoinOffer {
                        space_id: offer.space_id.clone(),
                        keyslot_blob: offer.keyslot_blob.clone(),
                        challenge_nonce: offer.challenge_nonce,
                    };
                    let master_key = executor
                        .space_access
                        .derive_master_key_for_proof(&join_offer, &domain_passphrase)
                        .await
                        .map_err(|e| SpaceAccessError::Crypto(anyhow::anyhow!(e)))?;

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

#[cfg(test)]
mod admit_tests {
    //! White-box coverage for `try_admit_peer_as_member`.
    //!
    //! We exercise the method directly rather than driving the state machine
    //! to `Granted` through `dispatch`, because `dispatch` requires a
    //! fully-wired `SpaceAccessExecutor` (Crypto / Transport / Proof / Timer
    //! / Persistence) that has nothing to do with admit behavior. Both
    //! dispatch branches (joiner-side `Granted` and sponsor-side
    //! responder-flow `Granted`) call this same method, so exercising it
    //! once covers both roles.
    use super::*;
    use std::sync::Mutex as StdMutex;
    use uc_core::{MembershipError, SpaceMember};

    /// In-memory `MemberRepositoryPort` with optional error injection, so
    /// tests can verify both the happy path and the "already admitted" /
    /// repository-error WARN paths.
    struct FakeMemberRepo {
        members: StdMutex<Vec<SpaceMember>>,
        fail_save_with: StdMutex<Option<MembershipError>>,
    }

    impl FakeMemberRepo {
        fn new() -> Self {
            Self {
                members: StdMutex::new(Vec::new()),
                fail_save_with: StdMutex::new(None),
            }
        }

        fn count(&self) -> usize {
            self.members.lock().unwrap().len()
        }

        fn first(&self) -> Option<SpaceMember> {
            self.members.lock().unwrap().first().cloned()
        }

        fn preload(&self, member: SpaceMember) {
            self.members.lock().unwrap().push(member);
        }
    }

    #[async_trait::async_trait]
    impl MemberRepositoryPort for FakeMemberRepo {
        async fn get(&self, device_id: &DeviceId) -> Result<Option<SpaceMember>, MembershipError> {
            Ok(self
                .members
                .lock()
                .unwrap()
                .iter()
                .find(|m| &m.device_id == device_id)
                .cloned())
        }

        async fn list(&self) -> Result<Vec<SpaceMember>, MembershipError> {
            Ok(self.members.lock().unwrap().clone())
        }

        async fn save(&self, member: &SpaceMember) -> Result<(), MembershipError> {
            if let Some(err) = self.fail_save_with.lock().unwrap().take() {
                return Err(err);
            }
            let mut guard = self.members.lock().unwrap();
            if let Some(existing) = guard.iter_mut().find(|m| m.device_id == member.device_id) {
                *existing = member.clone();
            } else {
                guard.push(member.clone());
            }
            Ok(())
        }

        async fn remove(&self, _device_id: &DeviceId) -> Result<bool, MembershipError> {
            Ok(false)
        }
    }

    fn make_sample_member(peer_id: &str) -> SpaceMember {
        SpaceMember {
            device_id: DeviceId::new(peer_id.to_string()),
            device_name: "preloaded".to_string(),
            identity_fingerprint: "old-fingerprint".to_string(),
            joined_at: Utc::now(),
            sync_preferences: MemberSyncPreferences::default(),
        }
    }

    async fn seed_context(
        orch: &SpaceAccessOrchestrator,
        peer_id: Option<&str>,
        device_name: Option<&str>,
        fingerprint: Option<&str>,
    ) {
        let ctx = orch.context();
        let mut guard = ctx.lock().await;
        guard.sponsor_peer_id = peer_id.map(String::from);
        guard.peer_device_name = device_name.map(String::from);
        guard.peer_fingerprint = fingerprint.map(String::from);
    }

    #[tokio::test]
    async fn no_admit_injected_is_noop() {
        let orch = SpaceAccessOrchestrator::new();
        seed_context(&orch, Some("peer-1"), Some("Alice"), Some("fp-1")).await;

        // No admit_member wired — should simply return without panicking.
        orch.try_admit_peer_as_member().await;
    }

    #[tokio::test]
    async fn missing_sponsor_peer_id_skips_admit() {
        let repo = Arc::new(FakeMemberRepo::new());
        let admit: Arc<AdmitMemberUseCaseDyn> = Arc::new(AdmitMemberUseCase::new(
            repo.clone() as Arc<dyn MemberRepositoryPort>
        ));
        let orch = SpaceAccessOrchestrator::new().with_admit_member(admit);
        seed_context(&orch, None, Some("Alice"), Some("fp-1")).await;

        orch.try_admit_peer_as_member().await;

        assert_eq!(repo.count(), 0, "admit should not fire without peer_id");
    }

    #[tokio::test]
    async fn missing_peer_device_name_skips_admit() {
        let repo = Arc::new(FakeMemberRepo::new());
        let admit: Arc<AdmitMemberUseCaseDyn> = Arc::new(AdmitMemberUseCase::new(
            repo.clone() as Arc<dyn MemberRepositoryPort>
        ));
        let orch = SpaceAccessOrchestrator::new().with_admit_member(admit);
        seed_context(&orch, Some("peer-1"), None, Some("fp-1")).await;

        orch.try_admit_peer_as_member().await;

        assert_eq!(repo.count(), 0, "admit should not fire without device_name");
    }

    #[tokio::test]
    async fn missing_peer_fingerprint_skips_admit() {
        let repo = Arc::new(FakeMemberRepo::new());
        let admit: Arc<AdmitMemberUseCaseDyn> = Arc::new(AdmitMemberUseCase::new(
            repo.clone() as Arc<dyn MemberRepositoryPort>
        ));
        let orch = SpaceAccessOrchestrator::new().with_admit_member(admit);
        seed_context(&orch, Some("peer-1"), Some("Alice"), None).await;

        orch.try_admit_peer_as_member().await;

        assert_eq!(repo.count(), 0, "admit should not fire without fingerprint");
    }

    #[tokio::test]
    async fn full_context_writes_member_to_repo() {
        let repo = Arc::new(FakeMemberRepo::new());
        let admit: Arc<AdmitMemberUseCaseDyn> = Arc::new(AdmitMemberUseCase::new(
            repo.clone() as Arc<dyn MemberRepositoryPort>
        ));
        let orch = SpaceAccessOrchestrator::new().with_admit_member(admit);
        seed_context(&orch, Some("peer-1"), Some("Alice"), Some("fp-1")).await;

        orch.try_admit_peer_as_member().await;

        assert_eq!(repo.count(), 1, "admit should persist exactly one member");
        let stored = repo.first().unwrap();
        assert_eq!(stored.device_id, DeviceId::new("peer-1".to_string()));
        assert_eq!(stored.device_name, "Alice");
        assert_eq!(stored.identity_fingerprint, "fp-1");
        assert_eq!(stored.sync_preferences, MemberSyncPreferences::default());
    }

    #[tokio::test]
    async fn already_admitted_is_warned_and_swallowed() {
        // Pre-existing row for peer-1 means AdmitMemberUseCase returns
        // AlreadyAdmitted. The orchestrator must log WARN but not panic,
        // and must not mutate the existing record.
        let repo = Arc::new(FakeMemberRepo::new());
        repo.preload(make_sample_member("peer-1"));

        let admit: Arc<AdmitMemberUseCaseDyn> = Arc::new(AdmitMemberUseCase::new(
            repo.clone() as Arc<dyn MemberRepositoryPort>
        ));
        let orch = SpaceAccessOrchestrator::new().with_admit_member(admit);
        seed_context(&orch, Some("peer-1"), Some("Alice"), Some("fp-1")).await;

        orch.try_admit_peer_as_member().await;

        assert_eq!(repo.count(), 1, "admit must not duplicate on conflict");
        let stored = repo.first().unwrap();
        assert_eq!(
            stored.device_name, "preloaded",
            "existing record must be preserved"
        );
        assert_eq!(stored.identity_fingerprint, "old-fingerprint");
    }
}
