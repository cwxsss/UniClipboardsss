//! File transport port.
//!
//! Defines file transfer message send/cancel capabilities over network transports.

use crate::network::protocol::FileTransferMessage;
use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
pub trait FileTransportPort: Send + Sync {
    /// Send a file announce message to a peer.
    async fn send_file_announce(&self, peer_id: &str, announce: FileTransferMessage) -> Result<()>;

    /// Send a file data chunk to a peer.
    async fn send_file_data(&self, peer_id: &str, data: FileTransferMessage) -> Result<()>;

    /// Send a file transfer completion message to a peer.
    async fn send_file_complete(&self, peer_id: &str, complete: FileTransferMessage) -> Result<()>;

    /// Cancel an ongoing file transfer with a peer.
    async fn cancel_transfer(&self, peer_id: &str, cancel: FileTransferMessage) -> Result<()>;

    /// Send a file to a peer using the chunked transfer protocol.
    ///
    /// This is the high-level entry point that orchestrates the full transfer
    /// (announce, chunks, complete) over a single stream.
    async fn send_file(
        &self,
        peer_id: &str,
        file_path: std::path::PathBuf,
        transfer_id: String,
        batch_id: Option<String>,
        batch_total: Option<u32>,
    ) -> Result<()>;
}

/// No-op stub implementation for compilation when no real adapter is available.
///
/// Used as a placeholder in `NetworkPorts` construction until the actual
/// libp2p file transfer adapter is implemented (Phase 30).
pub struct NoopFileTransportPort;

#[async_trait]
impl FileTransportPort for NoopFileTransportPort {
    async fn send_file_announce(
        &self,
        _peer_id: &str,
        _announce: FileTransferMessage,
    ) -> Result<()> {
        Ok(())
    }

    async fn send_file_data(&self, _peer_id: &str, _data: FileTransferMessage) -> Result<()> {
        Ok(())
    }

    async fn send_file_complete(
        &self,
        _peer_id: &str,
        _complete: FileTransferMessage,
    ) -> Result<()> {
        Ok(())
    }

    async fn cancel_transfer(&self, _peer_id: &str, _cancel: FileTransferMessage) -> Result<()> {
        Ok(())
    }

    async fn send_file(
        &self,
        _peer_id: &str,
        _file_path: std::path::PathBuf,
        _transfer_id: String,
        _batch_id: Option<String>,
        _batch_total: Option<u32>,
    ) -> Result<()> {
        Ok(())
    }
}
