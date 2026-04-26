//! # Tauri AppRuntime
//!
//! Tauri 端的运行时句柄。D14 (2026-04-26) 起,本类型不再持有
//! `uc_app::runtime::CoreRuntime`,而是直接持有:
//!
//! - `Arc<AppFacade>` —— 所有业务调用唯一入口(参见
//!   `uc-application/AGENTS.md` §11.4)
//! - 进程级零碎件:`task_registry` / `settings_port` / `storage_paths` /
//!   `event_emitter_cell` / `device_id` —— 这些是"外部环境/进程基础
//!   设施",不属于 application 层
//! - Tauri 专属:`app_handle`(setup 完成后注入)
//!
//! ## 与 daemon 的对齐
//!
//! 与 `uc-daemon::DaemonApp` 同款 —— bootstrap 期把 ports 拼成 facade,
//! commands / 业务代码只看见 `Arc<AppFacade>`,看不见 ports / deps /
//! coordinator / mode。
//!
//! ## 用法示例
//!
//! ```rust,ignore
//! use uc_tauri::bootstrap::AppRuntime;
//! use tauri::State;
//!
//! #[tauri::command]
//! async fn list_entries(
//!     runtime: State<'_, std::sync::Arc<AppRuntime>>,
//! ) -> Result<(), String> {
//!     let facade = runtime.app_facade();
//!     let entries = facade
//!         .clipboard_history
//!         .list_entry_projections(/* input */)
//!         .await
//!         .map_err(|e| e.to_string())?;
//!     Ok(())
//! }
//! ```

use std::sync::{Arc, RwLock};

use uc_app::task_registry::TaskRegistry;
use uc_app::AppDeps;
use uc_application::facade::{
    AppFacade, AppFacadeParts, AppPaths, ClipboardHistoryFacade, ClipboardHistoryFacadeDeps,
    ClipboardRestoreFacade, ClipboardRestoreFacadeDeps, DeviceFacade, EncryptionFacade,
    EncryptionFacadeDeps, HostEventEmitterPort, InMemoryLifecycleStatus, LifecycleFacade,
    LifecycleFacadeDeps, LifecycleStatusGateway, ResourceFacade, ResourceFacadeDeps, SearchFacade,
    SearchFacadeDeps, SettingsFacade, StorageFacade, StorageFacadeDeps,
};
use uc_core::ports::SettingsPort;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DaemonBootstrapOwnershipSnapshot {
    pub replacement_attempt: u8,
    pub spawned_child_pid: Option<u32>,
    pub last_incompatible_reason: Option<String>,
}

#[derive(Clone, Default)]
pub struct DaemonBootstrapOwnershipState(Arc<RwLock<DaemonBootstrapOwnershipSnapshot>>);

impl DaemonBootstrapOwnershipState {
    pub fn snapshot(&self) -> DaemonBootstrapOwnershipSnapshot {
        match self.0.read() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => {
                tracing::error!(
                    "RwLock poisoned in DaemonBootstrapOwnershipState::snapshot, recovering from poisoned state"
                );
                poisoned.into_inner().clone()
            }
        }
    }

    pub fn record_spawned_child(&self, pid: Option<u32>) {
        match self.0.write() {
            Ok(mut guard) => {
                guard.spawned_child_pid = pid;
            }
            Err(poisoned) => {
                tracing::error!(
                    "RwLock poisoned in DaemonBootstrapOwnershipState::record_spawned_child, recovering from poisoned state"
                );
                let mut guard = poisoned.into_inner();
                guard.spawned_child_pid = pid;
            }
        }
    }

    pub fn clear_spawned_child(&self) {
        self.record_spawned_child(None);
    }

    pub fn record_replacement_attempt(&self, reason: String) {
        match self.0.write() {
            Ok(mut guard) => {
                guard.replacement_attempt = guard.replacement_attempt.saturating_add(1);
                guard.last_incompatible_reason = Some(reason);
            }
            Err(poisoned) => {
                tracing::error!(
                    "RwLock poisoned in DaemonBootstrapOwnershipState::record_replacement_attempt, recovering from poisoned state"
                );
                let mut guard = poisoned.into_inner();
                guard.replacement_attempt = guard.replacement_attempt.saturating_add(1);
                guard.last_incompatible_reason = Some(reason);
            }
        }
    }
}

