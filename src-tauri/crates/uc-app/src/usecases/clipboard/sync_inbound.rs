use std::collections::{HashSet, VecDeque};
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::{Mutex as StdMutex, OnceLock};
use std::time::{Duration, Instant};

use crate::usecases::clipboard::clipboard_write_coordinator::{
    ClipboardWriteCoordinator, ClipboardWriteIntent,
};
use crate::usecases::clipboard::ClipboardIntegrationMode;
use anyhow::{Context, Result};
use tokio::sync::Mutex;
use tracing::{debug, error, info, info_span, warn, Instrument};
use tracing_opentelemetry::OpenTelemetrySpanExt;
use uc_core::ids::{EntryId, FormatId, RepresentationId};
use uc_core::network::protocol::{
    BinaryRepresentation, ClipboardBinaryPayload, ClipboardPayloadVersion, MIME_IMAGE_PREFIX,
    MIME_TEXT_HTML, MIME_TEXT_PLAIN, MIME_TEXT_RTF,
};
use uc_observability::otlp::propagator::extract_remote_context;

use uc_core::network::ClipboardMessage;
use uc_core::ports::clipboard::{RepresentationCachePort, SpoolQueuePort};
use uc_core::ports::{
    ClipboardEntryRepositoryPort, ClipboardEventWriterPort, ClipboardRepresentationNormalizerPort,
    DeviceIdentityPort, EncryptionPort, EncryptionSessionPort, SelectRepresentationPolicyPort,
    SettingsPort, TransferCipherPort,
};
use uc_core::{
    ClipboardChangeOrigin, MimeType, ObservedClipboardRepresentation, SystemClipboardSnapshot,
};

const RECENT_ID_TTL: Duration = Duration::from_secs(600);
const RECENT_ID_MAX: usize = 1024;

/// Per-session set of peer IDs that have already triggered the missing-traceparent warning.
/// Subsequent occurrences for the same peer emit debug instead of warn.
static MISSING_TP_PEERS: OnceLock<StdMutex<HashSet<String>>> = OnceLock::new();

/// Emit a rate-limited warning when an inbound message is missing W3C traceparent.
/// First occurrence per peer_id → warn!, subsequent occurrences → debug!.
fn warn_missing_traceparent_once(peer_id: &str) {
    let set = MISSING_TP_PEERS.get_or_init(|| StdMutex::new(HashSet::new()));
    match set.lock() {
        Ok(mut guard) => {
            if guard.insert(peer_id.to_string()) {
                tracing::warn!(
                    peer_id = %peer_id,
                    "inbound clipboard message missing traceparent; falling back to new local trace (legacy peer)"
                );
            } else {
                tracing::debug!(peer_id = %peer_id, "inbound clipboard message missing traceparent (already warned)");
            }
        }
        Err(poisoned) => {
            // Never panic — log and continue
            tracing::warn!(peer_id = %peer_id, error = ?poisoned, "MISSING_TP_PEERS lock poisoned");
        }
    }
}

/// Lightweight transfer linkage returned from inbound apply for file-backed messages.
/// Sufficient for the Tauri layer to emit pending status without re-deriving state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingTransferLinkage {
    pub transfer_id: String,
    pub filename: String,
    pub cached_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InboundApplyOutcome {
    Applied {
        entry_id: Option<EntryId>,
        /// Present only for file-backed clipboard messages.
        pending_transfers: Vec<PendingTransferLinkage>,
    },
    Skipped,
}

pub struct SyncInboundClipboardUseCase {
    mode: ClipboardIntegrationMode,
    /// Coordinator for Full-mode OS clipboard writes (write path).
    /// None for Passive-mode instances that never write to the OS clipboard.
    clipboard_write_coordinator: Option<Arc<ClipboardWriteCoordinator>>,
    /// 仅用于 `is_ready()` 早返回优化。实际解密的密钥获取已下沉到
    /// `transfer_cipher` adapter 内部。Slice 3 会把 session 整组迁移到
    /// `SpaceAccessPort`。
    encryption_session: Arc<dyn EncryptionSessionPort>,
    #[allow(dead_code)]
    encryption: Arc<dyn EncryptionPort>,
    device_identity: Arc<dyn DeviceIdentityPort>,
    transfer_cipher: Arc<dyn TransferCipherPort>,
    capture_clipboard:
        Option<crate::usecases::internal::capture_clipboard::CaptureClipboardUseCase>,
    recent_ids: Mutex<VecDeque<(String, Instant)>>,
    /// Local file cache directory for rewriting remote file paths.
    file_cache_dir: Option<PathBuf>,
    settings: Arc<dyn SettingsPort>,
}

