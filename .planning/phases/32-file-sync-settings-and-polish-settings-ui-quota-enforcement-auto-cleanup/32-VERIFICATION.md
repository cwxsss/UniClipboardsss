---
phase: 32-file-sync-settings-and-polish-settings-ui-quota-enforcement-auto-cleanup
verified: 2026-03-14T15:00:00Z
status: gaps_found
score: 18/19 must-haves verified
re_verification: false
gaps:
  - truth: 'FSYNC-POLISH requirement ID exists and is tracked in REQUIREMENTS.md'
    status: failed
    reason: 'All three plans declare requirements: [FSYNC-POLISH] in their frontmatter, but FSYNC-POLISH is not defined anywhere in .planning/REQUIREMENTS.md. The ID is orphaned — claimed by plans but absent from the requirements registry.'
    artifacts:
      - path: '.planning/REQUIREMENTS.md'
        issue: 'No FSYNC-POLISH entry exists in the file'
    missing:
      - 'Add FSYNC-POLISH requirement definition to .planning/REQUIREMENTS.md and add a traceability row pointing to Phase 32'
human_verification:
  - test: 'Open Settings > Sync tab in the running app'
    expected: "A 'File Sync' settings group appears below the existing sync controls with 6 controls: Enable toggle, Immediate transfer threshold, Maximum file size, Per-device cache quota, File retention period, Auto-cleanup toggle"
    why_human: 'Visual layout and scroll position cannot be verified programmatically'
  - test: "Toggle 'Enable file sync' off"
    expected: 'All 5 child controls (threshold input, max size input, cache quota input, retention input, auto-cleanup toggle) become disabled/greyed out immediately. The Enable toggle itself remains interactive.'
    why_human: 'Cascade-disable behavior requires UI interaction to confirm'
  - test: "Enter an invalid value in the Immediate transfer threshold field (e.g., 'abc' or 2000)"
    expected: 'An inline red error message appears below the input. No settings update is sent.'
    why_human: 'Inline validation feedback requires UI observation'
  - test: 'Enter a threshold value equal to or greater than the max file size value'
    expected: "The 'Must be less than max file size' cross-field validation error appears"
    why_human: 'Cross-field validation state requires interactive testing'
---

# Phase 32: File Sync Settings UI, Auto-Cleanup, and Guards Verification Report

**Phase Goal:** Add file sync settings UI (enable toggle, thresholds, quotas), enforce per-device file cache quotas, implement auto-cleanup of expired temp files, and polish error handling across the file sync pipeline.
**Verified:** 2026-03-14T15:00:00Z
**Status:** gaps_found
**Re-verification:** No — initial verification

---

## Goal Achievement

### Observable Truths

