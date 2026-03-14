---
phase: 31-file-sync-settings-and-polish-settings-ui-quota-enforcement-auto-cleanup
plan: 02
subsystem: file-sync
tags: [cleanup, quota, file-cache, background-task, use-case]

requires:
  - phase: 28-file-sync-foundation
    provides: FileSyncSettings model with retention_hours, auto_cleanup, quota_per_device
  - phase: 29-file-transfer-service
    provides: File cache directory structure and SyncInboundFileUseCase
provides:
  - CleanupExpiredFilesUseCase for filesystem-based expired file removal
  - check_device_quota function with QuotaExceededError typed error
  - Startup background task for automatic file cache cleanup
  - cleanup_expired_files accessor on UseCases struct
affects: [file-sync-settings-ui, sync-inbound-file]

tech-stack:
  added: []
  patterns: [filesystem-based-cleanup, fire-and-forget-startup-task]

key-files:
  created:
    - src-tauri/crates/uc-app/src/usecases/file_sync/cleanup.rs
  modified:
    - src-tauri/crates/uc-app/src/usecases/file_sync/mod.rs
    - src-tauri/crates/uc-tauri/src/bootstrap/runtime.rs
    - src-tauri/crates/uc-tauri/src/bootstrap/wiring.rs

key-decisions:
  - 'Filesystem-based cleanup instead of DB repository: no FileEntryRepository port exists yet, file-cache directory is the source of truth'
  - 'Retention comparison uses >= (not >) so retention_hours=0 means clean immediately'
  - 'Cleanup module placed in file_sync/ (not file/) to match existing module structure'

patterns-established:
  - 'Fire-and-forget startup task: spawn via TaskRegistry, log warn on failure, never block startup'

requirements-completed: [FSYNC-POLISH]

duration: 5min
completed: 2026-03-14
---

# Phase 31 Plan 02: Auto-Cleanup and Quota Enforcement Summary

**Filesystem-based file cache cleanup use case with configurable retention, per-device quota check function, and non-blocking startup task registration**

## Performance

- **Duration:** 5 min
- **Started:** 2026-03-14T14:00:09Z
- **Completed:** 2026-03-14T14:05:00Z
- **Tasks:** 2
- **Files modified:** 4

## Accomplishments

- CleanupExpiredFilesUseCase removes cache files older than retention period with summary logging
- check_device_quota function validates per-device cache usage against configurable quota
- Cleanup task registered as fire-and-forget startup background task via TaskRegistry
- 5 unit tests covering disabled cleanup, missing cache dir, expired file removal, and quota enforcement

## Task Commits

Each task was committed atomically:

1. **Task 1: Create CleanupExpiredFilesUseCase** - `2b00cf58` (feat)
2. **Task 2: Register cleanup task and wire UseCases accessor** - `982a0c3b` (feat)

## Files Created/Modified

- `src-tauri/crates/uc-app/src/usecases/file_sync/cleanup.rs` - CleanupExpiredFilesUseCase, check_device_quota, QuotaExceededError, CleanupResult
- `src-tauri/crates/uc-app/src/usecases/file_sync/mod.rs` - Added cleanup module and re-exports
- `src-tauri/crates/uc-tauri/src/bootstrap/runtime.rs` - Added cleanup_expired_files() accessor to UseCases
- `src-tauri/crates/uc-tauri/src/bootstrap/wiring.rs` - Spawned file_cache_cleanup task in start_background_tasks

## Decisions Made

- Used filesystem-based cleanup (walk directory tree, check modified time) instead of DB repository queries since no FileEntryRepository port exists yet
- Placed cleanup module in existing `file_sync/` module rather than creating new `file/` directory as plan suggested, to maintain module naming consistency
- Used `>=` comparison for retention to allow retention_hours=0 to mean "clean everything immediately"

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Module path adjusted from file/ to file_sync/**

- **Found during:** Task 1
- **Issue:** Plan specified `usecases/file/cleanup.rs` but the existing module is `usecases/file_sync/`
- **Fix:** Created cleanup module inside existing `file_sync/` module
- **Files modified:** src-tauri/crates/uc-app/src/usecases/file_sync/cleanup.rs, mod.rs
- **Verification:** cargo check -p uc-app passes
- **Committed in:** 2b00cf58

**2. [Rule 3 - Blocking] Filesystem-based approach instead of DB repository**

- **Found during:** Task 1
- **Issue:** Plan assumed FileEntryRepository port and FileTransferStatus enum exist, but they don't
- **Fix:** Implemented cleanup using filesystem metadata (modified time) instead of DB queries
- **Files modified:** src-tauri/crates/uc-app/src/usecases/file_sync/cleanup.rs
- **Verification:** All tests pass, cargo check passes
- **Committed in:** 2b00cf58

---

**Total deviations:** 2 auto-fixed (2 blocking)
**Impact on plan:** Both fixes necessary due to plan assumptions about non-existent infrastructure. Filesystem-based approach is consistent with existing SyncInboundFileUseCase patterns.

## Issues Encountered

None

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Cleanup use case and quota check function ready for use by settings UI and inbound file sync
- cleanup_expired_files accessor available for future Tauri commands if needed

---

_Phase: 31-file-sync-settings-and-polish-settings-ui-quota-enforcement-auto-cleanup_
_Completed: 2026-03-14_
