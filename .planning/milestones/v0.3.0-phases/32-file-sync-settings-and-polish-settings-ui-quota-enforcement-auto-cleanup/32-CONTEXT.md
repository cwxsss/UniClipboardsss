# Phase 32: File sync settings and polish — settings UI, quota enforcement, auto-cleanup - Context

**Gathered:** 2026-03-13
**Updated:** 2026-03-13 (split from monolithic Phase 28)
**Status:** Ready for planning
**Scope:** ~600-800 LoC
**Depends on:** Phase 31 (UI must be in place for settings integration)

<domain>
## Phase Boundary

Add file sync settings UI toggles, enforce per-device file cache quotas at runtime, implement auto-cleanup of expired temporary files, and polish error handling across the entire file sync pipeline. This is the final polish phase that makes the file sync feature production-ready.

</domain>

<decisions>
## Implementation Decisions

### Settings UI

- Settings > Sync section additions:
  - "Enable file sync" toggle (controls `file_sync_enabled`)
  - "Small file immediate transfer threshold" — size input, default 10MB
  - "Maximum file size limit" — size input, default 5GB
  - "Per-device file cache quota" — size input, default 500MB
  - "Temporary file retention period" — duration input, default 24h
  - "Auto-cleanup" toggle

### Quota Enforcement

- Per-device file-cache quota: default 500MB per device, configurable in Settings
- Receiver rejects transfer when quota exceeded with notification to both sender and receiver
- Quota check happens before accepting file transfer (in SyncInboundFileUseCase from Phase 30, but enforcement logic wired here)

### Auto-Cleanup

- Auto-cleanup of expired files (older than configured retention period) on app startup
- Cleanup runs as background task, does not block app startup
- Removes temp files from `file-cache/` subdirectory
- Updates database entries to `Expired` status

### File Cleanup & Conflicts

- Same-name conflict at paste destination: OS handles this (not app responsibility)
- Auto-rename with suffix (e.g., `file(1).txt`) only for files within `file-cache/` if receiving duplicate filenames from different transfers

### Error Handling Polish

- Ensure all error paths have proper user-facing feedback (system notification + Dashboard status)
- Review and standardize error messages across file transfer pipeline
- Ensure temp files are cleaned up on all failure paths

### Security Enforcement

- File size limit enforcement at transfer initiation (reject files exceeding max_file_size)
- Ensure `file_sync_enabled` toggle actually gates the entire file sync pipeline

### Claude's Discretion

- Settings UI layout within existing Settings page architecture
- Size input component design (KB/MB/GB selector or free-form)
- Cleanup scheduling details (on startup only, or periodic)

</decisions>

<code_context>

## Existing Code Insights

### Reusable Assets (This Phase)

- Settings model fields (from Phase 28): `file_sync_enabled`, `small_file_threshold`, etc.
- Existing Settings page: `src/pages/Settings/` with Sync section
- Settings commands: `get_settings` / `update_settings` Tauri commands

### Established Patterns

- Settings UI: follows existing pattern in Settings > Sync section (toggle + input fields)
- Settings persistence: TOML-based via `SettingsPort`

### Integration Points

- Phase 28 settings model fields consumed and wired to UI
- Phase 30 use cases: add quota check calls and file_sync_enabled guard
- Phase 31 UI: settings changes affect Dashboard file entry behavior
- App startup hook: register auto-cleanup task

</code_context>

<deferred>
## Deferred Ideas

- Cross-WAN file sync (beyond LAN) — future phase
- File deduplication / instant transfer (秒传) — future optimization
- Advanced quota management UI (per-device breakdown, usage visualization) — future enhancement

</deferred>

---

_Phase: 31 (Settings & Polish) — split from original monolithic Phase 28_
_Context gathered: 2026-03-13_
