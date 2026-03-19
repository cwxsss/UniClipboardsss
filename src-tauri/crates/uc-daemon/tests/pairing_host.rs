use std::sync::Arc;

use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use uc_app::usecases::PairingOrchestrator;
use uc_bootstrap::assembly::SetupAssemblyPorts;
use uc_bootstrap::{build_non_gui_runtime_with_setup, builders::build_daemon_app};
use uc_core::network::protocol::PairingRequest;
use uc_daemon::api::types::PairingSessionSummaryDto;
use uc_daemon::pairing::host::{DaemonPairingHost, DaemonPairingHostError};
use uc_daemon::pairing::session_projection::upsert_pairing_snapshot;
use uc_daemon::state::RuntimeState;

fn build_host() -> (
    DaemonPairingHost,
    Arc<RwLock<RuntimeState>>,
    Arc<PairingOrchestrator>,
    String,
) {
    let ctx = build_daemon_app().unwrap();
    let local_peer_id = ctx.deps.network_ports.peers.local_peer_id();
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
    let orchestrator = ctx.pairing_orchestrator.clone();
    let host = DaemonPairingHost::new(
        runtime,
        ctx.pairing_orchestrator,
        ctx.pairing_action_rx,
        state.clone(),
        ctx.space_access_orchestrator,
        ctx.key_slot_store,
    );
    (host, state, orchestrator, local_peer_id)
}

async fn build_host_async() -> (
    DaemonPairingHost,
    Arc<RwLock<RuntimeState>>,
    Arc<PairingOrchestrator>,
    String,
) {
    tokio::task::spawn_blocking(build_host)
        .await
        .expect("pairing host fixture join failed")
}

fn inbound_request(session_id: &str, local_peer_id: &str) -> PairingRequest {
    PairingRequest {
        session_id: session_id.to_string(),
        device_name: "Remote Device".to_string(),
        device_id: "remote-device-id".to_string(),
        peer_id: local_peer_id.to_string(),
        identity_pubkey: vec![1, 2, 3],
        nonce: vec![7; 32],
    }
}

#[tokio::test]
async fn daemon_pairing_host_enforces_single_active_session() {
    let (host, _state, _orchestrator, _local_peer_id) = build_host_async().await;
    host.set_discoverability("cli".to_string(), true, Some(60_000))
        .await;
    host.set_participant_ready("cli".to_string(), true, Some(60_000))
        .await;

    let first = host.initiate_pairing("peer-a".to_string()).await.unwrap();
    let second = host.initiate_pairing("peer-b".to_string()).await;

    assert_eq!(
        second.unwrap_err(),
        DaemonPairingHostError::ActivePairingSessionExists
    );
    assert_eq!(
        host.active_session_id().await.as_deref(),
        Some(first.as_str())
    );
}

#[tokio::test]
async fn daemon_pairing_host_starts_non_discoverable_in_headless_mode() {
    let (host, _state, _orchestrator, _local_peer_id) = build_host_async().await;

    assert!(!host.discoverable().await);
    assert!(!host.participant_ready().await);
}

#[tokio::test]
async fn daemon_pairing_host_rejects_inbound_without_ready_participant() {
    let (host, state, _orchestrator, local_peer_id) = build_host_async().await;
    host.set_discoverability("cli".to_string(), true, Some(60_000))
        .await;
    host.set_participant_ready("cli".to_string(), false, None)
        .await;

    let result = host
        .handle_incoming_request(
            "peer-remote".to_string(),
            inbound_request("session-inbound", &local_peer_id),
        )
        .await;

    assert_eq!(
        result.unwrap_err(),
        DaemonPairingHostError::NoLocalPairingParticipantReady
    );
    assert!(state
        .read()
        .await
        .pairing_session("session-inbound")
        .is_none());
}

#[tokio::test]
async fn daemon_pairing_host_survives_client_disconnect() {
    let (host, _state, _orchestrator, _local_peer_id) = build_host_async().await;
    let host = Arc::new(host);
    let cancel = CancellationToken::new();
    let task = tokio::spawn(Arc::clone(&host).run(cancel.child_token()));

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let (host, state, _orchestrator, _local_peer_id) = build_host_async().await;
    host.set_participant_ready("gui".to_string(), true, Some(60_000))
        .await;
    host.set_discoverability("gui".to_string(), true, Some(60_000))
        .await;
    let session_id = host.initiate_pairing("peer-a".to_string()).await.unwrap();
    host.set_participant_ready("gui".to_string(), false, None)
        .await;
    host.set_discoverability("gui".to_string(), false, None)
        .await;

    let snapshot = state.read().await.pairing_session(&session_id).cloned();
    cancel.cancel();
    let _ = task.await;

    assert_eq!(
        host.active_session_id().await.as_deref(),
        Some(session_id.as_str())
    );
    assert_eq!(snapshot.as_ref().map(|s| s.state.as_str()), Some("request"));
}

#[tokio::test]
async fn daemon_pairing_projection_omits_verification_secrets() {
    let state = Arc::new(RwLock::new(RuntimeState::new(vec![])));
    upsert_pairing_snapshot(
        &state,
        "session-1",
        Some("peer-1".to_string()),
        Some("Desk".to_string()),
        "verification",
        123,
    )
    .await;

    let snapshot = state
        .read()
        .await
        .pairing_session("session-1")
        .cloned()
        .unwrap();
    let dto = PairingSessionSummaryDto::from(snapshot);
    let json = serde_json::to_value(&dto).unwrap();

    assert_eq!(json["sessionId"], "session-1");
    assert!(json.get("code").is_none());
    assert!(json.get("localFingerprint").is_none());
    assert!(json.get("peerFingerprint").is_none());
    assert!(json.get("challenge").is_none());
}
