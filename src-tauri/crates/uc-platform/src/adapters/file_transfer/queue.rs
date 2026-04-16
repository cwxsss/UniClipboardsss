//! Serial FIFO transfer queue for file transfers.
//!
//! Processes one transfer at a time; new requests append to tail
//! without interrupting the current active transfer.

use std::path::PathBuf;
use tokio::sync::mpsc;
use tracing::{info, info_span, warn, Instrument};

/// A request to transfer a file to a specific peer.
#[derive(Debug, Clone)]
pub struct FileTransferRequest {
    pub peer_id: String,
    pub file_path: PathBuf,
    pub transfer_id: String,
    pub batch_id: Option<String>,
    pub batch_total: Option<u32>,
}

/// Serial FIFO queue for file transfers.
/// Processes one transfer at a time; new requests append to tail.
pub struct FileTransferQueue {
    tx: mpsc::Sender<FileTransferRequest>,
}

impl FileTransferQueue {
    /// Create a new queue and spawn the processing loop.
    /// Returns the queue handle for enqueueing requests.
    pub fn spawn<F, Fut>(
        buffer_size: usize,
        retry_policy: super::retry::RetryPolicy,
        transfer_fn: F,
    ) -> Self
    where
        F: Fn(FileTransferRequest) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<(), TransferError>> + Send + 'static,
    {
        let (tx, rx) = mpsc::channel(buffer_size);
        tokio::spawn(Self::process_loop(rx, retry_policy, transfer_fn));
        Self { tx }
    }

    /// Enqueue a file transfer request.
    /// Returns immediately -- transfer happens asynchronously.
    pub async fn enqueue(&self, request: FileTransferRequest) -> Result<(), anyhow::Error> {
        self.tx
            .send(request)
            .await
            .map_err(|_| anyhow::anyhow!("File transfer queue closed"))
    }

    async fn process_loop<F, Fut>(
        mut rx: mpsc::Receiver<FileTransferRequest>,
        retry_policy: super::retry::RetryPolicy,
        transfer_fn: F,
    ) where
        F: Fn(FileTransferRequest) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<(), TransferError>> + Send + 'static,
    {
        while let Some(request) = rx.recv().await {
            let span = info_span!(
                "file_transfer.queue.process",
                transfer_id = %request.transfer_id,
                peer_id = %request.peer_id,
            );
            async {
                info!("Processing file transfer: {}", request.transfer_id);
                match retry_policy.execute(|| transfer_fn(request.clone())).await {
                    Ok(()) => {
                        info!("File transfer complete: {}", request.transfer_id);
                    }
                    Err(err) => {
                        warn!(
                            "File transfer failed after retries: {}: {}",
                            request.transfer_id, err
                        );
                    }
                }
            }
            .instrument(span)
            .await;
        }
        info!("File transfer queue shut down");
    }
}

/// Categorized transfer errors for retry decisions.
#[derive(Debug, thiserror::Error)]
pub enum TransferError {
    #[error("network error: {0}")]
    Network(String),
    #[error("hash mismatch: expected {expected}, got {actual}")]
    HashMismatch { expected: String, actual: String },
    #[error("rejected by receiver: {0}")]
    Rejected(String),
    #[error("file error: {0}")]
    FileError(String),
}

impl TransferError {
    /// Whether this error type should trigger a retry.
    pub fn is_retriable(&self) -> bool {
        matches!(self, TransferError::Network(_))
    }
}
