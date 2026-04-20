//! Iroh-backed implementation of [`PairingSessionPort`].
//!
//! Joiner side (P7c.2) is the focus of this module:
//!
//! 1. `dial_by_invitation(code)` calls the rendezvous service's resolve
//!    endpoint (`GET {base}/v1/pairings/{code}`), deserializes the opaque
//!    sponsor ticket into an iroh [`EndpointAddr`], dials the sponsor with
//!    ALPN [`PAIRING_ALPN`], and opens a bi-directional stream.
//! 2. `send` / `recv_next` ride the stream with a 4-byte big-endian length
//!    prefix followed by a postcard-encoded [`PairingSessionMessage`] (see
//!    [`super::wire`]).
//! 3. `close` releases the stored session.
//!
//! Sponsor-side ALPN handler + [`PairingEventPort`] implementation lives in
//! P7c.3 (same struct, additional trait impl).
//!
//! [`PairingSessionPort`]: uc_core::ports::pairing::PairingSessionPort
//! [`EndpointAddr`]: iroh::EndpointAddr
//! [`PairingSessionMessage`]: uc_core::pairing::PairingSessionMessage

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use iroh::endpoint::{RecvStream, SendStream};
use iroh::{Endpoint, EndpointAddr};
use reqwest::StatusCode;
use serde::Deserialize;
use tokio::sync::Mutex;
use tracing::{debug, instrument};

use uc_core::pairing::{InvitationCode, PairingSessionMessage};
use uc_core::ports::pairing::{DialError, PairingSessionId, PairingSessionPort, SessionError};

use super::wire::{self, WireDecodeError};
use crate::rendezvous::RENDEZVOUS_BASE_URL;

/// ALPN identifier for the Slice 1 pairing protocol (F-014).
pub const PAIRING_ALPN: &[u8] = b"/uniclipboard/pairing/1";

const FRAME_LEN_BYTES: usize = 4;

// ============================================================================
// Adapter
// ============================================================================

/// Iroh-backed pairing session adapter.
pub struct IrohPairingSessionAdapter {
    endpoint: Arc<Endpoint>,
    base_url: String,
    sessions: Mutex<HashMap<PairingSessionId, Arc<SessionSlot>>>,
    next_session_seq: AtomicU64,
}

struct SessionSlot {
    send: Mutex<SendStream>,
    recv: Mutex<RecvStream>,
    // Hold the connection so it stays alive for the session's lifetime.
    _connection: iroh::endpoint::Connection,
}

impl IrohPairingSessionAdapter {
    pub fn new(endpoint: Arc<Endpoint>) -> Self {
        Self {
            endpoint,
            base_url: RENDEZVOUS_BASE_URL.to_string(),
            sessions: Mutex::new(HashMap::new()),
            next_session_seq: AtomicU64::new(0),
        }
    }

    /// Test-only override for the rendezvous base URL.
    #[cfg(test)]
    pub(crate) fn with_base_url(endpoint: Arc<Endpoint>, base_url: impl Into<String>) -> Self {
        Self {
            endpoint,
            base_url: base_url.into(),
            sessions: Mutex::new(HashMap::new()),
            next_session_seq: AtomicU64::new(0),
        }
    }

    fn mint_session_id(&self) -> PairingSessionId {
        let seq = self.next_session_seq.fetch_add(1, Ordering::Relaxed);
        PairingSessionId::new(format!("{}:{seq}", self.endpoint.id().fmt_short()))
    }

    async fn resolve_invitation(&self, code: &InvitationCode) -> Result<EndpointAddr, DialError> {
        let url = format!("{}/v1/pairings/{}", self.base_url, code.as_str());
        let response = reqwest::Client::new()
            .get(&url)
            .send()
            .await
            .map_err(|err| {
                debug!(error = %err, "rendezvous resolve transport failure");
                DialError::ServiceUnavailable
            })?;

        let status = response.status();
        match status {
            StatusCode::OK => {}
            StatusCode::NOT_FOUND => return Err(DialError::InvitationNotFound),
            StatusCode::GONE => return Err(DialError::InvitationExpired),
            s if s.is_server_error() => return Err(DialError::ServiceUnavailable),
            s => {
                let body = response.text().await.unwrap_or_default();
                return Err(DialError::Internal(format!(
                    "rendezvous resolve: status {s} body={body}"
                )));
            }
        }

        let body: ResolveResponse = response
            .json()
            .await
            .map_err(|err| DialError::Internal(format!("rendezvous resolve parse: {err}")))?;

        serde_json::from_str::<EndpointAddr>(&body.sponsor_ticket)
            .map_err(|err| DialError::Internal(format!("sponsor ticket decode: {err}")))
    }

