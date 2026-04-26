//! Read-only daemon query service.

use std::sync::Arc;

use anyhow::Result;
use tokio::sync::RwLock;
use uc_application::facade::{AppFacade, PeerSnapshotView};
use uc_application::space_access::SpaceAccessFacade;
use uc_core::space_access::state::SpaceAccessState;

use crate::api::types::{
    HealthResponse, PeerSnapshotDto, SpaceAccessStateResponse, SpaceMemberDto, StatusResponse,
    WorkerStatusDto,
};
use crate::service::ServiceHealth;
use crate::state::{DaemonServiceSnapshot, RuntimeState};
use crate::{DAEMON_API_REVISION, DAEMON_VERSION};

pub struct DaemonQueryService {
    state: Arc<RwLock<RuntimeState>>,
    app_facade: Arc<AppFacade>,
}

impl DaemonQueryService {
    /// Constructs a `DaemonQueryService` from the daemon-wide `RuntimeState`
    /// and the unified application facade used by read-only query endpoints.
    pub fn new(state: Arc<RwLock<RuntimeState>>, app_facade: Arc<AppFacade>) -> Self {
        Self { state, app_facade }
    }

    pub async fn health(&self) -> HealthResponse {
        HealthResponse {
            status: "ok".to_string(),
            package_version: DAEMON_VERSION.to_string(),
            api_revision: DAEMON_API_REVISION.to_string(),
        }
    }

    pub async fn status(&self) -> Result<StatusResponse> {
        let state = self.state.read().await;
        Ok(StatusResponse {
            package_version: DAEMON_VERSION.to_string(),
            api_revision: DAEMON_API_REVISION.to_string(),
            uptime_seconds: state.uptime_seconds(),
            workers: worker_statuses(state.worker_statuses()),
        })
    }

    pub async fn peers(&self) -> Result<Vec<PeerSnapshotDto>> {
        let peers = self.app_facade.list_peer_snapshots().await?;
        Ok(peers.into_iter().map(peer_snapshot_to_dto).collect())
    }

    pub async fn paired_devices(&self) -> Result<Vec<SpaceMemberDto>> {
        let members = self.app_facade.list_members().await?;
        Ok(members
            .into_iter()
            .map(|m| SpaceMemberDto {
                peer_id: m.device_id,
                device_name: m.device_name,
                pairing_state: "Trusted".to_string(),
                last_seen_at_ms: None,
                connected: false,
            })
            .collect())
    }

    pub async fn space_access_state(
        &self,
        orchestrator: Option<&SpaceAccessFacade>,
    ) -> SpaceAccessStateResponse {
        let state = match orchestrator {
            Some(o) => o.get_state().await,
            None => SpaceAccessState::Idle,
        };
        SpaceAccessStateResponse { state }
    }
}

fn peer_snapshot_to_dto(peer: PeerSnapshotView) -> PeerSnapshotDto {
    PeerSnapshotDto {
        peer_id: peer.peer_id,
        device_name: peer.device_name,
        addresses: peer.addresses,
        is_paired: peer.is_paired,
        connected: peer.connected,
        pairing_state: peer.pairing_state,
    }
}

fn worker_health_label(health: &ServiceHealth) -> String {
    match health {
        ServiceHealth::Healthy => "healthy".to_string(),
        ServiceHealth::Degraded(reason) => format!("degraded ({reason})"),
        ServiceHealth::Stopped => "stopped".to_string(),
    }
}

fn worker_statuses(snapshots: &[DaemonServiceSnapshot]) -> Vec<WorkerStatusDto> {
    snapshots
        .iter()
        .map(|worker| WorkerStatusDto {
            name: worker.name.clone(),
            health: worker_health_label(&worker.health),
        })
        .collect()
}
