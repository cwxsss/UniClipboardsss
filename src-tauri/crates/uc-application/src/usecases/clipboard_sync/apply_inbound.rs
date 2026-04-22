//! `ApplyInboundClipboardUseCase` — daemon-side inbound clipboard
//! processing pipeline (Slice 2 Phase 3 · T4).
//!
//! ## Flow
//!
//! 1. **Dedup short-circuit**: if `content_hash` already exists in the
//!    local `clipboard_event` table, return `DuplicateSkipped`. Skips
//!    persist + OS-clipboard write — Phase 3 acceptance #4 guarantees a
//!    repeat copy from a peer doesn't double-write the user's clipboard.
//! 2. **Envelope decode**: V3 → `SystemClipboardSnapshot`. Decode failure
//!    is non-fatal (`DecodeFailed` outcome) — corrupted payloads from a
//!    misbehaving peer don't crash the daemon's ingest loop.
//! 3. **Capture pipeline**: reuse `CaptureClipboardUseCase` with origin
//!    `RemotePush` so the entry, event, normalised representations,
//!    cache, spool, and (optional) search index all match the local
//!    capture path's schema (D5 decision).
//! 4. **OS clipboard write**: via `ClipboardWriteCoordinator` with
//!    `RemotePush` intent — registers a 60s hash guard + one-shot
//!    next-origin override so the daemon's own clipboard watcher doesn't
//!    re-dispatch the just-written content (write-back loop defence).
//!
//! Step ordering (4 → 5) matters: capture commits the event before the
//! OS write fires, so when the watcher consumes the origin guard it
//! already sees the persisted row.
//!
//! ## Testability
//!
//! `CaptureClipboardUseCase` and `ClipboardWriteCoordinator` are
//! concrete structs with 7+2 port dependencies. Holding them as
//! `Arc<dyn Trait>` via two thin internal abstractions
//! ([`InboundCapture`] / [`InboundWrite`]) keeps the use case mockable
//! without requiring tests to construct full real implementations.
//! Production wires the concrete types via the blanket impls below.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use bytes::Bytes;
use thiserror::Error;
use tracing::{debug, info, instrument};

use uc_core::ids::{DeviceId, EntryId};
use uc_core::ports::{ClipboardEntryRepositoryPort, SelectRepresentationPolicyPort};
use uc_core::{ClipboardChangeOrigin, SystemClipboardSnapshot};

use crate::clipboard_capture::CaptureClipboardUseCase;
use crate::clipboard_write::{narrow_to_primary, ClipboardWriteCoordinator, ClipboardWriteIntent};
use crate::usecases::clipboard_sync::payload_codec::decode_v3_bytes_to_snapshot;

/// Caller-supplied input mapped from the facade's public `InboundNotice`.
///
/// Keeping this struct separate from `crate::facade::clipboard::InboundNotice`
/// avoids the use case importing from the facade layer (§11.4 keeps the
/// arrow `facade → use case`, never the reverse).
#[derive(Debug, Clone)]
pub struct ApplyInboundInput {
    pub from_device: DeviceId,
    pub content_hash: String,
    pub plaintext: Bytes,
}

/// Result of one `execute` call. Daemon's worker maps each variant to a
/// distinct telemetry path (WS event for `Applied`, debug log for
/// `DuplicateSkipped`, warn log for `DecodeFailed`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyOutcome {
    /// New content — persisted + OS clipboard written. WS event fires.
    Applied { entry_id: EntryId },
    /// `content_hash` was already present in the local DB. No persist,
    /// no OS write, no WS event.
    DuplicateSkipped {
        content_hash: String,
        existing_entry_id: EntryId,
    },
    /// V3 envelope was malformed. Frame dropped silently except for a
    /// warning log; receiver loop keeps running.
    DecodeFailed { reason: String },
}

#[derive(Debug, Error)]
pub enum ApplyInboundError {
    #[error("dedup query failed: {0}")]
    DedupQuery(String),
    #[error("capture pipeline failed: {0}")]
    Capture(String),
    #[error("clipboard write failed: {0}")]
    WriteCoordinator(String),
    #[error("internal: {0}")]
    Internal(String),
}

