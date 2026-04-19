//!
//! Network lifecycle control port.
//!
//! This port allows the application layer to request network startup and
//! shutdown without depending on concrete network implementations.

use anyhow::Result;
use async_trait::async_trait;

/// Network control port — owns the network runtime lifecycle.
#[async_trait]
pub trait NetworkControlPort: Send + Sync {
    /// Start the network runtime.
    async fn start_network(&self) -> Result<()>;

    /// Stop the network runtime.
    ///
    /// Default implementation is a no-op so adapters that predate Slice 1
    /// (e.g. the frozen libp2p adapter) keep compiling unchanged. The iroh
    /// adapter overrides this to close its endpoint; see Slice 1 decision N-1.
    async fn stop_network(&self) -> Result<()> {
        Ok(())
    }
}
