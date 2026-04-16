use std::sync::Arc;

use anyhow::{bail, Context, Result};
use chrono::Utc;
use futures::executor;
use tracing::{debug, info, info_span, warn, Instrument};
use uuid::Uuid;

use uc_core::config::RECEIVE_PLAINTEXT_CAP;
use uc_core::network::paired_device::resolve_sync_settings;
use uc_core::network::protocol::{
    BinaryRepresentation, ClipboardBinaryPayload, ClipboardPayloadVersion,
};
use uc_core::network::{ClipboardMessage, ProtocolMessage};
use uc_core::ports::{
    ClipboardOutboundTransportPort, DeviceIdentityPort, EncryptionSessionPort,
    OutboundClipboardFrame, PairedDeviceRepositoryPort, PeerDirectoryPort, SettingsPort,
    SyncTargetId, SystemClipboardPort, TransferPayloadEncryptorPort,
};

use crate::usecases::pairing::list_sendable_peers::ListSendablePeers;
use uc_core::{ClipboardChangeOrigin, PeerId, SystemClipboardSnapshot};
use uc_observability::otlp::propagator::inject_current_context;

pub struct SyncOutboundClipboardUseCase {
    local_clipboard: Arc<dyn SystemClipboardPort>,
    clipboard_network: Arc<dyn ClipboardOutboundTransportPort>,
    peer_directory: Arc<dyn PeerDirectoryPort>,
    encryption_session: Arc<dyn EncryptionSessionPort>,
    device_identity: Arc<dyn DeviceIdentityPort>,
    settings: Arc<dyn SettingsPort>,
    transfer_encryptor: Arc<dyn TransferPayloadEncryptorPort>,
    paired_device_repo: Arc<dyn PairedDeviceRepositoryPort>,
}

impl SyncOutboundClipboardUseCase {
    pub fn new(
        local_clipboard: Arc<dyn SystemClipboardPort>,
        clipboard_network: Arc<dyn ClipboardOutboundTransportPort>,
        peer_directory: Arc<dyn PeerDirectoryPort>,
        encryption_session: Arc<dyn EncryptionSessionPort>,
        device_identity: Arc<dyn DeviceIdentityPort>,
        settings: Arc<dyn SettingsPort>,
        transfer_encryptor: Arc<dyn TransferPayloadEncryptorPort>,
        paired_device_repo: Arc<dyn PairedDeviceRepositoryPort>,
    ) -> Self {
        Self {
            local_clipboard,
            clipboard_network,
            peer_directory,
            encryption_session,
            device_identity,
            settings,
            transfer_encryptor,
            paired_device_repo,
        }
    }

    /// Filter sendable peers by per-device sync policy (auto_sync + content type).
    ///
    /// Peers not found in the paired device table are kept (safety fallback).
    /// Errors from settings/repo loads are logged and the peer is kept.
    /// The snapshot is classified once and the content type check is applied per-peer.
    pub async fn apply_sync_policy(
        &self,
        peers: &[uc_core::network::DiscoveredPeer],
        snapshot: &SystemClipboardSnapshot,
    ) -> Vec<uc_core::network::DiscoveredPeer> {
        use uc_core::settings::content_type_filter::{
            classify_snapshot, is_content_type_allowed, ContentTypeCategory,
        };

        let global_settings = match self.settings.load().await {
            Ok(s) => Some(s),
            Err(err) => {
                warn!(
                    error_kind = "settings_load_failed",
                    retryable = true,
                    error = %err,
                    "Failed to load global settings for per-device sync policy check; proceeding with all peers"
                );
                None
            }
        };

        // Global master toggle: if auto_sync is off, skip ALL outbound sync.
        if let Some(ref gs) = global_settings {
            if !gs.sync.auto_sync {
                info!("Global auto_sync disabled; returning empty peer list");
                return vec![];
            }
        }

        // Classify the snapshot once, not per-peer
        let content_category = classify_snapshot(snapshot);

        // Global file_sync_enabled guard for file content
        if content_category == ContentTypeCategory::File {
            if let Some(ref gs) = global_settings {
                if !gs.file_sync.file_sync_enabled {
                    info!("Global file_sync disabled; skipping outbound sync for file content");
                    return vec![];
                }
            }
        }

        let mut result = Vec::with_capacity(peers.len());
        for peer in peers {
            let peer_id = PeerId::from(peer.peer_id.as_str());
            match self.paired_device_repo.get_by_peer_id(&peer_id).await {
                Ok(Some(device)) => {
                    if let Some(ref gs) = global_settings {
                        let effective = resolve_sync_settings(&device, &gs.sync);
                        if !effective.auto_sync {
                            debug!(
                                peer_id = %peer.peer_id,
                                "Skipping sync for peer: auto_sync disabled"
                            );
                            continue;
                        }
                        if !is_content_type_allowed(content_category, &effective.content_types) {
                            debug!(
                                peer_id = %peer.peer_id,
                                content_type = ?content_category,
                                "Skipping sync for peer: content type disabled"
                            );
                            continue;
                        }
                    }
                    result.push(peer.clone());
                }
                Ok(None) => {
                    // Peer not in paired_device table yet -- proceed with sync
                    result.push(peer.clone());
                }
                Err(err) => {
                    warn!(
                        error_kind = "paired_device_load_failed",
                        retryable = true,
                        peer_id = %peer.peer_id,
                        error = %err,
                        "Failed to load paired device for sync policy check; proceeding with sync"
                    );
                    result.push(peer.clone());
                }
            }
        }
        result
    }