impl SyncInboundClipboardUseCase {
    pub fn new(
        mode: ClipboardIntegrationMode,
        encryption_session: Arc<dyn EncryptionSessionPort>,
        encryption: Arc<dyn EncryptionPort>,
        device_identity: Arc<dyn DeviceIdentityPort>,
        transfer_cipher: Arc<dyn TransferCipherPort>,
        settings: Arc<dyn SettingsPort>,
    ) -> Result<Self> {
        if mode == ClipboardIntegrationMode::Passive {
            return Err(anyhow::anyhow!(
                "invalid inbound sync configuration: Passive mode requires capture dependencies; use with_capture_dependencies"
            ));
        }

        Ok(Self {
            mode,
            clipboard_write_coordinator: None,
            encryption_session,
            encryption,
            device_identity,
            transfer_cipher,
            capture_clipboard: None,
            recent_ids: Mutex::new(VecDeque::new()),
            file_cache_dir: None,
            settings,
        })
    }

    pub fn with_capture_dependencies(
        mode: ClipboardIntegrationMode,
        encryption_session: Arc<dyn EncryptionSessionPort>,
        encryption: Arc<dyn EncryptionPort>,
        device_identity: Arc<dyn DeviceIdentityPort>,
        transfer_cipher: Arc<dyn TransferCipherPort>,
        entry_repo: Arc<dyn ClipboardEntryRepositoryPort>,
        event_writer: Arc<dyn ClipboardEventWriterPort>,
        representation_policy: Arc<dyn SelectRepresentationPolicyPort>,
        representation_normalizer: Arc<dyn ClipboardRepresentationNormalizerPort>,
        representation_cache: Arc<dyn RepresentationCachePort>,
        spool_queue: Arc<dyn SpoolQueuePort>,
        file_cache_dir: Option<PathBuf>,
        settings: Arc<dyn SettingsPort>,
    ) -> Self {
        Self {
            mode,
            clipboard_write_coordinator: None,
            encryption_session,
            encryption,
            device_identity: device_identity.clone(),
            transfer_cipher,
            capture_clipboard: Some(
                crate::usecases::internal::capture_clipboard::CaptureClipboardUseCase::new(
                    entry_repo,
                    event_writer,
                    representation_policy,
                    representation_normalizer,
                    device_identity,
                    representation_cache,
                    spool_queue,
                ),
            ),
            recent_ids: Mutex::new(VecDeque::new()),
            file_cache_dir,
            settings,
        }
    }

    /// Set the coordinator for Full-mode OS clipboard writes.
    ///
    /// Call this after `with_capture_dependencies()` to enable the coordinator path
    /// for Full-mode writes. Passive-mode instances do not need a coordinator.
    pub fn with_clipboard_write_coordinator(
        mut self,
        coordinator: Arc<ClipboardWriteCoordinator>,
    ) -> Self {
        self.clipboard_write_coordinator = Some(coordinator);
        self
    }

    pub async fn execute(
        &self,
        message: ClipboardMessage,
        pre_decoded_plaintext: Option<Vec<u8>>,
    ) -> Result<()> {
        self.execute_with_outcome(message, pre_decoded_plaintext)
            .await
            .map(|_| ())
    }

    pub fn mode(&self) -> ClipboardIntegrationMode {
        self.mode
    }

    async fn prune_recent_ids(&self) {
        let now = Instant::now();
        let mut recent_ids = self.recent_ids.lock().await;
        while let Some((_id, ts)) = recent_ids.front() {
            if now.duration_since(*ts) > RECENT_ID_TTL {
                recent_ids.pop_front();
            } else {
                break;
            }
        }
    }

    async fn rollback_recent_id(&self, message_id: &str) {
        self.prune_recent_ids().await;
        let mut recent_ids = self.recent_ids.lock().await;
        if let Some(index) = recent_ids.iter().position(|(id, _)| id == message_id) {
            recent_ids.remove(index);
        }
        while recent_ids.len() > RECENT_ID_MAX {
            recent_ids.pop_front();
        }
    }

