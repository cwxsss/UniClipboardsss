//! Sponsor-side inbound pairing orchestrator.
//!
//! See the module-level doc in [`super`] for the full design rationale.

use std::sync::Arc;

use chrono::{TimeZone, Utc};
use tokio::sync::mpsc::Receiver;
use tokio::task::JoinHandle;
use tracing::{debug, info, instrument, warn};

use uc_core::pairing::invitation::InvitationCode;
use uc_core::pairing::session_message::{
    JoinerRequest, PairingReject, PairingRejectReason, PairingSessionMessage,
};
use uc_core::ports::pairing::{
    PairingEventPort, PairingSessionEvent, PairingSessionId, PairingSessionPort,
};
use uc_core::ports::{ClockPort, ConsumeInvitationError, PairingInvitationPort};

use crate::pairing_invitation::holder::{InMemoryPairingInvitationHolder, TakeMatchingError};

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
}

impl PairingInboundOrchestrator {
    pub(crate) fn new(
        pairing_events: Arc<dyn PairingEventPort>,
        pairing_session: Arc<dyn PairingSessionPort>,
        pairing_invitation: Arc<dyn PairingInvitationPort>,
        holder: Arc<InMemoryPairingInvitationHolder>,
        clock: Arc<dyn ClockPort>,
    ) -> Self {
        Self {
            pairing_events,
            pairing_session,
            pairing_invitation,
            holder,
            clock,
        }
    }

