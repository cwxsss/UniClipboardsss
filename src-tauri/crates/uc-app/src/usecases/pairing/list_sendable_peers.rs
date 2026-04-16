use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result};

use uc_core::network::DiscoveredPeer;
use uc_core::pairing::PairingState;
use uc_core::ports::{PairedDeviceRepositoryPort, PeerDirectoryPort};

/// List peers eligible for outbound data sync.
///
/// Primary source: trusted paired devices (persistent DB).
/// Enrichment: discovered peer info (addresses, device_id, last_seen) from
/// the network discovery layer.
pub struct ListSendablePeers {
    paired_device_repo: Arc<dyn PairedDeviceRepositoryPort>,
    peer_directory: Arc<dyn PeerDirectoryPort>,
}

impl ListSendablePeers {
    pub fn new(
        paired_device_repo: Arc<dyn PairedDeviceRepositoryPort>,
        peer_directory: Arc<dyn PeerDirectoryPort>,
    ) -> Self {
        Self {
            paired_device_repo,
            peer_directory,
        }
    }

    pub async fn execute(&self) -> Result<Vec<DiscoveredPeer>> {
        let paired = self
            .paired_device_repo
            .list_all()
            .await
            .context("failed to load paired devices for sendable peer resolution")?;
        let local_peer_id = self.peer_directory.local_peer_id();
        let discovered = self
            .peer_directory
            .get_discovered_peers()
            .await
            .unwrap_or_default();
        let discovered_map: HashMap<&str, &DiscoveredPeer> =
            discovered.iter().map(|p| (p.peer_id.as_str(), p)).collect();

        Ok(paired
            .into_iter()
            .filter(|d| d.pairing_state == PairingState::Trusted)
            .filter(|d| d.peer_id.as_str() != local_peer_id)
            .map(|device| {
                let cached = discovered_map.get(device.peer_id.as_str());
                DiscoveredPeer {
                    peer_id: device.peer_id.as_str().to_string(),
                    device_name: cached
                        .and_then(|c| c.device_name.clone())
                        .or(Some(device.device_name)),
                    device_id: cached.and_then(|c| c.device_id.clone()),
                    addresses: cached.map(|c| c.addresses.clone()).unwrap_or_default(),
                    discovered_at: cached.map(|c| c.discovered_at).unwrap_or(device.paired_at),
                    last_seen: cached
                        .map(|c| c.last_seen)
                        .unwrap_or(device.last_seen_at.unwrap_or(device.paired_at)),
                    is_paired: true,
                }
            })
            .collect())
    }
}
