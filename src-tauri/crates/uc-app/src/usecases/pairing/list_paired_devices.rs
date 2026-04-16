use anyhow::Result;
use std::sync::Arc;
use uc_core::network::PairedDevice;
use uc_core::ports::PairedDeviceRepositoryPort;

pub struct ListPairedDevices {
    repo: Arc<dyn PairedDeviceRepositoryPort>,
}

impl ListPairedDevices {
    pub fn new(repo: Arc<dyn PairedDeviceRepositoryPort>) -> Self {
        Self { repo }
    }

    pub async fn execute(&self) -> Result<Vec<PairedDevice>> {
        self.repo
            .list_all()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to list paired devices: {}", e))
    }
}
