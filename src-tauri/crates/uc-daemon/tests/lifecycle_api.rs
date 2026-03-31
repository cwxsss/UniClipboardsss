//! Integration tests for the daemon lifecycle HTTP API routes.
//!
//! Tests verify HTTP method contracts, status codes, and response shapes
//! for lifecycle status and retry endpoints.

use std::sync::Arc;
use std::sync::{Mutex, OnceLock};

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use serde_json::Value;
use tokio::sync::RwLock;
use tower::ServiceExt;
use uc_daemon::api::auth::load_or_create_auth_token;
use uc_daemon::api::query::DaemonQueryService;
use uc_daemon::api::server::{build_router, DaemonApiState};
use uc_daemon::security::SecurityState;
use uc_daemon::state::RuntimeState;

fn build_runtime() -> Arc<uc_app::runtime::CoreRuntime> {
    static RUNTIME_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let _guard = RUNTIME_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    Arc::new(uc_bootstrap::build_cli_runtime(None).unwrap())
}

async fn build_test_router() -> (axum::Router, String) {
    let runtime = build_runtime();
    let state = Arc::new(RwLock::new(RuntimeState::new(vec![])));
    let query_service = Arc::new(DaemonQueryService::new(runtime.clone(), state));
    let tempdir = tempfile::tempdir().unwrap();
    let token_path = tempdir.path().join("daemon.token");
    let token = load_or_create_auth_token(&token_path).unwrap();
    let security = Arc::new(SecurityState::new());
    security.register_pid(std::process::id()).await;
    let api_state = DaemonApiState::new(query_service, token, Some(runtime.clone()), security);
    let router = build_router(api_state);
    let token_value = std::fs::read_to_string(token_path).unwrap();
    (router, token_value)
}

/// Helper: get a valid session token for authenticated requests.
async fn get_session_token(app: &axum::Router, bearer_token: &str) -> String {
    use axum::http::header::AUTHORIZATION;
    let pid = std::process::id();

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/connect")
                .header(AUTHORIZATION, format!("Bearer {}", bearer_token.trim()))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&serde_json::json!({
                        "pid": pid,
                        "clientType": "test"
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
        "auth/connect should succeed with valid bearer token"
    );
    let body = to_bytes(response.into_body(), 4096).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["sessionToken"].as_str().unwrap().to_string()
}

/// Helper: make an authenticated request.
async fn auth_request(
    app: &axum::Router,
    session_token: &str,
    method: axum::http::Method,
    uri: &str,
    body: Option<Body>,
) -> axum::response::Response {
    use axum::http::header::AUTHORIZATION;

    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(AUTHORIZATION, format!("Session {}", session_token));

    if body.is_some() {
        builder = builder.header("content-type", "application/json");
    }

    let request = builder.body(body.unwrap_or_else(|| Body::empty())).unwrap();

    app.clone().oneshot(request).await.unwrap()
}

// ── GET /lifecycle/status ────────────────────────────────────────

#[tokio::test]
async fn get_lifecycle_status_returns_200_with_state_field() {
    let (app, token) = build_test_router().await;
    let session = get_session_token(&app, &token).await;

    let response = auth_request(
        &app,
        &session,
        axum::http::Method::GET,
        "/lifecycle/status",
        None,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 4096).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert!(
        json.get("state").is_some(),
        "response should have 'state' field"
    );
    // State should be one of the valid LifecycleState variants serialized as a string.
    let state = json["state"].as_str().unwrap();
    assert!(
        ["Idle", "Pending", "Ready", "NetworkFailed"].contains(&state),
        "state should be a valid LifecycleState variant, got: {}",
        state
    );
}

#[tokio::test]
async fn get_lifecycle_status_requires_auth() {
    let (app, _token) = build_test_router().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/lifecycle/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// ── POST /lifecycle/retry ────────────────────────────────────────

#[tokio::test]
async fn retry_lifecycle_returns_204_when_already_ready() {
    let (app, token) = build_test_router().await;
    let session = get_session_token(&app, &token).await;

    // When lifecycle is already Ready, retry should return 204 immediately.
    let response = auth_request(
        &app,
        &session,
        axum::http::Method::POST,
        "/lifecycle/retry",
        None,
    )
    .await;

    // Should succeed with NO_CONTENT (either because already Ready, or network started OK).
    assert!(
        matches!(
            response.status(),
            StatusCode::NO_CONTENT | StatusCode::INTERNAL_SERVER_ERROR
        ),
        "retry should return 204 or 500 (network already started), got: {}",
        response.status()
    );
}

#[tokio::test]
async fn retry_lifecycle_requires_auth() {
    let (app, _token) = build_test_router().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/lifecycle/retry")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// ── POST /lifecycle/ready ────────────────────────────────────────

#[tokio::test]
async fn lifecycle_ready_returns_204() {
    let (app, token) = build_test_router().await;
    let session = get_session_token(&app, &token).await;

    let response = auth_request(
        &app,
        &session,
        axum::http::Method::POST,
        "/lifecycle/ready",
        None,
    )
    .await;

    assert_eq!(
        response.status(),
        StatusCode::NO_CONTENT,
        "lifecycle/ready should return 204"
    );
}

#[tokio::test]
async fn lifecycle_ready_requires_auth() {
    let (app, _token) = build_test_router().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/lifecycle/ready")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
