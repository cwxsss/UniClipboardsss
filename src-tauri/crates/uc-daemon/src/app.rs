//! # DaemonApp
//!
//! Top-level daemon lifecycle: starts the HTTP API server and services,
//! waits for shutdown signal, and tears down in reverse order.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::broadcast;
use tokio::sync::RwLock;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};
use uc_app::runtime::CoreRuntime;
use uc_app::usecases::CoreUseCases;
use uc_application::facade::{
    AppFacade, AppFacadeParts, ClipboardHistoryFacade, ClipboardHistoryFacadeDeps,
    ClipboardRestoreError, ClipboardRestoreFacade, ClipboardRestoreGateway, DeviceFacade,
    EncryptionFacade, EncryptionFacadeDeps, LifecycleFacade, LifecycleFacadeDeps,
    LifecycleStateView as LifecycleState, LifecycleStateView, LifecycleStatusGateway,
    MemberRosterFacade, ResourceFacade, ResourceFacadeDeps, SearchFacade, SearchFacadeError,
    SearchGateway, SearchPageView, SearchRebuildAcceptedView, SearchResultView, SearchStatusView,
    SettingsFacade, SpaceSetupFacade, StorageFacade, StorageFacadeDeps,
};
use uc_application::facade::{ManualRebuildResult, SearchCoordinator};
use uc_application::space_access::SpaceAccessFacade;
use uc_core::ids::EntryId;
use uc_core::search::{ContentType as SearchContentType, SearchQuery, SearchResultsPage};

use crate::api::auth::load_or_create_auth_token;
use crate::api::event_emitter::DaemonApiEventEmitter;
use crate::api::query::DaemonQueryService;
use crate::api::server::{run_http_server, DaemonApiState};
use crate::api::setup_events::spawn_pairing_completion_forwarder;
use crate::api::types::DaemonWsEvent;
use crate::peers::presence_monitor::PresenceMonitor;
use crate::process_metadata::DaemonPidManager;
use crate::security::{cleanup_rate_limiter_task, SecurityState};
use crate::service::DaemonService;
use crate::state::RuntimeState;

/// Recover encryption session from disk/keyring if encryption has been initialized.
///
/// # Parameters
///
/// - `runtime`: The core runtime
/// - `auto_unlock_enabled`: Whether to attempt automatic unlock via keyring
///
/// # Returns
///
/// - `Ok(true)`: Session was successfully unlocked (encryption initialized + unlock succeeded)
/// - `Ok(false)`: Session was NOT unlocked — either encryption is uninitialized, or
///   `auto_unlock_enabled` is false while encryption is initialized (requires manual unlock)
/// - `Err`: Unlock failed (daemon must not start in this case)
///
/// This function is `pub` so `main.rs` can call it BEFORE constructing `DaemonApp`,
/// using the result to decide whether to start `PeerDiscoveryWorker` immediately or defer.
pub async fn recover_encryption_session(
    runtime: &CoreRuntime,
    auto_unlock_enabled: bool,
) -> anyhow::Result<bool> {
    // Phase C: setup 完成真相源改为 `SetupStatus.has_completed`(取代原
    // `EncryptionStatePort.load_state()` marker 文件)。
    let setup_completed = runtime
        .wiring_deps()
        .setup_status
        .get_status()
        .await
        .map(|s| s.has_completed)
        .map_err(|e| anyhow::anyhow!("failed to load setup status: {}", e))?;

    // When auto-unlock is disabled, skip the unlock attempt entirely.
    // If setup has already been completed, return false so the GUI can prompt for manual unlock.
    // Otherwise, return false (no unlock needed — setup flow handles it).
    if !auto_unlock_enabled {
        if setup_completed {
            info!("Auto-unlock disabled via settings — skipping encryption session recovery");
        } else {
            info!("Setup not completed, skipping session recovery");
        }
        return Ok(false);
    }

    // Auto-unlock enabled: attempt to recover the session from keyring.
    let usecases = CoreUseCases::new(runtime);
    let uc = usecases.auto_unlock_encryption_session();
    match uc.execute().await {
        Ok(true) => {
            info!("Encryption session recovered from disk");
            Ok(true)
        }
        Ok(false) => {
            info!("Encryption not initialized, skipping session recovery");
            Ok(false)
        }
        Err(e) => {
            error!(error = %e, "Encryption session recovery failed");
            anyhow::bail!(
                "Cannot start daemon: encryption session recovery failed: {}",
                e
            )
        }
    }
}

