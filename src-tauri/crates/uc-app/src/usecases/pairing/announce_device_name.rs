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
