#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use uc_daemon::api::auth::DaemonConnectionInfo;
    use uc_daemon::api::types::{SetupActionAckResponse, SetupStateResponse};

    use crate::bootstrap::DaemonConnectionState;

    #[tokio::test]
    async fn daemon_setup_client_fetches_setup_state_from_daemon_api() {
        let expected = SetupStateResponse {
            state: serde_json::json!({
                "JoinSpaceSelectDevice": {
                    "deviceNames": []
                }
            }),
            session_id: Some("session-1".to_string()),
            next_step_hint: "join-select-peer".to_string(),
            profile: "default".to_string(),
            clipboard_mode: "full".to_string(),
            device_name: "Peer A".to_string(),
            peer_id: "peer-a".to_string(),
            selected_peer_id: None,
            selected_peer_name: None,
            has_completed: false,
        };

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr: SocketAddr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = vec![0u8; 2048];
            let size = stream.read(&mut request).await.unwrap();
            let request = String::from_utf8_lossy(&request[..size]);
            assert!(request.starts_with("GET /setup/state HTTP/1.1\r\n"));
            assert!(request.contains("authorization: Bearer test-token\r\n"));

            let body = serde_json::to_string(&expected).unwrap();
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        });

        let connection_state = DaemonConnectionState::default();
        connection_state.set(DaemonConnectionInfo {
            base_url: format!("http://{addr}"),
            ws_url: format!("ws://{addr}/ws"),
            token: "test-token".to_string(),
        });

        let client = super::TauriDaemonSetupClient::new(connection_state);
        let result = client.get_setup_state().await.unwrap();

        assert_eq!(result, expected);
    }

    #[tokio::test]
    async fn daemon_setup_client_posts_submit_passphrase_to_daemon_api() {
        let expected = SetupActionAckResponse {
            state: serde_json::json!({
                "JoinSpaceConfirmPeer": {
                    "shortCode": "123456"
                }
            }),
            session_id: Some("session-2".to_string()),
            next_step_hint: "host-confirm-peer".to_string(),
        };

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr: SocketAddr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = vec![0u8; 4096];
            let size = stream.read(&mut request).await.unwrap();
            let request = String::from_utf8_lossy(&request[..size]);
            assert!(request.starts_with("POST /setup/submit-passphrase HTTP/1.1\r\n"));
            assert!(request.contains("authorization: Bearer test-token\r\n"));
            assert!(request.contains("\r\n\r\n{\"passphrase\":\"secret-passphrase\"}"));

            let body = serde_json::to_string(&expected).unwrap();
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        });

        let connection_state = DaemonConnectionState::default();
        connection_state.set(DaemonConnectionInfo {
            base_url: format!("http://{addr}"),
            ws_url: format!("ws://{addr}/ws"),
            token: "test-token".to_string(),
        });

        let client = super::TauriDaemonSetupClient::new(connection_state);
        let result = client
            .submit_passphrase("secret-passphrase".to_string())
            .await
            .unwrap();

        assert_eq!(result, expected);
    }
}
