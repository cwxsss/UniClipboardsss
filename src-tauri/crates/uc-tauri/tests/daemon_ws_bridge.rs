use std::io::Write;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use tokio::time::sleep;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::fmt::MakeWriter;
use uc_app::testing::{NoopDiscoveryPort, NoopLifecycleEventEmitter};
use uc_app::usecases::pairing::PairingDomainEvent;
use uc_app::usecases::space_access::SpaceAccessOrchestrator;
use uc_core::ports::{
    PairingCompleteEvent, PairingFailedEvent, PairingVerificationRequiredEvent, RealtimeEvent,
    RealtimeTopic,
};
use uc_daemon::api::auth::DaemonConnectionInfo;
use uc_daemon::api::types::DaemonWsEvent;
use uc_tauri::bootstrap::daemon_ws_bridge::{
    BridgeState, DaemonWsBridge, DaemonWsBridgeConfig, ScriptedDaemonWsConnector,
};
use uc_tauri::bootstrap::{
    install_daemon_setup_pairing_facade, DaemonConnectionState, SetupAssemblyPorts,
    SetupPairingFacadePort,
};

fn connection_state() -> DaemonConnectionState {
    let state = DaemonConnectionState::default();
    state.set(DaemonConnectionInfo {
        base_url: "http://127.0.0.1:43123".into(),
        ws_url: "ws://127.0.0.1:43123/ws".into(),
        token: "test-token".into(),
    });
    state
}

fn unavailable_connection_state() -> DaemonConnectionState {
    let state = DaemonConnectionState::default();
    state.set(DaemonConnectionInfo {
        base_url: "http://127.0.0.1:9".into(),
        ws_url: "ws://127.0.0.1:9/ws".into(),
        token: "test-token".into(),
    });
    state
}

fn bridge_config(queue_capacity: usize) -> DaemonWsBridgeConfig {
    DaemonWsBridgeConfig {
        queue_capacity,
        terminal_retry_delay: Duration::from_millis(10),
        ..DaemonWsBridgeConfig::default()
    }
}

fn pairing_verification_required(session_id: &str, code: &str) -> DaemonWsEvent {
    serde_json::from_value(serde_json::json!({
        "topic": "pairing/verification",
        "type": "pairing.verification_required",
        "sessionId": session_id,
        "ts": 1,
        "payload": {
            "sessionId": session_id,
            "kind": "verification",
            "peerId": "peer-1",
            "deviceName": "Desk",
            "code": code,
            "localFingerprint": "local-fingerprint",
            "peerFingerprint": "peer-fingerprint"
        }
    }))
    .expect("pairing verification fixture should parse")
}

fn pairing_complete(session_id: &str, peer_id: &str) -> DaemonWsEvent {
    serde_json::from_value(serde_json::json!({
        "topic": "pairing/session",
        "type": "pairing.complete",
        "sessionId": session_id,
        "ts": 2,
        "payload": {
            "sessionId": session_id,
            "state": "complete",
            "stage": "complete",
            "peerId": peer_id,
            "deviceName": "Desk",
            "updatedAtMs": 2,
            "ts": 2
        }
    }))
    .expect("pairing complete fixture should parse")
}

fn pairing_failed(session_id: &str, reason: &str) -> DaemonWsEvent {
    serde_json::from_value(serde_json::json!({
        "topic": "pairing/verification",
        "type": "pairing.failed",
        "sessionId": session_id,
        "ts": 3,
        "payload": {
            "sessionId": session_id,
            "peerId": "peer-1",
            "error": reason,
            "reason": reason
        }
    }))
    .expect("pairing failed fixture should parse")
}

#[derive(Clone, Default)]
struct TestLogBuffer {
    buffer: Arc<Mutex<Vec<u8>>>,
}

struct TestLogWriter {
    buffer: Arc<Mutex<Vec<u8>>>,
}

