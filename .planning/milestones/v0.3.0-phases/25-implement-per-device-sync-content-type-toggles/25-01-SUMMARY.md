---
phase: 25-implement-per-device-sync-content-type-toggles
plan: 01
subsystem: sync
tags: [content-type, mime, filtering, clipboard, sync-policy]

requires:
  - phase: 24-per-device-sync-settings
    provides: per-device sync settings with resolve_sync_settings and filter_by_auto_sync

provides:
  - ContentTypeCategory enum for MIME-based content classification
  - classify_snapshot() function mapping clipboard MIME types to categories
  - is_content_type_allowed() function for toggle-based content filtering
  - apply_sync_policy() method replacing filter_by_auto_sync with content type awareness
  - ContentTypes::default() returning all-true (critical fix from derive(Default) all-false)

affects: [25-02, 25-03, sync-engine, device-settings-ui]

tech-stack:
  added: []
  patterns: [single-pass sync policy filtering, MIME-based content classification]

key-files:
  created:
    - src-tauri/crates/uc-core/src/settings/content_type_filter.rs
  modified:
    - src-tauri/crates/uc-core/src/settings/model.rs
    - src-tauri/crates/uc-core/src/settings/defaults.rs
    - src-tauri/crates/uc-core/src/settings/mod.rs
    - src-tauri/crates/uc-app/src/usecases/clipboard/sync_outbound.rs

key-decisions:
  - 'ContentTypes::default() fix from derive(Default) all-false to explicit all-true impl'
  - 'Classify snapshot once before peer loop for efficiency (not per-peer)'
  - 'Only Text and Image are filterable; unimplemented types always sync'

patterns-established:
  - 'Content type classification: MIME-to-category mapping via classify_snapshot()'
  - 'Sync policy pattern: auto_sync check then content type check in single pass'

requirements-completed: [CT-01, CT-02, CT-03, CT-04]

duration: 8min
completed: 2026-03-12
---

# Phase 25 Plan 01: Backend Content Type Classification and Sync Policy Summary

**MIME-based content type classification with per-device filtering in outbound sync using classify_snapshot() and apply_sync_policy()**

## Performance

- **Duration:** 8 min
- **Started:** 2026-03-12T04:24:04Z
- **Completed:** 2026-03-12T04:32:26Z
- **Tasks:** 2
- **Files modified:** 5

## Accomplishments

- Fixed critical ContentTypes::default() bug where derive(Default) produced all-false instead of all-true
- Created content_type_filter module with MIME-to-category classification (text, image, rich_text, link, file, unknown)
- Renamed filter_by_auto_sync to apply_sync_policy with content type awareness
- Single-pass filtering: auto_sync check then content type check per-peer, snapshot classified once

## Task Commits

Each task was committed atomically:

1. **Task 1: ContentTypes default fix and content_type_filter module** - `cc4d30ab` (test) + `28ee2777` (feat)
2. **Task 2: Rename filter_by_auto_sync to apply_sync_policy** - `48187c82` (test) + `fa1e91a3` (feat)

_Note: TDD tasks have two commits each (test RED then feat GREEN)_

## Files Created/Modified

- `src-tauri/crates/uc-core/src/settings/content_type_filter.rs` - ContentTypeCategory enum, classify_snapshot(), is_content_type_allowed() with 13 unit tests
- `src-tauri/crates/uc-core/src/settings/model.rs` - Removed derive(Default) from ContentTypes
- `src-tauri/crates/uc-core/src/settings/defaults.rs` - Added explicit Default impl with all-true fields
- `src-tauri/crates/uc-core/src/settings/mod.rs` - Added content_type_filter module export
- `src-tauri/crates/uc-app/src/usecases/clipboard/sync_outbound.rs` - Renamed to apply_sync_policy, added content type filtering with 6 new policy tests

## Decisions Made

- Fixed ContentTypes::default() from derive(Default) producing all-false to explicit all-true impl -- critical for new devices syncing everything by default
- Snapshot classified once before peer loop (not per-peer) for O(1) classification cost
- Only Text and Image are filterable in this phase; RichText, Link, File, CodeSnippet, Unknown always sync

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Fixed private module import path**

- **Found during:** Task 1 (RED phase)
- **Issue:** `crate::clipboard::system::SystemClipboardSnapshot` failed because `system` module is private
- **Fix:** Changed import to `crate::clipboard::SystemClipboardSnapshot` using the re-export
- **Files modified:** content_type_filter.rs
- **Verification:** Compilation succeeds
- **Committed in:** cc4d30ab (Task 1 RED commit)

---

**Total deviations:** 1 auto-fixed (1 blocking)
**Impact on plan:** Minor import path fix, no scope change.

## Issues Encountered

None beyond the import path correction noted above.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Backend content type classification and filtering complete
- Ready for Plan 02 (frontend UI: making text/image toggles interactive, "coming soon" badges)
- apply_sync_policy now enforces both auto_sync and content type policy per-peer

---

_Phase: 25-implement-per-device-sync-content-type-toggles_
_Completed: 2026-03-12_