/// Main daemon application.
///
/// Owns the service list and cancellation token.
/// Services use `Arc<dyn DaemonService>` (not `Box`) to allow cloning
/// for `tokio::spawn` `'static` requirement.
pub struct DaemonApp {
    services: Vec<Arc<dyn DaemonService>>,
    runtime: Arc<CoreRuntime>,
    state: Arc<RwLock<RuntimeState>>,
    event_tx: broadcast::Sender<DaemonWsEvent>,
    space_access_facade: Option<Arc<SpaceAccessFacade>>,
    cancel: CancellationToken,
    // Deferred services: clipboard-watcher, inbound-clipboard-sync, and peer-discovery
    // are deferred until the GUI signals ready (--gui-managed) or setup completes (uninitialized).
    deferred_services: Vec<Arc<dyn DaemonService>>,
    /// Notify triggered by either SetupCompletionEmitter or /lifecycle/ready API.
    deferred_ready_notify: Option<Arc<tokio::sync::Notify>>,
    /// External shutdown signal (e.g., from stdin pipe tether when GUI-managed).
    /// When cancelled, the daemon performs a graceful shutdown identical to SIGTERM.
    external_shutdown: Option<CancellationToken>,
    /// Gate that controls clipboard capture. Passed to DaemonApiState so the
    /// `/lifecycle/ready` endpoint can open it when the GUI signals readiness.
    clipboard_capture_gate: Option<Arc<AtomicBool>>,
    /// Search coordinator — wired into DaemonApiState for HTTP route access.
    search_coordinator: Option<Arc<SearchCoordinator>>,
    /// Slice4 P3 T3.3 — `SpaceSetupFacade` injected into `DaemonApiState`
    /// so the `/v2/setup/*` handlers can drive real pairing flows. Same
    /// `Arc` the keepalive worker holds, sourced from
    /// `space_setup_assembly.facade`. Also subscribed at `run()` to fan
    /// `PairingOutcome` events out as `setup.pairingCompleted` ws frames.
    space_setup_facade: Option<Arc<SpaceSetupFacade>>,
    member_roster_facade: Option<Arc<MemberRosterFacade>>,
    /// Local device id (sponsor view) baked in at construction so the
    /// pairing-completion forwarder doesn't need to pull
    /// `DeviceIdentityPort` at event time. Pre-resolved in `entrypoint`.
    local_device_id: Option<String>,
}

impl DaemonApp {
    /// Create a new DaemonApp with the given services and socket path.
    ///
    /// The `event_tx` is created by the caller and shared with all services
    /// that emit WebSocket events, so they all write to the same broadcast channel.
    pub fn new(
        services: Vec<Arc<dyn DaemonService>>,
        runtime: Arc<CoreRuntime>,
        state: Arc<RwLock<RuntimeState>>,
        event_tx: broadcast::Sender<DaemonWsEvent>,
        space_access_facade: Option<Arc<SpaceAccessFacade>>,
    ) -> Self {
        Self {
            services,
            runtime,
            state,
            event_tx,
            space_access_facade,
            cancel: CancellationToken::new(),
            deferred_services: Vec::new(),
            deferred_ready_notify: None,
            external_shutdown: None,
            clipboard_capture_gate: None,
            search_coordinator: None,
            space_setup_facade: None,
            member_roster_facade: None,
            local_device_id: None,
        }
    }