/// Internal abstraction over the persistence pipeline. Production uses
/// the blanket impl on `CaptureClipboardUseCase`; tests use a `mockall`
/// mock.
#[async_trait]
pub trait InboundCapture: Send + Sync {
    /// Persist `snapshot` as a `RemotePush`-origin entry. Returns
    /// `Ok(Some(EntryId))` on success, `Ok(None)` only in the legitimate
    /// "no supported representation" / `LocalRestore` short-circuit cases
    /// (which `RemotePush` never hits in practice — daemon treats `None`
    /// as `ApplyInboundError::Internal`).
    async fn capture(&self, snapshot: SystemClipboardSnapshot) -> Result<Option<EntryId>>;
}

#[async_trait]
impl InboundCapture for CaptureClipboardUseCase {
    async fn capture(&self, snapshot: SystemClipboardSnapshot) -> Result<Option<EntryId>> {
        self.execute_with_origin(snapshot, ClipboardChangeOrigin::RemotePush, None)
            .await
    }
}

/// Internal abstraction over the OS clipboard write boundary. Production
/// uses the blanket impl on `ClipboardWriteCoordinator`; tests mock it.
#[async_trait]
pub trait InboundWrite: Send + Sync {
    /// Write `snapshot` to the OS clipboard with the `RemotePush`
    /// intent (registers the appropriate hash guards + next-origin
    /// override per the coordinator's contract).
    async fn write(&self, snapshot: SystemClipboardSnapshot) -> Result<()>;
}

#[async_trait]
impl InboundWrite for ClipboardWriteCoordinator {
    async fn write(&self, snapshot: SystemClipboardSnapshot) -> Result<()> {
        ClipboardWriteCoordinator::write(self, snapshot, ClipboardWriteIntent::RemotePush).await
    }
}

pub struct ApplyInboundClipboardUseCase {
    entry_repo: Arc<dyn ClipboardEntryRepositoryPort>,
    capture: Arc<dyn InboundCapture>,
    write: Arc<dyn InboundWrite>,
    /// Reused by the narrow-to-primary step between capture + write so
    /// `paste_rep_id` priority matches what `CaptureClipboardUseCase`
    /// picks for UI paste. See `clipboard_write::narrow_to_primary` and
    /// `uc-platform/src/clipboard/common.rs` TODO (`write_snapshot`
    /// requires exactly one representation).
    representation_policy: Arc<dyn SelectRepresentationPolicyPort>,
}

impl ApplyInboundClipboardUseCase {
    pub fn new(
        entry_repo: Arc<dyn ClipboardEntryRepositoryPort>,
        capture: Arc<dyn InboundCapture>,
        write: Arc<dyn InboundWrite>,
        representation_policy: Arc<dyn SelectRepresentationPolicyPort>,
    ) -> Self {
        Self {
            entry_repo,
            capture,
            write,
            representation_policy,
        }
    }

