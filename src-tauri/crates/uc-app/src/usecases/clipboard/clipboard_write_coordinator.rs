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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_mocks::{MockClipboardChangeOrigin, MockSystemClipboard};
    use std::sync::{Arc, Mutex};
    use uc_core::clipboard::{MimeType, ObservedClipboardRepresentation, SystemClipboardSnapshot};
    use uc_core::ids::{FormatId, RepresentationId};

    fn make_snapshot(text: &str) -> SystemClipboardSnapshot {
        SystemClipboardSnapshot {
            ts_ms: 0,
            representations: vec![ObservedClipboardRepresentation::new(
                RepresentationId::from_str("rep-1"),
                FormatId::from("text"),
                Some(MimeType("text/plain".to_string())),
                text.as_bytes().to_vec(),
            )],
        }
    }

    fn make_clipboard(
        fail_with: Option<&str>,
    ) -> (Arc<MockSystemClipboard>, Arc<Mutex<Vec<String>>>) {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let calls_clone = calls.clone();
        let fail_with = fail_with.map(ToString::to_string);

        let mut clipboard = MockSystemClipboard::new();
        clipboard
            .expect_write_snapshot()
            .returning(move |snapshot| {
                calls_clone
                    .lock()
                    .unwrap()
                    .push(format!("write:{}", snapshot.origin_guard_key()));
                if let Some(err_msg) = &fail_with {
                    return Err(anyhow::anyhow!("{err_msg}"));
                }
                Ok(())
            });
        clipboard
            .expect_read_snapshot()
            .returning(|| Ok(make_snapshot("read-back")));

        (Arc::new(clipboard), calls)
    }

    fn make_origin() -> (Arc<MockClipboardChangeOrigin>, Arc<Mutex<Vec<String>>>) {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let mut origin = MockClipboardChangeOrigin::new();

        let calls_clone = calls.clone();
        origin
            .expect_set_next_origin()
            .returning(move |origin, ttl| {
                calls_clone.lock().unwrap().push(format!(
                    "set_next_origin:{:?}:{}ms",
                    origin,
                    ttl.as_millis()
                ));
            });

        let calls_clone = calls.clone();
        origin
            .expect_consume_origin_or_default()
            .returning(move |default_origin| {
                calls_clone
                    .lock()
                    .unwrap()
                    .push(format!("consume_default:{:?}", default_origin));
                default_origin
            });

        let calls_clone = calls.clone();
        origin
            .expect_remember_remote_snapshot_hash()
            .returning(move |snapshot_hash, ttl| {
                calls_clone.lock().unwrap().push(format!(
                    "remember_remote:{}:{}ms",
                    snapshot_hash,
                    ttl.as_millis()
                ));
            });

        let calls_clone = calls.clone();
        origin
            .expect_remember_local_snapshot_hash()
            .returning(move |snapshot_hash, ttl| {
                calls_clone.lock().unwrap().push(format!(
                    "remember_local:{}:{}ms",
                    snapshot_hash,
                    ttl.as_millis()
                ));
            });

        origin.expect_has_pending_origin().returning(|| false);

        let calls_clone = calls.clone();
        origin
            .expect_consume_origin_for_snapshot_or_default()
            .returning(move |snapshot_hash, default_origin| {
                calls_clone.lock().unwrap().push(format!(
                    "consume_snapshot:{}:{:?}",
                    snapshot_hash, default_origin
                ));
                default_origin
            });

        (Arc::new(origin), calls)
    }

    fn coordinator(
        clipboard: Arc<dyn SystemClipboardPort>,
        origin: Arc<dyn ClipboardChangeOriginPort>,
    ) -> ClipboardWriteCoordinator {
        ClipboardWriteCoordinator::new(clipboard, origin)
    }

    // ---------------------------------------------------------------------------
    // Test 1: LocalRestore calls remember_local_snapshot_hash with 2s TTL, then write_snapshot,
    // and sets next-origin for transformed clipboard callbacks
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test1_local_restore_registers_local_guard_writes_and_sets_next_origin() {
        let (clipboard, clipboard_calls) = make_clipboard(None);
        let (origin, origin_calls) = make_origin();
        let coord = coordinator(clipboard.clone(), origin.clone());

        let snapshot = make_snapshot("hello");
        let expected_key = snapshot.origin_guard_key();

        coord
            .write(snapshot, ClipboardWriteIntent::LocalRestore)
            .await
            .expect("write should succeed");

        let calls = origin_calls.lock().unwrap().clone();
        // First call must be remember_local with 2000ms TTL
        assert!(
            calls[0].starts_with(&format!("remember_local:{}:2000ms", expected_key)),
            "expected remember_local:...:2000ms, got: {:?}",
            calls
        );
        // write_snapshot must have been called once
        assert_eq!(
            clipboard_calls.lock().unwrap().len(),
            1,
            "write_snapshot must be called once"
        );
        let has_set_next = calls
            .iter()
            .any(|c| c == "set_next_origin:LocalRestore:2000ms");
        assert!(
            has_set_next,
            "set_next_origin(LocalRestore, 2s) must be called: {:?}",
            calls
        );
    }

    // ---------------------------------------------------------------------------
    // Test 2: RemotePush calls remember_remote_snapshot_hash with 60s TTL, then write, then set_next_origin
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test2_remote_push_registers_remote_guard_writes_and_sets_next_origin() {
        let (clipboard, clipboard_calls) = make_clipboard(None);
        let (origin, origin_calls) = make_origin();
        let coord = coordinator(clipboard.clone(), origin.clone());

        let snapshot = make_snapshot("remote content");
        let expected_key = snapshot.origin_guard_key();

        coord
            .write(snapshot, ClipboardWriteIntent::RemotePush)
            .await
            .expect("write should succeed");

        let calls = origin_calls.lock().unwrap().clone();
        // First: remember_remote with 60000ms TTL
        assert!(
            calls[0].starts_with(&format!("remember_remote:{}:60000ms", expected_key)),
            "expected remember_remote:...:60000ms, got: {:?}",
            calls
        );
        // write_snapshot called
        assert_eq!(clipboard_calls.lock().unwrap().len(), 1);
        // set_next_origin called with RemotePush and 60s TTL
        let has_set_next = calls
            .iter()
            .any(|c| c == "set_next_origin:RemotePush:60000ms");
        assert!(
            has_set_next,
            "set_next_origin(RemotePush, 60s) must be called: {:?}",
            calls
        );
    }

    // ---------------------------------------------------------------------------
    // Test 3: LocalCapture calls remember_local_snapshot_hash with 2s TTL, then write
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test3_local_capture_registers_local_guard_and_writes() {
        let (clipboard, clipboard_calls) = make_clipboard(None);
        let (origin, origin_calls) = make_origin();
        let coord = coordinator(clipboard.clone(), origin.clone());

        let snapshot = make_snapshot("local capture");
        let expected_key = snapshot.origin_guard_key();

        coord
            .write(snapshot, ClipboardWriteIntent::LocalCapture)
            .await
            .expect("write should succeed");

        let calls = origin_calls.lock().unwrap().clone();
        assert!(
            calls[0].starts_with(&format!("remember_local:{}:2000ms", expected_key)),
            "expected remember_local:...:2000ms for LocalCapture, got: {:?}",
            calls
        );
        assert_eq!(clipboard_calls.lock().unwrap().len(), 1);
        assert!(
            !calls.iter().any(|c| c.starts_with("set_next_origin")),
            "set_next_origin must NOT be called for LocalCapture: {:?}",
            calls
        );
    }

    // ---------------------------------------------------------------------------
    // Test 4: On write_snapshot error, calls consume_origin_for_snapshot_or_default and returns Err
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test4_write_failure_consumes_guard_and_returns_error() {
        let (clipboard, _clipboard_calls) = make_clipboard(Some("disk full"));
        let (origin, origin_calls) = make_origin();
        let coord = coordinator(clipboard.clone(), origin.clone());

        let snapshot = make_snapshot("failing write");
        let expected_key = snapshot.origin_guard_key();

        let result = coord
            .write(snapshot, ClipboardWriteIntent::LocalRestore)
            .await;
        assert!(result.is_err(), "should return error on write failure");

        let calls = origin_calls.lock().unwrap().clone();
        // Guard was registered
        assert!(
            calls[0].starts_with("remember_local:"),
            "guard must be registered before write: {:?}",
            calls
        );
        // consume_snapshot called with the correct key
        let consumed = calls
            .iter()
            .any(|c| c.starts_with(&format!("consume_snapshot:{}:", expected_key)));
        assert!(
            consumed,
            "consume_origin_for_snapshot_or_default must be called on error: {:?}",
            calls
        );
    }

    // ---------------------------------------------------------------------------
    // Test 5: On write_snapshot error for RemotePush, set_next_origin is NOT called
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test5_remote_push_write_failure_does_not_call_set_next_origin() {
        let (clipboard, _clipboard_calls) = make_clipboard(Some("network error"));
        let (origin, origin_calls) = make_origin();
        let coord = coordinator(clipboard.clone(), origin.clone());

        let snapshot = make_snapshot("remote fail");

        let result = coord
            .write(snapshot, ClipboardWriteIntent::RemotePush)
            .await;
        assert!(result.is_err(), "should return error on write failure");

        let calls = origin_calls.lock().unwrap().clone();
        // Guard registered
        assert!(
            calls[0].starts_with("remember_remote:"),
            "remote guard must be registered: {:?}",
            calls
        );
        // consume called on error
        assert!(
            calls.iter().any(|c| c.starts_with("consume_snapshot:")),
            "consume guard must be called on error: {:?}",
            calls
        );
        // set_next_origin must NOT be called
        assert!(
            !calls.iter().any(|c| c.starts_with("set_next_origin")),
            "set_next_origin must NOT be called on RemotePush failure: {:?}",
            calls
        );
    }
}
