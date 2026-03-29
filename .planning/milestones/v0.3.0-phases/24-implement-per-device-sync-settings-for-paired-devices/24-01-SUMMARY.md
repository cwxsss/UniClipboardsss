---
phase: 24-implement-per-device-sync-settings-for-paired-devices
plan: 01
subsystem: database
tags: [diesel, sqlite, serde-json, paired-device, sync-settings]

requires:
  - phase: none
    provides: existing PairedDevice model and repository
provides:
  - PairedDevice domain model with optional sync_settings field
  - resolve_sync_settings pure function for device/global fallback
  - update_sync_settings method on PairedDeviceRepositoryPort
  - Diesel migration adding sync_settings nullable TEXT column
  - JSON ser/de in mapper for sync_settings round-trip
affects: [24-02, 24-03, use-cases, commands, frontend-settings]

tech-stack:
  added: []
  patterns:
    [
      nullable-json-column-for-optional-domain-field,
      dedicated-update-method-avoiding-upsert-overwrite,
    ]

key-files:
  created:
    - src-tauri/crates/uc-infra/migrations/2026-03-11-000001_add_paired_device_sync_settings/up.sql
    - src-tauri/crates/uc-infra/migrations/2026-03-11-000001_add_paired_device_sync_settings/down.sql
  modified:
    - src-tauri/crates/uc-core/src/settings/model.rs
    - src-tauri/crates/uc-core/src/network/paired_device.rs
    - src-tauri/crates/uc-core/src/ports/paired_device_repository.rs
    - src-tauri/crates/uc-infra/src/db/schema.rs
    - src-tauri/crates/uc-infra/src/db/models/paired_device_row.rs
    - src-tauri/crates/uc-infra/src/db/mappers/paired_device_mapper.rs
    - src-tauri/crates/uc-infra/src/db/repositories/paired_device_repo.rs
    - src-tauri/crates/uc-tauri/src/bootstrap/runtime.rs
    - src-tauri/crates/uc-app/src/testing.rs
    - src-tauri/crates/uc-core/src/network/pairing_state_machine.rs

key-decisions:
  - 'Upsert ON CONFLICT SET excludes sync_settings to avoid overwriting per-device overrides during pairing'
  - 'serde(default) on sync_settings ensures backward-compatible deserialization for existing data'

patterns-established:
  - 'Nullable JSON column pattern: Option<T> domain field stored as Nullable<Text> with serde_json ser/de in mapper'
  - 'Dedicated update method pattern: update_sync_settings is independent of upsert to avoid accidental overwrite'

requirements-completed: [DEVSYNC-01, DEVSYNC-02, DEVSYNC-03]

duration: 4min
completed: 2026-03-11
---

# Phase 24 Plan 01: Per-device Sync Settings Data Foundation Summary

**Per-device sync_settings as nullable JSON on PairedDevice with dedicated repository update method and global fallback resolution**

## Performance

- **Duration:** 4 min
- **Started:** 2026-03-11T14:37:46Z
- **Completed:** 2026-03-11T14:42:36Z
- **Tasks:** 2
- **Files modified:** 10

## Accomplishments

- Extended PairedDevice domain model with optional SyncSettings field and serde(default) for backward compatibility
- Added resolve_sync_settings pure function returning device override or global default
- Created Diesel migration adding nullable TEXT column for JSON-serialized sync settings
- Implemented update_sync_settings in repository with proper NotFound handling
- Updated all NoopPort stubs in uc-app and uc-tauri

## Task Commits

Each task was committed atomically:

1. **Task 1: Extend domain model and port with sync_settings** - `f45a3e00` (feat)
2. **Task 2: Database migration, schema, mapper, and repository implementation** - `6a8a5694` (feat)

## Files Created/Modified

- `src-tauri/crates/uc-core/src/settings/model.rs` - Added PartialEq, Eq derives to ContentTypes and SyncSettings
- `src-tauri/crates/uc-core/src/network/paired_device.rs` - Added sync_settings field, resolve_sync_settings function, tests
- `src-tauri/crates/uc-core/src/ports/paired_device_repository.rs` - Added update_sync_settings method to trait
- `src-tauri/crates/uc-core/src/network/pairing_state_machine.rs` - Added sync_settings: None to device construction
- `src-tauri/crates/uc-infra/migrations/.../up.sql` - ALTER TABLE ADD COLUMN sync_settings
- `src-tauri/crates/uc-infra/migrations/.../down.sql` - DROP COLUMN sync_settings
- `src-tauri/crates/uc-infra/src/db/schema.rs` - Added sync_settings column definition
- `src-tauri/crates/uc-infra/src/db/models/paired_device_row.rs` - Added sync_settings field to row models
- `src-tauri/crates/uc-infra/src/db/mappers/paired_device_mapper.rs` - JSON ser/de for sync_settings
- `src-tauri/crates/uc-infra/src/db/repositories/paired_device_repo.rs` - Implemented update_sync_settings
- `src-tauri/crates/uc-tauri/src/bootstrap/runtime.rs` - NoopPort stub
- `src-tauri/crates/uc-app/src/testing.rs` - NoopPort stub

## Decisions Made

- Upsert ON CONFLICT SET intentionally excludes sync_settings to prevent overwriting device-specific overrides during pairing upserts
- Used serde(default) on sync_settings field for zero-cost backward compatibility with existing serialized PairedDevice data

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Fixed missing sync_settings in pairing_state_machine.rs**

- **Found during:** Task 1 (cargo check)
- **Issue:** PairedDevice construction in pairing_state_machine.rs missing the new sync_settings field
- **Fix:** Added `sync_settings: None` to the PairedDevice struct literal
- **Files modified:** src-tauri/crates/uc-core/src/network/pairing_state_machine.rs
- **Verification:** cargo check -p uc-core passes
- **Committed in:** f45a3e00 (Task 1 commit)

---

**Total deviations:** 1 auto-fixed (1 blocking)
**Impact on plan:** Necessary fix for compilation. No scope creep.

## Issues Encountered

None

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Data foundation complete: domain model, port, migration, mapper, and repository all wired
- Ready for Plan 02 (use cases and Tauri commands) to build on update_sync_settings
- Ready for Plan 03 (frontend UI) to consume the new commands

---

_Phase: 24-implement-per-device-sync-settings-for-paired-devices_
_Completed: 2026-03-11_
