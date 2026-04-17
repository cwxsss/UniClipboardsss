//! Host-event adapter for file-transfer domain events.
//!
//! Translates `FileTransferEvent` (domain truth) into `TransferHostEvent`
//! (UI broadcast contract) and emits through `HostEventEmitterPort`.
//!
//! `entry_id` is looked up through `FileTransferRepositoryPort` because the
//! receiver-side projection owns that binding and the domain event itself
//! intentionally does not carry it.

use std::sync::{Arc, RwLock};

use anyhow::Result;
use async_trait::async_trait;
use tracing::warn;

use uc_core::file_transfer::{
    FileTransferCancellationReason, FileTransferEvent, FileTransferEventPublisherPort,
    FileTransferFailureReason,
};
use uc_core::ports::file_transfer_repository::FileTransferRepositoryPort;

use crate::shared::host_event::{HostEvent, HostEventEmitterPort, TransferHostEvent};
use crate::shared::outbound_entry_cache::OutboundEntryIdCache;

pub struct FileTransferHostEventPublisher {
    emitter_cell: Arc<RwLock<Arc<dyn HostEventEmitterPort>>>,
    file_transfer_repo: Arc<dyn FileTransferRepositoryPort>,
    outbound_entry_cache: Arc<OutboundEntryIdCache>,
}

impl FileTransferHostEventPublisher {
    pub fn new(
        emitter_cell: Arc<RwLock<Arc<dyn HostEventEmitterPort>>>,
        file_transfer_repo: Arc<dyn FileTransferRepositoryPort>,
        outbound_entry_cache: Arc<OutboundEntryIdCache>,
    ) -> Self {
        Self {
            emitter_cell,
            file_transfer_repo,
            outbound_entry_cache,
        }
    }

    async fn resolve_entry_id(&self, transfer_id: &str) -> Option<String> {
        // Receiver-side projection has authoritative entry_id once seeded.
        match self
            .file_transfer_repo
            .get_entry_id_for_transfer(transfer_id)
            .await
        {
            Ok(Some(entry_id)) => return Some(entry_id),
            Ok(None) => {
                // Fall through to outbound cache (sender side or pre-seed race).
            }
            Err(err) => {
                warn!(error = %err, transfer_id, "failed to resolve entry_id from projection");
            }
        }

        self.outbound_entry_cache.get(transfer_id)
    }

    fn emit(&self, event: HostEvent) {
        let emitter = self
            .emitter_cell
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        if let Err(err) = emitter.emit(event) {
            warn!(error = %err, "failed to emit file transfer host event");
        }
    }
}

#[async_trait]
impl FileTransferEventPublisherPort for FileTransferHostEventPublisher {
    async fn publish(&self, event: FileTransferEvent) -> Result<()> {
        match event {
            FileTransferEvent::Started { transfer_id, .. } => {
                self.publish_status_change(&transfer_id, "transferring", None, "Started")
                    .await;
            }
            FileTransferEvent::Progress {
                transfer_id,
                peer_id,
                progress,
            } => {
                let entry_id = self.resolve_entry_id(&transfer_id).await;
                self.emit(HostEvent::Transfer(TransferHostEvent::Progress {
                    transfer_id,
                    entry_id,
                    peer_id,
                    direction: progress.direction,
                    bytes_transferred: progress.bytes_transferred,
                    total_bytes: progress.total_bytes,
                }));
            }
            FileTransferEvent::Completed { transfer_id, .. } => {
                self.publish_status_change(&transfer_id, "completed", None, "Completed")
                    .await;
            }
            FileTransferEvent::Failed {
                transfer_id,
                reason,
                detail,
                ..
            } => {
                let reason_label = Some(format_failure_reason(reason, detail.as_deref()));
                self.publish_status_change(&transfer_id, "failed", reason_label, "Failed")
                    .await;
            }
            FileTransferEvent::Cancelled {
                transfer_id,
                reason,
                ..
            } => {
                let reason_label = Some(cancellation_reason_label(reason).to_string());
                self.publish_status_change(&transfer_id, "failed", reason_label, "Cancelled")
                    .await;
            }
        }
        Ok(())
    }
}

impl FileTransferHostEventPublisher {
    async fn publish_status_change(
        &self,
        transfer_id: &str,
        status: &str,
        reason: Option<String>,
        event_kind: &'static str,
    ) {
        let Some(entry_id) = self.resolve_entry_id(transfer_id).await else {
            warn!(
                transfer_id,
                event_kind, "no entry_id resolved; skipping host status event"
            );
            return;
        };
        self.emit(HostEvent::Transfer(TransferHostEvent::StatusChanged {
            transfer_id: transfer_id.to_string(),
            entry_id,
            status: status.to_string(),
            reason,
        }));
    }
}

fn failure_reason_label(reason: FileTransferFailureReason) -> &'static str {
    match reason {
        FileTransferFailureReason::NetworkUnavailable => "network_unavailable",
        FileTransferFailureReason::TimedOut => "timed_out",
        FileTransferFailureReason::AccessDenied => "access_denied",
        FileTransferFailureReason::StorageUnavailable => "storage_unavailable",
        FileTransferFailureReason::IntegrityCheckFailed => "integrity_check_failed",
        FileTransferFailureReason::Unknown => "unknown",
    }
}

/// Compose the final `StatusChanged.reason` string from the typed failure
/// category and its optional free-text detail.
///
/// Output shape: `"{label}"` when no detail, `"{label}: {detail}"` otherwise.
fn format_failure_reason(reason: FileTransferFailureReason, detail: Option<&str>) -> String {
    let label = failure_reason_label(reason);
    match detail.map(str::trim).filter(|s| !s.is_empty()) {
        Some(detail) => format!("{label}: {detail}"),
        None => label.to_string(),
    }
}

fn cancellation_reason_label(reason: FileTransferCancellationReason) -> &'static str {
    match reason {
        FileTransferCancellationReason::LocalUser => "cancelled:local_user",
        FileTransferCancellationReason::RemotePeer => "cancelled:remote_peer",
        FileTransferCancellationReason::Replaced => "cancelled:replaced",
        FileTransferCancellationReason::Unknown => "cancelled:unknown",
    }
}
