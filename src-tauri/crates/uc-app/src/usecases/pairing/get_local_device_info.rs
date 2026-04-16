use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use uc_core::ports::{PeerDirectoryPort, SettingsPort};

const DEFAULT_PAIRING_DEVICE_NAME: &str = "Uniclipboard Device";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalDeviceInfo {
    pub peer_id: String,
    pub device_name: String,
}

pub struct GetLocalDeviceInfo {
    network: Arc<dyn PeerDirectoryPort>,
    settings: Arc<dyn SettingsPort>,
}

impl GetLocalDeviceInfo {
    pub fn new(network: Arc<dyn PeerDirectoryPort>, settings: Arc<dyn SettingsPort>) -> Self {
        Self { network, settings }
    }

    pub async fn execute(&self) -> Result<LocalDeviceInfo> {
        let device_name = match self.settings.load().await {
            Ok(settings) => {
                let name = settings.general.device_name.unwrap_or_default();
                let trimmed = name.trim();
                if trimmed.is_empty() {
                    DEFAULT_PAIRING_DEVICE_NAME.to_string()
                } else {
                    trimmed.to_string()
                }
            }
            Err(err) => {
                tracing::warn!(error = %err, "Failed to load settings for pairing device name");
                DEFAULT_PAIRING_DEVICE_NAME.to_string()
            }
        };

        Ok(LocalDeviceInfo {
            peer_id: self.network.local_peer_id(),
            device_name,
        })
    }
}
