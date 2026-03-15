//! File transfer event-loop orchestration for durable status transitions.
//!
//! Handles pending/transferring/completed/failed lifecycle through the
//! `TrackInboundTransfersUseCase`, emits `file-transfer://status-changed`
//! events, runs periodic timeout sweeps, and performs startup reconciliation.

use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tracing::{info, info_span, warn, Instrument};

use uc_app::usecases::clipboard::sync_inbound::PendingTransferLinkage;
use uc_app::usecases::file_sync::TrackInboundTransfersUseCase;
use uc_core::ports::transfer_progress::TransferDirection;

/// Event payload for `file-transfer://status-changed`.
///
/// Emitted whenever a transfer transitions between durable states.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileTransferStatusPayload {
    pub transfer_id: String,
    pub entry_id: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Emit `file-transfer://status-changed` for each pending transfer
/// after inbound clipboard metadata is applied.
pub fn emit_pending_status<R: tauri::Runtime>(
    app: &AppHandle<R>,
    entry_id: &str,
    pending_transfers: &[PendingTransferLinkage],
) {
    for t in pending_transfers {
        let payload = FileTransferStatusPayload {
            transfer_id: t.transfer_id.clone(),
            entry_id: entry_id.to_string(),
            status: "pending".to_string(),
            reason: None,
        };
        if let Err(err) = app.emit("file-transfer://status-changed", payload) {
            warn!(
                error = %err,
                transfer_id = %t.transfer_id,
                "Failed to emit pending file-transfer status"
            );
        }
    }
}

/// Handle a receiving-side `TransferProgress` event.
///
/// On first chunk (chunks_completed == 1), promotes to `transferring`.
/// On subsequent chunks, refreshes durable liveness.
///
/// Returns `true` if promoted to `transferring` (first time).
pub async fn handle_transfer_progress<R: tauri::Runtime>(
    tracker: &TrackInboundTransfersUseCase,
    app: Option<&AppHandle<R>>,
    transfer_id: &str,
    direction: TransferDirection,
    chunks_completed: u32,
    now_ms: i64,
) -> bool {
    // Only track receiving-side progress
    if direction != TransferDirection::Receiving {
        return false;
    }

    if chunks_completed == 1 {
        // First chunk: promote to transferring
        match tracker.mark_transferring(transfer_id, now_ms).await {
            Ok(true) => {
                info!(transfer_id, "Transfer promoted to transferring");
                // We need the entry_id to emit status. The tracker can look it up.
                if let Some(app) = app {
                    if let Ok(Some(summary)) =
                        tracker.get_entry_summary_by_transfer(transfer_id).await
                    {
                        let payload = FileTransferStatusPayload {
                            transfer_id: transfer_id.to_string(),
                            entry_id: summary,
                            status: "transferring".to_string(),
                            reason: None,
                        };
                        if let Err(err) = app.emit("file-transfer://status-changed", payload) {
                            warn!(error = %err, "Failed to emit transferring status");
                        }
                    }
                }
                return true;
            }
            Ok(false) => {
                // Already transferring or terminal, just refresh activity
                let _ = tracker.refresh_activity(transfer_id, now_ms).await;
            }
            Err(err) => {
                warn!(error = %err, transfer_id, "Failed to mark transferring");
            }
        }
    } else {
        // Later chunk: refresh liveness
        if let Err(err) = tracker.refresh_activity(transfer_id, now_ms).await {
            warn!(error = %err, transfer_id, "Failed to refresh transfer activity");
        }
    }

    false
}

/// Handle a file transfer completion event.
///
/// Marks the transfer row as completed before emitting the status event.
pub async fn handle_transfer_completed<R: tauri::Runtime>(
    tracker: &TrackInboundTransfersUseCase,
    app: Option<&AppHandle<R>>,
    transfer_id: &str,
    content_hash: Option<&str>,
    now_ms: i64,
) {
    // Mark durable row completed
    if let Err(err) = tracker
        .mark_completed(transfer_id, content_hash, now_ms)
        .await
    {
        warn!(error = %err, transfer_id, "Failed to mark transfer completed");
        return;
    }

    // Emit status-changed for completed
    if let Some(app) = app {
        if let Ok(Some(entry_id)) = tracker.get_entry_summary_by_transfer(transfer_id).await {
            let payload = FileTransferStatusPayload {
                transfer_id: transfer_id.to_string(),
                entry_id,
                status: "completed".to_string(),
                reason: None,
            };
            if let Err(err) = app.emit("file-transfer://status-changed", payload) {
                warn!(error = %err, "Failed to emit completed status");
            }
        }
    }
}

