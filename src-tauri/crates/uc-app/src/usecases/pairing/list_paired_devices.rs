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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_mocks::MockPairedDeviceRepository;
    use std::sync::Arc;
    use uc_core::network::{PairedDevice, PairingState};
    use uc_core::PeerId;

    #[tokio::test]
    async fn test_list_paired_devices_returns_devices() {
        let devices = vec![PairedDevice {
            peer_id: PeerId::from("peer-1"),
            device_name: "test-device".to_string(),
            pairing_state: PairingState::Trusted,
            identity_fingerprint: "fp".to_string(),
            paired_at: chrono::Utc::now(),
            last_seen_at: None,
            sync_settings: None,
        }];

        let mut repo = MockPairedDeviceRepository::new();
        repo.expect_list_all()
            .returning(move || Ok(devices.clone()));

        let uc = ListPairedDevices::new(Arc::new(repo));
        let devices = uc.execute().await.unwrap();

        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].peer_id.as_str(), "peer-1");
    }
}
