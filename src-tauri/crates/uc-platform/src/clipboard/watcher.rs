use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, warn};

use clipboard_rs::ClipboardHandler;

use uc_core::clipboard::SystemClipboardSnapshot;
use uc_core::ports::SystemClipboardPort;

/// Minimal platform event type retained for clipboard watcher channel.
/// Full PlatformEvent (ipc module) was removed in Phase 65; only the
/// ClipboardChanged variant is needed by the watcher.
#[derive(Debug, Clone)]
pub enum PlatformEvent {
    /// Local clipboard content changed.
    ClipboardChanged { snapshot: SystemClipboardSnapshot },
}

/// Channel sender for platform events emitted by the clipboard watcher.
pub type PlatformEventSender = tokio::sync::mpsc::Sender<PlatformEvent>;

/// Time window to suppress rapid consecutive file clipboard events.
/// macOS fires multiple events when copying files (e.g. APFS→resolved path transition)
/// where content bytes may differ slightly.
const FILE_DEDUP_WINDOW: std::time::Duration = std::time::Duration::from_millis(500);

pub struct ClipboardWatcher {
    local_clipboard: Arc<dyn SystemClipboardPort>,
    sender: PlatformEventSender,
    last_meaningful_dedupe_key: Option<String>,
    last_file_emit_time: Option<Instant>,
}

impl ClipboardWatcher {
    pub fn new(local_clipboard: Arc<dyn SystemClipboardPort>, sender: PlatformEventSender) -> Self {
        Self {
            local_clipboard,
            sender,
            last_meaningful_dedupe_key: None,
            last_file_emit_time: None,
        }
    }
}

fn is_file_representation(rep: &uc_core::ObservedClipboardRepresentation) -> bool {
    uc_core::clipboard::is_file_mime_or_format(rep.mime.as_ref(), &rep.format_id)
}

fn dedupe_key(snapshot: &SystemClipboardSnapshot) -> Option<String> {
    snapshot.meaningful_origin_key()
}

/// Returns true if any representation in the snapshot is a file representation.
fn snapshot_has_files(snapshot: &SystemClipboardSnapshot) -> bool {
    snapshot.representations.iter().any(is_file_representation)
}

impl ClipboardHandler for ClipboardWatcher {
    fn on_clipboard_change(&mut self) {
        match self.local_clipboard.read_snapshot() {
            Ok(snapshot) => {
                let current_dedupe_key = dedupe_key(&snapshot);
                if let Some(key) = current_dedupe_key.as_ref() {
                    if self.last_meaningful_dedupe_key.as_deref() == Some(key.as_str()) {
                        debug!(
                            dedupe_key = %key,
                            "Skipping duplicated meaningful clipboard snapshot"
                        );
                        return;
                    }
                }

                // Time-window suppression for file snapshots: macOS fires
                // multiple clipboard events when copying files (APFS→resolved
                // path transition) where content bytes may differ slightly.
                if snapshot_has_files(&snapshot) {
                    let now = Instant::now();
                    if let Some(last) = self.last_file_emit_time {
                        if now.duration_since(last) < FILE_DEDUP_WINDOW {
                            debug!(
                                elapsed_ms = now.duration_since(last).as_millis(),
                                "Suppressing rapid consecutive file clipboard event"
                            );
                            return;
                        }
                    }
                }

                if let Err(err) = self
                    .sender
                    .try_send(PlatformEvent::ClipboardChanged { snapshot })
                {
                    warn!(
                        error_kind = "notify_channel_send_failed",
                        retryable = true,
                        error = %err,
                        "Failed to notify clipboard change"
                    );
                } else {
                    if current_dedupe_key
                        .as_ref()
                        .is_some_and(|k| k.starts_with("files:"))
                    {
                        self.last_file_emit_time = Some(Instant::now());
                    }
                    if let Some(key) = current_dedupe_key {
                        self.last_meaningful_dedupe_key = Some(key);
                    }
                }
            }

            Err(e) => {
                warn!(
                    error_kind = "platform_clipboard_read_failed",
                    retryable = true,
                    error = %e,
                    "Failed to read clipboard snapshot"
                );
            }
        }
    }
}
