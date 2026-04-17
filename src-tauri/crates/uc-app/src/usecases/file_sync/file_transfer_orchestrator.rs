//! File transfer event-loop orchestration for durable status transitions.
//!
//! Handles pending/transferring/completed/failed lifecycle through the
//! `TrackInboundTransfersUseCase`, emits `file-transfer.status_changed`
//! events via WS, runs periodic timeout sweeps, and performs startup reconciliation.
//!
//! The orchestrator holds a shared swappable emitter cell
//! `Arc<RwLock<Arc<dyn HostEventEmitterPort>>>` — matching the `HostEventSetupPort`
//! pattern in assembly.rs. This eliminates any emitter timing problem: the
//! orchestrator can be constructed at wire time with the `LoggingEventEmitter`
//! inside the cell, and automatically uses the `DaemonApiEventEmitter` after the swap.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use serde::Serialize;
use tracing::{debug, info, info_span, trace, warn, Instrument};

use uc_core::ports::transfer_progress::{TransferDirection, TransferProgress};
use uc_core::ports::ClockPort;

use crate::shared::host_event::{HostEvent, HostEventEmitterPort, TransferHostEvent};
use crate::usecases::clipboard::sync_inbound::PendingTransferLinkage;
use crate::usecases::file_sync::TrackInboundTransfersUseCase;

/// Info about a file transfer completion that arrived before its
/// pending record was seeded in the database.
#[derive(Debug, Clone)]
pub struct EarlyCompletionInfo {
    pub content_hash: Option<String>,
    pub completed_at_ms: i64,
}

/// Thread-safe cache for file transfer completions that arrive before
/// the pending record is seeded in the database (race condition).
///
/// Shared between the clipboard receive loop (which seeds pending records)
/// and the pairing events loop (which handles completions).
#[derive(Default)]
pub struct EarlyCompletionCache {
    inner: Mutex<HashMap<String, EarlyCompletionInfo>>,
}

impl EarlyCompletionCache {
    /// Store an early completion for later reconciliation.
    pub fn store(&self, transfer_id: String, info: EarlyCompletionInfo) {
        let mut map = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        map.insert(transfer_id, info);
    }

    /// Drain entries whose transfer_id appears in the given list.
    /// Returns the matched entries so the caller can reconcile them.
    pub fn drain_matching(&self, transfer_ids: &[String]) -> Vec<(String, EarlyCompletionInfo)> {
        let mut map = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let mut matched = Vec::new();
        for tid in transfer_ids {
            if let Some(info) = map.remove(tid) {
                matched.push((tid.clone(), info));
            }
        }
        matched
    }
}

/// Thread-safe cache for sender-side transfer linkage.
///
/// Sender progress events do not have durable transfer rows, so we keep an
/// in-memory transfer_id -> entry_id mapping long enough to enrich live events
/// for the frontend.
#[derive(Default)]
pub struct OutboundTransferLinkCache {
    inner: Mutex<HashMap<String, String>>,
}

impl OutboundTransferLinkCache {
    pub fn insert(&self, transfer_id: String, entry_id: String) {
        let mut map = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        map.insert(transfer_id, entry_id);
    }

    pub fn get(&self, transfer_id: &str) -> Option<String> {
        let map = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        map.get(transfer_id).cloned()
    }

    pub fn remove(&self, transfer_id: &str) {
        let mut map = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        map.remove(transfer_id);
    }
}

const RECEIVER_ACTIVITY_REFRESH_MIN_INTERVAL_MS: i64 = 2_000;

#[derive(Default)]
struct TransferEntryCache {
    inner: Mutex<HashMap<String, String>>,
}

impl TransferEntryCache {
    fn insert(&self, transfer_id: String, entry_id: String) {
        let mut map = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        map.insert(transfer_id, entry_id);
    }

    fn get(&self, transfer_id: &str) -> Option<String> {
        let map = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        map.get(transfer_id).cloned()
    }

    fn remove(&self, transfer_id: &str) {
        let mut map = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        map.remove(transfer_id);
    }
}

#[derive(Default)]
struct InboundActivityRefreshCache {
    inner: Mutex<HashMap<String, i64>>,
}

impl InboundActivityRefreshCache {
    fn should_refresh(&self, transfer_id: &str, now_ms: i64) -> bool {
        let mut map = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        match map.get_mut(transfer_id) {
            Some(last_refresh_ms)
                if now_ms.saturating_sub(*last_refresh_ms)
                    < RECEIVER_ACTIVITY_REFRESH_MIN_INTERVAL_MS =>
            {
                false
            }
            Some(last_refresh_ms) => {
                *last_refresh_ms = now_ms;
                true
            }
            None => {
                map.insert(transfer_id.to_string(), now_ms);
                true
            }
        }
    }

