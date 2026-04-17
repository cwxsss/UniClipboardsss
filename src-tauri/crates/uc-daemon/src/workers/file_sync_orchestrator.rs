//! File sync orchestrator worker for the daemon.
//!
//! Subscribes to NetworkEventPort for file transfer lifecycle events
//! (TransferProgress, FileTransferCompleted, FileTransferFailed),
//! delegates to FileTransferOrchestrator for durable state tracking,
//! and restores completed files to the OS clipboard.
//!
//! Also runs startup reconciliation (orphaned in-flight → failed) and
//! periodic timeout sweeps (stalled pending/transferring → failed).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, info_span, instrument, warn, Instrument};

use uc_app::usecases::clipboard::clipboard_write_coordinator::{
    ClipboardWriteCoordinator, ClipboardWriteIntent,
};
use uc_app::usecases::file_sync::SyncInboundFileUseCase;
use uc_application::file_transfer::{
    CompleteTransfer, FailTransfer, ReportTransferProgress, StartTransfer,
};
use uc_bootstrap::file_transfer_lifecycle::FileTransferLifecycle;
use uc_core::file_transfer::{FileTransferFailureReason, FileTransferProgress};
use uc_core::network::NetworkEvent;
use uc_core::ports::{NetworkEventPort, SettingsPort};

use crate::service::{DaemonService, ServiceHealth};

pub struct FileSyncOrchestratorWorker {
    lifecycle: Arc<FileTransferLifecycle>,
    network_events: Arc<dyn NetworkEventPort>,
    clipboard_write_coordinator: Arc<ClipboardWriteCoordinator>,
    file_cache_dir: PathBuf,
    settings: Arc<dyn SettingsPort>,
}

impl FileSyncOrchestratorWorker {
    pub fn new(
        lifecycle: Arc<FileTransferLifecycle>,
        network_events: Arc<dyn NetworkEventPort>,
        clipboard_write_coordinator: Arc<ClipboardWriteCoordinator>,
        file_cache_dir: PathBuf,
        settings: Arc<dyn SettingsPort>,
    ) -> Self {
        Self {
            lifecycle,
            network_events,
            clipboard_write_coordinator,
            file_cache_dir,
            settings,
        }
    }
}

#[async_trait]
impl DaemonService for FileSyncOrchestratorWorker {
    fn name(&self) -> &str {
        "file-sync-orchestrator"
    }

    async fn start(&self, cancel: CancellationToken) -> anyhow::Result<()> {
        info!("file sync orchestrator starting");

        // 1. Run startup reconciliation (orphaned in-flight transfers → failed)
        self.lifecycle.reconcile_on_startup().await;

        // 2. Start timeout sweep (15s interval, cancellable via watch channel)
        let (sweep_cancel_tx, sweep_cancel_rx) = tokio::sync::watch::channel(false);
        let _sweep_handle = self.lifecycle.spawn_timeout_sweep(sweep_cancel_rx);

        // 3. Subscribe to network events
        let mut event_rx = match self.network_events.subscribe_events().await {
            Ok(rx) => rx,
            Err(err) => {
                let _ = sweep_cancel_tx.send(true);
                return Err(err);
            }
        };

        info!("file sync orchestrator subscribed to network events");

        // 4. Batch accumulator: batch_id -> (completed_paths, expected_total, peer_id)
        let mut batch_accumulator: HashMap<String, (Vec<PathBuf>, u32, String)> = HashMap::new();

        // 5. Event loop
        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    let _ = sweep_cancel_tx.send(true);
                    info!("file sync orchestrator cancelled");
                    return Ok(());
                }
                maybe_event = event_rx.recv() => {
                    let Some(event) = maybe_event else {
                        warn!("network event channel closed");
                        let _ = sweep_cancel_tx.send(true);
                        return Ok(());
                    };
                    self.handle_network_event(event, &mut batch_accumulator).await;
                }
            }
        }
    }

    async fn stop(&self) -> anyhow::Result<()> {
        info!("file sync orchestrator stopped");
        Ok(())
    }

    fn health_check(&self) -> ServiceHealth {
        ServiceHealth::Healthy
    }
}

