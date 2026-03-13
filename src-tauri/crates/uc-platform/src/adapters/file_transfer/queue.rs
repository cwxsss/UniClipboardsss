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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    /// Helper: create a RetryPolicy with no retries for queue-focused tests.
    fn no_retry_policy() -> super::super::retry::RetryPolicy {
        super::super::retry::RetryPolicy {
            max_retries: 0,
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn test_queue_processes_in_order() {
        let processed = Arc::new(Mutex::new(Vec::<String>::new()));
        let processed_clone = processed.clone();

        let queue = FileTransferQueue::spawn(16, no_retry_policy(), move |req| {
            let processed = processed_clone.clone();
            async move {
                processed.lock().await.push(req.transfer_id);
                Ok(())
            }
        });

        for i in 0..3 {
            queue
                .enqueue(FileTransferRequest {
                    peer_id: "peer-1".to_string(),
                    file_path: PathBuf::from(format!("/tmp/file{}.txt", i)),
                    transfer_id: format!("xfer-{}", i),
                    batch_id: None,
                    batch_total: None,
                })
                .await
                .unwrap();
        }

        // Give the queue time to process
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let result = processed.lock().await;
        assert_eq!(*result, vec!["xfer-0", "xfer-1", "xfer-2"]);
    }

    #[tokio::test]
    async fn test_queue_processes_serially() {
        let timestamps = Arc::new(Mutex::new(Vec::<(String, std::time::Instant)>::new()));
        let ts_clone = timestamps.clone();

        let queue = FileTransferQueue::spawn(16, no_retry_policy(), move |req| {
            let ts = ts_clone.clone();
            async move {
                ts.lock().await.push((req.transfer_id.clone(), std::time::Instant::now()));
                // Simulate work
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                ts.lock().await.push((format!("{}-done", req.transfer_id), std::time::Instant::now()));
                Ok(())
            }
        });

        queue
            .enqueue(FileTransferRequest {
                peer_id: "peer-1".to_string(),
                file_path: PathBuf::from("/tmp/a.txt"),
                transfer_id: "first".to_string(),
                batch_id: None,
                batch_total: None,
            })
            .await
            .unwrap();

        queue
            .enqueue(FileTransferRequest {
                peer_id: "peer-1".to_string(),
                file_path: PathBuf::from("/tmp/b.txt"),
                transfer_id: "second".to_string(),
                batch_id: None,
                batch_total: None,
            })
            .await
            .unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        let ts = timestamps.lock().await;
        // "first-done" should appear before "second" starts
        let first_done_idx = ts.iter().position(|(id, _)| id == "first-done").unwrap();
        let second_start_idx = ts.iter().position(|(id, _)| id == "second").unwrap();
        assert!(
            first_done_idx < second_start_idx,
            "second should start after first completes"
        );
    }

    #[tokio::test]
    async fn test_queue_append_during_transfer() {
        let processed = Arc::new(Mutex::new(Vec::<String>::new()));
        let processed_clone = processed.clone();
        let started = Arc::new(tokio::sync::Notify::new());
        let started_clone = started.clone();

        let queue = FileTransferQueue::spawn(16, no_retry_policy(), move |req| {
            let processed = processed_clone.clone();
            let started = started_clone.clone();
            async move {
                if req.transfer_id == "xfer-0" {
                    started.notify_one();
                    // Simulate a long transfer
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
                processed.lock().await.push(req.transfer_id);
                Ok(())
            }
        });

        // Enqueue first item
        queue
            .enqueue(FileTransferRequest {
                peer_id: "peer-1".to_string(),
                file_path: PathBuf::from("/tmp/file0.txt"),
                transfer_id: "xfer-0".to_string(),
                batch_id: None,
                batch_total: None,
            })
            .await
            .unwrap();

        // Wait for first to start processing
        started.notified().await;

        // Append 2 more while first is still in progress
        for i in 1..=2 {
            queue
                .enqueue(FileTransferRequest {
                    peer_id: "peer-1".to_string(),
                    file_path: PathBuf::from(format!("/tmp/file{}.txt", i)),
                    transfer_id: format!("xfer-{}", i),
                    batch_id: None,
                    batch_total: None,
                })
                .await
                .unwrap();
        }

        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        let result = processed.lock().await;
        assert_eq!(*result, vec!["xfer-0", "xfer-1", "xfer-2"]);
    }

    #[test]
    fn test_transfer_error_is_retriable() {
        assert!(TransferError::Network("timeout".to_string()).is_retriable());
        assert!(!TransferError::HashMismatch {
            expected: "a".to_string(),
            actual: "b".to_string(),
        }
        .is_retriable());
        assert!(!TransferError::Rejected("no space".to_string()).is_retriable());
        assert!(!TransferError::FileError("not found".to_string()).is_retriable());
    }
}
