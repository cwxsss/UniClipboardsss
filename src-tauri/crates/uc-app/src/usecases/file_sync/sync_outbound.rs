use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use tracing::{info, info_span, warn, Instrument};
use uuid::Uuid;

use uc_core::ports::{FileTransportPort, PeerDirectoryPort, SettingsPort};
use uc_core::MemberRepositoryPort;

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
    member_repo: Arc<dyn MemberRepositoryPort>,
    peer_directory: Arc<dyn PeerDirectoryPort>,
    file_transport: Arc<dyn FileTransportPort>,
}

impl SyncOutboundFileUseCase {
    pub fn new(
        settings: Arc<dyn SettingsPort>,
        member_repo: Arc<dyn MemberRepositoryPort>,
        peer_directory: Arc<dyn PeerDirectoryPort>,
        file_transport: Arc<dyn FileTransportPort>,
    ) -> Self {
        Self {
            settings,
            member_repo,
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
            let peers =
                ListSendablePeers::new(self.member_repo.clone(), self.peer_directory.clone())
                    .execute()
                    .await
                    .context("Failed to list sendable peers")?;

            // 6. Apply sync policy
            let eligible = apply_file_sync_policy(&self.settings, &self.member_repo, &peers).await;

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
