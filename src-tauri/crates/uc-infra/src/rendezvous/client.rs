//! Rendezvous HTTP client — implements [`PairingInvitationPort`] for the
//! sponsor side of B1.
//!
//! Protocol contract lives in `findings.md#F-030`:
//! `POST {base}/v1/pairings` with the sponsor's device id / name / iroh
//! endpoint id / opaque ticket; response carries the short code + server
//! authoritative `expires_at_ms`. Server owns the TTL (default 300s) and
//! uniqueness; client just displays the returned code.
//!
//! The iroh `EndpointAddr` is serialized to JSON and sent as the opaque
//! `sponsorTicket` string — the rendezvous server doesn't parse it, joiners
//! deserialize it back on their side. This is the wire contract between
//! sponsor and joiner, not between client and rendezvous.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
#[cfg(test)]
use chrono::DateTime;
use chrono::{TimeZone, Utc};
use iroh::Endpoint;
use serde::{Deserialize, Serialize};
use tracing::{debug, instrument, warn};

use uc_core::pairing::invitation::InvitationCode;
use uc_core::ports::{
    ConsumeInvitationError, DeviceIdentityPort, InvitationError, IssuedInvitation,
    PairingInvitationPort, SettingsPort,
};

/// Production rendezvous service base URL.
pub const RENDEZVOUS_BASE_URL: &str = "https://rendezvous.uniclipboard.app";

/// Hard cap on rendezvous HTTP requests; the service is behind CF edge and
/// typically answers in < 200 ms, so a short timeout keeps `issue_invitation`
/// responsive when the service degrades.
const HTTP_TIMEOUT: Duration = Duration::from_secs(10);

/// Explicit user agent. `reqwest::Client::builder()` leaves the header
/// blank by default, which Cloudflare's bot-management layer at the
/// rendezvous edge 550-resets on some rules. Sending a stable identifier
/// makes the transport behave like `curl`/browser clients and is also
/// useful on the server side for telemetry.
const USER_AGENT: &str = concat!("uniclipboard-cli/", env!("CARGO_PKG_VERSION"));

/// Rendezvous-backed adapter for [`PairingInvitationPort`].
pub struct RendezvousPairingInvitationAdapter {
    endpoint: Arc<Endpoint>,
    device_identity: Arc<dyn DeviceIdentityPort>,
    settings: Arc<dyn SettingsPort>,
    base_url: String,
}

impl RendezvousPairingInvitationAdapter {
    /// Wire the adapter to the production rendezvous service.
    pub fn new(
        endpoint: Arc<Endpoint>,
        device_identity: Arc<dyn DeviceIdentityPort>,
        settings: Arc<dyn SettingsPort>,
    ) -> Self {
        Self {
            endpoint,
            device_identity,
            settings,
            base_url: RENDEZVOUS_BASE_URL.to_string(),
        }
    }

    /// Construct an adapter pointed at a custom rendezvous URL (mock
    /// server, staging instance, on-prem deployment). Production callers
    /// use [`Self::new`] which defaults to
    /// [`crate::rendezvous::RENDEZVOUS_BASE_URL`].
    pub fn with_base_url(
        endpoint: Arc<Endpoint>,
        device_identity: Arc<dyn DeviceIdentityPort>,
        settings: Arc<dyn SettingsPort>,
        base_url: String,
    ) -> Self {
        Self {
            endpoint,
            device_identity,
            settings,
            base_url,
        }
    }

    async fn resolve_device_name(&self) -> Result<String, InvitationError> {
        let settings = self
            .settings
            .load()
            .await
            .map_err(|err| InvitationError::Internal(format!("settings load failed: {err}")))?;
        settings
            .general
            .device_name
            .filter(|n| !n.trim().is_empty())
            .ok_or_else(|| {
                InvitationError::Internal(
                    "device_name missing from settings; user must set it before pairing"
                        .to_string(),
                )
            })
    }

    fn serialize_ticket(&self) -> Result<(String, String), InvitationError> {
        let addr = self.endpoint.addr();
        if addr.addrs.is_empty() {
            // No relay, no direct addrs — endpoint is bound but has no way
            // to be contacted. Surface as NetworkNotStarted so UI tells the
            // user to wait / retry.
            return Err(InvitationError::NetworkNotStarted);
        }
        let endpoint_id = addr.id.to_string();
        let ticket = serde_json::to_string(&addr)
            .map_err(|err| InvitationError::Internal(format!("endpoint addr serialize: {err}")))?;
        Ok((endpoint_id, ticket))
    }
}

