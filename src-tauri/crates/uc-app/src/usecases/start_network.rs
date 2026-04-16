//! 启动网络的用例

use tracing::{info, info_span, Instrument};
use uc_core::ports::NetworkControlPort;

/// Error type for network startup failures.
#[derive(Debug, thiserror::Error)]
pub enum StartNetworkError {
    #[error("Failed to start network: {0}")]
    StartFailed(String),
}

impl From<anyhow::Error> for StartNetworkError {
    fn from(err: anyhow::Error) -> Self {
        StartNetworkError::StartFailed(err.to_string())
    }
}

/// Use case for starting the network runtime.
pub struct StartNetwork {
    network_control: std::sync::Arc<dyn NetworkControlPort>,
}

impl StartNetwork {
    /// Create a new StartNetwork use case.
    pub fn new(network_control: std::sync::Arc<dyn NetworkControlPort>) -> Self {
        Self { network_control }
    }

    /// Create a new StartNetwork use case from an Arc port.
    pub fn from_port(network_control: std::sync::Arc<dyn NetworkControlPort>) -> Self {
        Self::new(network_control)
    }

    /// Execute the use case.
    pub async fn execute(&self) -> Result<(), StartNetworkError> {
        let span = info_span!("usecase.start_network.execute");

        async {
            info!("Requesting network start");
            self.network_control.start_network().await?;
            info!("Network started successfully");
            Ok(())
        }
        .instrument(span)
        .await
    }
}
