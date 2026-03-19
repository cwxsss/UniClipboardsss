use anyhow::{anyhow, Context, Result};
use reqwest::header::AUTHORIZATION;
use reqwest::{Method, RequestBuilder};

use crate::bootstrap::DaemonConnectionState;
use uc_daemon::api::pairing::{
    AckedPairingCommandResponse, InitiatePairingRequest, SetPairingDiscoverabilityRequest,
    SetPairingParticipantRequest, VerifyPairingRequest,
};

const AUTHORIZATION_HEADER_NAME: &str = "Authorization";

#[derive(Clone)]
pub struct TauriDaemonPairingClient {
    http: reqwest::Client,
    connection_state: DaemonConnectionState,
}

impl TauriDaemonPairingClient {
    pub fn new(connection_state: DaemonConnectionState) -> Self {
        Self {
            http: reqwest::Client::new(),
            connection_state,
        }
    }

    pub async fn initiate_pairing(&self, peer_id: String) -> Result<AckedPairingCommandResponse> {
        self.send_json(
            Method::POST,
            "/pairing/sessions",
            Some(&InitiatePairingRequest { peer_id }),
        )
        .await
    }

    pub async fn accept_pairing(&self, session_id: &str) -> Result<AckedPairingCommandResponse> {
        self.send_empty(
            Method::POST,
            &format!("/pairing/sessions/{session_id}/accept"),
        )
        .await
    }

    pub async fn reject_pairing(&self, session_id: &str) -> Result<AckedPairingCommandResponse> {
        self.send_empty(
            Method::POST,
            &format!("/pairing/sessions/{session_id}/reject"),
        )
        .await
    }

    pub async fn cancel_pairing(&self, session_id: &str) -> Result<AckedPairingCommandResponse> {
        self.send_empty(
            Method::POST,
            &format!("/pairing/sessions/{session_id}/cancel"),
        )
        .await
    }

    pub async fn verify_pairing(
        &self,
        session_id: &str,
        pin_matches: bool,
    ) -> Result<AckedPairingCommandResponse> {
        self.send_json(
            Method::POST,
            &format!("/pairing/sessions/{session_id}/verify"),
            Some(&VerifyPairingRequest { pin_matches }),
        )
        .await
    }

    pub async fn set_pairing_discoverability(
        &self,
        client_kind: &str,
        discoverable: bool,
        lease_ttl_ms: Option<u64>,
    ) -> Result<AckedPairingCommandResponse> {
        self.send_json(
            Method::PUT,
            "/pairing/discoverability/current",
            Some(&SetPairingDiscoverabilityRequest {
                client_kind: client_kind.to_string(),
                discoverable,
                lease_ttl_ms,
            }),
        )
        .await
    }

    pub async fn set_pairing_participant_ready(
        &self,
        client_kind: &str,
        ready: bool,
        lease_ttl_ms: Option<u64>,
    ) -> Result<AckedPairingCommandResponse> {
        self.send_json(
            Method::PUT,
            "/pairing/participants/current",
            Some(&SetPairingParticipantRequest {
                client_kind: client_kind.to_string(),
                ready,
                lease_ttl_ms,
            }),
        )
        .await
    }

    fn authorized_request(&self, method: Method, path: &str) -> Result<RequestBuilder> {
        let connection = self
            .connection_state
            .get()
            .ok_or_else(|| anyhow!("daemon connection info is not available"))?;
        let url = format!("{}{}", connection.base_url, path);
        debug_assert_eq!(AUTHORIZATION_HEADER_NAME, AUTHORIZATION.as_str());
        Ok(self
            .http
            .request(method, url)
            .header(AUTHORIZATION, format!("Bearer {}", connection.token)))
    }

    async fn send_empty(&self, method: Method, path: &str) -> Result<AckedPairingCommandResponse> {
        let response = self
            .authorized_request(method, path)?
            .send()
            .await
            .with_context(|| format!("failed to call daemon pairing route {path}"))?;

        Self::decode_response(response, path).await
    }

    async fn send_json<T: serde::Serialize + ?Sized>(
        &self,
        method: Method,
        path: &str,
        payload: Option<&T>,
    ) -> Result<AckedPairingCommandResponse> {
        let request = self.authorized_request(method, path)?;
        let request = if let Some(payload) = payload {
            request.json(payload)
        } else {
            request
        };

        let response = request
            .send()
            .await
            .with_context(|| format!("failed to call daemon pairing route {path}"))?;

        Self::decode_response(response, path).await
    }

    async fn decode_response(
        response: reqwest::Response,
        path: &str,
    ) -> Result<AckedPairingCommandResponse> {
        let status = response.status();
        if status.is_success() {
            return response
                .json::<AckedPairingCommandResponse>()
                .await
                .with_context(|| format!("failed to decode daemon pairing response for {path}"));
        }

        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "<unreadable response body>".to_string());
        Err(anyhow!(
            "daemon pairing request {} failed with status {}: {}",
            path,
            status,
            body
        ))
    }
}
