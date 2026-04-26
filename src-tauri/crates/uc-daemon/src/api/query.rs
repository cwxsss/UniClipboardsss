//! Read-only daemon query service.

use std::sync::Arc;

use anyhow::Result;
use tokio::sync::RwLock;
use uc_app::runtime::CoreRuntime;
use uc_application::facade::MemberRosterFacade;
use uc_application::space_access::SpaceAccessFacade;
use uc_core::ports::PresencePort;
use uc_core::space_access::state::SpaceAccessState;

use crate::api::types::{
    HealthResponse, PeerSnapshotDto, SpaceAccessStateResponse, SpaceMemberDto, StatusResponse,
    WorkerStatusDto,
};
use crate::peers::snapshot::derive_peer_snapshot;
use crate::service::ServiceHealth;
use crate::state::{DaemonServiceSnapshot, RuntimeState};
use crate::{DAEMON_API_REVISION, DAEMON_VERSION};

pub struct DaemonQueryService {
    runtime: Arc<CoreRuntime>,
    presence: Arc<dyn PresencePort>,
    state: Arc<RwLock<RuntimeState>>,
    member_roster: Option<Arc<MemberRosterFacade>>,
}

impl DaemonQueryService {
    /// Constructs a `DaemonQueryService` from the shared runtime, the
    /// iroh-stack `PresencePort` (drives both `peers()` snapshots and the
    /// `peers.changed` WS topic), and the daemon-wide `RuntimeState`.
    pub fn new(
        runtime: Arc<CoreRuntime>,
        presence: Arc<dyn PresencePort>,
        state: Arc<RwLock<RuntimeState>>,
        member_roster: Option<Arc<MemberRosterFacade>>,
    ) -> Self {
        Self {
            runtime,
            presence,
            state,
            member_roster,
        }
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
        derive_peer_snapshot(&self.presence, self.runtime.as_ref()).await
    }

    pub async fn paired_devices(&self) -> Result<Vec<SpaceMemberDto>> {
        let facade = self
            .member_roster
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("member roster facade unavailable"))?;
        let members = facade.list_members().await?;
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
