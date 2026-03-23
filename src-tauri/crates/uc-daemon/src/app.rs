//! # DaemonApp
//!
//! Top-level daemon lifecycle: binds the RPC socket, starts workers,
//! waits for shutdown signal, and tears down in reverse order.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::net::UnixListener;
use tokio::sync::mpsc;
use tokio::sync::RwLock;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};
use uc_app::runtime::CoreRuntime;
use uc_app::usecases::space_access::SpaceAccessOrchestrator;
use uc_app::usecases::PairingOrchestrator;
use uc_core::network::pairing_state_machine::PairingAction;
use uc_infra::fs::key_slot_store::KeySlotStore;

use crate::api::auth::{load_or_create_auth_token, resolve_daemon_token_path};
use crate::api::event_emitter::DaemonApiEventEmitter;
use crate::api::query::DaemonQueryService;
use crate::api::server::{run_http_server, DaemonApiState};
use crate::pairing::host::DaemonPairingHost;
use crate::process_metadata::{remove_pid_file, write_current_pid};
use crate::rpc::server::{check_or_remove_stale_socket, run_rpc_accept_loop};
use crate::state::{DaemonWorkerSnapshot, RuntimeState};
use crate::worker::{DaemonWorker, WorkerHealth};

/// Main daemon application.
///
/// Owns the worker list, RPC state, and cancellation token.
/// Workers use `Arc<dyn DaemonWorker>` (not `Box`) to allow cloning
/// for `tokio::spawn` `'static` requirement.
pub struct DaemonApp {
    workers: Vec<Arc<dyn DaemonWorker>>,
    runtime: Arc<CoreRuntime>,
    state: Arc<RwLock<RuntimeState>>,
    pairing_orchestrator: Arc<PairingOrchestrator>,
    pairing_action_rx: mpsc::Receiver<PairingAction>,
    space_access_orchestrator: Arc<SpaceAccessOrchestrator>,
    key_slot_store: Arc<dyn KeySlotStore>,
    socket_path: PathBuf,
    cancel: CancellationToken,
}

impl DaemonApp {
    /// Create a new DaemonApp with the given workers and socket path.
    pub fn new(
        workers: Vec<Arc<dyn DaemonWorker>>,
        runtime: Arc<CoreRuntime>,
        pairing_orchestrator: Arc<PairingOrchestrator>,
        pairing_action_rx: mpsc::Receiver<PairingAction>,
        space_access_orchestrator: Arc<SpaceAccessOrchestrator>,
        key_slot_store: Arc<dyn KeySlotStore>,
        socket_path: PathBuf,
    ) -> Self {
        let initial_statuses: Vec<DaemonWorkerSnapshot> = workers
            .iter()
            .map(|w| DaemonWorkerSnapshot {
                name: w.name().to_string(),
                health: WorkerHealth::Healthy,
            })
            .collect();

        Self {
            workers,
            runtime,
            state: Arc::new(RwLock::new(RuntimeState::new(initial_statuses))),
            pairing_orchestrator,
            pairing_action_rx,
            space_access_orchestrator,
            key_slot_store,
            socket_path,
            cancel: CancellationToken::new(),
        }
    }

    /// Run the daemon: bind RPC socket, start workers, wait for shutdown, cleanup.
    pub async fn run(self) -> anyhow::Result<()> {
        info!("uniclipboard-daemon starting");

        // 1. Bind RPC socket FIRST (fail-fast before starting workers)
        check_or_remove_stale_socket(&self.socket_path).await?;
        let listener = UnixListener::bind(&self.socket_path)?;
        let token_base_dir = self
            .socket_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("/tmp"));
        let token_path = resolve_daemon_token_path(token_base_dir);
        let auth_token = load_or_create_auth_token(&token_path)?;
        let _pid_file_guard = DaemonPidFileGuard::activate()?;
        let query_service = Arc::new(DaemonQueryService::new(
            self.runtime.clone(),
            self.state.clone(),
        ));
        let api_state = DaemonApiState::new(query_service, auth_token, Some(self.runtime.clone()))
            .with_setup(self.runtime.setup_orchestrator().clone());
        self.runtime
            .set_event_emitter(Arc::new(DaemonApiEventEmitter::new(
                api_state.event_tx.clone(),
            )));
        let pairing_host = Arc::new(DaemonPairingHost::new(
            self.runtime.clone(),
            self.pairing_orchestrator.clone(),
            self.pairing_action_rx,
            self.state.clone(),
            self.space_access_orchestrator.clone(),
            self.key_slot_store.clone(),
            api_state.event_tx.clone(),
        ));
        let api_state = api_state.with_pairing_host(Arc::clone(&pairing_host));

