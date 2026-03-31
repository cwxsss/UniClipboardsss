//! Pairing-related Tauri commands
//! 配对相关的 Tauri 命令

use crate::bootstrap::AppRuntime;
use crate::commands::error::CommandError;
use crate::commands::record_trace_fields;
use std::sync::Arc;
use tauri::State;
use tracing::{info_span, Instrument};
use uc_core::PeerId;
use uc_platform::ports::observability::TraceMetadata;

/// Get resolved sync settings for a specific device.
/// Returns per-device overrides if set, otherwise global defaults.
#[tauri::command]
pub async fn get_device_sync_settings(
    runtime: State<'_, Arc<AppRuntime>>,
    peer_id: String,
    _trace: Option<TraceMetadata>,
) -> Result<uc_core::settings::model::SyncSettings, CommandError> {
    let span = info_span!(
        "command.pairing.get_device_sync_settings",
        trace_id = tracing::field::Empty,
        trace_ts = tracing::field::Empty,
        peer_id = %peer_id,
    );
    record_trace_fields(&span, &_trace);
    async {
        let uc = runtime.usecases().get_device_sync_settings();
        uc.execute(&PeerId::from(peer_id.as_str()))
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "Failed to get device sync settings");
                CommandError::InternalError(e.to_string())
            })
    }
    .instrument(span)
    .await
}

/// Update or clear per-device sync settings.
/// Passing `null` for settings resets to global defaults.
#[tauri::command]
pub async fn update_device_sync_settings(
    runtime: State<'_, Arc<AppRuntime>>,
    peer_id: String,
    settings: Option<uc_core::settings::model::SyncSettings>,
    _trace: Option<TraceMetadata>,
) -> Result<(), CommandError> {
    let span = info_span!(
        "command.pairing.update_device_sync_settings",
        trace_id = tracing::field::Empty,
        trace_ts = tracing::field::Empty,
        peer_id = %peer_id,
    );
    record_trace_fields(&span, &_trace);
    async {
        let uc = runtime.usecases().update_device_sync_settings();
        uc.execute(&PeerId::from(peer_id.as_str()), settings)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "Failed to update device sync settings");
                CommandError::InternalError(e.to_string())
            })
    }
    .instrument(span)
    .await
}
