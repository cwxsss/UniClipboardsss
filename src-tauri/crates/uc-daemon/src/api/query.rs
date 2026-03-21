//! Read-only daemon query service.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use serde_json::Value;
use tokio::sync::RwLock;
use uc_app::runtime::CoreRuntime;
use uc_app::usecases::{CoreUseCases, SetupOrchestrator};
use uc_core::clipboard::ClipboardIntegrationMode;
use uc_core::network::PairedDevice;
use uc_core::setup::SetupState;

use crate::api::types::{
    HealthResponse, PairedDeviceDto, PairingSessionSummaryDto, PeerSnapshotDto,
    SetupActionAckResponse, SetupStateResponse, StatusResponse, WorkerStatusDto,
};
use crate::pairing::host::DaemonPairingHost;
use crate::state::{DaemonPairingSessionSnapshot, DaemonWorkerSnapshot, RuntimeState};
use crate::worker::WorkerHealth;

pub struct DaemonQueryService {
    runtime: Arc<CoreRuntime>,
    state: Arc<RwLock<RuntimeState>>,
}

impl DaemonQueryService {
    pub fn new(runtime: Arc<CoreRuntime>, state: Arc<RwLock<RuntimeState>>) -> Self {
        Self { runtime, state }
    }

    pub async fn health(&self) -> HealthResponse {
        HealthResponse {
            status: "ok".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    pub async fn status(&self) -> Result<StatusResponse> {
        let connected_peers = self
            .peers()
            .await?
            .into_iter()
            .filter(|peer| peer.connected)
            .count() as u32;
        let state = self.state.read().await;
        Ok(StatusResponse {
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_seconds: state.uptime_seconds(),
            workers: worker_statuses(state.worker_statuses()),
            connected_peers,
        })
    }

    pub async fn peers(&self) -> Result<Vec<PeerSnapshotDto>> {
        let usecases = CoreUseCases::new(self.runtime.as_ref());
        let snapshots = usecases.get_p2p_peers_snapshot().execute().await?;
        Ok(snapshots.into_iter().map(PeerSnapshotDto::from).collect())
    }

    pub async fn paired_devices(&self) -> Result<Vec<PairedDeviceDto>> {
        let usecases = CoreUseCases::new(self.runtime.as_ref());
        let connected_peers = self
            .peers()
            .await?
            .into_iter()
            .map(|peer| (peer.peer_id, peer.connected))
            .collect::<HashMap<_, _>>();
        let paired_devices = usecases.list_paired_devices().execute().await?;

        Ok(paired_devices
            .into_iter()
            .map(|device| map_paired_device(device, &connected_peers))
            .collect())
    }

    pub async fn pairing_session(
        &self,
        session_id: &str,
    ) -> Result<Option<PairingSessionSummaryDto>> {
        let state = self.state.read().await;
        Ok(state
            .pairing_session(session_id)
            .cloned()
            .map(PairingSessionSummaryDto::from))
    }

    pub async fn pairing_sessions(&self) -> Vec<PairingSessionSummaryDto> {
        let state = self.state.read().await;
        state
            .pairing_sessions()
            .into_iter()
            .map(PairingSessionSummaryDto::from)
            .collect()
    }

    pub async fn setup_state(
        &self,
        setup_orchestrator: &SetupOrchestrator,
        pairing_host: Option<&DaemonPairingHost>,
    ) -> Result<SetupStateResponse> {
        let usecases = CoreUseCases::new(self.runtime.as_ref());
        let local_device = usecases.get_local_device_info().execute().await?;
        let setup_state = setup_orchestrator.get_state().await;
        let active_snapshot = active_pairing_snapshot(&self.state, pairing_host).await;

        Ok(SetupStateResponse {
            state: setup_state_payload(&setup_state),
            session_id: active_snapshot
                .as_ref()
                .map(|snapshot| snapshot.session_id.clone()),
            next_step_hint: next_step_hint(&setup_state, active_snapshot.as_ref()).to_string(),
            profile: resolved_profile(),
            clipboard_mode: clipboard_mode_label(self.runtime.clipboard_integration_mode()),
            device_name: local_device.device_name,
            peer_id: local_device.peer_id,
            selected_peer_id: active_snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.peer_id.clone()),
            selected_peer_name: active_snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.device_name.clone()),
            has_completed: matches!(setup_state, SetupState::Completed),
        })
    }

