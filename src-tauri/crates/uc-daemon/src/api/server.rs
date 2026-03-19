//! HTTP server bootstrap for the daemon API.

use std::sync::Arc;

use axum::Router;
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use uc_app::runtime::CoreRuntime;

use crate::api::auth::{build_connection_info, DaemonAuthToken, DaemonConnectionInfo};
use crate::api::query::DaemonQueryService;
use crate::api::routes;
use crate::api::types::DaemonWsEvent;
use crate::socket::resolve_daemon_http_addr;

#[derive(Clone)]
pub struct DaemonApiState {
    pub query_service: Arc<DaemonQueryService>,
    pub auth_token: DaemonAuthToken,
    pub runtime: Option<Arc<CoreRuntime>>,
    pub event_tx: broadcast::Sender<DaemonWsEvent>,
}

impl DaemonApiState {
    pub fn new(
        query_service: Arc<DaemonQueryService>,
        auth_token: DaemonAuthToken,
        runtime: Option<Arc<CoreRuntime>>,
    ) -> Self {
        let (event_tx, _) = broadcast::channel(64);
        Self {
            query_service,
            auth_token,
            runtime,
            event_tx,
        }
    }

    pub fn connection_info(&self) -> DaemonConnectionInfo {
        let addr = resolve_daemon_http_addr();
        build_connection_info("127.0.0.1", addr.port(), &self.auth_token)
    }
}

pub fn build_router(state: DaemonApiState) -> Router {
    Router::new().merge(routes::router()).with_state(state)
}

pub async fn run_http_server(
    state: DaemonApiState,
    cancel: CancellationToken,
) -> anyhow::Result<()> {
    let addr = resolve_daemon_http_addr();
    let listener = TcpListener::bind(addr).await?;
    let connection_info = state.connection_info();
    tracing::info!(
        base_url = %connection_info.base_url,
        ws_url = %connection_info.ws_url,
        "daemon HTTP API listening on 127.0.0.1"
    );

    axum::serve(listener, build_router(state).into_make_service())
        .with_graceful_shutdown(cancel.cancelled_owned())
        .await?;

    Ok(())
}
