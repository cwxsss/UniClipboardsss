use anyhow::Result;
use std::sync::Arc;

use uc_core::network::ConnectedPeer;
use uc_core::ports::PeerDirectoryPort;

pub struct ListConnectedPeers {
    network: Arc<dyn PeerDirectoryPort>,
}

impl ListConnectedPeers {
    pub fn new(network: Arc<dyn PeerDirectoryPort>) -> Self {
        Self { network }
    }

    pub async fn execute(&self) -> Result<Vec<ConnectedPeer>> {
        self.network
            .get_connected_peers()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to list connected peers: {}", e))
    }
}
