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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_mocks::MockPeerDirectory;
    use chrono::Utc;

    #[tokio::test]
    async fn returns_discovered_peers_on_success() {
        let peers = vec![DiscoveredPeer {
            peer_id: "peer-1".to_string(),
            device_name: Some("Desk".to_string()),
            device_id: Some("123456".to_string()),
            addresses: vec!["/ip4/127.0.0.1".to_string()],
            discovered_at: Utc::now(),
            last_seen: Utc::now(),
            is_paired: false,
        }];
        let peers_clone = peers.clone();

        let mut network = MockPeerDirectory::new();
        network
            .expect_get_discovered_peers()
            .returning(move || Ok(peers_clone.clone()));

        let usecase = ListDiscoveredPeers::new(Arc::new(network));

        let result = usecase.execute().await.expect("list discovered peers");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].peer_id, peers[0].peer_id);
        assert_eq!(result[0].device_name, peers[0].device_name);
    }

    #[tokio::test]
    async fn wraps_errors_with_context() {
        let mut network = MockPeerDirectory::new();
        network
            .expect_get_discovered_peers()
            .returning(|| Err(anyhow::anyhow!("boom")));

        let usecase = ListDiscoveredPeers::new(Arc::new(network));

        let err = usecase.execute().await.expect_err("expected error");
        let message = err.to_string();
        assert!(message.contains("Failed to list discovered peers"));
        assert!(message.contains("boom"));
    }
}