#[async_trait]
impl PairingInvitationPort for RendezvousPairingInvitationAdapter {
    #[instrument(skip_all)]
    async fn issue_invitation(&self) -> Result<IssuedInvitation, InvitationError> {
        let (endpoint_id, ticket) = self.serialize_ticket()?;
        let device_name = self.resolve_device_name().await?;
        let device_id = self.device_identity.current_device_id();

        let body = CreatePairingRequest {
            sponsor_device_id: device_id.as_str().to_string(),
            sponsor_device_name: device_name,
            sponsor_endpoint_id: endpoint_id,
            sponsor_ticket: ticket,
            ttl_secs: None,
        };

        let url = format!("{}/v1/pairings", self.base_url.trim_end_matches('/'));
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(HTTP_TIMEOUT)
            .build()
            .map_err(|err| InvitationError::Internal(format!("http client build: {err}")))?;
        let resp = client.post(&url).json(&body).send().await.map_err(|err| {
            warn!(error = %err, chain = %err_chain(&err), "rendezvous request transport failed");
            InvitationError::ServiceUnavailable
        })?;

        let status = resp.status();
        if status.is_server_error() {
            warn!(status = %status, "rendezvous service 5xx");
            return Err(InvitationError::ServiceUnavailable);
        }
        if !status.is_success() {
            let err_env = resp.json::<RendezvousErrorEnvelope>().await.ok();
            let slug = err_env
                .and_then(|e| e.error.map(|d| d.code))
                .unwrap_or_else(|| "unknown".to_string());
            warn!(status = %status, slug = %slug, "rendezvous returned error");
            return Err(InvitationError::Internal(format!(
                "rendezvous rejected create request ({status}, slug={slug})"
            )));
        }

        let parsed: CreatePairingResponse = resp.json().await.map_err(|err| {
            InvitationError::Internal(format!("rendezvous response parse: {err}"))
        })?;
        let code = InvitationCode::new(parsed.code);
        let expires_at = Utc
            .timestamp_millis_opt(parsed.expires_at_ms)
            .single()
            .ok_or_else(|| {
                InvitationError::Internal(format!(
                    "rendezvous returned invalid expires_at_ms: {}",
                    parsed.expires_at_ms
                ))
            })?;

        debug!(%expires_at, "rendezvous invitation issued");
        Ok(IssuedInvitation { code, expires_at })
    }

    #[instrument(skip(self), fields(code = %code.as_str()))]
    async fn consume_invitation(
        &self,
        code: &InvitationCode,
    ) -> Result<(), ConsumeInvitationError> {
        // POST {base}/v1/pairings/consume with JSON body `{ "code": ... }` —
        // the server marks the entry terminal and subsequent resolves return
        // 404/409. The server trusts the transport-level sponsor identity
        // (F-030). Note: the path does NOT interpolate the code — that
        // variant of the API doesn't exist on the rendezvous service.
        let url = format!(
            "{}/v1/pairings/consume",
            self.base_url.trim_end_matches('/'),
        );
        let body = CodeRequest {
            code: code.as_str().to_string(),
        };
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(HTTP_TIMEOUT)
            .build()
            .map_err(|err| ConsumeInvitationError::Internal(format!("http client build: {err}")))?;
        let resp = client.post(&url).json(&body).send().await.map_err(|err| {
            warn!(error = %err, "rendezvous consume transport failed");
            ConsumeInvitationError::ServiceUnavailable
        })?;

        let status = resp.status();
        if status.is_success() {
            debug!("rendezvous invitation consumed");
            return Ok(());
        }
        match status.as_u16() {
            404 => {
                // Server groups `pairing_not_found` + `pairing_expired` under
                // 404; caller maps both as benign (already-terminal), so we
                // don't need to distinguish on the HTTP status alone.
                debug!("rendezvous returned 404 on consume (not found or expired)");
                Err(ConsumeInvitationError::NotFound)
            }
            409 => {
                // `pairing_already_consumed` — some other sponsor client
                // (or us on retry) already claimed it. Treat as benign,
                // identical to NotFound from the caller's perspective.
                debug!("rendezvous returned 409 on consume (already consumed)");
                Err(ConsumeInvitationError::NotFound)
            }
            s if (500..600).contains(&s) => {
                warn!(status = %status, "rendezvous service 5xx on consume");
                Err(ConsumeInvitationError::ServiceUnavailable)
            }
            _ => {
                let err_env = resp.json::<RendezvousErrorEnvelope>().await.ok();
                let slug = err_env
                    .and_then(|e| e.error.map(|d| d.code))
                    .unwrap_or_else(|| "unknown".to_string());
                warn!(status = %status, slug = %slug, "rendezvous returned unexpected status on consume");
                Err(ConsumeInvitationError::Internal(format!(
                    "rendezvous rejected consume ({status}, slug={slug})"
                )))
            }
        }
    }
}

