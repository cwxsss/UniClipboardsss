use std::time::Duration;

use tokio::time::timeout;
use uc_core::ports::{
    PairingCompleteEvent, PairingFailedEvent, PairingVerificationRequiredEvent, RealtimeEvent,
    RealtimeTopic,
};
use uc_daemon::api::auth::DaemonConnectionInfo;
use uc_daemon::api::types::DaemonWsEvent;
use uc_tauri::bootstrap::daemon_ws_bridge::{
    BridgeState, DaemonWsBridge, DaemonWsBridgeConfig, ScriptedDaemonWsConnector,
};
use uc_tauri::bootstrap::DaemonConnectionState;

fn connection_state() -> DaemonConnectionState {
    let state = DaemonConnectionState::default();
    state.set(DaemonConnectionInfo {
        base_url: "http://127.0.0.1:43123".into(),
        ws_url: "ws://127.0.0.1:43123/ws".into(),
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
        "topic": "pairing",
        "type": "pairing.verification_required",
        "sessionId": session_id,
        "ts": 1,
        "payload": {
            "sessionId": session_id,
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
        "topic": "pairing",
        "type": "pairing.complete",
        "sessionId": session_id,
        "ts": 2,
        "payload": {
            "sessionId": session_id,
            "state": "completed",
            "peerId": peer_id,
            "deviceName": "Desk",
            "updatedAtMs": 2
        }
    }))
    .expect("pairing complete fixture should parse")
}

fn pairing_failed(session_id: &str, reason: &str) -> DaemonWsEvent {
    serde_json::from_value(serde_json::json!({
        "topic": "pairing",
        "type": "pairing.failed",
        "sessionId": session_id,
        "ts": 3,
        "payload": {
            "sessionId": session_id,
            "peerId": "peer-1",
            "error": reason
        }
    }))
    .expect("pairing failed fixture should parse")
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
