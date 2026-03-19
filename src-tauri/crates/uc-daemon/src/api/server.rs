//! HTTP server bootstrap for the daemon API.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::http::HeaderMap;
use axum::Router;
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use uc_app::runtime::CoreRuntime;

use crate::api::auth::{
    build_connection_info, parse_bearer_token, DaemonAuthToken, DaemonConnectionInfo,
};
use crate::api::query::DaemonQueryService;
use crate::api::routes;
use crate::api::types::DaemonWsEvent;
use crate::api::ws;
use crate::pairing::host::DaemonPairingHost;
use crate::socket::{resolve_daemon_http_addr, DEFAULT_HTTP_HOST};

#[derive(Clone)]
pub struct DaemonApiState {
    pub query_service: Arc<DaemonQueryService>,
    pub auth_token: DaemonAuthToken,
    pub runtime: Option<Arc<CoreRuntime>>,
    pub pairing_host: Option<Arc<DaemonPairingHost>>,
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
            pairing_host: None,
            event_tx,
        }
    }

    pub fn with_pairing_host(mut self, pairing_host: Arc<DaemonPairingHost>) -> Self {
        self.pairing_host = Some(pairing_host);
        self
    }

    pub fn connection_info_for_addr(&self, listen_addr: SocketAddr) -> DaemonConnectionInfo {
        build_connection_info(DEFAULT_HTTP_HOST, listen_addr.port(), &self.auth_token)
    }

    pub fn is_authorized(&self, headers: &HeaderMap) -> bool {
        headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(parse_bearer_token)
            .map(|token| token == self.auth_token.as_str())
            .unwrap_or(false)
    }
}

pub fn build_router(state: DaemonApiState) -> Router {
    Router::new()
        .merge(routes::router())
        .merge(ws::router())
        .with_state(state)
}

pub async fn run_http_server(
    state: DaemonApiState,
    cancel: CancellationToken,
) -> anyhow::Result<()> {
    let addr = resolve_daemon_http_addr();
    let listener = TcpListener::bind(addr).await?;
    let listen_addr = listener.local_addr()?;
    let connection_info = state.connection_info_for_addr(listen_addr);
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
