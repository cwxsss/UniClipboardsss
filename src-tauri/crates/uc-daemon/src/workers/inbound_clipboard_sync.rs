//! Inbound clipboard sync worker for the daemon.
//!
//! Subscribes to incoming clipboard transport frames from peers,
//! parses clipboard protocol frames in the worker boundary, applies them through
//! SyncInboundClipboardUseCase in Full mode, and broadcasts clipboard.new_content
//! WS events when a new entry is persisted.
//!
//! Write-back loop prevention: the shared `clipboard_change_origin` Arc prevents
//! the daemon's own OS clipboard writes from triggering re-capture via ClipboardWatcher.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{bail, Context};
use async_trait::async_trait;
use serde::Serialize;
use tokio::sync::broadcast;
use tokio::time::{sleep, Duration};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use uc_app::runtime::CoreRuntime;
use uc_app::usecases::clipboard::clipboard_write_coordinator::ClipboardWriteCoordinator;
use uc_app::usecases::clipboard::sync_inbound::{InboundApplyOutcome, SyncInboundClipboardUseCase};
use uc_app::usecases::clipboard::ClipboardIntegrationMode;
use uc_app::usecases::file_sync::FileTransferOrchestrator;
use uc_core::network::{ClipboardMessage, ProtocolMessage};
use uc_core::ports::file_transfer_repository::PendingInboundTransfer;
use uc_core::ports::{
    ClipboardInboundMessageSource, ClipboardTransportError, InboundClipboardFrame,
};
use uc_daemon_contract::constants::{ws_event, ws_topic};
use uc_infra::clipboard::TransferPayloadDecryptorAdapter;

use crate::api::types::DaemonWsEvent;
use crate::service::{DaemonService, ServiceHealth};

// ---------------------------------------------------------------------------
// ClipboardNewContentPayload
// ---------------------------------------------------------------------------

/// Payload for the clipboard.new_content WS event.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ClipboardNewContentPayload {
    entry_id: String,
    preview: String,
    origin: String,
}

// ---------------------------------------------------------------------------
// InboundClipboardSyncWorker
// ---------------------------------------------------------------------------

/// Daemon service that receives inbound clipboard messages from peers.
///
/// Mirrors the `run_clipboard_receive_loop` pattern from wiring.rs, adapted for
/// daemon-mode execution as a `DaemonService`.
///
/// Key behaviors:
/// - Subscribes to `ClipboardInboundTransportPort::subscribe_clipboard()` for raw inbound frames
/// - Uses `SyncInboundClipboardUseCase::with_capture_dependencies` in Full mode
/// - Emits `clipboard.new_content` WS event only for `Applied { entry_id: Some(_) }`
/// - Does NOT emit WS event for `Applied { entry_id: None }` — ClipboardWatcher handles it
/// - Does NOT emit WS event for `Skipped` outcomes (echo, dedup, encryption not ready)
pub struct InboundClipboardSyncWorker {
    runtime: Arc<CoreRuntime>,
    event_tx: broadcast::Sender<DaemonWsEvent>,
    /// Coordinator for Full-mode OS clipboard writes and write-back loop prevention.
    /// MUST wrap the SAME Arc<ClipboardChangeOriginPort> instance used by
    /// DaemonClipboardChangeHandler to share guard state.
    clipboard_write_coordinator: Arc<ClipboardWriteCoordinator>,
    file_cache_dir: Option<PathBuf>,
    file_transfer_orchestrator: Option<Arc<FileTransferOrchestrator>>,
}

impl InboundClipboardSyncWorker {
    /// Create a new InboundClipboardSyncWorker.
    ///
    /// The `clipboard_write_coordinator` MUST wrap the same `ClipboardChangeOriginPort`
    /// instance used by `DaemonClipboardChangeHandler` in the daemon composition root.
    /// Sharing the same origin port instance is what prevents write-back loops between
    /// inbound sync and the ClipboardWatcher.
    pub fn new(
        runtime: Arc<CoreRuntime>,
        event_tx: broadcast::Sender<DaemonWsEvent>,
        clipboard_write_coordinator: Arc<ClipboardWriteCoordinator>,
        file_cache_dir: Option<PathBuf>,
        file_transfer_orchestrator: Option<Arc<FileTransferOrchestrator>>,
    ) -> Self {
        Self {
            runtime,
            event_tx,
            clipboard_write_coordinator,
            file_cache_dir,
            file_transfer_orchestrator,
        }
    }