    #[instrument(
        name = "apply_inbound.execute",
        skip_all,
        fields(
            from_device = %input.from_device,
            content_hash = %input.content_hash,
            plaintext_len = input.plaintext.len(),
        )
    )]
    pub async fn execute(
        &self,
        input: ApplyInboundInput,
    ) -> Result<ApplyOutcome, ApplyInboundError> {
        // 1. Dedup short-circuit. The repo's default `Ok(None)` impl
        // (used by in-memory test fakes) degrades dedup to off — safe,
        // worst case we re-write the OS clipboard with identical bytes.
        let existing = self
            .entry_repo
            .find_entry_id_by_snapshot_hash(&input.content_hash)
            .await
            .map_err(|e| ApplyInboundError::DedupQuery(e.to_string()))?;
        if let Some(existing_entry_id) = existing {
            debug!(
                existing_entry_id = %existing_entry_id,
                "inbound dropped: duplicate of existing local entry"
            );
            return Ok(ApplyOutcome::DuplicateSkipped {
                content_hash: input.content_hash,
                existing_entry_id,
            });
        }

        // 2. Decode V3 envelope. Decode failure is non-fatal — drop the
        // frame, keep the loop alive (peer may be on a newer wire).
        let snapshot = match decode_v3_bytes_to_snapshot(input.plaintext.as_ref()) {
            Ok(s) => s,
            Err(e) => {
                let reason = e.to_string();
                debug!(reason, "inbound dropped: envelope decode failed");
                return Ok(ApplyOutcome::DecodeFailed { reason });
            }
        };

        // 3. Persist via the same capture pipeline local copies use
        // (D5: same schema). Cloning the snapshot lets us keep one for
        // the OS write below; capture takes ownership of the original.
        let snapshot_for_write = snapshot.clone();
        let entry_id = self
            .capture
            .capture(snapshot)
            .await
            .map_err(|e| ApplyInboundError::Capture(e.to_string()))?
            .ok_or_else(|| {
                ApplyInboundError::Internal(
                    "capture returned None for RemotePush origin (unexpected)".to_string(),
                )
            })?;

        // 4. Narrow the snapshot to its paste-priority representation.
        // V3 envelopes carry every rep the sender observed (text/plain
        // + text/html + text/rtf + image/png + ...); the platform layer
        // `write_snapshot` only accepts a single-rep snapshot (see
        // `uc-platform/src/clipboard/common.rs` TODO), so without this
        // step inbound sync fails with "platform::write expects exactly
        // ONE representation" for every multi-rep copy. Using the same
        // `SelectRepresentationPolicyPort` that `CaptureClipboardUseCase`
        // runs keeps capture's stored `paste_rep_id` and the byte we
        // push to the OS clipboard consistent.
        let narrowed = narrow_to_primary(snapshot_for_write, &*self.representation_policy)
            .map_err(|e| ApplyInboundError::WriteCoordinator(e.to_string()))?;

        // 5. Write OS clipboard with RemotePush guard. Order matters —
        // capture must complete first so the watcher's origin lookup
        // sees the persisted row even if it fires immediately.
        self.write
            .write(narrowed)
            .await
            .map_err(|e| ApplyInboundError::WriteCoordinator(e.to_string()))?;

        info!(entry_id = %entry_id, "inbound clipboard applied");
        Ok(ApplyOutcome::Applied { entry_id })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usecases::clipboard_sync::payload_codec::encode_snapshot_to_v3_bytes;
    use mockall::predicate::*;

    use uc_core::clipboard::SelectRepresentationPolicyV1;
    use uc_core::ids::{FormatId, RepresentationId};
    use uc_core::ports::PeerAddressError;
    use uc_core::{MimeType, ObservedClipboardRepresentation};

    // ── mockall: the 3 collaborator surfaces ────────────────────────────

    mockall::mock! {
        pub EntryRepo {}
        #[async_trait]
        impl ClipboardEntryRepositoryPort for EntryRepo {
            async fn save_entry_and_selection(
                &self,
                entry: &uc_core::ClipboardEntry,
                selection: &uc_core::ClipboardSelectionDecision,
            ) -> Result<()>;
            async fn get_entry(&self, entry_id: &EntryId) -> Result<Option<uc_core::ClipboardEntry>>;
            async fn list_entries(&self, limit: usize, offset: usize) -> Result<Vec<uc_core::ClipboardEntry>>;
            async fn touch_entry(&self, entry_id: &EntryId, active_time_ms: i64) -> Result<bool>;
            async fn delete_entry(&self, entry_id: &EntryId) -> Result<()>;
            async fn find_entry_id_by_snapshot_hash(&self, snapshot_hash: &str) -> Result<Option<EntryId>>;
        }
    }

    mockall::mock! {
        pub Capture {}
        #[async_trait]
        impl InboundCapture for Capture {
            async fn capture(&self, snapshot: SystemClipboardSnapshot) -> Result<Option<EntryId>>;
        }
    }

    mockall::mock! {
        pub Write {}
        #[async_trait]
        impl InboundWrite for Write {
            async fn write(&self, snapshot: SystemClipboardSnapshot) -> Result<()>;
        }
    }

    // ── helpers ─────────────────────────────────────────────────────────

    fn fixture_input(text: &str) -> (ApplyInboundInput, String) {
        let snapshot = SystemClipboardSnapshot {
            ts_ms: 1_700_000_000_000,
            representations: vec![ObservedClipboardRepresentation::new(
                RepresentationId::new(),
                FormatId::from("text"),
                Some(MimeType("text/plain".to_string())),
                text.as_bytes().to_vec(),
            )],
        };
        let (plaintext, content_hash) = encode_snapshot_to_v3_bytes(&snapshot).unwrap();
        (
            ApplyInboundInput {
                from_device: DeviceId::new("peer-x"),
                content_hash: content_hash.clone(),
                plaintext,
            },
            content_hash,
        )
    }

    fn build(
        repo: MockEntryRepo,
        capture: MockCapture,
        write: MockWrite,
    ) -> ApplyInboundClipboardUseCase {
        // `SelectRepresentationPolicyV1` is pure (no ports, no state) so
        // every verdict can share the real implementation instead of a
        // mock. The narrow step only runs when we reach step 4, i.e. on
        // the Applied-path verdicts — others never observe it.
        ApplyInboundClipboardUseCase::new(
            Arc::new(repo),
            Arc::new(capture),
            Arc::new(write),
            Arc::new(SelectRepresentationPolicyV1::default()),
        )
    }

    // ── verdicts ────────────────────────────────────────────────────────

    /// Verdict 1 — happy path: dedup miss → decode → capture → write →
    /// `Applied { entry_id }`. mockall asserts: dedup query once with
    /// the input hash, capture once, write once.
    #[tokio::test]
    async fn applied_on_new_content() {
        let (input, hash) = fixture_input("hello phase3");

        let mut repo = MockEntryRepo::new();
        repo.expect_find_entry_id_by_snapshot_hash()
            .with(eq(hash.clone()))
            .times(1)
            .returning(|_| Ok(None));

        let mut capture = MockCapture::new();
        capture
            .expect_capture()
            .times(1)
            .returning(|_| Ok(Some(EntryId::from("entry-new"))));

        let mut write = MockWrite::new();
        write.expect_write().times(1).returning(|_| Ok(()));

        let uc = build(repo, capture, write);
        let outcome = uc.execute(input).await.expect("happy path returns ok");
        assert_eq!(
            outcome,
            ApplyOutcome::Applied {
                entry_id: EntryId::from("entry-new")
            }
        );
    }

    /// Verdict 2 — dedup hit: returns `DuplicateSkipped` and **does
    /// not** call capture or write. Critical correctness property —
    /// repeated dispatches from a peer must not double-write the user's
    /// OS clipboard (Phase 3 acceptance #4).
    #[tokio::test]
    async fn duplicate_skipped_when_hash_already_local() {
        let (input, hash) = fixture_input("already-here");

        let mut repo = MockEntryRepo::new();
        repo.expect_find_entry_id_by_snapshot_hash()
            .with(eq(hash.clone()))
            .times(1)
            .returning(|_| Ok(Some(EntryId::from("entry-existing"))));

        // Zero expectations on capture + write — mockall panics on Drop
        // if either gets called. This pins the no-side-effect contract.
        let capture = MockCapture::new();
        let write = MockWrite::new();

        let uc = build(repo, capture, write);
        let outcome = uc.execute(input).await.expect("dedup path ok");
        assert_eq!(
            outcome,
            ApplyOutcome::DuplicateSkipped {
                content_hash: hash,
                existing_entry_id: EntryId::from("entry-existing"),
            }
        );
    }

    /// Verdict 3 — corrupt envelope returns `DecodeFailed`, no panic, no
    /// capture, no write. Daemon's ingest loop keeps running.
    #[tokio::test]
    async fn decode_failed_on_truncated_envelope() {
        let input = ApplyInboundInput {
            from_device: DeviceId::new("peer-broken"),
            content_hash: "blake3v1:00".to_string(),
            plaintext: Bytes::from_static(b"not a valid V3 envelope"),
        };

        let mut repo = MockEntryRepo::new();
        repo.expect_find_entry_id_by_snapshot_hash()
            .times(1)
            .returning(|_| Ok(None));
        let capture = MockCapture::new();
        let write = MockWrite::new();

        let uc = build(repo, capture, write);
        let outcome = uc.execute(input).await.expect("DecodeFailed is Ok variant");
        match outcome {
            ApplyOutcome::DecodeFailed { reason } => {
                assert!(
                    reason.contains("decode V3 envelope"),
                    "reason should mention V3 decode, got: {reason}"
                );
            }
            other => panic!("expected DecodeFailed, got {other:?}"),
        }
    }

    /// Verdict 4 — capture returns Ok(None) (shouldn't happen for
    /// RemotePush but guard it anyway): mapped to
    /// `ApplyInboundError::Internal`. Write must NOT fire.
    #[tokio::test]
    async fn capture_returning_none_maps_to_internal_error() {
        let (input, _) = fixture_input("orphan");

        let mut repo = MockEntryRepo::new();
        repo.expect_find_entry_id_by_snapshot_hash()
            .times(1)
            .returning(|_| Ok(None));

        let mut capture = MockCapture::new();
        capture.expect_capture().times(1).returning(|_| Ok(None));

        // Zero expectations on write.
        let write = MockWrite::new();

        let uc = build(repo, capture, write);
        let err = uc
            .execute(input)
            .await
            .expect_err("Ok(None) from capture must surface as error");
        match err {
            ApplyInboundError::Internal(msg) => {
                assert!(
                    msg.contains("RemotePush"),
                    "internal message should reference origin, got: {msg}"
                );
            }
            other => panic!("expected Internal, got {other:?}"),
        }
    }

    /// Verdict 5 — write coordinator failure surfaces as
    /// `WriteCoordinator` error. Capture has already committed (the
    /// entry stays in DB; manual cleanup is the daemon operator's job).
    /// Pin this trade-off so a future refactor doesn't silently start
    /// rolling back persistence on write failure.
    #[tokio::test]
    async fn write_failure_surfaces_after_capture_commits() {
        let (input, _) = fixture_input("write-will-fail");

        let mut repo = MockEntryRepo::new();
        repo.expect_find_entry_id_by_snapshot_hash()
            .times(1)
            .returning(|_| Ok(None));

        let mut capture = MockCapture::new();
        capture
            .expect_capture()
            .times(1)
            .returning(|_| Ok(Some(EntryId::from("entry-committed"))));

        let mut write = MockWrite::new();
        write
            .expect_write()
            .times(1)
            .returning(|_| Err(anyhow::anyhow!("OS clipboard locked")));

        let uc = build(repo, capture, write);
        let err = uc
            .execute(input)
            .await
            .expect_err("write failure must surface");
        match err {
            ApplyInboundError::WriteCoordinator(msg) => {
                assert!(
                    msg.contains("OS clipboard locked"),
                    "underlying error should propagate, got: {msg}"
                );
            }
            other => panic!("expected WriteCoordinator, got {other:?}"),
        }
    }

    /// Verdict 6 — dedup query failure surfaces as `DedupQuery`. No
    /// decode, no capture, no write — failing closed is the conservative
    /// choice (we'd rather lose an inbound frame than risk a corrupt
    /// double-write under unknown DB state).
    #[tokio::test]
    async fn dedup_query_failure_short_circuits() {
        let (input, _) = fixture_input("dedup-broken");

        let mut repo = MockEntryRepo::new();
        repo.expect_find_entry_id_by_snapshot_hash()
            .times(1)
            .returning(|_| {
                Err(anyhow::Error::from(PeerAddressError::Internal(
                    "db down".to_string(),
                )))
            });
        let capture = MockCapture::new();
        let write = MockWrite::new();

        let uc = build(repo, capture, write);
        let err = uc.execute(input).await.expect_err("dedup error propagates");
        match err {
            ApplyInboundError::DedupQuery(_) => {}
            other => panic!("expected DedupQuery, got {other:?}"),
        }
    }
}
