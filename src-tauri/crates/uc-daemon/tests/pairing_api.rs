use std::sync::Arc;
use std::sync::{Mutex, OnceLock};

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tokio::sync::{broadcast, RwLock};
use tower::ServiceExt;
use uc_bootstrap::assembly::SetupAssemblyPorts;
use uc_bootstrap::{build_non_gui_runtime_with_setup, builders::build_daemon_app};
use uc_daemon::api::auth::load_or_create_auth_token;
use uc_daemon::api::query::DaemonQueryService;
use uc_daemon::api::server::{build_router, DaemonApiState};
use uc_daemon::api::types::DaemonWsEvent;
use uc_daemon::pairing::host::DaemonPairingHost;
use uc_daemon::state::RuntimeState;

fn build_api_router() -> (axum::Router, String) {
    static RUNTIME_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let _guard = RUNTIME_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    let ctx = build_daemon_app().unwrap();
    let setup_ports = SetupAssemblyPorts::from_network(
        ctx.pairing_orchestrator.clone(),
        ctx.space_access_orchestrator.clone(),
        ctx.deps.network_ports.peers.clone(),
        None,
        Arc::new(uc_app::usecases::LoggingLifecycleEventEmitter),
    );
    let runtime = Arc::new(
        build_non_gui_runtime_with_setup(
            ctx.deps,
            ctx.storage_paths.clone(),
            setup_ports,
            ctx.watcher_control.clone(),
        )
        .unwrap(),
    );
    let state = Arc::new(RwLock::new(RuntimeState::new(vec![])));
    let query_service = Arc::new(DaemonQueryService::new(runtime.clone(), state.clone()));
    let tempdir = tempfile::tempdir().unwrap();
    let token_path = tempdir.path().join("daemon.token");
    let token = load_or_create_auth_token(&token_path).unwrap();
    let token_value = std::fs::read_to_string(&token_path).unwrap();
    let (event_tx, _event_rx) = broadcast::channel::<DaemonWsEvent>(128);
    let pairing_host = Arc::new(DaemonPairingHost::new(
        runtime,
        ctx.pairing_orchestrator,
        ctx.pairing_action_rx,
        state,
        ctx.space_access_orchestrator,
        ctx.key_slot_store,
        event_tx,
    ));
    let api_state = DaemonApiState::new(query_service, token, None).with_pairing_host(pairing_host);
    (build_router(api_state), token_value)
}

async fn build_api_router_async() -> (axum::Router, String) {
    tokio::task::spawn_blocking(build_api_router)
        .await
        .expect("pairing api fixture join failed")
}

fn authed_request(
    method: &str,
    uri: &str,
    token: &str,
    body: Body,
    content_type: Option<&str>,
) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("Authorization", format!("Bearer {}", token.trim()));
    if let Some(content_type) = content_type {
        builder = builder.header("Content-Type", content_type);
    }
    builder.body(body).unwrap()
}

async fn json_body(response: axum::response::Response) -> Value {
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&body).unwrap()
}

async fn set_discoverability(
    app: &axum::Router,
    token: &str,
    discoverable: bool,
    lease_ttl_ms: Option<u64>,
) -> axum::response::Response {
    app.clone()
        .oneshot(authed_request(
            "PUT",
            "/pairing/discoverability/current",
            token,
            Body::from(
                json!({
                    "clientKind": "cli",
                    "discoverable": discoverable,
                    "leaseTtlMs": lease_ttl_ms
                })
                .to_string(),
            ),
            Some("application/json"),
        ))
        .await
        .unwrap()
}

async fn set_participant_ready(
    app: &axum::Router,
    token: &str,
    ready: bool,
    lease_ttl_ms: Option<u64>,
) -> axum::response::Response {
    app.clone()
        .oneshot(authed_request(
            "PUT",
            "/pairing/participants/current",
            token,
            Body::from(
                json!({
                    "clientKind": "cli",
                    "ready": ready,
                    "leaseTtlMs": lease_ttl_ms
                })
                .to_string(),
            ),
            Some("application/json"),
        ))
        .await
        .unwrap()
}

