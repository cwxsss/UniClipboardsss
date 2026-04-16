use std::sync::Arc;

use tracing::{debug, info, warn};
use uc_core::network::paired_device::resolve_sync_settings;
use uc_core::network::DiscoveredPeer;
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::sync::Arc;
    use uc_core::network::{DiscoveredPeer, PairedDevice, PairingState};
    use uc_core::settings::model::{ContentTypes, Settings, SyncFrequency, SyncSettings};

    use crate::test_mocks::{MockPairedDeviceRepository, MockSettings};

    fn make_settings_port(settings: Option<Settings>) -> Arc<dyn SettingsPort> {
        let mut mock = MockSettings::new();
        match settings {
            Some(s) => {
                mock.expect_load().returning(move || Ok(s.clone()));
            }
            None => {
                mock.expect_load()
                    .returning(|| Err(anyhow::anyhow!("settings load error")));
            }
        }
        mock.expect_save().returning(|_| Ok(()));
        Arc::new(mock)
    }

    fn make_paired_device_repo(devices: Vec<PairedDevice>) -> Arc<dyn PairedDeviceRepositoryPort> {
        let mut mock = MockPairedDeviceRepository::new();
        let devices_for_get = devices.clone();
        mock.expect_get_by_peer_id().returning(move |peer_id| {
            Ok(devices_for_get
                .iter()
                .find(|d| d.peer_id == *peer_id)
                .cloned())
        });
        let devices_for_list = devices.clone();
        mock.expect_list_all()
            .returning(move || Ok(devices_for_list.clone()));
        mock.expect_upsert().returning(|_| Ok(()));
        mock.expect_set_state().returning(|_, _| Ok(()));
        mock.expect_update_last_seen().returning(|_, _| Ok(()));
        mock.expect_delete().returning(|_| Ok(()));
        mock.expect_update_sync_settings().returning(|_, _| Ok(()));
        Arc::new(mock)
    }

    fn make_peer(id: &str) -> DiscoveredPeer {
        DiscoveredPeer {
            peer_id: id.to_string(),
            device_name: Some(format!("Device {}", id)),
            device_id: None,
            addresses: vec![],
            discovered_at: Utc::now(),
            last_seen: Utc::now(),
            is_paired: true,
        }
    }

    fn make_settings_with_auto_sync(auto_sync: bool) -> Settings {
        let mut s = Settings::default();
        s.sync.auto_sync = auto_sync;
        s
    }

    fn make_paired_device(peer_id: &str, sync_settings: Option<SyncSettings>) -> PairedDevice {
        PairedDevice {
            peer_id: PeerId::from(peer_id),
            pairing_state: PairingState::Trusted,
            identity_fingerprint: "fp".to_string(),
            paired_at: Utc::now(),
            last_seen_at: None,
            device_name: format!("Device {}", peer_id),
            sync_settings,
        }
    }

    #[tokio::test]
    async fn test_file_policy_global_off_returns_empty() {
        let settings = make_settings_port(Some(make_settings_with_auto_sync(false)));
        let repo = make_paired_device_repo(vec![]);
        let peers = vec![make_peer("peer-1"), make_peer("peer-2")];

        let result = apply_file_sync_policy(&settings, &repo, &peers).await;
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_file_policy_peer_file_disabled_filtered() {
        let settings = make_settings_port(Some(make_settings_with_auto_sync(true)));
        let device_sync = SyncSettings {
            auto_sync: true,
            sync_frequency: SyncFrequency::Realtime,
            content_types: ContentTypes {
                text: true,
                image: true,
                link: true,
                file: false, // file disabled
                code_snippet: true,
                rich_text: true,
            },
        };
        let repo = make_paired_device_repo(vec![make_paired_device("peer-1", Some(device_sync))]);
        let peers = vec![make_peer("peer-1")];

        let result = apply_file_sync_policy(&settings, &repo, &peers).await;
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_file_policy_peer_auto_sync_disabled_filtered() {
        let settings = make_settings_port(Some(make_settings_with_auto_sync(true)));
        let device_sync = SyncSettings {
            auto_sync: false, // auto_sync off for this device
            sync_frequency: SyncFrequency::Realtime,
            content_types: ContentTypes::default(),
        };
        let repo = make_paired_device_repo(vec![make_paired_device("peer-1", Some(device_sync))]);
        let peers = vec![make_peer("peer-1")];

        let result = apply_file_sync_policy(&settings, &repo, &peers).await;
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_file_policy_settings_error_keeps_all_peers() {
        let settings = make_settings_port(None);
        let repo = make_paired_device_repo(vec![]);
        let peers = vec![make_peer("peer-1"), make_peer("peer-2")];

        let result = apply_file_sync_policy(&settings, &repo, &peers).await;
        assert_eq!(result.len(), 2);
    }

    #[tokio::test]
    async fn test_file_policy_unknown_peer_kept() {
        let settings = make_settings_port(Some(make_settings_with_auto_sync(true)));
        // No devices in repo -- unknown peer
        let repo = make_paired_device_repo(vec![]);
        let peers = vec![make_peer("peer-unknown")];

        let result = apply_file_sync_policy(&settings, &repo, &peers).await;
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].peer_id, "peer-unknown");
    }

    #[tokio::test]
    async fn test_file_policy_global_file_sync_disabled_returns_empty() {
        let mut s = make_settings_with_auto_sync(true);
        s.file_sync.file_sync_enabled = false;
        let settings = make_settings_port(Some(s));
        let repo = make_paired_device_repo(vec![]);
        let peers = vec![make_peer("peer-1"), make_peer("peer-2")];

        let result = apply_file_sync_policy(&settings, &repo, &peers).await;
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_file_policy_global_file_sync_enabled_keeps_eligible() {
        let mut s = make_settings_with_auto_sync(true);
        s.file_sync.file_sync_enabled = true;
        let settings = make_settings_port(Some(s));
        let repo = make_paired_device_repo(vec![]);
        let peers = vec![make_peer("peer-1")];

        let result = apply_file_sync_policy(&settings, &repo, &peers).await;
        assert_eq!(result.len(), 1);
    }
}
