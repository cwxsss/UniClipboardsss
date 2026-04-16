//! Shared error type for network-startup use cases.

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