/// Tauri 端的应用运行时句柄。
///
/// 持有 `Arc<AppFacade>` 作为唯一业务入口,加上几个进程级字段
/// (task registry、settings port、storage paths、emitter cell、device id)。
///
/// **不持有** `Arc<CoreRuntime>`。所有业务调用走 `runtime.app_facade()`。
pub struct AppRuntime {
    app_facade: Arc<AppFacade>,
    /// Tauri AppHandle for event emission (set after Tauri setup).
    app_handle: Arc<RwLock<Option<tauri::AppHandle>>>,
    task_registry: Arc<TaskRegistry>,
    settings_port: Arc<dyn SettingsPort>,
    storage_paths: AppPaths,
    /// Shared emitter cell —— bootstrap 期可 swap (例如从
    /// `LoggingHostEventEmitter` 切到 daemon API emitter)。
    event_emitter_cell: Arc<RwLock<Arc<dyn HostEventEmitterPort>>>,
    device_id: String,
}

impl AppRuntime {
    /// 默认 emitter (`LoggingHostEventEmitter`)。其它情况调用 `with_setup`。
    pub fn new(
        deps: AppDeps,
        storage_paths: AppPaths,
        clipboard_write_coordinator: Arc<uc_app::usecases::ClipboardWriteCoordinator>,
    ) -> Self {
        let event_emitter: Arc<dyn HostEventEmitterPort> =
            Arc::new(uc_bootstrap::LoggingHostEventEmitter);
        Self::with_setup(
            deps,
            storage_paths,
            event_emitter,
            clipboard_write_coordinator,
        )
    }

    /// 装配 `AppFacade` + 收集进程级零碎件,产出 `AppRuntime`。
    ///
    /// `clipboard_write_coordinator` 是必填参数 —— `ClipboardRestoreFacade`
    /// 需要它,所以装 facade 时必须传入。
    pub fn with_setup(
        deps: AppDeps,
        storage_paths: AppPaths,
        event_emitter: Arc<dyn HostEventEmitterPort>,
        clipboard_write_coordinator: Arc<uc_app::usecases::ClipboardWriteCoordinator>,
    ) -> Self {
        let device_id = deps.device.device_identity.current_device_id().to_string();
        let settings_port = deps.settings.clone();

        let lifecycle_status: Arc<dyn LifecycleStatusGateway> =
            Arc::new(InMemoryLifecycleStatus::new());
        let task_registry = Arc::new(TaskRegistry::new());

        // Clipboard integration mode is resolved from the UC_CLIPBOARD_MODE env var.
        // Defaults to Full (standalone GUI watches clipboard directly).
        // Set UC_CLIPBOARD_MODE=passive when a daemon is running and handling
        // clipboard capture + broadcast via DaemonWsBridge.
        let clipboard_integration_mode = uc_bootstrap::resolve_clipboard_integration_mode();

        let event_emitter_cell = Arc::new(RwLock::new(event_emitter));

        // Compose AppFacade — 与 uc-daemon::entrypoint 同款装配代码。
        // GUI 端不直接做 space setup / member roster / search coordinator,
        // 这三处传 None;其它 facade 全部从 deps 拼齐。
        let app_facade = Arc::new(AppFacade::new(AppFacadeParts {
            space_setup: None,
            member_roster: None,
            lifecycle: Arc::new(LifecycleFacade::new(LifecycleFacadeDeps {
                status: lifecycle_status,
            })),
            encryption: Arc::new(EncryptionFacade::new(EncryptionFacadeDeps {
                setup_status: deps.setup_status.clone(),
                space_access: deps.security.space_access.clone(),
            })),
            resource: Arc::new(ResourceFacade::new(ResourceFacadeDeps {
                representation_repo: deps.clipboard.representation_repo.clone(),
                thumbnail_repo: deps.storage.thumbnail_repo.clone(),
                blob_store: deps.storage.blob_store.clone(),
            })),
            clipboard_history: Arc::new(ClipboardHistoryFacade::new(ClipboardHistoryFacadeDeps {
                entry_repo: deps.clipboard.clipboard_entry_repo.clone(),
                selection_repo: deps.clipboard.selection_repo.clone(),
                representation_repo: deps.clipboard.representation_repo.clone(),
                event_writer: deps.clipboard.clipboard_event_repo.clone(),
                payload_resolver: deps.clipboard.payload_resolver.clone(),
                blob_store: deps.storage.blob_store.clone(),
                thumbnail_repo: deps.storage.thumbnail_repo.clone(),
                file_transfer_repo: deps.storage.file_transfer_repo.clone(),
                search_index: Some(deps.search.search_index.clone()),
                file_cache_dir: Some(storage_paths.cache_dir.clone()),
            })),
            clipboard_restore: Arc::new(ClipboardRestoreFacade::new(ClipboardRestoreFacadeDeps {
                entry_repo: deps.clipboard.clipboard_entry_repo.clone(),
                selection_repo: deps.clipboard.selection_repo.clone(),
                representation_repo: deps.clipboard.representation_repo.clone(),
                blob_store: deps.storage.blob_store.clone(),
                clock: deps.system.clock.clone(),
                write_coordinator: clipboard_write_coordinator,
                integration_mode: clipboard_integration_mode,
            })),
            search: Arc::new(SearchFacade::new(SearchFacadeDeps {
                search_index: deps.search.search_index.clone(),
                coordinator: None,
            })),
            settings: Arc::new(SettingsFacade::new(deps.settings.clone())),
            device: Arc::new(DeviceFacade::new(
                deps.device.device_identity.clone(),
                deps.settings.clone(),
            )),
            storage: Arc::new(StorageFacade::new(StorageFacadeDeps {
                db_path: storage_paths.db_path.clone(),
                vault_dir: storage_paths.vault_dir.clone(),
                cache_dir: storage_paths.cache_dir.clone(),
                logs_dir: storage_paths.logs_dir.clone(),
                app_data_root_dir: storage_paths.app_data_root_dir.clone(),
                cache_fs: deps.system.cache_fs.clone(),
            })),
        }));

        Self {
            app_facade,
            app_handle: Arc::new(RwLock::new(None)),
            task_registry,
            settings_port,
            storage_paths,
            event_emitter_cell,
            device_id,
        }
    }

