use std::sync::Arc;
use std::sync::{Mutex as StdMutex, OnceLock};

use anyhow::Result;
use async_trait::async_trait;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tokio::sync::{mpsc, Mutex, RwLock};
use tower::ServiceExt;
use uc_app::runtime::CoreRuntime;
use uc_app::testing::{
    NoopDiscoveryPort, NoopLifecycleEventEmitter, NoopLifecycleStatus, NoopNetworkControl,
    NoopPairingTransport, NoopProofPort, NoopSessionReadyEmitter, NoopSetupEventPort,
    NoopSpaceAccessPersistence, NoopSpaceAccessTransport, NoopTimerPort,
};
use uc_app::usecases::app_lifecycle::{AppLifecycleCoordinator, AppLifecycleCoordinatorDeps};
use uc_app::usecases::pairing::PairingDomainEvent;
use uc_app::usecases::setup::{MarkSetupComplete, SetupPairingFacadePort};
use uc_app::usecases::space_access::{SpaceAccessCryptoFactory, SpaceAccessOrchestrator};
use uc_app::usecases::{InitializeEncryption, SetupOrchestrator, StartClipboardWatcherPort};
use uc_bootstrap::build_cli_runtime;
use uc_core::ports::space::CryptoPort;
use uc_core::ports::SetupStatusPort;
use uc_core::security::model::{KeySlot, MasterKey};
use uc_core::setup::SetupStatus;
use uc_daemon::api::auth::load_or_create_auth_token;
use uc_daemon::api::query::DaemonQueryService;
use uc_daemon::api::server::{build_router, DaemonApiState};
use uc_daemon::state::RuntimeState;

fn build_runtime() -> Arc<CoreRuntime> {
    static RUNTIME: OnceLock<Arc<CoreRuntime>> = OnceLock::new();
    RUNTIME
        .get_or_init(|| Arc::new(build_cli_runtime(None).expect("build cli runtime")))
        .clone()
}

