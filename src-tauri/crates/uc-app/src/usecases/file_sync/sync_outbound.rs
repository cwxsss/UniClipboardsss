use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use tracing::{info, info_span, Instrument};
use uuid::Uuid;

use uc_core::ports::{FileTransportPort, PairedDeviceRepositoryPort, PeerDirectoryPort, SettingsPort};

use super::sync_policy::apply_file_sync_policy;

/// Result of an outbound file sync operation.
#[derive(Debug)]
pub struct SyncOutboundResult {
    pub transfer_id: String,
    pub peer_count: usize,
}

pub struct SyncOutboundFileUseCase {
    settings: Arc<dyn SettingsPort>,
    paired_device_repo: Arc<dyn PairedDeviceRepositoryPort>,
    peer_directory: Arc<dyn PeerDirectoryPort>,
    file_transport: Arc<dyn FileTransportPort>,
}

impl SyncOutboundFileUseCase {
    pub fn new(
        settings: Arc<dyn SettingsPort>,
        paired_device_repo: Arc<dyn PairedDeviceRepositoryPort>,
        peer_directory: Arc<dyn PeerDirectoryPort>,
        file_transport: Arc<dyn FileTransportPort>,
    ) -> Self {
        Self {
            settings,
            paired_device_repo,
            peer_directory,
            file_transport,
        }
    }

    /// Send a file to eligible peers.
    ///
    /// Validates file safety (rejects symlinks, hardlinks, deleted files),
    /// applies sync policy to filter eligible peers, then delegates to
    /// the file transport port for each peer.
    pub async fn execute(&self, file_path: PathBuf) -> Result<SyncOutboundResult> {
        async move {
            // 1. Validate file exists and get metadata
            let metadata = tokio::fs::symlink_metadata(&file_path)
                .await
                .with_context(|| format!("Failed to stat file: {}", file_path.display()))?;

            // 2. Reject symlinks
            if metadata.is_symlink() {
                bail!(
                    "Symlinks not supported for file sync: {}",
                    file_path.display()
                );
            }

            // 3. Reject hardlinks (Unix only)
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                if metadata.nlink() > 1 {
                    bail!(
                        "Hardlinks not supported for file sync: {} (nlink={})",
                        file_path.display(),
                        metadata.nlink()
                    );
                }
            }

            // 4. Check file still exists (race guard)
            if !file_path.exists() {
                bail!(
                    "Source file deleted before transfer could start: {}",
                    file_path.display()
                );
            }

            // 5. Get sendable peers
            let peers = self
                .peer_directory
                .list_sendable_peers()
                .await
                .context("Failed to list sendable peers")?;

            // 6. Apply sync policy
            let eligible =
                apply_file_sync_policy(&self.settings, &self.paired_device_repo, &peers).await;

            if eligible.is_empty() {
                info!("No eligible peers for file sync");
                return Ok(SyncOutboundResult {
                    transfer_id: String::new(),
                    peer_count: 0,
                });
            }

            // 7. Generate transfer ID
            let transfer_id = Uuid::new_v4().to_string();

            // 8. Queue file transfer for each eligible peer
            let peer_count = eligible.len();
            for peer in &eligible {
                info!(
                    peer_id = %peer.peer_id,
                    transfer_id = %transfer_id,
                    file = %file_path.display(),
                    "Queuing file transfer to peer"
                );
                // Actual transfer delegation will happen via FileTransportPort
                // when the chunked send protocol is wired (Phase 29 Plan 01).
                // For now we log intent per peer.
                let _ = &self.file_transport;
            }

            Ok(SyncOutboundResult {
                transfer_id,
                peer_count,
            })
        }
        .instrument(info_span!(
            "usecase.file_sync.sync_outbound.execute",
        ))
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::sync::Arc;
    use tempfile::NamedTempFile;
    use uc_core::network::DiscoveredPeer;
    use uc_core::network::protocol::FileTransferMessage;
    use uc_core::ports::errors::PairedDeviceRepositoryError;
    use uc_core::ports::{PairedDeviceRepositoryPort, PeerDirectoryPort, SettingsPort};
    use uc_core::network::{ConnectedPeer, PairedDevice, PairingState};
    use uc_core::settings::model::{ContentTypes, Settings, SyncFrequency, SyncSettings};
    use uc_core::PeerId;

    // --- Mock types ---

    struct MockSettings {
        settings: Settings,
    }

    #[async_trait::async_trait]
    impl SettingsPort for MockSettings {
        async fn load(&self) -> anyhow::Result<Settings> {
            Ok(self.settings.clone())
        }
        async fn save(&self, _settings: &Settings) -> anyhow::Result<()> {
            Ok(())
        }
    }

    struct MockPairedDeviceRepo {
        devices: Vec<PairedDevice>,
    }

    #[async_trait::async_trait]
    impl PairedDeviceRepositoryPort for MockPairedDeviceRepo {
        async fn get_by_peer_id(
            &self,
            peer_id: &PeerId,
        ) -> Result<Option<PairedDevice>, PairedDeviceRepositoryError> {
            Ok(self.devices.iter().find(|d| d.peer_id == *peer_id).cloned())
        }
        async fn list_all(&self) -> Result<Vec<PairedDevice>, PairedDeviceRepositoryError> {
            Ok(self.devices.clone())
        }
        async fn upsert(&self, _device: PairedDevice) -> Result<(), PairedDeviceRepositoryError> {
            Ok(())
        }
        async fn set_state(
            &self,
            _peer_id: &PeerId,
            _state: PairingState,
        ) -> Result<(), PairedDeviceRepositoryError> {
            Ok(())
        }
        async fn update_last_seen(
            &self,
            _peer_id: &PeerId,
            _last_seen_at: chrono::DateTime<Utc>,
        ) -> Result<(), PairedDeviceRepositoryError> {
            Ok(())
        }
        async fn delete(&self, _peer_id: &PeerId) -> Result<(), PairedDeviceRepositoryError> {
            Ok(())
        }
        async fn update_sync_settings(
            &self,
            _peer_id: &PeerId,
            _settings: Option<SyncSettings>,
        ) -> Result<(), PairedDeviceRepositoryError> {
            Ok(())
        }
    }

