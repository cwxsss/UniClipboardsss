//! POST /auth/connect - exchange bearer token for JWT session token.
//!
//! This endpoint is the entry point for the daemon's JWT authentication flow.
//! It accepts a bearer token (the daemon's local secret), validates it,
//! registers the client PID, and returns a short-lived JWT session token.

use std::net::SocketAddr;

use axum::body::{to_bytes, Body};
use axum::extract::{ConnectInfo, Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use url::form_urlencoded;

use crate::api::auth::parse_bearer_token;
use crate::api::server::DaemonApiState;
use crate::security::claims::{SessionTokenClaims, LEVEL_L2, REFRESH_AT_SECS, TTL_SECS};

/// Request body for POST /auth/connect
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectRequest {
    /// Client process ID. Used for PID whitelist verification in JWT middleware.
    pub pid: u32,
    /// Client type: "gui", "cli", or "other".
    pub client_type: String,
}

struct ParsedConnectRequest {
    pid: u32,
    client_type: String,
    token: Option<String>,
}

/// Response body for POST /auth/connect
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectResponse {
    /// HS256-signed JWT session token.
    pub session_token: String,
    /// Token time-to-live in seconds (5 minutes).
    pub expires_in_secs: i64,
    /// Recommended refresh time in seconds (4 minutes).
    pub refresh_at_secs: i64,
}

/// Router for auth-related routes.
///
/// POST /auth/connect is the only route - it accepts bearer token
/// (not session token) because it's the entry point for getting a session token.
pub fn router() -> Router<DaemonApiState> {
    // NOTE: cors_middleware is applied once at the outermost layer in
    // `build_router`; do not re-layer it here.
    Router::new().route(
        uc_daemon_contract::constants::auth_route::AUTH_CONNECT,
        post(connect_handler),
    )
}

/// Handler for POST /auth/connect.
///
/// Validates the bearer token in Authorization header, registers the client PID,
/// and returns a JWT session token.
///
/// Rate limiting: This endpoint has no session token yet, so rate limiting
/// is applied by client IP address (from ConnectInfo). This is trustworthy
/// because it comes from the TCP stack, not caller-controlled input.
///
/// NOTE on ConnectInfo: ConnectInfo<SocketAddr> reads the socket address from
/// the TCP connection metadata, NOT from HTTP headers. In test contexts (using
/// tower::ServiceExt::oneshot without a real TCP listener), the socket address
/// will be a default value (typically 127.0.0.1:0 or ::1:0). The unit tests
/// for SlidingWindowRateLimiter cover the rate limiting logic independently.
/// IP-based rate limiting for /auth/connect works correctly in production.
///
/// IMPORTANT: ConnectInfo<SocketAddr> works ONLY when the server uses
/// `into_make_service_with_connect_info::<SocketAddr>()`.
/// In test contexts using tower::ServiceExt::oneshot, ConnectInfo may be absent.
/// The handler uses Option<ConnectInfo<SocketAddr>> so tests work correctly.
/// IP-based rate limiting is skipped when ConnectInfo is unavailable (test-only code path).
async fn connect_handler(
    State(state): State<DaemonApiState>,
    connect_info: Option<ConnectInfo<SocketAddr>>,
    headers: HeaderMap,
    request: Request<Body>,
) -> axum::response::Response {
    // Step 1: Apply rate limiting by client IP (pre-auth, no session token yet).
    // ConnectInfo is None in test contexts (no real TCP connection via oneshot).
    // In production (real TCP listener via into_make_service_with_connect_info), it is always Some.
    if let Some(ConnectInfo(client_ip)) = connect_info {
        let client_ip_str = client_ip.ip().to_string();
        if !state.security.rate_limiter.check(&client_ip_str).await {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                Json(json!({"error": "rate_limit_exceeded", "retry_after_secs": 60})),
            )
                .into_response();
        }
    }

    let parsed = match parse_connect_request(request).await {
        Some(parsed) => parsed,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "invalid_connect_request"})),
            )
                .into_response();
        }
    };

    // Step 2: Validate bearer token (same as existing daemon auth)
    let token = parsed.token.as_deref().or_else(|| {
        headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(parse_bearer_token)
    });

    let Some(token) = token else {
        return unauthorized().into_response();
    };

    if token != state.auth_token.as_str() {
        return unauthorized().into_response();
    }

    // Step 3: Register PID in whitelist
    //
    // NOTE: The PID from the request body is trusted because:
    // 1. The bearer token (from daemon.token file) has already been validated above
    // 2. The frontend runs on the same machine as the daemon
    // 3. PID verification is defense-in-depth against local malware, not a hard security boundary
    // 4. The bearer token file has filesystem permissions (600)
    state.security.register_pid(parsed.pid).await;

    // Step 4: Build claims
    //
    // NOTE on L3/L4 (Phase 75 scope):
    // Phase 75 does NOT implement L3/L4 permission enforcement.
    // All clients receive L2 tokens (access_level = LEVEL_L2, encryption_ready = false).
    // The access_level field exists in the JWT for future use, but is not enforced by middleware.
    // Future phases (Phase 76+) will wire encryption state from CoreRuntime to determine
    // if the client should receive L3/L4 tokens based on encryption session state.
    let encryption_ready = false; // Phase 75: always false
    let access_level = LEVEL_L2; // Phase 75: always L2

    let claims = SessionTokenClaims::new(
        parsed.pid,
        parsed.client_type,
        access_level,
        encryption_ready,
    );

    // Step 5: Sign JWT with HS256
    let token = match claims.sign(state.security.jwt_secret.as_ref()) {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(error = %e, "failed to sign session token");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "token_generation_failed"})),
            )
                .into_response();
        }
    };

    let response = ConnectResponse {
        session_token: token,
        expires_in_secs: TTL_SECS,
        refresh_at_secs: REFRESH_AT_SECS,
    };

    (StatusCode::OK, Json(response)).into_response()
}

async fn parse_connect_request(request: Request<Body>) -> Option<ParsedConnectRequest> {
    let content_type = request
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|value: &axum::http::HeaderValue| value.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();

    let (parts, body) = request.into_parts();
    let bytes = to_bytes(body, 16 * 1024).await.ok()?;

    if content_type.starts_with("application/x-www-form-urlencoded") {
        let form: HashMap<String, String> = form_urlencoded::parse(bytes.as_ref())
            .into_owned()
            .collect();

        return Some(ParsedConnectRequest {
            pid: form.get("pid")?.parse().ok()?,
            client_type: form.get("clientType")?.clone(),
            token: form.get("token").cloned(),
        });
    }

    let req: ConnectRequest = serde_json::from_slice(&bytes).ok()?;
    let token = parts.uri.query().and_then(|query: &str| {
        form_urlencoded::parse(query.as_bytes())
            .find(|(key, _)| key == "token")
            .map(|(_, value)| value.into_owned())
    });

    Some(ParsedConnectRequest {
        pid: req.pid,
        client_type: req.client_type,
        token,
    })
}

fn unauthorized() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({"error": "unauthorized"})),
    )
}