    pub fn execute_current_snapshot(&self, origin: ClipboardChangeOrigin) -> Result<()> {
        let snapshot = self
            .local_clipboard
            .read_snapshot()
            .context("failed to read current clipboard snapshot for outbound sync")?;
        self.execute(snapshot, origin, None, vec![])
    }

    pub fn execute(
        &self,
        snapshot: SystemClipboardSnapshot,
        origin: ClipboardChangeOrigin,
        origin_flow_id: Option<String>,
        file_transfers: Vec<uc_core::network::protocol::FileTransferMapping>,
    ) -> Result<()> {
        let span = info_span!(
            "usecase.clipboard.sync_outbound.execute",
            origin = ?origin,
            representation_count = snapshot.representations.len(),
        );

        executor::block_on(
            self.execute_async(snapshot, origin, origin_flow_id, file_transfers)
                .instrument(span),
        )
    }

    async fn execute_async(
        &self,
        snapshot: SystemClipboardSnapshot,
        origin: ClipboardChangeOrigin,
        origin_flow_id: Option<String>,
        file_transfers: Vec<uc_core::network::protocol::FileTransferMapping>,
    ) -> Result<()> {
        if origin == ClipboardChangeOrigin::RemotePush {
            debug!(origin = ?origin, "Skipping outbound sync for remote-push origin");
            return Ok(());
        }

        if !self.encryption_session.is_ready().await {
            info!(origin = ?origin, "Skipping outbound sync because encryption session is not ready");
            return Ok(());
        }

        // V3: All representations are sent. Return early if there are none.
        if snapshot.representations.is_empty() {
            debug!("Skipping outbound sync because snapshot has no representations");
            return Ok(());
        }

        let all_sendable_peers =
            ListSendablePeers::new(self.paired_device_repo.clone(), self.peer_directory.clone())
                .execute()
                .await
                .context("failed to load sendable peers for outbound sync")?;

        // Filter out peers whose effective sync policy disallows this content
        let sendable_peers = self.apply_sync_policy(&all_sendable_peers, &snapshot).await;
        let discovered_peer_count = match self.peer_directory.get_discovered_peers().await {
            Ok(peers) => peers.len(),
            Err(err) => {
                warn!(
                    error_kind = "peer_directory_query_failed",
                    retryable = true,
                    error = %err,
                    "get_discovered_peers failed during outbound clipboard peer evaluation"
                );
                0
            }
        };
        if all_sendable_peers.is_empty() {
            warn!(
                discovered_peer_count,
                "Skipping outbound sync: no peers discovered on network"
            );
            return Ok(());
        } else {
            info!(
                discovered_peer_count,
                sendable_peer_count = sendable_peers.len(),
                "Evaluated outbound clipboard sendable peers"
            );
        }
        if sendable_peers.is_empty() {
            info!("Skipping outbound sync: all peers filtered by sync policy");
            return Ok(());
        }

        let message_id = Uuid::new_v4().to_string();

        // Extract content_hash and ts_ms BEFORE consuming representations via into_iter().
        let content_hash = snapshot.snapshot_hash().to_string();
        let ts_ms = snapshot.ts_ms;

        // Build V3 binary payload from snapshot representations.
        let binary_reps: Vec<BinaryRepresentation> = snapshot
            .representations
            .into_iter()
            .map(|rep| BinaryRepresentation {
                format_id: rep.format_id.into_inner(),
                mime: rep.mime.map(|m| m.0),
                data: rep.bytes,
            })
            .collect();

        let v3_payload = ClipboardBinaryPayload {
            ts_ms,
            representations: binary_reps,
        };

        let plaintext_bytes = {
            let _guard = info_span!("clipboard.encode_payload").entered();
            v3_payload
                .encode_to_vec()
                .context("failed to encode V3 clipboard binary payload")?
        };
        let plaintext_bytes_len = plaintext_bytes.len();
        if plaintext_bytes_len > RECEIVE_PLAINTEXT_CAP {
            bail!(
                "plaintext exceeds receive-side cap: {} > {}",
                plaintext_bytes_len,
                RECEIVE_PLAINTEXT_CAP
            );
        }

        let origin_device_id = self.device_identity.current_device_id().to_string();
        let origin_device_name = match self.settings.load().await {
            Ok(settings) => settings
                .general
                .device_name
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| "Unknown Device".to_string()),
            Err(err) => {
                warn!(
                    error_kind = "settings_load_failed",
                    retryable = true,
                    error = %err,
                    "Failed to load settings for outbound sync; using fallback device name"
                );
                "Unknown Device".to_string()
            }
        };