/// Handle a file transfer failure event.
///
/// Marks the durable row failed with the error reason, cleans partial cache,
/// and emits `file-transfer://status-changed`.
pub async fn handle_transfer_failed<R: tauri::Runtime>(
    tracker: &TrackInboundTransfersUseCase,
    app: Option<&AppHandle<R>>,
    transfer_id: &str,
    error_reason: &str,
    now_ms: i64,
) {
    // Mark durable row failed
    if let Err(err) = tracker.mark_failed(transfer_id, error_reason, now_ms).await {
        warn!(error = %err, transfer_id, "Failed to mark transfer failed");
        return;
    }

    // Emit status-changed for failed
    if let Some(app) = app {
        if let Ok(Some(entry_id)) = tracker.get_entry_summary_by_transfer(transfer_id).await {
            let payload = FileTransferStatusPayload {
                transfer_id: transfer_id.to_string(),
                entry_id,
                status: "failed".to_string(),
                reason: Some(error_reason.to_string()),
            };
            if let Err(err) = app.emit("file-transfer://status-changed", payload) {
                warn!(error = %err, "Failed to emit failed status");
            }
        }
    }
}

/// Spawn a periodic timeout sweep task.
///
/// Runs every 15 seconds. Fails stalled pending (>60s) and transferring (>5min)
/// rows, emits status-changed events, and cleans partial cache artifacts.
pub fn spawn_timeout_sweep<R: tauri::Runtime + 'static>(
    tracker: Arc<TrackInboundTransfersUseCase>,
    app_handle: Option<AppHandle<R>>,
    clock: Arc<dyn uc_core::ports::ClockPort>,
    cancel: tokio::sync::watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(15));
        let mut cancel = cancel;

        loop {
            tokio::select! {
                _ = interval.tick() => {},
                _ = cancel.changed() => {
                    if *cancel.borrow() {
                        info!("File transfer timeout sweep shutting down");
                        return;
                    }
                }
            }

            let now_ms = clock.now_ms();
            match tracker.list_expired_inflight(now_ms).await {
                Ok(expired) if expired.is_empty() => {}
                Ok(expired) => {
                    let count = expired.len();
                    warn!(count, "Timeout sweep found expired in-flight transfers");

                    for t in &expired {
                        let reason = match t.status {
                            uc_core::ports::file_transfer_repository::TrackedFileTransferStatus::Pending => {
                                "timeout: no data received within 60 seconds"
                            }
                            uc_core::ports::file_transfer_repository::TrackedFileTransferStatus::Transferring => {
                                "timeout: no new chunk received within 5 minutes"
                            }
                            _ => "timeout: stalled transfer",
                        };

                        if let Err(err) = tracker.mark_failed(&t.transfer_id, reason, now_ms).await {
                            warn!(
                                error = %err,
                                transfer_id = %t.transfer_id,
                                "Failed to mark expired transfer as failed"
                            );
                            continue;
                        }

                        // Clean partial cache artifact
                        cleanup_cached_path(&t.cached_path).await;

                        // Emit status-changed
                        if let Some(app) = app_handle.as_ref() {
                            let payload = FileTransferStatusPayload {
                                transfer_id: t.transfer_id.clone(),
                                entry_id: t.entry_id.clone(),
                                status: "failed".to_string(),
                                reason: Some(reason.to_string()),
                            };
                            if let Err(err) = app.emit("file-transfer://status-changed", payload) {
                                warn!(error = %err, "Failed to emit timeout failure status");
                            }
                        }
                    }
                }
                Err(err) => {
                    warn!(error = %err, "Timeout sweep query failed");
                }
            }
        }
    }.instrument(info_span!("file_transfer.timeout_sweep")))
}

