# Phase 32: Fix file sync eventual consistency - Research

**Researched:** 2026-03-15
**Domain:** Receiver-side file transfer state machine, persisted status projection, startup reconciliation
**Confidence:** HIGH

## Summary

The root cause is not the two-phase protocol itself. The root cause is that the receiver currently persists the clipboard entry when metadata arrives, but it does not persist the transfer lifecycle that determines whether the referenced files ever became usable. The UI therefore shows a file entry immediately, while the only transfer state the frontend sees is transient progress/error events kept in Redux. If the blob never arrives, the app restarts, or the event stream is missed, the entry remains visible without durable transfer truth.

The codebase already contains most of the raw building blocks:

- `SyncInboundClipboardUseCase` already persists file-backed clipboard entries as soon as a `ClipboardMessage` with `file_transfers` arrives.
- `FileTransferService` already emits `FileTransferStarted`, `TransferProgress`, `FileTransferCompleted`, and `FileTransferFailed`.
- The database already has a `file_transfer` table with a `status` column and indexes.

What is missing is the structural wiring between those pieces:

1. No repository or port uses `file_transfer`.
2. No durable mapping exists between `clipboard_entry` and transfer lifecycle.
3. No runtime timeout sweep marks stalled pending/transferring rows failed.
4. No startup reconciliation marks orphaned in-flight transfers failed and cleans associated cache artifacts.
5. The frontend receives runtime transfer progress, but list queries do not surface persisted transfer state.

The recommended fix is a receiver-side persisted state machine that keeps the current protocol unchanged:

- Metadata receipt seeds `pending` transfer rows tied to the persisted clipboard entry.
- The first receiving-side progress event promotes the transfer to `transferring`, and later progress refreshes liveness without changing the semantic state.
- Runtime timeout sweeps fail `pending` rows after 60 seconds and `transferring` rows after 5 minutes without new chunk activity.
- Completion/failure updates the same durable row before any UI event is emitted and cleans partial cache artifacts on failure paths.
- Clipboard list projections expose aggregate entry transfer state so restart/reload still shows truth.
- Startup reconciliation marks lingering `pending` and `transferring` rows as `failed` and removes their cached partial files.

## Locked Constraints From Context

### Must preserve

- Keep the current two-phase flow: clipboard metadata first, blob stream separately.
- Do not require sender-side success tracking or retries.
- Dashboard must still show the incoming file entry as soon as metadata is persisted.
- Auto-write to OS clipboard still happens only when transfer reaches `completed`.
- Delete remains available for all transfer states.

### Important interpretation

- `transferring` should mean "receiver has begun consuming blob data", not merely "announce frame arrived".
- Because `ClipboardMessage.file_transfers` currently carries only `transfer_id` and `filename`, metadata receipt cannot fully populate final blob metadata without either:
  - a protocol expansion, or
  - a schema/repository shape that accepts partial metadata until announce/progress fills it in.

Given the explicit "no protocol changes" constraint, the second option is the correct one.

- `batch_id` exists in the current `file_transfer` schema. Phase 32 preserves this column but does not actively use it for aggregation. Entry-level aggregation is done through `entry_id`. Cleanup or active utilization of `batch_id` is deferred to a future phase.

## Existing Architecture Findings

### 1. Metadata path already persists the entry, but stops there

In `src-tauri/crates/uc-app/src/usecases/clipboard/sync_inbound.rs`, the `has_file_transfers` branch rewrites remote file URIs to local cache targets, skips OS clipboard write, and persists the clipboard entry through `CaptureClipboardUseCase`. That is exactly the right place to seed durable `pending` transfer records because it already has:

- the persisted `entry_id`
- `origin_device_id`
- every `transfer_id`
- the intended cache path `{cache_dir}/{transfer_id}/{filename}`

Current gap: `InboundApplyOutcome` only returns `entry_id`, so the Tauri layer cannot emit durable pending linkage or seed frontend state from the apply result.

### 2. File transfer lifecycle events exist, but they are not joined to the entry