    fn build_sync_inbound_usecase(&self) -> SyncInboundClipboardUseCase {
        let deps = self.runtime.wiring_deps();
        SyncInboundClipboardUseCase::with_capture_dependencies(
            ClipboardIntegrationMode::Full,
            deps.security.encryption_session.clone(),
            deps.security.encryption.clone(),
            deps.device.device_identity.clone(),
            Arc::new(TransferPayloadDecryptorAdapter),
            deps.clipboard.clipboard_entry_repo.clone(),
            deps.clipboard.clipboard_event_repo.clone(),
            deps.clipboard.representation_policy.clone(),
            deps.clipboard.representation_normalizer.clone(),
            deps.clipboard.representation_cache.clone(),
            deps.clipboard.spool_queue.clone(),
            self.file_cache_dir.clone(),
            deps.settings.clone(),
        )
        .with_clipboard_write_coordinator(self.clipboard_write_coordinator.clone())
    }
}

#[async_trait]
impl DaemonService for InboundClipboardSyncWorker {
    fn name(&self) -> &str {
        "inbound-clipboard-sync"
    }

    async fn start(&self, cancel: CancellationToken) -> anyhow::Result<()> {
        info!("inbound clipboard sync starting");
        let usecase = Arc::new(self.build_sync_inbound_usecase());
        let clipboard_network = self
            .runtime
            .wiring_deps()
            .network_ports
            .clipboard_inbound
            .clone();
        let event_tx = self.event_tx.clone();
        let orchestrator = self.file_transfer_orchestrator.clone();

        loop {
            let subscribe_result = tokio::select! {
                _ = cancel.cancelled() => {
                    info!("inbound clipboard sync cancelled");
                    return Ok(());
                }
                result = clipboard_network.subscribe_clipboard() => result,
            };

            match subscribe_result {
                Ok(rx) => {
                    // Run receive loop inline (not spawned) so we block until
                    // the channel closes. subscribe_clipboard() uses take-once
                    // semantics — calling it again after take would always fail
                    // with "clipboard receiver already taken".
                    Self::run_receive_loop(
                        rx,
                        Arc::clone(&usecase),
                        cancel.clone(),
                        event_tx.clone(),
                        orchestrator.clone(),
                    )
                    .await;
                    info!("inbound clipboard receive loop ended, service will exit");
                    return Ok(());
                }
                Err(e) => {
                    warn!(error = %e, "inbound clipboard subscribe failed; retrying in 2s");
                    tokio::select! {
                        _ = cancel.cancelled() => {
                            info!("inbound clipboard sync cancelled during backoff");
                            return Ok(());
                        }
                        _ = sleep(Duration::from_secs(2)) => {}
                    }
                }
            }
        }
    }

    async fn stop(&self) -> anyhow::Result<()> {
        Ok(())
    }

    fn health_check(&self) -> ServiceHealth {
        ServiceHealth::Healthy
    }
}

