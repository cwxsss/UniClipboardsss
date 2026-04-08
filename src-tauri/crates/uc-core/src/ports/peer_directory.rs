//! Peer directory port.
//!
//! Defines peer listing and local peer identity capabilities.

use crate::network::{ConnectedPeer, DiscoveredPeer};
use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
pub trait PeerDirectoryPort: Send + Sync {
    /// Get all discovered peers (from mDNS/discovery).
    async fn get_discovered_peers(&self) -> Result<Vec<DiscoveredPeer>>;

    /// Get currently connected peers.
    async fn get_connected_peers(&self) -> Result<Vec<ConnectedPeer>>;

    /// Get local peer ID.
    fn local_peer_id(&self) -> String;

    /// Announce local device name to peers.
    async fn announce_device_name(&self, device_name: String) -> Result<()>;
}