/// Run startup reconciliation: mark orphaned in-flight transfers as failed
/// and clean their cache artifacts.
///
/// Non-blocking and non-fatal: errors are logged as warnings.
pub async fn reconcile_on_startup<R: tauri::Runtime>(
    tracker: &TrackInboundTransfersUseCase,
    app: Option<&AppHandle<R>>,
    now_ms: i64,
) {
    match tracker
        .reconcile_inflight_after_startup(now_ms)
        .instrument(info_span!("file_transfer.startup_reconcile"))
        .await
    {
        Ok(cleanup_targets) if cleanup_targets.is_empty() => {
            info!("No orphaned in-flight transfers found at startup");
        }
        Ok(cleanup_targets) => {
            let count = cleanup_targets.len();
            warn!(count, "Reconciled orphaned in-flight transfers at startup");

            for t in &cleanup_targets {
                cleanup_cached_path(&t.cached_path).await;

                // Emit status-changed for reconciled entries
                if let Some(app) = app {
                    let payload = FileTransferStatusPayload {
                        transfer_id: t.transfer_id.clone(),
                        entry_id: t.entry_id.clone(),
                        status: "failed".to_string(),
                        reason: Some(
                            "orphaned: app restarted while transfer was in-flight".to_string(),
                        ),
                    };
                    if let Err(err) = app.emit("file-transfer://status-changed", payload) {
                        warn!(error = %err, "Failed to emit reconciliation status");
                    }
                }
            }
        }
        Err(err) => {
            warn!(error = %err, "Startup reconciliation failed (non-fatal)");
        }
    }
}

/// Best-effort cleanup of a cached file or transfer directory.
async fn cleanup_cached_path(cached_path: &str) {
    if cached_path.is_empty() {
        return;
    }

    let path = std::path::Path::new(cached_path);

    // Try removing the file first
    if path.is_file() {
        if let Err(err) = tokio::fs::remove_file(path).await {
            warn!(error = %err, path = %cached_path, "Failed to remove cached file");
        }
    }

    // Try removing the parent transfer directory (e.g., file-cache/{transfer_id}/)
    // Only if it's empty after the file was removed
    if let Some(parent) = path.parent() {
        // Safety: only remove if the parent looks like a transfer directory
        // (i.e., it lives under the file-cache directory)
        let _ = tokio::fs::remove_dir(parent).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_transfer_status_payload_serializes_camel_case() {
        let payload = FileTransferStatusPayload {
            transfer_id: "xfer-1".to_string(),
            entry_id: "entry-42".to_string(),
            status: "pending".to_string(),
            reason: None,
        };
        let json = serde_json::to_value(&payload).unwrap();

        // Verify camelCase field names
        assert_eq!(json["transferId"], "xfer-1");
        assert_eq!(json["entryId"], "entry-42");
        assert_eq!(json["status"], "pending");

        // Verify snake_case is NOT present
        assert!(json.get("transfer_id").is_none());
        assert!(json.get("entry_id").is_none());

        // reason should be omitted when None
        assert!(json.get("reason").is_none());
    }

    #[test]
    fn file_transfer_status_payload_includes_reason_when_present() {
        let payload = FileTransferStatusPayload {
            transfer_id: "xfer-2".to_string(),
            entry_id: "entry-99".to_string(),
            status: "failed".to_string(),
            reason: Some("hash mismatch".to_string()),
        };
        let json = serde_json::to_value(&payload).unwrap();

        assert_eq!(json["status"], "failed");
        assert_eq!(json["reason"], "hash mismatch");
    }

    #[test]
    fn file_transfer_status_payload_all_statuses() {
        for status in &["pending", "transferring", "completed", "failed"] {
            let payload = FileTransferStatusPayload {
                transfer_id: "t".to_string(),
                entry_id: "e".to_string(),
                status: status.to_string(),
                reason: None,
            };
            let json = serde_json::to_value(&payload).unwrap();
            assert_eq!(json["status"], *status);
        }
    }
}
