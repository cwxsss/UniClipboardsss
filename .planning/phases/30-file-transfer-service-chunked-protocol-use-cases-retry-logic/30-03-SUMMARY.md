---
phase: 30-file-transfer-service-chunked-protocol-use-cases-retry-logic
plan: 03
subsystem: network
tags: [file-transfer, queue, retry, exponential-backoff, tokio-mpsc, wiring]

# Dependency graph
requires:
  - phase: 30-01
    provides: FileTransferService with chunked protocol and binary framing
  - phase: 30-02
    provides: SyncOutboundFileUseCase and SyncInboundFileUseCase with safety checks
provides:
  - Serial FIFO FileTransferQueue with append-during-transfer support
  - Exponential backoff RetryPolicy for transient network errors
  - UseCases accessor methods for file sync use cases (sync_outbound_file, sync_inbound_file)
affects: [phase-31-file-sync-ui, phase-32-file-sync-settings]

# Tech tracking
tech-stack:
  added: []
  patterns: [serial-queue-via-mpsc, categorized-error-retry-policy, exponential-backoff-with-cap]

key-files:
  created:
    - src-tauri/crates/uc-platform/src/adapters/file_transfer/queue.rs
    - src-tauri/crates/uc-platform/src/adapters/file_transfer/retry.rs
  modified:
    - src-tauri/crates/uc-platform/src/adapters/file_transfer/mod.rs
    - src-tauri/crates/uc-tauri/src/bootstrap/runtime.rs

key-decisions:
  - 'Queue and retry modules created together since queue.rs depends on retry.rs (circular module reference resolved)'
  - "File cache directory derived from storage_paths.cache_dir.join('file-cache') rather than adding to AppConfig"
  - 'NoopFileTransportPort remains in wiring.rs for now (TODO for real adapter integration)'

patterns-established:
  - 'Serial queue pattern: mpsc channel with single consumer loop for ordered processing'
  - 'TransferError categorization: is_retriable() method drives retry policy decisions'

requirements-completed: [FSYNC-TRANSFER]

# Metrics
duration: 4min
completed: 2026-03-13
---

# Phase 30 Plan 03: Queue, Retry, and Bootstrap Wiring Summary

**Serial FIFO transfer queue with exponential backoff retry policy and UseCases accessor wiring for file sync**

## Performance

- **Duration:** 4 min
- **Started:** 2026-03-13T11:50:29Z
- **Completed:** 2026-03-13T11:54:50Z
- **Tasks:** 3
- **Files modified:** 4

## Accomplishments

- Serial FIFO FileTransferQueue processes one transfer at a time with append-during-transfer support
- RetryPolicy with configurable exponential backoff (1s, 2s, 4s...) retries only on network errors
- Non-retriable errors (hash mismatch, rejection, file error) fail immediately without retry
- UseCases accessor methods sync_outbound_file() and sync_inbound_file() wired in bootstrap runtime
- 12 unit tests covering queue ordering, serial processing, retry logic, and backoff behavior

## Task Commits

Each task was committed atomically:

1. **Task 1: Create serial transfer queue** - `7ebff5c3` (feat) - also includes retry.rs due to module dependency
2. **Task 2: Create retry policy with exponential backoff** - included in `7ebff5c3` (queue.rs depends on retry.rs)
3. **Task 3: Wire FileTransferService and use cases into bootstrap** - `cba391ad` (feat)

## Files Created/Modified

- `src-tauri/crates/uc-platform/src/adapters/file_transfer/queue.rs` - Serial FIFO transfer queue with FileTransferRequest, FileTransferQueue, and TransferError types
- `src-tauri/crates/uc-platform/src/adapters/file_transfer/retry.rs` - Exponential backoff RetryPolicy with configurable max_retries, initial_delay, max_delay, and multiplier
- `src-tauri/crates/uc-platform/src/adapters/file_transfer/mod.rs` - Added queue and retry module declarations with re-exports
- `src-tauri/crates/uc-tauri/src/bootstrap/runtime.rs` - Added sync_outbound_file() and sync_inbound_file() to UseCases accessor

## Decisions Made

- Queue and retry modules created in a single commit because queue.rs imports retry::RetryPolicy (co-dependent modules)
- File cache directory is `{cache_dir}/file-cache` derived from existing AppPaths rather than adding a new field to AppConfig
- NoopFileTransportPort stub remains in NetworkPorts wiring until a real FileTransferService adapter is integrated

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Added 'static bound to Future type parameter in FileTransferQueue::spawn**

- **Found during:** Task 1 (queue compilation)
- **Issue:** Compiler required `'static` bound on `Fut` because futures are spawned via `tokio::spawn`
- **Fix:** Added `+ 'static` to `Fut` type bound in `spawn()` and `process_loop()`
- **Files modified:** queue.rs
- **Verification:** All queue tests pass

**2. [Rule 1 - Bug] Removed unused `info` import from retry.rs**

- **Found during:** Task 3 (cargo check)
- **Issue:** `tracing::info` imported but not used in retry.rs (only `warn` is used)
- **Fix:** Changed `use tracing::{info, warn}` to `use tracing::warn`
- **Files modified:** retry.rs
- **Verification:** Clean cargo check with no warnings

---

**Total deviations:** 2 auto-fixed (2 bugs)
**Impact on plan:** Both auto-fixes were minor compile-time issues. No scope creep.

## Issues Encountered

None

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- File transfer queue, retry policy, and use case accessors are fully wired
- Phase 31 can build file sync UI with Dashboard file entries, context menu, progress, and notifications
- Phase 32 can add file sync settings UI, quota enforcement, and auto-cleanup

---

_Phase: 30-file-transfer-service-chunked-protocol-use-cases-retry-logic_
_Completed: 2026-03-13_
