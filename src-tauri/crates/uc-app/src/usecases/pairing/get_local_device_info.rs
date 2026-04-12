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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_mocks::{MockPeerDirectory, MockSettings};
    use uc_core::settings::model::Settings;

    #[tokio::test]
    async fn uses_device_name_from_settings() {
        let mut settings = Settings::default();
        settings.general.device_name = Some("Desk".to_string());

        let mut network = MockPeerDirectory::new();
        network
            .expect_local_peer_id()
            .returning(|| "peer-1".to_string());

        let mut settings_port = MockSettings::new();
        settings_port
            .expect_load()
            .returning(move || Ok(settings.clone()));

        let usecase = GetLocalDeviceInfo::new(Arc::new(network), Arc::new(settings_port));

        let info = usecase.execute().await.expect("load device info");
        assert_eq!(info.peer_id, "peer-1");
        assert_eq!(info.device_name, "Desk");
    }

    #[tokio::test]
    async fn trims_device_name_from_settings() {
        let mut settings = Settings::default();
        settings.general.device_name = Some("  Desk  ".to_string());

        let mut network = MockPeerDirectory::new();
        network
            .expect_local_peer_id()
            .returning(|| "peer-2".to_string());

        let mut settings_port = MockSettings::new();
        settings_port
            .expect_load()
            .returning(move || Ok(settings.clone()));

        let usecase = GetLocalDeviceInfo::new(Arc::new(network), Arc::new(settings_port));

        let info = usecase.execute().await.expect("load device info");
        assert_eq!(info.device_name, "Desk");
    }

    #[tokio::test]
    async fn uses_default_name_when_settings_missing_or_empty() {
        let mut settings = Settings::default();
        settings.general.device_name = Some("   ".to_string());

        let mut network = MockPeerDirectory::new();
        network
            .expect_local_peer_id()
            .returning(|| "peer-3".to_string());

        let mut settings_port = MockSettings::new();
        settings_port
            .expect_load()
            .returning(move || Ok(settings.clone()));

        let usecase = GetLocalDeviceInfo::new(Arc::new(network), Arc::new(settings_port));

        let info = usecase.execute().await.expect("load device info");
        assert_eq!(info.device_name, DEFAULT_PAIRING_DEVICE_NAME);
    }

    #[tokio::test]
    async fn uses_default_name_when_settings_fail_to_load() {
        let mut network = MockPeerDirectory::new();
        network
            .expect_local_peer_id()
            .returning(|| "peer-4".to_string());

        let mut settings_port = MockSettings::new();
        settings_port
            .expect_load()
            .returning(|| Err(anyhow::anyhow!("load failed")));

        let usecase = GetLocalDeviceInfo::new(Arc::new(network), Arc::new(settings_port));

        let info = usecase.execute().await.expect("load device info");
        assert_eq!(info.device_name, DEFAULT_PAIRING_DEVICE_NAME);
    }
}
