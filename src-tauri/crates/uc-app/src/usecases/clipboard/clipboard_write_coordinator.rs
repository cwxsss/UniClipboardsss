use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tracing::info_span;
use tracing::Instrument;

use uc_core::clipboard::ClipboardChangeOrigin;
use uc_core::clipboard::SystemClipboardSnapshot;
use uc_core::ports::clipboard::{ClipboardChangeOriginPort, SystemClipboardPort};

/// Represents the intent behind a programmatic clipboard write.
///
/// Each variant carries per-intent guard TTL semantics:
/// - `LocalRestore`: 2-second hash guard + one-shot next-origin override
/// - `LocalCapture`: 2-second hash guard (short-lived, local op)
/// - `RemotePush`: 60-second hash guard + one-shot next-origin override (OS re-encoding guard)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardWriteIntent {
    /// A local clipboard history restore (user clicked "restore" in history).
    LocalRestore,
    /// A remote push from a peer device arriving via inbound sync.
    RemotePush,
    /// A local clipboard write triggered by a local app action (e.g., copy file).
    LocalCapture,
}

/// Single write boundary for all programmatic clipboard writes.
///
/// Centralises the guard-registration + write + cleanup-on-error pattern
/// that was previously duplicated across `restore_clipboard_selection.rs`,
/// `sync_inbound.rs`, and `copy_file_to_clipboard.rs`.
///
/// # Contract
///
/// `write(snapshot, intent)` is the ONLY caller of `snapshot.origin_guard_key()`.
/// Callers build the snapshot and choose the intent; the coordinator handles
/// all guard lifecycle operations.
pub struct ClipboardWriteCoordinator {
    system_clipboard: Arc<dyn SystemClipboardPort>,
    clipboard_change_origin: Arc<dyn ClipboardChangeOriginPort>,
    /// True while a `write()` call is in progress. Used by workers to detect
    /// genuinely concurrent clipboard writes, replacing the overly broad
    /// `has_pending_origin()` check that conflated attribution guards with
    /// active write operations.
    writing: AtomicBool,
}

impl ClipboardWriteCoordinator {
    pub fn new(
        system_clipboard: Arc<dyn SystemClipboardPort>,
        clipboard_change_origin: Arc<dyn ClipboardChangeOriginPort>,
    ) -> Self {
        Self {
            system_clipboard,
            clipboard_change_origin,
            writing: AtomicBool::new(false),
        }
    }

    /// Returns `true` while another `write()` call is actively in progress.
    ///
    /// Unlike the previous `has_pending_origin()`, this only returns `true`
    /// during an actual concurrent write — it does NOT trigger on stale
    /// attribution guards left by previously completed writes.
    pub fn is_write_in_progress(&self) -> bool {
        self.writing.load(Ordering::Acquire)
    }

    /// Write a snapshot to the system clipboard with the given intent.
    ///
    /// # Intent semantics
    ///
    /// - `LocalRestore`: registers a 2-second local snapshot hash guard, writes, then
    ///   sets a one-shot `set_next_origin(LocalRestore, 2s)` to cover file URI/path
    ///   rewrites that change bytes between write and watcher callback.
    /// - `LocalCapture`: registers a 2-second local snapshot hash guard, then writes.
    ///   On error, consumes the guard to prevent stale state.
    /// - `RemotePush`: registers a 60-second remote snapshot hash guard, writes, then
    ///   sets a one-shot `set_next_origin(RemotePush, 60s)` to guard against OS re-encoding
    ///   loopback (e.g., Windows DIB→PNG re-encode produces a different hash than the guard).
    ///
    /// # Error handling
    ///
    /// On `write_snapshot` failure, the registered guard is consumed via
    /// `consume_origin_for_snapshot_or_default` to prevent stale guard accumulation.
    /// The `set_next_origin` call for `RemotePush` is NOT made on failure.
    pub async fn write(
        &self,
        snapshot: SystemClipboardSnapshot,
        intent: ClipboardWriteIntent,
    ) -> Result<()> {
        self.writing.store(true, Ordering::Release);
        let result = self.write_inner(snapshot, intent).await;
        self.writing.store(false, Ordering::Release);
        result
    }

    async fn write_inner(
        &self,
        snapshot: SystemClipboardSnapshot,
        intent: ClipboardWriteIntent,
    ) -> Result<()> {
        let origin_guard_key = snapshot.origin_guard_key();

        async {
            // Register the appropriate hash guard before writing.
            match intent {
                ClipboardWriteIntent::LocalRestore | ClipboardWriteIntent::LocalCapture => {
                    self.clipboard_change_origin
                        .remember_local_snapshot_hash(
                            origin_guard_key.clone(),
                            Duration::from_secs(2),
                        )
                        .await;
                }
                ClipboardWriteIntent::RemotePush => {
                    self.clipboard_change_origin
                        .remember_remote_snapshot_hash(
                            origin_guard_key.clone(),
                            Duration::from_secs(60),
                        )
                        .await;
                }
            }

            // Attempt the write.
            if let Err(err) = self.system_clipboard.write_snapshot(snapshot) {
                // On failure: consume the guard to prevent stale state accumulation.
                self.clipboard_change_origin
                    .consume_origin_for_snapshot_or_default(
                        &origin_guard_key,
                        ClipboardChangeOrigin::LocalCapture,
                    )
                    .await;
                return Err(err);
            }

            match intent {
                ClipboardWriteIntent::LocalRestore => {
                    // File restores can come back from the platform clipboard with rewritten
                    // URI/path bytes, so the hash guard alone is not sufficient.
                    self.clipboard_change_origin
                        .set_next_origin(
                            ClipboardChangeOrigin::LocalRestore,
                            Duration::from_secs(2),
                        )
                        .await;
                }
                ClipboardWriteIntent::RemotePush => {
                    // Some platforms (e.g. Windows clipboard-rs) re-encode images (PNG→DIB→PNG),
                    // producing different bytes than the original. The hash guard above won't match
                    // the re-encoded content, so we set a one-shot origin override: the NEXT clipboard
                    // change will be treated as RemotePush regardless of hash.
                    self.clipboard_change_origin
                        .set_next_origin(ClipboardChangeOrigin::RemotePush, Duration::from_secs(60))
                        .await;
                }
                ClipboardWriteIntent::LocalCapture => {}
            }

            Ok(())
        }
        .instrument(info_span!(
            "clipboard_write_coordinator.write",
            intent = ?intent,
            origin_guard_key = %origin_guard_key
        ))
        .await
    }
}