    pub async fn setup_action_ack(
        &self,
        setup_orchestrator: &SetupOrchestrator,
        pairing_host: Option<&DaemonPairingHost>,
    ) -> Result<SetupActionAckResponse> {
        let state = self.setup_state(setup_orchestrator, pairing_host).await?;
        Ok(SetupActionAckResponse {
            state: state.state,
            session_id: state.session_id,
            next_step_hint: state.next_step_hint,
        })
    }
}

fn map_paired_device(
    device: PairedDevice,
    connected_peers: &HashMap<String, bool>,
) -> PairedDeviceDto {
    let peer_id = device.peer_id.to_string();
    let mut dto = PairedDeviceDto::from(device);
    dto.connected = connected_peers.get(&peer_id).copied().unwrap_or(false);
    dto
}

fn worker_health_label(health: &WorkerHealth) -> String {
    match health {
        WorkerHealth::Healthy => "healthy".to_string(),
        WorkerHealth::Degraded(reason) => format!("degraded ({reason})"),
        WorkerHealth::Stopped => "stopped".to_string(),
    }
}

fn worker_statuses(snapshots: &[DaemonWorkerSnapshot]) -> Vec<WorkerStatusDto> {
    snapshots
        .iter()
        .map(|worker| WorkerStatusDto {
            name: worker.name.clone(),
            health: worker_health_label(&worker.health),
        })
        .collect()
}

async fn active_pairing_snapshot(
    state: &Arc<RwLock<RuntimeState>>,
    pairing_host: Option<&DaemonPairingHost>,
) -> Option<DaemonPairingSessionSnapshot> {
    let session_id = pairing_host?.active_session_id().await?;
    let guard = state.read().await;
    guard.pairing_session(&session_id).cloned()
}

fn setup_state_payload(state: &SetupState) -> Value {
    serde_json::to_value(state).unwrap_or_else(|_| Value::String(format!("{state:?}")))
}

fn resolved_profile() -> String {
    std::env::var("UC_PROFILE")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "default".to_string())
}

fn clipboard_mode_label(mode: ClipboardIntegrationMode) -> String {
    match mode {
        ClipboardIntegrationMode::Full => "full".to_string(),
        ClipboardIntegrationMode::Passive => "passive".to_string(),
    }
}

fn next_step_hint(
    state: &SetupState,
    active_snapshot: Option<&DaemonPairingSessionSnapshot>,
) -> &'static str {
    match state {
        SetupState::Welcome => "idle",
        SetupState::CreateSpaceInputPassphrase { .. }
        | SetupState::ProcessingCreateSpace { .. } => "create-space-passphrase",
        SetupState::JoinSpaceSelectDevice { .. } => "join-select-peer",
        SetupState::JoinSpaceConfirmPeer { .. } => "host-confirm-peer",
        SetupState::JoinSpaceInputPassphrase { .. } => "join-enter-passphrase",
        SetupState::ProcessingJoinSpace { .. } => {
            match active_snapshot.map(|snapshot| snapshot.state.as_str()) {
                Some("request") | Some("verifying") | Some("complete") | Some("failed") | None => {
                    "join-waiting-for-host"
                }
                Some(_) => "join-waiting-for-host",
            }
        }
        SetupState::Completed => {
            if matches!(
                active_snapshot.map(|snapshot| snapshot.state.as_str()),
                Some("request")
            ) {
                "host-confirm-peer"
            } else {
                "completed"
            }
        }
    }
}
