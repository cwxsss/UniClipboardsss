use anyhow::{anyhow, Context, Result};
use reqwest::{Method, RequestBuilder};

use crate::bootstrap::DaemonConnectionState;
use crate::daemon_client::authorized_daemon_request;
use uc_daemon::api::types::{PairedDeviceDto, PeerSnapshotDto};

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

    pub async fn get_paired_devices(&self) -> Result<Vec<PairedDeviceDto>> {
        self.get_json(Method::GET, "/paired-devices").await
    }

    fn authorized_request(&self, method: Method, path: &str) -> Result<RequestBuilder> {
        authorized_daemon_request(&self.http, &self.connection_state, method, path)
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