    /// Subscribe to the event port and spawn a long-lived task that drains
    /// incoming events into [`Self::handle_event`]. The returned
    /// [`JoinHandle`] is owned by the facade so it can `abort()` on
    /// shutdown.
    ///
    /// A subscribe failure is logged and the task exits — the caller still
    /// owns the orchestrator and can rebuild + respawn if the port becomes
    /// usable later. We don't retry internally to keep the "one spawn =
    /// one live subscription" invariant clean.
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
                // Follow-up messages belong to the P7f handshake state
                // machine. Logging keeps ops aware that sessions are
                // emitting the expected traffic.
                debug!(session = %session, variant = variant_name(&message), "follow-up pairing message (handler deferred to P7f)");
            }
            PairingSessionEvent::Closed { session, reason } => {
                debug!(session = %session, reason = ?reason, "pairing session closed");
            }
        }
    }

    async fn handle_incoming(&self, session: PairingSessionId, message: PairingSessionMessage) {
        let request = match message {
            PairingSessionMessage::Request(req) => req,
            other => {
                // Any non-Request first message is a protocol violation
                // from the joiner side. Reject explicitly rather than wait
                // for a Request that will never come.
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
        let JoinerRequest {
            invitation_code, ..
        } = &request;

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

        match self.holder.take_matching(invitation_code, now).await {
            Ok(invitation) => {
                info!(
                    session = %session,
                    code = %invitation.code().as_str(),
                    joiner_device_id = %request.device_id.as_str(),
                    "accepted joiner request for pending invitation"
                );
                self.notify_rendezvous_consume(invitation.code()).await;
                // Handshake state (issuing KeyslotOffer etc.) is the next
                // phase's concern. For now the session is accepted and
                // left open; P7f will wire the keyslot/challenge/confirm
                // flow onto this same session id.
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

    async fn reject_and_close(&self, session: &PairingSessionId, reason: PairingRejectReason) {
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

    use uc_core::ids::{DeviceId, SpaceId};
    use uc_core::pairing::invitation::PairingInvitation;
    use uc_core::pairing::session_message::{JoinerChallengeResponse, SponsorKeyslotOffer};
    use uc_core::ports::pairing::{DialError, SessionError};
    use uc_core::ports::pairing_invitation::{InvitationError, IssuedInvitation};
    use uc_core::security::IdentityFingerprint;

    // ── fakes ────────────────────────────────────────────────────────────

    struct FakeClock(i64);
    impl ClockPort for FakeClock {
        fn now_ms(&self) -> i64 {
            self.0
        }
    }

    /// Records every `send` / `close` call so tests can assert side effects
    /// without a real iroh endpoint.
    #[derive(Default)]
    struct RecordingSessionPort {
        sent: StdMutex<Vec<(PairingSessionId, PairingSessionMessage)>>,
        closed: StdMutex<Vec<(PairingSessionId, Option<String>)>>,
        fail_send: StdMutex<bool>,
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
            if *self.fail_send.lock().unwrap() {
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

    /// Fake event port whose `subscribe` hands out a pre-constructed
    /// receiver — tests drive events by pushing through the retained
    /// sender. Only one subscribe is allowed (the trait contract).
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

    /// Records each `consume_invitation` call and lets tests override the
    /// returned result per-call.
    #[derive(Default)]
    struct RecordingInvitationPort {
        consumed: StdMutex<Vec<InvitationCode>>,
        consume_err: StdMutex<Option<ConsumeInvitationError>>,
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
            if let Some(err) = self.consume_err.lock().unwrap().take() {
                return Err(err);
            }
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

    fn joiner_request(code: &str) -> JoinerRequest {
        JoinerRequest {
            invitation_code: InvitationCode::new(code),
            device_id: DeviceId::new("joiner-device"),
            device_name: "joiner's laptop".into(),
            identity_fingerprint: IdentityFingerprint::from_raw_string("ABCDEFGHIJKLMNOP").unwrap(),
            nonce: vec![1, 2, 3, 4],
        }
    }

    fn make_orchestrator(
        holder: Arc<InMemoryPairingInvitationHolder>,
        session: Arc<RecordingSessionPort>,
        invitation: Arc<RecordingInvitationPort>,
        events: Arc<ScriptedEventPort>,
        clock_ms: i64,
    ) -> Arc<PairingInboundOrchestrator> {
        Arc::new(PairingInboundOrchestrator::new(
            events as Arc<dyn PairingEventPort>,
            session as Arc<dyn PairingSessionPort>,
            invitation as Arc<dyn PairingInvitationPort>,
            holder,
            Arc::new(FakeClock(clock_ms)) as Arc<dyn ClockPort>,
        ))
    }

    fn scripted() -> (
        Arc<ScriptedEventPort>,
        tokio::sync::mpsc::Sender<PairingSessionEvent>,
    ) {
        let (tx, rx) = mpsc::channel(16);
        (Arc::new(ScriptedEventPort::new(rx)), tx)
    }

    // ── handle_event: direct dispatch ───────────────────────────────────

    #[tokio::test]
    async fn incoming_with_matching_invitation_consumes_and_stays_open() {
        let holder = Arc::new(InMemoryPairingInvitationHolder::new());
        holder.insert(pending("ABCD-1234")).await;
        let session_port = Arc::new(RecordingSessionPort::default());
        let invitation_port = Arc::new(RecordingInvitationPort::default());
        let (events, _tx) = scripted();
        let orch = make_orchestrator(
            holder.clone(),
            session_port.clone(),
            invitation_port.clone(),
            events,
            fixed_now_ms(),
        );

        let session = PairingSessionId::new("sess-1");
        orch.handle_event(PairingSessionEvent::Incoming {
            session: session.clone(),
            message: PairingSessionMessage::Request(joiner_request("ABCD-1234")),
        })
        .await;

        // No Reject was sent and the session was not closed.
        assert!(
            session_port.sent().is_empty(),
            "matching invitation path must not emit a Reject"
        );
        assert!(
            session_port.closed().is_empty(),
            "matching invitation path must leave session open for P7f"
        );
        // Rendezvous consume notified.
        assert_eq!(
            invitation_port.consumed(),
            vec![InvitationCode::new("ABCD-1234")]
        );
        // Holder slot was taken.
        assert_eq!(holder.len().await, 0);
    }

    #[tokio::test]
    async fn incoming_with_unknown_code_rejects_and_closes_session() {
        let holder = Arc::new(InMemoryPairingInvitationHolder::new());
        holder.insert(pending("EXPECTED")).await;
        let session_port = Arc::new(RecordingSessionPort::default());
        let invitation_port = Arc::new(RecordingInvitationPort::default());
        let (events, _tx) = scripted();
        let orch = make_orchestrator(
            holder.clone(),
            session_port.clone(),
            invitation_port.clone(),
            events,
            fixed_now_ms(),
        );

        let session = PairingSessionId::new("sess-2");
        orch.handle_event(PairingSessionEvent::Incoming {
            session: session.clone(),
            message: PairingSessionMessage::Request(joiner_request("WRONG")),
        })
        .await;

        let sent = session_port.sent();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, session);
        match &sent[0].1 {
            PairingSessionMessage::Reject(r) => {
                assert_eq!(r.reason, PairingRejectReason::InvitationMismatch)
            }
            other => panic!("expected Reject, got {:?}", other),
        }
        assert_eq!(session_port.closed().len(), 1, "session must be closed");
        assert!(
            invitation_port.consumed().is_empty(),
            "mismatch path must not notify rendezvous consume"
        );
        assert_eq!(
            holder.len().await,
            1,
            "unrelated pending invitation must remain parked"
        );
    }

    #[tokio::test]
    async fn incoming_with_expired_invitation_rejects_and_drops_slot() {
        let holder = Arc::new(InMemoryPairingInvitationHolder::new());
        holder.insert(pending("STALE")).await;
        let session_port = Arc::new(RecordingSessionPort::default());
        let invitation_port = Arc::new(RecordingInvitationPort::default());
        let (events, _tx) = scripted();
        // Clock 10 minutes past issue → aggregate is expired.
        let late_ms = (fixed_now() + Duration::minutes(10)).timestamp_millis();
        let orch = make_orchestrator(
            holder.clone(),
            session_port.clone(),
            invitation_port.clone(),
            events,
            late_ms,
        );

        orch.handle_event(PairingSessionEvent::Incoming {
            session: PairingSessionId::new("sess-3"),
            message: PairingSessionMessage::Request(joiner_request("STALE")),
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
        assert_eq!(holder.len().await, 0, "expired aggregate was dropped");
    }

    #[tokio::test]
    async fn incoming_with_non_request_first_message_is_rejected() {
        let holder = Arc::new(InMemoryPairingInvitationHolder::new());
        let session_port = Arc::new(RecordingSessionPort::default());
        let invitation_port = Arc::new(RecordingInvitationPort::default());
        let (events, _tx) = scripted();
        let orch = make_orchestrator(
            holder,
            session_port.clone(),
            invitation_port,
            events,
            fixed_now_ms(),
        );

        // Joiner accidentally sends a ChallengeResponse as first message.
        let bad = PairingSessionMessage::ChallengeResponse(JoinerChallengeResponse {
            encrypted_challenge: vec![0xDE, 0xAD],
        });
        orch.handle_event(PairingSessionEvent::Incoming {
            session: PairingSessionId::new("sess-4"),
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
                other => panic!("expected Internal reason, got {other:?}"),
            },
            other => panic!("expected Reject, got {:?}", other),
        }
        assert_eq!(session_port.closed().len(), 1);
    }

    #[tokio::test]
    async fn matching_invitation_swallows_rendezvous_consume_error() {
        let holder = Arc::new(InMemoryPairingInvitationHolder::new());
        holder.insert(pending("NET-GONE")).await;
        let session_port = Arc::new(RecordingSessionPort::default());
        let invitation_port = Arc::new(RecordingInvitationPort::default());
        *invitation_port.consume_err.lock().unwrap() =
            Some(ConsumeInvitationError::ServiceUnavailable);
        let (events, _tx) = scripted();
        let orch = make_orchestrator(
            holder,
            session_port.clone(),
            invitation_port.clone(),
            events,
            fixed_now_ms(),
        );

        orch.handle_event(PairingSessionEvent::Incoming {
            session: PairingSessionId::new("sess-5"),
            message: PairingSessionMessage::Request(joiner_request("NET-GONE")),
        })
        .await;

        // Consume was attempted but error must not disturb session state.
        assert_eq!(invitation_port.consumed().len(), 1);
        assert!(
            session_port.sent().is_empty(),
            "consume failure must not emit Reject"
        );
        assert!(
            session_port.closed().is_empty(),
            "consume failure must not close session"
        );
    }

    #[tokio::test]
    async fn message_received_and_closed_are_currently_no_op() {
        let holder = Arc::new(InMemoryPairingInvitationHolder::new());
        let session_port = Arc::new(RecordingSessionPort::default());
        let invitation_port = Arc::new(RecordingInvitationPort::default());
        let (events, _tx) = scripted();
        let orch = make_orchestrator(
            holder,
            session_port.clone(),
            invitation_port.clone(),
            events,
            fixed_now_ms(),
        );

        orch.handle_event(PairingSessionEvent::MessageReceived {
            session: PairingSessionId::new("sess-6"),
            message: PairingSessionMessage::KeyslotOffer(SponsorKeyslotOffer {
                space_id: SpaceId::from_string("space-x".into()),
                keyslot_blob: vec![],
                challenge: vec![],
            }),
        })
        .await;
        orch.handle_event(PairingSessionEvent::Closed {
            session: PairingSessionId::new("sess-6"),
            reason: Some("peer finish".into()),
        })
        .await;

        assert!(session_port.sent().is_empty());
        assert!(session_port.closed().is_empty());
        assert!(invitation_port.consumed().is_empty());
    }

    // ── spawn: run through real event channel ────────────────────────────

    #[tokio::test]
    async fn spawn_drains_events_from_subscription() {
        let holder = Arc::new(InMemoryPairingInvitationHolder::new());
        holder.insert(pending("LIVE")).await;
        let session_port = Arc::new(RecordingSessionPort::default());
        let invitation_port = Arc::new(RecordingInvitationPort::default());
        let (events, tx) = scripted();
        let orch = make_orchestrator(
            holder.clone(),
            session_port.clone(),
            invitation_port.clone(),
            events,
            fixed_now_ms(),
        );

        let handle = Arc::clone(&orch).spawn();

        tx.send(PairingSessionEvent::Incoming {
            session: PairingSessionId::new("live-1"),
            message: PairingSessionMessage::Request(joiner_request("LIVE")),
        })
        .await
        .unwrap();
        // Closing the channel lets the loop exit cleanly so we can join.
        drop(tx);
        tokio::time::timeout(std::time::Duration::from_secs(2), handle)
            .await
            .expect("spawn task must finish once channel closes")
            .expect("spawn task must not panic");

        assert_eq!(holder.len().await, 0);
        assert_eq!(
            invitation_port.consumed(),
            vec![InvitationCode::new("LIVE")]
        );
    }

    #[tokio::test]
    async fn spawn_exits_when_subscribe_fails() {
        // Exhaust the scripted receiver once so the second subscribe fails.
        let (_rx_tx, rx) = mpsc::channel::<PairingSessionEvent>(1);
        let events = Arc::new(ScriptedEventPort::new(rx));
        // Pre-consume the subscription slot.
        let _ = events.subscribe().await.unwrap();

        let orch = Arc::new(PairingInboundOrchestrator::new(
            events as Arc<dyn PairingEventPort>,
            Arc::new(RecordingSessionPort::default()) as Arc<dyn PairingSessionPort>,
            Arc::new(RecordingInvitationPort::default()) as Arc<dyn PairingInvitationPort>,
            Arc::new(InMemoryPairingInvitationHolder::new()),
            Arc::new(FakeClock(fixed_now_ms())) as Arc<dyn ClockPort>,
        ));
        let handle = orch.spawn();
        tokio::time::timeout(std::time::Duration::from_secs(2), handle)
            .await
            .expect("task exits when subscribe fails")
            .expect("task must not panic on subscribe failure");
    }
}
