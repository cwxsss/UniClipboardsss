//! `SetupStatusFacade` implementation.

use std::sync::Arc;

use uc_core::ports::SetupStatusPort;

use crate::setup::usecases::{IsSetupCompleteUsecase, MarkSetupCompleteUsecase};

/// Setup-status facade: read + write the persisted completion flag.
///
/// Owns both use cases, each built from the same shared
/// [`SetupStatusPort`] adapter so bootstrap only wires the port once.
pub struct SetupStatusFacade {
    is_complete: IsSetupCompleteUsecase,
    mark_complete: MarkSetupCompleteUsecase,
}

impl SetupStatusFacade {
    pub fn new(setup_status: Arc<dyn SetupStatusPort>) -> Self {
        Self {
            is_complete: IsSetupCompleteUsecase::new(setup_status.clone()),
            mark_complete: MarkSetupCompleteUsecase::new(setup_status),
        }
    }

    /// Returns `Ok(true)` when `SetupStatus.has_completed` is `true`.
    /// Propagates the port's `anyhow::Error` on read failure.
    pub async fn is_complete(&self) -> anyhow::Result<bool> {
        self.is_complete.execute().await
    }

    /// Persists `SetupStatus.has_completed = true`, preserving any other
    /// fields (e.g. `space_id`) already on the record.
    pub async fn mark_complete(&self) -> anyhow::Result<()> {
        self.mark_complete.execute().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::setup::testing::FakeSetupStatus;

    #[tokio::test]
    async fn is_complete_reports_true_when_status_completed() {
        let facade = SetupStatusFacade::new(FakeSetupStatus::completed());
        assert!(facade.is_complete().await.unwrap());
    }

    #[tokio::test]
    async fn is_complete_reports_false_when_status_default() {
        let facade = SetupStatusFacade::new(FakeSetupStatus::default_not_completed());
        assert!(!facade.is_complete().await.unwrap());
    }

    #[tokio::test]
    async fn mark_complete_flips_the_persisted_flag() {
        let port = FakeSetupStatus::default_not_completed();
        let facade = SetupStatusFacade::new(port.clone());

        facade.mark_complete().await.unwrap();

        assert!(port.snapshot().await.has_completed);
        assert!(facade.is_complete().await.unwrap());
    }
}
