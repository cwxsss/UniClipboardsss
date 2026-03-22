use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio::time::timeout;
use uc_app::realtime;
use uc_app::usecases::pairing::PairingDomainEvent;
use uc_core::network::pairing_state_machine::FailureReason;
use uc_core::ports::host_event_emitter::SetupHostEvent;
use uc_core::ports::host_event_emitter::{EmitError, HostEvent, HostEventEmitterPort};
use uc_core::ports::realtime::{
    PairingCompleteEvent, PairingVerificationRequiredEvent, PeerChangedEvent, RealtimeEvent,
    RealtimeFrontendPayload, RealtimePeerSummary, RealtimeTopic, RealtimeTopicPort,
    SetupStateChangedEvent, FRONTEND_REALTIME_EVENT,
};
use uc_core::setup::SetupState;

struct StubRealtimePort {
    receiver: Mutex<Option<mpsc::Receiver<RealtimeEvent>>>,
    subscribe_calls: Mutex<Vec<(&'static str, Vec<RealtimeTopic>)>>,
}

impl StubRealtimePort {
    fn new(receiver: mpsc::Receiver<RealtimeEvent>) -> Self {
        Self {
            receiver: Mutex::new(Some(receiver)),
            subscribe_calls: Mutex::new(Vec::new()),
        }
    }

