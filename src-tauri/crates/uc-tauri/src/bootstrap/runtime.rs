//! # Use Cases Accessor
//!
//! This module provides the `UseCases` accessor which is attached to `AppRuntime`
//! to provide convenient access to all use cases with their dependencies pre-wired.
//!
//! ## Architecture
//!
//! - **uc-app/usecases**: Pure use cases with `new()` constructors taking ports
//! - **uc-tauri/bootstrap**: This module wires `Arc<dyn Port>` from AppDeps into use cases
//! - **Commands**: Call `runtime.usecases().xxx()` to get use case instances
//!
//! ## Usage
//!
//! ```rust,no_run
//! use uc_tauri::bootstrap::AppRuntime;
//! use tauri::State;
//!
//! #[tauri::command]
//! async fn my_command(runtime: State<'_, AppRuntime>) -> Result<(), String> {
//!     let uc = runtime.usecases().list_entry_projections();
//!     uc.execute(50, 0).await.map_err(|e| e.to_string())?;
//!     Ok(())
//! }
//! ```
//!
//! ## Adding New Use Cases
//!
//! 1. Ensure use case has a `new()` constructor taking its required ports
//! 2. Add a method to `UseCases` that calls `new()` with deps
//! 3. Commands can now call `runtime.usecases().your_use_case()`

use std::sync::{Arc, RwLock};

use uc_app::task_registry::TaskRegistry;
use uc_app::{runtime::CoreRuntime, App, AppDeps};
use uc_core::config::AppConfig;
use uc_core::ports::SettingsPort;

use uc_app::shared::host_event::HostEventEmitterPort;

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

/// Application runtime with dependencies.
///
/// This struct holds all application dependencies and provides
/// access to use cases through the `usecases()` method.
///
/// Approved access pattern for command modules:
/// - Use `runtime.usecases()` for business operations
/// - Use `runtime.device_id()`, `runtime.is_encryption_ready()`, and
///   `runtime.settings_port()` for simple read-only state access
/// - Direct `runtime.deps.*` access is not allowed in command modules
///
/// ## Architecture / 架构
///
/// The `AppRuntime` serves as the central point for accessing all application
/// dependencies and use cases. It wraps `AppDeps` and provides a `usecases()`
/// method that returns a `UseCases` accessor.
///
/// `AppRuntime` 是访问所有应用依赖和用例的中心点。它包装 `AppDeps` 并提供
/// 返回 `UseCases` 访问器的 `usecases()` 方法。
///
/// ## Usage Example / 使用示例
///
/// ```rust,no_run
/// use uc_tauri::bootstrap::AppRuntime;
/// use tauri::State;
///
/// #[tauri::command]
/// async fn get_entries(runtime: State<'_, AppRuntime>) -> Result<(), String> {
///     let uc = runtime.usecases().list_entry_projections();
///     let entries = uc.execute(50, 0).await.map_err(|e| e.to_string())?;
///     Ok(())
/// }
/// ```
///
/// 包含所有应用依赖的运行时。
///
/// 此结构体保存所有应用依赖，并通过 `usecases()` 方法提供用例访问。
pub struct AppRuntime {
    /// Tauri-free core runtime with all domain state.
    core: Arc<CoreRuntime>,
    /// Tauri AppHandle for event emission (optional, set after Tauri setup).
    /// Uses RwLock for interior mutability since Arc<AppRuntime> is shared.
    app_handle: Arc<std::sync::RwLock<Option<tauri::AppHandle>>>,
}

impl AppRuntime {
    /// Create a new AppRuntime from dependencies.
    /// 从依赖创建新的 AppRuntime。
    pub fn new(deps: AppDeps, storage_paths: uc_app::app_paths::AppPaths) -> Self {
        let event_emitter: Arc<dyn HostEventEmitterPort> =
            Arc::new(uc_bootstrap::LoggingHostEventEmitter);
        Self::with_setup(deps, storage_paths, event_emitter)
    }