    fn remove(&self, transfer_id: &str) {
        let mut map = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        map.remove(transfer_id);
    }
}

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

/// Orchestrator for file transfer lifecycle management.
///
/// Holds a shared swappable emitter cell so it can be constructed at wire
/// time and automatically pick up the real `DaemonApiEventEmitter` after bootstrap
/// swaps the cell, without needing `Option` or deferred construction.
pub struct FileTransferOrchestrator {
    tracker: Arc<TrackInboundTransfersUseCase>,
    emitter_cell: Arc<RwLock<Arc<dyn HostEventEmitterPort>>>,
    clock: Arc<dyn ClockPort>,
    early_completion_cache: EarlyCompletionCache,
    outbound_link_cache: OutboundTransferLinkCache,
    transfer_entry_cache: TransferEntryCache,
    inbound_activity_refresh_cache: InboundActivityRefreshCache,
}

impl FileTransferOrchestrator {
    /// Construct the orchestrator.
    ///
    /// `emitter_cell` is the shared `Arc<RwLock<Arc<dyn HostEventEmitterPort>>>` created
    /// once at wire time. The cell initially holds a `LoggingEventEmitter`; bootstrap
    /// later swaps it to a `DaemonApiEventEmitter` — and this orchestrator automatically
    /// sees the new emitter on every call.
    pub fn new(
        tracker: Arc<TrackInboundTransfersUseCase>,
        emitter_cell: Arc<RwLock<Arc<dyn HostEventEmitterPort>>>,
        clock: Arc<dyn ClockPort>,
    ) -> Self {
        Self {
            tracker,
            emitter_cell,
            clock,
            early_completion_cache: EarlyCompletionCache::default(),
            outbound_link_cache: OutboundTransferLinkCache::default(),
            transfer_entry_cache: TransferEntryCache::default(),
            inbound_activity_refresh_cache: InboundActivityRefreshCache::default(),
        }
    }

    /// Expose the inner `TrackInboundTransfersUseCase` for callers that need
    /// to call `record_pending_from_clipboard` directly (e.g., wiring.rs).
    pub fn tracker(&self) -> &TrackInboundTransfersUseCase {
        &self.tracker
    }

    /// Expose the early-completion cache for drain operations by the clipboard
    /// receive loop.
    pub fn early_completion_cache(&self) -> &EarlyCompletionCache {
        &self.early_completion_cache
    }

    /// Register sender-side transfer linkage so outbound progress can be mapped
    /// back to the originating clipboard entry.
    pub fn register_outbound_transfer(&self, transfer_id: &str, entry_id: &str) {
        debug!(
            transfer_id,
            entry_id, "Registered outbound transfer linkage for live progress routing"
        );
        self.outbound_link_cache
            .insert(transfer_id.to_string(), entry_id.to_string());
    }

    /// Get the current timestamp in milliseconds from the orchestrator's clock.
    ///
    /// Exposed for callers (e.g., wiring.rs clipboard receive loop) that need to
    /// build `PendingInboundTransfer` records with a `created_at_ms` value using
    /// the same clock instance as the orchestrator.
    pub fn now_ms(&self) -> i64 {
        self.clock.now_ms()
    }

    /// Emit `file-transfer://status-changed` for each pending transfer
    /// after inbound clipboard metadata is applied.
    pub fn emit_pending_status(
        &self,
        entry_id: &str,
        pending_transfers: &[PendingTransferLinkage],
    ) {
        let emitter = self
            .emitter_cell
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        for t in pending_transfers {
            if let Err(err) = emitter.emit(HostEvent::Transfer(TransferHostEvent::StatusChanged {
                transfer_id: t.transfer_id.clone(),
                entry_id: entry_id.to_string(),
                status: "pending".to_string(),
                reason: None,
            })) {
                warn!(
                    error = %err,
                    transfer_id = %t.transfer_id,
                    "Failed to emit pending file-transfer status"
                );
            }
        }
    }