    /// Construct a DaemonApp with deferred services support.
    ///
    /// Services in `deferred_services` will only start when `setup_complete_rx` fires.
    /// This is used for:
    /// - `--gui-managed` mode: clipboard services are deferred until the GUI signals unlock
    /// - Uninitialized encryption: peer-discovery is deferred until setup completes
    ///
    /// `encryption_unlocked` is a required parameter to enforce the invariant that
    /// the caller MUST have completed encryption recovery before constructing DaemonApp.
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_deferred(
        services: Vec<Arc<dyn DaemonService>>,
        runtime: Arc<CoreRuntime>,
        state: Arc<RwLock<RuntimeState>>,
        event_tx: broadcast::Sender<DaemonWsEvent>,
        space_access_facade: Option<Arc<SpaceAccessFacade>>,
        _encryption_unlocked: bool,
        deferred_services: Vec<Arc<dyn DaemonService>>,
        deferred_ready_notify: Option<Arc<tokio::sync::Notify>>,
        external_shutdown: Option<CancellationToken>,
        clipboard_capture_gate: Option<Arc<AtomicBool>>,
        search_coordinator: Option<Arc<SearchCoordinator>>,
        space_setup_facade: Option<Arc<SpaceSetupFacade>>,
        member_roster_facade: Option<Arc<MemberRosterFacade>>,
        local_device_id: Option<String>,
    ) -> Self {
        // Validate invariant: deferred_services and deferred_ready_notify must be
        // consistent. If there are deferred services, there must be a Notify to trigger them.
        debug_assert!(
            deferred_services.is_empty() || deferred_ready_notify.is_some(),
            "deferred_services is non-empty but deferred_ready_notify is None — services would never start"
        );
        // Slice4 P3 T3.3 invariant: when the new SpaceSetupFacade is wired
        // in, the sponsor device id must be too — the pairing-completion
        // forwarder needs both to emit `setup.pairingCompleted`.
        debug_assert!(
            space_setup_facade.is_some() == local_device_id.is_some(),
            "space_setup_facade and local_device_id must be wired together"
        );
        Self {
            services,
            runtime,
            state,
            event_tx,
            space_access_facade,
            cancel: CancellationToken::new(),
            deferred_services,
            deferred_ready_notify,
            external_shutdown,
            clipboard_capture_gate,
            search_coordinator,
            space_setup_facade,
            member_roster_facade,
            local_device_id,
        }
    }