In `src-tauri/crates/uc-platform/src/adapters/file_transfer/service.rs`, the receiver emits:

- `FileTransferStarted` after announce parsing
- `TransferProgress` for chunk progress
- `FileTransferCompleted` when the final file lands
- `FileTransferFailed` on error

Current gap: these events operate on `transfer_id`, while the Dashboard operates on `entry_id`. No persistent bridge exists.

### 3. `file_transfer` schema already exists, but is effectively dead

The migration `src-tauri/crates/uc-infra/migrations/2026-03-13-000001_create_file_transfer/up.sql` and generated schema `src-tauri/crates/uc-infra/src/db/schema.rs` show a `file_transfer` table with:

- `transfer_id`
- `filename`
- `file_size`
- `content_hash`
- `status`
- `source_device`
- `batch_id`
- `cached_path`
- timestamps

Current gaps:

- no `entry_id`
- no `failure_reason`
- `file_size` and `content_hash` are required even though clipboard metadata does not have them yet
- no repository, mapper, or app port exists

This is the clearest signal that the architectural intent was already present, but the implementation stopped before integration.

### 4. Frontend transfer state is ephemeral by design

`src/store/slices/fileTransferSlice.ts` tracks progress in Redux only. It is useful for live animation, but it is not the source of truth because:

- it is seeded only from runtime events
- completed transfers auto-clear
- entry linkage depends on runtime mapping, not query results
- reload/startup loses state

Phase 32 should keep this slice for progress, but pair it with persisted entry-level status from commands and `file-transfer://status-changed`.

## Recommended Structural Design

### Domain model

Add a dedicated app/core transfer tracking contract, for example:

- `TrackedFileTransferStatus`: `Pending | Transferring | Completed | Failed`
- `TrackedFileTransfer`
- `PendingInboundTransfer`
- `EntryTransferSummary`

The port should support:

- seeding pending rows from clipboard metadata
- backfilling announce metadata if it becomes available later
- promoting first-data-chunk transitions
- refreshing liveness on subsequent receiving progress events
- marking completion/failure with reason/path
- querying or sweeping expired in-flight rows based on `updated_at_ms`
- aggregating entry-level status for list projections
- marking stale in-flight rows failed on startup

### Schema shape

To support metadata-first persistence without protocol changes, the stored row needs:

- `entry_id`
- `failure_reason`
- nullable `file_size`
- nullable `content_hash`

That is cleaner than stuffing placeholder `0` or empty string values into columns that are not truly known at metadata time.

### Transition source of truth

- `pending`: seeded in `SyncInboundClipboardUseCase` when file-backed clipboard entry is persisted
- `transferring`: first inbound `TransferProgress` with `direction == Receiving` and `chunks_completed > 0`
- `liveness refresh`: every later receiving-side `TransferProgress` updates the durable activity timestamp
- `completed`: after `SyncInboundFileUseCase::handle_transfer_complete` succeeds
- `failed`: file transport failure, timeout sweep, hash mismatch, file-sync-disabled cleanup, or startup reconciliation

### Projection rule for multi-file entries

The Dashboard renders entries, not individual transfers. The list projection should therefore expose an aggregate status:

- `failed` if any transfer for the entry failed
- `transferring` if none failed and any is transferring
- `pending` if none failed/transferring and any is pending
- `completed` only when all tracked transfers are completed

This gives restart-safe UI without forcing the frontend to recompute aggregate truth from multiple low-level rows.

## Plan Split Recommendation

The phase should be split along architectural boundaries, not just by file count:

### Plan 01: Core/App contract and projection plumbing

- Add `FileTransferRepositoryPort` and transfer status model in `uc-core`
- Add app-layer tracking use case(s) in `uc-app`
- Extend `SyncInboundClipboardUseCase` to seed pending transfers and return pending linkage
- Extend list projections to surface aggregate entry transfer status

Reason: this establishes the behavior contract without mixing database or Tauri wiring.

### Plan 02: Infra repository and schema migration

