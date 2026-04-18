use std::sync::Arc;

use tracing::{debug, info, warn};
use uc_core::network::DiscoveredPeer;
use uc_core::ports::SettingsPort;
use uc_core::settings::content_type_filter::{is_content_type_allowed, ContentTypeCategory};
use uc_core::{DeviceId, MemberRepositoryPort};

/// Filter peers by sync policy for file content.
///
/// Checks the two global master toggles (`auto_sync`, `file_sync_enabled`),
/// then for each peer reads `MemberSyncPreferences` via `member_repo`:
/// the peer is kept only when `send_enabled` is on **and** the file
/// category is allowed by `send_content_types`.
///
/// Peers missing from `member_repo` are dropped — `member_repo` is the
/// authoritative source of sendable members after phase 3.1/3.2. Infra
/// errors are logged and the peer is kept (safety fallback for transient
/// failures).
pub async fn apply_file_sync_policy(
    settings: &Arc<dyn SettingsPort>,
    member_repo: &Arc<dyn MemberRepositoryPort>,
    peers: &[DiscoveredPeer],
) -> Vec<DiscoveredPeer> {
    // Load global settings
    let global_settings = match settings.load().await {
        Ok(s) => Some(s),
        Err(err) => {
            warn!(
                error = %err,
                "Failed to load settings for file sync policy; proceeding with all peers"
            );
            None
        }
    };

    // Global master toggle check
    if let Some(ref gs) = global_settings {
        if !gs.sync.auto_sync {
            info!("Global auto_sync disabled; skipping file sync");
            return vec![];
        }
        if !gs.file_sync.file_sync_enabled {
            info!("Global file_sync disabled; skipping file sync policy");
            return vec![];
        }
    }

    let mut result = Vec::with_capacity(peers.len());
    for peer in peers {
        let device_id = DeviceId::new(peer.peer_id.as_str());
        match member_repo.get(&device_id).await {
            Ok(Some(member)) => {
                let prefs = &member.sync_preferences;
                if !prefs.send_enabled {
                    debug!(
                        peer_id = %peer.peer_id,
                        "Skipping file sync: member send_enabled disabled"
                    );
                    continue;
                }
                if !is_content_type_allowed(ContentTypeCategory::File, &prefs.send_content_types) {
                    debug!(
                        peer_id = %peer.peer_id,
                        "Skipping file sync: file content type disabled for member"
                    );
                    continue;
                }
                result.push(peer.clone());
            }
            Ok(None) => {
                debug!(
                    peer_id = %peer.peer_id,
                    "Skipping file sync: peer is not a space member"
                );
            }
            Err(err) => {
                warn!(
                    peer_id = %peer.peer_id,
                    error = %err,
                    "Failed to load space member for file sync policy; proceeding with sync"
                );
                result.push(peer.clone());
            }
        }
    }
    result
}