    /// Install a ready-built session into the map and return the minted id.
    /// Shared between the real `dial_by_invitation` path and sponsor-side
    /// accept (P7c.3).
    pub(crate) async fn register_session(
        &self,
        connection: iroh::endpoint::Connection,
        send: SendStream,
        recv: RecvStream,
    ) -> PairingSessionId {
        let id = self.mint_session_id();
        let slot = Arc::new(SessionSlot {
            send: Mutex::new(send),
            recv: Mutex::new(recv),
            _connection: connection,
        });
        self.sessions.lock().await.insert(id.clone(), slot);
        id
    }

    async fn session(&self, id: &PairingSessionId) -> Result<Arc<SessionSlot>, SessionError> {
        self.sessions
            .lock()
            .await
            .get(id)
            .cloned()
            .ok_or_else(|| SessionError::NotFound(id.clone()))
    }
}

#[async_trait]
impl PairingSessionPort for IrohPairingSessionAdapter {
    #[instrument(skip_all, fields(code = %code.as_str()))]
    async fn dial_by_invitation(
        &self,
        code: &InvitationCode,
    ) -> Result<PairingSessionId, DialError> {
        let sponsor_addr = self.resolve_invitation(code).await?;

        let connection = self
            .endpoint
            .connect(sponsor_addr, PAIRING_ALPN)
            .await
            .map_err(|err| {
                debug!(error = %err, "iroh connect failed");
                DialError::SponsorUnreachable
            })?;

        let (send, recv) = connection
            .open_bi()
            .await
            .map_err(|err| DialError::Internal(format!("open_bi failed: {err}")))?;

        Ok(self.register_session(connection, send, recv).await)
    }

    #[instrument(skip_all, fields(session = %session))]
    async fn send(
        &self,
        session: &PairingSessionId,
        message: PairingSessionMessage,
    ) -> Result<(), SessionError> {
        let slot = self.session(session).await?;
        let payload = wire::encode(&message)
            .map_err(|err| SessionError::Internal(format!("wire encode: {err}")))?;
        let len: u32 = payload
            .len()
            .try_into()
            .map_err(|_| SessionError::Internal(format!("payload too large: {}", payload.len())))?;

        let mut send = slot.send.lock().await;
        send.write_all(&len.to_be_bytes())
            .await
            .map_err(map_write_err)?;
        send.write_all(&payload).await.map_err(map_write_err)?;
        Ok(())
    }

    #[instrument(skip_all, fields(session = %session))]
    async fn recv_next(
        &self,
        session: &PairingSessionId,
    ) -> Result<Option<PairingSessionMessage>, SessionError> {
        let slot = self.session(session).await?;
        let mut recv = slot.recv.lock().await;

        let mut len_buf = [0u8; FRAME_LEN_BYTES];
        match recv.read_exact(&mut len_buf).await {
            Ok(()) => {}
            Err(iroh::endpoint::ReadExactError::FinishedEarly(0)) => return Ok(None),
            Err(err) => return Err(map_read_err(err)),
        }
        let len = u32::from_be_bytes(len_buf) as usize;

        let mut payload = vec![0u8; len];
        recv.read_exact(&mut payload).await.map_err(map_read_err)?;

        wire::decode(&payload).map(Some).map_err(|err| match err {
            WireDecodeError::Postcard(_)
            | WireDecodeError::UnsupportedVersion { .. }
            | WireDecodeError::InvalidFingerprint(_) => {
                SessionError::Internal(format!("wire decode: {err}"))
            }
        })
    }

    #[instrument(skip_all, fields(session = %session))]
    async fn close(&self, session: &PairingSessionId, reason: Option<String>) {
        let mut map = self.sessions.lock().await;
        if let Some(slot) = map.remove(session) {
            // Try to half-close the send side so the peer sees EOF.
            if let Ok(mut send) = slot.send.try_lock() {
                let _ = send.finish();
            }
            debug!(?reason, "pairing session closed");
        }
    }
}

// ============================================================================
// Error mappers
// ============================================================================

