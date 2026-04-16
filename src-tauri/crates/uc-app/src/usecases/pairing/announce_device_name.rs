use anyhow::Result;
use std::sync::Arc;
use tracing::{info_span, Instrument};
use uc_core::ports::PeerDirectoryPort;

/// Use case for announcing the local device name over the network.
pub struct AnnounceDeviceName {
    network: Arc<dyn PeerDirectoryPort>,
}

impl AnnounceDeviceName {
    /// Create a new AnnounceDeviceName use case.
    pub fn new(network: Arc<dyn PeerDirectoryPort>) -> Self {
        Self { network }
    }

    /// Execute the use case.
    pub async fn execute(&self, device_name: String) -> Result<()> {
        let span = info_span!("usecase.announce_device_name.execute");

        async { self.network.announce_device_name(device_name).await }
            .instrument(span)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::AnnounceDeviceName;
    use crate::test_mocks::MockPeerDirectory;
    use std::sync::{Arc, Mutex};

    #[tokio::test]
    async fn announce_device_name_invokes_network_port() {
        let called: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let called_clone = called.clone();

        let mut network = MockPeerDirectory::new();
        network
            .expect_announce_device_name()
            .returning(move |device_name| {
                called_clone.lock().expect("called lock").push(device_name);
                Ok(())
            });

        let uc = AnnounceDeviceName::new(Arc::new(network));

        uc.execute("Desk".to_string())
            .await
            .expect("announce device name");

        let called = called.lock().expect("called lock");
        assert_eq!(called.as_slice(), ["Desk".to_string()]);
    }

    #[tokio::test]
    async fn announce_device_name_propagates_error() {
        let mut network = MockPeerDirectory::new();
        network
            .expect_announce_device_name()
            .returning(|_| Err(anyhow::anyhow!("announce failed")));

        let uc = AnnounceDeviceName::new(Arc::new(network));

        let result = uc.execute("Desk".to_string()).await;

        assert!(result.is_err());
    }
}