async fn build_setup_router() -> (axum::Router, String) {
    static TEST_LOCK: OnceLock<StdMutex<()>> = OnceLock::new();
    let _guard = TEST_LOCK
        .get_or_init(|| StdMutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    let runtime = build_runtime();
    let state = Arc::new(RwLock::new(RuntimeState::new(vec![])));
    let query_service = Arc::new(DaemonQueryService::new(runtime.clone(), state));
    let tempdir = tempfile::tempdir().unwrap();
    let token_path = tempdir.path().join("daemon.token");
    let token = load_or_create_auth_token(&token_path).unwrap();
    let token_value = std::fs::read_to_string(token_path).unwrap();
    let setup_orchestrator = build_setup_orchestrator(runtime);
    let api_state = DaemonApiState::new(query_service, token, None).with_setup(setup_orchestrator);
    (build_router(api_state), token_value)
}

fn build_setup_orchestrator(runtime: Arc<CoreRuntime>) -> Arc<SetupOrchestrator> {
    let wiring = runtime.wiring_deps();
    let initialize_encryption = Arc::new(InitializeEncryption::from_ports(
        wiring.security.encryption.clone(),
        wiring.security.key_material.clone(),
        wiring.security.key_scope.clone(),
        wiring.security.encryption_state.clone(),
        wiring.security.encryption_session.clone(),
    ));
    let setup_status = Arc::new(InMemorySetupStatus::default());
    let mark_setup_complete = Arc::new(MarkSetupComplete::new(setup_status.clone()));

    Arc::new(SetupOrchestrator::new(
        initialize_encryption,
        mark_setup_complete,
        setup_status,
        build_mock_lifecycle(),
        Arc::new(FakeSetupPairingFacade::default()),
        Arc::new(NoopSetupEventPort),
        Arc::new(SpaceAccessOrchestrator::new()),
        Arc::new(NoopDiscoveryPort),
        Arc::new(NoopNetworkControl),
        Arc::new(NoopSpaceAccessCryptoFactory),
        Arc::new(NoopPairingTransport),
        Arc::new(Mutex::new(NoopSpaceAccessTransport)),
        Arc::new(NoopProofPort),
        Arc::new(Mutex::new(NoopTimerPort)),
        Arc::new(Mutex::new(NoopSpaceAccessPersistence)),
    ))
}

fn build_mock_lifecycle() -> Arc<AppLifecycleCoordinator> {
    Arc::new(AppLifecycleCoordinator::from_deps(
        AppLifecycleCoordinatorDeps {
            watcher: Arc::new(NoopStartClipboardWatcher),
            network: Arc::new(uc_app::usecases::StartNetworkAfterUnlock::new(Arc::new(
                NoopNetworkControl,
            ))),
            announcer: None,
            emitter: Arc::new(NoopSessionReadyEmitter),
            status: Arc::new(NoopLifecycleStatus),
            lifecycle_emitter: Arc::new(NoopLifecycleEventEmitter),
        },
    ))
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

async fn get_setup_state(app: &axum::Router, token: &str) -> Value {
    let response = app
        .clone()
        .oneshot(authed_request(
            "GET",
            "/setup/state",
            token,
            Body::empty(),
            None,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    json_body(response).await
}

fn assert_setup_state_metadata_shape(body: &Value) {
    assert!(body.get("sessionId").is_some());
    assert!(body.get("nextStepHint").is_some());
    assert!(body.get("clipboardMode").is_some());
    assert!(body.get("deviceName").is_some());
    assert!(body.get("peerId").is_some());
    assert!(body.get("session_id").is_none());
    assert!(body.get("next_step_hint").is_none());
    assert!(body.get("clipboard_mode").is_none());
    assert!(body.get("device_name").is_none());
    assert!(body.get("peer_id").is_none());

    let hint = body["nextStepHint"].as_str().expect("nextStepHint string");
    assert!(matches!(
        hint,
        "create-space-passphrase"
            | "join-select-peer"
            | "join-waiting-for-host"
            | "join-enter-passphrase"
            | "host-confirm-peer"
            | "completed"
            | "idle"
    ));
}

#[derive(Default)]
struct InMemorySetupStatus {
    status: Mutex<SetupStatus>,
}

#[async_trait]
impl SetupStatusPort for InMemorySetupStatus {
    async fn get_status(&self) -> Result<SetupStatus> {
        Ok(self.status.lock().await.clone())
    }

    async fn set_status(&self, status: &SetupStatus) -> Result<()> {
        *self.status.lock().await = status.clone();
        Ok(())
    }
}

#[derive(Default)]
struct FakeSetupPairingFacade;

#[async_trait]
impl SetupPairingFacadePort for FakeSetupPairingFacade {
    async fn subscribe(&self) -> Result<mpsc::Receiver<PairingDomainEvent>> {
        let (_tx, rx) = mpsc::channel(8);
        Ok(rx)
    }

    async fn initiate_pairing(&self, _peer_id: String) -> Result<String> {
        Ok("session-test".to_string())
    }

    async fn accept_pairing(&self, _session_id: &str) -> Result<()> {
        Ok(())
    }

    async fn reject_pairing(&self, _session_id: &str) -> Result<()> {
        Ok(())
    }

    async fn cancel_pairing(&self, _session_id: &str) -> Result<()> {
        Ok(())
    }

    async fn verify_pairing(&self, _session_id: &str, _pin_matches: bool) -> Result<()> {
        Ok(())
    }
}

struct NoopStartClipboardWatcher;

#[async_trait]
impl StartClipboardWatcherPort for NoopStartClipboardWatcher {
    async fn execute(&self) -> Result<(), uc_core::ports::StartClipboardWatcherError> {
        Ok(())
    }
}

struct NoopSpaceAccessCryptoFactory;

impl SpaceAccessCryptoFactory for NoopSpaceAccessCryptoFactory {
    fn build(&self, _passphrase: uc_core::security::SecretString) -> Box<dyn CryptoPort> {
        Box::new(NoopSpaceAccessCrypto)
    }
}

struct NoopSpaceAccessCrypto;

#[async_trait]
impl CryptoPort for NoopSpaceAccessCrypto {
    async fn generate_nonce32(&self) -> [u8; 32] {
        [0u8; 32]
    }

    async fn export_keyslot_blob(&self, _space_id: &uc_core::ids::SpaceId) -> Result<KeySlot> {
        Err(anyhow::anyhow!("noop export_keyslot_blob"))
    }

    async fn derive_master_key_from_keyslot(
        &self,
        _keyslot_blob: &[u8],
        _passphrase: uc_core::security::SecretString,
    ) -> Result<MasterKey> {
        Err(anyhow::anyhow!("noop derive_master_key_from_keyslot"))
    }
}

#[tokio::test]
async fn setup_state_requires_authentication() {
    let (app, _) = build_setup_router().await;

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
    let (app, token) = build_setup_router().await;

    let response = app
        .clone()
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
    assert_eq!(body["nextStepHint"], "create-space-passphrase");
    assert!(body.get("sessionId").is_some());
    assert!(body.get("nextStepHint").is_some());
    assert!(body.get("session_id").is_none());
    assert!(body.get("next_step_hint").is_none());
}

#[tokio::test]
async fn setup_join_route_returns_join_select_device_state() {
    let (app, token) = build_setup_router().await;

    let response = app
        .clone()
        .oneshot(authed_request(
            "POST",
            "/setup/join",
            &token,
            Body::empty(),
            None,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert!(body["state"]["JoinSpaceSelectDevice"]["error"].is_null());
    assert_eq!(body["nextStepHint"], "join-select-peer");

    let state = get_setup_state(&app, &token).await;
    assert!(state["state"]["JoinSpaceSelectDevice"]["error"].is_null());
    assert_setup_state_metadata_shape(&state);
}

#[tokio::test]
async fn setup_select_peer_route_returns_processing_join_state() {
    let (app, token) = build_setup_router().await;

    let join_response = app
        .clone()
        .oneshot(authed_request(
            "POST",
            "/setup/join",
            &token,
            Body::empty(),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(join_response.status(), StatusCode::OK);

    let response = app
        .clone()
        .oneshot(authed_request(
            "POST",
            "/setup/select-peer",
            &token,
            Body::from(json!({ "peerId": "peer-remote" }).to_string()),
            Some("application/json"),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert!(body["state"]["ProcessingJoinSpace"]["message"].is_string());
    assert_eq!(body["nextStepHint"], "join-waiting-for-host");
}

#[tokio::test]
async fn setup_confirm_peer_route_rejects_when_no_pending_confirmation_exists() {
    let (app, token) = build_setup_router().await;

    let response = app
        .clone()
        .oneshot(authed_request(
            "POST",
            "/setup/confirm-peer",
            &token,
            Body::empty(),
            None,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = json_body(response).await;
    assert_eq!(body["code"], "invalid_setup_transition");
}

#[tokio::test]
async fn setup_submit_passphrase_route_rejects_malformed_payload() {
    let (app, token) = build_setup_router().await;

    let response = app
        .clone()
        .oneshot(authed_request(
            "POST",
            "/setup/submit-passphrase",
            &token,
            Body::from(json!({ "pass": "secret" }).to_string()),
            Some("application/json"),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_body(response).await;
    assert_eq!(body["code"], "bad_request");
}

#[tokio::test]
async fn setup_cancel_route_returns_idle_or_select_state_without_500() {
    let (app, token) = build_setup_router().await;

    let join_response = app
        .clone()
        .oneshot(authed_request(
            "POST",
            "/setup/join",
            &token,
            Body::empty(),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(join_response.status(), StatusCode::OK);

    let select_response = app
        .clone()
        .oneshot(authed_request(
            "POST",
            "/setup/select-peer",
            &token,
            Body::from(json!({ "peerId": "peer-remote" }).to_string()),
            Some("application/json"),
        ))
        .await
        .unwrap();
    assert_eq!(select_response.status(), StatusCode::OK);

    let response = app
        .clone()
        .oneshot(authed_request(
            "POST",
            "/setup/cancel",
            &token,
            Body::empty(),
            None,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert!(
        body["state"]["JoinSpaceSelectDevice"].is_object()
            || body["state"] == Value::String("Welcome".to_string())
    );
    assert!(
        body["nextStepHint"] == Value::String("join-select-peer".to_string())
            || body["nextStepHint"] == Value::String("idle".to_string())
    );
}