        // Inject the current span's W3C traceparent for cross-device distributed tracing.
        // This MUST run after the outbound flow span is active so Span::current() is non-trivial.
        let traceparent = inject_current_context();

        // Build the JSON header (V3: encrypted payload goes as raw trailing bytes)
        #[allow(deprecated)]
        let clipboard_header = ClipboardMessage {
            id: message_id,
            content_hash,
            encrypted_content: vec![], // V3 binary is NOT in the JSON
            timestamp: Utc::now(),
            origin_device_id,
            origin_device_name,
            payload_version: ClipboardPayloadVersion::V3,
            origin_flow_id,
            traceparent,
            file_transfers,
        };

        // Clone values needed for parallel encryption block (to avoid &self borrow in tokio::join!)
        let transfer_encryptor = self.transfer_encryptor.clone();
        let encryption_session = self.encryption_session.clone();

        // Prepare the framed payload once, then fan it out with cheap frame clones.
        let prepare_future = async move {
            let master_key = async {
                encryption_session
                    .get_master_key()
                    .await
                    .map_err(anyhow::Error::from)
                    .context("failed to access encryption session master key for outbound sync")
            }
            .instrument(info_span!("clipboard.get_master_key"))
            .await?;

            let encrypted_content = {
                let _guard = info_span!("clipboard.encrypt", plaintext_len = plaintext_bytes.len())
                    .entered();
                transfer_encryptor
                    .encrypt(&master_key, &plaintext_bytes)
                    .map_err(|e| {
                        anyhow::anyhow!("failed to encrypt outbound clipboard payload: {e}")
                    })?
            };

            let framed = {
                let _guard = info_span!("clipboard.frame", encrypted_len = encrypted_content.len())
                    .entered();
                ProtocolMessage::Clipboard(clipboard_header)
                    .frame_to_bytes(Some(&encrypted_content))
                    .context("failed to frame outbound V3 clipboard message")?
            };

            Ok::<OutboundClipboardFrame, anyhow::Error>(OutboundClipboardFrame(Arc::from(
                framed.into_boxed_slice(),
            )))
        }
        .instrument(info_span!(
            uc_observability::stages::OUTBOUND_PREPARE, // "clipboard.outbound_prepare"
            raw_bytes_len = plaintext_bytes_len,
        ));

        let outbound_frame = if tokio::runtime::Handle::try_current().is_ok() {
            let prepare_handle = tokio::spawn(prepare_future);
            let prepare_result = prepare_handle
                .await
                .map_err(anyhow::Error::from)
                .context("outbound prepare task join failed")?;
            prepare_result?
        } else {
            prepare_future.await?
        };

        let mut send_failures = Vec::new();
        let mut sent_count = 0usize;

        for peer in &sendable_peers {
            if let Err(err) = async {
                self.clipboard_network
                    .send_clipboard(&SyncTargetId(peer.peer_id.clone()), outbound_frame.clone())
                    .await
            }
            .instrument(
                info_span!(uc_observability::stages::OUTBOUND_SEND, peer_id = %peer.peer_id),
            ) // "clipboard.outbound_send"
            .await
            {
                warn!(
                    error_kind = "peer_send_failed",
                    retryable = true,
                    peer_id = %peer.peer_id,
                    peer_address_count = peer.addresses.len(),
                    error = %err,
                    "failed to send outbound clipboard message to peer; continuing best-effort fanout"
                );
                send_failures.push(format!("{}: {}", peer.peer_id, err));
                continue;
            }

            sent_count += 1;
        }

        if sent_count == 0 {
            return Err(anyhow::anyhow!(
                "outbound clipboard fanout failed: 0 sent, {} failed ({})",
                send_failures.len(),
                send_failures.join(" | ")
            ));
        }

        if !send_failures.is_empty() {
            let failure_count = send_failures.len();
            warn!(
                sent_count,
                failure_count,
                "outbound clipboard fanout partially failed after best-effort retries"
            );
            return Err(anyhow::anyhow!(
                "outbound clipboard fanout partially failed: {sent_count} sent, {failure_count} failed ({})",
                send_failures.join(" | ")
            ));
        }

        info!(sent_count, "Outbound clipboard sync sent to sendable peers");
        Ok(())
    }
}
