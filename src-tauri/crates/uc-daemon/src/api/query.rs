//! Read-only daemon query service.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use tokio::sync::RwLock;
use uc_app::runtime::CoreRuntime;
use uc_app::usecases::CoreUseCases;
use uc_core::network::PairedDevice;

use crate::api::types::{
    HealthResponse, PairedDeviceDto, PairingSessionSummaryDto, PeerSnapshotDto, StatusResponse,
    WorkerStatusDto,
};
use crate::state::{DaemonWorkerSnapshot, RuntimeState};
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