impl TestLogBuffer {
    fn content(&self) -> String {
        String::from_utf8(self.buffer.lock().expect("buffer lock").clone())
            .expect("log output should be utf8")
    }
}

impl<'a> MakeWriter<'a> for TestLogBuffer {
    type Writer = TestLogWriter;

    fn make_writer(&'a self) -> Self::Writer {
        TestLogWriter {
            buffer: self.buffer.clone(),
        }
    }
}

impl Write for TestLogWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.buffer
            .lock()
            .expect("buffer lock")
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct InitialSetupPairingFacade;

#[async_trait]
impl SetupPairingFacadePort for InitialSetupPairingFacade {
    async fn subscribe(&self) -> anyhow::Result<tokio::sync::mpsc::Receiver<PairingDomainEvent>> {
        let (_tx, rx) = tokio::sync::mpsc::channel(1);
        Ok(rx)
    }

    async fn initiate_pairing(&self, _peer_id: String) -> anyhow::Result<String> {
        Err(anyhow::anyhow!(
            "initial setup pairing facade should be replaced"
        ))
    }

    async fn accept_pairing(&self, _session_id: &str) -> anyhow::Result<()> {
        Ok(())
    }

    async fn reject_pairing(&self, _session_id: &str) -> anyhow::Result<()> {
        Ok(())
    }

    async fn cancel_pairing(&self, _session_id: &str) -> anyhow::Result<()> {
        Ok(())
    }

    async fn verify_pairing(&self, _session_id: &str, _pin_matches: bool) -> anyhow::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn daemon_ws_bridge_starts_single_connection() {
    let connector = ScriptedDaemonWsConnector::new();
    connector
        .queue_connection(vec![pairing_complete("session-1", "peer-1")])
        .await
        .expect("fixture connection should queue");

    let bridge =
        DaemonWsBridge::new_for_test(connection_state(), connector.clone(), bridge_config(4));
    let mut rx = bridge
        .subscribe("pairing_consumer", &[RealtimeTopic::Pairing])
        .await
        .expect("subscription should succeed");

    bridge
        .run_until_idle()
        .await
        .expect("bridge should drain scripted connection");

    assert_eq!(bridge.state(), BridgeState::Ready);
    assert_eq!(connector.connect_attempts(), 1);
    assert_eq!(
        connector.subscribe_requests(),
        vec![vec!["pairing".to_string()]]
    );
    assert_eq!(
        connector.auth_headers(),
        vec!["Bearer test-token".to_string()]
    );

    let event = timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("pairing consumer should receive an event")
        .expect("pairing consumer channel should stay open");
    assert!(matches!(
        event,
        RealtimeEvent::PairingComplete(PairingCompleteEvent { session_id, peer_id, .. })
            if session_id == "session-1" && peer_id.as_deref() == Some("peer-1")
    ));
}

#[tokio::test]
async fn daemon_ws_bridge_resubscribes_after_reconnect() {
    let connector = ScriptedDaemonWsConnector::new();
    connector
        .queue_connection(vec![pairing_verification_required("session-1", "123456")])
        .await
        .expect("first scripted connection should queue");
    connector
        .queue_connection(vec![pairing_complete("session-1", "peer-1")])
        .await
        .expect("second scripted connection should queue");

    let bridge =
        DaemonWsBridge::new_for_test(connection_state(), connector.clone(), bridge_config(4));
    let mut rx = bridge
        .subscribe("pairing_consumer", &[RealtimeTopic::Pairing])
        .await
        .expect("subscription should succeed");

    bridge
        .run_until_idle()
        .await
        .expect("bridge should reconnect and drain scripted connections");

    let first = timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("first pairing event should arrive")
        .expect("receiver should remain open");
    let second = timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("second pairing event should arrive after reconnect")
        .expect("receiver should remain open");

    assert!(matches!(
        first,
        RealtimeEvent::PairingVerificationRequired(PairingVerificationRequiredEvent { session_id, code, .. })
            if session_id == "session-1" && code.as_deref() == Some("123456")
    ));
    assert!(matches!(
        second,
        RealtimeEvent::PairingComplete(PairingCompleteEvent { session_id, .. })
            if session_id == "session-1"
    ));
    assert_eq!(connector.connect_attempts(), 2);
    assert_eq!(
        connector.subscribe_requests(),
        vec![vec!["pairing".to_string()], vec!["pairing".to_string()]]
    );
}

#[tokio::test]
async fn daemon_ws_bridge_backpressure() {
    let connector = ScriptedDaemonWsConnector::new();
    connector
        .queue_connection(vec![
            pairing_verification_required("session-backpressure", "111111"),
            pairing_complete("session-backpressure", "peer-1"),
            pairing_failed("session-backpressure", "timed out"),
        ])
        .await
        .expect("scripted connection should queue");

    let bridge = DaemonWsBridge::new_for_test(connection_state(), connector, bridge_config(1));
    let mut rx = bridge
        .subscribe("setup_consumer", &[RealtimeTopic::Pairing])
        .await
        .expect("subscription should succeed");

    bridge
        .run_until_idle()
        .await
        .expect("bridge should apply backpressure policy while draining");

    let first = timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("first event should be delivered")
        .expect("channel should remain open");
    let second = timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("terminal event should be retried and delivered")
        .expect("channel should remain open");
    let third = timeout(Duration::from_millis(100), rx.recv()).await;

    assert!(matches!(
        first,
        RealtimeEvent::PairingVerificationRequired(PairingVerificationRequiredEvent { session_id, .. })
            if session_id == "session-backpressure"
    ));
    assert!(matches!(
        second,
        RealtimeEvent::PairingFailed(PairingFailedEvent { session_id, reason })
            if session_id == "session-backpressure" && reason == "timed out"
    ));
    assert!(
        third.is_err(),
        "ordinary events should drop under backpressure while terminal events get one retry"
    );
}

#[tokio::test]
async fn daemon_ws_bridge_logs_daemon_unavailable_without_panicking() {
    let log_buffer = TestLogBuffer::default();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(log_buffer.clone())
        .with_max_level(tracing::Level::WARN)
        .with_ansi(false)
        .without_time()
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);

