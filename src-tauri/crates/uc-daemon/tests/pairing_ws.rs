use std::sync::Arc;
use std::sync::{Mutex, OnceLock};

use axum::body::{to_bytes, Body};
use axum::http::Request;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tower::ServiceExt;
use uc_daemon::api::auth::load_or_create_auth_token;
use uc_daemon::api::query::DaemonQueryService;
use uc_daemon::api::server::{build_router, DaemonApiState};
use uc_daemon::api::types::{
    DaemonWsEvent, PairedDevicesChangedPayload, PairingFailurePayload, PairingVerificationPayload,
    PeerChangedPayload, PeerConnectionChangedPayload, PeerNameUpdatedPayload,
};
use uc_daemon::pairing::session_projection::upsert_pairing_snapshot;
use uc_daemon::state::RuntimeState;

struct PairingWsHarness {
    app: axum::Router,
    url: String,
    token: String,
    event_tx: tokio::sync::broadcast::Sender<DaemonWsEvent>,
    state: Arc<RwLock<RuntimeState>>,
    handle: tokio::task::JoinHandle<()>,
}

fn build_runtime() -> Arc<uc_app::runtime::CoreRuntime> {
    static RUNTIME_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let _guard = RUNTIME_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    Arc::new(uc_bootstrap::build_cli_runtime(None).unwrap())
}

