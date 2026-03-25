//! Real clipboard watcher service for the daemon.
//!
//! Monitors OS clipboard changes via clipboard_rs, persists captured entries
//! via CaptureClipboardUseCase, and broadcasts clipboard.new_content WS events.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde::Serialize;
use tokio::sync::{broadcast, mpsc};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use clipboard_rs::{ClipboardWatcherContext, ClipboardWatcher as RSClipboardWatcher};
use uc_app::runtime::CoreRuntime;
use uc_app::usecases::internal::capture_clipboard::CaptureClipboardUseCase;
use uc_core::network::daemon_api_strings::{ws_event, ws_topic};
use uc_core::ports::ClipboardChangeHandler;
use uc_core::{ClipboardChangeOrigin, SystemClipboardSnapshot};
use uc_platform::clipboard::watcher::ClipboardWatcher;
use uc_platform::ipc::PlatformEvent;
use uc_platform::runtime::event_bus::PlatformEventSender;

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
// DaemonClipboardChangeHandler
// ---------------------------------------------------------------------------

/// Clipboard change handler for the daemon.
///
/// Invoked by ClipboardWatcherWorker for each de-duplicated clipboard change.
/// Persists entries via CaptureClipboardUseCase and broadcasts a
/// clipboard.new_content WS event through the shared event broadcast channel.
pub struct DaemonClipboardChangeHandler {
    runtime: Arc<CoreRuntime>,
    event_tx: broadcast::Sender<DaemonWsEvent>,
}

impl DaemonClipboardChangeHandler {
    pub fn new(runtime: Arc<CoreRuntime>, event_tx: broadcast::Sender<DaemonWsEvent>) -> Self {
        Self { runtime, event_tx }
    }

    fn build_capture_use_case(&self) -> CaptureClipboardUseCase {
        let deps = self.runtime.wiring_deps();
        CaptureClipboardUseCase::new(
            deps.clipboard.clipboard_entry_repo.clone(),
            deps.clipboard.clipboard_event_repo.clone(),
            deps.clipboard.representation_policy.clone(),
            deps.clipboard.representation_normalizer.clone(),
            deps.device.device_identity.clone(),
            deps.clipboard.representation_cache.clone(),
            deps.clipboard.spool_queue.clone(),
        )
    }
}

#[async_trait]
impl ClipboardChangeHandler for DaemonClipboardChangeHandler {
    async fn on_clipboard_changed(&self, snapshot: SystemClipboardSnapshot) -> Result<()> {
        let usecase = self.build_capture_use_case();

        // For initial local-capture-only plan, use LocalCapture directly.
        // Plan 03 will add shared ClipboardChangeOriginPort for write-back loop prevention.
        match usecase
            .execute_with_origin(snapshot, ClipboardChangeOrigin::LocalCapture)
            .await
        {
            Ok(Some(entry_id)) => {
                debug!(entry_id = %entry_id, "Daemon clipboard capture succeeded");

                let payload = ClipboardNewContentPayload {
                    entry_id: entry_id.to_string(),
                    preview: "New clipboard content".to_string(),
                    origin: "local".to_string(),
                };
                let payload_value = match serde_json::to_value(payload) {
                    Ok(v) => v,
                    Err(e) => {
                        warn!(error = %e, "Failed to serialize clipboard.new_content payload");
                        return Ok(());
                    }
                };

                let event = DaemonWsEvent {
                    topic: ws_topic::CLIPBOARD.to_string(),
                    event_type: ws_event::CLIPBOARD_NEW_CONTENT.to_string(),
                    session_id: None,
                    ts: chrono::Utc::now().timestamp_millis(),
                    payload: payload_value,
                };

                // broadcast::send returns Err only when there are no receivers;
                // that's expected when no WS clients are connected — log at debug.
                if let Err(e) = self.event_tx.send(event) {
                    debug!(error = %e, "No WS subscribers for clipboard.new_content");
                }
            }
            Ok(None) => {
                // Dedup at use-case level (e.g. unsupported representation) — skip silently.
                debug!("Clipboard capture returned None (dedup or unsupported)");
            }
            Err(e) => {
                warn!(error = %e, "Daemon clipboard capture failed");
            }
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// ClipboardWatcherWorker
// ---------------------------------------------------------------------------

/// Daemon service that monitors OS clipboard changes.
///
/// Uses clipboard_rs::ClipboardWatcherContext (via spawn_blocking) and
/// uc_platform::ClipboardWatcher for dedup. Captured snapshots are forwarded
/// to DaemonClipboardChangeHandler which persists and broadcasts WS events.
pub struct ClipboardWatcherWorker {
    local_clipboard: Arc<dyn uc_core::ports::SystemClipboardPort>,
    change_handler: Arc<DaemonClipboardChangeHandler>,
}

impl ClipboardWatcherWorker {
    pub fn new(
        local_clipboard: Arc<dyn uc_core::ports::SystemClipboardPort>,
        change_handler: Arc<DaemonClipboardChangeHandler>,
    ) -> Self {
        Self {
            local_clipboard,
            change_handler,
        }
    }
}

#[async_trait]
impl DaemonService for ClipboardWatcherWorker {
    fn name(&self) -> &str {
        "clipboard-watcher"
    }

    async fn start(&self, cancel: CancellationToken) -> anyhow::Result<()> {
        info!("clipboard watcher starting");

        // Channel to receive platform events from the blocking watcher thread.
        let (platform_tx, mut platform_rx): (PlatformEventSender, _) = mpsc::channel(64);

        // Create the uc-platform ClipboardWatcher (handles dedup logic).
        let handler = ClipboardWatcher::new(self.local_clipboard.clone(), platform_tx);

        // Create clipboard_rs watcher context and register our handler.
        let mut watcher_ctx = ClipboardWatcherContext::new()
            .map_err(|e| anyhow::anyhow!("Failed to create ClipboardWatcherContext: {}", e))?;

        // get_shutdown_channel() requires adding the handler first.
        let shutdown = watcher_ctx.add_handler(handler).get_shutdown_channel();

        // Run the blocking watcher loop on a dedicated thread (per D-07).
        // WatcherShutdown is NOT Send, so we create and consume it within this
        // same async fn — it never crosses an await boundary to another task.
        tokio::task::spawn_blocking(move || {
            info!("clipboard watcher thread started");
            watcher_ctx.start_watch();
            info!("clipboard watcher thread stopped");
        });

        let change_handler = self.change_handler.clone();

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!("clipboard watcher cancellation received");
                    // Signal the blocking watcher thread to stop (per D-08).
                    shutdown.stop();
                    break;
                }
                event = platform_rx.recv() => {
                    match event {
                        Some(PlatformEvent::ClipboardChanged { snapshot }) => {
                            if snapshot.is_empty() {
                                debug!("Clipboard changed event had no representations; skipping");
                                continue;
                            }
                            if let Err(e) = change_handler.on_clipboard_changed(snapshot).await {
                                warn!(error = %e, "Failed to handle clipboard change in daemon");
                            }
                        }
                        Some(_) => {
                            // Other PlatformEvent variants are not relevant here.
                        }
                        None => {
                            // Channel closed (watcher thread exited).
                            info!("Clipboard watcher platform channel closed");
                            break;
                        }
                    }
                }
            }
        }

        info!("clipboard watcher stopped");
        Ok(())
    }

    async fn stop(&self) -> anyhow::Result<()> {
        // Cancellation is handled via CancellationToken in start().
        Ok(())
    }

    fn health_check(&self) -> ServiceHealth {
        ServiceHealth::Healthy
    }
}