    fn subscribe_calls(&self) -> Vec<(&'static str, Vec<RealtimeTopic>)> {
        self.subscribe_calls.lock().unwrap().clone()
    }
}

#[async_trait]
impl RealtimeTopicPort for StubRealtimePort {
    async fn subscribe(
        &self,
        consumer: &'static str,
        topics: &[RealtimeTopic],
    ) -> anyhow::Result<mpsc::Receiver<RealtimeEvent>> {
        self.subscribe_calls
            .lock()
            .unwrap()
            .push((consumer, topics.to_vec()));
        self.receiver
            .lock()
            .unwrap()
            .take()
            .ok_or_else(|| anyhow::anyhow!("test receiver already consumed"))
    }
}

#[derive(Default)]
struct RecordingEmitter {
    events: Mutex<Vec<HostEvent>>,
}

impl RecordingEmitter {
    fn events(&self) -> Vec<HostEvent> {
        self.events.lock().unwrap().clone()
    }
}

impl HostEventEmitterPort for RecordingEmitter {
    fn emit(&self, event: HostEvent) -> Result<(), EmitError> {
        self.events.lock().unwrap().push(event);
        Ok(())
    }
}

async fn assert_pairing_consumer_maps_typed_realtime_events_to_daemon_realtime_envelope() {
    let (tx, rx) = mpsc::channel(8);
    let realtime = Arc::new(StubRealtimePort::new(rx));
    let emitter = Arc::new(RecordingEmitter::default());
    let consumer_realtime = realtime.clone();
    let consumer_emitter = emitter.clone();

    let task = tokio::spawn(async move {
        realtime::run_pairing_realtime_consumer(consumer_realtime, consumer_emitter).await
    });

    tx.send(RealtimeEvent::PairingVerificationRequired(
        PairingVerificationRequiredEvent {
            session_id: "session-1".into(),
            peer_id: Some("peer-1".into()),
            device_name: Some("Desk".into()),
            code: Some("123456".into()),
            local_fingerprint: Some("local".into()),
            peer_fingerprint: Some("peer".into()),
        },
    ))
    .await
    .unwrap();
    drop(tx);

    task.await.unwrap().unwrap();

    let calls = realtime.subscribe_calls();
    assert_eq!(
        calls,
        vec![("pairing_realtime_consumer", vec![RealtimeTopic::Pairing])]
    );

    let events = emitter.events();
    assert_eq!(events.len(), 1);
    match &events[0] {
        HostEvent::Realtime(event) => {
            assert_eq!(FRONTEND_REALTIME_EVENT, "daemon://realtime");
            assert_eq!(event.topic, RealtimeTopic::Pairing);
            assert_eq!(event.event_type(), "pairing.verificationRequired");
            assert_eq!(event.ts, 0);
            assert!(matches!(
                &event.payload,
                RealtimeFrontendPayload::PairingVerificationRequired(payload)
                    if payload.session_id == "session-1"
                        && payload.peer_id.as_deref() == Some("peer-1")
                        && payload.code.as_deref() == Some("123456")
            ));
        }
        other => panic!("expected HostEvent::Realtime, got {other:?}"),
    }
}

#[tokio::test]
async fn pairing_consumer_maps_typed_realtime_events_to_daemon_realtime_envelope() {
    assert_pairing_consumer_maps_typed_realtime_events_to_daemon_realtime_envelope().await;
}

async fn assert_peers_consumer_emits_peer_delta_envelopes_without_transport_dto_leakage() {
    let (tx, rx) = mpsc::channel(8);
    let realtime = Arc::new(StubRealtimePort::new(rx));
    let emitter = Arc::new(RecordingEmitter::default());
    let consumer_realtime = realtime.clone();
    let consumer_emitter = emitter.clone();

    let task = tokio::spawn(async move {
        realtime::run_peers_realtime_consumer(consumer_realtime, consumer_emitter).await
    });

    tx.send(RealtimeEvent::PeersChanged(PeerChangedEvent {
        peers: vec![RealtimePeerSummary {
            peer_id: "peer-1".into(),
            device_name: Some("Desk".into()),
            connected: true,
        }],
    }))
    .await
    .unwrap();
    drop(tx);

    task.await.unwrap().unwrap();

    let calls = realtime.subscribe_calls();
    assert_eq!(
        calls,
        vec![("peers_realtime_consumer", vec![RealtimeTopic::Peers])]
    );

    let events = emitter.events();
    assert_eq!(events.len(), 1);
    match &events[0] {
        HostEvent::Realtime(event) => {
            assert_eq!(event.topic, RealtimeTopic::Peers);
            assert_eq!(event.event_type(), "peers.changed");
            assert!(matches!(
                &event.payload,
                RealtimeFrontendPayload::PeersChanged(payload)
                    if payload.peers.len() == 1
                        && payload.peers[0].peer_id == "peer-1"
                        && payload.peers[0].device_name.as_deref() == Some("Desk")
                        && payload.peers[0].connected
            ));
        }
        other => panic!("expected HostEvent::Realtime, got {other:?}"),
    }
}

#[tokio::test]
async fn peers_consumer_emits_peer_delta_envelopes_without_transport_dto_leakage() {
    assert_peers_consumer_emits_peer_delta_envelopes_without_transport_dto_leakage().await;
}

async fn assert_setup_consumer_reuses_shared_realtime_hub() {
    let (tx, rx) = mpsc::channel(8);
    let realtime = Arc::new(StubRealtimePort::new(rx));
    let hub = Arc::new(realtime::SetupPairingEventHub::new(8));
    let mut subscriber_a = hub.subscribe().await.unwrap();
    let mut subscriber_b = hub.subscribe().await.unwrap();
    let consumer_realtime = realtime.clone();
    let consumer_hub = hub.clone();

    let task = tokio::spawn(async move {
        realtime::run_setup_realtime_consumer(consumer_realtime, consumer_hub).await
    });

    tx.send(RealtimeEvent::PairingVerificationRequired(
        PairingVerificationRequiredEvent {
            session_id: "session-1".into(),
            peer_id: Some("peer-1".into()),
            device_name: Some("Desk".into()),
            code: Some("123456".into()),
            local_fingerprint: Some("local".into()),
            peer_fingerprint: Some("peer".into()),
        },
    ))
    .await
    .unwrap();
    drop(tx);

    let event_a = timeout(Duration::from_secs(1), subscriber_a.recv())
        .await
        .expect("first setup subscriber should receive an event")
        .expect("first setup subscriber channel should stay open");
    let event_b = timeout(Duration::from_secs(1), subscriber_b.recv())
        .await
        .expect("second setup subscriber should receive an event")
        .expect("second setup subscriber channel should stay open");

    task.await.unwrap().unwrap();

    let expected = PairingDomainEvent::PairingVerificationRequired {
        session_id: "session-1".into(),
        peer_id: "peer-1".into(),
        short_code: "123456".into(),
        local_fingerprint: "local".into(),
        peer_fingerprint: "peer".into(),
    };
    assert_eq!(event_a, expected);
    assert_eq!(event_b, expected);
    assert_eq!(realtime.subscribe_calls().len(), 1);
}

#[tokio::test]
async fn setup_consumer_reuses_shared_realtime_hub() {
    assert_setup_consumer_reuses_shared_realtime_hub().await;
}

async fn assert_setup_consumer_filters_session_ordering_before_forwarding() {
    let (tx, rx) = mpsc::channel(8);
    let realtime = Arc::new(StubRealtimePort::new(rx));
    let hub = Arc::new(realtime::SetupPairingEventHub::new(8));
    let mut subscriber = hub.subscribe().await.unwrap();
    let consumer_realtime = realtime.clone();
    let consumer_hub = hub.clone();

    let task = tokio::spawn(async move {
        realtime::run_setup_realtime_consumer(consumer_realtime, consumer_hub).await
    });

    tx.send(RealtimeEvent::PairingComplete(PairingCompleteEvent {
        session_id: "session-ordered".into(),
        peer_id: Some("peer-1".into()),
        device_name: Some("Desk".into()),
    }))
    .await
    .unwrap();
    tx.send(RealtimeEvent::PairingVerificationRequired(
        PairingVerificationRequiredEvent {
            session_id: "session-ordered".into(),
            peer_id: Some("peer-1".into()),
            device_name: Some("Desk".into()),
            code: Some("123456".into()),
            local_fingerprint: Some("local".into()),
            peer_fingerprint: Some("peer".into()),
        },
    ))
    .await
    .unwrap();
    drop(tx);

    let first = timeout(Duration::from_secs(1), subscriber.recv())
        .await
        .expect("setup subscriber should receive the monotonic session event")
        .expect("setup subscriber channel should stay open");
    assert_eq!(
        first,
        PairingDomainEvent::PairingSucceeded {
            session_id: "session-ordered".into(),
            peer_id: "peer-1".into(),
        }
    );

    let second = timeout(Duration::from_millis(100), subscriber.recv()).await;
    assert!(
        second.is_err(),
        "setup consumer should drop out-of-order session regressions before forwarding"
    );

    task.await.unwrap().unwrap();

    let failure = PairingDomainEvent::PairingFailed {
        session_id: "session-ordered".into(),
        peer_id: String::new(),
        reason: FailureReason::Other("unused".into()),
    };
    let debug = format!("{failure:?}");
    assert!(
        debug.contains("PairingFailed"),
        "PairingDomainEvent remains the setup-facing contract, got {debug}"
    );
}

#[tokio::test]
async fn setup_consumer_filters_session_ordering_before_forwarding() {
    assert_setup_consumer_filters_session_ordering_before_forwarding().await;
}

async fn assert_setup_state_consumer_emits_setup_host_events_to_frontend_adapter() {
    let (tx, rx) = mpsc::channel(8);
    let realtime = Arc::new(StubRealtimePort::new(rx));
    let emitter = Arc::new(RecordingEmitter::default());
    let consumer_realtime = realtime.clone();
    let consumer_emitter = emitter.clone();

    let task = tokio::spawn(async move {
        realtime::run_setup_state_realtime_consumer(consumer_realtime, consumer_emitter).await
    });

    tx.send(RealtimeEvent::SetupStateChanged(SetupStateChangedEvent {
        session_id: Some("session-setup".into()),
        state: SetupState::JoinSpaceConfirmPeer {
            short_code: "123456".into(),
            peer_fingerprint: Some("peer-fp".into()),
            error: None,
        },
    }))
    .await
    .unwrap();
    drop(tx);

    task.await.unwrap().unwrap();

    let calls = realtime.subscribe_calls();
    assert_eq!(
        calls,
        vec![("setup_state_realtime_consumer", vec![RealtimeTopic::Setup])]
    );

    let events = emitter.events();
    assert_eq!(events.len(), 1);
    match &events[0] {
        HostEvent::Setup(SetupHostEvent::StateChanged { state, session_id }) => {
            assert_eq!(session_id.as_deref(), Some("session-setup"));
            assert!(
                matches!(state, SetupState::JoinSpaceConfirmPeer { short_code, .. } if short_code == "123456")
            );
        }
        other => panic!("expected HostEvent::Setup, got {other:?}"),
    }
}

#[tokio::test]
async fn setup_state_consumer_emits_setup_host_events_to_frontend_adapter() {
    assert_setup_state_consumer_emits_setup_host_events_to_frontend_adapter().await;
}

#[tokio::test]
async fn realtime_consumer_contract_executes_all_cases() {
    assert_pairing_consumer_maps_typed_realtime_events_to_daemon_realtime_envelope().await;
    assert_peers_consumer_emits_peer_delta_envelopes_without_transport_dto_leakage().await;
    assert_setup_consumer_reuses_shared_realtime_hub().await;
    assert_setup_consumer_filters_session_ordering_before_forwarding().await;
    assert_setup_state_consumer_emits_setup_host_events_to_frontend_adapter().await;
}
