---
phase: 30-file-transfer-service-chunked-protocol-use-cases-retry-logic
plan: 02
subsystem: app
tags: [file-sync, use-cases, blake3, quota, sync-policy]

requires:
  - phase: 28-file-sync-foundation
    provides: FileTransportPort trait, FileSyncSettings, content type filter for File category
provides:
  - SyncOutboundFileUseCase with file safety checks and peer selection
  - SyncInboundFileUseCase with quota enforcement and disk space checks
  - Shared sync_policy module for peer filtering reusable by clipboard and file sync
affects: [31-file-sync-ui, 32-file-sync-settings-polish]

tech-stack:
  added: [blake3, libc]
  patterns: [shared-sync-policy, file-safety-validation, quota-enforcement]

key-files:
  created:
    - src-tauri/crates/uc-app/src/usecases/file_sync/mod.rs
    - src-tauri/crates/uc-app/src/usecases/file_sync/sync_policy.rs
    - src-tauri/crates/uc-app/src/usecases/file_sync/sync_outbound.rs
    - src-tauri/crates/uc-app/src/usecases/file_sync/sync_inbound.rs
  modified:
    - src-tauri/crates/uc-app/src/usecases/mod.rs
    - src-tauri/crates/uc-app/Cargo.toml

key-decisions:
  - 'Used libc::statvfs directly for disk space check instead of adding fs2 dependency'
  - 'Hash verification failure deletes temp file immediately with no retry policy'
  - 'File transport delegation is logged-only pending Phase 30 Plan 01 chunked send wiring'

patterns-established:
  - 'Shared sync policy: extract peer filtering into standalone async fn for reuse across content types'
  - 'File safety validation: symlink_metadata + nlink check + existence race guard before transfer'

requirements-completed: [FSYNC-TRANSFER]

duration: 4min
completed: 2026-03-13
---

# Phase 30 Plan 02: File Sync Use Cases Summary

**SyncOutbound/InboundFileUseCase with symlink/hardlink rejection, shared sync policy filtering, per-device 500MB quota, and Blake3 hash verification**

## Performance

- **Duration:** 4 min
- **Started:** 2026-03-13T11:41:05Z
- **Completed:** 2026-03-13T11:46:00Z
- **Tasks:** 1
- **Files modified:** 6

## Accomplishments

- SyncOutboundFileUseCase validates file safety (symlinks, hardlinks, existence) before peer selection
- SyncInboundFileUseCase enforces per-device cache quota and disk space pre-checks
- Shared sync_policy module extracts peer filtering for reuse between clipboard and file sync
- Blake3 hash verification deletes temp file on mismatch with no retry
- 16 unit tests covering all safety checks, policy filtering, quota enforcement, and hash scenarios

## Task Commits

Each task was committed atomically:

1. **Task 1: Create shared sync policy module and file sync use cases** - `13bffc19` (feat)

**Plan metadata:** pending (docs: complete plan)

## Files Created/Modified

- `src-tauri/crates/uc-app/src/usecases/file_sync/mod.rs` - Module root with re-exports
- `src-tauri/crates/uc-app/src/usecases/file_sync/sync_policy.rs` - Shared peer filtering by auto_sync and file content type
- `src-tauri/crates/uc-app/src/usecases/file_sync/sync_outbound.rs` - Outbound use case with file safety validation
- `src-tauri/crates/uc-app/src/usecases/file_sync/sync_inbound.rs` - Inbound use case with quota, disk space, hash verification
- `src-tauri/crates/uc-app/src/usecases/mod.rs` - Added file_sync module and re-exports
- `src-tauri/crates/uc-app/Cargo.toml` - Added blake3 and libc dependencies

## Decisions Made

- Used libc::statvfs directly for Unix disk space check instead of adding fs2 crate dependency
- Hash verification failure deletes temp file immediately -- no retry on hash mismatch per plan
- File transport delegation logs intent per peer; actual chunked send wiring deferred to Plan 01 integration

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

- Missing `#[derive(Debug)]` on result structs caused test compilation failure -- added Debug derive (trivial fix)

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- File sync use cases ready for integration with FileTransferService (Plan 01)
- Shared sync_policy module available for clipboard sync refactoring if desired
- Inbound use case ready for wiring into event handler when file transfer events arrive

---

_Phase: 30-file-transfer-service-chunked-protocol-use-cases-retry-logic_
_Completed: 2026-03-13_