- Add/reshape the `file_transfer` schema for metadata-first persistence
- Implement Diesel models/repository/tests in `uc-infra`

Reason: port definition and adapter implementation must not land in the same commit.

### Plan 03: Tauri/platform integration and startup reconciliation

- Wire the repository-backed use cases through runtime/bootstrap
- Emit `file-transfer://status-changed` payloads with camelCase fields
- Promote transitions from inbound apply/progress/completed/failed/startup cleanup
- Run runtime timeout sweeps and cache cleanup for timed-out or reconciled rows
- Keep clipboard restore behavior gated on `completed`

Reason: app-layer logic and platform integration must stay split.

### Plan 04: Frontend durable status UX

- Extend clipboard item API types to include persisted transfer status
- Listen to `file-transfer://status-changed`
- Render pending/transferring/failed state in list and preview
- Disable Copy until `completed`, keep Delete enabled

Reason: frontend should consume the durable contract, not infer transfer truth from transient progress events.

## Key Risks And How To Avoid Them

### Risk 1: Treating `FileTransferStarted` as `transferring`

Avoid using the announce event as the `transferring` transition. The context explicitly wants first-data-chunk semantics. Use receiving-side `TransferProgress` instead.

### Risk 2: Placeholder values in required DB columns

If the schema is left as-is and the implementation writes fake `file_size` or `content_hash`, the model will drift from truth and future code will have to compensate. Prefer a small migration over persistent fake metadata.

### Risk 3: Event-only fix

If the phase only adds a `status-changed` event but does not extend list projections, restart and missed-event scenarios remain broken. Persisted projection output is mandatory.

### Risk 4: Startup cleanup blocking launch

Reconciliation should be background, bounded, and non-fatal. It should update rows and remove temp files, but not stop app startup.

### Risk 5: Missing liveness refresh for transferring rows

If only the first progress event is persisted, the app cannot enforce the locked 5-minute stall timeout. Later progress must refresh durable activity even when the semantic status stays `transferring`.

### Risk 6: Mixing clipboard entry state with transfer state

Do not overload stale-file UX or `is_downloaded` to represent transfer lifecycle. Transfer status is a separate concern with separate transitions and reason text.

## Validation Architecture

This phase is a good Nyquist candidate because it crosses app, infra, platform, and frontend boundaries while still having narrow deterministic transitions.

### Automated coverage targets

- app tests for metadata seeding, progress promotion, completion/failure transitions, timeout selection, and startup reconciliation
- infra tests for migration shape, repository CRUD/transition semantics, and aggregate entry summary queries
- tauri tests for payload serialization (`camelCase` event payload) and event-loop orchestration
- frontend tests for:
  - initial API response carrying persisted transfer status
  - `file-transfer://status-changed` reducer updates
  - disabled Copy for non-completed states
  - failed state rendering after startup reconciliation

### Manual verification targets

- send a file from one device and confirm entry goes `pending -> transferring -> completed`
- leave a transfer idle past the 60-second / 5-minute timeout thresholds and confirm it fails without restart
- interrupt a transfer mid-flight, restart receiver, confirm entry becomes failed without disappearing from history
- verify Delete works on pending/failed entries and removes cache artifacts
- verify completed entries still auto-write to OS clipboard while failed entries do not

### Recommended sampling

- quick feedback after every task: targeted Rust tests for touched crate or targeted Vitest run
- after every wave: run all affected Rust crates plus frontend tests
- before phase verification: full `cargo test` for touched crates and `bun run test -- --run`

## Planning Guidance For The Next Step

- Use `FSYNC-CONSISTENCY` once roadmap and requirements are updated together.
- Make the event payload an explicit struct with `#[serde(rename_all = "camelCase")]`.
- Keep `fileTransferSlice` for progress, but add a durable entry-level status path from commands/events.

---

_Phase: 32-fix-file-sync-eventual-consistency-ensure-atomic-sync-with-metadata-and-blob-together_
_Research completed: 2026-03-15_
