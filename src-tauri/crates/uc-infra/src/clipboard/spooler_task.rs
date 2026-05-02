//! Async spooler task for writing representation bytes to disk.
//! 异步将表示字节写入磁盘缓存的任务。

use std::sync::Arc;

use tokio::sync::mpsc;
use tracing::{debug, error, warn};
use uc_core::ids::RepresentationId;
use uc_core::ports::clipboard::SpoolRequest;

use crate::clipboard::{RepresentationCache, SpoolManager};

/// Background task to write spool requests to disk.
/// 后台任务：将请求写入磁盘缓存。
pub struct SpoolerTask {
    spool_rx: mpsc::Receiver<SpoolRequest>,
    spool_manager: Arc<SpoolManager>,
    worker_tx: mpsc::Sender<RepresentationId>,
    cache: Arc<RepresentationCache>,
}

impl SpoolerTask {
    pub fn new(
        spool_rx: mpsc::Receiver<SpoolRequest>,
        spool_manager: Arc<SpoolManager>,
        worker_tx: mpsc::Sender<RepresentationId>,
        cache: Arc<RepresentationCache>,
    ) -> Self {
        Self {
            spool_rx,
            spool_manager,
            worker_tx,
            cache,
        }
    }

    /// Run the spooler loop until the channel is closed.
    /// 运行写入循环，直到通道关闭。
    pub async fn run(mut self) {
        while let Some(request) = self.spool_rx.recv().await {
            debug!(
                representation_id = %request.rep_id,
                bytes = request.bytes.len(),
                "Spooler received request"
            );
            self.cache.mark_spooling(&request.rep_id).await;
            if let Err(err) = self
                .spool_manager
                .write(&request.rep_id, &request.bytes)
                .await
            {
                error!(
                    representation_id = %request.rep_id,
                    error = %err,
                    "Failed to write spool entry"
                );
                // Revert to Pending to allow retry on next resolution
                self.cache.mark_pending(&request.rep_id).await;
            } else {
                self.cache.mark_completed(&request.rep_id).await;
                debug!(
                    representation_id = %request.rep_id,
                    "Spooler wrote spool entry"
                );
                if let Err(err) = self.worker_tx.try_send(request.rep_id.clone()) {
                    warn!(
                        representation_id = %request.rep_id,
                        error = %err,
                        "Failed to enqueue worker after spool write"
                    );
                }
            }
        }
    }
}
