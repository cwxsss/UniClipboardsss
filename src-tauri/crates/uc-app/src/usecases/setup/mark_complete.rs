use std::sync::Arc;

use uc_core::ports::SetupStatusPort;

/// Use case for marking setup as complete.
///
/// This updates the persistent setup completion flag.
pub struct MarkSetupComplete {
    setup_status: Arc<dyn SetupStatusPort>,
}

impl MarkSetupComplete {
    /// Create a new MarkSetupComplete use case from trait objects.
    pub fn new(setup_status: Arc<dyn SetupStatusPort>) -> Self {
        Self { setup_status }
    }

    /// Create a new MarkSetupComplete use case from cloned Arc<dyn Port> references.
    pub fn from_ports(setup_status: Arc<dyn SetupStatusPort>) -> Self {
        Self::new(setup_status)
    }

    pub async fn execute(&self) -> anyhow::Result<()> {
        let mut status = self.setup_status.get_status().await?;
        status.has_completed = true;
        self.setup_status.set_status(&status).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_mocks::MockSetupStatus;
    use uc_core::setup::SetupStatus;

    #[tokio::test]
    async fn mark_setup_complete_sets_has_completed() {
        let state = Arc::new(std::sync::Mutex::new(SetupStatus::default()));
        let get_state = state.clone();
        let set_state = state.clone();

        let mut mock = MockSetupStatus::new();
        mock.expect_get_status()
            .returning(move || Ok(get_state.lock().unwrap().clone()));
        mock.expect_set_status().returning(move |s| {
            *set_state.lock().unwrap() = s.clone();
            Ok(())
        });

        let use_case = MarkSetupComplete::new(Arc::new(mock));

        assert!(!state.lock().unwrap().has_completed);

        use_case.execute().await.unwrap();

        assert!(state.lock().unwrap().has_completed);
    }
}