    /// Run the daemon: start the HTTP API server and services, wait for shutdown, cleanup.
    ///
    /// NOTE: `recover_encryption_session` is called in `main.rs` BEFORE constructing
    /// `DaemonApp`, so it does NOT appear here (Phase 67: moved for deferred-start logic).
    pub async fn run(mut self) -> anyhow::Result<()> {
        info!("uniclipboard-daemon starting");

        // 1. Load or create auth token (stored alongside PID metadata)
        let storage_paths = self.runtime.storage_paths();
        let token_path = storage_paths.daemon_token_path();
        debug!(
            token_path = %token_path.display(),
            "loading daemon auth token"
        );
        let auth_token = load_or_create_auth_token(&token_path)?;
        let pid_manager = DaemonPidManager::new(storage_paths.clone());
        let _pid_file_guard = DaemonPidFileGuard::activate(pid_manager.clone())?;
        let pid = pid_manager.write_current_pid()?;
        info!(pid, "wrote daemon pid metadata");
        let app_facade = Arc::new(AppFacade::new(AppFacadeParts {
            space_setup: self.space_setup_facade.clone(),
            member_roster: self.member_roster_facade.clone(),
            lifecycle: Arc::new(LifecycleFacade::new(LifecycleFacadeDeps {
                status: Arc::new(AppLifecycleStatusGateway {
                    status: self.runtime.lifecycle_status().clone(),
                }),
            })),
            encryption: Arc::new(EncryptionFacade::new(EncryptionFacadeDeps {
                setup_status: self.runtime.wiring_deps().setup_status.clone(),
                space_access: self.runtime.wiring_deps().security.space_access.clone(),
            })),
            resource: Arc::new(ResourceFacade::new(ResourceFacadeDeps {
                representation_repo: self
                    .runtime
                    .wiring_deps()
                    .clipboard
                    .representation_repo
                    .clone(),
                thumbnail_repo: self.runtime.wiring_deps().storage.thumbnail_repo.clone(),
                blob_store: self.runtime.wiring_deps().storage.blob_store.clone(),
            })),
            clipboard_history: Arc::new(ClipboardHistoryFacade::new(ClipboardHistoryFacadeDeps {
                entry_repo: self
                    .runtime
                    .wiring_deps()
                    .clipboard
                    .clipboard_entry_repo
                    .clone(),
                selection_repo: self.runtime.wiring_deps().clipboard.selection_repo.clone(),
                representation_repo: self
                    .runtime
                    .wiring_deps()
                    .clipboard
                    .representation_repo
                    .clone(),
                event_writer: self
                    .runtime
                    .wiring_deps()
                    .clipboard
                    .clipboard_event_repo
                    .clone(),
                payload_resolver: self
                    .runtime
                    .wiring_deps()
                    .clipboard
                    .payload_resolver
                    .clone(),
                blob_store: self.runtime.wiring_deps().storage.blob_store.clone(),
                thumbnail_repo: self.runtime.wiring_deps().storage.thumbnail_repo.clone(),
                file_transfer_repo: self
                    .runtime
                    .wiring_deps()
                    .storage
                    .file_transfer_repo
                    .clone(),
                search_index: Some(self.runtime.wiring_deps().search.search_index.clone()),
                file_cache_dir: Some(storage_paths.cache_dir.clone()),
            })),
            clipboard_restore: Arc::new(ClipboardRestoreFacade::new(Arc::new(
                DaemonClipboardRestoreGateway {
                    runtime: self.runtime.clone(),
                },
            ))),
            search: Arc::new(SearchFacade::new(Box::new(DaemonSearchGateway {
                runtime: self.runtime.clone(),
                coordinator: self.search_coordinator.clone(),
            }))),
            settings: Arc::new(SettingsFacade::new(
                self.runtime.wiring_deps().settings.clone(),
            )),
            device: Arc::new(DeviceFacade::new(
                self.runtime.wiring_deps().device.device_identity.clone(),
                self.runtime.wiring_deps().settings.clone(),
            )),
            storage: Arc::new(StorageFacade::new(StorageFacadeDeps {
                db_path: storage_paths.db_path.clone(),
                vault_dir: storage_paths.vault_dir.clone(),
                cache_dir: storage_paths.cache_dir.clone(),
                logs_dir: storage_paths.logs_dir.clone(),
                app_data_root_dir: storage_paths.app_data_root_dir.clone(),
                cache_fs: self.runtime.wiring_deps().system.cache_fs.clone(),
            })),
        }));
        let query_service = Arc::new(DaemonQueryService::new(
            self.state.clone(),
            Arc::clone(&app_facade),
        ));
        let presence_monitor = Arc::new(PresenceMonitor::new(
            Arc::clone(&app_facade),
            self.event_tx.clone(),
        ));

        // 2. Build security state and register daemon's own PID
        let security = Arc::new(SecurityState::new());
        security.register_pid(pid).await;

        // 3. Build API state using the shared event_tx (same channel used by all services)
        let mut api_state = DaemonApiState::new(
            query_service,
            auth_token,
            Some(self.runtime.clone()),
            security,
        );
        // Replace the default-created channel with our shared one so all services
        // emit to the same broadcast channel that WebSocket subscribers receive from.
        api_state.event_tx = self.event_tx.clone();
        let api_state = match &self.space_access_facade {
            Some(sao) => api_state.with_space_access(sao.clone()),
            None => api_state,
        };
        let api_state = match &self.clipboard_capture_gate {
            Some(gate) => api_state.with_clipboard_gate(Arc::clone(gate)),
            None => api_state,
        };
        let api_state = match &self.deferred_ready_notify {
            Some(notify) => api_state.with_deferred_ready_notify(Arc::clone(notify)),
            None => api_state,
        };
        let api_state = api_state.with_app_facade(app_facade);

        // 3. Wire the event emitter into the runtime so use cases can emit WS events
        self.runtime
            .set_event_emitter(Arc::new(DaemonApiEventEmitter::new(self.event_tx.clone())));

        info!("uniclipboard-daemon running");

        // 4. Start ALL services uniformly via JoinSet
        let mut service_tasks = JoinSet::new();
        self.services
            .push(Arc::clone(&presence_monitor) as Arc<dyn DaemonService>);
        for service in &self.services {
            let svc = Arc::clone(service);
            let token = self.cancel.child_token();
            service_tasks.spawn(async move { svc.start(token).await });
        }

        // Slice4 P3 T3.3: spawn the sponsor-side pairing-completion
        // forwarder before the HTTP server. Subscribes to the facade's
        // broadcast stream and translates each `PairingOutcome` into a
        // `setup.pairingCompleted` ws frame on the shared event bus.
        // Lives until `self.cancel` fires.
        if let (Some(facade), Some(sponsor_id)) = (
            self.space_setup_facade.as_ref(),
            self.local_device_id.as_ref(),
        ) {
            spawn_pairing_completion_forwarder(
                facade.subscribe_pairing_completion(),
                self.event_tx.clone(),
                sponsor_id.clone(),
                self.cancel.child_token(),
            );
        }

        // 5. Spawn HTTP server and rate limiter cleanup task
        // Clone security and cancel BEFORE moving api_state into the HTTP server
        let security_for_cleanup = api_state.security.clone();
        let cleanup_cancel = self.cancel.child_token();
        let http_cancel = self.cancel.child_token();
        let mut http_handle = tokio::spawn(run_http_server(api_state, http_cancel));

        // Rate limiter cleanup: runs every 5 minutes, respects cleanup_cancel
        let _cleanup_handle = cleanup_rate_limiter_task(security_for_cleanup, cleanup_cancel);

        // Prepare deferred services start
        let mut deferred = std::mem::take(&mut self.deferred_services);
        let ready_notify = self.deferred_ready_notify.take();

        // 6. Wait for shutdown signal, infrastructure crash, service crash, or deferred start
        loop {
            tokio::select! {
                _ = wait_for_shutdown_signal() => {
                    info!("shutdown signal received");
                    break;
                }
                _ = async {
                    match &self.external_shutdown {
                        Some(token) => token.cancelled().await,
                        None => std::future::pending::<()>().await,
                    }
                } => {
                    info!("external shutdown signal received (parent process gone)");
                    break;
                }
                result = &mut http_handle => {
                    warn!("HTTP server exited unexpectedly: {:?}", result);
                    break;
                }
                Some(result) = service_tasks.join_next() => {
                    warn!("service task exited unexpectedly: {:?}", result);
                    break;
                }
                _ = async {
                    match &ready_notify {
                        Some(n) => n.notified().await,
                        None => std::future::pending::<()>().await,
                    }
                }, if !deferred.is_empty() => {
                    info!(
                        count = deferred.len(),
                        "ready signal received — starting deferred services"
                    );
                    for worker in deferred.drain(..) {
                        let name = worker.name().to_string();
                        info!(service = %name, "starting deferred service");
                        let worker_for_shutdown: Arc<dyn DaemonService> = Arc::clone(&worker);
                        let token = self.cancel.child_token();
                        service_tasks.spawn(async move { worker.start(token).await });
                        self.services.push(worker_for_shutdown);
                        {
                            let mut state = self.state.write().await;
                            state.update_service_health(&name, crate::service::ServiceHealth::Healthy);
                        }
                    }
                    // continue loop — don't break, daemon keeps running
                }
            }
        }

        // 7. Shutdown sequence
        info!("shutting down...");
        self.cancel.cancel();

        // Drain service tasks with timeout
        tokio::time::timeout(Duration::from_secs(5), async {
            while service_tasks.join_next().await.is_some() {}
        })
        .await
        .ok();

        // Await HTTP server with timeout
        tokio::time::timeout(Duration::from_secs(5), http_handle)
            .await
            .ok();

        // Stop services in reverse order
        for service in self.services.iter().rev() {
            info!(service = service.name(), "stopping service");
            if let Err(e) = service.stop().await {
                warn!(service = service.name(), "error stopping service: {}", e);
            }
        }

        info!("uniclipboard-daemon stopped");
        Ok(())
    }
}

