use std::sync::Arc;

use uc_core::ports::PeerDirectoryPort;

pub struct GetLocalPeerId {
    network: Arc<dyn PeerDirectoryPort>,
}

impl GetLocalPeerId {
    pub fn new(network: Arc<dyn PeerDirectoryPort>) -> Self {
        Self { network }
    }

    pub fn execute(&self) -> String {
        self.network.local_peer_id()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_mocks::MockPeerDirectory;

    #[test]
    fn returns_local_peer_id_from_network() {
        let mut network = MockPeerDirectory::new();
        network
            .expect_local_peer_id()
            .returning(|| "peer-123".to_string());

        let usecase = GetLocalPeerId::new(Arc::new(network));

        let peer_id = usecase.execute();
        assert_eq!(peer_id, "peer-123");
    }
}