impl FileSyncOrchestratorWorker {
    async fn handle_network_event(
        &self,
        event: NetworkEvent,
        batch_accumulator: &mut HashMap<String, (Vec<PathBuf>, u32, String)>,
    ) {
        #[allow(deprecated)]
        match event {
            NetworkEvent::FileTransferStarted {
                transfer_id,
                peer_id,
                filename,
                file_size,
            } => {
                if let Err(err) = self
                    .lifecycle
                    .start
                    .execute(StartTransfer {
                        transfer_id: transfer_id.clone(),
                        peer_id: peer_id.clone(),
                        filename,
                        file_size,
                    })
                    .await
                {
                    warn!(
                        error = %err,
                        transfer_id = %transfer_id,
                        peer_id = %peer_id,
                        "Failed to record transfer start"
                    );
                }
            }
            NetworkEvent::TransferProgress(progress) => {
                let transfer_id = progress.transfer_id.clone();
                let peer_id = progress.peer_id.clone();
                let domain_progress: FileTransferProgress = progress.into();
                if let Err(err) = self
                    .lifecycle
                    .report_progress
                    .execute(ReportTransferProgress {
                        transfer_id: transfer_id.clone(),
                        peer_id,
                        progress: domain_progress,
                    })
                    .await
                {
                    // Progress arriving before the matching Started event is
                    // rare but possible under protocol reordering; log and move on.
                    debug!(
                        error = %err,
                        transfer_id = %transfer_id,
                        "Failed to record transfer progress"
                    );
                }
            }
            NetworkEvent::FileTransferCompleted {
                transfer_id,
                peer_id,
                filename,
                file_path,
                batch_id,
                batch_total,
            } => {
                debug!(
                    transfer_id = %transfer_id,
                    peer_id = %peer_id,
                    filename = %filename,
                    file_path = %file_path.display(),
                    batch_id = ?batch_id,
                    batch_total = ?batch_total,
                    "File transfer completed, processing inbound file"
                );

                let inbound_uc =
                    SyncInboundFileUseCase::new(self.settings.clone(), self.file_cache_dir.clone());

                let lifecycle = Arc::clone(&self.lifecycle);
                let coordinator = self.clipboard_write_coordinator.clone();
                let is_batch = batch_id.is_some() && batch_total.is_some();
                let span_tid = transfer_id.clone();
                let file_path_for_spawn = file_path.clone();
                let transfer_id_for_spawn = transfer_id.clone();
                let peer_id_for_spawn = peer_id.clone();

                tokio::spawn(
                    async move {
                        let file_bytes = match tokio::fs::read(&file_path_for_spawn).await {
                            Ok(bytes) => bytes,
                            Err(err) => {
                                error!(
                                    transfer_id = %transfer_id_for_spawn,
                                    error = %err,
                                    "Failed to read transferred file for hash verification"
                                );
                                fail_transfer(
                                    &lifecycle,
                                    &transfer_id_for_spawn,
                                    &peer_id_for_spawn,
                                    FileTransferFailureReason::StorageUnavailable,
                                    Some(format!("Failed to read file: {err}")),
                                )
                                .await;
                                return;
                            }
                        };

                        let expected_hash = blake3::hash(&file_bytes).to_hex().to_string();

                        match inbound_uc
                            .handle_transfer_complete(
                                &transfer_id_for_spawn,
                                &file_path_for_spawn,
                                &expected_hash,
                            )
                            .await
                        {
                            Ok(result) => {
                                info!(
                                    transfer_id = %result.transfer_id,
                                    file_size = result.file_size,
                                    auto_pulled = result.auto_pulled,
                                    "Inbound file sync processed"
                                );

                                if let Err(err) = lifecycle
                                    .complete
                                    .execute(CompleteTransfer {
                                        transfer_id: result.transfer_id.clone(),
                                        peer_id: peer_id_for_spawn.clone(),
                                    })
                                    .await
                                {
                                    warn!(
                                        error = %err,
                                        transfer_id = %result.transfer_id,
                                        "Failed to record transfer completion"
                                    );
                                }

                                // Drop sender-side entry hint once the transfer finishes.
                                lifecycle.outbound_entry_cache.remove(&result.transfer_id);

                                if !is_batch {
                                    restore_file_to_clipboard_after_transfer(
                                        vec![result.file_path],
                                        &coordinator,
                                    )
                                    .await;
                                }
                            }
                            Err(err) => {
                                error!(
                                    transfer_id = %transfer_id_for_spawn,
                                    error = %err,
                                    "Inbound file sync processing failed"
                                );
                                fail_transfer(
                                    &lifecycle,
                                    &transfer_id_for_spawn,
                                    &peer_id_for_spawn,
                                    FileTransferFailureReason::Unknown,
                                    Some(format!("Inbound file sync failed: {err}")),
                                )
                                .await;
                            }
                        }
                    }
                    .instrument(info_span!("inbound_file_sync", transfer_id = %span_tid)),
                );

                // Handle batch accumulation (outside spawn for state access)
                if let (Some(bid), Some(total)) = (batch_id, batch_total) {
                    let entry = batch_accumulator
                        .entry(bid.clone())
                        .or_insert_with(|| (Vec::new(), total, peer_id.clone()));
                    entry.0.push(file_path.clone());

                    if entry.0.len() < total as usize {
                        info!(
                            batch_id = %bid,
                            completed = entry.0.len(),
                            total = total,
                            "Batch file received, waiting for remaining files"
                        );
                    } else {
                        let all_paths = entry.0.clone();
                        batch_accumulator.remove(&bid);
                        info!(
                            batch_id = %bid,
                            total = total,
                            "Batch complete, restoring all files to clipboard"
                        );

                        let coordinator_batch = self.clipboard_write_coordinator.clone();
                        tokio::spawn(async move {
                            restore_file_to_clipboard_after_transfer(all_paths, &coordinator_batch)
                                .await;
                        });
                    }
                }
            }
            NetworkEvent::FileTransferFailed {
                transfer_id,
                peer_id,
                error: error_msg,
            } => {
                warn!(
                    transfer_id = %transfer_id,
                    peer_id = %peer_id,
                    error = %error_msg,
                    "File transfer failed"
                );
                fail_transfer(
                    &self.lifecycle,
                    &transfer_id,
                    &peer_id,
                    classify_failure_reason(&error_msg),
                    Some(error_msg),
                )
                .await;
                self.lifecycle.outbound_entry_cache.remove(&transfer_id);
            }
            // All other network events (PeerDiscovered, PeerLost, PeerReady, etc.)
            // are handled by PeerDiscoveryWorker and PeerMonitor
            _ => {}
        }
    }
}

