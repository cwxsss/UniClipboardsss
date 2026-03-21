use anyhow::{anyhow, Context, Result};
use reqwest::header::AUTHORIZATION;
use reqwest::{Method, RequestBuilder};

use crate::bootstrap::DaemonConnectionState;
use uc_daemon::api::types::PeerSnapshotDto;

#[derive(Clone)]
pub struct TauriDaemonQueryClient {
    http: reqwest::Client,
    connection_state: DaemonConnectionState,
}

impl TauriDaemonQueryClient {
    pub fn new(connection_state: DaemonConnectionState) -> Self {
        Self {
            http: reqwest::Client::new(),
            connection_state,
        }
    }

    pub async fn get_peers(&self) -> Result<Vec<PeerSnapshotDto>> {
        self.get_json(Method::GET, "/peers").await
    }

    fn authorized_request(&self, method: Method, path: &str) -> Result<RequestBuilder> {
        let connection = self
            .connection_state
            .get()
            .ok_or_else(|| anyhow!("daemon connection info is not available"))?;
        let url = format!("{}{}", connection.base_url, path);
        Ok(self
            .http
            .request(method, url)
            .header(AUTHORIZATION, format!("Bearer {}", connection.token)))
    }

    async fn get_json<T>(&self, method: Method, path: &str) -> Result<T>
    where
        T: serde::de::DeserializeOwned,
    {
        let response = self
            .authorized_request(method, path)?
            .send()
            .await
            .with_context(|| format!("failed to call daemon query route {path}"))?;

        let status = response.status();
        if status.is_success() {
            return response
                .json::<T>()
                .await
                .with_context(|| format!("failed to decode daemon query response for {path}"));
        }

        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "<failed to read body>".to_string());
        Err(anyhow!(
            "daemon query request {path} failed with status {}: {}",
            status,
            body
        ))
    }
}
