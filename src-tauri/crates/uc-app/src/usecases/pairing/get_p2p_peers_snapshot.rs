//! GetP2pPeersSnapshot use case - combines discovered, connected, and admitted
//! members into a unified snapshot.

use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use uc_core::ports::PeerDirectoryPort;
use uc_core::MemberRepositoryPort;

/// Unified peer snapshot combining discovered, connected, and admitted
/// member information.
///
/// 字段语义（phase 4b PR-4 之后）：
/// - `pairing_state` 只有两种取值：`"Trusted"`（命中 `space_member`）或
///   `"NotPaired"`（未命中）。历史上的 `"Pending"` / `"Revoked"` 随着
///   `paired_device` 表被 PR-5 清除而彻底消失——那两种状态属于配对流程中转
///   态，现在只由 `trusted_peer` / 配对流程状态机持有，快照不再暴露。
/// - `identity_fingerprint` 取自 `SpaceMember`，未命中成员时为空字符串。
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

/// Use case that aggregates discovered peers, connected peers, and admitted
/// space members into a single unified snapshot for both GUI and CLI
/// consumption.
///
/// This consolidates the peer aggregation logic that was previously duplicated
/// in Tauri commands (`get_p2p_peers`, `get_paired_peers_with_status`).
pub struct GetP2pPeersSnapshot {
    peer_dir: Arc<dyn PeerDirectoryPort>,
    member_repo: Arc<dyn MemberRepositoryPort>,
}

impl GetP2pPeersSnapshot {
    pub fn new(
        peer_dir: Arc<dyn PeerDirectoryPort>,
        member_repo: Arc<dyn MemberRepositoryPort>,
    ) -> Self {
        Self {
            peer_dir,
            member_repo,
        }
    }

    /// Produce a merged snapshot of discovered, connected, and admitted
    /// members.
    ///
    /// The result contains one entry per relevant peer (excluding the local
    /// peer).
    /// - Discovered peers appear with their discovered addresses and
    ///   `is_connected` reflecting the connected list.
    /// - If a space-member record exists for a discovered peer, the snapshot
    ///   uses the member's `device_name` when non-empty and exposes
    ///   `pairing_state = "Trusted"` + the member `identity_fingerprint`.
    /// - Members with no matching discovered/connected entry are included with
    ///   an empty address list, `is_paired = true`, and
    ///   `is_connected = false`.
    ///
    /// # Returns
    ///
    /// `Ok(Vec<P2pPeerSnapshot>)` with the merged snapshots; `Err` if listing
    /// discovered peers, connected peers, or members fails.
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

        // 3. List admitted space members (phase 4b PR-4：唯一权威来源).
        let members = self
            .member_repo
            .list()
            .await
            .context("failed to list space members")?;
        let member_map: HashMap<String, _> = members
            .into_iter()
            .filter(|m| m.device_id.as_str() != local_id)
            .map(|m| (m.device_id.as_str().to_string(), m))
            .collect();

        // 4. Merge into unified snapshot
        let mut snapshots = Vec::new();
        let discovered_ids: HashSet<_> = discovered.iter().map(|p| p.peer_id.clone()).collect();

        // Add discovered peers
        for peer in discovered {
            let peer_id = peer.peer_id.clone();
            let member = member_map.get(&peer_id);
            snapshots.push(P2pPeerSnapshot {
                peer_id: peer_id.clone(),
                device_name: member
                    .and_then(|m| {
                        if m.device_name.is_empty() {
                            None
                        } else {
                            Some(m.device_name.clone())
                        }
                    })
                    .or(peer.device_name),
                addresses: peer.addresses,
                is_paired: member.is_some() || peer.is_paired,
                is_connected: connected_ids.contains(&peer_id),
                pairing_state: if member.is_some() {
                    "Trusted".to_string()
                } else {
                    "NotPaired".to_string()
                },
                identity_fingerprint: member
                    .map(|m| m.identity_fingerprint.clone())
                    .unwrap_or_default(),
            });
        }

        // Add members that are neither discovered nor connected.
        for (peer_id, member) in member_map {
            if !connected_ids.contains(&peer_id) && !discovered_ids.contains(&peer_id) {
                snapshots.push(P2pPeerSnapshot {
                    peer_id: peer_id.clone(),
                    device_name: {
                        let name = &member.device_name;
                        if name.is_empty() {
                            None
                        } else {
                            Some(name.clone())
                        }
                    },
                    addresses: vec![],
                    is_paired: true,
                    is_connected: false,
                    pairing_state: "Trusted".to_string(),
                    identity_fingerprint: member.identity_fingerprint.clone(),
                });
            }
        }

        Ok(snapshots)
    }
}