        info!("uniclipboard-daemon running, RPC at {:?}", self.socket_path);

        // 2. Start workers
        let mut worker_tasks = JoinSet::new();
        for worker in &self.workers {
            let w = Arc::clone(worker);
            let token = self.cancel.child_token();
            worker_tasks.spawn(async move { w.start(token).await });
        }

        // 3. Spawn accept loop and wait for shutdown signal, accept loop crash, or worker crash
        let rpc_state = self.state.clone();
        let rpc_cancel = self.cancel.child_token();
        let mut rpc_handle = tokio::spawn(run_rpc_accept_loop(listener, rpc_state, rpc_cancel));
        let http_cancel = self.cancel.child_token();
        let mut http_handle = tokio::spawn(run_http_server(api_state, http_cancel));
        let pairing_cancel = self.cancel.child_token();
        let mut pairing_handle = tokio::spawn(Arc::clone(&pairing_host).run(pairing_cancel));
        let mut completed_rpc_handle = false;
        let mut completed_http_handle = false;
        let mut completed_pairing_handle = false;

        tokio::select! {
            _ = wait_for_shutdown_signal() => {
                info!("shutdown signal received");
            }
            result = &mut rpc_handle => {
                completed_rpc_handle = true;
                warn!("RPC accept loop exited unexpectedly: {:?}", result);
            }
            result = &mut http_handle => {
                completed_http_handle = true;
                warn!("HTTP server exited unexpectedly: {:?}", result);
            }
            result = &mut pairing_handle => {
                completed_pairing_handle = true;
                warn!("pairing host exited unexpectedly: {:?}", result);
            }
            Some(result) = worker_tasks.join_next() => {
                warn!("worker task exited unexpectedly: {:?}", result);
            }
        }

        // 4. Shutdown sequence
        info!("shutting down...");

        // Cancel all child tokens
        self.cancel.cancel();

        // Drain worker tasks with timeout
        tokio::time::timeout(Duration::from_secs(5), async {
            while worker_tasks.join_next().await.is_some() {}
        })
        .await
        .ok();

        // Await RPC accept loop with timeout
        if !completed_rpc_handle {
            tokio::time::timeout(Duration::from_secs(5), rpc_handle)
                .await
                .ok();
        }
        if !completed_http_handle {
            tokio::time::timeout(Duration::from_secs(5), http_handle)
                .await
                .ok();
        }
        if !completed_pairing_handle {
            tokio::time::timeout(Duration::from_secs(5), pairing_handle)
                .await
                .ok();
        }

        // Stop workers in reverse order
        for worker in self.workers.iter().rev() {
            info!(worker = worker.name(), "stopping worker");
            if let Err(e) = worker.stop().await {
                warn!(worker = worker.name(), "error stopping worker: {}", e);
            }
        }

        // Remove socket file
        if let Err(e) = std::fs::remove_file(&self.socket_path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                warn!("failed to remove socket file: {}", e);
            }
        }

        info!("uniclipboard-daemon stopped");
        Ok(())
    }
}

struct DaemonPidFileGuard;

impl DaemonPidFileGuard {
    fn activate() -> anyhow::Result<Self> {
        let pid = write_current_pid()?;
        info!(pid, "wrote daemon pid metadata");
        Ok(Self)
    }
}

impl Drop for DaemonPidFileGuard {
    fn drop(&mut self) {
        if let Err(error) = remove_pid_file() {
            warn!(error = %error, "failed to remove daemon pid metadata");
        }
    }
}