| #   | Truth                                                                            | Status   | Evidence                                                                                                                                                            |
| --- | -------------------------------------------------------------------------------- | -------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | File sync settings group appears within SyncSection below existing sync controls | ? HUMAN  | SyncSection.tsx has the group at line 315 with `mt-6` wrapper div; visual confirmation needed                                                                       |
| 2   | Enable file sync toggle controls file_sync_enabled field                         | VERIFIED | `handleFileSyncEnabledChange` calls `updateFileSyncSetting({ file_sync_enabled: checked })` at SyncSection.tsx:132                                                  |
| 3   | Small file threshold input validates range and persists                          | VERIFIED | Handler at SyncSection.tsx:135 validates 1–1000, cross-validates against maxFileSizeLimit, calls `updateFileSyncSetting({ small_file_threshold: mbToBytes(size) })` |
| 4   | Max file size input validates range and persists                                 | VERIFIED | Handler at SyncSection.tsx:173 validates 1–10240, calls `updateFileSyncSetting({ max_file_size: mbToBytes(size) })`                                                 |
| 5   | Per-device cache quota input validates range and persists                        | VERIFIED | Handler at SyncSection.tsx:200 validates 50–10240, calls `updateFileSyncSetting({ file_cache_quota_per_device: mbToBytes(size) })`                                  |
| 6   | File retention period input validates range and persists                         | VERIFIED | Handler at SyncSection.tsx:227 validates 1–720, calls `updateFileSyncSetting({ file_retention_hours: hours })`                                                      |
| 7   | Auto-cleanup toggle controls file_auto_cleanup field                             | VERIFIED | `handleFileAutoCleanupChange` calls `updateFileSyncSetting({ file_auto_cleanup: checked })` at SyncSection.tsx:255                                                  |
| 8   | All file sync inputs cascade-disabled when file_sync_enabled is false            | VERIFIED | All 4 inputs have `disabled={!fileSyncEnabled}`; auto-cleanup Switch has `disabled={!fileSyncEnabled}`                                                              |
| 9   | CleanupExpiredFilesUseCase queries expired files and removes them from disk      | VERIFIED | cleanup.rs:40 walks cache dir tree, removes files older than retention_hours; logs files_removed + bytes_reclaimed                                                  |
| 10  | Cleanup runs as non-blocking background task on startup                          | VERIFIED | wiring.rs:1545 spawns `"file_cache_cleanup"` via TaskRegistry with fire-and-forget pattern; errors logged as warn                                                   |
| 11  | Cleanup uses file_retention_hours and file_auto_cleanup from settings            | VERIFIED | cleanup.rs:45-50 reads `settings.file_sync.file_auto_cleanup` and `settings.file_sync.file_retention_hours`                                                         |
| 12  | Cleanup logs summary: count of files removed and space reclaimed                 | VERIFIED | cleanup.rs:103-107 logs `files_removed`, `bytes_reclaimed_mb`, `errors` fields                                                                                      |
| 13  | Quota enforcement function calculates cache usage per source device              | VERIFIED | `check_device_quota` in cleanup.rs:213 reads per-device dir size using `dir_size()` and compares to quota bytes                                                     |
| 14  | SyncOutboundFileUseCase checks file_sync_enabled before sending                  | VERIFIED | sync_outbound.rs:54-67: first guard checks `settings.file_sync.file_sync_enabled`, returns empty SyncOutboundResult if false                                        |
| 15  | SyncOutboundFileUseCase checks max_file_size_limit before sending                | VERIFIED | sync_outbound.rs:103-122: guard checks `file_size > settings.file_sync.max_file_size`, bails with descriptive message                                               |
| 16  | SyncInboundFileUseCase calls check_device_quota before accepting                 | VERIFIED | sync_inbound.rs:74-86: `check_quota_for_transfer` delegates to `check_device_quota` from cleanup module                                                             |
| 17  | SyncInboundFileUseCase checks file_sync_enabled before accepting                 | VERIFIED | sync_inbound.rs:156-171: `handle_transfer_complete` guards on `settings.file_sync.file_sync_enabled`, cleans up temp file on disable                                |
| 18  | Error messages are standardized across the file transfer pipeline                | VERIFIED | `transfer_errors` module in sync_inbound.rs:12-33 defines FILE_SYNC_DISABLED constant + quota_exceeded/file_exceeds_max_size/transfer_failed formatters             |
| 19  | FSYNC-POLISH requirement ID is registered in REQUIREMENTS.md                     | FAILED   | All three plans claim `requirements: [FSYNC-POLISH]` but FSYNC-POLISH does not exist in .planning/REQUIREMENTS.md                                                   |

**Score: 18/19 truths verified** (1 human-confirmation pending, 1 gap found)

---

### Required Artifacts