fn map_write_err(err: iroh::endpoint::WriteError) -> SessionError {
    use iroh::endpoint::WriteError;
    match err {
        WriteError::ClosedStream | WriteError::Stopped(_) => SessionError::Closed,
        other => SessionError::Internal(format!("stream write: {other}")),
    }
}

fn map_read_err(err: iroh::endpoint::ReadExactError) -> SessionError {
    use iroh::endpoint::{ReadError, ReadExactError};
    match err {
        ReadExactError::FinishedEarly(_) => SessionError::Closed,
        ReadExactError::ReadError(ReadError::ClosedStream)
        | ReadExactError::ReadError(ReadError::Reset(_)) => SessionError::Closed,
        other => SessionError::Internal(format!("stream read: {other}")),
    }
}

// ============================================================================
// Wire types (rendezvous resolve response)
// ============================================================================

#[derive(Debug, Deserialize)]
struct ResolveResponse {
    #[serde(rename = "sponsorTicket")]
    sponsor_ticket: String,
    #[serde(rename = "sponsorEndpointId", default)]
    _sponsor_endpoint_id: Option<String>,
    #[serde(rename = "expiresAtMs", default)]
    _expires_at_ms: Option<i64>,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use iroh::RelayMode;
    use tokio::task::JoinHandle;
    use uc_core::ids::DeviceId;
    use uc_core::pairing::JoinerRequest;
    use uc_core::security::IdentityFingerprint;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn bound_endpoint() -> Arc<Endpoint> {
        Arc::new(
            Endpoint::builder()
                .alpns(vec![PAIRING_ALPN.to_vec()])
                .relay_mode(RelayMode::Disabled)
                .bind()
                .await
                .expect("bind endpoint"),
        )
    }

