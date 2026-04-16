//! Use case for tracking receiver-side file transfer lifecycle.
//!
//! Orchestrates state transitions through `FileTransferRepositoryPort`
//! without touching Diesel, Tauri, or filesystem implementation details.

use std::sync::Arc;

use anyhow::Result;
use tracing::{debug_span, info, info_span, warn, Instrument};
use uc_core::ports::file_transfer_repository::{
    EntryTransferSummary, ExpiredInflightTransfer, FileTransferRepositoryPort,
    PendingInboundTransfer,
};

/// Timeout budget: pending transfers fail after 60 seconds without first chunk.
pub const PENDING_TIMEOUT_MS: i64 = 60_000;

/// Timeout budget: transferring transfers fail after 5 minutes without new chunk activity.
pub const TRANSFERRING_TIMEOUT_MS: i64 = 300_000;

/// App-layer use case for tracking inbound file transfer state transitions.
///
/// Free of Diesel, Tauri, and filesystem implementation details.
pub struct TrackInboundTransfersUseCase {
    repo: Arc<dyn FileTransferRepositoryPort>,
}

impl TrackInboundTransfersUseCase {
    pub fn new(repo: Arc<dyn FileTransferRepositoryPort>) -> Self {
        Self { repo }
    }

    /// Seed pending transfer records from clipboard metadata.
    ///
    /// Called by `SyncInboundClipboardUseCase` after a file-backed entry is persisted.
    pub async fn record_pending_from_clipboard(
        &self,
        transfers: Vec<PendingInboundTransfer>,
    ) -> Result<()> {
        if transfers.is_empty() {
            return Ok(());
        }

        let count = transfers.len();
        async {
            self.repo.insert_pending_transfers(&transfers).await?;
            info!(
                count,
                "Seeded pending transfer records from clipboard metadata"
            );
            Ok(())
        }
        .instrument(info_span!("track_inbound.record_pending", count))
        .await
    }

    /// Promote a transfer to `transferring` on first data chunk.
    pub async fn mark_transferring(&self, transfer_id: &str, now_ms: i64) -> Result<bool> {
        self.repo
            .mark_transferring(transfer_id, now_ms)
            .instrument(info_span!("track_inbound.mark_transferring", transfer_id))
            .await
    }

    /// Refresh liveness timestamp on subsequent progress events.
    pub async fn refresh_activity(&self, transfer_id: &str, now_ms: i64) -> Result<()> {
        self.repo
            .refresh_activity(transfer_id, now_ms)
            .instrument(info_span!("track_inbound.refresh_activity", transfer_id))
            .await
    }

    /// Mark a transfer as completed.
    ///
    /// Returns `true` if a row was actually updated, `false` if the pending
    /// record hasn't been seeded yet (race condition).
    pub async fn mark_completed(
        &self,
        transfer_id: &str,
        content_hash: Option<&str>,
        now_ms: i64,
    ) -> Result<bool> {
        self.repo
            .mark_completed(transfer_id, content_hash, now_ms)
            .instrument(info_span!("track_inbound.mark_completed", transfer_id))
            .await
    }

    /// Mark a transfer as failed with a reason.
    pub async fn mark_failed(&self, transfer_id: &str, reason: &str, now_ms: i64) -> Result<()> {
        self.repo
            .mark_failed(transfer_id, reason, now_ms)
            .instrument(info_span!("track_inbound.mark_failed", transfer_id))
            .await
    }

    /// List expired in-flight transfers for timeout sweep.
    ///
    /// Uses the locked timeout budgets:
    /// - pending: `PENDING_TIMEOUT_MS` (60s)
    /// - transferring: `TRANSFERRING_TIMEOUT_MS` (5min)
    pub async fn list_expired_inflight(&self, now_ms: i64) -> Result<Vec<ExpiredInflightTransfer>> {
        let pending_cutoff = now_ms - PENDING_TIMEOUT_MS;
        let transferring_cutoff = now_ms - TRANSFERRING_TIMEOUT_MS;

        self.repo
            .list_expired_inflight(pending_cutoff, transferring_cutoff)
            .instrument(debug_span!("track_inbound.list_expired_inflight"))
            .await
    }

    /// Startup reconciliation: bulk-fail all in-flight transfers and return cleanup targets.
    ///
    /// Returns expired transfer records whose `cached_path` the platform layer
    /// can use to delete partial downloads.
    pub async fn reconcile_inflight_after_startup(
        &self,
        now_ms: i64,
    ) -> Result<Vec<ExpiredInflightTransfer>> {
        let reason = "orphaned: app restarted while transfer was in-flight";

        let cleanup_targets = self
            .repo
            .bulk_fail_inflight(reason, now_ms)
            .instrument(info_span!("track_inbound.reconcile_startup"))
            .await?;

        if !cleanup_targets.is_empty() {
            warn!(
                count = cleanup_targets.len(),
                "Reconciled in-flight transfers after startup — marked as failed"
            );
        }

        Ok(cleanup_targets)
    }

    /// Get aggregate transfer summary for an entry (for projections).
    pub async fn get_entry_summary(&self, entry_id: &str) -> Result<Option<EntryTransferSummary>> {
        self.repo.get_entry_transfer_summary(entry_id).await
    }

    /// Look up the entry_id for a given transfer_id.
    ///
    /// Used by platform wiring when only transfer_id is available (e.g., progress events).
    pub async fn get_entry_summary_by_transfer(&self, transfer_id: &str) -> Result<Option<String>> {
        self.repo.get_entry_id_for_transfer(transfer_id).await
    }
}
