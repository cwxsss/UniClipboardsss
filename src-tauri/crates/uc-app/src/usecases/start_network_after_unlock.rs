use std::sync::Arc;
use tracing::{info, info_span, Instrument};
use uc_core::ports::NetworkControlPort;

use super::start_network::StartNetworkError;

/// Use case for starting the network runtime after unlock.
pub struct StartNetworkAfterUnlock {
    network_control: Arc<dyn NetworkControlPort>,
}

impl StartNetworkAfterUnlock {
    /// Create a new StartNetworkAfterUnlock use case.
    pub fn new(network_control: Arc<dyn NetworkControlPort>) -> Self {
        Self { network_control }
    }

    /// Create a new StartNetworkAfterUnlock use case from an Arc port.
    pub fn from_port(network_control: Arc<dyn NetworkControlPort>) -> Self {
        Self::new(network_control)
    }

    /// Execute the use case.
    pub async fn execute(&self) -> Result<(), StartNetworkError> {
        let span = info_span!("usecase.start_network_after_unlock.execute");

        async {
            info!("Requesting network start after unlock");
            if let Err(err) = self.network_control.start_network().await {
                tracing::warn!(error = %err, "Network start after unlock failed");
                return Err(err.into());
            }
            info!("Network started successfully after unlock");
            Ok(())
        }
        .instrument(span)
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_mocks::MockNetworkControl;

    #[tokio::test]
    async fn start_network_after_unlock_invokes_network_control() {
        let started = Arc::new(std::sync::Mutex::new(false));
        let started_clone = started.clone();
        let mut control = MockNetworkControl::new();
        control.expect_start_network().returning(move || {
            *started_clone.lock().unwrap() = true;
            Ok(())
        });

        let use_case = StartNetworkAfterUnlock::new(Arc::new(control));
        let result = use_case.execute().await;

        assert!(result.is_ok(), "start_network should succeed");
        assert!(*started.lock().unwrap(), "network should be started");
    }
}
