use std::future::Future;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use thiserror::Error;
use uc_daemon::api::auth::{resolve_daemon_token_path, DaemonConnectionInfo};
use uc_daemon::api::types::HealthResponse;
use uc_daemon::socket::{resolve_daemon_http_addr, resolve_daemon_socket_path};

use super::runtime::DaemonConnectionState;
use crate::commands::startup::StartupBarrier;

pub const DAEMON_CONNECTION_EVENT: &str = "daemon://connection-info";
const DAEMON_BINARY_NAME: &str = "uniclipboard-daemon";
const HEALTH_PATH: &str = "/health";
const HEALTH_CHECK_TIMEOUT: Duration = Duration::from_secs(8);
const HEALTH_POLL_INTERVAL: Duration = Duration::from_millis(200);
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Error)]
pub enum DaemonBootstrapError {
    #[error("failed to initialize daemon HTTP probe client: {0}")]
    Client(anyhow::Error),
    #[error("failed to probe daemon health: {0}")]
    Probe(anyhow::Error),
    #[error("incompatible daemon is already running: {details}")]
    IncompatibleDaemon { details: String },
    #[error("failed to spawn uniclipboard-daemon: {0}")]
    Spawn(anyhow::Error),
    #[error("daemon startup timed out after {timeout_ms}ms")]
    StartupTimeout { timeout_ms: u64 },
    #[error("failed to load daemon connection info: {0}")]
    ConnectionInfo(anyhow::Error),
    #[error("main webview window is not available")]
    MainWindowUnavailable,
    #[error("failed to emit daemon connection info event: {0}")]
    Emit(anyhow::Error),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct DaemonConnectionPayload {
    base_url: String,
    ws_url: String,
    token: String,
}

impl From<&DaemonConnectionInfo> for DaemonConnectionPayload {
    fn from(value: &DaemonConnectionInfo) -> Self {
        Self {
            base_url: value.base_url.clone(),
            ws_url: value.ws_url.clone(),
            token: value.token.clone(),
        }
    }
}

pub async fn bootstrap_daemon_connection(
    state: &DaemonConnectionState,
) -> Result<DaemonConnectionInfo, DaemonBootstrapError> {
    let client = reqwest::Client::builder()
        .timeout(PROBE_TIMEOUT)
        .build()
        .map_err(|error| {
            DaemonBootstrapError::Client(
                anyhow::Error::new(error).context("failed to build daemon probe client"),
            )
        })?;

    bootstrap_daemon_connection_with(
        state,
        || spawn_daemon_process().map(|_| ()),
        || probe_daemon_health(&client),
        load_daemon_connection_info,
        HEALTH_CHECK_TIMEOUT,
        HEALTH_POLL_INTERVAL,
    )
    .await
}

pub fn emit_daemon_connection_info_if_ready<R: Runtime>(
    app_handle: &AppHandle<R>,
    state: &DaemonConnectionState,
    startup_barrier: &StartupBarrier,
) -> Result<bool, DaemonBootstrapError> {
    if !startup_barrier.frontend_ready() {
        return Ok(false);
    }

    let connection_info = match state.get() {
        Some(connection_info) => connection_info,
        None => return Ok(false),
    };

    if !startup_barrier.try_begin_daemon_connection_emit() {
        return Ok(false);
    }

    let window = match app_handle.get_webview_window("main") {
        Some(window) => window,
        None => {
            startup_barrier.release_daemon_connection_emit();
            return Err(DaemonBootstrapError::MainWindowUnavailable);
        }
    };

    let payload = DaemonConnectionPayload::from(&connection_info);
    if let Err(error) = window.emit(DAEMON_CONNECTION_EVENT, payload) {
        startup_barrier.release_daemon_connection_emit();
        return Err(DaemonBootstrapError::Emit(anyhow::Error::new(error)));
    }

    Ok(true)
}

async fn bootstrap_daemon_connection_with<Spawn, Probe, ProbeFuture, LoadInfo>(
    state: &DaemonConnectionState,
    spawn: Spawn,
    mut probe: Probe,
    load_connection_info: LoadInfo,
    timeout: Duration,
    poll_interval: Duration,
) -> Result<DaemonConnectionInfo, DaemonBootstrapError>
where
    Spawn: FnOnce() -> Result<(), DaemonBootstrapError>,
    Probe: FnMut() -> ProbeFuture,
    ProbeFuture: Future<Output = Result<bool, DaemonBootstrapError>>,
    LoadInfo: Fn() -> Result<DaemonConnectionInfo, DaemonBootstrapError>,
{
    if !probe().await? {
        spawn()?;
        wait_for_daemon_health(&mut probe, timeout, poll_interval).await?;
    }

    let connection_info = load_connection_info()?;
    state.set(connection_info.clone());
    Ok(connection_info)
}

async fn wait_for_daemon_health<Probe, ProbeFuture>(
    probe: &mut Probe,
    timeout: Duration,
    poll_interval: Duration,
) -> Result<(), DaemonBootstrapError>
where
    Probe: FnMut() -> ProbeFuture,
    ProbeFuture: Future<Output = Result<bool, DaemonBootstrapError>>,
{
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if probe().await? {
            return Ok(());
        }

        if tokio::time::Instant::now() >= deadline {
            return Err(DaemonBootstrapError::StartupTimeout {
                timeout_ms: timeout.as_millis() as u64,
            });
        }

        tokio::time::sleep(poll_interval).await;
    }
}

pub async fn probe_daemon_health(client: &reqwest::Client) -> Result<bool, DaemonBootstrapError> {
    let addr = resolve_daemon_http_addr();
    probe_daemon_health_at(client, addr).await
}

