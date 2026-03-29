---
phase: 24-implement-per-device-sync-settings-for-paired-devices
plan: 02
subsystem: sync
tags: [tauri-commands, use-cases, sync-engine, per-device-settings]

requires:
  - phase: 24-implement-per-device-sync-settings-for-paired-devices
    provides: PairedDevice.sync_settings field, resolve_sync_settings(), update_sync_settings() repo method

provides:
  - GetDeviceSyncSettings use case returning resolved (per-device or global) sync settings
  - UpdateDeviceSyncSettings use case for setting or clearing per-device overrides
  - get_device_sync_settings and update_device_sync_settings Tauri commands
  - Outbound sync engine filtering peers by per-device auto_sync toggle

affects: [24-03 frontend UI, sync behavior]

tech-stack:
  added: []
  patterns: [from_ports constructor for use cases, filter_by_auto_sync pre-send peer filtering]

key-files:
  created:
    - src-tauri/crates/uc-app/src/usecases/pairing/get_device_sync_settings.rs
    - src-tauri/crates/uc-app/src/usecases/pairing/update_device_sync_settings.rs
  modified:
    - src-tauri/crates/uc-app/src/usecases/pairing/mod.rs
    - src-tauri/crates/uc-app/src/usecases/mod.rs
    - src-tauri/crates/uc-tauri/src/commands/pairing.rs
    - src-tauri/crates/uc-tauri/src/bootstrap/runtime.rs
    - src-tauri/src/main.rs
    - src-tauri/crates/uc-app/src/usecases/clipboard/sync_outbound.rs
    - src-tauri/crates/uc-app/tests/clipboard_sync_e2e_test.rs

key-decisions:
  - 'Settings loaded from storage each time (not cached) per user decision'
  - 'Peers not in paired_device table proceed with sync as safety fallback'
  - 'Per-device auto_sync filtering happens before ensure_business_path to avoid unnecessary connections'

patterns-established:
  - 'from_ports() constructor pattern for use cases requiring multiple port dependencies'
  - 'filter_by_auto_sync pre-send peer filtering with warn-and-continue error handling'

requirements-completed: [DEVSYNC-04, DEVSYNC-05]

duration: 6min
completed: 2026-03-11
---

# Phase 24 Plan 02: Use Cases, Commands, and Sync Engine Integration Summary

**Two Tauri commands for per-device sync settings with outbound sync engine auto_sync filtering via resolve_sync_settings**

## Performance

- **Duration:** 6 min
- **Started:** 2026-03-11T14:49:46Z
- **Completed:** 2026-03-11T14:56:30Z
- **Tasks:** 2
- **Files modified:** 9

## Accomplishments

- GetDeviceSyncSettings use case resolves effective settings (per-device override or global fallback)
- UpdateDeviceSyncSettings use case updates or clears per-device overrides (None resets to global)
- Both commands registered in main.rs invoke_handler and accessible from frontend
- Outbound sync engine filters peers by effective auto_sync before sending -- disabled peers are skipped
- All 13 existing sync_outbound unit tests pass unchanged
- All e2e clipboard sync tests updated and passing

## Task Commits

Each task was committed atomically:

1. **Task 1: Create use cases and Tauri commands for device sync settings** - `868ebf90` (feat)
2. **Task 2: Integrate per-device auto_sync check into outbound sync engine** - `c2e409a9` (feat)

## Files Created/Modified

- `src-tauri/crates/uc-app/src/usecases/pairing/get_device_sync_settings.rs` - GetDeviceSyncSettings use case with resolve_sync_settings
- `src-tauri/crates/uc-app/src/usecases/pairing/update_device_sync_settings.rs` - UpdateDeviceSyncSettings use case with None-reset support
- `src-tauri/crates/uc-app/src/usecases/pairing/mod.rs` - Module exports for new use cases
- `src-tauri/crates/uc-app/src/usecases/mod.rs` - Re-exports for GetDeviceSyncSettings, UpdateDeviceSyncSettings
- `src-tauri/crates/uc-tauri/src/commands/pairing.rs` - Two new Tauri commands with spans and error handling
- `src-tauri/crates/uc-tauri/src/bootstrap/runtime.rs` - UseCases accessor methods + paired_device_repo in sync_outbound
- `src-tauri/src/main.rs` - Command registration in generate_handler
- `src-tauri/crates/uc-app/src/usecases/clipboard/sync_outbound.rs` - filter_by_auto_sync + paired_device_repo field
- `src-tauri/crates/uc-app/tests/clipboard_sync_e2e_test.rs` - NoopPairedDeviceRepo for e2e tests

## Decisions Made

- Settings loaded from storage on every call (not cached) -- SQLite + WAL is fast for 2-5 devices
- Peers not found in paired_device table proceed with sync (safety fallback for peers not yet persisted)
- Per-device auto_sync filtering applied before ensure_business_path to avoid unnecessary network connections
- Used from_ports() constructor (instead of new()) to match plan specification

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Updated e2e test file with new constructor parameter**

- **Found during:** Task 2
- **Issue:** clipboard_sync_e2e_test.rs also constructs SyncOutboundClipboardUseCase and needed the new paired_device_repo parameter
- **Fix:** Added NoopPairedDeviceRepo and passed it to all 4 constructor calls in e2e test
- **Files modified:** src-tauri/crates/uc-app/tests/clipboard_sync_e2e_test.rs
- **Verification:** All e2e tests pass
- **Committed in:** c2e409a9 (Task 2 commit)

---

**Total deviations:** 1 auto-fixed (1 blocking)
**Impact on plan:** Necessary to maintain existing test compatibility after adding new constructor parameter.

## Issues Encountered

None

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Use cases and commands ready for frontend integration in plan 03
- Outbound sync engine respects per-device auto_sync toggle
- resolve_sync_settings fallback chain working: per-device -> global defaults

---

_Phase: 24-implement-per-device-sync-settings-for-paired-devices_
_Completed: 2026-03-11_
