use std::sync::Arc;

use tracing::{debug, info, warn};
use uc_core::network::DiscoveredPeer;
use uc_core::pairing::resolve_sync_settings;
use uc_core::ports::{PairedDeviceRepositoryPort, SettingsPort};
use uc_core::settings::content_type_filter::{is_content_type_allowed, ContentTypeCategory};
use uc_core::PeerId;

/// Filter peers by sync policy for file content.
///
/// Checks global auto_sync, per-device auto_sync, and file content type toggle.
/// Peers not found in the paired device table are kept (safety fallback).
/// Errors from settings/repo loads are logged and the peer is kept.
pub async fn apply_file_sync_policy(
    settings: &Arc<dyn SettingsPort>,
    paired_device_repo: &Arc<dyn PairedDeviceRepositoryPort>,
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
        let peer_id = PeerId::from(peer.peer_id.as_str());
        match paired_device_repo.get_by_peer_id(&peer_id).await {
            Ok(Some(device)) => {
                if let Some(ref gs) = global_settings {
                    let effective = resolve_sync_settings(&device, &gs.sync);
                    if !effective.auto_sync {
                        debug!(
                            peer_id = %peer.peer_id,
                            "Skipping file sync: auto_sync disabled"
                        );
                        continue;
                    }
                    // Check file content type toggle
                    if !is_content_type_allowed(ContentTypeCategory::File, &effective.content_types)
                    {
                        debug!(
                            peer_id = %peer.peer_id,
                            "Skipping file sync: file content type disabled"
                        );
                        continue;
                    }
                }
                result.push(peer.clone());
            }
            Ok(None) => result.push(peer.clone()),
            Err(err) => {
                warn!(
                    peer_id = %peer.peer_id,
                    error = %err,
                    "Failed to load device; proceeding with sync"
                );
                result.push(peer.clone());
            }
        }
    }
    result
}