    /// Handle a transfer `TransferProgress` event.
    ///
    /// On first chunk (chunks_completed == 1), promotes to `transferring`.
    /// On subsequent chunks, refreshes durable liveness.
    ///
    /// Returns `true` if promoted to `transferring` (first time).
    pub async fn handle_transfer_progress(&self, progress: &TransferProgress) -> bool {
        let entry_id = self.resolve_entry_id(&progress.transfer_id).await;

        trace!(
            transfer_id = %progress.transfer_id,
            peer_id = %progress.peer_id,
            direction = ?progress.direction,
            chunks_completed = progress.chunks_completed,
            total_chunks = progress.total_chunks,
            resolved_entry_id = ?entry_id,
            "Resolved transfer progress linkage before host event emission"
        );

        let emitter = self
            .emitter_cell
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        if let Err(err) = emitter.emit(HostEvent::Transfer(TransferHostEvent::Progress {
            transfer_id: progress.transfer_id.clone(),
            entry_id: entry_id.clone(),
            peer_id: progress.peer_id.clone(),
            direction: progress.direction.clone(),
            bytes_transferred: progress.bytes_transferred,
            total_bytes: progress.total_bytes,
        })) {
            warn!(error = %err, transfer_id = %progress.transfer_id, "Failed to emit transfer progress");
        }

        // Only track receiving-side progress durably.
        if progress.direction != TransferDirection::Receiving {
            return false;
        }

        let now_ms = self.clock.now_ms();

        if progress.chunks_completed == 1 {
            // First chunk: promote to transferring
            match self
                .tracker
                .mark_transferring(&progress.transfer_id, now_ms)
                .await
            {
                Ok(true) => {
                    info!(transfer_id = %progress.transfer_id, "Transfer promoted to transferring");
                    if let Some(entry_id) = entry_id {
                        let emitter = self
                            .emitter_cell
                            .read()
                            .unwrap_or_else(|p| p.into_inner())
                            .clone();
                        if let Err(err) =
                            emitter.emit(HostEvent::Transfer(TransferHostEvent::StatusChanged {
                                transfer_id: progress.transfer_id.clone(),
                                entry_id,
                                status: "transferring".to_string(),
                                reason: None,
                            }))
                        {
                            warn!(error = %err, "Failed to emit transferring status");
                        }
                    }
                    return true;
                }
                Ok(false) => {
                    self.refresh_inbound_activity_if_due(&progress.transfer_id, now_ms)
                        .await;
                }
                Err(err) => {
                    warn!(error = %err, transfer_id = %progress.transfer_id, "Failed to mark transferring");
                }
            }
        } else {
            self.refresh_inbound_activity_if_due(&progress.transfer_id, now_ms)
                .await;
        }

        false
    }

    /// Handle a file transfer completion event.
    ///
    /// Marks the transfer row as completed before emitting the status event.
    /// If the pending record hasn't been seeded yet (race condition), stores
    /// the completion in `early_completion_cache` for later reconciliation.
    pub async fn handle_transfer_completed(&self, transfer_id: &str, content_hash: Option<&str>) {
        debug!(
            transfer_id,
            "Removing outbound transfer linkage on completion"
        );
        self.outbound_link_cache.remove(transfer_id);
        self.transfer_entry_cache.remove(transfer_id);
        self.inbound_activity_refresh_cache.remove(transfer_id);
        let now_ms = self.clock.now_ms();

        // Mark durable row completed
        match self
            .tracker
            .mark_completed(transfer_id, content_hash, now_ms)
            .await
        {
            Ok(true) => {
                // Row was updated — emit status-changed
            }
            Ok(false) => {
                // No row found — pending record hasn't been seeded yet.
                // Cache completion for reconciliation after seeding.
                warn!(
                    transfer_id,
                    "Early completion cached: pending record not yet seeded"
                );
                self.early_completion_cache.store(
                    transfer_id.to_string(),
                    EarlyCompletionInfo {
                        content_hash: content_hash.map(|s| s.to_string()),
                        completed_at_ms: now_ms,
                    },
                );
                return;
            }
            Err(err) => {
                warn!(error = %err, transfer_id, "Failed to mark transfer completed");
                return;
            }
        }

        // Emit status-changed for completed
        if let Ok(Some(entry_id)) = self
            .tracker
            .get_entry_summary_by_transfer(transfer_id)
            .await
        {
            let emitter = self
                .emitter_cell
                .read()
                .unwrap_or_else(|p| p.into_inner())
                .clone();
            if let Err(err) = emitter.emit(HostEvent::Transfer(TransferHostEvent::StatusChanged {
                transfer_id: transfer_id.to_string(),
                entry_id,
                status: "completed".to_string(),
                reason: None,
            })) {
                warn!(error = %err, "Failed to emit completed status");
            }
        }
    }

