use anyhow::Result;
use std::sync::Arc;
use uc_core::ports::PairedDeviceRepositoryPort;
use uc_core::settings::model::SyncSettings;
use uc_core::PeerId;

pub struct UpdateDeviceSyncSettings {
    paired_device_repo: Arc<dyn PairedDeviceRepositoryPort>,
}

impl UpdateDeviceSyncSettings {
    pub fn from_ports(paired_device_repo: Arc<dyn PairedDeviceRepositoryPort>) -> Self {
        Self { paired_device_repo }
    }

    pub async fn execute(&self, peer_id: &PeerId, settings: Option<SyncSettings>) -> Result<()> {
        self.paired_device_repo
            .update_sync_settings(peer_id, settings)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to update device sync settings: {}", e))
    }
}