async fn initiate_pairing(
    app: &axum::Router,
    token: &str,
    peer_id: &str,
) -> axum::response::Response {
    app.clone()
        .oneshot(authed_request(
            "POST",
            "/pairing/sessions",
            token,
            Body::from(json!({ "peerId": peer_id }).to_string()),
            Some("application/json"),
        ))
        .await
        .unwrap()
}

#[tokio::test]
async fn pairing_api_returns_409_active_pairing_session_exists() {
    let (app, token) = build_api_router_async().await;
    assert_eq!(
        set_discoverability(&app, &token, true, Some(60_000))
            .await
            .status(),
        StatusCode::ACCEPTED
    );
    assert_eq!(
        set_participant_ready(&app, &token, true, Some(60_000))
            .await
            .status(),
        StatusCode::ACCEPTED
    );

    let first = initiate_pairing(&app, &token, "peer-a").await;
    let second = initiate_pairing(&app, &token, "peer-b").await;

    assert_eq!(first.status(), StatusCode::ACCEPTED);
    assert_eq!(second.status(), StatusCode::CONFLICT);
    assert_eq!(
        json_body(second).await["error"],
        Value::String("active_pairing_session_exists".to_string())
    );
}

#[tokio::test]
async fn pairing_api_returns_412_when_no_local_participant_ready() {
    let (app, token) = build_api_router_async().await;
    assert_eq!(
        set_discoverability(&app, &token, true, Some(60_000))
            .await
            .status(),
        StatusCode::ACCEPTED
    );

    let response = initiate_pairing(&app, &token, "peer-a").await;

    assert_eq!(response.status(), StatusCode::PRECONDITION_FAILED);
    assert_eq!(
        json_body(response).await["error"],
        Value::String("no_local_pairing_participant_ready".to_string())
    );
}

#[tokio::test]
async fn pairing_api_returns_409_host_not_discoverable() {
    let (app, token) = build_api_router_async().await;
    assert_eq!(
        set_participant_ready(&app, &token, true, Some(60_000))
            .await
            .status(),
        StatusCode::ACCEPTED
    );

    let response = initiate_pairing(&app, &token, "peer-a").await;

    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(
        json_body(response).await["error"],
        Value::String("host_not_discoverable".to_string())
    );
}

#[tokio::test]
async fn pairing_api_returns_404_for_unknown_followup_session() {
    let (app, token) = build_api_router_async().await;

    let response = app
        .clone()
        .oneshot(authed_request(
            "POST",
            "/pairing/sessions/missing-session/accept",
            &token,
            Body::empty(),
            None,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn pairing_api_returns_400_for_malformed_payload() {
    let (app, token) = build_api_router_async().await;

    let response = app
        .clone()
        .oneshot(authed_request(
            "PUT",
            "/pairing/discoverability/current",
            &token,
            Body::from("{"),
            Some("application/json"),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn pairing_api_requires_explicit_discoverability_opt_in_for_cli() {
    let (app, token) = build_api_router_async().await;
    set_participant_ready(&app, &token, true, Some(60_000)).await;

    let response = initiate_pairing(&app, &token, "peer-a").await;
    let body = json_body(response).await;

    assert_eq!(body["error"], "host_not_discoverable");
}

#[tokio::test]
async fn pairing_api_expires_discoverability_lease() {
    let (app, token) = build_api_router_async().await;
    set_discoverability(&app, &token, true, Some(10)).await;
    set_participant_ready(&app, &token, true, Some(60_000)).await;

    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    let response = initiate_pairing(&app, &token, "peer-a").await;

    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(
        json_body(response).await["error"],
        Value::String("host_not_discoverable".to_string())
    );
}
