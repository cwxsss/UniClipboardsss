//! Read-only daemon query service.

use std::sync::Arc;

use anyhow::Result;
use tokio::sync::RwLock;
use uc_app::runtime::CoreRuntime;
use uc_app::usecases::CoreUseCases;
use uc_application::membership::usecases::ListMembersUseCase;
use uc_application::space_access::SpaceAccessFacade;
use uc_core::space_access::state::SpaceAccessState;

use crate::api::projection::IntoApiDto;
use crate::api::types::{
    HealthResponse, PeerSnapshotDto, SpaceAccessStateResponse, SpaceMemberDto, StatusResponse,
    WorkerStatusDto,
};
use crate::service::ServiceHealth;
use crate::state::{DaemonServiceSnapshot, RuntimeState};
use crate::{DAEMON_API_REVISION, DAEMON_VERSION};

pub struct DaemonQueryService {
    runtime: Arc<CoreRuntime>,
    state: Arc<RwLock<RuntimeState>>,
}

impl DaemonQueryService {
    /// Constructs a `DaemonQueryService` using the given runtime handle and shared runtime state.
    ///
    /// `runtime` is the shared core runtime used to build use-cases and read configuration.
    /// `state` is the shared, asynchronously lockable runtime state.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    /// use tokio::sync::RwLock;
    /// // assume CoreRuntime and RuntimeState types are in scope
    /// let runtime = Arc::new(CoreRuntime::new_for_tests());
    /// let state = Arc::new(RwLock::new(RuntimeState::default()));
    /// let svc = DaemonQueryService::new(runtime, state);
    /// ```
    pub fn new(runtime: Arc<CoreRuntime>, state: Arc<RwLock<RuntimeState>>) -> Self {
        Self { runtime, state }
    }

    pub async fn health(&self) -> HealthResponse {
        HealthResponse {
            status: "ok".to_string(),
            package_version: DAEMON_VERSION.to_string(),
            api_revision: DAEMON_API_REVISION.to_string(),
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
            package_version: DAEMON_VERSION.to_string(),
            api_revision: DAEMON_API_REVISION.to_string(),
            uptime_seconds: state.uptime_seconds(),
            workers: worker_statuses(state.worker_statuses()),
            connected_peers,
        })
    }

    pub async fn peers(&self) -> Result<Vec<PeerSnapshotDto>> {
        let usecases = CoreUseCases::new(self.runtime.as_ref());
        let snapshots = usecases.get_p2p_peers_snapshot().execute().await?;
        Ok(snapshots
            .into_iter()
            .map(IntoApiDto::into_api_dto)
            .collect())
    }

    pub async fn paired_devices(&self) -> Result<Vec<SpaceMemberDto>> {
        let members =
            ListMembersUseCase::new(self.runtime.wiring_deps().device.member_repo.clone())
                .execute()
                .await?;

        // Slice 4 P5a-1: connected 字段在 libp2p 删除后失去数据源；
        // 暂时全部填 false，等 iroh 侧的连接状态接入后再回填。
        Ok(members.into_iter().map(|m| m.into_api_dto()).collect())
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