/// Best-effort mapping of a free-text failure string to a typed reason.
///
/// The string originates from several unrelated code sites (I/O errors,
/// protocol-layer rejection strings, timeout sweep output, etc.). The
/// mapping here captures the well-known prefixes produced by the daemon and
/// platform layers; any unfamiliar shape falls back to `Unknown`.
fn classify_failure_reason(message: &str) -> FileTransferFailureReason {
    let lowered = message.to_ascii_lowercase();
    if lowered.contains("timeout") {
        FileTransferFailureReason::TimedOut
    } else if lowered.contains("failed to read file") || lowered.contains("storage") {
        FileTransferFailureReason::StorageUnavailable
    } else if lowered.contains("hash") || lowered.contains("integrity") {
        FileTransferFailureReason::IntegrityCheckFailed
    } else if lowered.contains("rejected") || lowered.contains("access") {
        FileTransferFailureReason::AccessDenied
    } else if lowered.contains("network") || lowered.contains("closed") {
        FileTransferFailureReason::NetworkUnavailable
    } else {
        FileTransferFailureReason::Unknown
    }
}

async fn fail_transfer(
    lifecycle: &Arc<FileTransferLifecycle>,
    transfer_id: &str,
    peer_id: &str,
    reason: FileTransferFailureReason,
    detail: Option<String>,
) {
    if let Err(err) = lifecycle
        .fail
        .execute(FailTransfer {
            transfer_id: transfer_id.to_string(),
            peer_id: peer_id.to_string(),
            reason,
            detail,
        })
        .await
    {
        warn!(
            error = %err,
            transfer_id = %transfer_id,
            "Failed to record transfer failure"
        );
    }
}

/// Restore file(s) to OS clipboard after successful inbound transfer.
///
/// Canonicalizes paths to absolute paths, then delegates guard-registration + write
/// to the ClipboardWriteCoordinator with LocalRestore intent.
#[instrument(
    name = "inbound_file_sync.restore_to_clipboard",
    level = "info",
    skip(file_paths, coordinator),
    fields(file_count = file_paths.len())
)]
async fn restore_file_to_clipboard_after_transfer(
    file_paths: Vec<PathBuf>,
    coordinator: &Arc<ClipboardWriteCoordinator>,
) {
    use uc_app::usecases::file_sync::copy_file_to_clipboard::{
        build_file_snapshot, build_path_list,
    };

    // Canonicalize paths to absolute paths.
    // The clipboard (CF_HDROP on Windows, NSPasteboard on macOS) requires absolute
    // paths; relative paths won't resolve when pasting.
    let file_paths: Vec<PathBuf> = file_paths
        .into_iter()
        .map(|p| {
            if p.is_relative() {
                match p.canonicalize() {
                    Ok(abs) => abs,
                    Err(err) => {
                        warn!(
                            path = %p.display(),
                            error = %err,
                            "Failed to canonicalize relative file path, using as-is"
                        );
                        p
                    }
                }
            } else {
                p
            }
        })
        .collect();

    // Verify all files exist before attempting clipboard write
    let files_exist: Vec<bool> = file_paths.iter().map(|p| p.exists()).collect();
    let all_exist = files_exist.iter().all(|&e| e);
    if !all_exist {
        warn!(
            paths = ?file_paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
            files_exist = ?files_exist,
            "Some files do not exist on disk — clipboard write will likely fail"
        );
    }

    let path_list = build_path_list(&file_paths);
    let snapshot = build_file_snapshot(&path_list);

    // FCLIP-03: Check for genuinely concurrent clipboard write operations.
    // Uses is_write_in_progress() which only returns true while another write()
    // call is actively executing — not merely because attribution guards from
    // a previous completed write are still within their TTL window.
    if coordinator.is_write_in_progress() {
        warn!(
            file_count = file_paths.len(),
            "Concurrent clipboard write in progress, skipping auto-restore. Files available in Dashboard."
        );
        return;
    }

    // Restore to system clipboard via coordinator (handles guard + write + cleanup-on-error)
    if let Err(err) = coordinator
        .write(snapshot, ClipboardWriteIntent::LocalRestore)
        .await
    {
        warn!(error = %err, "Failed to write file URIs to system clipboard");
    }
}