    /// Construct an AppRuntime backed by an explicit emitter.
    ///
    /// Slice4 P3 T3.4 collapsed the previous `with_setup(deps, setup_ports,
    /// paths, emitter)` signature: the legacy `SetupFacade` and its
    /// `SetupAssemblyPorts` bundle are gone, so the GUI-side runtime now
    /// only needs an emitter to wire CoreRuntime.
    pub fn with_setup(
        deps: AppDeps,
        storage_paths: uc_app::app_paths::AppPaths,
        event_emitter: Arc<dyn HostEventEmitterPort>,
    ) -> Self {
        let lifecycle_status: Arc<dyn uc_app::usecases::LifecycleStatusPort> =
            Arc::new(uc_app::usecases::InMemoryLifecycleStatus::new());
        let app_handle = Arc::new(std::sync::RwLock::new(None));
        // Clipboard integration mode is resolved from the UC_CLIPBOARD_MODE env var.
        // Defaults to Full (standalone GUI watches clipboard directly).
        // Set UC_CLIPBOARD_MODE=passive when a daemon is running and handling
        // clipboard capture + broadcast via DaemonWsBridge.
        let clipboard_integration_mode = uc_bootstrap::resolve_clipboard_integration_mode();
        let task_registry = Arc::new(TaskRegistry::new());

        // Shared emitter cell for downstream consumers that may need
        // read-through after a future emitter swap.
        let emitter_cell = Arc::new(std::sync::RwLock::new(event_emitter));

        let core = Arc::new(CoreRuntime::new(
            deps,
            emitter_cell,
            lifecycle_status,
            clipboard_integration_mode,
            task_registry,
            storage_paths,
        ));

        Self { core, app_handle }
    }

    /// Wire a `ClipboardWriteCoordinator` into the inner `CoreRuntime`.
    ///
    /// Must be called BEFORE `Arc::new(runtime)` (i.e. while the runtime is still
    /// uniquely owned). GUI bootstrap calls this with
    /// `background.clipboard_write_coordinator.clone()` after `with_setup()`.
    pub fn with_clipboard_write_coordinator(
        mut self,
        coordinator: Arc<uc_app::usecases::ClipboardWriteCoordinator>,
    ) -> Self {
        // Arc::get_mut succeeds here because the caller has not yet shared the Arc.
        if let Some(core) = Arc::get_mut(&mut self.core) {
            core.set_clipboard_write_coordinator(coordinator);
        } else {
            tracing::warn!(
                "with_clipboard_write_coordinator called after Arc was shared — coordinator not set"
            );
        }
        self
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
    pub fn app_handle_cell(&self) -> Arc<std::sync::RwLock<Option<tauri::AppHandle>>> {
        self.app_handle.clone()
    }

    /// Get the current event emitter (clones the inner Arc).
    ///
    /// Returns the active [`HostEventEmitterPort`] implementation. During early bootstrap,
    /// this is a [`LoggingEventEmitter`]; after daemon setup, a `DaemonApiEventEmitter`.
    pub fn event_emitter(&self) -> Arc<dyn HostEventEmitterPort> {
        self.core.event_emitter()
    }

    /// Swap the event emitter. Called from daemon setup to replace the
    /// initial [`LoggingEventEmitter`] with a [`DaemonApiEventEmitter`].
    pub fn set_event_emitter(&self, emitter: Arc<dyn HostEventEmitterPort>) {
        self.core.set_event_emitter(emitter);
    }

    /// Returns a reference to the CoreRuntime for consumers that need it.
    pub fn core(&self) -> &Arc<CoreRuntime> {
        &self.core
    }

    /// Get use cases accessor.
    /// 获取用例访问器。
    pub fn usecases(&self) -> AppUseCases<'_> {
        AppUseCases::new(self)
    }

    /// Returns the current device ID for tracing spans and session context.
    /// For business operations involving device identity, use `self.usecases()`.
    pub fn device_id(&self) -> String {
        self.core.device_id()
    }

    /// Check if the encryption session is ready.
    pub async fn is_encryption_ready(&self) -> bool {
        self.core.is_encryption_ready().await
    }