async fn spawn_server() -> PairingWsHarness {
    let runtime = build_runtime();
    let state = Arc::new(RwLock::new(RuntimeState::new(vec![])));
    let query_service = Arc::new(DaemonQueryService::new(runtime, state.clone()));
    let tempdir = tempfile::tempdir().unwrap();
    let token_path = tempdir.path().join("daemon.token");
    let token = load_or_create_auth_token(&token_path).unwrap();
    let token_value = std::fs::read_to_string(&token_path).unwrap();
    let api_state = DaemonApiState::new(query_service, token, None);
    let event_tx = api_state.event_tx.clone();
    let app = build_router(api_state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server_app = app.clone();
    let handle = tokio::spawn(async move {
        axum::serve(listener, server_app.into_make_service())
            .await
            .unwrap();
    });

    PairingWsHarness {
        app,
        url: format!("ws://{}/ws", addr),
        token: token_value,
        event_tx,
        state,
        handle,
    }
}

async fn connect_with_token(
    url: &str,
    token: &str,
) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>> {
    let mut request = url.into_client_request().unwrap();
    request.headers_mut().insert(
        "Authorization",
        format!("Bearer {}", token.trim()).parse().unwrap(),
    );
    let (socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();
    socket
}

async fn subscribe(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    topics: &[&str],
) {
    socket
        .send(tokio_tungstenite::tungstenite::Message::Text(
            json!({"action": "subscribe", "topics": topics})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
}

async fn next_json(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) -> Value {
    serde_json::from_str(socket.next().await.unwrap().unwrap().to_text().unwrap()).unwrap()
}

fn authed_get_request(uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header("Authorization", format!("Bearer {}", token.trim()))
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn snapshot_contains_session_id_and_omits_verification_secrets() {
    let harness = spawn_server().await;
    upsert_pairing_snapshot(
        &harness.state,
        "session-1",
        Some("peer-1".to_string()),
        Some("Desk".to_string()),
        "request",
        1_742_371_200_000,
    )
    .await;

    let mut socket = connect_with_token(&harness.url, &harness.token).await;
    subscribe(&mut socket, &["pairing"]).await;

    let event = next_json(&mut socket).await;
    harness.handle.abort();

    assert_eq!(event["type"], "pairing.snapshot");
    assert!(event.get("event_type").is_none());
    let payload = event["payload"].as_array().unwrap();
    assert_eq!(payload[0]["sessionId"], "session-1");
    assert!(payload[0].get("code").is_none());
    assert!(payload[0].get("localFingerprint").is_none());
    assert!(payload[0].get("peerFingerprint").is_none());
    assert!(payload[0].get("keyslotFile").is_none());
    assert!(payload[0].get("challenge").is_none());
}

#[tokio::test]
async fn incremental_verification_event_contains_code_and_fingerprints() {
    let harness = spawn_server().await;
    let mut socket = connect_with_token(&harness.url, &harness.token).await;
    subscribe(&mut socket, &["pairing"]).await;
    let _snapshot = next_json(&mut socket).await;

    harness
        .event_tx
        .send(DaemonWsEvent {
            topic: "pairing".to_string(),
            event_type: "pairing.verification_required".to_string(),
            session_id: Some("session-2".to_string()),
            ts: 1_742_371_200_123,
            payload: serde_json::to_value(PairingVerificationPayload {
                session_id: "session-2".to_string(),
                peer_id: "peer-2".to_string(),
                device_name: Some("Laptop".to_string()),
                code: "123456".to_string(),
                local_fingerprint: "local-fp".to_string(),
                peer_fingerprint: "peer-fp".to_string(),
            })
            .unwrap(),
        })
        .unwrap();

    let event = next_json(&mut socket).await;
    harness.handle.abort();

    assert_eq!(event["type"], "pairing.verification_required");
    assert_eq!(event["payload"]["code"], "123456");
    assert_eq!(event["payload"]["localFingerprint"], "local-fp");
    assert_eq!(event["payload"]["peerFingerprint"], "peer-fp");
}

#[tokio::test]
async fn peers_and_paired_devices_incremental_events_preserve_bridge_fields() {
    let harness = spawn_server().await;
    let mut socket = connect_with_token(&harness.url, &harness.token).await;
    subscribe(&mut socket, &["peers", "paired-devices"]).await;
    let _peers_snapshot = next_json(&mut socket).await;
    let _paired_devices_snapshot = next_json(&mut socket).await;

    harness
        .event_tx
        .send(DaemonWsEvent {
            topic: "peers".to_string(),
            event_type: "peers.changed".to_string(),
            session_id: None,
            ts: 1,
            payload: serde_json::to_value(PeerChangedPayload {
                peer_id: "peer-3".to_string(),
                device_name: Some("Desk".to_string()),
                addresses: vec!["/ip4/127.0.0.1/tcp/7000".to_string()],
                discovered: false,
                connected: true,
            })
            .unwrap(),
        })
        .unwrap();
    harness
        .event_tx
        .send(DaemonWsEvent {
            topic: "peers".to_string(),
            event_type: "peers.name_updated".to_string(),
            session_id: None,
            ts: 2,
            payload: serde_json::to_value(PeerNameUpdatedPayload {
                peer_id: "peer-3".to_string(),
                device_name: "Renamed".to_string(),
            })
            .unwrap(),
        })
        .unwrap();
    harness
        .event_tx
        .send(DaemonWsEvent {
            topic: "peers".to_string(),
            event_type: "peers.connection_changed".to_string(),
            session_id: None,
            ts: 3,
            payload: serde_json::to_value(PeerConnectionChangedPayload {
                peer_id: "peer-3".to_string(),
                device_name: Some("Renamed".to_string()),
                connected: false,
            })
            .unwrap(),
        })
        .unwrap();
    harness
        .event_tx
        .send(DaemonWsEvent {
            topic: "paired-devices".to_string(),
            event_type: "paired-devices.changed".to_string(),
            session_id: None,
            ts: 4,
            payload: serde_json::to_value(PairedDevicesChangedPayload {
                peer_id: "peer-3".to_string(),
                device_name: Some("Renamed".to_string()),
                connected: false,
            })
            .unwrap(),
        })
        .unwrap();

    let peers_changed = next_json(&mut socket).await;
    let peers_name_updated = next_json(&mut socket).await;
    let peers_connection_changed = next_json(&mut socket).await;
    let paired_devices_changed = next_json(&mut socket).await;
    harness.handle.abort();

    assert_eq!(peers_changed["type"], "peers.changed");
    assert_eq!(peers_changed["payload"]["peerId"], "peer-3");
    assert_eq!(peers_changed["payload"]["deviceName"], "Desk");
    assert_eq!(
        peers_changed["payload"]["addresses"][0],
        "/ip4/127.0.0.1/tcp/7000"
    );
    assert_eq!(peers_changed["payload"]["discovered"], false);
    assert_eq!(peers_changed["payload"]["connected"], true);

    assert_eq!(peers_name_updated["type"], "peers.name_updated");
    assert_eq!(peers_name_updated["payload"]["peerId"], "peer-3");
    assert_eq!(peers_name_updated["payload"]["deviceName"], "Renamed");

    assert_eq!(peers_connection_changed["type"], "peers.connection_changed");
    assert_eq!(peers_connection_changed["payload"]["peerId"], "peer-3");
    assert_eq!(peers_connection_changed["payload"]["connected"], false);
    assert!(peers_connection_changed["payload"]
        .get("deviceName")
        .is_some());

    assert_eq!(paired_devices_changed["type"], "paired-devices.changed");
    assert_eq!(paired_devices_changed["payload"]["peerId"], "peer-3");
    assert_eq!(paired_devices_changed["payload"]["connected"], false);
}

#[tokio::test]
async fn websocket_event_uses_type_not_event_type() {
    let harness = spawn_server().await;
    let mut socket = connect_with_token(&harness.url, &harness.token).await;
    subscribe(&mut socket, &["pairing"]).await;
    let _snapshot = next_json(&mut socket).await;

    harness
        .event_tx
        .send(DaemonWsEvent {
            topic: "pairing".to_string(),
            event_type: "pairing.failed".to_string(),
            session_id: Some("session-3".to_string()),
            ts: 5,
            payload: serde_json::to_value(PairingFailurePayload {
                session_id: "session-3".to_string(),
                peer_id: Some("peer-3".to_string()),
                error: "transport_failed".to_string(),
            })
            .unwrap(),
        })
        .unwrap();

    let event = next_json(&mut socket).await;
    harness.handle.abort();

    assert_eq!(event["type"], "pairing.failed");
    assert!(event.get("event_type").is_none());
}

#[tokio::test]
async fn pairing_session_http_response_omits_verification_secrets_even_with_realtime_event() {
    let harness = spawn_server().await;
    upsert_pairing_snapshot(
        &harness.state,
        "session-4",
        Some("peer-4".to_string()),
        Some("Phone".to_string()),
        "verification",
        1_742_371_200_456,
    )
    .await;

    let mut socket = connect_with_token(&harness.url, &harness.token).await;
    subscribe(&mut socket, &["pairing"]).await;
    let _snapshot = next_json(&mut socket).await;

    harness
        .event_tx
        .send(DaemonWsEvent {
            topic: "pairing".to_string(),
            event_type: "pairing.verification_required".to_string(),
            session_id: Some("session-4".to_string()),
            ts: 6,
            payload: serde_json::to_value(PairingVerificationPayload {
                session_id: "session-4".to_string(),
                peer_id: "peer-4".to_string(),
                device_name: Some("Phone".to_string()),
                code: "654321".to_string(),
                local_fingerprint: "local-secret".to_string(),
                peer_fingerprint: "peer-secret".to_string(),
            })
            .unwrap(),
        })
        .unwrap();
    let _event = next_json(&mut socket).await;

    let response = harness
        .app
        .clone()
        .oneshot(authed_get_request(
            "/pairing/sessions/session-4",
            &harness.token,
        ))
        .await
        .unwrap();
    harness.handle.abort();

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["sessionId"], "session-4");
    assert!(json.get("code").is_none());
    assert!(json.get("localFingerprint").is_none());
    assert!(json.get("peerFingerprint").is_none());
    assert!(json.get("keyslotFile").is_none());
    assert!(json.get("challenge").is_none());
}
