//! Inbound clipboard sync worker (Slice 2 Phase 3 · T8).
//!
//! Subscribes to the iroh-stack `ClipboardSyncFacade::subscribe_inbound_notices`
//! broadcast, feeds each notice into `ApplyInboundClipboardUseCase`
//! (dedup → V3 envelope decode → persist via capture pipeline →
//! `ClipboardWriteCoordinator.write(RemotePush)`), and emits the
//! `clipboard.new_content` WS event when a new entry lands.
//!
//! Write-back loop prevention:
//! `ApplyInboundClipboardUseCase` routes the OS write through the
//! daemon's shared `ClipboardWriteCoordinator`, which registers the
//! 60-second `RemotePush` hash guard + one-shot next-origin override.
//! The `ClipboardWatcherWorker` / `DaemonClipboardChangeHandler` on the
//! same daemon process consume that guard via
//! `ClipboardChangeOriginPort::consume_origin_for_snapshot_or_default`
//! and short-circuit the re-dispatch path — both workers share the
//! same `Arc<dyn ClipboardChangeOriginPort>` instance wired in
//! `entrypoint.rs`.
//!
//! Phase 3 scope:
//! * Text-only — file transfers flagged inside the V3 envelope are not
//!   re-materialised here (Slice 3 blob ticket path).
//! * No per-member filtering — Phase 3 delivers to all online paired
//!   peers (D3 decision; per-member preferences推 Phase 3 follow-up).

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde::Serialize;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use uc_application::facade::{ClipboardSyncFacade, InboundNotice};
use uc_application::{ApplyInboundClipboardUseCase, ApplyInboundInput, ApplyOutcome};
use uc_core::ids::EntryId;
use uc_daemon_contract::constants::{ws_event, ws_topic};

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

/// Daemon service that drains the iroh-stack inbound notice broadcast
/// and feeds each frame through `ApplyInboundClipboardUseCase`.
pub struct InboundClipboardSyncWorker {
    clipboard_sync: Arc<ClipboardSyncFacade>,
    apply_inbound_uc: Arc<ApplyInboundClipboardUseCase>,
    event_tx: broadcast::Sender<DaemonWsEvent>,
}

impl InboundClipboardSyncWorker {
    pub fn new(
        clipboard_sync: Arc<ClipboardSyncFacade>,
        apply_inbound_uc: Arc<ApplyInboundClipboardUseCase>,
        event_tx: broadcast::Sender<DaemonWsEvent>,
    ) -> Self {
        Self {
            clipboard_sync,
            apply_inbound_uc,
            event_tx,
        }
    }

    async fn handle_one(&self, notice: InboundNotice) {
        let input = ApplyInboundInput {
            from_device: notice.from_device,
            content_hash: notice.content_hash,
            plaintext: notice.plaintext,
        };
        match self.apply_inbound_uc.execute(input).await {
            Ok(ApplyOutcome::Applied { entry_id }) => {
                info!(entry_id = %entry_id, "inbound clipboard applied; broadcasting WS event");
                Self::emit_ws_event(&self.event_tx, entry_id);
            }
            Ok(ApplyOutcome::DuplicateSkipped {
                content_hash,
                existing_entry_id,
            }) => {
                debug!(
                    content_hash = %content_hash,
                    existing_entry_id = %existing_entry_id,
                    "inbound dropped: duplicate of existing local entry"
                );
            }
            Ok(ApplyOutcome::DecodeFailed { reason }) => {
                warn!(reason, "inbound dropped: V3 envelope decode failed");
            }
            Err(e) => {
                warn!(error = %e, "inbound apply failed");
            }
        }
    }

    fn emit_ws_event(event_tx: &broadcast::Sender<DaemonWsEvent>, entry_id: EntryId) {
        let payload = ClipboardNewContentPayload {
            entry_id: entry_id.to_string(),
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

#[async_trait]
impl DaemonService for InboundClipboardSyncWorker {
    fn name(&self) -> &str {
        "inbound-clipboard-sync"
    }

    async fn start(&self, cancel: CancellationToken) -> Result<()> {
        info!("inbound clipboard sync starting (iroh)");
        // `subscribe_inbound_notices` spawns a relay task per call that
        // bridges the internal `InboundClipboardNotice` broadcast to a
        // fresh public `InboundNotice` broadcast. We subscribe once at
        // worker start — the relay task lives until the facade itself
        // is dropped, which happens when the `SpaceSetupAssembly` is
        // torn down (shutdown path in `entrypoint.rs`).
        let mut rx = self.clipboard_sync.subscribe_inbound_notices();

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!("inbound clipboard sync cancelled");
                    return Ok(());
                }
                recv = rx.recv() => {
                    match recv {
                        Ok(notice) => self.handle_one(notice).await,
                        Err(broadcast::error::RecvError::Lagged(missed)) => {
                            warn!(
                                missed,
                                "inbound clipboard sync lagged; dropped frames. \
                                 Next frame catches up."
                            );
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            info!("inbound clipboard sync channel closed; worker exiting");
                            return Ok(());
                        }
                    }
                }
            }
        }
    }

    async fn stop(&self) -> Result<()> {
        Ok(())
    }

    fn health_check(&self) -> ServiceHealth {
        ServiceHealth::Healthy
    }
}
