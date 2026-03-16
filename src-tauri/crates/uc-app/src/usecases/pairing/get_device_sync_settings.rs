use anyhow::Result;
use std::sync::Arc;
use uc_core::network::paired_device::resolve_sync_settings;
use uc_core::ports::{PairedDeviceRepositoryPort, SettingsPort};
use uc_core::settings::model::SyncSettings;
use uc_core::PeerId;

pub struct GetDeviceSyncSettings {
    paired_device_repo: Arc<dyn PairedDeviceRepositoryPort>,
    settings: Arc<dyn SettingsPort>,
}

impl GetDeviceSyncSettings {
    pub fn from_ports(
        paired_device_repo: Arc<dyn PairedDeviceRepositoryPort>,
        settings: Arc<dyn SettingsPort>,
    ) -> Self {
        Self {
            paired_device_repo,
            settings,
        }
    }

    pub async fn execute(&self, peer_id: &PeerId) -> Result<SyncSettings> {
        let device = self
            .paired_device_repo
            .get_by_peer_id(peer_id)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to load paired device: {}", e))?
            .ok_or_else(|| anyhow::anyhow!("Paired device not found: {}", peer_id.as_str()))?;

        let global_settings = self
            .settings
            .load()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to load global settings: {}", e))?;

        let effective = resolve_sync_settings(&device, &global_settings.sync);
        Ok(effective.clone())
    }
}