/// Wait for either Ctrl-C or SIGTERM (Unix).
async fn wait_for_shutdown_signal() -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigterm = signal(SignalKind::terminate())
            .map_err(|e| anyhow::anyhow!("failed to register SIGTERM handler: {}", e))?;
        tokio::select! {
            result = tokio::signal::ctrl_c() => {
                result.map_err(|e| anyhow::anyhow!("ctrl_c handler error: {}", e))?;
            }
            _ = sigterm.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c()
            .await
            .map_err(|e| anyhow::anyhow!("ctrl_c handler error: {}", e))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process_metadata::{read_pid_file, resolve_daemon_pid_path};
    use std::path::Path;
    use std::sync::{Mutex, OnceLock};

    fn with_daemon_env<T>(
        profile: Option<&str>,
        xdg_runtime_dir: Option<&Path>,
        f: impl FnOnce() -> T,
    ) -> T {
        static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        let _guard = ENV_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let previous_profile = std::env::var("UC_PROFILE").ok();
        let previous_xdg_runtime_dir = std::env::var("XDG_RUNTIME_DIR").ok();

        match profile {
            Some(profile) => std::env::set_var("UC_PROFILE", profile),
            None => std::env::remove_var("UC_PROFILE"),
        }
        match xdg_runtime_dir {
            Some(path) => std::env::set_var("XDG_RUNTIME_DIR", path),
            None => std::env::remove_var("XDG_RUNTIME_DIR"),
        }

        let result = f();

        match previous_profile {
            Some(profile) => std::env::set_var("UC_PROFILE", profile),
            None => std::env::remove_var("UC_PROFILE"),
        }
        match previous_xdg_runtime_dir {
            Some(path) => std::env::set_var("XDG_RUNTIME_DIR", path),
            None => std::env::remove_var("XDG_RUNTIME_DIR"),
        }

        result
    }

    #[test]
    fn run_method_contains_encryption_recovery_call() {
        // Structural verification split into two concerns (Codex R6-F1):
        // 1. run() calls the helper function before resource acquisition
        // 2. The helper function invokes the actual use case
        //
        // NOTE: We split at #[cfg(test)] to exclude this test module from the search,
        // preventing the test from being self-fulfilling (Codex R1-F1).
        let full_source = include_str!("app.rs");
        let prod_source = full_source.split("#[cfg(test)]").next().unwrap_or(full_source);

        // 1. run() calls recover_encryption_session helper (Codex R6-F1)
        let recovery_call_pos = prod_source.find("recover_encryption_session")
            .expect("DaemonApp::run() must call recover_encryption_session helper");

        // 2. Helper function invokes the actual use case with .execute().await (Codex R5-F1)
        assert!(
            prod_source.contains("auto_unlock_encryption_session"),
            "recover_encryption_session helper must call auto_unlock_encryption_session use case"
        );
        assert!(
            prod_source.contains(".execute().await"),
            "Recovery must actually call .execute().await on the use case"
        );

        // 3. Tracing span exists
        assert!(
            prod_source.contains("daemon.startup.recover_encryption_session"),
            "Recovery must be instrumented with a tracing span"
        );

        // 4. Recovery call appears BEFORE resource acquisition (Codex R2-F2)
        let socket_bind_pos = prod_source.find("check_or_remove_stale_socket")
            .expect("DaemonApp::run() must contain socket bind step");
        assert!(
            recovery_call_pos < socket_bind_pos,
            "Encryption recovery must run BEFORE resource acquisition (socket bind). \
             Recovery at byte {}, socket bind at byte {}",
            recovery_call_pos, socket_bind_pos
        );
    }

    #[test]
    fn daemon_pid_guard_removes_pid_file_on_drop() {
        let tempdir = tempfile::tempdir().expect("tempdir should be created");

        with_daemon_env(Some("a"), Some(tempdir.path()), || {
            {
                let _guard = DaemonPidFileGuard::activate().expect("pid guard should activate");
                assert_eq!(
                    read_pid_file()
                        .expect("pid file should be readable")
                        .expect("pid file should exist"),
                    std::process::id()
                );
                assert!(resolve_daemon_pid_path().exists());
            }

            assert!(!resolve_daemon_pid_path().exists());
            assert!(read_pid_file()
                .expect("pid file read should succeed")
                .is_none());
        });
    }
}
