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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_mocks::MockPeerDirectory;
    use chrono::Utc;

    #[tokio::test]
    async fn returns_connected_peers_on_success() {
        let peers = vec![ConnectedPeer {
            peer_id: "peer-1".to_string(),
            device_name: "Desk".to_string(),
            connected_at: Utc::now(),
        }];
        let peers_clone = peers.clone();

        let mut network = MockPeerDirectory::new();
        network
            .expect_get_connected_peers()
            .returning(move || Ok(peers_clone.clone()));

        let usecase = ListConnectedPeers::new(Arc::new(network));

        let result = usecase.execute().await.expect("list connected peers");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].peer_id, peers[0].peer_id);
        assert_eq!(result[0].device_name, peers[0].device_name);
    }

    #[tokio::test]
    async fn wraps_errors_with_context() {
        let mut network = MockPeerDirectory::new();
        network
            .expect_get_connected_peers()
            .returning(|| Err(anyhow::anyhow!("boom")));

        let usecase = ListConnectedPeers::new(Arc::new(network));

        let err = usecase.execute().await.expect_err("expected error");
        let message = err.to_string();
        assert!(message.contains("Failed to list connected peers"));
        assert!(message.contains("boom"));
    }
}
