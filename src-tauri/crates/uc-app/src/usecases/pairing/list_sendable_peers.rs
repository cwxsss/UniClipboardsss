use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result};

use uc_core::network::DiscoveredPeer;
use uc_core::ports::PeerDirectoryPort;
use uc_core::MemberRepositoryPort;

/// List peers eligible for outbound data sync.
///
/// Primary source: admitted space members (persistent DB). Every
/// `SpaceMember` record represents an active, trusted peer, so no
/// additional state filter is needed — revocation is hard delete.
/// Enrichment: discovered peer info (addresses, device_id, last_seen)
/// from the network discovery layer.
pub struct ListSendablePeers {
    member_repo: Arc<dyn MemberRepositoryPort>,
    peer_directory: Arc<dyn PeerDirectoryPort>,
}

impl ListSendablePeers {
    pub fn new(
        member_repo: Arc<dyn MemberRepositoryPort>,
        peer_directory: Arc<dyn PeerDirectoryPort>,
    ) -> Self {
        Self {
            member_repo,
            peer_directory,
        }
    }

    pub async fn execute(&self) -> Result<Vec<DiscoveredPeer>> {
        let members = self
            .member_repo
            .list()
            .await
            .context("failed to load members for sendable peer resolution")?;
        let local_peer_id = self.peer_directory.local_peer_id();
        let discovered = self
            .peer_directory
            .get_discovered_peers()
            .await
            .unwrap_or_default();
        let discovered_map: HashMap<&str, &DiscoveredPeer> =
            discovered.iter().map(|p| (p.peer_id.as_str(), p)).collect();

        Ok(members
            .into_iter()
            .filter(|m| m.device_id.as_str() != local_peer_id)
            .map(|member| {
                let peer_id = member.device_id.as_str().to_string();
                let cached = discovered_map.get(peer_id.as_str());
                DiscoveredPeer {
                    peer_id,
                    device_name: cached
                        .and_then(|c| c.device_name.clone())
                        .or(Some(member.device_name)),
                    device_id: cached.and_then(|c| c.device_id.clone()),
                    addresses: cached.map(|c| c.addresses.clone()).unwrap_or_default(),
                    discovered_at: cached.map(|c| c.discovered_at).unwrap_or(member.joined_at),
                    last_seen: cached.map(|c| c.last_seen).unwrap_or(member.joined_at),
                    is_paired: true,
                }
            })
            .collect())
    }
}