    /// Phase C: unified truth source = `SetupStatus.has_completed`.
    /// Replaces prior `encryption_state()` helper backed by `EncryptionStatePort`.
    pub async fn has_completed_setup(&self) -> Result<bool, String> {
        self.core.has_completed_setup().await
    }

    /// Returns a clone of the settings port for resolve_pairing_device_name.
    /// This is a thin accessor; for settings business operations, use usecases().
    pub fn settings_port(&self) -> Arc<dyn SettingsPort> {
        self.core.settings_port()
    }

    /// Returns a reference to the underlying AppDeps for wiring/bootstrap code only.
    ///
    /// **IMPORTANT**: This method is intended exclusively for bootstrap wiring code
    /// (e.g., `start_background_tasks` in `main.rs`). Command handlers MUST NOT use
    /// this method — use `runtime.usecases()` or specific facade methods instead.
    pub fn wiring_deps(&self) -> &AppDeps {
        self.core.wiring_deps()
    }

    pub fn clipboard_integration_mode(&self) -> uc_core::clipboard::ClipboardIntegrationMode {
        self.core.clipboard_integration_mode()
    }

    /// Returns a reference to the task registry for lifecycle management.
    ///
    /// Used by bootstrap code to spawn tracked background tasks and by the
    /// app exit hook to trigger graceful shutdown.
    pub fn task_registry(&self) -> &Arc<TaskRegistry> {
        self.core.task_registry()
    }
}

/// Tauri-aware use case accessors wrapping CoreUseCases.
///
/// Provides transparent access to all CoreUseCases methods (via Deref) plus
/// 3 non-core accessors that cannot live in uc-app:
/// - apply_autostart (needs AppHandle)
/// - app_lifecycle_coordinator (needs LoggingSessionReadyEmitter)
/// - sync_outbound_clipboard (needs uc_infra TransferCipherAdapter)
pub struct AppUseCases<'a> {
    app_runtime: &'a AppRuntime,
    core: uc_app::usecases::CoreUseCases<'a>,
}

impl<'a> AppUseCases<'a> {
    pub fn new(app_runtime: &'a AppRuntime) -> Self {
        let core = uc_app::usecases::CoreUseCases::new(&app_runtime.core);
        Self { app_runtime, core }
    }

    /// Apply OS-level autostart setting.
    ///
    /// Requires AppHandle to be set (returns None during early bootstrap).
    pub fn apply_autostart(
        &self,
    ) -> Option<
        uc_platform::usecases::ApplyAutostartSetting<crate::adapters::autostart::TauriAutostart>,
    > {
        let guard = self.app_runtime.app_handle();
        let handle = guard.as_ref()?;
        let adapter = Arc::new(crate::adapters::autostart::TauriAutostart::new(
            handle.clone(),
        ));
        Some(uc_platform::usecases::ApplyAutostartSetting::new(adapter))
    }

    /// Get the AppLifecycleCoordinator use case for orchestrating
    /// network startup and session readiness.
    pub fn app_lifecycle_coordinator(&self) -> uc_app::usecases::AppLifecycleCoordinator {
        let announcer = Arc::new(uc_app::usecases::DeviceNameAnnouncer::new(
            self.app_runtime.wiring_deps().network_ports.peers.clone(),
            self.app_runtime.wiring_deps().settings.clone(),
        ));
        uc_app::usecases::AppLifecycleCoordinator::from_deps(
            uc_app::usecases::AppLifecycleCoordinatorDeps {
                network: Arc::new(self.core.start_network_after_unlock()),
                announcer: Some(announcer),
                emitter: Arc::new(
                    uc_app::usecases::app_lifecycle::adapters::LoggingSessionReadyEmitter,
                ),
                status: self.app_runtime.core.lifecycle_status().clone(),
                lifecycle_emitter: Arc::new(uc_app::usecases::LoggingLifecycleEventEmitter),
            },
        )
    }

