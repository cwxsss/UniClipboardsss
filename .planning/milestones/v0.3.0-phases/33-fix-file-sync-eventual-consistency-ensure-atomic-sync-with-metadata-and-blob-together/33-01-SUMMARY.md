---
phase: 33-fix-file-sync-eventual-consistency-ensure-atomic-sync-with-metadata-and-blob-together
plan: 01
subsystem: file-sync
tags: [hexagonal-architecture, state-machine, file-transfer, ports, projections]

# Dependency graph
requires:
  - phase: 28
    provides: file_transfer table schema and FileTransferMapping protocol type
  - phase: 32.1
    provides: SyncInboundClipboardUseCase with file_transfers path rewriting
provides:
  - FileTransferRepositoryPort hexagonal contract for receiver-side transfer tracking
  - TrackedFileTransferStatus domain types (Pending/Transferring/Completed/Failed)
  - TrackInboundTransfersUseCase with full state machine orchestration
  - InboundApplyOutcome with pending_transfers linkage for file-backed messages
  - EntryProjectionDto with aggregate file transfer status fields
  - NoopFileTransferRepositoryPort stub for pre-adapter compilation
affects: [33-02-infra-repository, 33-03-tauri-platform-wiring, 33-04-frontend-durable-status]

# Tech tracking
tech-stack:
  added: []
  patterns: [noop-port-stub, aggregate-status-priority-rule, pending-transfer-linkage]

key-files:
  created:
    - src-tauri/crates/uc-core/src/ports/file_transfer_repository.rs
    - src-tauri/crates/uc-app/src/usecases/file_sync/track_inbound_transfers.rs
  modified:
    - src-tauri/crates/uc-core/src/ports/mod.rs
    - src-tauri/crates/uc-app/src/deps.rs
    - src-tauri/crates/uc-app/src/usecases/file_sync/mod.rs
    - src-tauri/crates/uc-app/src/usecases/clipboard/sync_inbound.rs
    - src-tauri/crates/uc-app/src/usecases/clipboard/list_entry_projections/list_entry_projections.rs
    - src-tauri/crates/uc-tauri/src/bootstrap/wiring.rs
    - src-tauri/crates/uc-tauri/src/bootstrap/runtime.rs
    - src-tauri/crates/uc-tauri/src/commands/clipboard.rs
    - src-tauri/crates/uc-tauri/src/commands/encryption.rs

key-decisions:
  - 'String-based entry_id in port types to avoid coupling to uc_ids across crate boundaries'
  - 'NoopFileTransferRepositoryPort stub for compilation before infra adapter lands in Plan 02'
  - 'PendingTransferLinkage returned from InboundApplyOutcome for Tauri layer to emit status without re-deriving'
  - 'Aggregate status priority: failed > transferring > pending > completed'

patterns-established:
  - 'Noop port stub pattern: provide NoopXxxPort alongside trait for pre-adapter compilation'
  - 'Transfer linkage pattern: apply outcome carries enough data for platform layer to emit events'

requirements-completed: [FSYNC-CONSISTENCY]

# Metrics
duration: 13min
completed: 2026-03-15
---

# Phase 33 Plan 01: Core/App Transfer Tracking Contract Summary

**Receiver-side file transfer state machine contract in uc-core/uc-app with metadata-seeded pending records and aggregate transfer status in clipboard projections**

## Performance

- **Duration:** 13 min
- **Started:** 2026-03-15T02:49:04Z
- **Completed:** 2026-03-15T03:02:04Z
- **Tasks:** 2
- **Files modified:** 11

## Accomplishments

- Defined hexagonal FileTransferRepositoryPort with full state machine contract (seed, promote, complete, fail, expire, reconcile, aggregate)
- Added TrackInboundTransfersUseCase orchestrating all state transitions with locked timeout budgets (60s pending, 5min transferring)
- Extended InboundApplyOutcome to carry pending transfer linkage so platform layer can emit status immediately
- Surfaced aggregate file transfer status in clipboard list projections without depending on transient frontend state
- 59 tests passing (8 core aggregate tests + 10 app tracking tests + 15 projection tests + 26 sync_inbound tests)

## Task Commits

1. **Task 1: Add core/app transfer tracking contract and metadata-seeding use case** - `4c139474` (feat)
2. **Task 2: Surface aggregate file transfer state in clipboard projections** - `73870b03` (feat)

## Files Created/Modified

- `src-tauri/crates/uc-core/src/ports/file_transfer_repository.rs` - Hexagonal port with domain types, aggregate computation, and noop stub
- `src-tauri/crates/uc-core/src/ports/mod.rs` - Register and re-export new port
- `src-tauri/crates/uc-app/src/usecases/file_sync/track_inbound_transfers.rs` - App-layer use case with state transitions and mock-based tests
- `src-tauri/crates/uc-app/src/usecases/file_sync/mod.rs` - Register track_inbound_transfers module
- `src-tauri/crates/uc-app/src/deps.rs` - Add file_transfer_repo to StoragePorts
- `src-tauri/crates/uc-app/src/usecases/clipboard/sync_inbound.rs` - Extended InboundApplyOutcome with pending_transfers, updated all return sites
- `src-tauri/crates/uc-app/src/usecases/clipboard/list_entry_projections/list_entry_projections.rs` - Added transfer status fields to DTO, query aggregate status per entry
- `src-tauri/crates/uc-tauri/src/bootstrap/wiring.rs` - Updated InboundApplyOutcome pattern matching for new fields, noop in StoragePorts
- `src-tauri/crates/uc-tauri/src/bootstrap/runtime.rs` - Pass file_transfer_repo to ListClipboardEntryProjections, noop in StoragePorts
- `src-tauri/crates/uc-tauri/src/commands/clipboard.rs` - Noop in test StoragePorts
- `src-tauri/crates/uc-tauri/src/commands/encryption.rs` - Noop in test StoragePorts

## Decisions Made

- Used String-based entry_id in port types rather than uc_ids::EntryId to avoid coupling across crate boundaries
- Added NoopFileTransferRepositoryPort alongside the trait so all construction sites compile before Plan 02 adds the Diesel adapter
- Returned PendingTransferLinkage from InboundApplyOutcome so Tauri layer can emit pending status without guessing transfer-to-entry linkage
- Aggregate status uses strict priority rule (failed > transferring > pending > completed) matching the research recommendation

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Updated wiring.rs and command test files for new StoragePorts field**

- **Found during:** Task 1
- **Issue:** Adding file_transfer_repo to StoragePorts broke all construction sites in wiring.rs, runtime.rs, and test files
- **Fix:** Added NoopFileTransferRepositoryPort at all StoragePorts construction sites
- **Files modified:** wiring.rs, runtime.rs, clipboard.rs (commands), encryption.rs (commands)
- **Committed in:** 4c139474 (Task 1 commit)

---

**Total deviations:** 1 auto-fixed (1 blocking)
**Impact on plan:** Necessary for compilation. No scope creep.

## Issues Encountered

None

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Core/app contract complete, ready for Plan 02 (Diesel repository implementation)
- All construction sites use NoopFileTransferRepositoryPort, ready for real adapter swap
- Projection DTO extended, Tauri model mapping deferred to Plan 03/04

---

_Phase: 33-fix-file-sync-eventual-consistency-ensure-atomic-sync-with-metadata-and-blob-together_
_Completed: 2026-03-15_