| Artifact                                                          | Expected                                                           | Status   | Details                                                                                                                                |
| ----------------------------------------------------------------- | ------------------------------------------------------------------ | -------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| `src/components/setting/SyncSection.tsx`                          | File sync settings SettingGroup with 6 controls                    | VERIFIED | Contains file_sync_enabled state, handlers, and SettingGroup JSX with 6 rows (lines 315-434)                                           |
| `src/types/setting.ts`                                            | FileSyncSettings interface + updateFileSyncSetting in context type | VERIFIED | FileSyncSettings at line 113 with all 6 fields; SettingContextType includes updateFileSyncSetting at line 192                          |
| `src/i18n/locales/en-US.json`                                     | English labels for file sync settings                              | VERIFIED | `fileSync` key present under settings.sections.sync with all 6 control labels and error messages                                       |
| `src/i18n/locales/zh-CN.json`                                     | Chinese labels for file sync settings                              | VERIFIED | `fileSync` key present with Chinese translations                                                                                       |
| `src/contexts/SettingContext.tsx`                                 | updateFileSyncSetting implementation                               | VERIFIED | Implemented at line 113; merges partial update with defaults, calls saveSetting                                                        |
| `src-tauri/crates/uc-app/src/usecases/file_sync/cleanup.rs`       | CleanupExpiredFilesUseCase with execute method                     | VERIFIED | Full implementation with 5 unit tests; CleanupResult struct; check_device_quota and QuotaExceededError                                 |
| `src-tauri/crates/uc-app/src/usecases/file_sync/mod.rs`           | cleanup module re-exported                                         | VERIFIED | pub use cleanup::{check_device_quota, CleanupExpiredFilesUseCase, CleanupResult, QuotaExceededError}; transfer_errors also re-exported |
| `src-tauri/crates/uc-tauri/src/bootstrap/wiring.rs`               | Cleanup task in start_background_tasks                             | VERIFIED | Lines 1540-1566: spawns "file_cache_cleanup" task, logs result or warns on error                                                       |
| `src-tauri/crates/uc-tauri/src/bootstrap/runtime.rs`              | cleanup_expired_files accessor on UseCases                         | VERIFIED | Lines 1006-1011: returns CleanupExpiredFilesUseCase wired with deps.settings and storage_paths.file_cache_dir                          |
| `src-tauri/crates/uc-app/src/usecases/file_sync/sync_outbound.rs` | file_sync_enabled and max_file_size guards                         | VERIFIED | Guards at lines 54-67 (file_sync_enabled) and 103-122 (max_file_size); uses bail! for rejection                                        |
| `src-tauri/crates/uc-app/src/usecases/file_sync/sync_inbound.rs`  | Quota check, file_sync_enabled guard, transfer_errors module       | VERIFIED | is_file_sync_enabled, check_quota_for_transfer, transfer_errors module, cleanup_temp_file helper all present                           |

---

### Key Link Verification

| From             | To                                                               | Via                                        | Status   | Details                                                                                                                           |
| ---------------- | ---------------------------------------------------------------- | ------------------------------------------ | -------- | --------------------------------------------------------------------------------------------------------------------------------- |
| SyncSection.tsx  | SettingContext (updateFileSyncSetting)                           | useSetting hook                            | VERIFIED | SyncSection.tsx:27 imports `updateFileSyncSetting` from `useSetting()`; all handlers call it with typed partial updates           |
| SyncSection.tsx  | src/types/setting.ts (FileSyncSettings)                          | Local state typed against file_sync fields | VERIFIED | `setting?.file_sync?.file_sync_enabled` used in state init at line 40; byte-to-MB conversion explicit                             |
| wiring.rs        | file_sync::CleanupExpiredFilesUseCase                            | Startup background task                    | VERIFIED | wiring.rs:1546 constructs `uc_app::usecases::file_sync::CleanupExpiredFilesUseCase::new(cleanup_settings, cleanup_cache_dir)`     |
| cleanup.rs       | uc-core/settings/model (file_auto_cleanup, file_retention_hours) | SettingsPort::load()                       | VERIFIED | cleanup.rs:43-50 reads both fields via `settings.file_sync.file_auto_cleanup` and `settings.file_sync.file_retention_hours`       |
| sync_inbound.rs  | cleanup.rs (check_device_quota)                                  | Direct module import                       | VERIFIED | sync_inbound.rs:9: `use super::cleanup::{check_device_quota, QuotaExceededError};`; check_quota_for_transfer delegates at line 79 |
| sync_outbound.rs | uc-core/settings/model (file_sync_enabled, max_file_size)        | SettingsPort::load()                       | VERIFIED | sync_outbound.rs:55-67 and 103-122 read `settings.file_sync.file_sync_enabled` and `settings.file_sync.max_file_size`             |