async fn probe_daemon_health_at(
    client: &reqwest::Client,
    addr: std::net::SocketAddr,
) -> Result<bool, DaemonBootstrapError> {
    let url = format!("http://{}:{}{}", addr.ip(), addr.port(), HEALTH_PATH);

    let response = match client.get(url).send().await {
        Ok(response) => response,
        Err(error) if error.is_connect() || error.is_timeout() => return Ok(false),
        Err(error) => {
            return Err(DaemonBootstrapError::Probe(
                anyhow::Error::new(error).context("daemon health probe request failed"),
            ))
        }
    };

    if !response.status().is_success() {
        return Ok(false);
    }

    let health = response.json::<HealthResponse>().await.map_err(|error| {
        DaemonBootstrapError::Probe(
            anyhow::Error::new(error).context("failed to decode daemon health response"),
        )
    })?;

    Ok(health.status == "ok")
}

fn load_daemon_connection_info() -> Result<DaemonConnectionInfo, DaemonBootstrapError> {
    let token_path = resolve_token_path();
    let token = std::fs::read_to_string(&token_path).map_err(|error| {
        DaemonBootstrapError::ConnectionInfo(anyhow::Error::new(error).context(format!(
            "failed to read daemon auth token at {}",
            token_path.display()
        )))
    })?;
    let token = token.trim().to_string();
    if token.is_empty() {
        return Err(DaemonBootstrapError::ConnectionInfo(anyhow::anyhow!(
            "daemon auth token at {} is empty",
            token_path.display()
        )));
    }

    let addr = resolve_daemon_http_addr();
    Ok(DaemonConnectionInfo {
        base_url: format!("http://{}:{}", addr.ip(), addr.port()),
        ws_url: format!("ws://{}:{}/ws", addr.ip(), addr.port()),
        token,
    })
}

fn spawn_daemon_process() -> Result<Child, DaemonBootstrapError> {
    let daemon_binary = resolve_daemon_binary_path().map_err(|error| {
        DaemonBootstrapError::Spawn(
            anyhow::Error::new(error).context("failed to resolve daemon binary path"),
        )
    })?;

    Command::new(&daemon_binary)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| {
            DaemonBootstrapError::Spawn(anyhow::Error::new(error).context(format!(
                "failed to spawn {} from {}",
                DAEMON_BINARY_NAME,
                daemon_binary.display()
            )))
        })
}

fn resolve_daemon_binary_path() -> std::io::Result<PathBuf> {
    let current_exe = std::env::current_exe()?;
    let binary_name = daemon_binary_name();
    let sibling = current_exe
        .parent()
        .map(|parent| parent.join(binary_name))
        .filter(|candidate| candidate.exists());

    Ok(sibling.unwrap_or_else(|| PathBuf::from(binary_name)))
}

fn daemon_binary_name() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "uniclipboard-daemon.exe"
    }
    #[cfg(not(target_os = "windows"))]
    {
        DAEMON_BINARY_NAME
    }
}

fn resolve_token_path() -> PathBuf {
    let socket_path = resolve_daemon_socket_path();
    let token_base_dir = socket_path.parent().unwrap_or_else(|| Path::new("/tmp"));
    resolve_daemon_token_path(token_base_dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn probe_helper_returns_success_on_healthy_health_endpoint() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buffer = [0_u8; 1024];
            let _ = stream.read(&mut buffer).await.unwrap();
            let body = r#"{"status":"ok","version":"0.1.0"}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        });

        let client = reqwest::Client::builder()
            .timeout(PROBE_TIMEOUT)
            .build()
            .unwrap();
        let is_healthy = probe_daemon_health_at(&client, addr).await.unwrap();

        assert!(is_healthy);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn startup_helper_treats_spawn_failure_as_error() {
        let state = DaemonConnectionState::default();
        let result = bootstrap_daemon_connection_with(
            &state,
            || Err(DaemonBootstrapError::Spawn(anyhow::anyhow!("spawn failed"))),
            || async { Ok(false) },
            || unreachable!(),
            Duration::from_millis(10),
            Duration::from_millis(1),
        )
        .await;

        assert!(matches!(result, Err(DaemonBootstrapError::Spawn(_))));
    }

    #[tokio::test]
    async fn startup_helper_treats_timeout_as_error() {
        let state = DaemonConnectionState::default();
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_for_probe = attempts.clone();
        let result = bootstrap_daemon_connection_with(
            &state,
            || Ok(()),
            move || {
                let attempts_for_probe = attempts_for_probe.clone();
                async move {
                    attempts_for_probe.fetch_add(1, Ordering::SeqCst);
                    Ok(false)
                }
            },
            || unreachable!(),
            Duration::from_millis(20),
            Duration::from_millis(5),
        )
        .await;

        assert!(matches!(
            result,
            Err(DaemonBootstrapError::StartupTimeout { .. })
        ));
        assert!(attempts.load(Ordering::SeqCst) >= 2);
    }

    #[test]
    fn emitted_event_payload_uses_camel_case_keys() {
        let payload = DaemonConnectionPayload::from(&DaemonConnectionInfo {
            base_url: "http://127.0.0.1:42715".to_string(),
            ws_url: "ws://127.0.0.1:42715/ws".to_string(),
            token: "secret".to_string(),
        });

        let value = serde_json::to_value(payload).unwrap();

        assert_eq!(value["baseUrl"], "http://127.0.0.1:42715");
        assert_eq!(value["wsUrl"], "ws://127.0.0.1:42715/ws");
        assert_eq!(value["token"], "secret");
        assert!(value.get("base_url").is_none());
        assert!(value.get("ws_url").is_none());
    }
}
