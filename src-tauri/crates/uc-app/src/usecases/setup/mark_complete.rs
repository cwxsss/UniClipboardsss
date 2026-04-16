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