struct AppLifecycleStatusGateway {
    status: Arc<dyn LifecycleStatusGateway>,
}

#[async_trait]
impl LifecycleStatusGateway for AppLifecycleStatusGateway {
    async fn set_state(&self, state: LifecycleStateView) -> anyhow::Result<()> {
        self.status
            .set_state(lifecycle_state_from_view(state))
            .await
    }

    async fn get_state(&self) -> LifecycleStateView {
        lifecycle_state_to_view(self.status.get_state().await)
    }
}

fn lifecycle_state_to_view(state: LifecycleState) -> LifecycleStateView {
    match state {
        LifecycleState::Idle => LifecycleStateView::Idle,
        LifecycleState::Pending => LifecycleStateView::Pending,
        LifecycleState::Ready => LifecycleStateView::Ready,
        LifecycleState::NetworkFailed => LifecycleStateView::NetworkFailed,
    }
}

fn lifecycle_state_from_view(state: LifecycleStateView) -> LifecycleState {
    match state {
        LifecycleStateView::Idle => LifecycleState::Idle,
        LifecycleStateView::Pending => LifecycleState::Pending,
        LifecycleStateView::Ready => LifecycleState::Ready,
        LifecycleStateView::NetworkFailed => LifecycleState::NetworkFailed,
    }
}