    /// Set the Tauri AppHandle for event emission.
    /// This must be called after Tauri setup completes.
    pub fn set_app_handle(&self, handle: tauri::AppHandle) {
        match self.app_handle.write() {
            Ok(mut guard) => {
                *guard = Some(handle);
            }
            Err(poisoned) => {
                tracing::error!(
                    "RwLock poisoned in set_app_handle, recovering from poisoned state"
                );
                let mut guard = poisoned.into_inner();
                *guard = Some(handle);
            }
        }
    }

    /// Get a reference to the AppHandle, if available.
    pub fn app_handle(&self) -> std::sync::RwLockReadGuard<'_, Option<tauri::AppHandle>> {
        self.app_handle.read().unwrap_or_else(|poisoned| {
            tracing::error!("RwLock poisoned in app_handle, recovering from poisoned state");
            poisoned.into_inner()
        })
    }

    /// Returns a clone of the shared app_handle cell.
    pub fn app_handle_cell(&self) -> Arc<RwLock<Option<tauri::AppHandle>>> {
        self.app_handle.clone()
    }

    /// 业务入口 —— commands / 后台任务通过它访问业务。
    pub fn app_facade(&self) -> &Arc<AppFacade> {
        &self.app_facade
    }

    /// Get the current event emitter (clones the inner Arc).
    pub fn event_emitter(&self) -> Arc<dyn HostEventEmitterPort> {
        match self.event_emitter_cell.read() {
            Ok(guard) => Arc::clone(&*guard),
            Err(poisoned) => {
                tracing::error!(
                    "RwLock poisoned in AppRuntime::event_emitter, recovering from poisoned state"
                );
                Arc::clone(&*poisoned.into_inner())
            }
        }
    }

    /// Swap the event emitter. Called from daemon setup to replace the
    /// initial `LoggingHostEventEmitter` with a daemon API emitter.
    pub fn set_event_emitter(&self, emitter: Arc<dyn HostEventEmitterPort>) {
        match self.event_emitter_cell.write() {
            Ok(mut guard) => {
                *guard = emitter;
            }
            Err(poisoned) => {
                tracing::error!(
                    "RwLock poisoned in AppRuntime::set_event_emitter, recovering from poisoned state"
                );
                let mut guard = poisoned.into_inner();
                *guard = emitter;
            }
        }
    }

    /// Returns the current device ID for tracing spans and session context.
    pub fn device_id(&self) -> String {
        self.device_id.clone()
    }

    /// Returns a clone of the settings port for resolve_pairing_device_name and startup tasks.
    pub fn settings_port(&self) -> Arc<dyn SettingsPort> {
        self.settings_port.clone()
    }

    /// Returns the storage paths bundle (db / vault / cache / logs / app data root).
    pub fn storage_paths(&self) -> &AppPaths {
        &self.storage_paths
    }

    /// Returns a reference to the task registry for lifecycle management.
    pub fn task_registry(&self) -> &Arc<TaskRegistry> {
        &self.task_registry
    }
}
