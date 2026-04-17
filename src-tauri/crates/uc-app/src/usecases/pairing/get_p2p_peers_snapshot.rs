//! GetP2pPeersSnapshot use case - combines discovered, connected, and paired peers into a unified snapshot.

use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use uc_core::ports::paired_device_repository::PairedDeviceRepositoryPort;
use uc_core::ports::PeerDirectoryPort;

/// Unified peer snapshot combining discovered, connected, and paired peer information.
#[derive(Debug, Clone)]
pub struct P2pPeerSnapshot {
    pub peer_id: String,
    pub device_name: Option<String>,
    pub addresses: Vec<String>,
    pub is_paired: bool,
    pub is_connected: bool,
    pub pairing_state: String,
    pub identity_fingerprint: String,
}

/// Use case that aggregates discovered peers, connected peers, and paired devices
/// into a single unified snapshot for both GUI and CLI consumption.
///
/// This consolidates the peer aggregation logic that was previously duplicated
/// in Tauri commands (`get_p2p_peers`, `get_paired_peers_with_status`).
pub struct GetP2pPeersSnapshot {
    peer_dir: Arc<dyn PeerDirectoryPort>,
    paired_repo: Arc<dyn PairedDeviceRepositoryPort>,
}

impl GetP2pPeersSnapshot {
    pub fn new(
        peer_dir: Arc<dyn PeerDirectoryPort>,
        paired_repo: Arc<dyn PairedDeviceRepositoryPort>,
    ) -> Self {
        Self {
            peer_dir,
            paired_repo,
        }
    }

    /// Produce a merged snapshot of discovered, connected, and paired peers.
    ///
    /// The result contains one entry per relevant peer (excluding the local peer).
    /// - Discovered peers appear with their discovered addresses and `is_connected` reflecting the connected list.
    /// - If a paired record exists for a discovered peer, the snapshot uses the paired `device_name` when non-empty and includes the paired `pairing_state` and `identity_fingerprint`.
    /// - Peers present only in the paired repository are included with an empty address list, `is_paired = true`, and `is_connected = false`.
    /// The `pairing_state` field is emitted as one of the stable strings `"Pending"`, `"Trusted"`, `"Revoked"`, or `"NotPaired"` when no paired record exists.
    ///
    /// # Returns
    ///
    /// `Ok(Vec<P2pPeerSnapshot>)` with the merged snapshots; `Err` if listing discovered peers, connected peers, or paired devices fails.
    pub async fn execute(&self) -> Result<Vec<P2pPeerSnapshot>> {
        let local_id = self.peer_dir.local_peer_id();

        // 1. List discovered peers
        let discovered = self
            .peer_dir
            .get_discovered_peers()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to list discovered peers: {}", e))?;
        // Defense-in-depth: exclude local peer even if adapter missed it
        let discovered: Vec<_> = discovered
            .into_iter()
            .filter(|p| p.peer_id != local_id)
            .collect();

        // 2. List connected peers
        let connected = self
            .peer_dir
            .get_connected_peers()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to list connected peers: {}", e))?;
        let connected_ids: HashSet<_> = connected.iter().map(|p| p.peer_id.clone()).collect();

        // 3. List paired devices
        let paired = self
            .paired_repo
            .list_all()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to list paired devices: {}", e))?;
        let paired_map: HashMap<_, _> = paired.iter().map(|p| (p.peer_id.to_string(), p)).collect();

        // 4. Merge into unified snapshot
        let mut snapshots = Vec::new();
        let discovered_ids: HashSet<_> = discovered.iter().map(|p| p.peer_id.clone()).collect();

        // Add discovered peers
        for peer in discovered {
            let peer_id = peer.peer_id.clone();
            let paired_dev = paired_map.get(&peer_id);
            snapshots.push(P2pPeerSnapshot {
                peer_id: peer_id.clone(),
                device_name: paired_dev
                    .and_then(|p| {
                        if p.device_name.is_empty() {
                            None
                        } else {
                            Some(p.device_name.clone())
                        }
                    })
                    .or(peer.device_name),
                addresses: peer.addresses,
                is_paired: peer.is_paired,
                is_connected: connected_ids.contains(&peer_id),
                pairing_state: paired_dev
                    .map(|p| p.pairing_state.to_string())
                    .unwrap_or_else(|| "NotPaired".to_string()),
                identity_fingerprint: paired_dev
                    .map(|p| p.identity_fingerprint.clone())
                    .unwrap_or_default(),
            });
        }

        // Add paired but not discovered peers
        for (peer_id, dev) in paired_map {
            if !connected_ids.contains(&peer_id) && !discovered_ids.contains(&peer_id) {
                snapshots.push(P2pPeerSnapshot {
                    peer_id: peer_id.clone(),
                    device_name: {
                        let name = &dev.device_name;
                        if name.is_empty() {
                            None
                        } else {
                            Some(name.clone())
                        }
                    },
                    addresses: vec![],
                    is_paired: true,
                    is_connected: false,
                    pairing_state: dev.pairing_state.to_string(),
                    identity_fingerprint: dev.identity_fingerprint.clone(),
                });
            }
        }

        Ok(snapshots)
    }
}