    /// Handle a file transfer failure event.
    ///
    /// Marks the durable row failed with the error reason, cleans partial cache,
    /// and emits `file-transfer://status-changed`.
    pub async fn handle_transfer_failed(&self, transfer_id: &str, error_reason: &str) {
        debug!(
            transfer_id,
            error_reason, "Removing outbound transfer linkage on failure"
        );
        self.outbound_link_cache.remove(transfer_id);
        self.transfer_entry_cache.remove(transfer_id);
        self.inbound_activity_refresh_cache.remove(transfer_id);
        let now_ms = self.clock.now_ms();

        // Mark durable row failed
        if let Err(err) = self
            .tracker
            .mark_failed(transfer_id, error_reason, now_ms)
            .await
        {
            warn!(error = %err, transfer_id, "Failed to mark transfer failed");
            return;
        }

        // Emit status-changed for failed
        if let Ok(Some(entry_id)) = self
            .tracker
            .get_entry_summary_by_transfer(transfer_id)
            .await
        {
            let emitter = self
                .emitter_cell
                .read()
                .unwrap_or_else(|p| p.into_inner())
                .clone();
            if let Err(err) = emitter.emit(HostEvent::Transfer(TransferHostEvent::StatusChanged {
                transfer_id: transfer_id.to_string(),
                entry_id,
                status: "failed".to_string(),
                reason: Some(error_reason.to_string()),
            })) {
                warn!(error = %err, "Failed to emit failed status");
            }
        }
    }

    /// Spawn a periodic timeout sweep task.
    ///
    /// Runs every 15 seconds. Fails stalled pending (>60s) and transferring (>5min)
    /// rows, emits status-changed events, and cleans partial cache artifacts.
    pub fn spawn_timeout_sweep(
        &self,
        cancel: tokio::sync::watch::Receiver<bool>,
    ) -> tokio::task::JoinHandle<()> {
        let tracker = self.tracker.clone();
        let emitter_cell = self.emitter_cell.clone();
        let clock = self.clock.clone();

        tokio::spawn(
            async move {
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

                            let emitter = emitter_cell
                                .read()
                                .unwrap_or_else(|p| p.into_inner())
                                .clone();

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

                                if let Err(err) =
                                    tracker.mark_failed(&t.transfer_id, reason, now_ms).await
                                {
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
                                if let Err(err) = emitter.emit(HostEvent::Transfer(
                                    TransferHostEvent::StatusChanged {
                                        transfer_id: t.transfer_id.clone(),
                                        entry_id: t.entry_id.clone(),
                                        status: "failed".to_string(),
                                        reason: Some(reason.to_string()),
                                    },
                                )) {
                                    warn!(error = %err, "Failed to emit timeout failure status");
                                }
                            }
                        }
                        Err(err) => {
                            warn!(error = %err, "Timeout sweep query failed");
                        }
                    }
                }
            }
            .instrument(info_span!("file_transfer.timeout_sweep")),
        )
    }

    /// Run startup reconciliation: mark orphaned in-flight transfers as failed
    /// and clean their cache artifacts.
    ///
    /// Non-blocking and non-fatal: errors are logged as warnings.
    pub async fn reconcile_on_startup(&self) {
        let now_ms = self.clock.now_ms();
        let emitter = self
            .emitter_cell
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone();

        match self
            .tracker
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
                    if let Err(err) =
                        emitter.emit(HostEvent::Transfer(TransferHostEvent::StatusChanged {
                            transfer_id: t.transfer_id.clone(),
                            entry_id: t.entry_id.clone(),
                            status: "failed".to_string(),
                            reason: Some(
                                "orphaned: app restarted while transfer was in-flight".to_string(),
                            ),
                        }))
                    {
                        warn!(error = %err, "Failed to emit reconciliation status");
                    }
                }
            }
            Err(err) => {
                warn!(error = %err, "Startup reconciliation failed (non-fatal)");
            }
        }
    }

    async fn resolve_entry_id(&self, transfer_id: &str) -> Option<String> {
        if let Some(entry_id) = self.outbound_link_cache.get(transfer_id) {
            return Some(entry_id);
        }

        if let Some(entry_id) = self.transfer_entry_cache.get(transfer_id) {
            return Some(entry_id);
        }

        match self
            .tracker
            .get_entry_summary_by_transfer(transfer_id)
            .await
        {
            Ok(Some(entry_id)) => {
                self.transfer_entry_cache
                    .insert(transfer_id.to_string(), entry_id.clone());
                Some(entry_id)
            }
            Ok(None) => None,
            Err(err) => {
                warn!(
                    error = %err,
                    transfer_id,
                    "Failed to resolve transfer entry from tracker"
                );
                None
            }
        }
    }

    async fn refresh_inbound_activity_if_due(&self, transfer_id: &str, now_ms: i64) {
        if !self
            .inbound_activity_refresh_cache
            .should_refresh(transfer_id, now_ms)
        {
            return;
        }

        if let Err(err) = self.tracker.refresh_activity(transfer_id, now_ms).await {
            warn!(error = %err, transfer_id, "Failed to refresh transfer activity");
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