struct DaemonSearchGateway {
    runtime: Arc<CoreRuntime>,
    coordinator: Option<Arc<SearchCoordinator>>,
}

#[async_trait]
impl SearchGateway for DaemonSearchGateway {
    async fn query(&self, query: SearchQuery) -> Result<SearchPageView, SearchFacadeError> {
        let usecases = CoreUseCases::new(self.runtime.as_ref());
        let page = usecases
            .search_clipboard_entries()
            .execute(query)
            .await
            .map_err(uc_application::facade::map_search_error)?;
        Ok(search_page_to_view(page))
    }

    async fn status(&self) -> Result<SearchStatusView, SearchFacadeError> {
        let coordinator = self.coordinator.as_ref().ok_or_else(|| {
            SearchFacadeError::ServiceUnavailable("search coordinator unavailable".to_string())
        })?;
        coordinator
            .status_view()
            .await
            .map_err(uc_application::facade::map_search_error)
    }

    async fn request_rebuild(&self) -> Result<SearchRebuildAcceptedView, SearchFacadeError> {
        let coordinator = self.coordinator.as_ref().ok_or_else(|| {
            SearchFacadeError::ServiceUnavailable("search coordinator unavailable".to_string())
        })?;

        match coordinator.request_manual_rebuild().await {
            ManualRebuildResult::Accepted => Ok(SearchRebuildAcceptedView { accepted: true }),
            ManualRebuildResult::AlreadyInProgress => Err(SearchFacadeError::RebuildAlreadyRunning),
        }
    }
}

fn search_page_to_view(page: SearchResultsPage) -> SearchPageView {
    SearchPageView {
        total: page.total,
        has_more: page.has_more,
        items: page
            .items
            .into_iter()
            .map(|item| SearchResultView {
                entry_id: item.entry_id.to_string(),
                content_type: search_content_type_to_string(&item.content_type),
                active_time_ms: item.active_time_ms,
                text_preview: item.text_preview,
                mime_type: item.mime_type,
                file_extensions: item.file_extensions,
            })
            .collect(),
    }
}

fn search_content_type_to_string(content_type: &SearchContentType) -> String {
    match content_type {
        SearchContentType::Text => "text",
        SearchContentType::Html => "html",
        SearchContentType::Link => "link",
        SearchContentType::File => "file",
        SearchContentType::Image => "image",
        SearchContentType::Other => "other",
    }
    .to_string()
}

struct DaemonClipboardRestoreGateway {
    runtime: Arc<CoreRuntime>,
}

#[async_trait]
impl ClipboardRestoreGateway for DaemonClipboardRestoreGateway {
    async fn restore_entry(&self, entry_id: &str) -> Result<(), ClipboardRestoreError> {
        let parsed_id = EntryId::from(entry_id);
        let usecases = CoreUseCases::new(self.runtime.as_ref());
        let restore_uc = usecases
            .restore_clipboard_selection()
            .map_err(|err| ClipboardRestoreError::Internal(err.to_string()))?;

        restore_uc.execute(&parsed_id).await.map_err(|err| {
            let message = err.to_string();
            if message.to_lowercase().contains("not found") {
                ClipboardRestoreError::NotFound
            } else {
                ClipboardRestoreError::Internal(message)
            }
        })?;

        if let Err(err) = usecases.touch_clipboard_entry().execute(&parsed_id).await {
            tracing::warn!(error = %err, entry_id = %entry_id, "touch_clipboard_entry failed after restore");
        }

        Ok(())
    }
}

struct DaemonPidFileGuard {
    manager: DaemonPidManager,
}

impl DaemonPidFileGuard {
    fn activate(manager: DaemonPidManager) -> anyhow::Result<Self> {
        let pid = manager.write_current_pid()?;
        info!(pid, "wrote daemon pid metadata");
        Ok(Self { manager })
    }
}

impl Drop for DaemonPidFileGuard {
    fn drop(&mut self) {
        if let Err(error) = self.manager.remove_pid_file() {
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
