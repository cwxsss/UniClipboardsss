//! File sync orchestrator worker for the daemon.
//!
//! Slice4 P5c: 旧的 `FileTransferEventInboundPort` 已退役,事件循环本身
//! 失去了上游(libp2p adapter 删除后,iroh 侧通过 `FileTransferEventPublisherPort`
//! 直接写 store/lifecycle,不再经此 worker)。本 worker 现在只承担两件事:
//!
//! 1. 启动期 reconcile —— 把进程崩溃留下的 in-flight transfer 标记为 failed
//! 2. 周期性 sweep —— 把超时的 pending/transferring 状态收口
//!
//! 事件路径下沉到 `FileTransferEventStore` + iroh blob handler 直接消费;
//! 这里的 `handle_event` / `handle_*` 私有方法保留为内部辅助,等 iroh 侧
//! 接入 daemon 事件分发时(`vNext`)再复用。

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
use uc_core::file_transfer::{FileTransferEvent, FileTransferFailureReason};
use uc_core::ports::file_transfer_repository::FileTransferRepositoryPort;
use uc_core::ports::SettingsPort;

use crate::service::{DaemonService, ServiceHealth};

pub struct FileSyncOrchestratorWorker {
    lifecycle: Arc<FileTransferLifecycle>,
    #[allow(dead_code)]
    file_transfer_repo: Arc<dyn FileTransferRepositoryPort>,
    #[allow(dead_code)]
    clipboard_write_coordinator: Arc<ClipboardWriteCoordinator>,
    #[allow(dead_code)]
    file_cache_dir: PathBuf,
    #[allow(dead_code)]
    settings: Arc<dyn SettingsPort>,
}

impl FileSyncOrchestratorWorker {
    pub fn new(
        lifecycle: Arc<FileTransferLifecycle>,
        file_transfer_repo: Arc<dyn FileTransferRepositoryPort>,
        clipboard_write_coordinator: Arc<ClipboardWriteCoordinator>,
        file_cache_dir: PathBuf,
        settings: Arc<dyn SettingsPort>,
    ) -> Self {
        Self {
            lifecycle,
            file_transfer_repo,
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

        // 3. 等取消 —— 旧的 inbound 事件循环已下线,iroh 侧改走
        //    `FileTransferEventPublisherPort` 直接写 store/lifecycle。
        cancel.cancelled().await;
        let _ = sweep_cancel_tx.send(true);
        info!("file sync orchestrator cancelled");
        Ok(())
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
    #[allow(dead_code)]
    async fn handle_event(&self, event: FileTransferEvent) {
        match event {
            FileTransferEvent::Started {
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
            FileTransferEvent::Progress {
                transfer_id,
                peer_id,
                progress,
            } => {
                if let Err(err) = self
                    .lifecycle
                    .report_progress
                    .execute(ReportTransferProgress {
                        transfer_id: transfer_id.clone(),
                        peer_id,
                        progress,
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
            FileTransferEvent::Completed {
                transfer_id,
                peer_id,
            } => {
                self.handle_completed(transfer_id, peer_id).await;
            }
            FileTransferEvent::Failed {
                transfer_id,
                peer_id,
                reason,
                detail,
            } => {
                warn!(
                    transfer_id = %transfer_id,
                    peer_id = %peer_id,
                    reason = ?reason,
                    detail = ?detail,
                    "File transfer failed"
                );
                fail_transfer(&self.lifecycle, &transfer_id, &peer_id, reason, detail).await;
                self.lifecycle.outbound_entry_cache.remove(&transfer_id);
            }
            FileTransferEvent::Cancelled {
                transfer_id,
                peer_id,
                reason,
            } => {
                info!(
                    transfer_id = %transfer_id,
                    peer_id = %peer_id,
                    reason = ?reason,
                    "File transfer cancelled"
                );
                // No adapter currently emits Cancelled; the cancel use case is
                // wired but not invoked from this event loop yet. Route through
                // the fail path with a typed detail so the projection still
                // settles, mirroring the deprecated Cancelled → Failed fold on
                // the receiver projection.
                fail_transfer(
                    &self.lifecycle,
                    &transfer_id,
                    &peer_id,
                    FileTransferFailureReason::Unknown,
                    Some(format!("cancelled: {reason:?}")),
                )
                .await;
                self.lifecycle.outbound_entry_cache.remove(&transfer_id);
            }
        }
    }

    async fn handle_completed(&self, transfer_id: String, peer_id: String) {
        let tracked = match self.file_transfer_repo.get_transfer(&transfer_id).await {
            Ok(Some(row)) => row,
            Ok(None) => {
                // No receiver projection row — typically a sender-side transfer
                // whose Completed event still needs to advance the event store.
                if let Err(err) = self
                    .lifecycle
                    .complete
                    .execute(CompleteTransfer {
                        transfer_id: transfer_id.clone(),
                        peer_id: peer_id.clone(),
                    })
                    .await
                {
                    warn!(
                        error = %err,
                        transfer_id = %transfer_id,
                        "Failed to record sender-side transfer completion"
                    );
                }
                self.lifecycle.outbound_entry_cache.remove(&transfer_id);
                return;
            }
            Err(err) => {
                error!(
                    error = %err,
                    transfer_id = %transfer_id,
                    "Failed to look up transfer projection for completion"
                );
                return;
            }
        };

        debug!(
            transfer_id = %transfer_id,
            peer_id = %peer_id,
            filename = %tracked.filename,
            cached_path = %tracked.cached_path,
            "File transfer completed, processing inbound file"
        );

        let inbound_uc =
            SyncInboundFileUseCase::new(self.settings.clone(), self.file_cache_dir.clone());

        let lifecycle = Arc::clone(&self.lifecycle);
        let coordinator = self.clipboard_write_coordinator.clone();
        let file_path = PathBuf::from(&tracked.cached_path);
        let transfer_id_for_spawn = transfer_id.clone();
        let peer_id_for_spawn = peer_id.clone();
        let span_tid = transfer_id.clone();

        tokio::spawn(
            async move {
                let file_bytes = match tokio::fs::read(&file_path).await {
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
                    .handle_transfer_complete(&transfer_id_for_spawn, &file_path, &expected_hash)
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

                        lifecycle.outbound_entry_cache.remove(&result.transfer_id);

                        restore_file_to_clipboard_after_transfer(
                            vec![result.file_path],
                            &coordinator,
                        )
                        .await;
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