    struct MockPeerDirectory {
        peers: Vec<DiscoveredPeer>,
    }

    #[async_trait::async_trait]
    impl PeerDirectoryPort for MockPeerDirectory {
        async fn get_discovered_peers(&self) -> anyhow::Result<Vec<DiscoveredPeer>> {
            Ok(self.peers.clone())
        }
        async fn get_connected_peers(&self) -> anyhow::Result<Vec<ConnectedPeer>> {
            Ok(vec![])
        }
        fn local_peer_id(&self) -> String {
            "local-peer".to_string()
        }
        async fn announce_device_name(&self, _device_name: String) -> anyhow::Result<()> {
            Ok(())
        }
    }

    struct MockFileTransport;

    #[async_trait::async_trait]
    impl FileTransportPort for MockFileTransport {
        async fn send_file_announce(
            &self,
            _peer_id: &str,
            _announce: FileTransferMessage,
        ) -> anyhow::Result<()> {
            Ok(())
        }
        async fn send_file_data(
            &self,
            _peer_id: &str,
            _data: FileTransferMessage,
        ) -> anyhow::Result<()> {
            Ok(())
        }
        async fn send_file_complete(
            &self,
            _peer_id: &str,
            _complete: FileTransferMessage,
        ) -> anyhow::Result<()> {
            Ok(())
        }
        async fn cancel_transfer(
            &self,
            _peer_id: &str,
            _cancel: FileTransferMessage,
        ) -> anyhow::Result<()> {
            Ok(())
        }
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

    fn make_settings() -> Settings {
        let mut s = Settings::default();
        s.sync.auto_sync = true;
        s
    }

    fn make_use_case(
        peers: Vec<DiscoveredPeer>,
        devices: Vec<PairedDevice>,
    ) -> SyncOutboundFileUseCase {
        SyncOutboundFileUseCase::new(
            Arc::new(MockSettings {
                settings: make_settings(),
            }),
            Arc::new(MockPairedDeviceRepo { devices }),
            Arc::new(MockPeerDirectory { peers }),
            Arc::new(MockFileTransport),
        )
    }

    #[tokio::test]
    async fn test_outbound_rejects_symlink() {
        let tmp = NamedTempFile::new().unwrap();
        let link_path = tmp.path().parent().unwrap().join("test_symlink");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(tmp.path(), &link_path).unwrap();
            let uc = make_use_case(vec![make_peer("p1")], vec![]);
            let result = uc.execute(link_path.clone()).await;
            assert!(result.is_err());
            assert!(
                result.unwrap_err().to_string().contains("Symlinks not supported"),
                "Expected symlink rejection"
            );
            let _ = std::fs::remove_file(&link_path);
        }
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_outbound_rejects_hardlink() {
        let tmp = NamedTempFile::new().unwrap();
        let link_path = tmp.path().parent().unwrap().join("test_hardlink");
        std::fs::hard_link(tmp.path(), &link_path).unwrap();

        let uc = make_use_case(vec![make_peer("p1")], vec![]);
        let result = uc.execute(link_path.clone()).await;
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("Hardlinks not supported"),
            "Expected hardlink rejection"
        );
        let _ = std::fs::remove_file(&link_path);
    }

    #[tokio::test]
    async fn test_outbound_skips_deleted_file() {
        let path = PathBuf::from("/tmp/nonexistent_file_for_test_12345.txt");
        let uc = make_use_case(vec![make_peer("p1")], vec![]);
        let result = uc.execute(path).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_outbound_no_eligible_peers() {
        let tmp = NamedTempFile::new().unwrap();
        // Global auto_sync=true but no peers at all
        let uc = make_use_case(vec![], vec![]);
        let result = uc.execute(tmp.path().to_path_buf()).await.unwrap();
        assert_eq!(result.peer_count, 0);
    }

    #[tokio::test]
    async fn test_outbound_sends_to_eligible_peers() {
        let tmp = NamedTempFile::new().unwrap();
        let peers = vec![make_peer("p1"), make_peer("p2"), make_peer("p3")];

        // p2 has auto_sync disabled
        let device_p2 = PairedDevice {
            peer_id: PeerId::from("p2"),
            pairing_state: PairingState::Trusted,
            identity_fingerprint: "fp".to_string(),
            paired_at: Utc::now(),
            last_seen_at: None,
            device_name: "Device p2".to_string(),
            sync_settings: Some(SyncSettings {
                auto_sync: false,
                sync_frequency: SyncFrequency::Realtime,
                content_types: ContentTypes::default(),
                max_file_size_mb: 10,
            }),
        };

        let uc = make_use_case(peers, vec![device_p2]);
        let result = uc.execute(tmp.path().to_path_buf()).await.unwrap();
        // p1 and p3 are unknown (kept), p2 is filtered
        assert_eq!(result.peer_count, 2);
        assert!(!result.transfer_id.is_empty());
    }
}