// ── wire types ──────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CreatePairingRequest {
    sponsor_device_id: String,
    sponsor_device_name: String,
    sponsor_endpoint_id: String,
    sponsor_ticket: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    ttl_secs: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreatePairingResponse {
    code: String,
    expires_at_ms: i64,
}

/// Request body for `POST /v1/pairings/consume` and — used elsewhere —
/// `POST /v1/pairings/resolve`. The field name is unambiguously `code`
/// (no rename needed) on the server side.
#[derive(Debug, Serialize)]
struct CodeRequest {
    code: String,
}

#[derive(Debug, Deserialize)]
struct RendezvousErrorEnvelope {
    error: Option<RendezvousErrorDetail>,
}

#[derive(Debug, Deserialize)]
struct RendezvousErrorDetail {
    code: String,
}

// Helper for DateTime expires_at conversion — kept here so test asserts can
// reproduce it without depending on `chrono::Utc` directly.
#[cfg(test)]
fn utc_from_ms(ms: i64) -> DateTime<Utc> {
    Utc.timestamp_millis_opt(ms).single().unwrap()
}

/// Walk an error's `Error::source()` chain and render it into a single
/// `A -> B -> C` string. `reqwest::Error`'s `Display` intentionally hides
/// the inner cause (the whole point is "transport failed"), which makes
/// "error sending request for url" impossible to diagnose without the
/// underlying hyper / rustls / io error — this helper surfaces them.
fn err_chain(err: &dyn std::error::Error) -> String {
    let mut parts = vec![err.to_string()];
    let mut source = err.source();
    while let Some(s) = source {
        parts.push(s.to_string());
        source = s.source();
    }
    parts.join(" -> ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    use async_trait::async_trait;
    use serde_json::json;
    use uc_core::ids::DeviceId;
    use uc_core::settings::model::Settings;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    struct FakeDeviceIdentity(DeviceId);
    impl DeviceIdentityPort for FakeDeviceIdentity {
        fn current_device_id(&self) -> DeviceId {
            self.0.clone()
        }
    }

    struct InMemorySettings {
        inner: StdMutex<Settings>,
    }
    impl InMemorySettings {
        fn with_device_name(name: Option<&str>) -> Arc<Self> {
            let mut s = Settings::default();
            s.general.device_name = name.map(String::from);
            Arc::new(Self {
                inner: StdMutex::new(s),
            })
        }
    }
    #[async_trait]
    impl SettingsPort for InMemorySettings {
        async fn load(&self) -> anyhow::Result<Settings> {
            Ok(self.inner.lock().unwrap().clone())
        }
        async fn save(&self, s: &Settings) -> anyhow::Result<()> {
            *self.inner.lock().unwrap() = s.clone();
            Ok(())
        }
    }

    async fn loopback_endpoint() -> Arc<Endpoint> {
        // Loopback-only endpoint: no relay, no external discovery. That
        // leaves a single direct IPv4 address in `EndpointAddr.addrs`, which
        // is enough to satisfy `serialize_ticket`'s "addrs non-empty" guard.
        let ep = Endpoint::builder()
            .relay_mode(iroh::RelayMode::Disabled)
            .bind()
            .await
            .expect("bind loopback endpoint");
        Arc::new(ep)
    }

    fn make_adapter(
        endpoint: Arc<Endpoint>,
        settings: Arc<dyn SettingsPort>,
        base_url: String,
    ) -> RendezvousPairingInvitationAdapter {
        RendezvousPairingInvitationAdapter::with_base_url(
            endpoint,
            Arc::new(FakeDeviceIdentity(DeviceId::new("device-a"))),
            settings,
            base_url,
        )
    }

    #[tokio::test]
    async fn issue_invitation_happy_path() {
        let ep = loopback_endpoint().await;
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/pairings"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "code": "ABCD-EFGH",
                "expiresAtMs": 1_700_000_000_000_i64,
            })))
            .expect(1)
            .mount(&server)
            .await;

        let adapter = make_adapter(
            ep,
            InMemorySettings::with_device_name(Some("mac")),
            server.uri(),
        );
        let issued = adapter.issue_invitation().await.expect("happy path");

        assert_eq!(issued.code.as_str(), "ABCD-EFGH");
        assert_eq!(issued.expires_at, utc_from_ms(1_700_000_000_000));
    }

    #[tokio::test]
    async fn issue_invitation_includes_required_body_fields() {
        use wiremock::matchers::body_partial_json;

        let ep = loopback_endpoint().await;
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/pairings"))
            .and(body_partial_json(json!({
                "sponsorDeviceId": "device-a",
                "sponsorDeviceName": "mac",
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "code": "ABCD-EFGH",
                "expiresAtMs": 1_700_000_000_000_i64,
            })))
            .expect(1)
            .mount(&server)
            .await;

        let adapter = make_adapter(
            ep,
            InMemorySettings::with_device_name(Some("mac")),
            server.uri(),
        );
        adapter.issue_invitation().await.expect("body matches");
    }

    #[tokio::test]
    async fn issue_invitation_maps_5xx_to_service_unavailable() {
        let ep = loopback_endpoint().await;
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/pairings"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;

        let adapter = make_adapter(
            ep,
            InMemorySettings::with_device_name(Some("mac")),
            server.uri(),
        );
        let err = adapter.issue_invitation().await.unwrap_err();
        assert!(matches!(err, InvitationError::ServiceUnavailable));
    }

    #[tokio::test]
    async fn issue_invitation_maps_4xx_to_internal_with_slug() {
        let ep = loopback_endpoint().await;
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/pairings"))
            .respond_with(ResponseTemplate::new(400).set_body_json(json!({
                "error": { "code": "invalid_request" }
            })))
            .mount(&server)
            .await;

        let adapter = make_adapter(
            ep,
            InMemorySettings::with_device_name(Some("mac")),
            server.uri(),
        );
        let err = adapter.issue_invitation().await.unwrap_err();
        let msg = match err {
            InvitationError::Internal(m) => m,
            other => panic!("expected Internal, got {other:?}"),
        };
        assert!(msg.contains("invalid_request"), "msg was {msg}");
        assert!(msg.contains("400"));
    }

    #[tokio::test]
    async fn issue_invitation_maps_malformed_response_to_internal() {
        let ep = loopback_endpoint().await;
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/pairings"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not-json"))
            .mount(&server)
            .await;

        let adapter = make_adapter(
            ep,
            InMemorySettings::with_device_name(Some("mac")),
            server.uri(),
        );
        let err = adapter.issue_invitation().await.unwrap_err();
        assert!(
            matches!(err, InvitationError::Internal(ref m) if m.contains("parse")),
            "got {err:?}"
        );
    }

    #[tokio::test]
    async fn issue_invitation_maps_transport_failure_to_service_unavailable() {
        let ep = loopback_endpoint().await;
        // Point at a port guaranteed to reject — no server running there.
        let adapter = make_adapter(
            ep,
            InMemorySettings::with_device_name(Some("mac")),
            "http://127.0.0.1:1".to_string(),
        );
        let err = adapter.issue_invitation().await.unwrap_err();
        assert!(matches!(err, InvitationError::ServiceUnavailable));
    }

    #[tokio::test]
    async fn issue_invitation_rejects_missing_device_name() {
        let ep = loopback_endpoint().await;
        let server = MockServer::start().await;
        // Even if the server would accept, we should short-circuit before
        // sending — assert no request by leaving no `.mount(&server)`.
        let adapter = make_adapter(ep, InMemorySettings::with_device_name(None), server.uri());
        let err = adapter.issue_invitation().await.unwrap_err();
        let msg = match err {
            InvitationError::Internal(m) => m,
            other => panic!("expected Internal, got {other:?}"),
        };
        assert!(msg.contains("device_name"), "msg was {msg}");
    }

    // ── consume_invitation (P7e) ─────────────────────────────────────────

    #[tokio::test]
    async fn consume_invitation_happy_path_is_204_ok() {
        use wiremock::matchers::body_partial_json;
        let ep = loopback_endpoint().await;
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/pairings/consume"))
            .and(body_partial_json(json!({ "code": "ABCD-EFGH" })))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;

        let adapter = make_adapter(
            ep,
            InMemorySettings::with_device_name(Some("mac")),
            server.uri(),
        );
        adapter
            .consume_invitation(&InvitationCode::new("ABCD-EFGH"))
            .await
            .expect("consume happy path");
    }

    #[tokio::test]
    async fn consume_invitation_maps_404_to_not_found() {
        let ep = loopback_endpoint().await;
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/pairings/consume"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let adapter = make_adapter(
            ep,
            InMemorySettings::with_device_name(Some("mac")),
            server.uri(),
        );
        let err = adapter
            .consume_invitation(&InvitationCode::new("GONE-CODE"))
            .await
            .unwrap_err();
        assert!(matches!(err, ConsumeInvitationError::NotFound));
    }

    #[tokio::test]
    async fn consume_invitation_maps_409_to_not_found() {
        // Server returns 409 `pairing_already_consumed` when a second
        // sponsor client wins the race. Caller treats as benign, so we
        // collapse to NotFound like the existing expired branch.
        let ep = loopback_endpoint().await;
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/pairings/consume"))
            .respond_with(ResponseTemplate::new(409))
            .mount(&server)
            .await;

        let adapter = make_adapter(
            ep,
            InMemorySettings::with_device_name(Some("mac")),
            server.uri(),
        );
        let err = adapter
            .consume_invitation(&InvitationCode::new("OLD-CODE"))
            .await
            .unwrap_err();
        assert!(matches!(err, ConsumeInvitationError::NotFound));
    }

    #[tokio::test]
    async fn consume_invitation_maps_5xx_to_service_unavailable() {
        let ep = loopback_endpoint().await;
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/pairings/consume"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;

        let adapter = make_adapter(
            ep,
            InMemorySettings::with_device_name(Some("mac")),
            server.uri(),
        );
        let err = adapter
            .consume_invitation(&InvitationCode::new("BUSY-CODE"))
            .await
            .unwrap_err();
        assert!(matches!(err, ConsumeInvitationError::ServiceUnavailable));
    }

    #[tokio::test]
    async fn consume_invitation_maps_transport_failure_to_service_unavailable() {
        let ep = loopback_endpoint().await;
        let adapter = make_adapter(
            ep,
            InMemorySettings::with_device_name(Some("mac")),
            "http://127.0.0.1:1".to_string(),
        );
        let err = adapter
            .consume_invitation(&InvitationCode::new("ANY"))
            .await
            .unwrap_err();
        assert!(matches!(err, ConsumeInvitationError::ServiceUnavailable));
    }

    #[tokio::test]
    async fn consume_invitation_maps_other_4xx_to_internal_with_slug() {
        let ep = loopback_endpoint().await;
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/pairings/consume"))
            .respond_with(ResponseTemplate::new(400).set_body_json(json!({
                "error": { "code": "malformed_code" }
            })))
            .mount(&server)
            .await;

        let adapter = make_adapter(
            ep,
            InMemorySettings::with_device_name(Some("mac")),
            server.uri(),
        );
        let err = adapter
            .consume_invitation(&InvitationCode::new("WEIRD"))
            .await
            .unwrap_err();
        let msg = match err {
            ConsumeInvitationError::Internal(m) => m,
            other => panic!("expected Internal, got {other:?}"),
        };
        assert!(msg.contains("malformed_code"), "msg was {msg}");
        assert!(msg.contains("400"));
    }

    #[tokio::test]
    async fn issue_invitation_maps_invalid_expires_at_to_internal() {
        let ep = loopback_endpoint().await;
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/pairings"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "code": "ABCD-EFGH",
                "expiresAtMs": i64::MAX,  // out-of-range for chrono
            })))
            .mount(&server)
            .await;

        let adapter = make_adapter(
            ep,
            InMemorySettings::with_device_name(Some("mac")),
            server.uri(),
        );
        let err = adapter.issue_invitation().await.unwrap_err();
        assert!(
            matches!(err, InvitationError::Internal(ref m) if m.contains("expires_at_ms")),
            "got {err:?}"
        );
    }
}
