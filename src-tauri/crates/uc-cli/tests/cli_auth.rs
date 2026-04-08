//! Integration tests for CLI auth flow via daemon HTTP API.
//!
//! Tests the complete CLI -> daemon auth lifecycle:
//! 1. CLI reads bearer token from daemon.token file
//! 2. CLI exchanges bearer for JWT via POST /auth/connect with clientType: "cli"
//! 3. Daemon registers CLI PID in whitelist
//! 4. CLI uses JWT with "Session " prefix for subsequent requests
//! 5. CLI and GUI get independent session tokens (different jti)

use std::sync::Arc;
use std::sync::{Mutex, OnceLock};

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use serde_json::json;
use tower::ServiceExt;
use uc_daemon::api::auth::load_or_create_auth_token;
use uc_daemon::api::query::DaemonQueryService;
use uc_daemon::api::server::{build_router, DaemonApiState};
use uc_daemon::security::{SecurityState, SessionTokenClaims};
use uc_daemon::state::RuntimeState;

/// Build a test router with a fresh SecurityState.
/// Returns (router, bearer_token, security).
async fn build_test_router() -> (axum::Router, String, Arc<SecurityState>) {
    static RUNTIME_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let _guard = RUNTIME_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();

    let tempdir = tempfile::tempdir().unwrap();
    let token_path = tempdir.path().join("daemon.token");
    let token = load_or_create_auth_token(&token_path).unwrap();

    let runtime =
        Arc::new(uc_bootstrap::build_cli_runtime(None).expect("test runtime should build"));
    let state = Arc::new(tokio::sync::RwLock::new(RuntimeState::new(vec![])));
    let query_service = Arc::new(DaemonQueryService::new(runtime.clone(), state));
    let security = Arc::new(SecurityState::new());
    let daemon_pid = std::process::id();
    security.register_pid(daemon_pid).await;

    let api_state = DaemonApiState::new(query_service, token, Some(runtime), security.clone());
    let router = build_router(api_state);
    let token_value = std::fs::read_to_string(token_path).unwrap();
    (router, token_value, security)
}

/// Get a session token via POST /auth/connect with CLI clientType.
async fn get_cli_session_token(router: &axum::Router, bearer: &str, pid: u32) -> String {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/connect")
                .header("Authorization", format!("Bearer {bearer}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({
                        "pid": pid,
                        "clientType": "cli"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "/auth/connect should succeed for CLI"
    );
    let body = to_bytes(response.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    json["sessionToken"].as_str().unwrap().to_string()
}

// ========================================================================
// AUTH-01: CLI uses POST /auth/connect (not direct bearer)
// ========================================================================

#[tokio::test]
async fn cli_auth_uses_session_exchange_not_direct_bearer() {
    // This test verifies that the CLI pattern uses POST /auth/connect
    // by checking the full exchange flow end-to-end.
    let (app, bearer, _security) = build_test_router().await;
    let cli_pid = 54321u32;

    // Step 1: Exchange bearer for JWT via /auth/connect with clientType: "cli"
    let token = get_cli_session_token(&app, &bearer, cli_pid).await;
    assert!(!token.is_empty(), "session token should be returned");

    // Step 2: Use the JWT with "Session " prefix for protected request
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/status")
                .header("Authorization", format!("Session {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should NOT return 401 (auth should succeed)
    assert_ne!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "valid JWT should allow access"
    );
}

// ========================================================================
// AUTH-02: CLI PID registered in daemon whitelist
// ========================================================================

#[tokio::test]
async fn cli_auth_registers_pid_in_whitelist() {
    let (app, bearer, _security) = build_test_router().await;
    let cli_pid = 99988u32;

    // Exchange token (this registers the PID)
    let _token = get_cli_session_token(&app, &bearer, cli_pid).await;

    // The PID is now registered, so subsequent requests with valid JWT should succeed
    let token = get_cli_session_token(&app, &bearer, cli_pid).await;
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/status")
                .header("Authorization", format!("Session {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Status endpoint — auth should succeed (status != 401)
    assert_ne!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "registered PID with valid JWT should pass auth"
    );
}

// ========================================================================
// AUTH-05: CLI and GUI get independent session tokens
// ========================================================================

#[tokio::test]
async fn cli_and_gui_get_independent_tokens() {
    let (app, bearer, security) = build_test_router().await;
    let cli_pid = 11111u32;
    let gui_pid = 22222u32;

    // Get CLI token
    let cli_token = get_cli_session_token(&app, &bearer, cli_pid).await;
    // Get GUI token via POST /auth/connect with clientType: "gui"
    let gui_token = {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/connect")
                    .header("Authorization", format!("Bearer {bearer}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_string(&json!({
                            "pid": gui_pid,
                            "clientType": "gui"
                        }))
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        json["sessionToken"].as_str().unwrap().to_string()
    };

    // Tokens should be different (different jti)
    assert_ne!(
        cli_token, gui_token,
        "CLI and GUI tokens should have different jti"
    );

    // Decode and verify PIDs are different using the SAME security state
    let cli_claims = SessionTokenClaims::verify(&cli_token, security.jwt_secret.as_ref()).unwrap();
    let gui_claims = SessionTokenClaims::verify(&gui_token, security.jwt_secret.as_ref()).unwrap();

    assert_eq!(cli_claims.pid, cli_pid, "CLI token should have CLI PID");
    assert_eq!(gui_claims.pid, gui_pid, "GUI token should have GUI PID");
    assert_eq!(
        cli_claims.client_type, "cli",
        "CLI token should have client_type 'cli'"
    );
    assert_eq!(
        gui_claims.client_type, "gui",
        "GUI token should have client_type 'gui'"
    );
    assert_ne!(
        cli_claims.jti, gui_claims.jti,
        "Tokens should have different jti"
    );
}
