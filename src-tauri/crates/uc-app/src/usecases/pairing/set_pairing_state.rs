use anyhow::Result;
use std::sync::Arc;
use uc_core::network::PairingState;
use uc_core::ports::PairedDeviceRepositoryPort;
use uc_core::PeerId;

pub struct SetPairingState {
    repo: Arc<dyn PairedDeviceRepositoryPort>,
}

impl SetPairingState {
    pub fn new(repo: Arc<dyn PairedDeviceRepositoryPort>) -> Self {
        Self { repo }
    }

    pub async fn execute(&self, peer_id: PeerId, state: PairingState) -> Result<()> {
        self.repo
            .set_state(&peer_id, state)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to set pairing state: {}", e))
    }
}
