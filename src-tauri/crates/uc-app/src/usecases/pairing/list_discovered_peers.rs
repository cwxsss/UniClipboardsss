use anyhow::Result;
use std::sync::Arc;

use uc_core::network::DiscoveredPeer;
use uc_core::ports::PeerDirectoryPort;

pub struct ListDiscoveredPeers {
    network: Arc<dyn PeerDirectoryPort>,
}

impl ListDiscoveredPeers {
    pub fn new(network: Arc<dyn PeerDirectoryPort>) -> Self {
        Self { network }
    }

    pub async fn execute(&self) -> Result<Vec<DiscoveredPeer>> {
        self.network
            .get_discovered_peers()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to list discovered peers: {}", e))
    }
}
