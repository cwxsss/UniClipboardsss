//! Transfer progress reporting port.
//!
//! Provides progress tracking for chunked clipboard transfers,
//! enabling the frontend to display transfer progress UI.

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Direction of a clipboard transfer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TransferDirection {
    Sending,
    Receiving,
}

/// Progress of an ongoing clipboard transfer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferProgress {
    pub transfer_id: String,
    pub peer_id: String,
    pub direction: TransferDirection,
    pub chunks_completed: u32,
    pub total_chunks: u32,
    pub bytes_transferred: u64,
    /// Total bytes for this transfer, or `None` if unknown (e.g. receiving side).
    pub total_bytes: Option<u64>,
}

/// Port for reporting transfer progress events.
#[async_trait]
pub trait TransferProgressPort: Send + Sync {
    /// Report progress of an active transfer.
    async fn report_progress(&self, progress: TransferProgress) -> Result<()>;
}

/// No-op implementation of `TransferProgressPort` for tests and default usage.
pub struct NoopTransferProgressPort;

#[async_trait]
impl TransferProgressPort for NoopTransferProgressPort {
    async fn report_progress(&self, _progress: TransferProgress) -> Result<()> {
        Ok(())
    }
}
