//! Real clipboard watcher service for the daemon.
//!
//! Monitors OS clipboard changes via clipboard_rs, persists captured entries
//! via CaptureClipboardUseCase, and broadcasts clipboard.new_content WS events.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde::Serialize;
use tokio::sync::{broadcast, mpsc};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, instrument, warn, Instrument};

use clipboard_rs::{ClipboardWatcher as RSClipboardWatcher, ClipboardWatcherContext};
use uc_app::runtime::CoreRuntime;
use uc_app::usecases::internal::capture_clipboard::CaptureClipboardUseCase;
use uc_app::usecases::sync_planner::{FileCandidate, FileSyncIntent, OutboundSyncPlanner};
use uc_app::usecases::CoreUseCases;
use uc_application::facade::{BlobTransferFacade, ClipboardSyncFacade, PublishBlobCommand};
use uc_application::V3BlobRef;
use uc_core::ids::EntryId;
use uc_core::ports::{ClipboardChangeHandler, ClipboardChangeOriginPort};
use uc_core::{ClipboardChangeOrigin, SystemClipboardSnapshot};
use uc_daemon_contract::constants::{ws_event, ws_topic};

use crate::search::projection::SearchProjectionBuilder;
use uc_observability::FlowId;
use uc_platform::clipboard::watcher::{ClipboardWatcher, PlatformEvent, PlatformEventSender};

use crate::api::types::DaemonWsEvent;
use crate::service::{DaemonService, ServiceHealth};

// ---------------------------------------------------------------------------
// File path extraction helper
// ---------------------------------------------------------------------------

/// On macOS, attempt to resolve APFS file references (e.g. `/.file/id=...`) to real paths.
/// Currently a no-op stub — APFS resolution deferred to a future phase.
#[cfg(target_os = "macos")]
fn resolve_apfs_file_reference(_path: &std::path::Path) -> Option<PathBuf> {
    None
}

/// Extract file paths from a clipboard snapshot's representations.
///
/// Looks for `text/uri-list` or `file/uri-list` MIME types, or `files` / `public.file-url`
/// format IDs, and parses `file://` URIs into `PathBuf`s.
fn extract_file_paths_from_snapshot(snapshot: &SystemClipboardSnapshot) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for rep in &snapshot.representations {
        let is_file_rep = rep
            .mime
            .as_ref()
            .map(|m| {
                let s = m.as_str();
                s.eq_ignore_ascii_case("text/uri-list") || s.eq_ignore_ascii_case("file/uri-list")
            })
            .unwrap_or(false)
            || rep.format_id.eq_ignore_ascii_case("files")
            || rep.format_id.eq_ignore_ascii_case("public.file-url");

        if !is_file_rep {
            continue;
        }

        // Parse bytes as UTF-8 text containing file:// URIs (one per line)
        let text = match std::str::from_utf8(&rep.bytes) {
            Ok(t) => t,
            Err(_) => continue,
        };

        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Ok(url) = url::Url::parse(line) {
                if url.scheme() == "file" {
                    if let Ok(path) = url.to_file_path() {
                        // On macOS, resolve APFS file references (/.file/id=...) to real paths
                        #[cfg(target_os = "macos")]
                        let resolved = resolve_apfs_file_reference(&path).unwrap_or(path);
                        #[cfg(not(target_os = "macos"))]
                        let resolved = path;
                        paths.push(resolved);
                    }
                }
            }
        }
    }
    // Safety net: deduplicate in case multiple representations contain the same path
    paths.sort();
    paths.dedup();
    paths
}

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
///
/// The shared `clipboard_change_origin` instance is used to detect whether a
/// clipboard change was triggered by daemon inbound sync (RemotePush) or by
/// the local user (LocalCapture), preventing write-back loops.
pub struct DaemonClipboardChangeHandler {
    runtime: Arc<CoreRuntime>,
    event_tx: broadcast::Sender<DaemonWsEvent>,
    clipboard_change_origin: Arc<dyn ClipboardChangeOriginPort>,
    /// Gate that controls whether clipboard capture is active.
    /// When false, clipboard change events are silently dropped.
    /// Used in `--gui-managed` mode to defer clipboard capture until
    /// the GUI user explicitly unlocks the app.
    capture_gate: Arc<AtomicBool>,
    /// Slice 2 Phase 3 · T7 — iroh-stack clipboard sync. Replaces the
    /// deprecated libp2p `SyncOutboundClipboardUseCase` path. Wired from
    /// `DaemonBootstrapContext.clipboard_sync_facade` in `entrypoint.rs`.
    clipboard_sync: Arc<ClipboardSyncFacade>,
    /// Slice 3 Phase 3:发布本机文件为 blob refs,再随剪贴板消息广播。
    blob_transfer: Arc<BlobTransferFacade>,
}

