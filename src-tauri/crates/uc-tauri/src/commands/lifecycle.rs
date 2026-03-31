//! Lifecycle-related Tauri commands
//! 应用生命周期相关的 Tauri 命令

use crate::bootstrap::AppRuntime;
use crate::commands::error::CommandError;
use crate::commands::record_trace_fields;
use crate::models::LifecycleStatusDto;
use std::sync::Arc;
use tauri::State;
use tracing::{info_span, Instrument};
use uc_platform::ports::observability::TraceMetadata;

/// Retry lifecycle boot (watcher + network + session ready).
///
/// 重试生命周期启动（监视器 + 网络 + 会话就绪）。
#[tauri::command]
pub async fn retry_lifecycle(
    runtime: State<'_, Arc<AppRuntime>>,
    _trace: Option<TraceMetadata>,
) -> Result<(), CommandError> {
    let span = info_span!(
        "command.lifecycle.retry",
        trace_id = tracing::field::Empty,
        trace_ts = tracing::field::Empty,
    );
    record_trace_fields(&span, &_trace);
    async {
        runtime
            .usecases()
            .app_lifecycle_coordinator()
            .ensure_ready()
            .await
            .map_err(CommandError::internal)
    }
    .instrument(span)
    .await
}

/// Get the OS process ID of the Tauri application.
///
/// 获取 Tauri 应用的操作系统进程 ID。
#[tauri::command]
pub fn get_tauri_pid() -> u32 {
    std::process::id()
}

/// Get current lifecycle status as a typed DTO.
///
/// 获取当前生命周期状态（类型化 DTO）。
#[tauri::command]
pub async fn get_lifecycle_status(
    runtime: State<'_, Arc<AppRuntime>>,
    _trace: Option<TraceMetadata>,
) -> Result<LifecycleStatusDto, CommandError> {
    let span = info_span!(
        "command.lifecycle.get_status",
        trace_id = tracing::field::Empty,
        trace_ts = tracing::field::Empty,
    );
    record_trace_fields(&span, &_trace);
    async {
        let status_port = runtime.usecases().get_lifecycle_status();
        let state = status_port.get_state().await;
        Ok(LifecycleStatusDto::from_state(state))
    }
    .instrument(span)
    .await
}