impl InboundClipboardSyncWorker {
    /// Receive loop: processes messages until the channel closes or cancellation.
    async fn run_receive_loop(
        mut source: Box<dyn ClipboardInboundMessageSource>,
        usecase: Arc<SyncInboundClipboardUseCase>,
        cancel: CancellationToken,
        event_tx: broadcast::Sender<DaemonWsEvent>,
        file_transfer_orchestrator: Option<Arc<FileTransferOrchestrator>>,
    ) {
        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!("inbound clipboard receive loop cancelled");
                    break;
                }
                item = source.recv() => {
                    match item {
                        Ok(frame) => {
                            let source_id = frame.source.0.clone();
                            let message = match Self::parse_clipboard_frame(frame) {
                                Ok(message) => message,
                                Err(err) => {
                                    warn!(error = %err, source = %source_id, "Failed to parse inbound clipboard frame");
                                    continue;
                                }
                            };
                            // Capture origin_device_id before message is consumed by execute_with_outcome.
                            let message_origin_device_id = message.origin_device_id.clone();

                            let outcome = match usecase.execute_with_outcome(message, None).await {
                                Ok(o) => o,
                                Err(e) => {
                                    warn!(error = %e, "Failed to apply inbound clipboard message");
                                    continue;
                                }
                            };

                            // Emit WS event ONLY for Applied { entry_id: Some(_) }.
                            // In Full mode with non-file content, entry_id is None and
                            // ClipboardWatcher fires the event — emitting here would cause double events.
                            // In Passive mode or file transfers: entry_id is Some, must emit.
                            if let InboundApplyOutcome::Applied {
                                entry_id: Some(ref entry_id),
                                ref pending_transfers,
                            } = outcome {
                                // Seed pending transfer records for file transfers.
                                if !pending_transfers.is_empty() {
                                    if let Some(ref orch) = file_transfer_orchestrator {
                                        let now_ms = orch.now_ms();
                                        let db_transfers: Vec<PendingInboundTransfer> =
                                            pending_transfers.iter().map(|t| PendingInboundTransfer {
                                                transfer_id: t.transfer_id.clone(),
                                                entry_id: entry_id.to_string(),
                                                origin_device_id: message_origin_device_id.clone(),
                                                filename: t.filename.clone(),
                                                cached_path: t.cached_path.clone(),
                                                created_at_ms: now_ms,
                                            }).collect();

                                        match orch.tracker().record_pending_from_clipboard(db_transfers).await {
                                            Err(err) => {
                                                warn!(error = %err, "Failed to persist pending transfer records");
                                            }
                                            Ok(()) => {
                                                // Reconcile early completions that arrived before seeding.
                                                let seeded_ids: Vec<String> = pending_transfers
                                                    .iter()
                                                    .map(|t| t.transfer_id.clone())
                                                    .collect();
                                                let early = orch.early_completion_cache().drain_matching(&seeded_ids);
                                                for (tid, info) in &early {
                                                    info!(transfer_id = %tid, "Reconciling early completion after seeding");
                                                    if let Err(err) = orch.tracker().mark_completed(
                                                        tid,
                                                        info.content_hash.as_deref(),
                                                        info.completed_at_ms,
                                                    ).await {
                                                        warn!(error = %err, transfer_id = %tid, "Failed to mark early-completed transfer");
                                                    }
                                                }
                                            }
                                        }

                                        // Emit pending status events to frontend.
                                        orch.emit_pending_status(&entry_id.to_string(), pending_transfers);
                                    }
                                }

                                Self::emit_ws_event(&event_tx, entry_id.to_string());
                            }
                            // InboundApplyOutcome::Applied { entry_id: None } — ClipboardWatcher handles it
                            // InboundApplyOutcome::Skipped — nothing to do
                        }
                        Err(ClipboardTransportError::SubscriptionClosed) => {
                            info!("inbound clipboard receive channel closed");
                            break;
                        }
                        Err(err) => {
                            warn!(error = %err, "inbound clipboard source recv failed; continuing");
                        }
                    }
                }
            }
        }
    }

    fn parse_clipboard_frame(frame: InboundClipboardFrame) -> anyhow::Result<ClipboardMessage> {
        let bytes = frame.frame;
        if bytes.len() < 4 {
            bail!("clipboard frame too short: missing 4-byte JSON prefix");
        }

        let json_len = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;
        if bytes.len() < 4 + json_len {
            bail!(
                "clipboard frame truncated: expected {} JSON bytes, have {}",
                json_len,
                bytes.len().saturating_sub(4)
            );
        }

        let json_bytes = &bytes[4..4 + json_len];
        let trailing = &bytes[4 + json_len..];
        let message = ProtocolMessage::from_bytes(json_bytes)
            .context("failed to decode framed JSON header as ProtocolMessage")?;

        match message {
            ProtocolMessage::Clipboard(mut clipboard_message) => {
                if !trailing.is_empty() {
                    clipboard_message.encrypted_content = trailing.to_vec();
                }
                Ok(clipboard_message)
            }
            other => bail!("expected clipboard frame, got {:?}", other),
        }
    }

    fn emit_ws_event(event_tx: &broadcast::Sender<DaemonWsEvent>, entry_id: String) {
        let payload = ClipboardNewContentPayload {
            entry_id,
            preview: "Remote clipboard content".to_string(),
            origin: "remote".to_string(),
        };
        let payload_value = match serde_json::to_value(payload) {
            Ok(v) => v,
            Err(e) => {
                warn!(error = %e, "Failed to serialize clipboard.new_content payload");
                return;
            }
        };

        let event = DaemonWsEvent {
            topic: ws_topic::CLIPBOARD.to_string(),
            event_type: ws_event::CLIPBOARD_NEW_CONTENT.to_string(),
            session_id: None,
            ts: chrono::Utc::now().timestamp_millis(),
            payload: payload_value,
        };

        if let Err(e) = event_tx.send(event) {
            debug!(error = %e, "No WS subscribers for clipboard.new_content");
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