---

### Requirements Coverage

| Requirement  | Source Plans                       | Description                                                          | Status   | Evidence                                                                                                                                                                                          |
| ------------ | ---------------------------------- | -------------------------------------------------------------------- | -------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| FSYNC-POLISH | 32-01-PLAN, 32-02-PLAN, 32-03-PLAN | File sync settings UI, quota enforcement, auto-cleanup, error polish | ORPHANED | ID claimed by all three plans but **not defined in REQUIREMENTS.md**. No traceability row exists. The implementation is complete — the ID registration is missing from the requirements registry. |

---

### Anti-Patterns Found

No blocker anti-patterns found across the modified files.

| File                    | Pattern                                                                      | Severity | Impact                                                                   |
| ----------------------- | ---------------------------------------------------------------------------- | -------- | ------------------------------------------------------------------------ |
| sync_inbound.rs:111-120 | `Ok(true)` returned on non-Unix for `check_disk_space` (optimistic fallback) | INFO     | Windows users will not get disk space checks; documented in code comment |

---

### Human Verification Required

#### 1. File Sync Settings Group Visual Layout

**Test:** Open the app, go to Settings, click the Sync category.
**Expected:** Two distinct setting groups — the first contains existing Auto Sync / Sync Frequency / Max File Size controls; a second group titled "File Sync" appears below with 6 controls.
**Why human:** Visual rendering and scroll position cannot be asserted by code analysis.

#### 2. Cascade-Disable Behavior

**Test:** Toggle "Enable file sync" to Off.
**Expected:** The 4 numeric inputs and the Auto-cleanup toggle immediately become visually disabled (greyed out, no longer interactive). Only the "Enable file sync" switch itself remains active.
**Why human:** CSS disabled state and interactive behavior must be observed.

#### 3. Inline Validation Error Display

**Test:** Type "abc" in the Immediate transfer threshold field.
**Expected:** An inline red error message "Must be a positive number" appears below the input. The value is not persisted (no settings call made).
**Why human:** Error rendering and absence of API call require runtime observation.

#### 4. Cross-Field Validation

**Test:** Set Max file size to 100 MB, then set Immediate transfer threshold to 150 MB.
**Expected:** The threshold field shows "Must be less than max file size" error.
**Why human:** Cross-field state dependency requires interactive testing.

---

### Gaps Summary

**One gap found:** The requirement ID `FSYNC-POLISH` is declared in the frontmatter of all three plan files (`requirements: [FSYNC-POLISH]`) but does not exist in `.planning/REQUIREMENTS.md`. There is no definition, description, or traceability row for this ID anywhere in the requirements registry.

The implementation itself is complete and substantive — all UI controls, backend use cases, guards, quota enforcement, and auto-cleanup are properly implemented and wired. The gap is purely in requirements traceability: the work delivered in this phase is not traceable to any registered requirement.

**Fix:** Add FSYNC-POLISH to `.planning/REQUIREMENTS.md` with a description covering: file sync settings UI controls, per-device quota enforcement, auto-cleanup of expired files, and error handling polish across the file transfer pipeline. Add a traceability row to the table pointing to Phase 32 with status Complete.

---

_Verified: 2026-03-14T15:00:00Z_
_Verifier: Claude (gsd-verifier)_