    pub fn sync_outbound_clipboard(
        &self,
    ) -> uc_app::usecases::clipboard::sync_outbound::SyncOutboundClipboardUseCase {
        uc_app::usecases::clipboard::sync_outbound::SyncOutboundClipboardUseCase::new(
            self.app_runtime
                .wiring_deps()
                .clipboard
                .system_clipboard
                .clone(),
            self.app_runtime
                .wiring_deps()
                .network_ports
                .clipboard_outbound
                .clone(),
            self.app_runtime.wiring_deps().network_ports.peers.clone(),
            self.app_runtime.wiring_deps().security.space_access.clone(),
            self.app_runtime
                .wiring_deps()
                .device
                .device_identity
                .clone(),
            self.app_runtime.wiring_deps().settings.clone(),
            self.app_runtime
                .wiring_deps()
                .security
                .transfer_cipher
                .clone(),
            self.app_runtime.wiring_deps().device.member_repo.clone(),
        )
    }
}

impl<'a> std::ops::Deref for AppUseCases<'a> {
    type Target = uc_app::usecases::CoreUseCases<'a>;

    fn deref(&self) -> &Self::Target {
        &self.core
    }
}

/// Seed for creating the application runtime.
///
/// This is an assembly context that holds the AppConfig
/// before Tauri setup phase completes. It does NOT contain
/// a fully constructed runtime - that happens in the setup phase.
///
/// ## English
///
/// This struct serves as a bridge between:
/// - Phase 1: Configuration loading (pre-Tauri)
/// - Phase 2: Dependency wiring (Tauri setup)
/// - Phase 3: App construction (post-setup)
///
/// ## 中文
///
/// 此结构作为以下阶段之间的桥梁：
/// - 阶段 1：配置加载（Tauri 之前）
/// - 阶段 2：依赖连接（Tauri 设置）
/// - 阶段 3：应用构造（设置之后）
pub struct AppRuntimeSeed {
    /// Application configuration loaded from TOML
    /// 从 TOML 加载的应用配置
    pub config: AppConfig,
}

/// Create the runtime seed without touching Tauri.
///
/// This function must not depend on Tauri or any UI framework.
/// 不依赖 Tauri 或任何 UI 框架创建运行时种子。
///
/// ## Phase Integration / 阶段集成
///
/// - **Phase 1**: Call this after `load_config()` to create the seed
/// - **Phase 2**: Pass seed to `wire_dependencies()` in Tauri setup
/// - **Phase 3**: Call `create_app()` with wired dependencies
///
/// ## English
///
/// This is the entry point for the bootstrap sequence:
/// 1. `load_config()` → reads TOML into `AppConfig`
/// 2. `create_runtime()` → wraps config in `AppRuntimeSeed`
/// 3. `wire_dependencies()` → creates ports from config
/// 4. `create_app()` → constructs `App` from dependencies
pub fn create_runtime(config: AppConfig) -> anyhow::Result<AppRuntimeSeed> {
    Ok(AppRuntimeSeed { config })
}

/// Create the App instance from wired dependencies.
/// 从已连接的依赖创建 App 实例。
///
/// ## English
///
/// This function is called in Phase 3 (after Tauri setup completes)
/// to construct the final `App` instance from the dependencies
/// that were wired in Phase 2.
///
/// This is a direct construction function - NOT a builder pattern.
/// All dependencies must be provided; no defaults, no optionals.
///
/// ## 中文
///
/// 此函数在阶段 3（Tauri 设置完成后）调用，
/// 用于从阶段 2 中连接的依赖构造最终的 `App` 实例。
///
/// 这是一个直接构造函数 - 不是 Builder 模式。
/// 必须提供所有依赖；无默认值，无可选项。
///
/// # Parameters / 参数
///
/// - `deps`: Application dependencies wired from configuration
///           从配置连接的应用依赖
///
/// # Returns / 返回
///
/// - `App`: Fully constructed application runtime
///          完全构造的应用运行时
///
/// # Phase 3 Integration / 阶段 3 集成
///
/// This function completes the bootstrap sequence:
/// ```text
/// load_config() → create_runtime() → wire_dependencies() → create_app()
///     ↓                 ↓                    ↓                    ↓
///   AppConfig      AppRuntimeSeed        AppDeps               App
/// ```
pub fn create_app(deps: AppDeps) -> App {
    App::new(deps)
}