    pub async fn execute_with_outcome(
        &self,
        message: ClipboardMessage,
        pre_decoded_plaintext: Option<Vec<u8>>,
    ) -> Result<InboundApplyOutcome> {
        // Create inbound root span — W3C traceparent from remote peer links this trace
        // to the originating device's clipboard.flow span (cross-device distributed tracing).
        let inbound_span = info_span!(
            "clipboard.flow",
            origin = "inbound_sync",
            message_id = %message.id,
            origin_device_id = %message.origin_device_id,
            payload_version = ?message.payload_version,
        );
        // set_parent returns Err only if the span is already closed; safe to ignore here.
        let _ = inbound_span.set_parent(extract_remote_context(message.traceparent.as_deref()));
        if message.traceparent.is_none() {
            warn_missing_traceparent_once(&message.origin_device_id);
        }

        async move {
            info!(
                mode = ?self.mode,
                allow_os_read = self.mode.allow_os_read(),
                allow_os_write = self.mode.allow_os_write(),
                incoming_content_hash = %message.content_hash,
                "Processing inbound clipboard message"
            );

            // Echo prevention: check before any decryption attempt
            let local_device_id = self.device_identity.current_device_id().to_string();
            if message.origin_device_id == local_device_id {
                debug!("Ignoring inbound clipboard message from local device");
                return Ok(InboundApplyOutcome::Skipped);
            }

            if !self.encryption_session.is_ready().await {
                info!("Skipping inbound apply because encryption session is not ready");
                return Ok(InboundApplyOutcome::Skipped);
            }

            match message.payload_version {
                ClipboardPayloadVersion::V3 => {
                    self.apply_v3_inbound(message, pre_decoded_plaintext).await
                }
                #[allow(unreachable_patterns)]
                other => {
                    error!(version = ?other, "Unsupported inbound payload version — dropping message");
                    Ok(InboundApplyOutcome::Skipped)
                }
            }
        }
        .instrument(inbound_span)
        .await
    }

