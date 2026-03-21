use std::sync::Arc;
use std::sync::{Mutex, OnceLock};

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use serde_json::Value;
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
        runtime.clone(),
        ctx.pairing_orchestrator,
        ctx.pairing_action_rx,
        state,
        ctx.space_access_orchestrator,
        ctx.key_slot_store,
        event_tx,
    ));
    let api_state = DaemonApiState::new(query_service, token, None)
        .with_setup(runtime.setup_orchestrator().clone())
        .with_pairing_host(pairing_host);
    (build_router(api_state), token_value)
}

async fn build_api_router_async() -> (axum::Router, String) {
    tokio::task::spawn_blocking(build_api_router)
        .await
        .expect("setup api fixture join failed")
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

#[tokio::test]
async fn setup_state_requires_authentication() {
    let (app, _) = build_api_router_async().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/setup/state")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn setup_host_route_starts_new_space_and_returns_setup_state() {
    let (app, token) = build_api_router_async().await;

    let response = app
        .oneshot(authed_request(
            "POST",
            "/setup/host",
            &token,
            Body::empty(),
            None,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = json_body(response).await;
    assert!(body["state"]["CreateSpaceInputPassphrase"]["error"].is_null());
    assert_eq!(
        body["nextStepHint"],
        Value::String("create-space-passphrase".to_string())
    );
    assert!(body["sessionId"].is_null());
    assert!(body.get("session_id").is_none());
    assert!(body.get("next_step_hint").is_none());
}