    let bridge = Arc::new(DaemonWsBridge::new(
        unavailable_connection_state(),
        bridge_config(4),
    ));
    let _rx = bridge
        .subscribe("pairing_consumer", &[RealtimeTopic::Pairing])
        .await
        .expect("subscription should succeed");
    let cancel = CancellationToken::new();

    let task = tokio::spawn({
        let bridge = bridge.clone();
        let token = cancel.child_token();
        async move { bridge.run(token).await }
    });

    timeout(Duration::from_secs(1), async {
        loop {
            if bridge.state() == BridgeState::Degraded {
                break;
            }
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("bridge should report degraded state for unavailable daemon");

    cancel.cancel();

    let result = timeout(Duration::from_secs(1), task)
        .await
        .expect("bridge task should stop after cancellation")
        .expect("bridge join should succeed");
    assert!(
        result.is_ok(),
        "bridge should not panic on daemon unavailability"
    );

    let logs = log_buffer.content();
    assert!(
        logs.contains("daemon websocket bridge cycle failed"),
        "expected bridge failure log, got: {}",
        logs
    );
}

#[tokio::test]
async fn daemon_ws_bridge_fans_out_to_multiple_consumers() {
    let connector = ScriptedDaemonWsConnector::new();
    connector
        .queue_connection(vec![pairing_complete("session-fanout", "peer-fanout")])
        .await
        .expect("scripted connection should queue");

    let bridge = DaemonWsBridge::new_for_test(connection_state(), connector, bridge_config(4));
    let mut rx_a = bridge
        .subscribe("pairing_consumer_a", &[RealtimeTopic::Pairing])
        .await
        .expect("first subscription should succeed");
    let mut rx_b = bridge
        .subscribe("pairing_consumer_b", &[RealtimeTopic::Pairing])
        .await
        .expect("second subscription should succeed");

    bridge
        .run_until_idle()
        .await
        .expect("bridge should fan out to all active consumers");

    let a = timeout(Duration::from_secs(1), rx_a.recv())
        .await
        .expect("first consumer should receive event")
        .expect("first consumer channel should stay open");
    let b = timeout(Duration::from_secs(1), rx_b.recv())
        .await
        .expect("second consumer should receive event")
        .expect("second consumer channel should stay open");

    assert!(matches!(
        a,
        RealtimeEvent::PairingComplete(PairingCompleteEvent { ref session_id, ref peer_id, .. })
            if session_id == "session-fanout" && peer_id.as_deref() == Some("peer-fanout")
    ));
    assert_eq!(a, b);
}

#[tokio::test]
async fn install_daemon_setup_pairing_facade_routes_bridge_events_into_setup_subscription() {
    let connector = ScriptedDaemonWsConnector::new();
    connector
        .queue_connection(vec![pairing_verification_required(
            "session-setup",
            "654321",
        )])
        .await
        .expect("scripted connection should queue");

    let bridge = Arc::new(DaemonWsBridge::new_for_test(
        connection_state(),
        connector.clone(),
        bridge_config(4),
    ));
    let mut pairing_rx = bridge
        .subscribe("pairing_consumer", &[RealtimeTopic::Pairing])
        .await
        .expect("pairing subscription should succeed");
    let mut setup_ports = SetupAssemblyPorts {
        setup_pairing_facade: Arc::new(InitialSetupPairingFacade),
        space_access_orchestrator: Arc::new(SpaceAccessOrchestrator::new()),
        discovery_port: Arc::new(NoopDiscoveryPort),
        device_announcer: None,
        lifecycle_emitter: Arc::new(NoopLifecycleEventEmitter),
    };

    let setup_hub = install_daemon_setup_pairing_facade(&mut setup_ports, connection_state());
    let mut setup_rx = setup_ports
        .setup_pairing_facade
        .subscribe()
        .await
        .expect("setup facade subscription should succeed");

    let consumer_bridge = bridge.clone();
    let consumer_hub = setup_hub.clone();
    let consumer_task = tokio::spawn(async move {
        uc_app::realtime::run_setup_realtime_consumer(consumer_bridge, consumer_hub)
            .await
            .expect("setup realtime consumer should stay healthy");
    });
    tokio::task::yield_now().await;

    bridge
        .run_until_idle()
        .await
        .expect("bridge should drain scripted connection");

    let event = timeout(Duration::from_secs(1), setup_rx.recv())
        .await
        .expect("setup subscription should receive a verification event")
        .expect("setup subscription channel should stay open");
    let pairing_event = timeout(Duration::from_secs(1), pairing_rx.recv())
        .await
        .expect("pairing consumer should receive the same verification event")
        .expect("pairing consumer channel should stay open");

    consumer_task.abort();

    assert!(matches!(
        event,
        PairingDomainEvent::PairingVerificationRequired {
            session_id,
            peer_id,
            short_code,
            local_fingerprint,
            peer_fingerprint,
        }
            if session_id == "session-setup"
                && peer_id == "peer-1"
                && short_code == "654321"
                && local_fingerprint == "local-fingerprint"
                && peer_fingerprint == "peer-fingerprint"
    ));
    assert!(matches!(
        pairing_event,
        RealtimeEvent::PairingVerificationRequired(PairingVerificationRequiredEvent { session_id, code, .. })
            if session_id == "session-setup" && code.as_deref() == Some("654321")
    ));
    assert_eq!(connector.connect_attempts(), 1);
    assert_eq!(
        connector.subscribe_requests(),
        vec![vec!["pairing".to_string()]]
    );
}