    /// V3 inbound path: decode V3 binary payload, select highest-priority representation.
    ///
    /// Dedup strategy: uses recent_ids by message.id only.
    /// Unlike V1, we do NOT read the OS clipboard to compare snapshot_hash.
    /// Rationale: V3 carries a multi-representation payload whose snapshot_hash is computed from ALL
    /// representations. The OS clipboard holds only the highest-priority representation written by a
    /// prior receive. Comparing snapshot_hash against the OS clipboard would require re-reading
    /// the OS clipboard and re-computing a hash, which is expensive and fragile (OS clipboard format
    /// may not round-trip exactly). The recent_ids dedup (by message.id, TTL-bounded) is sufficient
    /// to prevent duplicate processing from the same message broadcast to multiple paths.
    async fn apply_v3_inbound(
        &self,
        message: ClipboardMessage,
        pre_decoded_plaintext: Option<Vec<u8>>,
    ) -> Result<InboundApplyOutcome> {
        // V3 dedup: by message.id only (see rationale above)
        self.prune_recent_ids().await;
        {
            let now = Instant::now();
            let mut recent_ids = self.recent_ids.lock().await;
            let is_duplicate = recent_ids.iter().any(|(id, _)| id == &message.id);
            if is_duplicate {
                debug!(
                    message_id = %message.id,
                    dedupe_hit = true,
                    "Skipping V3 inbound: already processed this message id"
                );
                return Ok(InboundApplyOutcome::Skipped);
            }
            recent_ids.push_back((message.id.clone(), now));
            while recent_ids.len() > RECENT_ID_MAX {
                recent_ids.pop_front();
            }
        }

        // Decrypt/decode within inbound.decode span
        let payload = async {
            // Use pre-decoded plaintext from transport layer when available (streaming decode),
            // otherwise fall back to in-process decrypt + decode.
            let plaintext_bytes = match pre_decoded_plaintext {
                Some(bytes) => bytes,
                None => {
                    // Fallback: transport didn't pre-decode — decrypt in-process.
                    // adapter 内部自己 is_ready + 取 master_key + chunked 解密。
                    match self
                        .transfer_cipher
                        .decrypt(&message.encrypted_content)
                        .await
                    {
                        Ok(bytes) => bytes,
                        Err(e) => {
                            error!(
                                error = %e,
                                message_id = %message.id,
                                "V3 inbound: failed to decrypt chunked payload — dropping message"
                            );
                            self.rollback_recent_id(&message.id).await;
                            return Err(anyhow::anyhow!(
                                "V3 inbound: failed to decrypt chunked payload for message {}: {e}",
                                message.id
                            ));
                        }
                    }
                }
            };

            // Decode V3 binary payload
            let v3_payload = ClipboardBinaryPayload::decode_from(&mut Cursor::new(
                &plaintext_bytes,
            ))
            .map_err(|e| {
                anyhow::anyhow!(
                    "V3 inbound: failed to decode binary payload for message {}: {e}",
                    message.id
                )
            })?;

            Ok::<ClipboardBinaryPayload, anyhow::Error>(v3_payload)
        }
        .instrument(info_span!(
            uc_observability::stages::INBOUND_DECODE, // "clipboard.inbound_decode"
            wire_bytes = message.encrypted_content.len(),
        ))
        .await;

        let v3_payload = match payload {
            Ok(p) => p,
            Err(e) => {
                self.rollback_recent_id(&message.id).await;
                return Err(e);
            }
        };

        // Log each representation at debug level
        for rep in &v3_payload.representations {
            debug!(
                format_id = %rep.format_id,
                mime = ?rep.mime,
                size = rep.data.len(),
                "inbound rep"
            );
        }

        async {
            let selected_idx = match select_highest_priority_repr_index(&v3_payload.representations)
            {
                Some(i) => i,
                None => {
                    warn!(message_id = %message.id, "V3 inbound: no representations — dropping");
                    self.rollback_recent_id(&message.id).await;
                    return Ok(InboundApplyOutcome::Skipped);
                }
            };

            // Convert all BinaryRepresentation values into ObservedClipboardRepresentation so that
            // downstream consumers (capture path) can see the full multi-representation snapshot.
            let ClipboardBinaryPayload {
                ts_ms,
                representations: binary_reps,
            } = v3_payload;

            let has_file_transfers = !message.file_transfers.is_empty();

            let all_reps: Vec<ObservedClipboardRepresentation> = binary_reps
                .into_iter()
                .map(|rep| {
                    let mut data = rep.data;

                    // When file_transfers are present, rewrite file path representations
                    // to point to local cache paths ({cache_dir}/{transfer_id}/{filename}).
                    if has_file_transfers {
                        let is_file_rep = rep
                            .mime
                            .as_deref()
                            .map(|m| m == "text/uri-list" || m == "file/uri-list")
                            .unwrap_or(false)
                            || rep.format_id.eq_ignore_ascii_case("files")
                            || rep.format_id.eq_ignore_ascii_case("public.file-url")
                            || rep.format_id.contains("uri-list");

                        if is_file_rep {
                            if let Some(ref cache_dir) = self.file_cache_dir {
                                let local_paths: Vec<String> = message
                                    .file_transfers
                                    .iter()
                                    .map(|ft| {
                                        let path = cache_dir
                                            .join(&ft.transfer_id)
                                            .join(&ft.filename);
                                        url::Url::from_file_path(&path)
                                            .map(|u| u.to_string())
                                            .unwrap_or_else(|_| path.to_string_lossy().to_string())
                                    })
                                    .collect();
                                data = local_paths.join("\n").into_bytes();
                                debug!(
                                    format_id = %rep.format_id,
                                    path_count = local_paths.len(),
                                    "Rewrote file paths to local cache locations"
                                );
                            }
                        }
                    }

                    ObservedClipboardRepresentation::new(
                        RepresentationId::new(),
                        FormatId::from(rep.format_id.as_str()),
                        rep.mime.map(MimeType),
                        data,
                    )
                })
                .collect();

            // Global file_sync_enabled guard for inbound file content
            {
                use uc_core::settings::content_type_filter::{classify_snapshot, ContentTypeCategory};
                let inbound_snapshot = SystemClipboardSnapshot {
                    ts_ms,
                    representations: all_reps.clone(),
                };
                let content_category = classify_snapshot(&inbound_snapshot);
                if content_category == ContentTypeCategory::File {
                    if let Ok(settings) = self.settings.load().await {
                        if !settings.file_sync.file_sync_enabled {
                            info!(message_id = %message.id, "Rejecting inbound file content: file_sync disabled");
                            self.rollback_recent_id(&message.id).await;
                            return Ok(InboundApplyOutcome::Skipped);
                        }
                    }
                }
            }

            // When file_transfers are present, skip OS clipboard write (files don't exist yet)
            // and force DB persistence so the entry exists before file transfer completes.
            if has_file_transfers {
                let capture = match self.capture_clipboard.as_ref() {
                    Some(c) => c,
                    None => {
                        warn!(
                            message_id = %message.id,
                            "V3 inbound with file_transfers: capture dependencies required but missing"
                        );
                        return Ok(InboundApplyOutcome::Applied {
                            entry_id: None,
                            pending_transfers: vec![],
                        });
                    }
                };

                let snapshot_for_capture = SystemClipboardSnapshot {
                    ts_ms,
                    representations: all_reps,
                };

                // Build pending transfer linkage for the Tauri layer.
                let linkage: Vec<PendingTransferLinkage> = message
                    .file_transfers
                    .iter()
                    .map(|ft| {
                        let cached_path = self
                            .file_cache_dir
                            .as_ref()
                            .map(|d| {
                                d.join(&ft.transfer_id)
                                    .join(&ft.filename)
                                    .to_string_lossy()
                                    .to_string()
                            })
                            .unwrap_or_default();
                        PendingTransferLinkage {
                            transfer_id: ft.transfer_id.clone(),
                            filename: ft.filename.clone(),
                            cached_path,
                        }
                    })
                    .collect();

                return match capture
                    .execute_with_origin(
                        snapshot_for_capture,
                        ClipboardChangeOrigin::RemotePush,
                        None, // flow_id deprecated in Phase 87; traceparent used instead
                    )
                    .await
                {
                    Ok(Some(entry_id)) => {
                        info!(
                            message_id = %message.id,
                            entry_id = %entry_id,
                            file_transfer_count = message.file_transfers.len(),
                            "V3 inbound with file_transfers: persisted entry, skipped OS clipboard write"
                        );
                        Ok(InboundApplyOutcome::Applied {
                            entry_id: Some(entry_id),
                            pending_transfers: linkage,
                        })
                    }
                    Ok(None) => {
                        self.rollback_recent_id(&message.id).await;
                        Err(anyhow::anyhow!("V3 file_transfers capture skipped persistence"))
                    }
                    Err(err) => {
                        self.rollback_recent_id(&message.id).await;
                        Err(err).context("V3 inbound with file_transfers: capture failed")
                    }
                };
            }

            // For OS clipboard writes we still restrict to a single highest-priority representation.
            // write_snapshot requires exactly ONE representation (tracked in issue #92).
            let selected_rep = all_reps
                .get(selected_idx)
                .cloned()
                .expect("selected index must be within range");

            let snapshot_for_os = SystemClipboardSnapshot {
                ts_ms,
                representations: vec![selected_rep],
            };

            // For Passive-mode capture we want the full set of representations so that title
            // generation and normalization can choose the most appropriate representation.
            let snapshot_for_capture = SystemClipboardSnapshot {
                ts_ms,
                representations: all_reps,
            };

            // In Full mode: write to OS clipboard via coordinator (handles guard + write + loopback guard)
            if self.mode.allow_os_write() {
                let selected_rep_ref = &snapshot_for_os.representations[0];
                info!(
                    message_id = %message.id,
                    format_id = %selected_rep_ref.format_id,
                    mime = ?selected_rep_ref.mime.as_ref().map(|m| m.as_str()),
                    data_size = selected_rep_ref.bytes.len(),
                    "V3 inbound: writing selected representation to OS clipboard"
                );

                let Some(coordinator) = self.clipboard_write_coordinator.as_ref() else {
                    self.rollback_recent_id(&message.id).await;
                    return Err(anyhow::anyhow!(
                        "clipboard_write_coordinator required for Full-mode OS write"
                    ))
                    .context("V3 inbound: coordinator unavailable");
                };
                if let Err(err) = coordinator
                    .write(snapshot_for_os, ClipboardWriteIntent::RemotePush)
                    .await
                {
                    self.rollback_recent_id(&message.id).await;
                    return Err(err).context("V3 inbound: failed to write snapshot to OS clipboard");
                }

                info!(message_id = %message.id, "V3 inbound clipboard applied");
                return Ok(InboundApplyOutcome::Applied {
                    entry_id: None,
                    pending_transfers: vec![],
                });
            }

            // In Passive mode (allow_os_read = false): persist via capture use case
            if !self.mode.allow_os_read() {
                let capture = self
                    .capture_clipboard
                    .as_ref()
                    .context("V3 passive inbound: capture dependencies required")?;

                // Debug snapshot before handing off to capture use case
                debug!(
                    origin = ?ClipboardChangeOrigin::RemotePush,
                    repr_count = snapshot_for_capture.representations.len(),
                    repr_format_ids = ?snapshot_for_capture
                        .representations
                        .iter()
                        .map(|r| r.format_id.to_string())
                        .collect::<Vec<_>>(),
                    repr_mimes = ?snapshot_for_capture
                        .representations
                        .iter()
                        .map(|r| r.mime.as_ref().map(|m| m.as_str().to_string()))
                        .collect::<Vec<_>>(),
                    "V3 passive snapshot before capture",
                );

                return match capture
                    .execute_with_origin(
                        snapshot_for_capture,
                        ClipboardChangeOrigin::RemotePush,
                        None, // flow_id deprecated in Phase 87; traceparent used instead
                    )
                    .await
                {
                    Ok(Some(entry_id)) => {
                        info!(message_id = %message.id, "V3 inbound clipboard persisted (passive)");
                        Ok(InboundApplyOutcome::Applied {
                            entry_id: Some(entry_id),
                            pending_transfers: vec![],
                        })
                    }
                    Ok(None) => {
                        self.rollback_recent_id(&message.id).await;
                        Err(anyhow::anyhow!("V3 passive capture skipped persistence"))
                    }
                    Err(err) => {
                        self.rollback_recent_id(&message.id).await;
                        Err(err).context("V3 passive inbound: capture failed")
                    }
                };
            }

            // WriteOnly mode — should not happen in practice for inbound
            info!(mode = ?self.mode, "V3 inbound: mode disallows write — skipped");
            Ok(InboundApplyOutcome::Skipped)
        }
        .instrument(info_span!(
            uc_observability::stages::INBOUND_APPLY // "clipboard.inbound_apply"
        ))
        .await
    }
}

