---
phase: 28-implement-file-sync-via-libp2p-for-cross-platform-file-copy-paste
plan: 3
subsystem: network
tags: [libp2p, file-transfer, protocol, diesel, ports]

requires:
  - phase: 28
    provides: FileTransferMessage binary codec, file classification fix, settings model
provides:
  - ProtocolId::FileTransfer variant (/uniclipboard/file-transfer/1.0.0)
  - FileTransportPort trait with send/receive/cancel methods
  - NoopFileTransportPort stub for compilation
  - NetworkEvent file transfer lifecycle variants (Started/Completed/Failed/Cancelled)
  - PlatformEvent::FileCopied variant
  - file_transfer database table with status/batch/cache indexes
  - FileTransportPort wired into NetworkPorts dependency graph
affects: [phase-30-file-transfer-service, phase-31-file-sync-ui]

tech-stack:
  added: []
  patterns: [noop-stub-for-port-wiring, file-transfer-event-lifecycle]

key-files:
  created:
    - src-tauri/crates/uc-core/src/ports/file_transport.rs
    - src-tauri/crates/uc-infra/migrations/2026-03-13-000001_create_file_transfer/up.sql
    - src-tauri/crates/uc-infra/migrations/2026-03-13-000001_create_file_transfer/down.sql
  modified:
    - src-tauri/crates/uc-core/src/network/protocol_ids.rs
    - src-tauri/crates/uc-core/src/network/events.rs
    - src-tauri/crates/uc-core/src/ports/mod.rs
    - src-tauri/crates/uc-platform/src/ipc/event.rs
    - src-tauri/crates/uc-platform/src/runtime/runtime.rs
    - src-tauri/crates/uc-app/src/deps.rs
    - src-tauri/crates/uc-infra/src/db/schema.rs
    - src-tauri/crates/uc-tauri/src/test_utils.rs
    - src-tauri/crates/uc-tauri/src/bootstrap/wiring.rs

key-decisions:
  - 'NoopFileTransportPort stub used at construction sites to keep builds green until Phase 30 adapter'
  - 'Manual schema.rs update since diesel CLI not available in environment'

patterns-established:
  - 'Noop port stub pattern: provide NoopXxxPort alongside trait for pre-adapter compilation'

requirements-completed: [FSYNC-FOUNDATION]

duration: 7min
completed: 2026-03-13
---

# Phase 28 Plan 3: Protocol Registration, Port Trait, Network Events, Database Schema, and Wiring Summary

**FileTransfer protocol ID, FileTransportPort trait with noop stub, file transfer lifecycle events, file_transfer DB table, and NetworkPorts wiring**

## Performance

- **Duration:** 7 min
- **Started:** 2026-03-13T09:44:20Z
- **Completed:** 2026-03-13T09:51:57Z
- **Tasks:** 2
- **Files modified:** 12

## Accomplishments

- Registered FileTransfer protocol ID with canonical path /uniclipboard/file-transfer/1.0.0
- Defined FileTransportPort trait with send_file_announce/data/complete and cancel_transfer methods plus NoopFileTransportPort stub
- Extended NetworkEvent with FileTransferStarted/Completed/Failed/Cancelled lifecycle variants
- Added PlatformEvent::FileCopied for file copy detection from system clipboard
- Created file_transfer database migration with indexes on status, batch_id, and created_at_ms
- Wired FileTransportPort into NetworkPorts struct with noop stub at all construction sites

## Task Commits

Each task was committed atomically:

1. **Task 1: Add protocol ID, FileTransportPort trait, and event extensions** - `24d86297` (feat)
2. **Task 2: Create database migration and wire FileTransportPort into deps** - `e9c944f6` (feat)

## Files Created/Modified

- `src-tauri/crates/uc-core/src/ports/file_transport.rs` - FileTransportPort trait + NoopFileTransportPort stub
- `src-tauri/crates/uc-core/src/network/protocol_ids.rs` - Added FileTransfer variant
- `src-tauri/crates/uc-core/src/network/events.rs` - File transfer lifecycle event variants + tests
- `src-tauri/crates/uc-core/src/ports/mod.rs` - Re-export FileTransportPort and NoopFileTransportPort
- `src-tauri/crates/uc-platform/src/ipc/event.rs` - PlatformEvent::FileCopied variant
- `src-tauri/crates/uc-platform/src/runtime/runtime.rs` - FileCopied match arm in event handler
- `src-tauri/crates/uc-app/src/deps.rs` - file_transfer field in NetworkPorts
- `src-tauri/crates/uc-infra/migrations/2026-03-13-000001_create_file_transfer/up.sql` - Create file_transfer table
- `src-tauri/crates/uc-infra/migrations/2026-03-13-000001_create_file_transfer/down.sql` - Drop file_transfer table
- `src-tauri/crates/uc-infra/src/db/schema.rs` - Diesel schema for file_transfer table
- `src-tauri/crates/uc-tauri/src/test_utils.rs` - NoopFileTransportPort in test network ports
- `src-tauri/crates/uc-tauri/src/bootstrap/wiring.rs` - NoopFileTransportPort in production wiring

## Decisions Made

- Used NoopFileTransportPort stub at NetworkPorts construction sites to keep all crates compiling; real adapter comes in Phase 30
- Manually updated schema.rs since diesel CLI is not installed in the environment; migration SQL files are authoritative

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Added FileCopied match arm in platform runtime**

- **Found during:** Task 1 (event extensions)
- **Issue:** Adding PlatformEvent::FileCopied caused exhaustive match error in runtime.rs
- **Fix:** Added match arm with debug log and TODO comment for Phase 30
- **Files modified:** src-tauri/crates/uc-platform/src/runtime/runtime.rs
- **Verification:** cargo check -p uc-platform passes
- **Committed in:** 24d86297 (Task 1 commit)

**2. [Rule 3 - Blocking] Updated NetworkPorts construction sites with noop stub**

- **Found during:** Task 2 (wiring)
- **Issue:** Adding file_transfer field to NetworkPorts broke construction in wiring.rs and test_utils.rs
- **Fix:** Used NoopFileTransportPort at both sites with TODO(phase-30) comments
- **Files modified:** src-tauri/crates/uc-tauri/src/bootstrap/wiring.rs, src-tauri/crates/uc-tauri/src/test_utils.rs
- **Verification:** cargo check -p uc-app -p uc-infra passes
- **Committed in:** e9c944f6 (Task 2 commit)

---

**Total deviations:** 2 auto-fixed (2 blocking)
**Impact on plan:** Both auto-fixes were necessary to keep builds green. Plan anticipated these (mentioned noop stub if needed). No scope creep.

## Issues Encountered

None

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Phase 28 foundation complete: message types, classification, settings, protocol ID, port trait, events, and schema all in place
- Phase 30 can implement the actual file transfer adapter against FileTransportPort
- Phase 31 can build UI against the NetworkEvent file transfer variants

---

_Phase: 28-implement-file-sync-via-libp2p-for-cross-platform-file-copy-paste_
_Completed: 2026-03-13_

## Self-Check: PASSED

All created files verified present. Both task commits (24d86297, e9c944f6) verified in git log.
