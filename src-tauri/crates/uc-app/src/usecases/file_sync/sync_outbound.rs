use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use tracing::{info, info_span, warn, Instrument};
use uuid::Uuid;

use uc_core::ports::{
    FileTransportPort, PairedDeviceRepositoryPort, PeerDirectoryPort, SettingsPort,
};

use crate::usecases::pairing::list_sendable_peers::ListSendablePeers;

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
    pub async fn execute(
        &self,
        file_path: PathBuf,
        pre_generated_transfer_id: Option<String>,
    ) -> Result<SyncOutboundResult> {
        async move {
            // 1. Validate file exists and get metadata
            // Note: file_sync_enabled and max_file_size guards have been removed.
            // The OutboundSyncPlanner in runtime.rs guarantees both pre-conditions
            // before constructing FileSyncIntent entries that invoke this use case.
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
            let peers = ListSendablePeers::new(
                self.paired_device_repo.clone(),
                self.peer_directory.clone(),
            )
            .execute()
            .await
            .context("Failed to list sendable peers")?;

            // 6. Apply sync policy
            let eligible =
                apply_file_sync_policy(&self.settings, &self.paired_device_repo, &peers).await;

            if eligible.is_empty() {
                if peers.is_empty() {
                    warn!("No eligible peers for file sync: no peers discovered on network");
                } else {
                    info!(
                        discovered_peer_count = peers.len(),
                        "No eligible peers for file sync: all peers filtered by sync policy"
                    );
                }
                return Ok(SyncOutboundResult {
                    transfer_id: String::new(),
                    peer_count: 0,
                });
            }

            // 7. Use pre-generated transfer ID or generate a new one
            let transfer_id =
                pre_generated_transfer_id.unwrap_or_else(|| Uuid::new_v4().to_string());

            // 8. Queue file transfer for each eligible peer
            let peer_count = eligible.len();
            for peer in &eligible {
                info!(
                    peer_id = %peer.peer_id,
                    transfer_id = %transfer_id,
                    file = %file_path.display(),
                    "Sending file to peer"
                );
                if let Err(e) = self
                    .file_transport
                    .send_file(
                        &peer.peer_id,
                        file_path.clone(),
                        transfer_id.clone(),
                        None, // batch_id — single-file transfer for now
                        None, // batch_total
                    )
                    .await
                {
                    warn!(
                        peer_id = %peer.peer_id,
                        error = %e,
                        "Failed to send file to peer"
                    );
                }
            }

            Ok(SyncOutboundResult {
                transfer_id,
                peer_count,
            })
        }
        .instrument(info_span!("usecase.file_sync.sync_outbound.execute",))
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
    use uc_core::network::{PairedDevice, PairingState};
    use uc_core::settings::model::{ContentTypes, Settings, SyncFrequency, SyncSettings};
    use uc_core::PeerId;

    use crate::test_mocks::{
        MockFileTransport, MockPairedDeviceRepository, MockPeerDirectory, MockSettings,
    };

    fn make_trusted_device(id: &str) -> PairedDevice {
        PairedDevice {
            peer_id: PeerId::from(id),
            pairing_state: PairingState::Trusted,
            identity_fingerprint: "fp".to_string(),
            paired_at: Utc::now(),
            last_seen_at: None,
            device_name: format!("Device {}", id),
            sync_settings: None,
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

    fn build_settings_port(settings: Settings) -> Arc<dyn SettingsPort> {
        let mut mock = MockSettings::new();
        mock.expect_load().returning(move || Ok(settings.clone()));
        mock.expect_save().returning(|_| Ok(()));
        Arc::new(mock)
    }

    fn build_paired_device_repo(devices: Vec<PairedDevice>) -> Arc<dyn PairedDeviceRepositoryPort> {
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

    fn build_peer_directory(peers: Vec<DiscoveredPeer>) -> Arc<dyn PeerDirectoryPort> {
        let mut mock = MockPeerDirectory::new();
        let peers_clone = peers.clone();
        mock.expect_get_discovered_peers()
            .returning(move || Ok(peers_clone.clone()));
        mock.expect_get_connected_peers().returning(|| Ok(vec![]));
        mock.expect_local_peer_id()
            .returning(|| "local-peer".to_string());
        mock.expect_announce_device_name().returning(|_| Ok(()));
        Arc::new(mock)
    }

    fn make_noop_transport() -> Arc<dyn FileTransportPort> {
        let mut mock = MockFileTransport::new();
        mock.expect_send_file().returning(|_, _, _, _, _| Ok(()));
        mock.expect_send_file_announce().returning(|_, _| Ok(()));
        mock.expect_send_file_data().returning(|_, _| Ok(()));
        mock.expect_send_file_complete().returning(|_, _| Ok(()));
        mock.expect_cancel_transfer().returning(|_, _| Ok(()));
        Arc::new(mock)
    }

    fn make_use_case(
        peers: Vec<DiscoveredPeer>,
        devices: Vec<PairedDevice>,
    ) -> SyncOutboundFileUseCase {
        SyncOutboundFileUseCase::new(
            build_settings_port(make_settings()),
            build_paired_device_repo(devices),
            build_peer_directory(peers),
            make_noop_transport(),
        )
    }

    fn make_use_case_with_transport(
        peers: Vec<DiscoveredPeer>,
        devices: Vec<PairedDevice>,
        transport: Arc<dyn FileTransportPort>,
    ) -> SyncOutboundFileUseCase {
        SyncOutboundFileUseCase::new(
            build_settings_port(make_settings()),
            build_paired_device_repo(devices),
            build_peer_directory(peers),
            transport,
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
            let result = uc.execute(link_path.clone(), None).await;
            assert!(result.is_err());
            assert!(
                result
                    .unwrap_err()
                    .to_string()
                    .contains("Symlinks not supported"),
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
        let result = uc.execute(link_path.clone(), None).await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Hardlinks not supported"),
            "Expected hardlink rejection"
        );
        let _ = std::fs::remove_file(&link_path);
    }

    #[tokio::test]
    async fn test_outbound_skips_deleted_file() {
        let path = PathBuf::from("/tmp/nonexistent_file_for_test_12345.txt");
        let uc = make_use_case(vec![make_peer("p1")], vec![]);
        let result = uc.execute(path, None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_outbound_no_eligible_peers() {
        let tmp = NamedTempFile::new().unwrap();
        // Global auto_sync=true but no peers at all
        let uc = make_use_case(vec![], vec![]);
        let result = uc.execute(tmp.path().to_path_buf(), None).await.unwrap();
        assert_eq!(result.peer_count, 0);
    }

    #[tokio::test]
    async fn test_outbound_sends_to_eligible_peers() {
        let tmp = NamedTempFile::new().unwrap();
        let peers = vec![make_peer("p1"), make_peer("p2"), make_peer("p3")];

        // p2 has auto_sync disabled
        let mut device_p2 = make_trusted_device("p2");
        device_p2.sync_settings = Some(SyncSettings {
            auto_sync: false,
            sync_frequency: SyncFrequency::Realtime,
            content_types: ContentTypes::default(),
        });

        let uc = make_use_case(
            peers,
            vec![
                make_trusted_device("p1"),
                device_p2,
                make_trusted_device("p3"),
            ],
        );
        let result = uc.execute(tmp.path().to_path_buf(), None).await.unwrap();
        // p1 and p3 are unknown (kept), p2 is filtered
        assert_eq!(result.peer_count, 2);
        assert!(!result.transfer_id.is_empty());
    }

    #[tokio::test]
    async fn test_outbound_calls_send_file_with_correct_args() {
        let tmp = NamedTempFile::new().unwrap();
        let file_path = tmp.path().to_path_buf();
        let peers = vec![make_peer("p1"), make_peer("p2"), make_peer("p3")];

        let calls: Arc<std::sync::Mutex<Vec<(String, PathBuf, String)>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        let calls_clone = calls.clone();

        let mut transport = MockFileTransport::new();
        transport.expect_send_file().returning(
            move |peer_id, file_path, transfer_id, _batch_id, _batch_total| {
                calls_clone
                    .lock()
                    .unwrap()
                    .push((peer_id.to_string(), file_path, transfer_id));
                Ok(())
            },
        );
        transport
            .expect_send_file_announce()
            .returning(|_, _| Ok(()));
        transport.expect_send_file_data().returning(|_, _| Ok(()));
        transport
            .expect_send_file_complete()
            .returning(|_, _| Ok(()));
        transport.expect_cancel_transfer().returning(|_, _| Ok(()));

        let devices = vec![
            make_trusted_device("p1"),
            make_trusted_device("p2"),
            make_trusted_device("p3"),
        ];
        let uc = make_use_case_with_transport(peers, devices, Arc::new(transport));
        let result = uc.execute(file_path.clone(), None).await.unwrap();

        let recorded = calls.lock().unwrap();
        assert_eq!(
            recorded.len(),
            3,
            "send_file should be called exactly 3 times"
        );

        // Verify peer_ids in order
        assert_eq!(recorded[0].0, "p1");
        assert_eq!(recorded[1].0, "p2");
        assert_eq!(recorded[2].0, "p3");

        // All calls share the same non-empty transfer_id
        let tid = &recorded[0].2;
        assert!(!tid.is_empty(), "transfer_id should not be empty");
        assert_eq!(&recorded[1].2, tid);
        assert_eq!(&recorded[2].2, tid);
        assert_eq!(tid, &result.transfer_id);

        // All calls have the correct file_path
        assert_eq!(recorded[0].1, file_path);
        assert_eq!(recorded[1].1, file_path);
        assert_eq!(recorded[2].1, file_path);
    }

    #[tokio::test]
    async fn test_outbound_partial_failure_does_not_abort() {
        let tmp = NamedTempFile::new().unwrap();
        let peers = vec![make_peer("p1"), make_peer("p2"), make_peer("p3")];

        let attempted: Arc<std::sync::Mutex<Vec<String>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        let attempted_clone = attempted.clone();

        let mut transport = MockFileTransport::new();
        transport.expect_send_file().returning(
            move |peer_id, _file_path, _transfer_id, _batch_id, _batch_total| {
                attempted_clone.lock().unwrap().push(peer_id.to_string());
                if peer_id == "p2" {
                    anyhow::bail!("connection refused");
                }
                Ok(())
            },
        );
        transport
            .expect_send_file_announce()
            .returning(|_, _| Ok(()));
        transport.expect_send_file_data().returning(|_, _| Ok(()));
        transport
            .expect_send_file_complete()
            .returning(|_, _| Ok(()));
        transport.expect_cancel_transfer().returning(|_, _| Ok(()));

        let devices = vec![
            make_trusted_device("p1"),
            make_trusted_device("p2"),
            make_trusted_device("p3"),
        ];
        let uc = make_use_case_with_transport(peers, devices, Arc::new(transport));
        let result = uc.execute(tmp.path().to_path_buf(), None).await;

        // The use case should succeed despite p2 failing
        assert!(
            result.is_ok(),
            "use case should return Ok even with partial failure"
        );
        let result = result.unwrap();

        // peer_count includes the failed peer
        assert_eq!(result.peer_count, 3);

        // All 3 peers were attempted
        let attempted = attempted.lock().unwrap();
        assert_eq!(attempted.len(), 3, "all 3 peers should be attempted");
        assert_eq!(attempted[0], "p1");
        assert_eq!(attempted[1], "p2");
        assert_eq!(attempted[2], "p3");
    }
}
