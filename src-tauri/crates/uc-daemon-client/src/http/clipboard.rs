use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use reqwest::Method;

use crate::http::authorized_daemon_request_with_type;
use crate::DaemonConnectionState;

#[derive(Clone)]
pub struct DaemonClipboardClient {
    http: Arc<reqwest::Client>,
    connection_state: DaemonConnectionState,
    client_type: String,
}

impl DaemonClipboardClient {
    pub fn new(connection_state: DaemonConnectionState) -> Self {
        Self {
            http: Arc::new(reqwest::Client::new()),
            connection_state,
            client_type: "gui".to_string(),
        }
    }

    pub(crate) fn with_http_conn_state_and_type(
        http: Arc<reqwest::Client>,
        connection_state: DaemonConnectionState,
        client_type: String,
    ) -> Self {
        Self {
            http,
            connection_state,
            client_type,
        }
    }

    /// Restore a clipboard entry to the OS clipboard via daemon.
    /// Returns Ok(()) on success. The daemon handles origin tracking; no outbound sync
    /// occurs because CaptureClipboardUseCase skips capture for LocalRestore origin.
    pub async fn restore_clipboard_entry(&self, entry_id: &str) -> Result<()> {
        let connection = self
            .connection_state
            .get()
            .ok_or_else(|| anyhow!("daemon connection info is not available"))?;
        let path = format!(
            "{}/{entry_id}",
            uc_daemon_contract::constants::http_route::CLIPBOARD_RESTORE
        );
        let request = authorized_daemon_request_with_type(
            &*self.http,
            &self.connection_state,
            Method::POST,
            &path,
            connection.pid,
            &self.client_type,
        )
        .await?;

        let response = request
            .send()
            .await
            .with_context(|| format!("failed to call daemon clipboard restore route {path}"))?;
        let status = response.status();

        if status.is_success() {
            return Ok(());
        }

        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "<failed to read body>".to_string());

        // Use structured error prefix so callers can distinguish 404 from other errors
        // without parsing free-form text (F-2 round 3).
        if status == reqwest::StatusCode::NOT_FOUND {
            Err(anyhow::anyhow!("[NOT_FOUND] {path}: {body}"))
        } else {
            Err(anyhow::anyhow!(
                "daemon clipboard restore request {path} failed with status {}: {}",
                status,
                body
            ))
        }
    }
}
