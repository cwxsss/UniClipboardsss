---
phase: 33-fix-file-sync-eventual-consistency-ensure-atomic-sync-with-metadata-and-blob-together
plan: 03
subsystem: infra, platform
tags: [tauri, file-transfer, event-loop, wiring, sqlite, timeout-sweep, startup-reconciliation]

# Dependency graph
requires:
  - phase: 33-01
    provides: Core/app transfer tracking contract and metadata-seeding use case
  - phase: 33-02
    provides: Infra SQLite repository implementing FileTransferRepositoryPort
provides:
  - Real DieselFileTransferRepository wired into runtime (replacing Noop)
  - TrackInboundTransfersUseCase accessible via runtime.usecases()
  - file-transfer://status-changed event emission for all transfer state transitions
  - Periodic timeout sweep for stalled pending (60s) and transferring (5min) rows
  - Startup reconciliation marking orphaned in-flight transfers as failed
  - Clipboard command responses include file_transfer_status and file_transfer_reason
affects: [33-04-frontend-durable-status-ux, 33-05-validation]

# Tech tracking
tech-stack:
  added: []
  patterns: [file-transfer-wiring-module, arc-tracker-in-spawn, camelCase-event-payload]

key-files:
  created:
    - src-tauri/crates/uc-tauri/src/bootstrap/file_transfer_wiring.rs
  modified:
    - src-tauri/crates/uc-tauri/src/bootstrap/runtime.rs
    - src-tauri/crates/uc-tauri/src/bootstrap/wiring.rs
    - src-tauri/crates/uc-tauri/src/bootstrap/mod.rs
    - src-tauri/crates/uc-tauri/src/events/transfer_progress.rs
    - src-tauri/crates/uc-tauri/src/models/mod.rs
    - src-tauri/crates/uc-tauri/src/commands/clipboard.rs
    - src-tauri/crates/uc-tauri/tests/models_serialization_test.rs
    - src-tauri/crates/uc-core/src/ports/file_transfer_repository.rs
    - src-tauri/crates/uc-infra/src/db/repositories/file_transfer_repo.rs
    - src-tauri/crates/uc-app/src/usecases/file_sync/track_inbound_transfers.rs
    - src-tauri/crates/uc-app/src/usecases/clipboard/list_entry_projections/list_entry_projections.rs

key-decisions:
  - 'Arc<TrackInboundTransfersUseCase> shared across spawned tasks for durable marking inside async spawns'
  - 'get_entry_id_for_transfer added to port for progress-event-to-entry-id resolution (transfer_id-only context)'
  - 'file-transfer:// namespace prefix unifies all file transfer events under one prefix'
  - 'Timeout sweep uses tokio::watch for cooperative cancellation instead of TaskRegistry token'

patterns-established:
  - 'file_transfer_wiring: dedicated orchestration module for transfer lifecycle event handling'
  - 'FileTransferStatusPayload: camelCase-serialized struct for status-changed events'
  - 'dto_to_projection: centralized DTO mapping in commands/clipboard.rs'

requirements-completed: [FSYNC-CONSISTENCY]

# Metrics
duration: 20min
completed: 2026-03-15
---

# Phase 33 Plan 03: Tauri/Platform Integration Summary

**Durable file transfer tracking wired into Tauri runtime with event emission, timeout sweep, startup reconciliation, and repository-backed command responses**

## Performance

- **Duration:** 20 min
- **Started:** 2026-03-15T03:14:10Z
- **Completed:** 2026-03-15T03:34:08Z
- **Tasks:** 2
- **Files modified:** 12

## Accomplishments

- Replaced NoopFileTransferRepositoryPort with real DieselFileTransferRepository in production wiring
- Created file_transfer_wiring.rs with orchestration for all 6 transfer lifecycle behaviors
- Clipboard commands now return persisted file_transfer_status and file_transfer_reason for file entries
- Startup reconciliation marks orphaned in-flight transfers as failed without blocking app launch
- Periodic 15-second timeout sweep catches stalled pending (>60s) and transferring (>5min) transfers