    async fn wait_for_direct_addrs(endpoint: &Endpoint) {
        // Give the magicsock a beat to publish local direct addresses.
        for _ in 0..50 {
            if !endpoint.addr().addrs.is_empty() {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }

    /// Spawn a sponsor-side loopback that accepts the first incoming bi-stream
    /// and echoes framed bytes back. Returns the join handle so the test can
    /// drop it after assertions.
    fn spawn_echo_sponsor(endpoint: Arc<Endpoint>) -> JoinHandle<()> {
        tokio::spawn(async move {
            while let Some(incoming) = endpoint.accept().await {
                let connection = match incoming.await {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                tokio::spawn(async move {
                    while let Ok((mut send, mut recv)) = connection.accept_bi().await {
                        loop {
                            let mut len_buf = [0u8; FRAME_LEN_BYTES];
                            if recv.read_exact(&mut len_buf).await.is_err() {
                                break;
                            }
                            let len = u32::from_be_bytes(len_buf) as usize;
                            let mut payload = vec![0u8; len];
                            if recv.read_exact(&mut payload).await.is_err() {
                                break;
                            }
                            if send.write_all(&len_buf).await.is_err() {
                                break;
                            }
                            if send.write_all(&payload).await.is_err() {
                                break;
                            }
                        }
                    }
                });
            }
        })
    }

    async fn mock_resolve(server: &MockServer, code: &str, ticket: String) {
        let body = serde_json::json!({
            "sponsorTicket": ticket,
            "sponsorEndpointId": "ignored-for-tests",
            "expiresAtMs": 0,
        });
        Mock::given(method("GET"))
            .and(path(format!("/v1/pairings/{code}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(server)
            .await;
    }

    fn sample_fingerprint() -> IdentityFingerprint {
        IdentityFingerprint::from_raw_string("ABCDEFGHIJKLMNOP").unwrap()
    }

    fn sample_request() -> PairingSessionMessage {
        PairingSessionMessage::Request(JoinerRequest {
            invitation_code: InvitationCode::new("CODE-9999"),
            device_id: DeviceId::new("joiner-1"),
            device_name: "Joiner".into(),
            identity_fingerprint: sample_fingerprint(),
            nonce: vec![7; 8],
        })
    }

    #[tokio::test]
    async fn dial_send_recv_close_round_trip() {
        let sponsor_endpoint = bound_endpoint().await;
        wait_for_direct_addrs(&sponsor_endpoint).await;
        let sponsor_addr = sponsor_endpoint.addr();
        let ticket = serde_json::to_string(&sponsor_addr).unwrap();
        let _echo = spawn_echo_sponsor(sponsor_endpoint.clone());

        let rendezvous = MockServer::start().await;
        mock_resolve(&rendezvous, "CODE-9999", ticket).await;

        let joiner_endpoint = bound_endpoint().await;
        wait_for_direct_addrs(&joiner_endpoint).await;
        let adapter = IrohPairingSessionAdapter::with_base_url(joiner_endpoint, &rendezvous.uri());

        let session = adapter
            .dial_by_invitation(&InvitationCode::new("CODE-9999"))
            .await
            .expect("dial");

        let msg = sample_request();
        adapter.send(&session, msg.clone()).await.expect("send");

        let echoed = adapter
            .recv_next(&session)
            .await
            .expect("recv")
            .expect("message");
        match (msg, echoed) {
            (PairingSessionMessage::Request(a), PairingSessionMessage::Request(b)) => {
                assert_eq!(a.invitation_code.as_str(), b.invitation_code.as_str());
                assert_eq!(a.device_id.as_str(), b.device_id.as_str());
                assert_eq!(a.identity_fingerprint, b.identity_fingerprint);
                assert_eq!(a.nonce, b.nonce);
            }
            (a, b) => panic!("variant mismatch: {a:?} vs {b:?}"),
        }

        adapter.close(&session, Some("done".into())).await;

        match adapter.send(&session, sample_request()).await {
            Err(SessionError::NotFound(id)) => assert_eq!(id.as_str(), session.as_str()),
            other => panic!("expected NotFound after close, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn dial_maps_404_to_invitation_not_found() {
        let rendezvous = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/pairings/UNKNOWN"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&rendezvous)
            .await;

        let endpoint = bound_endpoint().await;
        let adapter = IrohPairingSessionAdapter::with_base_url(endpoint, &rendezvous.uri());

        match adapter
            .dial_by_invitation(&InvitationCode::new("UNKNOWN"))
            .await
        {
            Err(DialError::InvitationNotFound) => {}
            other => panic!("expected InvitationNotFound, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn dial_maps_410_to_invitation_expired() {
        let rendezvous = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/pairings/STALE"))
            .respond_with(ResponseTemplate::new(410))
            .mount(&rendezvous)
            .await;

        let endpoint = bound_endpoint().await;
        let adapter = IrohPairingSessionAdapter::with_base_url(endpoint, &rendezvous.uri());

        match adapter
            .dial_by_invitation(&InvitationCode::new("STALE"))
            .await
        {
            Err(DialError::InvitationExpired) => {}
            other => panic!("expected InvitationExpired, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn dial_maps_5xx_to_service_unavailable() {
        let rendezvous = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/pairings/BUSY"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&rendezvous)
            .await;

        let endpoint = bound_endpoint().await;
        let adapter = IrohPairingSessionAdapter::with_base_url(endpoint, &rendezvous.uri());

        match adapter
            .dial_by_invitation(&InvitationCode::new("BUSY"))
            .await
        {
            Err(DialError::ServiceUnavailable) => {}
            other => panic!("expected ServiceUnavailable, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn dial_maps_bad_ticket_to_internal() {
        let rendezvous = MockServer::start().await;
        let bad_body = serde_json::json!({
            "sponsorTicket": "not-valid-json",
            "sponsorEndpointId": "x",
            "expiresAtMs": 0,
        });
        Mock::given(method("GET"))
            .and(path("/v1/pairings/BADTICKET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(bad_body))
            .mount(&rendezvous)
            .await;

        let endpoint = bound_endpoint().await;
        let adapter = IrohPairingSessionAdapter::with_base_url(endpoint, &rendezvous.uri());

        match adapter
            .dial_by_invitation(&InvitationCode::new("BADTICKET"))
            .await
        {
            Err(DialError::Internal(msg)) => assert!(msg.contains("sponsor ticket decode")),
            other => panic!("expected Internal, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn send_on_unknown_session_returns_not_found() {
        let endpoint = bound_endpoint().await;
        let adapter = IrohPairingSessionAdapter::new(endpoint);
        let ghost = PairingSessionId::new("no-such-session");

        match adapter.send(&ghost, sample_request()).await {
            Err(SessionError::NotFound(id)) => assert_eq!(id.as_str(), "no-such-session"),
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    // Note: a dedicated "peer finishes without sending → recv_next returns None"
    // test lives in P7c.3 where the sponsor-side handler opens a real bi-stream
    // before closing. iroh bi-streams require the dialer to write first for
    // `accept_bi()` on the responder to resolve, so a sponsor that finishes
    // without ever reading cannot be modelled faithfully here.
}
