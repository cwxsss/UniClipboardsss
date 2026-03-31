//! # Dependency Injection / 依赖注入模块
//!
//! ## Responsibilities / 职责
//!
//! - ✅ Create infra implementations (db, fs, secure storage) / 创建 infra 层具体实现
//! - ✅ Create platform implementations (clipboard, network) / 创建 platform 层具体实现
//! - ✅ Inject all dependencies into App / 将所有依赖注入到 App
//!
//! ## Prohibited / 禁止事项
//!
//! ❌ **No business logic / 禁止包含任何业务逻辑**
//! - Do not decide "what to do if encryption uninitialized"
//! - 不判断"如果加密未初始化就怎样"
//! - Do not handle "what to do if device not registered"
//! - 不处理"如果设备未注册就怎样"
//!
//! ❌ **No configuration validation / 禁止做配置验证**
//! - Config already loaded in config.rs
//! - 配置已在 config.rs 加载
//! - Validation should be in use case or upper layer
//! - 验证应在 use case 或上层
//!
//! ❌ **No direct concrete implementation usage / 禁止直接使用具体实现**
//! - Must inject through Port traits
//! - 必须通过 Port trait 注入
//! - Do not call implementation methods directly after App construction
//! - 不在 App 构造后直接调用实现方法
//!
//! ## Architecture Principle / 架构原则
//!
//! > **This is the only place allowed to depend on uc-infra + uc-platform + uc-app simultaneously.**
//! > **这是唯一允许同时依赖 uc-infra、uc-platform 和 uc-app 的地方。**
//! > But this privilege is only for "assembly", not for "decision making".
//! > 但这种特权仅用于"组装"，不用于"决策"。

use std::sync::Arc;
use tauri::async_runtime;
use tracing::info;

use uc_app::task_registry::TaskRegistry;
use uc_app::AppDeps;
use uc_core::ports::host_event_emitter::HostEventEmitterPort;
use uc_daemon_client::realtime::start_realtime_runtime;
// Re-export assembly types from uc-bootstrap.
pub use uc_bootstrap::assembly::{
    get_storage_paths, resolve_pairing_config, resolve_pairing_device_name, wire_dependencies,
    wire_dependencies_with_identity_store, HostEventSetupPort, WiredDependencies, WiringError,
    WiringResult,
};

// Re-export BackgroundRuntimeDeps from uc-bootstrap (definition moved in Phase 40).
pub use uc_bootstrap::BackgroundRuntimeDeps;

/// Start background spooler and blob worker tasks.
/// 启动后台假脱机写入和 blob 物化任务。
///
/// All long-lived tasks are spawned through the `TaskRegistry` for centralized
/// lifecycle management and graceful shutdown via cooperative cancellation.
pub fn start_background_tasks(
    background: BackgroundRuntimeDeps,
    deps: &AppDeps,
    event_emitter: Arc<dyn HostEventEmitterPort>,
    daemon_connection_state: uc_daemon_client::DaemonConnectionState,
    setup_pairing_event_hub: Arc<uc_app::realtime::SetupPairingEventHub>,
    task_registry: &Arc<TaskRegistry>,
) {
    // Clones for GUI-only tasks
    let deps_settings = deps.settings.clone();
    let cleanup_file_cache_dir = background.file_cache_dir.clone();
    let blob_ports = uc_bootstrap::BlobProcessingPorts::from_app_deps(deps);

    // Spawn all long-lived tasks through the TaskRegistry for lifecycle management.
    // We use a single orchestration spawn to set up all registry tasks, since
    // registry.spawn() is async and start_background_tasks is sync.
    let registry = task_registry.clone();
    async_runtime::spawn(async move {
        // --- Shared blob processing tasks (SpoolScanner + SpoolerTask + BackgroundBlobWorker + SpoolJanitor) ---
        uc_bootstrap::spawn_blob_processing_tasks(background, blob_ports, &registry).await;

        // --- Unified realtime runtime (daemon WebSocket bridge + app consumers) ---
        start_realtime_runtime(
            daemon_connection_state,
            event_emitter.clone(),
            setup_pairing_event_hub,
            &registry,
        )
        .await;
        info!("Started unified daemon realtime runtime");

        // --- File cache cleanup (runs once at startup, fire-and-forget) ---
        {
            use tracing::warn;
            let cleanup_settings = deps_settings.clone();
            let cleanup_cache_dir = cleanup_file_cache_dir.clone();
            registry
                .spawn("file_cache_cleanup", |_token| async move {
                    let uc = uc_app::usecases::file_sync::CleanupExpiredFilesUseCase::new(
                        cleanup_settings,
                        cleanup_cache_dir,
                    );
                    match uc.execute().await {
                        Ok(result) => {
                            if result.files_removed > 0 {
                                info!(
                                    files_removed = result.files_removed,
                                    bytes_reclaimed = result.bytes_reclaimed,
                                    "Startup file cache cleanup completed"
                                );
                            }
                        }
                        Err(e) => {
                            warn!(error = %e, "Startup file cache cleanup failed (non-fatal)");
                        }
                    }
                })
                .await;
        }

        info!("All background tasks registered with TaskRegistry");
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wiring_error_display() {
        let err = WiringError::DatabaseInit("connection failed".to_string());
        assert!(err.to_string().contains("Database initialization"));
        assert!(err.to_string().contains("connection failed"));
    }
}
