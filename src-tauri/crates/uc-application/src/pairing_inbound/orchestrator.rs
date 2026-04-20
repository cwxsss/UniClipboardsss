//! Sponsor-side inbound pairing orchestrator.
//!
//! See the module-level doc in [`super`] for the full design rationale.
//!
//! ## Slice 1 sponsor handshake
//!
//! The orchestrator **reuses** the `uc-core` space-access FSM
//! ([`SpaceAccessStateMachine`] + [`SpaceAccessEvent`] + [`SpaceAccessAction`])
//! rather than reimplementing the state transitions. What's Slice-1-local is
//! the dispatcher that translates actions into `PairingSessionPort` /
//! `MemberRepositoryPort` / `TrustedPeerRepositoryPort` calls on the
//! iroh-native wire:
//!
//! ```text
//!   Incoming(Request) ─ match code ─► dispatch SponsorAuthorizationRequested
//!                      │
//!                      ▼
//!           FSM → WaitingJoinerProof
//!           actions: [RequestOfferPreparation, SendOffer, StartTimer]
//!                      │
//!                      ▼ (per-session ctx parked)
//!     MessageReceived(ChallengeResponse)
//!           verify_proof → ProofVerified | ProofRejected
//!                      ▼
//!           FSM → Granted | Denied
//!           actions: [SendResult, PersistSponsorAccess?, StopTimer]
//!                      ▼
//!           wire: Confirm | Reject(PassphraseMismatch)
//!                      ▼
//!                   close
//!
//!   Closed(session) ─ dispatch SessionClosed ─ clear ctx
//! ```
//!
//! Slice 1 P7f ignores `StartTimer` / `StopTimer` — no TTL on the handshake
//! yet. P7g will wire a `TimerPort` and translate those actions into
//! `tokio::time::sleep_until` cancel tokens.
//!
//! [`SpaceAccessStateMachine`]: uc_core::space_access::state_machine::SpaceAccessStateMachine
//! [`SpaceAccessEvent`]: uc_core::space_access::event::SpaceAccessEvent
//! [`SpaceAccessAction`]: uc_core::space_access::action::SpaceAccessAction

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{TimeZone, Utc};
use tokio::sync::mpsc::Receiver;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{debug, info, instrument, warn};

use uc_core::ids::{DeviceId, SessionId, SpaceId};
use uc_core::membership::{MemberRepositoryPort, MemberSyncPreferences, SpaceMember};
use uc_core::pairing::invitation::InvitationCode;
use uc_core::pairing::session_message::{
    JoinerChallengeResponse, JoinerRequest, PairingReject, PairingRejectReason,
    PairingSessionMessage, SponsorConfirm, SponsorKeyslotOffer,
};
use uc_core::ports::pairing::{
    PairingEventPort, PairingSessionEvent, PairingSessionId, PairingSessionPort,
};
use uc_core::ports::space::{ProofPort, SpaceAccessPort};
use uc_core::ports::{
    ClockPort, ConsumeInvitationError, DeviceIdentityPort, LocalIdentityPort,
    PairingInvitationPort, SettingsPort,
};
use uc_core::security::IdentityFingerprint;
use uc_core::space_access::action::SpaceAccessAction;
use uc_core::space_access::domain::{JoinOffer, SpaceAccessProofArtifact};
use uc_core::space_access::event::SpaceAccessEvent;
use uc_core::space_access::state::{DenyReason, SpaceAccessState};
use uc_core::space_access::state_machine::SpaceAccessStateMachine;
use uc_core::trusted_peer::{TrustedPeer, TrustedPeerRepositoryPort};

use crate::pairing_invitation::holder::{InMemoryPairingInvitationHolder, TakeMatchingError};

/// Per-session context mirroring the FSM state plus the Slice-1-local
/// data the action dispatcher needs (prepared offer, nonce, joiner
/// facts, verdict). Dropped on any terminal outcome (Granted, Denied,
/// Cancelled).
#[derive(Debug)]
struct SessionCtx {
    state: SpaceAccessState,
    /// Core-layer session id used as the HMAC binding input for
    /// `verify_proof`. Derived from the transport's `PairingSessionId`
    /// so joiner and sponsor independently arrive at the same value.
    core_session_id: SessionId,
    space_id: SpaceId,
    /// Filled by [`SpaceAccessAction::RequestOfferPreparation`], drained
    /// by [`SpaceAccessAction::SendOffer`].
    prepared_offer: Option<JoinOffer>,
    /// Stashed so `ChallengeResponse` can rebuild the proof artifact
    /// long after `SendOffer` has consumed `prepared_offer`.
    challenge_nonce: Option<[u8; 32]>,
    /// Set right before `ProofVerified` / `ProofRejected` is dispatched;
    /// consumed by [`SpaceAccessAction::SendResult`].
    result_success: Option<bool>,
    /// Set alongside `result_success = Some(false)`; surfaces in logs.
    result_deny_reason: Option<DenyReason>,
    /// Joiner facts carried from the inbound `Request`; `SendResult`
    /// and `PersistSponsorAccess` read them to build the `Confirm` wire
    /// and the `SpaceMember` / `TrustedPeer` rows.
    joiner_device_id: DeviceId,
    joiner_device_name: String,
    joiner_fingerprint: IdentityFingerprint,
}

/// Drives sponsor-side inbound pairing events.
///
/// Construction only captures the ports; the subscribe + event loop starts
/// with [`PairingInboundOrchestrator::spawn`]. Kept `pub(crate)` per §11.4.
pub(crate) struct PairingInboundOrchestrator {
    pairing_events: Arc<dyn PairingEventPort>,
    pairing_session: Arc<dyn PairingSessionPort>,
    pairing_invitation: Arc<dyn PairingInvitationPort>,
    holder: Arc<InMemoryPairingInvitationHolder>,
    clock: Arc<dyn ClockPort>,
    space_access: Arc<dyn SpaceAccessPort>,
    proof_port: Arc<dyn ProofPort>,
    member_repo: Arc<dyn MemberRepositoryPort>,
    trusted_peer_repo: Arc<dyn TrustedPeerRepositoryPort>,
    local_identity: Arc<dyn LocalIdentityPort>,
    device_identity: Arc<dyn DeviceIdentityPort>,
    settings: Arc<dyn SettingsPort>,
    sessions: Mutex<HashMap<PairingSessionId, SessionCtx>>,
}

#[allow(clippy::too_many_arguments)]
impl PairingInboundOrchestrator {
    pub(crate) fn new(
        pairing_events: Arc<dyn PairingEventPort>,
        pairing_session: Arc<dyn PairingSessionPort>,
        pairing_invitation: Arc<dyn PairingInvitationPort>,
        holder: Arc<InMemoryPairingInvitationHolder>,
        clock: Arc<dyn ClockPort>,
        space_access: Arc<dyn SpaceAccessPort>,
        proof_port: Arc<dyn ProofPort>,
        member_repo: Arc<dyn MemberRepositoryPort>,
        trusted_peer_repo: Arc<dyn TrustedPeerRepositoryPort>,
        local_identity: Arc<dyn LocalIdentityPort>,
        device_identity: Arc<dyn DeviceIdentityPort>,
        settings: Arc<dyn SettingsPort>,
    ) -> Self {
        Self {
            pairing_events,
            pairing_session,
            pairing_invitation,
            holder,
            clock,
            space_access,
            proof_port,
            member_repo,
            trusted_peer_repo,
            local_identity,
            device_identity,
            settings,
            sessions: Mutex::new(HashMap::new()),
        }
    }

