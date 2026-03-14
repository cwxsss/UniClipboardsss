---
phase: 31-file-sync-settings-and-polish-settings-ui-quota-enforcement-auto-cleanup
plan: 03
subsystem: file-sync
tags: [rust, use-case, guards, quota, error-handling, file-transfer]

requires:
  - phase: 29-file-transfer-service
    provides: SyncOutboundFileUseCase and SyncInboundFileUseCase
  - phase: 31-02
    provides: check_device_quota function and CleanupExpiredFilesUseCase
provides:
  - file_sync_enabled gate on both outbound and inbound file sync
  - max_file_size guard on outbound file sync
  - Quota enforcement delegation from inbound use case to cleanup module
  - Standardized transfer_errors module for consistent error messages
  - cleanup_temp_file helper for robust temp file cleanup
affects: [file-transfer-service, wiring]

tech-stack:
  added: []
  patterns: [early-return-guards, standardized-error-messages, temp-file-cleanup-helper]

key-files:
  created: []
  modified:
    - src-tauri/crates/uc-app/src/usecases/file_sync/sync_outbound.rs
    - src-tauri/crates/uc-app/src/usecases/file_sync/sync_inbound.rs
    - src-tauri/crates/uc-app/src/usecases/file_sync/mod.rs

key-decisions:
  - 'Guards return Result errors (not events) since use cases have no event channel access; callers handle event emission'
  - 'max_file_size guard uses bail! to propagate error to caller rather than silent return'
  - 'cleanup_temp_file helper extracted for reuse across hash mismatch and disabled-sync paths'
  - 'transfer_errors module provides constants and formatters for consistent user-facing messages'

patterns-established:
  - 'Early-return guard pattern: load settings once, check file_sync_enabled first, then size/quota guards'
  - 'Standardized error message module for file transfer pipeline'

requirements-completed: [FSYNC-POLISH]

duration: 5min
completed: 2026-03-14
---

# Phase 31 Plan 03: File Sync Guards and Error Standardization Summary

**file_sync_enabled and max_file_size guards on outbound, file_sync_enabled and quota enforcement on inbound, with standardized transfer_errors module and temp file cleanup helper**

## Performance

- **Duration:** 5 min
- **Started:** 2026-03-14T14:09:27Z
- **Completed:** 2026-03-14T14:14:44Z
- **Tasks:** 2
- **Files modified:** 3

## Accomplishments

- Outbound file sync gated by file_sync_enabled (returns 0 peers) and max_file_size (returns error with descriptive message)
- Inbound file sync gated by file_sync_enabled in handle_transfer_complete with temp file cleanup
- check_quota_for_transfer delegates to cleanup module's check_device_quota for quota enforcement
- transfer_errors module provides standardized message constants and formatters
- cleanup_temp_file helper ensures consistent temp file removal across all error paths
- 7 new tests covering all guard paths

## Task Commits

Each task was committed atomically:

1. **Task 1: Add file_sync_enabled and max_file_size guards to outbound use case** - `ea2ea668` (feat)
2. **Task 2: Add quota enforcement and file_sync_enabled guard to inbound use case** - `437f5c45` (feat)

## Files Created/Modified

- `src-tauri/crates/uc-app/src/usecases/file_sync/sync_outbound.rs` - Added file_sync_enabled early return and max_file_size bail guard with 2 tests
- `src-tauri/crates/uc-app/src/usecases/file_sync/sync_inbound.rs` - Added is_file_sync_enabled, check_quota_for_transfer, transfer_errors module, cleanup_temp_file helper, file_sync_enabled guard in handle_transfer_complete with 5 tests
- `src-tauri/crates/uc-app/src/usecases/file_sync/mod.rs` - Re-exported transfer_errors module

## Decisions Made

- Guards return Result errors rather than emitting NetworkEvent directly, since use cases don't have access to event channels (proper layer separation). Callers in the wiring/runtime layer handle event emission.
- max_file_size guard uses `bail!` to propagate error to caller rather than silently returning Ok, since the caller needs to know the file was rejected (for UI notification).
- Extracted `cleanup_temp_file` helper to avoid duplicating cleanup logic across hash mismatch and disabled-sync paths.
- Created `transfer_errors` module with constants and formatting functions for consistent error messages across the pipeline.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Adapted to actual architecture (no event channel in use cases)**

- **Found during:** Task 1
- **Issue:** Plan assumed use cases have access to event_tx for emitting NetworkEvent::FileTransferFailed, but use cases follow ports-and-adapters pattern without event channels
- **Fix:** Guards return descriptive errors via Result; callers (runtime.rs, wiring.rs) already handle error logging and event emission
- **Files modified:** sync_outbound.rs, sync_inbound.rs
- **Verification:** cargo check -p uc-app and cargo check -p uc-tauri both pass
- **Committed in:** ea2ea668, 437f5c45

**2. [Rule 3 - Blocking] Module path file/ vs file_sync/**

- **Found during:** Task 2
- **Issue:** Plan referenced `usecases/file/sync_inbound.rs` but actual module is `usecases/file_sync/sync_inbound.rs`
- **Fix:** Used correct file_sync/ module path
- **Files modified:** sync_inbound.rs, mod.rs
- **Committed in:** 437f5c45

---

**Total deviations:** 2 auto-fixed (2 blocking)
**Impact on plan:** Necessary adaptations to match actual architecture. No scope creep.

## Issues Encountered

None

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- All file sync guards active: file_sync_enabled, max_file_size, quota enforcement
- Standardized error messages available for UI consumption via transfer_errors module
- File sync pipeline production-ready with proper guards and error handling

---

_Phase: 31-file-sync-settings-and-polish-settings-ui-quota-enforcement-auto-cleanup_
_Completed: 2026-03-14_
