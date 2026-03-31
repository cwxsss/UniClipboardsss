//! Storage management Tauri commands
//! 存储管理相关的 Tauri 命令

use crate::commands::error::CommandError;
use crate::commands::record_trace_fields;
use tracing::{info_span, Instrument};
use uc_core::ports::file_manager::FileManagerError;
use uc_platform::ports::observability::TraceMetadata;

/// Open the application data directory in the system file manager.
/// 在系统文件管理器中打开应用数据目录。
#[tauri::command]
pub async fn open_data_directory(
    runtime: tauri::State<'_, std::sync::Arc<crate::bootstrap::AppRuntime>>,
    _trace: Option<TraceMetadata>,
) -> Result<(), CommandError> {
    let span = info_span!(
        "command.storage.open_data_dir",
        trace_id = tracing::field::Empty,
        trace_ts = tracing::field::Empty,
    );
    record_trace_fields(&span, &_trace);

    async move {
        runtime
            .usecases()
            .open_data_directory()
            .execute()
            .await
            .map_err(|e| {
                if e.downcast_ref::<FileManagerError>()
                    .is_some_and(|fe| matches!(fe, FileManagerError::DirectoryNotFound(_)))
                {
                    CommandError::NotFound(e.to_string())
                } else {
                    CommandError::InternalError(e.to_string())
                }
            })
    }
    .instrument(span)
    .await
}