    /// Subscribe to the event port and spawn a long-lived task that drains
    /// incoming events into [`Self::handle_event`]. The returned
    /// [`JoinHandle`] is owned by the facade so it can `abort()` on
    /// shutdown.
    pub(crate) fn spawn(self: Arc<Self>) -> JoinHandle<()> {
        tokio::spawn(async move {
            let rx = match self.pairing_events.subscribe().await {
                Ok(rx) => rx,
                Err(err) => {
                    warn!(
                        error = %err,
                        "pairing inbound orchestrator failed to subscribe; task exiting"
                    );
                    return;
                }
            };
            self.run_loop(rx).await;
        })
    }

    async fn run_loop(self: Arc<Self>, mut rx: Receiver<PairingSessionEvent>) {
        info!("pairing inbound orchestrator started");
        while let Some(event) = rx.recv().await {
            self.handle_event(event).await;
        }
        info!("pairing inbound orchestrator stopped (event channel closed)");
    }

    /// Dispatch one event. Exposed at `pub(crate)` so unit tests can feed
    /// synthetic events without spinning a real event loop.
    #[instrument(skip_all, fields(event = event_kind(&event)))]
    pub(crate) async fn handle_event(&self, event: PairingSessionEvent) {
        match event {
            PairingSessionEvent::Incoming { session, message } => {
                self.handle_incoming(session, message).await
            }
            PairingSessionEvent::MessageReceived { session, message } => {
                self.handle_message_received(session, message).await
            }
            PairingSessionEvent::Closed { session, reason } => {
                self.handle_closed(session, reason).await
            }
        }
    }

    async fn handle_incoming(&self, session: PairingSessionId, message: PairingSessionMessage) {
        let request = match message {
            PairingSessionMessage::Request(req) => req,
            other => {
                warn!(
                    session = %session,
                    variant = variant_name(&other),
                    "first pairing message was not Request; rejecting session"
                );
                self.reject_and_close(
                    &session,
                    PairingRejectReason::Internal(
                        "expected Request as first pairing message".into(),
                    ),
                )
                .await;
                return;
            }
        };
        self.handle_joiner_request(session, request).await;
    }

    async fn handle_joiner_request(&self, session: PairingSessionId, request: JoinerRequest) {
        let now_ms = self.clock.now_ms();
        let now = match Utc.timestamp_millis_opt(now_ms).single() {
            Some(ts) => ts,
            None => {
                warn!(
                    session = %session,
                    now_ms,
                    "ClockPort returned out-of-range timestamp; treating inbound as internal error"
                );
                self.reject_and_close(
                    &session,
                    PairingRejectReason::Internal("sponsor clock out of range".into()),
                )
                .await;
                return;
            }
        };

        let invitation_code = request.invitation_code.clone();
        match self.holder.take_matching(&invitation_code, now).await {
            Ok(invitation) => {
                info!(
                    session = %session,
                    code = %invitation.code().as_str(),
                    joiner_device_id = %request.device_id.as_str(),
                    "accepted joiner request for pending invitation"
                );
                self.notify_rendezvous_consume(invitation.code()).await;
                self.begin_handshake(session, request).await;
            }
            Err(TakeMatchingError::NotFound) => {
                warn!(
                    session = %session,
                    code = %invitation_code.as_str(),
                    "inbound pairing request for unknown code; rejecting"
                );
                self.reject_and_close(&session, PairingRejectReason::InvitationMismatch)
                    .await;
            }
            Err(TakeMatchingError::Expired) => {
                warn!(
                    session = %session,
                    code = %invitation_code.as_str(),
                    "inbound pairing request after invitation expired; rejecting"
                );
                self.reject_and_close(&session, PairingRejectReason::InvitationMismatch)
                    .await;
            }
            Err(TakeMatchingError::Internal(msg)) => {
                warn!(
                    session = %session,
                    code = %invitation_code.as_str(),
                    error = %msg,
                    "holder invariant broken on inbound pairing request; rejecting"
                );
                self.reject_and_close(&session, PairingRejectReason::Internal(msg))
                    .await;
            }
        }
    }

    /// Drive the FSM from `Idle` to `WaitingJoinerProof` and flush the
    /// emitted actions (prepare offer, send offer, start timer).
    async fn begin_handshake(&self, session: PairingSessionId, request: JoinerRequest) {
        let core_session_id = SessionId::new(session.as_str().to_string());
        // Adapter's Branch A doesn't consult this; SpaceId still gets
        // echoed onto the wire so joiner + persistence see a stable id.
        let space_id = SpaceId::new();

        let event = SpaceAccessEvent::SponsorAuthorizationRequested {
            pairing_session_id: core_session_id.clone(),
            space_id: space_id.clone(),
            // Slice 1 P7f: no TTL wired yet — StartTimer action is a
            // no-op in our dispatcher. P7g will plumb a TimerPort here.
            ttl_secs: 0,
        };
        let (next_state, actions) =
            SpaceAccessStateMachine::transition(SpaceAccessState::Idle, event);

        let mut ctx = SessionCtx {
            state: next_state,
            core_session_id,
            space_id,
            prepared_offer: None,
            challenge_nonce: None,
            result_success: None,
            result_deny_reason: None,
            joiner_device_id: request.device_id,
            joiner_device_name: request.device_name,
            joiner_fingerprint: request.identity_fingerprint,
        };

        if let Err(reason) = self.execute_actions(&session, &mut ctx, actions).await {
            warn!(
                session = %session,
                error = %reason,
                "action execution failed during begin_handshake"
            );
            self.reject_and_close(&session, PairingRejectReason::Internal(reason))
                .await;
            return;
        }

        if matches!(ctx.state, SpaceAccessState::WaitingJoinerProof { .. }) {
            self.sessions.lock().await.insert(session.clone(), ctx);
            debug!(session = %session, "KeyslotOffer sent; awaiting ChallengeResponse");
        } else {
            // FSM ended up somewhere terminal (Cancelled, etc.) before we
            // could park state — nothing to remember.
            debug!(
                session = %session,
                state = ?ctx.state,
                "FSM terminal after begin_handshake; no ctx parked"
            );
        }
    }

    async fn handle_message_received(
        &self,
        session: PairingSessionId,
        message: PairingSessionMessage,
    ) {
        match message {
            PairingSessionMessage::ChallengeResponse(response) => {
                self.handle_challenge_response(session, response).await
            }
            other => {
                // `KeyslotOffer` is sponsor→joiner; `Request` is only a
                // valid first frame; `Confirm`/`Reject` should never flow
                // joiner→sponsor. Drop with a warn — session naturally
                // terminates via the joiner's own Reject or its close.
                warn!(
                    session = %session,
                    variant = variant_name(&other),
                    "unexpected mid-handshake message from joiner"
                );
            }
        }
    }