impl DaemonClipboardChangeHandler {
    pub fn new(
        runtime: Arc<CoreRuntime>,
        event_tx: broadcast::Sender<DaemonWsEvent>,
        clipboard_change_origin: Arc<dyn ClipboardChangeOriginPort>,
        capture_gate: Arc<AtomicBool>,
        clipboard_sync: Arc<ClipboardSyncFacade>,
        blob_transfer: Arc<BlobTransferFacade>,
    ) -> Self {
        Self {
            runtime,
            event_tx,
            clipboard_change_origin,
            capture_gate,
            clipboard_sync,
            blob_transfer,
        }
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
    #[instrument(
        name = "daemon.on_clipboard_changed",
        level = "info",
        skip(self, snapshot),
        fields(trace_id = %FlowId::generate())
    )]
    async fn on_clipboard_changed(&self, snapshot: SystemClipboardSnapshot) -> Result<()> {
        if !self.capture_gate.load(Ordering::Relaxed) {
            debug!("Clipboard capture gate closed, skipping clipboard change");
            return Ok(());
        }
        let usecase = self.build_capture_use_case();
        let flow_id = FlowId::generate().to_string();

        // 1. Compute snapshot hash for write-back loop prevention.
        let origin_guard_key = snapshot.origin_guard_key();

        // 2. Check if this clipboard change was triggered by daemon inbound sync (RemotePush)
        //    or by the local user (LocalCapture). This prevents re-capturing content that
        //    the daemon itself wrote to the OS clipboard during inbound sync.
        let origin = self
            .clipboard_change_origin
            .consume_origin_for_snapshot_or_default(
                &origin_guard_key,
                ClipboardChangeOrigin::LocalCapture,
            )
            .await;

        debug!(
            origin_guard_key = %origin_guard_key,
            rep_count = snapshot.representations.len(),
            origin = ?origin,
            flow_id = %flow_id,
            "daemon clipboard watcher resolved origin for snapshot"
        );

        // 3. Determine the origin string for the WS event payload.
        let origin_str = match origin {
            ClipboardChangeOrigin::LocalCapture | ClipboardChangeOrigin::LocalRestore => "local",
            ClipboardChangeOrigin::RemotePush => "remote",
        };

        // 4. Clone snapshot BEFORE execute_with_origin which takes ownership.
        let outbound_snapshot = snapshot.clone();

        match usecase
            .execute_with_origin(snapshot, origin, Some(flow_id.clone()))
            .await
        {
            Ok(Some(entry_id)) => {
                debug!(entry_id = %entry_id, ?origin, "Daemon clipboard capture succeeded");

                let payload = ClipboardNewContentPayload {
                    entry_id: entry_id.to_string(),
                    preview: "New clipboard content".to_string(),
                    origin: origin_str.to_string(),
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

                // --- Search indexing ---
                // Build search document for the captured entry using the projection builder.
                // We use a clone of the snapshot made before execute_with_origin consumed it.
                {
                    let search_span =
                        tracing::info_span!("search.live_index", entry_id = %entry_id);
                    let deps = self.runtime.wiring_deps();
                    async {
                    // Fetch the persisted ClipboardEntry to get event_id and timestamps
                    match deps.clipboard.clipboard_entry_repo.get_entry(&entry_id).await {
                        Ok(Some(entry)) => {
                            // Compute the selection policy result for the live snapshot
                            let selection_result =
                                deps.clipboard.representation_policy.select(&outbound_snapshot);
                            match selection_result {
                                Ok(selection) => {
                                    // Build SearchPipelineInput via the single projection authority
                                    match SearchProjectionBuilder::build_from_capture(
                                        &entry,
                                        &outbound_snapshot,
                                        &selection,
                                    ) {
                                        Some(pipeline_input) => {
                                            // Derive search key and build postings
                                            match deps.search.search_key_derivation.derive_search_key().await {
                                                Ok(search_key) => {
                                                    match deps.search.search_pipeline.build(&pipeline_input, &search_key) {
                                                        Ok((document, postings)) => {
                                                            if postings.is_empty() {
                                                                debug!(
                                                                    entry_id = %entry_id,
                                                                    "search: no postings generated, skipping index"
                                                                );
                                                            } else {
                                                                let uc = CoreUseCases::new(self.runtime.as_ref());
                                                                if let Err(e) = uc.index_clipboard_entry().execute(document, postings).await {
                                                                    warn!(
                                                                        error = %e,
                                                                        entry_id = %entry_id,
                                                                        "search: index_clipboard_entry failed"
                                                                    );
                                                                }
                                                            }
                                                        }
                                                        Err(e) => {
                                                            warn!(
                                                                error = %e,
                                                                entry_id = %entry_id,
                                                                "search: pipeline.build failed"
                                                            );
                                                        }
                                                    }
                                                }
                                                Err(e) => {
                                                    debug!(
                                                        error = %e,
                                                        entry_id = %entry_id,
                                                        "search: key derivation failed (session likely locked)"
                                                    );
                                                }
                                            }
                                        }
                                        None => {
                                            debug!(
                                                entry_id = %entry_id,
                                                "search: no searchable content in capture, skipping"
                                            );
                                        }
                                    }
                                }
                                Err(e) => {
                                    debug!(
                                        error = %e,
                                        entry_id = %entry_id,
                                        "search: representation policy selection failed, skipping"
                                    );
                                }
                            }
                        }
                        Ok(None) => {
                            debug!(
                                entry_id = %entry_id,
                                "search: captured entry not found in repo, skipping"
                            );
                        }
                        Err(e) => {
                            warn!(
                                error = %e,
                                entry_id = %entry_id,
                                "search: failed to fetch entry from repo"
                            );
                        }
                    }
                    }.instrument(search_span).await;
                }

                // --- Outbound sync dispatch (mirrors AppRuntime::on_clipboard_changed) ---

                // Extract file paths only for LocalCapture (RemotePush must not re-sync).
                let resolved_paths = if origin == ClipboardChangeOrigin::LocalCapture {
                    extract_file_paths_from_snapshot(&outbound_snapshot)
                } else {
                    Vec::new()
                };

                // Capture count BEFORE metadata filtering for all_files_excluded detection.
                let extracted_paths_count = resolved_paths.len();

                // Build FileCandidate vec by reading metadata per resolved path.
                let file_candidates: Vec<FileCandidate> = resolved_paths
                    .into_iter()
                    .filter_map(|path| {
                        match std::fs::metadata(&path) {
                            Ok(meta) => Some(FileCandidate { path, size: meta.len() }),
                            Err(e) => {
                                warn!(error = %e, file = %path.display(), "Excluding file from sync: metadata read failed");
                                None
                            }
                        }
                    })
                    .collect();

                // Delegate sync policy to OutboundSyncPlanner.
                let deps = self.runtime.wiring_deps();
                let planner = OutboundSyncPlanner::new(deps.settings.clone());
                let plan = planner
                    .plan(
                        outbound_snapshot,
                        origin,
                        file_candidates,
                        extracted_paths_count,
                    )
                    .await;

                // Slice 2 Phase 3 · T7 — route outbound clipboard dispatch through the
                // iroh-stack `ClipboardSyncFacade::dispatch_snapshot`. The legacy
                // libp2p `SyncOutboundClipboardUseCase` path is retired; per-member
                // sync preferences + content_type filters are not yet migrated
                // (D3 decision: Phase 3 follow-up). The `OutboundSyncPlanner` still
                // gates on global `auto_sync` + LocalCapture origin, so that layer
                // continues to work.
                //
                if let Some(clipboard_intent) = plan.clipboard {
                    let clipboard_sync = Arc::clone(&self.clipboard_sync);
                    let blob_transfer = Arc::clone(&self.blob_transfer);
                    let snapshot_for_dispatch = clipboard_intent.snapshot;
                    let file_intents = plan.files;
                    let entry_id_for_blob = entry_id.clone();
                    tokio::spawn(
                        async move {
                            let blob_refs = match publish_file_blob_refs(
                                &blob_transfer,
                                &file_intents,
                                &entry_id_for_blob,
                            )
                            .await
                            {
                                Ok(refs) => refs,
                                Err(e) => {
                                    warn!(
                                        error = %e,
                                        "Daemon outbound file blob publish failed; skipping clipboard dispatch"
                                    );
                                    return;
                                }
                            };

                            let dispatch_result = if blob_refs.is_empty() {
                                clipboard_sync
                                    .dispatch_snapshot(snapshot_for_dispatch, origin)
                                    .await
                            } else {
                                clipboard_sync
                                    .dispatch_snapshot_with_blob_refs(
                                        snapshot_for_dispatch,
                                        blob_refs,
                                        origin,
                                    )
                                    .await
                            };

                            match dispatch_result {
                                Ok(outcome) => info!(
                                    accepted = outcome.total_accepted,
                                    duplicate = outcome.total_duplicate,
                                    offline = outcome.total_offline,
                                    errored = outcome.total_errored,
                                    "Daemon outbound clipboard sync (iroh) completed"
                                ),
                                Err(e) => warn!(
                                    error = %e,
                                    "Daemon outbound clipboard sync (iroh) failed"
                                ),
                            }
                        }
                        .in_current_span(),
                    );
                }
            }
            Ok(None) => {
                // Dedup at use-case level (e.g. unsupported representation) — skip silently.
                debug!(origin_guard_key = %origin_guard_key, ?origin, "Clipboard capture returned None");
            }
            Err(e) => {
                warn!(error = %e, origin_guard_key = %origin_guard_key, ?origin, "Daemon clipboard capture failed");
            }
        }

        Ok(())
    }
}

async fn publish_file_blob_refs(
    blob_transfer: &BlobTransferFacade,
    files: &[FileSyncIntent],
    entry_id: &EntryId,
) -> anyhow::Result<Vec<V3BlobRef>> {
    let mut blob_refs = Vec::with_capacity(files.len());

    for file in files {
        let plaintext = tokio::fs::read(&file.path).await?;
        let size_bytes = plaintext.len() as u64;
        let result = blob_transfer
            .publish_blob(PublishBlobCommand {
                plaintext: plaintext.into(),
                entry_id: Some(entry_id.clone()),
            })
            .await?;

        blob_refs.push(V3BlobRef {
            ticket: result.ticket,
            entry_id: result.entry_id,
            filename: Some(file.filename.clone()).filter(|name| !name.is_empty()),
            mime: None,
            size_bytes,
        });
    }

    Ok(blob_refs)
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
