use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::MockRuntime;
use tauri::test::{get_ipc_response, mock_builder, mock_context, noop_assets, INVOKE_KEY};
use tauri::webview::{InvokeRequest, WebviewWindowBuilder};
use tauri::Listener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use uc_daemon::api::auth::DaemonConnectionInfo;
use uc_tauri::bootstrap::DaemonConnectionState;
use uc_tauri::commands::pairing::PairedPeer;

#[derive(Debug, Clone, PartialEq, Eq)]
struct CapturedRequest {
    method: String,
    path: String,
    body: String,
}

fn invoke_command<T: DeserializeOwned>(
    webview: &tauri::WebviewWindow<MockRuntime>,
    cmd: &str,
    body: Value,
) -> Result<T, Value> {
    get_ipc_response(
        webview,
        InvokeRequest {
            cmd: cmd.to_string(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: "http://tauri.localhost".parse().expect("valid url"),
            body: InvokeBody::Json(body),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        },
    )
    .map(|response| response.deserialize::<T>().expect("deserialize response"))
}

async fn spawn_pairing_shell_server() -> (SocketAddr, Arc<Mutex<Vec<CapturedRequest>>>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind fake daemon");
    let addr = listener.local_addr().expect("local addr");
    let requests = Arc::new(Mutex::new(Vec::new()));
    let captured = requests.clone();

    tokio::spawn(async move {
        for index in 0..3 {
            let (mut stream, _) = listener.accept().await.expect("accept request");
            let mut buffer = vec![0u8; 4096];
            let size = stream.read(&mut buffer).await.expect("read request");
            let request = String::from_utf8_lossy(&buffer[..size]).to_string();

            let mut lines = request.lines();
            let first_line = lines.next().expect("request line");
            let mut parts = first_line.split_whitespace();
            let method = parts.next().expect("request method").to_string();
            let path = parts.next().expect("request path").to_string();
            let body = request
                .split("\r\n\r\n")
                .nth(1)
                .unwrap_or_default()
                .to_string();

            captured
                .lock()
                .expect("lock captured requests")
                .push(CapturedRequest { method, path, body });

            let response = match index {
                0 | 1 => {
                    let body = serde_json::to_string(&json!([
                        {
                            "peerId": "peer-a",
                            "deviceName": "Peer A",
                            "pairingState": "Trusted",
                            "lastSeenAtMs": 1_704_067_200_000_i64,
                            "connected": true
                        }
                    ]))
                    .expect("serialize paired devices");
                    format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                        body.len(),
                        body
                    )
                }
                2 => "HTTP/1.1 204 No Content\r\ncontent-length: 0\r\n\r\n".to_string(),
                _ => unreachable!("unexpected request index"),
            };

            stream
                .write_all(response.as_bytes())
                .await
                .expect("write response");
        }
    });

    (addr, requests)
}

#[tokio::test(flavor = "multi_thread")]
async fn pairing_commands_use_daemon_queries_and_unpair_route() {
    let (addr, captured_requests) = spawn_pairing_shell_server().await;
    let daemon_connection = DaemonConnectionState::default();
    daemon_connection.set(DaemonConnectionInfo {
        base_url: format!("http://{addr}"),
        ws_url: format!("ws://{addr}/ws"),
        token: "test-token".to_string(),
    });

    let app = mock_builder()
        .manage(daemon_connection)
        .invoke_handler(tauri::generate_handler![
            uc_tauri::commands::pairing::list_paired_devices,
            uc_tauri::commands::pairing::get_paired_peers_with_status,
            uc_tauri::commands::pairing::unpair_p2p_device
        ])
        .build(mock_context(noop_assets()))
        .expect("build mock app");
    let webview = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("build webview");

    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    app.handle().listen("p2p-command-error", move |event| {
        let _ = event_tx.send(event.payload().to_string());
    });

    let listed: Vec<PairedPeer> =
        invoke_command(&webview, "list_paired_devices", json!({})).expect("list paired devices");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].peer_id, "peer-a");
    assert_eq!(listed[0].device_name, "Peer A");
    assert!(listed[0].shared_secret.is_empty());
    assert_eq!(listed[0].paired_at, "");
    assert_eq!(listed[0].last_known_addresses, Vec::<String>::new());
    assert_eq!(
        listed[0].last_seen.as_deref(),
        Some("2024-01-01T00:00:00+00:00")
    );
    assert!(listed[0].connected);

    let with_status: Vec<PairedPeer> =
        invoke_command(&webview, "get_paired_peers_with_status", json!({}))
            .expect("get paired peers with status");
    assert_eq!(with_status.len(), listed.len());
    assert_eq!(with_status[0].peer_id, listed[0].peer_id);
    assert_eq!(with_status[0].device_name, listed[0].device_name);
    assert_eq!(with_status[0].last_seen, listed[0].last_seen);
    assert_eq!(with_status[0].connected, listed[0].connected);

    let unpair_result: Result<(), Value> =
        invoke_command(&webview, "unpair_p2p_device", json!({ "peerId": "peer-a" }));
    unpair_result.expect("unpair device");

    let requests = captured_requests
        .lock()
        .expect("lock captured requests")
        .clone();
    assert_eq!(
        requests,
        vec![
            CapturedRequest {
                method: "GET".to_string(),
                path: "/paired-devices".to_string(),
                body: String::new(),
            },
            CapturedRequest {
                method: "GET".to_string(),
                path: "/paired-devices".to_string(),
                body: String::new(),
            },
            CapturedRequest {
                method: "POST".to_string(),
                path: "/pairing/unpair".to_string(),
                body: "{\"peerId\":\"peer-a\"}".to_string(),
            },
        ]
    );

    assert!(
        tokio::time::timeout(Duration::from_millis(50), event_rx.recv())
            .await
            .is_err(),
        "legacy p2p-command-error event should not be emitted"
    );
}