/// Returns the index of the highest-priority BinaryRepresentation, or None if empty.
///
/// Priority order (highest first): image/* > text/plain > text/html > text/rtf > other.
/// While write_snapshot is single-representation-only, prefer plain text for textual payloads.
fn select_highest_priority_repr_index(representations: &[BinaryRepresentation]) -> Option<usize> {
    fn fallback_priority_from_format_id(format_id: &str) -> u8 {
        if format_id.eq_ignore_ascii_case("public.png")
            || format_id.eq_ignore_ascii_case("public.jpeg")
            || format_id.eq_ignore_ascii_case("public.jpg")
            || format_id.eq_ignore_ascii_case("public.tiff")
            || format_id.eq_ignore_ascii_case("public.gif")
            || format_id.eq_ignore_ascii_case("public.webp")
            || format_id.eq_ignore_ascii_case("image/png")
            || format_id.eq_ignore_ascii_case("image/jpeg")
            || format_id.eq_ignore_ascii_case("image/jpg")
            || format_id.eq_ignore_ascii_case("image/gif")
            || format_id.eq_ignore_ascii_case("image/webp")
        {
            4
        } else if format_id.eq_ignore_ascii_case("text")
            || format_id.eq_ignore_ascii_case("public.utf8-plain-text")
            || format_id.eq_ignore_ascii_case("public.text")
            || format_id.eq_ignore_ascii_case("NSStringPboardType")
            || format_id.eq_ignore_ascii_case(MIME_TEXT_PLAIN)
        {
            3
        } else if format_id.eq_ignore_ascii_case("public.html")
            || format_id.eq_ignore_ascii_case("html")
            || format_id.eq_ignore_ascii_case(MIME_TEXT_HTML)
        {
            2
        } else if format_id.eq_ignore_ascii_case("public.rtf")
            || format_id.eq_ignore_ascii_case("rtf")
            || format_id.eq_ignore_ascii_case(MIME_TEXT_RTF)
        {
            1
        } else {
            0
        }
    }

    fn priority_from_mime(mime: &str) -> u8 {
        let normalized = mime
            .split(';')
            .next()
            .map(str::trim)
            .unwrap_or_default()
            .to_ascii_lowercase();
        if normalized.starts_with(MIME_IMAGE_PREFIX) {
            4
        } else if normalized == MIME_TEXT_PLAIN {
            3
        } else if normalized == MIME_TEXT_HTML {
            2
        } else if normalized == MIME_TEXT_RTF {
            1
        } else {
            0
        }
    }

    fn priority(rep: &BinaryRepresentation) -> u8 {
        match rep.mime.as_deref() {
            Some(mime) => {
                let mime_priority = priority_from_mime(mime);
                if mime_priority > 0 {
                    mime_priority
                } else {
                    fallback_priority_from_format_id(&rep.format_id)
                }
            }
            None => fallback_priority_from_format_id(&rep.format_id),
        }
    }

    representations
        .iter()
        .enumerate()
        .max_by_key(|(_, r)| priority(r))
        .map(|(i, _)| i)
}