    async fn handle_challenge_response(
        &self,
        session: PairingSessionId,
        response: JoinerChallengeResponse,
    ) {
        let mut ctx = match self.sessions.lock().await.remove(&session) {
            Some(c) => c,
            None => {
                warn!(
                    session = %session,
                    "ChallengeResponse arrived with no parked sponsor state; ignoring"
                );
                return;
            }
        };

        let challenge_nonce = match ctx.challenge_nonce {
            Some(n) => n,
            None => {
                // Shouldn't happen — RequestOfferPreparation always fills
                // `challenge_nonce`. Defensive guard so any future FSM
                // reshuffle fails loud.
                warn!(
                    session = %session,
                    "parked ctx missing challenge_nonce; rejecting"
                );
                self.reject_and_close(
                    &session,
                    PairingRejectReason::Internal("missing challenge nonce".into()),
                )
                .await;
                return;
            }
        };

        let artifact = SpaceAccessProofArtifact {
            pairing_session_id: ctx.core_session_id.clone(),
            space_id: ctx.space_id.clone(),
            challenge_nonce,
            proof_bytes: response.encrypted_challenge,
        };
        let verified = match self
            .proof_port
            .verify_proof(&artifact, challenge_nonce)
            .await
        {
            Ok(v) => v,
            Err(err) => {
                warn!(
                    session = %session,
                    error = %err,
                    "proof verification errored; treating as invalid proof"
                );
                false
            }
        };

        let event = if verified {
            ctx.result_success = Some(true);
            ctx.result_deny_reason = None;
            SpaceAccessEvent::ProofVerified {
                pairing_session_id: ctx.core_session_id.clone(),
                space_id: ctx.space_id.clone(),
            }
        } else {
            ctx.result_success = Some(false);
            ctx.result_deny_reason = Some(DenyReason::InvalidProof);
            SpaceAccessEvent::ProofRejected {
                pairing_session_id: ctx.core_session_id.clone(),
                space_id: ctx.space_id.clone(),
                reason: DenyReason::InvalidProof,
            }
        };

        let prior_state = ctx.state.clone();
        let (next_state, actions) = SpaceAccessStateMachine::transition(prior_state, event);
        ctx.state = next_state;

        if let Err(reason) = self.execute_actions(&session, &mut ctx, actions).await {
            warn!(
                session = %session,
                error = %reason,
                "action execution failed during handle_challenge_response"
            );
            // Session is already terminal; close + exit. No Reject wire —
            // if `SendResult` failed partway the joiner may or may not
            // have received Confirm/Reject; we can't cleanly retransmit.
        }

        // Either way the handshake is done: the FSM is in Granted /
        // Denied / Cancelled. Close the transport.
        self.pairing_session
            .close(
                &session,
                Some(format!("handshake terminal: {:?}", ctx.state)),
            )
            .await;
    }

    async fn handle_closed(&self, session: PairingSessionId, reason: Option<String>) {
        // Best-effort: if we have live ctx, feed `SessionClosed` through
        // the FSM so the state transition is observable in logs. We
        // don't care about the emitted actions (StopTimer only) here.
        let ctx = self.sessions.lock().await.remove(&session);
        if let Some(mut ctx) = ctx {
            let prior = ctx.state.clone();
            let (next_state, _actions) =
                SpaceAccessStateMachine::transition(prior, SpaceAccessEvent::SessionClosed);
            ctx.state = next_state;
            debug!(
                session = %session,
                reason = ?reason,
                terminal_state = ?ctx.state,
                "session closed; ctx dropped"
            );
        } else {
            debug!(session = %session, reason = ?reason, "session closed; no parked ctx");
        }
    }

    /// Slice-1-local action dispatcher. Handles exactly the action set
    /// the sponsor-side FSM paths produce; joiner-side actions are
    /// logged as unexpected (should not occur in this orchestrator).
    async fn execute_actions(
        &self,
        session: &PairingSessionId,
        ctx: &mut SessionCtx,
        actions: Vec<SpaceAccessAction>,
    ) -> Result<(), String> {
        for action in actions {
            match action {
                SpaceAccessAction::RequestOfferPreparation { space_id, .. } => {
                    let offer = self
                        .space_access
                        .prepare_join_offer(
                            &space_id,
                            &uc_core::crypto::domain::Passphrase::new(""),
                        )
                        .await
                        .map_err(|e| format!("prepare_join_offer: {e}"))?;
                    ctx.challenge_nonce = Some(offer.challenge_nonce);
                    // Preserve the adapter-authoritative space_id — the
                    // FSM carries a separate copy but the offer's is the
                    // one we echo on the wire and persist.
                    ctx.space_id = offer.space_id.clone();
                    ctx.prepared_offer = Some(offer);
                }
                SpaceAccessAction::SendOffer => {
                    let offer = ctx
                        .prepared_offer
                        .take()
                        .ok_or_else(|| "SendOffer without prepared_offer".to_string())?;
                    let keyslot = PairingSessionMessage::KeyslotOffer(SponsorKeyslotOffer {
                        space_id: offer.space_id,
                        keyslot_blob: offer.keyslot_blob,
                        challenge: offer.challenge_nonce.to_vec(),
                        pairing_session_id: session.clone(),
                    });
                    self.pairing_session
                        .send(session, keyslot)
                        .await
                        .map_err(|e| format!("send KeyslotOffer: {e}"))?;
                }
                SpaceAccessAction::SendResult => match ctx.result_success {
                    Some(true) => {
                        let sender_device_name = self.resolve_device_name().await?;
                        let sender_identity_fingerprint = self
                            .local_identity
                            .ensure()
                            .await
                            .map_err(|e| format!("local_identity.ensure: {e}"))?;
                        let confirm = PairingSessionMessage::Confirm(SponsorConfirm {
                            space_id: ctx.space_id.clone(),
                            sender_device_id: self.device_identity.current_device_id(),
                            sender_device_name,
                            sender_identity_fingerprint,
                        });
                        self.pairing_session
                            .send(session, confirm)
                            .await
                            .map_err(|e| format!("send Confirm: {e}"))?;
                    }
                    Some(false) => {
                        let reject = PairingSessionMessage::Reject(PairingReject {
                            reason: PairingRejectReason::PassphraseMismatch,
                        });
                        self.pairing_session
                            .send(session, reject)
                            .await
                            .map_err(|e| format!("send Reject: {e}"))?;
                    }
                    None => {
                        return Err("SendResult without result_success".to_string());
                    }
                },
                SpaceAccessAction::PersistSponsorAccess { space_id: _ } => {
                    // FSM passes the space_id, but ctx.space_id is the
                    // adapter-authoritative one (set in RequestOfferPreparation).
                    self.persist_peer(ctx).await?;
                }
                SpaceAccessAction::StartTimer { .. } | SpaceAccessAction::StopTimer => {
                    // Slice 1 P7f: no TTL wired yet. P7g introduces a
                    // TimerPort + cancel tokens.
                    debug!(session = %session, ?action, "timer action ignored (Slice 1 P7f)");
                }
                // Joiner-side intents — must not surface on a sponsor
                // path. Log loudly so any FSM drift is caught.
                SpaceAccessAction::RequestSpaceKeyDerivation { .. }
                | SpaceAccessAction::SendProof
                | SpaceAccessAction::PersistJoinerAccess { .. } => {
                    warn!(
                        session = %session,
                        ?action,
                        "joiner-side action surfaced on sponsor path; ignored"
                    );
                }
            }
        }
        Ok(())
    }