## Task Commits

Each task was committed atomically:

1. **Task 1: Wire repository-backed app use cases into runtime and clipboard commands** - `2c001104` (feat)
2. **Task 2: Orchestrate pending/transferring/completed/failed events and startup reconciliation** - `87e15d0d` (feat)

## Files Created/Modified

- `src-tauri/crates/uc-tauri/src/bootstrap/file_transfer_wiring.rs` - New module: event-loop orchestration for transfer lifecycle
- `src-tauri/crates/uc-tauri/src/bootstrap/runtime.rs` - Added track_inbound_transfers() use case accessor
- `src-tauri/crates/uc-tauri/src/bootstrap/wiring.rs` - Real repo wiring, event handling for progress/completed/failed/reconciliation
- `src-tauri/crates/uc-tauri/src/bootstrap/mod.rs` - Added file_transfer_wiring module
- `src-tauri/crates/uc-tauri/src/events/transfer_progress.rs` - Renamed to file-transfer:// namespace
- `src-tauri/crates/uc-tauri/src/models/mod.rs` - Extended ClipboardEntryProjection with transfer status fields
- `src-tauri/crates/uc-tauri/src/commands/clipboard.rs` - dto_to_projection helper mapping new fields
- `src-tauri/crates/uc-tauri/tests/models_serialization_test.rs` - Added 3 transfer status serialization tests
- `src-tauri/crates/uc-core/src/ports/file_transfer_repository.rs` - Added get_entry_id_for_transfer to port
- `src-tauri/crates/uc-infra/src/db/repositories/file_transfer_repo.rs` - Implemented get_entry_id_for_transfer
- `src-tauri/crates/uc-app/src/usecases/file_sync/track_inbound_transfers.rs` - Added get_entry_summary_by_transfer
- `src-tauri/crates/uc-app/src/usecases/clipboard/list_entry_projections/list_entry_projections.rs` - Updated mock

## Decisions Made

- Used Arc<TrackInboundTransfersUseCase> to share tracker across spawned tasks in the event loop
- Added get_entry_id_for_transfer to the port rather than maintaining a runtime HashMap, since the DB is the source of truth
- Unified transfer events under file-transfer:// namespace prefix (was transfer://)
- Used tokio::watch for timeout sweep cancellation rather than TaskRegistry to keep it simpler
- FileTransferStatusPayload uses skip_serializing_if for optional reason field

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Added get_entry_id_for_transfer to FileTransferRepositoryPort**

- **Found during:** Task 2 (file_transfer_wiring.rs needed transfer_id-to-entry_id lookup for status events)
- **Issue:** Port had no method to look up entry_id by transfer_id, needed for progress/completed/failed event emission
- **Fix:** Added get_entry_id_for_transfer to port + Noop + infra adapter + app use case + all mocks
- **Files modified:** file_transfer_repository.rs, file_transfer_repo.rs, track_inbound_transfers.rs, list_entry_projections.rs
- **Verification:** All existing tests pass + infra tests pass
- **Committed in:** 87e15d0d

---

**Total deviations:** 1 auto-fixed (1 blocking)
**Impact on plan:** Essential for correct event emission. No scope creep.

## Issues Encountered

- ClockPort method is `now_ms()` not `now_millis()` as initially written - fixed after first compile error
- TransferDirection doesn't implement Copy, needed .clone() for partial move in progress handler

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Tauri bridge complete: durable transfer truth flows to both events and command responses
- Frontend (Plan 04) can now consume file-transfer://status-changed and persisted transfer status
- All file transfer lifecycle transitions are repository-backed and restart-safe

---

_Phase: 33-fix-file-sync-eventual-consistency-ensure-atomic-sync-with-metadata-and-blob-together_
_Completed: 2026-03-15_
