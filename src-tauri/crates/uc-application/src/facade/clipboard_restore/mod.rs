use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use tracing::instrument;

#[async_trait]
pub trait ClipboardRestoreGateway: Send + Sync {
    async fn restore_entry(&self, entry_id: &str) -> Result<(), ClipboardRestoreError>;
}

#[derive(Debug, thiserror::Error)]
pub enum ClipboardRestoreError {
    #[error("clipboard entry not found")]
    NotFound,
    #[error("clipboard restore failed: {0}")]
    Internal(String),
}

pub struct ClipboardRestoreFacade {
    gateway: Arc<dyn ClipboardRestoreGateway>,
}

impl ClipboardRestoreFacade {
    pub fn new(gateway: Arc<dyn ClipboardRestoreGateway>) -> Self {
        Self { gateway }
    }

    #[instrument(skip_all, fields(entry_id = %entry_id))]
    pub async fn restore_entry(&self, entry_id: &str) -> Result<(), ClipboardRestoreError> {
        self.gateway.restore_entry(entry_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Mutex;

    struct FakeRestoreGateway {
        calls: Mutex<Vec<String>>,
        result: Mutex<Option<Result<(), ClipboardRestoreError>>>,
    }

    #[async_trait]
    impl ClipboardRestoreGateway for FakeRestoreGateway {
        async fn restore_entry(&self, entry_id: &str) -> Result<(), ClipboardRestoreError> {
            self.calls
                .lock()
                .expect("calls lock")
                .push(entry_id.to_string());
            self.result
                .lock()
                .expect("result lock")
                .take()
                .unwrap_or(Ok(()))
        }
    }

    fn facade_with(result: Option<Result<(), ClipboardRestoreError>>) -> ClipboardRestoreFacade {
        ClipboardRestoreFacade::new(Arc::new(FakeRestoreGateway {
            calls: Mutex::new(Vec::new()),
            result: Mutex::new(result),
        }))
    }

    #[tokio::test]
    async fn restore_entry_delegates_to_gateway() {
        let facade = facade_with(None);

        facade.restore_entry("entry-1").await.expect("restore");
    }

    #[tokio::test]
    async fn restore_entry_preserves_not_found_error() {
        let facade = facade_with(Some(Err(ClipboardRestoreError::NotFound)));

        let error = facade
            .restore_entry("missing")
            .await
            .expect_err("not found");

        assert!(matches!(error, ClipboardRestoreError::NotFound));
    }
}