    async fn persist_peer(&self, ctx: &SessionCtx) -> Result<(), String> {
        let now_ms = self.clock.now_ms();
        let now = Utc
            .timestamp_millis_opt(now_ms)
            .single()
            .ok_or_else(|| format!("clock out of range: {now_ms}"))?;

        let member = SpaceMember {
            device_id: ctx.joiner_device_id.clone(),
            device_name: ctx.joiner_device_name.clone(),
            identity_fingerprint: ctx.joiner_fingerprint.clone(),
            joined_at: now,
            sync_preferences: MemberSyncPreferences::default(),
        };
        self.member_repo
            .save(&member)
            .await
            .map_err(|e| format!("member_repo.save: {e}"))?;

        let trusted = TrustedPeer {
            local_device_id: self.device_identity.current_device_id(),
            peer_device_id: ctx.joiner_device_id.clone(),
            peer_fingerprint: ctx.joiner_fingerprint.clone(),
            trusted_at: now,
        };
        self.trusted_peer_repo
            .save(&trusted)
            .await
            .map_err(|e| format!("trusted_peer_repo.save: {e}"))?;
        Ok(())
    }

    async fn resolve_device_name(&self) -> Result<String, String> {
        self.settings
            .load()
            .await
            .map_err(|e| format!("settings.load: {e}"))?
            .general
            .device_name
            .filter(|n| !n.trim().is_empty())
            .ok_or_else(|| "device_name missing from settings".to_string())
    }

    async fn reject_and_close(&self, session: &PairingSessionId, reason: PairingRejectReason) {
        // Defensive: release any parked ctx from a partial handshake.
        self.sessions.lock().await.remove(session);

        let reject = PairingSessionMessage::Reject(PairingReject {
            reason: reason.clone(),
        });
        if let Err(err) = self.pairing_session.send(session, reject).await {
            warn!(
                session = %session,
                error = %err,
                "failed to deliver PairingReject to joiner; closing session anyway"
            );
        }
        self.pairing_session
            .close(session, Some(format!("reject: {:?}", reason)))
            .await;
    }

    async fn notify_rendezvous_consume(&self, code: &InvitationCode) {
        match self.pairing_invitation.consume_invitation(code).await {
            Ok(()) => debug!(code = %code.as_str(), "rendezvous consume acknowledged"),
            Err(ConsumeInvitationError::NotFound) => debug!(
                code = %code.as_str(),
                "rendezvous entry already gone on consume (benign)"
            ),
            Err(ConsumeInvitationError::Expired) => debug!(
                code = %code.as_str(),
                "rendezvous entry expired on consume (benign)"
            ),
            Err(err) => warn!(
                code = %code.as_str(),
                error = %err,
                "rendezvous consume failed; local handshake proceeds regardless"
            ),
        }
    }

    #[cfg(test)]
    async fn sessions_len(&self) -> usize {
        self.sessions.lock().await.len()
    }
}

fn event_kind(event: &PairingSessionEvent) -> &'static str {
    match event {
        PairingSessionEvent::Incoming { .. } => "Incoming",
        PairingSessionEvent::MessageReceived { .. } => "MessageReceived",
        PairingSessionEvent::Closed { .. } => "Closed",
    }
}

