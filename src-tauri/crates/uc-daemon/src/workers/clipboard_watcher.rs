//! Placeholder clipboard watcher service.

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use crate::service::{DaemonService, ServiceHealth};

/// Placeholder clipboard watcher service.
///
/// In the skeleton phase this service does nothing except wait for cancellation.
/// A future plan will integrate with the platform clipboard watcher.
pub struct ClipboardWatcherWorker;

#[async_trait]
impl DaemonService for ClipboardWatcherWorker {
    fn name(&self) -> &str {
        "clipboard-watcher"
    }

    async fn start(&self, cancel: CancellationToken) -> anyhow::Result<()> {
        tracing::info!("clipboard watcher started (placeholder)");
        cancel.cancelled().await;
        tracing::info!("clipboard watcher cancelled");
        Ok(())
    }

    async fn stop(&self) -> anyhow::Result<()> {
        tracing::info!("clipboard watcher stopped");
        Ok(())
    }

    fn health_check(&self) -> ServiceHealth {
        ServiceHealth::Healthy
    }
}
