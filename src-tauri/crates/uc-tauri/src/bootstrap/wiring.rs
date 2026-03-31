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
use tracing::{info, warn};

use uc_app::task_registry::TaskRegistry;
use uc_app::usecases::file_sync::CleanupExpiredFilesUseCase;

// Re-export assembly types from uc-bootstrap.
pub use uc_bootstrap::assembly::{
    get_storage_paths, resolve_pairing_config, resolve_pairing_device_name, wire_dependencies,
    wire_dependencies_with_identity_store, HostEventSetupPort, WiredDependencies, WiringError,
    WiringResult,
};

// Re-export BackgroundRuntimeDeps from uc-bootstrap (definition moved in Phase 40).
pub use uc_bootstrap::BackgroundRuntimeDeps;

/// Start the file cache cleanup task (runs once at startup, fire-and-forget).
pub fn start_background_tasks(
    settings: Arc<dyn uc_core::ports::SettingsPort>,
    file_cache_dir: std::path::PathBuf,
    task_registry: &Arc<TaskRegistry>,
) {
    let registry = task_registry.clone();
    async_runtime::spawn(async move {
        registry
            .spawn("file_cache_cleanup", |_token| async move {
                let uc = CleanupExpiredFilesUseCase::new(settings, file_cache_dir.clone());
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