fn variant_name(message: &PairingSessionMessage) -> &'static str {
    match message {
        PairingSessionMessage::Request(_) => "Request",
        PairingSessionMessage::KeyslotOffer(_) => "KeyslotOffer",
        PairingSessionMessage::ChallengeResponse(_) => "ChallengeResponse",
        PairingSessionMessage::Confirm(_) => "Confirm",
        PairingSessionMessage::Reject(_) => "Reject",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Mutex as StdMutex;

    use async_trait::async_trait;
    use chrono::{DateTime, Duration};
    use tokio::sync::mpsc;

    use uc_core::crypto::domain::{ActiveSpace, Passphrase};
    use uc_core::ids::DeviceId;
    use uc_core::membership::MembershipError;
    use uc_core::pairing::invitation::PairingInvitation;
    use uc_core::ports::pairing::{DialError, SessionError};
    use uc_core::ports::pairing_invitation::{InvitationError, IssuedInvitation};
    use uc_core::ports::space::SpaceAccessError;
    use uc_core::ports::LocalIdentityError;
    use uc_core::settings::model::Settings;
    use uc_core::space_access::domain::ProofDerivedKey;
    use uc_core::trusted_peer::TrustedPeerError;

    // ── fakes ────────────────────────────────────────────────────────────

    struct FakeClock(i64);
    impl ClockPort for FakeClock {
        fn now_ms(&self) -> i64 {
            self.0
        }
    }

    #[derive(Default)]
    struct RecordingSessionPort {
        sent: StdMutex<Vec<(PairingSessionId, PairingSessionMessage)>>,
        closed: StdMutex<Vec<(PairingSessionId, Option<String>)>>,
        fail_send_from: StdMutex<Option<usize>>,
    }

    impl RecordingSessionPort {
        fn sent(&self) -> Vec<(PairingSessionId, PairingSessionMessage)> {
            self.sent.lock().unwrap().clone()
        }
        fn closed(&self) -> Vec<(PairingSessionId, Option<String>)> {
            self.closed.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl PairingSessionPort for RecordingSessionPort {
        async fn dial_by_invitation(
            &self,
            _code: &InvitationCode,
        ) -> Result<PairingSessionId, DialError> {
            unimplemented!("sponsor orchestrator does not dial")
        }
        async fn send(
            &self,
            session: &PairingSessionId,
            message: PairingSessionMessage,
        ) -> Result<(), SessionError> {
            let should_fail = {
                let mut guard = self.fail_send_from.lock().unwrap();
                if let Some(n) = *guard {
                    if n == 0 {
                        true
                    } else {
                        *guard = Some(n - 1);
                        false
                    }
                } else {
                    false
                }
            };
            if should_fail {
                return Err(SessionError::Closed);
            }
            self.sent.lock().unwrap().push((session.clone(), message));
            Ok(())
        }
        async fn recv_next(
            &self,
            _session: &PairingSessionId,
        ) -> Result<Option<PairingSessionMessage>, SessionError> {
            unimplemented!("sponsor orchestrator does not poll recv directly")
        }
        async fn close(&self, session: &PairingSessionId, reason: Option<String>) {
            self.closed.lock().unwrap().push((session.clone(), reason));
        }
    }

    struct ScriptedEventPort {
        inner: StdMutex<Option<Receiver<PairingSessionEvent>>>,
    }

    impl ScriptedEventPort {
        fn new(rx: Receiver<PairingSessionEvent>) -> Self {
            Self {
                inner: StdMutex::new(Some(rx)),
            }
        }
    }

    #[async_trait]
    impl PairingEventPort for ScriptedEventPort {
        async fn subscribe(&self) -> anyhow::Result<Receiver<PairingSessionEvent>> {
            self.inner
                .lock()
                .unwrap()
                .take()
                .ok_or_else(|| anyhow::anyhow!("ScriptedEventPort already subscribed"))
        }
    }

    #[derive(Default)]
    struct RecordingInvitationPort {
        consumed: StdMutex<Vec<InvitationCode>>,
    }

    impl RecordingInvitationPort {
        fn consumed(&self) -> Vec<InvitationCode> {
            self.consumed.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl PairingInvitationPort for RecordingInvitationPort {
        async fn issue_invitation(&self) -> Result<IssuedInvitation, InvitationError> {
            unimplemented!("sponsor orchestrator never issues invitations")
        }
        async fn consume_invitation(
            &self,
            code: &InvitationCode,
        ) -> Result<(), ConsumeInvitationError> {
            self.consumed.lock().unwrap().push(code.clone());
            Ok(())
        }
    }

    struct StubSpaceAccess {
        space_id: SpaceId,
        challenge_nonce: [u8; 32],
        fail_prepare: StdMutex<bool>,
    }

    impl StubSpaceAccess {
        fn new(space_id: SpaceId, challenge_nonce: [u8; 32]) -> Self {
            Self {
                space_id,
                challenge_nonce,
                fail_prepare: StdMutex::new(false),
            }
        }
    }

    #[async_trait]
    impl SpaceAccessPort for StubSpaceAccess {
        async fn initialize(
            &self,
            _space_id: &SpaceId,
            _passphrase: &Passphrase,
        ) -> Result<ActiveSpace, SpaceAccessError> {
            unimplemented!()
        }
        async fn unlock(
            &self,
            _space_id: &SpaceId,
            _passphrase: &Passphrase,
        ) -> Result<ActiveSpace, SpaceAccessError> {
            unimplemented!()
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
            if *self.fail_prepare.lock().unwrap() {
                return Err(SpaceAccessError::Internal("prepare boom".into()));
            }
            Ok(JoinOffer {
                space_id: self.space_id.clone(),
                keyslot_blob: vec![0xAA; 32],
                challenge_nonce: self.challenge_nonce,
            })
        }
        async fn derive_master_key_for_proof(
            &self,
            _offer: &JoinOffer,
            _passphrase: &Passphrase,
        ) -> Result<ProofDerivedKey, SpaceAccessError> {
            unimplemented!()
        }
    }

    struct ScriptedProofPort {
        verdicts: StdMutex<Vec<anyhow::Result<bool>>>,
        observed: StdMutex<Vec<SpaceAccessProofArtifact>>,
    }

    impl ScriptedProofPort {
        fn always(verdict: bool) -> Self {
            Self {
                verdicts: StdMutex::new(vec![Ok(verdict)]),
                observed: StdMutex::new(Vec::new()),
            }
        }
        fn failing(msg: &str) -> Self {
            Self {
                verdicts: StdMutex::new(vec![Err(anyhow::anyhow!(msg.to_string()))]),
                observed: StdMutex::new(Vec::new()),
            }
        }
        fn observed(&self) -> Vec<SpaceAccessProofArtifact> {
            self.observed.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl ProofPort for ScriptedProofPort {
        async fn build_proof(
            &self,
            _pairing_session_id: &SessionId,
            _space_id: &SpaceId,
            _challenge_nonce: [u8; 32],
            _derived_key: &ProofDerivedKey,
        ) -> anyhow::Result<SpaceAccessProofArtifact> {
            unimplemented!("sponsor never builds")
        }
        async fn verify_proof(
            &self,
            proof: &SpaceAccessProofArtifact,
            _expected_nonce: [u8; 32],
        ) -> anyhow::Result<bool> {
            self.observed.lock().unwrap().push(proof.clone());
            let mut v = self.verdicts.lock().unwrap();
            if v.is_empty() {
                return Ok(false);
            }
            match v.remove(0) {
                Ok(b) => Ok(b),
                Err(e) => Err(e),
            }
        }
    }

    #[derive(Default)]
    struct RecordingMemberRepo {
        saved: StdMutex<Vec<SpaceMember>>,
        fail_with: StdMutex<Option<MembershipError>>,
    }

    #[async_trait]
    impl MemberRepositoryPort for RecordingMemberRepo {
        async fn get(&self, _device_id: &DeviceId) -> Result<Option<SpaceMember>, MembershipError> {
            Ok(None)
        }
        async fn list(&self) -> Result<Vec<SpaceMember>, MembershipError> {
            Ok(self.saved.lock().unwrap().clone())
        }
        async fn save(&self, member: &SpaceMember) -> Result<(), MembershipError> {
            if let Some(err) = self.fail_with.lock().unwrap().take() {
                return Err(err);
            }
            self.saved.lock().unwrap().push(member.clone());
            Ok(())
        }
        async fn remove(&self, _device_id: &DeviceId) -> Result<bool, MembershipError> {
            Ok(false)
        }
    }

    #[derive(Default)]
    struct RecordingTrustedPeerRepo {
        saved: StdMutex<Vec<TrustedPeer>>,
    }

    #[async_trait]
    impl TrustedPeerRepositoryPort for RecordingTrustedPeerRepo {
        async fn get(&self, _: &DeviceId) -> Result<Option<TrustedPeer>, TrustedPeerError> {
            Ok(None)
        }
        async fn list(&self) -> Result<Vec<TrustedPeer>, TrustedPeerError> {
            Ok(self.saved.lock().unwrap().clone())
        }
        async fn save(&self, trusted_peer: &TrustedPeer) -> Result<(), TrustedPeerError> {
            self.saved.lock().unwrap().push(trusted_peer.clone());
            Ok(())
        }
        async fn remove(&self, _: &DeviceId) -> Result<bool, TrustedPeerError> {
            Ok(false)
        }
    }

    struct FixedLocalIdentity(IdentityFingerprint);
    #[async_trait]
    impl LocalIdentityPort for FixedLocalIdentity {
        async fn create(&self) -> Result<IdentityFingerprint, LocalIdentityError> {
            Ok(self.0.clone())
        }
        async fn ensure(&self) -> Result<IdentityFingerprint, LocalIdentityError> {
            Ok(self.0.clone())
        }
        async fn get_current_fingerprint(
            &self,
        ) -> Result<Option<IdentityFingerprint>, LocalIdentityError> {
            Ok(Some(self.0.clone()))
        }
    }

    struct FixedDeviceIdentity(DeviceId);
    impl DeviceIdentityPort for FixedDeviceIdentity {
        fn current_device_id(&self) -> DeviceId {
            self.0.clone()
        }
    }

    struct InMemorySettings(StdMutex<Settings>);
    impl InMemorySettings {
        fn with_device_name(name: &str) -> Self {
            let mut s = Settings::default();
            s.general.device_name = Some(name.to_string());
            Self(StdMutex::new(s))
        }
        fn empty() -> Self {
            Self(StdMutex::new(Settings::default()))
        }
    }
    #[async_trait]
    impl SettingsPort for InMemorySettings {
        async fn load(&self) -> anyhow::Result<Settings> {
            Ok(self.0.lock().unwrap().clone())
        }
        async fn save(&self, s: &Settings) -> anyhow::Result<()> {
            *self.0.lock().unwrap() = s.clone();
            Ok(())
        }
    }

    // ── helpers ──────────────────────────────────────────────────────────

    fn fixed_now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-04-20T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn fixed_now_ms() -> i64 {
        fixed_now().timestamp_millis()
    }

    fn pending(code: &str) -> PairingInvitation {
        let issued = fixed_now();
        let expires = issued + Duration::minutes(5);
        let (invitation, _) = PairingInvitation::issue(
            InvitationCode::new(code),
            issued,
            expires,
            DeviceId::new("sponsor-1"),
        );
        invitation
    }

    fn joiner_fingerprint() -> IdentityFingerprint {
        IdentityFingerprint::from_raw_string("AAAAAAAAAAAAAAAA").unwrap()
    }

    fn sponsor_fingerprint() -> IdentityFingerprint {
        IdentityFingerprint::from_raw_string("BBBBBBBBBBBBBBBB").unwrap()
    }

    fn joiner_request(code: &str) -> JoinerRequest {
        JoinerRequest {
            invitation_code: InvitationCode::new(code),
            device_id: DeviceId::new("joiner-device"),
            device_name: "joiner's laptop".into(),
            identity_fingerprint: joiner_fingerprint(),
            nonce: vec![1, 2, 3, 4],
        }
    }

    struct Bundle {
        events: Arc<ScriptedEventPort>,
        session_port: Arc<RecordingSessionPort>,
        invitation_port: Arc<RecordingInvitationPort>,
        holder: Arc<InMemoryPairingInvitationHolder>,
        space_access: Arc<StubSpaceAccess>,
        proof_port: Arc<ScriptedProofPort>,
        member_repo: Arc<RecordingMemberRepo>,
        trusted_peer_repo: Arc<RecordingTrustedPeerRepo>,
        local_identity: Arc<FixedLocalIdentity>,
        device_identity: Arc<FixedDeviceIdentity>,
        settings: Arc<InMemorySettings>,
        clock_ms: i64,
    }

    impl Bundle {
        fn happy() -> (Self, mpsc::Sender<PairingSessionEvent>) {
            let (tx, rx) = mpsc::channel(16);
            let b = Self {
                events: Arc::new(ScriptedEventPort::new(rx)),
                session_port: Arc::new(RecordingSessionPort::default()),
                invitation_port: Arc::new(RecordingInvitationPort::default()),
                holder: Arc::new(InMemoryPairingInvitationHolder::new()),
                space_access: Arc::new(StubSpaceAccess::new(
                    SpaceId::from_str("space-xyz"),
                    [0x42; 32],
                )),
                proof_port: Arc::new(ScriptedProofPort::always(true)),
                member_repo: Arc::new(RecordingMemberRepo::default()),
                trusted_peer_repo: Arc::new(RecordingTrustedPeerRepo::default()),
                local_identity: Arc::new(FixedLocalIdentity(sponsor_fingerprint())),
                device_identity: Arc::new(FixedDeviceIdentity(DeviceId::new("sponsor-device"))),
                settings: Arc::new(InMemorySettings::with_device_name("sponsor-mac")),
                clock_ms: fixed_now_ms(),
            };
            (b, tx)
        }

        fn build(self) -> Arc<PairingInboundOrchestrator> {
            Arc::new(PairingInboundOrchestrator::new(
                self.events,
                self.session_port,
                self.invitation_port,
                self.holder,
                Arc::new(FakeClock(self.clock_ms)) as Arc<dyn ClockPort>,
                self.space_access,
                self.proof_port,
                self.member_repo,
                self.trusted_peer_repo,
                self.local_identity,
                self.device_identity,
                self.settings,
            ))
        }
    }

    // ── P7e regression ────────────────────────────────────────────────────

    #[tokio::test]
    async fn incoming_with_unknown_code_rejects_and_closes_session() {
        let (b, _tx) = Bundle::happy();
        b.holder.insert(pending("EXPECTED")).await;
        let session_port = b.session_port.clone();
        let invitation_port = b.invitation_port.clone();
        let holder = b.holder.clone();
        let orch = b.build();

        let session = PairingSessionId::new("sess-unknown");
        orch.handle_event(PairingSessionEvent::Incoming {
            session: session.clone(),
            message: PairingSessionMessage::Request(joiner_request("WRONG")),
        })
        .await;

        let sent = session_port.sent();
        assert_eq!(sent.len(), 1);
        match &sent[0].1 {
            PairingSessionMessage::Reject(r) => {
                assert_eq!(r.reason, PairingRejectReason::InvitationMismatch)
            }
            other => panic!("expected Reject, got {:?}", other),
        }
        assert_eq!(session_port.closed().len(), 1);
        assert!(invitation_port.consumed().is_empty());
        assert_eq!(holder.len().await, 1);
        assert_eq!(orch.sessions_len().await, 0);
    }

    #[tokio::test]
    async fn incoming_with_expired_invitation_rejects_and_drops_slot() {
        let (mut b, _tx) = Bundle::happy();
        b.holder.insert(pending("STALE")).await;
        b.clock_ms = (fixed_now() + Duration::minutes(10)).timestamp_millis();
        let session_port = b.session_port.clone();
        let holder = b.holder.clone();
        let orch = b.build();

        orch.handle_event(PairingSessionEvent::Incoming {
            session: PairingSessionId::new("sess-stale"),
            message: PairingSessionMessage::Request(joiner_request("STALE")),
        })
        .await;

        let sent = session_port.sent();
        assert_eq!(sent.len(), 1);
        assert!(matches!(
            sent[0].1,
            PairingSessionMessage::Reject(ref r) if r.reason == PairingRejectReason::InvitationMismatch
        ));
        assert_eq!(session_port.closed().len(), 1);
        assert_eq!(holder.len().await, 0);
    }

    #[tokio::test]
    async fn incoming_with_non_request_first_message_is_rejected() {
        let (b, _tx) = Bundle::happy();
        let session_port = b.session_port.clone();
        let orch = b.build();

        let bad = PairingSessionMessage::ChallengeResponse(JoinerChallengeResponse {
            encrypted_challenge: vec![0xDE, 0xAD],
        });
        orch.handle_event(PairingSessionEvent::Incoming {
            session: PairingSessionId::new("sess-bad-first"),
            message: bad,
        })
        .await;

        let sent = session_port.sent();
        assert_eq!(sent.len(), 1);
        match &sent[0].1 {
            PairingSessionMessage::Reject(r) => match &r.reason {
                PairingRejectReason::Internal(msg) => {
                    assert!(msg.contains("Request"), "reason was {msg}")
                }
                other => panic!("expected Internal, got {other:?}"),
            },
            other => panic!("expected Reject, got {other:?}"),
        }
        assert_eq!(session_port.closed().len(), 1);
    }

    // ── P7f: KeyslotOffer on match ───────────────────────────────────────

    #[tokio::test]
    async fn matching_invitation_sends_keyslot_offer_and_parks_ctx() {
        let (b, _tx) = Bundle::happy();
        b.holder.insert(pending("LIVE")).await;
        let session_port = b.session_port.clone();
        let invitation_port = b.invitation_port.clone();
        let orch = b.build();

        let session = PairingSessionId::new("sess-live");
        orch.handle_event(PairingSessionEvent::Incoming {
            session: session.clone(),
            message: PairingSessionMessage::Request(joiner_request("LIVE")),
        })
        .await;

        assert_eq!(
            invitation_port.consumed(),
            vec![InvitationCode::new("LIVE")]
        );
        assert_eq!(orch.sessions_len().await, 1);

        let sent = session_port.sent();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, session);
        match &sent[0].1 {
            PairingSessionMessage::KeyslotOffer(offer) => {
                assert_eq!(offer.space_id.inner(), "space-xyz");
                assert_eq!(offer.keyslot_blob, vec![0xAA; 32]);
                assert_eq!(offer.challenge, vec![0x42; 32]);
                assert_eq!(offer.pairing_session_id, session);
            }
            other => panic!("expected KeyslotOffer, got {other:?}"),
        }
        assert!(session_port.closed().is_empty());
    }

    #[tokio::test]
    async fn prepare_join_offer_failure_rejects_and_drops_ctx() {
        let (b, _tx) = Bundle::happy();
        b.holder.insert(pending("PF")).await;
        *b.space_access.fail_prepare.lock().unwrap() = true;
        let session_port = b.session_port.clone();
        let orch = b.build();

        orch.handle_event(PairingSessionEvent::Incoming {
            session: PairingSessionId::new("sess-pf"),
            message: PairingSessionMessage::Request(joiner_request("PF")),
        })
        .await;

        let sent = session_port.sent();
        assert_eq!(sent.len(), 1);
        match &sent[0].1 {
            PairingSessionMessage::Reject(r) => match &r.reason {
                PairingRejectReason::Internal(msg) => {
                    assert!(msg.contains("prepare_join_offer"), "reason was {msg}")
                }
                other => panic!("expected Internal, got {other:?}"),
            },
            other => panic!("expected Reject, got {other:?}"),
        }
        assert_eq!(session_port.closed().len(), 1);
        assert_eq!(orch.sessions_len().await, 0);
    }

    #[tokio::test]
    async fn keyslot_offer_send_failure_aborts_and_drops_ctx() {
        let (b, _tx) = Bundle::happy();
        b.holder.insert(pending("SF")).await;
        *b.session_port.fail_send_from.lock().unwrap() = Some(0);
        let session_port = b.session_port.clone();
        let orch = b.build();

        orch.handle_event(PairingSessionEvent::Incoming {
            session: PairingSessionId::new("sess-sf"),
            message: PairingSessionMessage::Request(joiner_request("SF")),
        })
        .await;

        // KeyslotOffer send fails → reject_and_close path (Reject send also
        // fails because fail_send_from=Some(0) still matches). Either way,
        // close happens and no ctx remains.
        assert_eq!(session_port.closed().len(), 1);
        assert_eq!(orch.sessions_len().await, 0);
    }

    // ── P7f: ChallengeResponse ───────────────────────────────────────────

    #[tokio::test]
    async fn verified_challenge_persists_sends_confirm_and_closes() {
        let (b, _tx) = Bundle::happy();
        b.holder.insert(pending("OK")).await;
        let session_port = b.session_port.clone();
        let member_repo = b.member_repo.clone();
        let trusted_peer_repo = b.trusted_peer_repo.clone();
        let proof_port = b.proof_port.clone();
        let orch = b.build();

        let session = PairingSessionId::new("sess-ok");
        orch.handle_event(PairingSessionEvent::Incoming {
            session: session.clone(),
            message: PairingSessionMessage::Request(joiner_request("OK")),
        })
        .await;
        assert_eq!(session_port.sent().len(), 1);

        orch.handle_event(PairingSessionEvent::MessageReceived {
            session: session.clone(),
            message: PairingSessionMessage::ChallengeResponse(JoinerChallengeResponse {
                encrypted_challenge: vec![0x11, 0x22, 0x33],
            }),
        })
        .await;

        let observed = proof_port.observed();
        assert_eq!(observed.len(), 1);
        assert_eq!(observed[0].pairing_session_id.as_str(), session.as_str());
        assert_eq!(observed[0].space_id.inner(), "space-xyz");
        assert_eq!(observed[0].challenge_nonce, [0x42; 32]);
        assert_eq!(observed[0].proof_bytes, vec![0x11, 0x22, 0x33]);

        let members = member_repo.saved.lock().unwrap().clone();
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].device_id.as_str(), "joiner-device");
        assert_eq!(members[0].device_name, "joiner's laptop");
        assert_eq!(members[0].identity_fingerprint, joiner_fingerprint());
        assert_eq!(members[0].joined_at, fixed_now());

        let trusted = trusted_peer_repo.saved.lock().unwrap().clone();
        assert_eq!(trusted.len(), 1);
        assert_eq!(trusted[0].local_device_id.as_str(), "sponsor-device");
        assert_eq!(trusted[0].peer_device_id.as_str(), "joiner-device");
        assert_eq!(trusted[0].peer_fingerprint, joiner_fingerprint());

        let sent = session_port.sent();
        assert_eq!(sent.len(), 2);
        match &sent[1].1 {
            PairingSessionMessage::Confirm(c) => {
                assert_eq!(c.space_id.inner(), "space-xyz");
                assert_eq!(c.sender_device_id.as_str(), "sponsor-device");
                assert_eq!(c.sender_device_name, "sponsor-mac");
                assert_eq!(c.sender_identity_fingerprint, sponsor_fingerprint());
            }
            other => panic!("expected Confirm, got {other:?}"),
        }
        assert_eq!(session_port.closed().len(), 1);
        assert_eq!(orch.sessions_len().await, 0);
    }

    #[tokio::test]
    async fn unverified_challenge_sends_passphrase_mismatch_reject() {
        let (mut b, _tx) = Bundle::happy();
        b.holder.insert(pending("BAD")).await;
        b.proof_port = Arc::new(ScriptedProofPort::always(false));
        let session_port = b.session_port.clone();
        let member_repo = b.member_repo.clone();
        let orch = b.build();

        let session = PairingSessionId::new("sess-bad");
        orch.handle_event(PairingSessionEvent::Incoming {
            session: session.clone(),
            message: PairingSessionMessage::Request(joiner_request("BAD")),
        })
        .await;
        orch.handle_event(PairingSessionEvent::MessageReceived {
            session: session.clone(),
            message: PairingSessionMessage::ChallengeResponse(JoinerChallengeResponse {
                encrypted_challenge: vec![0xFF],
            }),
        })
        .await;

        let sent = session_port.sent();
        assert_eq!(sent.len(), 2, "KeyslotOffer + Reject");
        match &sent[1].1 {
            PairingSessionMessage::Reject(r) => {
                assert_eq!(r.reason, PairingRejectReason::PassphraseMismatch)
            }
            other => panic!("expected Reject, got {other:?}"),
        }
        assert!(member_repo.saved.lock().unwrap().is_empty());
        assert_eq!(session_port.closed().len(), 1);
        assert_eq!(orch.sessions_len().await, 0);
    }

    #[tokio::test]
    async fn proof_port_error_is_treated_as_mismatch() {
        let (mut b, _tx) = Bundle::happy();
        b.holder.insert(pending("ERR")).await;
        b.proof_port = Arc::new(ScriptedProofPort::failing("transient"));
        let session_port = b.session_port.clone();
        let orch = b.build();

        let session = PairingSessionId::new("sess-err");
        orch.handle_event(PairingSessionEvent::Incoming {
            session: session.clone(),
            message: PairingSessionMessage::Request(joiner_request("ERR")),
        })
        .await;
        orch.handle_event(PairingSessionEvent::MessageReceived {
            session: session.clone(),
            message: PairingSessionMessage::ChallengeResponse(JoinerChallengeResponse {
                encrypted_challenge: vec![0x00],
            }),
        })
        .await;

        let sent = session_port.sent();
        assert_eq!(sent.len(), 2);
        assert!(matches!(
            sent[1].1,
            PairingSessionMessage::Reject(ref r) if r.reason == PairingRejectReason::PassphraseMismatch
        ));
    }

    #[tokio::test]
    async fn member_repo_failure_aborts_before_trusted_peer() {
        let (b, _tx) = Bundle::happy();
        b.holder.insert(pending("MF")).await;
        *b.member_repo.fail_with.lock().unwrap() =
            Some(MembershipError::Repository("db down".into()));
        let session_port = b.session_port.clone();
        let member_repo = b.member_repo.clone();
        let trusted_peer_repo = b.trusted_peer_repo.clone();
        let orch = b.build();

        let session = PairingSessionId::new("sess-mf");
        orch.handle_event(PairingSessionEvent::Incoming {
            session: session.clone(),
            message: PairingSessionMessage::Request(joiner_request("MF")),
        })
        .await;
        orch.handle_event(PairingSessionEvent::MessageReceived {
            session: session.clone(),
            message: PairingSessionMessage::ChallengeResponse(JoinerChallengeResponse {
                encrypted_challenge: vec![],
            }),
        })
        .await;

        // SendResult(Confirm) succeeds before PersistSponsorAccess fails —
        // the action list is [SendResult, PersistSponsorAccess, StopTimer],
        // so Confirm went out but persistence errored afterwards.
        let sent = session_port.sent();
        assert_eq!(sent.len(), 2, "KeyslotOffer + Confirm");
        assert!(matches!(sent[1].1, PairingSessionMessage::Confirm(_)));

        // Neither store landed (member_repo.save errored; trusted_peer never
        // reached).
        assert!(member_repo.saved.lock().unwrap().is_empty());
        assert!(trusted_peer_repo.saved.lock().unwrap().is_empty());
        // Session still closes cleanly at handler end.
        assert_eq!(session_port.closed().len(), 1);
        assert_eq!(orch.sessions_len().await, 0);
    }

    #[tokio::test]
    async fn confirm_missing_device_name_rejects_result_with_internal_error() {
        let (mut b, _tx) = Bundle::happy();
        b.holder.insert(pending("NN")).await;
        b.settings = Arc::new(InMemorySettings::empty());
        let session_port = b.session_port.clone();
        let member_repo = b.member_repo.clone();
        let orch = b.build();

        let session = PairingSessionId::new("sess-nn");
        orch.handle_event(PairingSessionEvent::Incoming {
            session: session.clone(),
            message: PairingSessionMessage::Request(joiner_request("NN")),
        })
        .await;
        orch.handle_event(PairingSessionEvent::MessageReceived {
            session: session.clone(),
            message: PairingSessionMessage::ChallengeResponse(JoinerChallengeResponse {
                encrypted_challenge: vec![],
            }),
        })
        .await;

        // SendResult failed before Confirm went out — only KeyslotOffer
        // is in the wire log. Persistence never attempted.
        assert_eq!(session_port.sent().len(), 1);
        assert!(member_repo.saved.lock().unwrap().is_empty());
        assert_eq!(session_port.closed().len(), 1);
    }

    #[tokio::test]
    async fn challenge_response_without_parked_ctx_is_ignored() {
        let (b, _tx) = Bundle::happy();
        let session_port = b.session_port.clone();
        let member_repo = b.member_repo.clone();
        let orch = b.build();

        orch.handle_event(PairingSessionEvent::MessageReceived {
            session: PairingSessionId::new("sess-ghost"),
            message: PairingSessionMessage::ChallengeResponse(JoinerChallengeResponse {
                encrypted_challenge: vec![],
            }),
        })
        .await;

        assert!(session_port.sent().is_empty());
        assert!(session_port.closed().is_empty());
        assert!(member_repo.saved.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn stray_keyslot_from_joiner_is_logged_not_closed() {
        let (b, _tx) = Bundle::happy();
        b.holder.insert(pending("ST")).await;
        let session_port = b.session_port.clone();
        let orch = b.build();

        let session = PairingSessionId::new("sess-st");
        orch.handle_event(PairingSessionEvent::Incoming {
            session: session.clone(),
            message: PairingSessionMessage::Request(joiner_request("ST")),
        })
        .await;
        assert_eq!(orch.sessions_len().await, 1);

        orch.handle_event(PairingSessionEvent::MessageReceived {
            session: session.clone(),
            message: PairingSessionMessage::KeyslotOffer(SponsorKeyslotOffer {
                space_id: SpaceId::from_str("fake"),
                keyslot_blob: vec![],
                challenge: vec![],
                pairing_session_id: session.clone(),
            }),
        })
        .await;

        assert_eq!(session_port.sent().len(), 1);
        assert!(session_port.closed().is_empty());
        assert_eq!(orch.sessions_len().await, 1);
    }

    #[tokio::test]
    async fn closed_event_clears_parked_ctx() {
        let (b, _tx) = Bundle::happy();
        b.holder.insert(pending("DR")).await;
        let orch = b.build();

        let session = PairingSessionId::new("sess-dr");
        orch.handle_event(PairingSessionEvent::Incoming {
            session: session.clone(),
            message: PairingSessionMessage::Request(joiner_request("DR")),
        })
        .await;
        assert_eq!(orch.sessions_len().await, 1);

        orch.handle_event(PairingSessionEvent::Closed {
            session,
            reason: Some("joiner bailed".into()),
        })
        .await;
        assert_eq!(orch.sessions_len().await, 0);
    }

    #[tokio::test]
    async fn closed_event_on_unknown_session_is_noop() {
        let (b, _tx) = Bundle::happy();
        let orch = b.build();
        orch.handle_event(PairingSessionEvent::Closed {
            session: PairingSessionId::new("never-seen"),
            reason: None,
        })
        .await;
        assert_eq!(orch.sessions_len().await, 0);
    }

    // ── spawn ────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn spawn_drains_events_from_subscription() {
        let (b, tx) = Bundle::happy();
        b.holder.insert(pending("SP")).await;
        let holder = b.holder.clone();
        let invitation_port = b.invitation_port.clone();
        let orch = b.build();

        let handle = Arc::clone(&orch).spawn();

        tx.send(PairingSessionEvent::Incoming {
            session: PairingSessionId::new("sp-1"),
            message: PairingSessionMessage::Request(joiner_request("SP")),
        })
        .await
        .unwrap();
        drop(tx);
        tokio::time::timeout(std::time::Duration::from_secs(2), handle)
            .await
            .expect("spawn task finishes on channel close")
            .expect("spawn task must not panic");

        assert_eq!(holder.len().await, 0);
        assert_eq!(invitation_port.consumed(), vec![InvitationCode::new("SP")]);
    }

    #[tokio::test]
    async fn spawn_exits_when_subscribe_fails() {
        let (b, _tx) = Bundle::happy();
        let _ = b.events.subscribe().await.unwrap();
        let orch = b.build();
        let handle = orch.spawn();
        tokio::time::timeout(std::time::Duration::from_secs(2), handle)
            .await
            .expect("task exits when subscribe fails")
            .expect("task must not panic");
    }
}
