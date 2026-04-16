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
//!     let uc = runtime.usecases().list_clipboard_entries();
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
use uc_core::security::state::EncryptionState;

use uc_core::ports::host_event_emitter::HostEventEmitterPort;

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
///     let uc = runtime.usecases().list_clipboard_entries();
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
        let setup_ports = uc_bootstrap::assembly::SetupAssemblyPorts::placeholder(&deps);
        let event_emitter: Arc<dyn HostEventEmitterPort> =
            Arc::new(uc_bootstrap::LoggingHostEventEmitter);
        Self::with_setup(deps, setup_ports, storage_paths, event_emitter)
    }

    /// Create a new AppRuntime with explicit setup orchestrator dependencies.
    pub fn with_setup(
        deps: AppDeps,
        setup_ports: uc_bootstrap::assembly::SetupAssemblyPorts,
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

        // Create the shared emitter cell BEFORE both consumers.
        // This cell is shared between CoreRuntime and build_setup_orchestrator
        // so that HostEventSetupPort reads the current emitter after swap.
        let emitter_cell = Arc::new(std::sync::RwLock::new(event_emitter));

        // Build session_ready_emitter — emits "Session ready" log, no frontend event.
        let session_ready_emitter: Arc<dyn uc_app::usecases::SessionReadyEmitter> =
            Arc::new(uc_app::usecases::app_lifecycle::adapters::LoggingSessionReadyEmitter);

        // Pass shared state + adapters to build_setup_orchestrator as SEPARATE params.
        let setup_orchestrator = uc_bootstrap::assembly::build_setup_orchestrator(
            &deps,
            setup_ports,
            lifecycle_status.clone(), // same instance goes to CoreRuntime below
            emitter_cell.clone(),     // same instance goes to CoreRuntime below
            session_ready_emitter,    // constructed from app_handle above
        );

        // Pass the SAME cell to CoreRuntime — no re-wrapping.
        let core = Arc::new(CoreRuntime::new(
            deps,
            emitter_cell,
            lifecycle_status,
            setup_orchestrator,
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

    /// Returns the persisted encryption state used by readiness checks.
    pub async fn encryption_state(&self) -> Result<EncryptionState, String> {
        self.core.encryption_state().await
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
/// - sync_outbound_clipboard (needs uc_infra TransferPayloadEncryptorAdapter)
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
                .clipboard
                .clone(),
            self.app_runtime.wiring_deps().network_ports.peers.clone(),
            self.app_runtime
                .wiring_deps()
                .security
                .encryption_session
                .clone(),
            self.app_runtime
                .wiring_deps()
                .device
                .device_identity
                .clone(),
            self.app_runtime.wiring_deps().settings.clone(),
            Arc::new(uc_infra::clipboard::TransferPayloadEncryptorAdapter),
            self.app_runtime
                .wiring_deps()
                .device
                .paired_device_repo
                .clone(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::noop_network_ports;
    use async_trait::async_trait;
    use mockall::mock;
    use std::sync::{Arc, Mutex};
    use tokio::sync::mpsc;
    use uc_core::clipboard::PolicyError;
    use uc_core::ports::clipboard::{RepresentationCachePort, SpoolQueuePort, SpoolRequest};
    use uc_core::ports::host_event_emitter::{ClipboardHostEvent, HostEvent};
    use uc_core::ports::security::encryption_state::EncryptionStatePort;
    use uc_core::ports::security::key_scope::KeyScopePort;
    use uc_core::ports::*;
    use uc_core::security::model::{
        EncryptedBlob, EncryptionAlgo, EncryptionError, KdfParams, Kek, KeyScope, KeySlot,
        MasterKey, Passphrase,
    };
    use uc_core::security::state::{EncryptionState, EncryptionStateError};
    use uc_core::PeerId;
    use uc_core::{
        Blob, BlobId, ContentHash, DeviceId, PersistedClipboardRepresentation,
        SystemClipboardSnapshot,
    };
    use uc_infra::clipboard::new_in_memory_change_origin;

    fn test_origin() -> std::sync::Arc<dyn uc_core::ports::clipboard::ClipboardChangeOriginPort> {
        new_in_memory_change_origin()
    }

    mock! {
        EntryRepository {}

        #[async_trait]
        impl ClipboardEntryRepositoryPort for EntryRepository {
            async fn save_entry_and_selection(
                &self,
                entry: &uc_core::ClipboardEntry,
                selection: &uc_core::ClipboardSelectionDecision,
            ) -> anyhow::Result<()>;
            async fn get_entry(
                &self,
                entry_id: &uc_core::ids::EntryId,
            ) -> anyhow::Result<Option<uc_core::ClipboardEntry>>;
            async fn list_entries(
                &self,
                limit: usize,
                offset: usize,
            ) -> anyhow::Result<Vec<uc_core::ClipboardEntry>>;
            async fn touch_entry(
                &self,
                entry_id: &uc_core::ids::EntryId,
                active_time_ms: i64,
            ) -> anyhow::Result<bool>;
            async fn delete_entry(&self, entry_id: &uc_core::ids::EntryId) -> anyhow::Result<()>;
        }
    }

    mock! {
        EventWriter {}

        #[async_trait]
        impl ClipboardEventWriterPort for EventWriter {
            async fn insert_event(
                &self,
                event: &uc_core::ClipboardEvent,
                representations: &Vec<uc_core::PersistedClipboardRepresentation>,
            ) -> anyhow::Result<()>;
            async fn delete_event_and_representations(
                &self,
                event_id: &uc_core::ids::EventId,
            ) -> anyhow::Result<()>;
        }
    }

    mock! {
        RepresentationPolicy {}

        impl SelectRepresentationPolicyPort for RepresentationPolicy {
            fn select(
                &self,
                snapshot: &SystemClipboardSnapshot,
            ) -> std::result::Result<uc_core::clipboard::ClipboardSelection, PolicyError>;
        }
    }

    mock! {
        Normalizer {}

        #[async_trait]
        impl ClipboardRepresentationNormalizerPort for Normalizer {
            async fn normalize(
                &self,
                observed: &uc_core::clipboard::ObservedClipboardRepresentation,
            ) -> anyhow::Result<uc_core::PersistedClipboardRepresentation>;
        }
    }

    mock! {
        RepresentationCache {}

        #[async_trait]
        impl RepresentationCachePort for RepresentationCache {
            async fn put(&self, rep_id: &uc_core::ids::RepresentationId, bytes: Vec<u8>);
            async fn get(&self, rep_id: &uc_core::ids::RepresentationId) -> Option<Vec<u8>>;
            async fn mark_completed(&self, rep_id: &uc_core::ids::RepresentationId);
            async fn mark_spooling(&self, rep_id: &uc_core::ids::RepresentationId);
            async fn remove(&self, rep_id: &uc_core::ids::RepresentationId);
        }
    }

    mock! {
        SpoolQueue {}

        #[async_trait]
        impl SpoolQueuePort for SpoolQueue {
            async fn enqueue(&self, request: SpoolRequest) -> anyhow::Result<()>;
        }
    }

    mock! {
        HostEventEmitter {}

        impl HostEventEmitterPort for HostEventEmitter {
            fn emit(
                &self,
                event: HostEvent,
            ) -> Result<(), uc_core::ports::host_event_emitter::EmitError>;
        }
    }

    mock! {
        DeviceIdentity {}

        impl DeviceIdentityPort for DeviceIdentity {
            fn current_device_id(&self) -> DeviceId;
        }
    }

    mock! {
        SystemClipboard {}

        #[async_trait]
        impl SystemClipboardPort for SystemClipboard {
            fn read_snapshot(&self) -> anyhow::Result<SystemClipboardSnapshot>;
            fn write_snapshot(&self, snapshot: SystemClipboardSnapshot) -> anyhow::Result<()>;
        }
    }

    mock! {
        SelectionRepo {}

        #[async_trait]
        impl ClipboardSelectionRepositoryPort for SelectionRepo {
            async fn get_selection(
                &self,
                entry_id: &uc_core::ids::EntryId,
            ) -> anyhow::Result<Option<uc_core::ClipboardSelectionDecision>>;
            async fn delete_selection(&self, entry_id: &uc_core::ids::EntryId) -> anyhow::Result<()>;
        }
    }

    mock! {
        RepresentationRepo {}

        #[async_trait]
        impl ClipboardRepresentationRepositoryPort for RepresentationRepo {
            async fn get_representation(
                &self,
                event_id: &uc_core::ids::EventId,
                representation_id: &uc_core::ids::RepresentationId,
            ) -> anyhow::Result<Option<uc_core::PersistedClipboardRepresentation>>;
            async fn get_representation_by_id(
                &self,
                representation_id: &uc_core::ids::RepresentationId,
            ) -> anyhow::Result<Option<uc_core::PersistedClipboardRepresentation>>;
            async fn get_representation_by_blob_id(
                &self,
                blob_id: &BlobId,
            ) -> anyhow::Result<Option<uc_core::PersistedClipboardRepresentation>>;
            async fn update_blob_id(
                &self,
                representation_id: &uc_core::ids::RepresentationId,
                blob_id: &BlobId,
            ) -> anyhow::Result<()>;
            async fn update_blob_id_if_none(
                &self,
                representation_id: &uc_core::ids::RepresentationId,
                blob_id: &BlobId,
            ) -> anyhow::Result<bool>;
            #[mockall::concretize]
            async fn update_processing_result(
                &self,
                rep_id: &uc_core::ids::RepresentationId,
                expected_states: &[uc_core::clipboard::PayloadAvailability],
                blob_id: Option<&BlobId>,
                new_state: uc_core::clipboard::PayloadAvailability,
                last_error: Option<&str>,
            ) -> anyhow::Result<uc_core::ports::clipboard::ProcessingUpdateOutcome>;
        }
    }

    mock! {
        PayloadResolver {}

        #[async_trait]
        impl ClipboardPayloadResolverPort for PayloadResolver {
            async fn resolve(
                &self,
                representation: &PersistedClipboardRepresentation,
            ) -> anyhow::Result<ResolvedClipboardPayload>;
        }
    }

    mock! {
        Encryption {}

        #[async_trait]
        impl EncryptionPort for Encryption {
            async fn derive_kek(
                &self,
                passphrase: &Passphrase,
                salt: &[u8],
                kdf: &KdfParams,
            ) -> Result<Kek, EncryptionError>;
            async fn wrap_master_key(
                &self,
                kek: &Kek,
                master_key: &MasterKey,
                aead: EncryptionAlgo,
            ) -> Result<EncryptedBlob, EncryptionError>;
            async fn unwrap_master_key(
                &self,
                kek: &Kek,
                wrapped: &EncryptedBlob,
            ) -> Result<MasterKey, EncryptionError>;
            async fn encrypt_blob(
                &self,
                master_key: &MasterKey,
                plaintext: &[u8],
                aad: &[u8],
                aead: EncryptionAlgo,
            ) -> Result<EncryptedBlob, EncryptionError>;
            async fn decrypt_blob(
                &self,
                master_key: &MasterKey,
                encrypted: &EncryptedBlob,
                aad: &[u8],
            ) -> Result<Vec<u8>, EncryptionError>;
        }
    }

    mock! {
        EncryptionSession {}

        #[async_trait]
        impl EncryptionSessionPort for EncryptionSession {
            async fn is_ready(&self) -> bool;
            async fn get_master_key(&self) -> Result<MasterKey, EncryptionError>;
            async fn set_master_key(&self, master_key: MasterKey) -> Result<(), EncryptionError>;
            async fn clear(&self) -> Result<(), EncryptionError>;
        }
    }

    mock! {
        EncryptionState {}

        #[async_trait]
        impl EncryptionStatePort for EncryptionState {
            async fn load_state(&self) -> Result<EncryptionState, EncryptionStateError>;
            async fn persist_initialized(&self) -> Result<(), EncryptionStateError>;
            async fn clear_initialized(&self) -> Result<(), EncryptionStateError>;
        }
    }

    mock! {
        KeyScope {}

        #[async_trait]
        impl KeyScopePort for KeyScope {
            async fn current_scope(
                &self,
            ) -> Result<KeyScope, uc_core::ports::security::key_scope::ScopeError>;
        }
    }

    mock! {
        KeyMaterial {}

        #[async_trait]
        impl KeyMaterialPort for KeyMaterial {
            async fn load_kek(&self, scope: &KeyScope) -> Result<Kek, EncryptionError>;
            async fn store_kek(&self, scope: &KeyScope, kek: &Kek) -> Result<(), EncryptionError>;
            async fn delete_kek(&self, scope: &KeyScope) -> Result<(), EncryptionError>;
            async fn load_keyslot(&self, scope: &KeyScope) -> Result<KeySlot, EncryptionError>;
            async fn store_keyslot(&self, keyslot: &KeySlot) -> Result<(), EncryptionError>;
            async fn delete_keyslot(&self, scope: &KeyScope) -> Result<(), EncryptionError>;
        }
    }

    mock! {
        DeviceRepo {}

        #[async_trait]
        impl DeviceRepositoryPort for DeviceRepo {
            async fn find_by_id(
                &self,
                id: &uc_core::device::DeviceId,
            ) -> Result<Option<uc_core::device::Device>, uc_core::ports::errors::DeviceRepositoryError>;
            async fn save(
                &self,
                device: uc_core::device::Device,
            ) -> Result<(), uc_core::ports::errors::DeviceRepositoryError>;
            async fn delete(
                &self,
                id: &uc_core::device::DeviceId,
            ) -> Result<(), uc_core::ports::errors::DeviceRepositoryError>;
            async fn list_all(
                &self,
            ) -> Result<Vec<uc_core::device::Device>, uc_core::ports::errors::DeviceRepositoryError>;
        }
    }

    mock! {
        NetworkControl {}

        #[async_trait]
        impl uc_core::ports::NetworkControlPort for NetworkControl {
            async fn start_network(&self) -> anyhow::Result<()>;
        }
    }

    mock! {
        SetupStatus {}

        #[async_trait]
        impl SetupStatusPort for SetupStatus {
            async fn get_status(&self) -> anyhow::Result<uc_core::setup::SetupStatus>;
            async fn set_status(&self, status: &uc_core::setup::SetupStatus) -> anyhow::Result<()>;
        }
    }

    mock! {
        SecureStorage {}

        impl uc_core::ports::SecureStoragePort for SecureStorage {
            fn get(&self, key: &str) -> Result<Option<Vec<u8>>, uc_core::ports::SecureStorageError>;
            fn set(&self, key: &str, value: &[u8]) -> Result<(), uc_core::ports::SecureStorageError>;
            fn delete(&self, key: &str) -> Result<(), uc_core::ports::SecureStorageError>;
        }
    }

    mock! {
        BlobStore {}

        #[async_trait]
        impl BlobStorePort for BlobStore {
            async fn put(
                &self,
                blob_id: &BlobId,
                data: &[u8],
            ) -> anyhow::Result<(std::path::PathBuf, Option<i64>)>;
            async fn get(&self, blob_id: &BlobId) -> anyhow::Result<Vec<u8>>;
        }
    }

    mock! {
        BlobRepo {}

        #[async_trait]
        impl BlobRepositoryPort for BlobRepo {
            async fn insert_blob(&self, blob: &Blob) -> anyhow::Result<()>;
            async fn find_by_hash(&self, content_hash: &ContentHash) -> anyhow::Result<Option<Blob>>;
        }
    }

    mock! {
        BlobWriter {}

        #[async_trait]
        impl BlobWriterPort for BlobWriter {
            async fn write_if_absent(
                &self,
                content_id: &ContentHash,
                plaintext_bytes: &[u8],
            ) -> anyhow::Result<Blob>;
        }
    }

    mock! {
        ThumbnailRepo {}

        #[async_trait]
        impl ThumbnailRepositoryPort for ThumbnailRepo {
            async fn get_by_representation_id(
                &self,
                representation_id: &uc_core::ids::RepresentationId,
            ) -> anyhow::Result<Option<uc_core::clipboard::ThumbnailMetadata>>;
            async fn insert_thumbnail(
                &self,
                metadata: &uc_core::clipboard::ThumbnailMetadata,
            ) -> anyhow::Result<()>;
        }
    }

    mock! {
        ThumbnailGenerator {}

        #[async_trait]
        impl ThumbnailGeneratorPort for ThumbnailGenerator {
            async fn generate_thumbnail(
                &self,
                image_bytes: &[u8],
            ) -> anyhow::Result<uc_core::ports::clipboard::GeneratedThumbnail>;
            async fn generate_thumbnail_from_rgba(
                &self,
                rgba_bytes: &[u8],
                width: u32,
                height: u32,
            ) -> anyhow::Result<uc_core::ports::clipboard::GeneratedThumbnail>;
        }
    }

    mock! {
        Settings {}

        #[async_trait]
        impl SettingsPort for Settings {
            async fn load(&self) -> anyhow::Result<uc_core::settings::model::Settings>;
            async fn save(&self, settings: &uc_core::settings::model::Settings) -> anyhow::Result<()>;
        }
    }

    mock! {
        PairedDeviceRepo {}

        #[async_trait]
        impl PairedDeviceRepositoryPort for PairedDeviceRepo {
            async fn get_by_peer_id(
                &self,
                peer_id: &PeerId,
            ) -> Result<Option<uc_core::network::PairedDevice>, PairedDeviceRepositoryError>;
            async fn list_all(
                &self,
            ) -> Result<Vec<uc_core::network::PairedDevice>, PairedDeviceRepositoryError>;
            async fn upsert(
                &self,
                device: uc_core::network::PairedDevice,
            ) -> Result<(), PairedDeviceRepositoryError>;
            async fn set_state(
                &self,
                peer_id: &PeerId,
                state: uc_core::network::PairingState,
            ) -> Result<(), PairedDeviceRepositoryError>;
            async fn update_last_seen(
                &self,
                peer_id: &PeerId,
                last_seen_at: chrono::DateTime<chrono::Utc>,
            ) -> Result<(), PairedDeviceRepositoryError>;
            async fn delete(&self, peer_id: &PeerId) -> Result<(), PairedDeviceRepositoryError>;
            async fn update_sync_settings(
                &self,
                peer_id: &PeerId,
                settings: Option<uc_core::settings::model::SyncSettings>,
            ) -> Result<(), PairedDeviceRepositoryError>;
        }
    }

    mock! {
        Clock {}

        impl ClockPort for Clock {
            fn now_ms(&self) -> i64;
        }
    }

    mock! {
        ContentHash {}

        impl ContentHashPort for ContentHash {
            fn hash_bytes(&self, bytes: &[u8]) -> anyhow::Result<ContentHash>;
        }
    }

    mock! {
        FileManager {}

        impl uc_core::ports::FileManagerPort for FileManager {
            fn open_directory(
                &self,
                path: &std::path::Path,
            ) -> Result<(), uc_core::ports::FileManagerError>;
        }
    }

    mock! {
        CacheFs {}

        #[async_trait]
        impl uc_core::ports::CacheFsPort for CacheFs {
            async fn exists(&self, path: &std::path::Path) -> bool;
            async fn read_dir(
                &self,
                path: &std::path::Path,
            ) -> anyhow::Result<Vec<uc_core::ports::CacheFsDirEntry>>;
            async fn remove_dir_all(&self, path: &std::path::Path) -> anyhow::Result<()>;
            async fn remove_file(&self, path: &std::path::Path) -> anyhow::Result<()>;
            async fn dir_size(&self, path: &std::path::Path) -> anyhow::Result<u64>;
        }
    }

    mock! {
        SearchIndex {}

        #[async_trait]
        impl uc_core::ports::search::search_index::SearchIndexPort for SearchIndex {
            async fn index_entry(
                &self,
                d: uc_core::search::SearchDocument,
                p: Vec<uc_core::search::SearchPosting>,
            ) -> Result<(), uc_core::search::SearchError>;
            async fn remove_entry(
                &self,
                id: &uc_core::ids::EntryId,
            ) -> Result<(), uc_core::search::SearchError>;
            async fn search(
                &self,
                q: uc_core::search::SearchQuery,
            ) -> Result<uc_core::search::SearchResultsPage, uc_core::search::SearchError>;
            async fn rebuild(
                &self,
                e: Vec<(
                    uc_core::search::SearchDocument,
                    Vec<uc_core::search::SearchPosting>,
                )>,
                tx: tokio::sync::mpsc::Sender<uc_core::search::RebuildProgress>,
            ) -> Result<(), uc_core::search::SearchError>;
            async fn get_index_meta(
                &self,
            ) -> Result<uc_core::search::SearchIndexMeta, uc_core::search::SearchError>;
        }
    }

    mock! {
        SearchKeyDerivation {}

        #[async_trait]
        impl uc_core::ports::search::search_key::SearchKeyDerivationPort for SearchKeyDerivation {
            async fn derive_search_key(
                &self,
            ) -> Result<uc_core::search::SearchKey, uc_core::search::SearchError>;
        }
    }

    fn build_entry_repository_mock() -> MockEntryRepository {
        let mut mock = MockEntryRepository::new();
        mock.expect_save_entry_and_selection()
            .times(0..)
            .returning(|_, _| Ok(()));
        mock.expect_get_entry().times(0..).returning(|_| Ok(None));
        mock.expect_list_entries()
            .times(0..)
            .returning(|_, _| Ok(vec![]));
        mock.expect_touch_entry()
            .times(0..)
            .returning(|_, _| Ok(false));
        mock.expect_delete_entry().times(0..).returning(|_| Ok(()));
        mock
    }

    fn build_event_writer_mock() -> MockEventWriter {
        let mut mock = MockEventWriter::new();
        mock.expect_insert_event()
            .times(0..)
            .returning(|_, _| Ok(()));
        mock.expect_delete_event_and_representations()
            .times(0..)
            .returning(|_| Ok(()));
        mock
    }

    fn build_representation_policy_mock() -> MockRepresentationPolicy {
        let mut mock = MockRepresentationPolicy::new();
        mock.expect_select()
            .times(0..)
            .returning(|_| Err(PolicyError::NoUsableRepresentation));
        mock
    }

    fn build_normalizer_mock() -> MockNormalizer {
        let mut mock = MockNormalizer::new();
        mock.expect_normalize()
            .times(0..)
            .returning(|_| Err(anyhow::anyhow!("normalize should not be called")));
        mock
    }

    fn build_representation_cache_mock() -> MockRepresentationCache {
        let mut mock = MockRepresentationCache::new();
        mock.expect_put().times(0..).returning(|_, _| ());
        mock.expect_get().times(0..).returning(|_| None);
        mock.expect_mark_completed().times(0..).returning(|_| ());
        mock.expect_mark_spooling().times(0..).returning(|_| ());
        mock.expect_remove().times(0..).returning(|_| ());
        mock
    }

    fn build_spool_queue_mock() -> MockSpoolQueue {
        let mut mock = MockSpoolQueue::new();
        mock.expect_enqueue().times(0..).returning(|_| Ok(()));
        mock
    }

    fn build_device_identity_mock() -> MockDeviceIdentity {
        let mut mock = MockDeviceIdentity::new();
        mock.expect_current_device_id()
            .times(0..)
            .returning(|| DeviceId::new("device-test"));
        mock
    }

    fn build_recording_emitter_mock(events: Arc<Mutex<Vec<HostEvent>>>) -> MockHostEventEmitter {
        let mut mock = MockHostEventEmitter::new();
        mock.expect_emit().times(0..).returning(move |event| {
            events.lock().unwrap().push(event);
            Ok(())
        });
        mock
    }

    fn build_system_clipboard_mock() -> MockSystemClipboard {
        let mut mock = MockSystemClipboard::new();
        mock.expect_read_snapshot().times(0..).returning(|| {
            Ok(SystemClipboardSnapshot {
                ts_ms: 0,
                representations: vec![],
            })
        });
        mock.expect_write_snapshot()
            .times(0..)
            .returning(|_| Ok(()));
        mock
    }

    fn build_selection_repo_mock() -> MockSelectionRepo {
        let mut mock = MockSelectionRepo::new();
        mock.expect_get_selection()
            .times(0..)
            .returning(|_| Ok(None));
        mock.expect_delete_selection()
            .times(0..)
            .returning(|_| Ok(()));
        mock
    }

    fn build_representation_repo_mock() -> MockRepresentationRepo {
        let mut mock = MockRepresentationRepo::new();
        mock.expect_get_representation()
            .times(0..)
            .returning(|_, _| Ok(None));
        mock.expect_get_representation_by_id()
            .times(0..)
            .returning(|_| Ok(None));
        mock.expect_get_representation_by_blob_id()
            .times(0..)
            .returning(|_| Ok(None));
        mock.expect_update_blob_id()
            .times(0..)
            .returning(|_, _| Ok(()));
        mock.expect_update_blob_id_if_none()
            .times(0..)
            .returning(|_, _| Ok(false));
        mock.expect_update_processing_result()
            .times(0..)
            .returning(|_, _, _, _, _| {
                Ok(uc_core::ports::clipboard::ProcessingUpdateOutcome::NotFound)
            });
        mock
    }

    fn build_payload_resolver_mock() -> MockPayloadResolver {
        let mut mock = MockPayloadResolver::new();
        mock.expect_resolve()
            .times(0..)
            .returning(|_| Err(anyhow::anyhow!("noop payload resolver")));
        mock
    }

    fn build_encryption_mock() -> MockEncryption {
        let mut mock = MockEncryption::new();
        mock.expect_derive_kek()
            .times(0..)
            .returning(|_, _, _| Err(EncryptionError::NotInitialized));
        mock.expect_wrap_master_key()
            .times(0..)
            .returning(|_, _, _| Err(EncryptionError::NotInitialized));
        mock.expect_unwrap_master_key()
            .times(0..)
            .returning(|_, _| Err(EncryptionError::NotInitialized));
        mock.expect_encrypt_blob()
            .times(0..)
            .returning(|_, _, _, _| Err(EncryptionError::NotInitialized));
        mock.expect_decrypt_blob()
            .times(0..)
            .returning(|_, _, _| Err(EncryptionError::NotInitialized));
        mock
    }

    fn build_encryption_session_mock() -> MockEncryptionSession {
        let mut mock = MockEncryptionSession::new();
        mock.expect_is_ready().times(0..).returning(|| false);
        mock.expect_get_master_key()
            .times(0..)
            .returning(|| Err(EncryptionError::NotInitialized));
        mock.expect_set_master_key()
            .times(0..)
            .returning(|_| Ok(()));
        mock.expect_clear().times(0..).returning(|| Ok(()));
        mock
    }

    fn build_encryption_state_mock() -> MockEncryptionState {
        let mut mock = MockEncryptionState::new();
        mock.expect_load_state()
            .times(0..)
            .returning(|| Err(EncryptionStateError::LoadError("noop".to_string())));
        mock.expect_persist_initialized()
            .times(0..)
            .returning(|| Ok(()));
        mock.expect_clear_initialized()
            .times(0..)
            .returning(|| Ok(()));
        mock
    }

    fn build_key_scope_mock() -> MockKeyScope {
        let mut mock = MockKeyScope::new();
        mock.expect_current_scope().times(0..).returning(|| {
            Err(uc_core::ports::security::key_scope::ScopeError::FailedToGetCurrentScope)
        });
        mock
    }

    fn build_key_material_mock() -> MockKeyMaterial {
        let mut mock = MockKeyMaterial::new();
        mock.expect_load_kek()
            .times(0..)
            .returning(|_| Err(EncryptionError::KeyNotFound));
        mock.expect_store_kek().times(0..).returning(|_, _| Ok(()));
        mock.expect_delete_kek().times(0..).returning(|_| Ok(()));
        mock.expect_load_keyslot()
            .times(0..)
            .returning(|_| Err(EncryptionError::KeyNotFound));
        mock.expect_store_keyslot().times(0..).returning(|_| Ok(()));
        mock.expect_delete_keyslot()
            .times(0..)
            .returning(|_| Ok(()));
        mock
    }

    fn build_device_repo_mock() -> MockDeviceRepo {
        let mut mock = MockDeviceRepo::new();
        mock.expect_find_by_id().times(0..).returning(|_| Ok(None));
        mock.expect_save().times(0..).returning(|_| Ok(()));
        mock.expect_delete().times(0..).returning(|_| Ok(()));
        mock.expect_list_all().times(0..).returning(|| Ok(vec![]));
        mock
    }

    fn build_network_control_mock() -> MockNetworkControl {
        let mut mock = MockNetworkControl::new();
        mock.expect_start_network().times(0..).returning(|| Ok(()));
        mock
    }

    fn build_setup_status_mock() -> MockSetupStatus {
        let mut mock = MockSetupStatus::new();
        mock.expect_get_status()
            .times(0..)
            .returning(|| Ok(uc_core::setup::SetupStatus::default()));
        mock.expect_set_status().times(0..).returning(|_| Ok(()));
        mock
    }

    fn build_secure_storage_mock() -> MockSecureStorage {
        let mut mock = MockSecureStorage::new();
        mock.expect_get().times(0..).returning(|_| Ok(None));
        mock.expect_set().times(0..).returning(|_, _| Ok(()));
        mock.expect_delete().times(0..).returning(|_| Ok(()));
        mock
    }

    fn build_blob_store_mock() -> MockBlobStore {
        let mut mock = MockBlobStore::new();
        mock.expect_put().times(0..).returning(|_, data| {
            Ok((
                std::path::PathBuf::from("/tmp/noop"),
                i64::try_from(data.len()).ok(),
            ))
        });
        mock.expect_get().times(0..).returning(|_| Ok(vec![]));
        mock
    }

    fn build_blob_repo_mock() -> MockBlobRepo {
        let mut mock = MockBlobRepo::new();
        mock.expect_insert_blob().times(0..).returning(|_| Ok(()));
        mock.expect_find_by_hash()
            .times(0..)
            .returning(|_| Ok(None));
        mock
    }

    fn build_blob_writer_mock() -> MockBlobWriter {
        let mut mock = MockBlobWriter::new();
        mock.expect_write_if_absent()
            .times(0..)
            .returning(|_, _| Err(anyhow::anyhow!("noop blob writer")));
        mock
    }

    fn build_thumbnail_repo_mock() -> MockThumbnailRepo {
        let mut mock = MockThumbnailRepo::new();
        mock.expect_get_by_representation_id()
            .times(0..)
            .returning(|_| Ok(None));
        mock.expect_insert_thumbnail()
            .times(0..)
            .returning(|_| Ok(()));
        mock
    }

    fn build_thumbnail_generator_mock() -> MockThumbnailGenerator {
        let mut mock = MockThumbnailGenerator::new();
        mock.expect_generate_thumbnail()
            .times(0..)
            .returning(|_| Err(anyhow::anyhow!("noop thumbnail generator")));
        mock.expect_generate_thumbnail_from_rgba()
            .times(0..)
            .returning(|_, _, _| Err(anyhow::anyhow!("noop thumbnail generator")));
        mock
    }

    fn build_settings_mock() -> MockSettings {
        let mut mock = MockSettings::new();
        mock.expect_load()
            .times(0..)
            .returning(|| Err(anyhow::anyhow!("noop settings")));
        mock.expect_save().times(0..).returning(|_| Ok(()));
        mock
    }

    fn build_paired_device_repo_mock() -> MockPairedDeviceRepo {
        let mut mock = MockPairedDeviceRepo::new();
        mock.expect_get_by_peer_id()
            .times(0..)
            .returning(|_| Ok(None));
        mock.expect_list_all()
            .times(0..)
            .returning(|| Ok(Vec::new()));
        mock.expect_upsert().times(0..).returning(|_| Ok(()));
        mock.expect_set_state().times(0..).returning(|_, _| Ok(()));
        mock.expect_update_last_seen()
            .times(0..)
            .returning(|_, _| Ok(()));
        mock.expect_delete().times(0..).returning(|_| Ok(()));
        mock.expect_update_sync_settings()
            .times(0..)
            .returning(|_, _| Ok(()));
        mock
    }

    fn build_clock_mock() -> MockClock {
        let mut mock = MockClock::new();
        mock.expect_now_ms().times(0..).returning(|| 0);
        mock
    }

    fn build_content_hash_mock() -> MockContentHash {
        let mut mock = MockContentHash::new();
        mock.expect_hash_bytes()
            .times(0..)
            .returning(|_| Err(anyhow::anyhow!("noop hash")));
        mock
    }

    fn build_file_manager_mock() -> MockFileManager {
        let mut mock = MockFileManager::new();
        mock.expect_open_directory()
            .times(0..)
            .returning(|_| Ok(()));
        mock
    }

    fn build_cache_fs_mock() -> MockCacheFs {
        let mut mock = MockCacheFs::new();
        mock.expect_exists().times(0..).returning(|_| false);
        mock.expect_read_dir().times(0..).returning(|_| Ok(vec![]));
        mock.expect_remove_dir_all()
            .times(0..)
            .returning(|_| Ok(()));
        mock.expect_remove_file().times(0..).returning(|_| Ok(()));
        mock.expect_dir_size().times(0..).returning(|_| Ok(0));
        mock
    }

    fn build_search_index_mock() -> MockSearchIndex {
        let mut mock = MockSearchIndex::new();
        mock.expect_index_entry()
            .times(0..)
            .returning(|_, _| Ok(()));
        mock.expect_remove_entry().times(0..).returning(|_| Ok(()));
        mock.expect_search().times(0..).returning(|_| {
            Ok(uc_core::search::SearchResultsPage {
                items: vec![],
                total: 0,
                has_more: false,
            })
        });
        mock.expect_rebuild().times(0..).returning(|_, _| Ok(()));
        mock.expect_get_index_meta().times(0..).returning(|| {
            Ok(uc_core::search::SearchIndexMeta {
                index_version: "noop".into(),
                search_blocked: false,
                last_rebuild_started_at_ms: None,
                last_rebuild_completed_at_ms: None,
            })
        });
        mock
    }

    fn build_search_key_derivation_mock() -> MockSearchKeyDerivation {
        let mut mock = MockSearchKeyDerivation::new();
        mock.expect_derive_search_key()
            .times(0..)
            .returning(|| Ok(uc_core::search::SearchKey([0u8; 32])));
        mock
    }

    fn test_storage_paths() -> uc_app::app_paths::AppPaths {
        uc_app::app_paths::AppPaths {
            db_path: std::path::PathBuf::from("/tmp/uniclipboard-test/uniclipboard.db"),
            vault_dir: std::path::PathBuf::from("/tmp/uniclipboard-test/vault"),
            settings_path: std::path::PathBuf::from("/tmp/uniclipboard-test/settings.json"),
            logs_dir: std::path::PathBuf::from("/tmp/uniclipboard-test/logs"),
            cache_dir: std::path::PathBuf::from("/tmp/uniclipboard-test-cache"),
            file_cache_dir: std::path::PathBuf::from("/tmp/uniclipboard-test-cache/file-cache"),
            spool_dir: std::path::PathBuf::from("/tmp/uniclipboard-test-cache/spool"),
            app_data_root_dir: std::path::PathBuf::from("/tmp/uniclipboard-test"),
        }
    }

    #[test]
    fn runtime_event_emitter_can_be_swapped_after_setup() {
        let deps = AppDeps {
            clipboard: uc_app::ClipboardPorts {
                clipboard: Arc::new(build_system_clipboard_mock()),
                system_clipboard: Arc::new(build_system_clipboard_mock()),
                clipboard_entry_repo: Arc::new(build_entry_repository_mock()),
                clipboard_event_repo: Arc::new(build_event_writer_mock()),
                representation_repo: Arc::new(build_representation_repo_mock()),
                representation_normalizer: Arc::new(build_normalizer_mock()),
                selection_repo: Arc::new(build_selection_repo_mock()),
                representation_policy: Arc::new(build_representation_policy_mock()),
                representation_cache: Arc::new(build_representation_cache_mock()),
                spool_queue: Arc::new(build_spool_queue_mock()),
                worker_tx: mpsc::channel(1).0,
                clipboard_change_origin: test_origin(),
                payload_resolver: Arc::new(build_payload_resolver_mock()),
            },
            security: uc_app::SecurityPorts {
                encryption: Arc::new(build_encryption_mock()),
                encryption_session: Arc::new(build_encryption_session_mock()),
                encryption_state: Arc::new(build_encryption_state_mock()),
                key_scope: Arc::new(build_key_scope_mock()),
                secure_storage: Arc::new(build_secure_storage_mock()),
                key_material: Arc::new(build_key_material_mock()),
            },
            device: uc_app::DevicePorts {
                device_repo: Arc::new(build_device_repo_mock()),
                device_identity: Arc::new(build_device_identity_mock()),
                paired_device_repo: Arc::new(build_paired_device_repo_mock()),
            },
            network_ports: noop_network_ports(),
            network_control: Arc::new(build_network_control_mock()),
            setup_status: Arc::new(build_setup_status_mock()),
            storage: uc_app::StoragePorts {
                blob_store: Arc::new(build_blob_store_mock()),
                blob_repository: Arc::new(build_blob_repo_mock()),
                blob_writer: Arc::new(build_blob_writer_mock()),
                thumbnail_repo: Arc::new(build_thumbnail_repo_mock()),
                thumbnail_generator: Arc::new(build_thumbnail_generator_mock()),
                file_transfer_repo: Arc::new(uc_core::ports::NoopFileTransferRepositoryPort),
            },
            settings: Arc::new(build_settings_mock()),
            system: uc_app::SystemPorts {
                clock: Arc::new(build_clock_mock()),
                hash: Arc::new(build_content_hash_mock()),
                file_manager: Arc::new(build_file_manager_mock()),
                cache_fs: Arc::new(build_cache_fs_mock()),
            },
            search: uc_app::deps::SearchPorts {
                search_index: Arc::new(build_search_index_mock()),
                search_key_derivation: Arc::new(build_search_key_derivation_mock()),
                search_pipeline: std::sync::Arc::new(uc_infra::search::SearchPipeline::new()),
            },
        };
        let setup_ports = uc_bootstrap::assembly::SetupAssemblyPorts::placeholder(&deps);
        let initial_events = Arc::new(Mutex::new(Vec::new()));
        let swapped_events = Arc::new(Mutex::new(Vec::new()));
        let initial_emitter: Arc<dyn HostEventEmitterPort> =
            Arc::new(build_recording_emitter_mock(initial_events.clone()));
        let swapped_emitter: Arc<dyn HostEventEmitterPort> =
            Arc::new(build_recording_emitter_mock(swapped_events.clone()));
        let runtime = AppRuntime::with_setup(
            deps,
            setup_ports,
            test_storage_paths(),
            initial_emitter.clone(),
        );

        runtime.set_event_emitter(swapped_emitter.clone());
        runtime
            .event_emitter()
            .emit(HostEvent::Clipboard(ClipboardHostEvent::NewContent {
                entry_id: "entry-swap".to_string(),
                preview: "swapped".to_string(),
                origin: uc_core::ports::host_event_emitter::ClipboardOriginKind::Local,
            }))
            .expect("emit through swapped emitter");

        assert!(initial_events.lock().unwrap().is_empty());
        let recorded_swapped_events = swapped_events.lock().unwrap();
        assert_eq!(recorded_swapped_events.len(), 1);
        match &recorded_swapped_events[0] {
            HostEvent::Clipboard(ClipboardHostEvent::NewContent { entry_id, .. }) => {
                assert_eq!(entry_id, "entry-swap");
            }
            other => panic!("expected NewContent event on swapped emitter, got {other:?}"),
        }
    }
}
