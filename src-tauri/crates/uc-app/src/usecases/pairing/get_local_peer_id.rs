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
