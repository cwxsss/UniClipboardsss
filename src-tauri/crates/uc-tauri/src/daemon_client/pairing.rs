use anyhow::{anyhow, Context, Result};
use reqwest::header::AUTHORIZATION;
use reqwest::{Method, RequestBuilder};

use crate::bootstrap::DaemonConnectionState;
use uc_daemon::api::pairing::{
    AckedPairingCommandResponse, InitiatePairingRequest, InitiatePairingResponse,
    PairingApiErrorResponse, PairingGuiLeaseRequest, PairingSessionCommandRequest,
    SetPairingDiscoverabilityRequest, SetPairingParticipantRequest, VerifyPairingRequest,
};

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

    pub async fn initiate_pairing(&self, peer_id: String) -> Result<InitiatePairingResponse> {
        self.send_json(
            Method::POST,
            "/pairing/initiate",
            Some(&InitiatePairingRequest { peer_id }),
        )
        .await
    }

    pub async fn accept_pairing(&self, session_id: &str) -> Result<()> {
        self.send_json_no_content(
            Method::POST,
            "/pairing/accept",
            &PairingSessionCommandRequest {
                session_id: session_id.to_string(),
            },
        )
        .await
    }

    pub async fn reject_pairing(&self, session_id: &str) -> Result<()> {
        self.send_json_no_content(
            Method::POST,
            "/pairing/reject",
            &PairingSessionCommandRequest {
                session_id: session_id.to_string(),
            },
        )
        .await
    }

    pub async fn cancel_pairing(&self, session_id: &str) -> Result<()> {
        self.send_json_no_content(
            Method::POST,
            "/pairing/cancel",
            &PairingSessionCommandRequest {
                session_id: session_id.to_string(),
            },
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

    pub async fn register_gui_participant(&self, enabled: bool) -> Result<()> {
        self.send_json_no_content(
            Method::POST,
            "/pairing/gui/lease",
            &PairingGuiLeaseRequest { enabled },
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
        Ok(self
            .http
            .request(method, url)
            .header(AUTHORIZATION, format!("Bearer {}", connection.token)))
    }

    async fn send_json<TReq, TResp>(
        &self,
        method: Method,
        path: &str,
        payload: Option<&TReq>,
    ) -> Result<TResp>
    where
        TReq: serde::Serialize + ?Sized,
        TResp: serde::de::DeserializeOwned,
    {
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

        Self::decode_json_response(response, path).await
    }

    async fn send_json_no_content<T: serde::Serialize + ?Sized>(
        &self,
        method: Method,
        path: &str,
        payload: &T,
    ) -> Result<()> {
        let request = self.authorized_request(method, path)?;
        let response = request
            .json(payload)
            .send()
            .await
            .with_context(|| format!("failed to call daemon pairing route {path}"))?;

        Self::decode_no_content_response(response, path).await
    }

    async fn decode_json_response<T: serde::de::DeserializeOwned>(
        response: reqwest::Response,
        path: &str,
    ) -> Result<T> {
        let status = response.status();
        if status.is_success() {
            return response
                .json::<T>()
                .await
                .with_context(|| format!("failed to decode daemon pairing response for {path}"));
        }

        Err(Self::decode_error_response(response, path).await)
    }

    async fn decode_no_content_response(response: reqwest::Response, path: &str) -> Result<()> {
        let status = response.status();
        if status.is_success() {
            return Ok(());
        }

        Err(Self::decode_error_response(response, path).await)
    }

    async fn decode_error_response(response: reqwest::Response, path: &str) -> anyhow::Error {
        let status = response.status();
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "<unreadable response body>".to_string());
        let maybe_error = serde_json::from_str::<PairingApiErrorResponse>(&body).ok();
        if let Some(error) = maybe_error {
            return anyhow!(
                "daemon pairing request {} failed with status {} [{}]: {}",
                path,
                status,
                error.code,
                error.message
            );
        }

        anyhow!(
            "daemon pairing request {} failed with status {}: {}",
            path,
            status,
            body
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uc_daemon::api::auth::DaemonConnectionInfo;

    #[test]
    fn authorized_request_builds_bearer_header() {
        let connection_state = DaemonConnectionState::default();
        connection_state.set(DaemonConnectionInfo {
            base_url: "http://127.0.0.1:42715".to_string(),
            ws_url: "ws://127.0.0.1:42715/ws".to_string(),
            token: "test-token".to_string(),
        });
        let client = TauriDaemonPairingClient::new(connection_state);

        let request = client
            .authorized_request(Method::POST, "/pairing/initiate")
            .expect("request should build")
            .build()
            .expect("request should be valid");

        let auth_header = request
            .headers()
            .get(AUTHORIZATION)
            .expect("authorization header should exist")
            .to_str()
            .expect("authorization header should be utf-8");
        assert_eq!(auth_header, "Bearer test-token");
    }
}
