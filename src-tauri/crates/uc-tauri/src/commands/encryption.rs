//! Encryption-related Tauri commands and helpers.
//! 加密相关的 Tauri 命令和辅助函数。

use crate::bootstrap::AppRuntime;
use crate::commands::record_trace_fields;
use crate::events::EncryptionEvent;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Runtime};
use tracing::{info, info_span, warn, Instrument};
use uc_platform::ports::observability::TraceMetadata;

const UNLOCK_CONTEXT: &str = "[unlock_encryption_session]";

fn emit_session_failed<R: Runtime>(
    app_handle: &AppHandle<R>,
    reason: String,
) -> Result<(), String> {
    app_handle
        .emit("encryption://event", EncryptionEvent::Failed { reason })
        .map_err(|e| format!("emit session failed event failed: {}", e))
}

pub async fn unlock_encryption_session_with_runtime<R: Runtime>(
    runtime: &Arc<AppRuntime>,
    app_handle: &AppHandle<R>,
    trace: Option<TraceMetadata>,
    daemon_connection_state: Option<&uc_daemon_client::DaemonConnectionState>,
) -> Result<bool, String> {
    let span = info_span!(
        "command.encryption.unlock_session",
        trace_id = tracing::field::Empty,
        trace_ts = tracing::field::Empty,
    );
    record_trace_fields(&span, &trace);
    let uc = runtime.usecases().auto_unlock_encryption_session();
    info!("{} Attempting keyring unlock", UNLOCK_CONTEXT);
    async {
        match uc.execute().await {
            Ok(true) => {
                info!("{} Keyring unlock completed", UNLOCK_CONTEXT);
                if let Err(e) = runtime
                    .usecases()
                    .app_lifecycle_coordinator()
                    .ensure_ready()
                    .await
                {
                    warn!("{} Auto lifecycle boot failed: {}", UNLOCK_CONTEXT, e);
                } else {
                    info!("{} Auto lifecycle boot completed", UNLOCK_CONTEXT);
                }

                // Signal the daemon to enable clipboard capture.
                // In --gui-managed mode, the daemon defers clipboard monitoring
                // until the GUI explicitly signals readiness after unlock.
                if let Some(conn) = daemon_connection_state {
                    let client = uc_daemon_client::DaemonQueryClient::new(conn.clone());
                    if let Err(e) = client.signal_lifecycle_ready().await {
                        warn!(
                            "{} Failed to signal daemon lifecycle ready: {}",
                            UNLOCK_CONTEXT, e
                        );
                    } else {
                        info!("{} Daemon clipboard capture enabled", UNLOCK_CONTEXT);
                    }
                }

                Ok(true)
            }
            Ok(false) => {
                info!(
                    "{} Encryption not initialized, unlock skipped",
                    UNLOCK_CONTEXT
                );
                Ok(false)
            }
            Err(err) => {
                let reason = err.to_string();
                warn!("{} Keyring unlock failed: {}", UNLOCK_CONTEXT, reason);
                if let Err(emit_err) = emit_session_failed(app_handle, reason.clone()) {
                    warn!(
                        "{} Failed to emit session failed event: {}",
                        UNLOCK_CONTEXT, emit_err
                    );
                }
                Err(reason)
            }
        }
    }
    .instrument(span)
    .await
}
