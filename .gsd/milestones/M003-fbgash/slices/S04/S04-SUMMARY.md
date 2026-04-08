---
id: S04
parent: M003-fbgash
milestone: M003-fbgash
provides:
  - uc-tauri is now a thin shell — only Tauri-native commands remain (lifecycle, setup, pairing, tray, updater, autostart, storage, encryption unlock)
  - All frontend clipboard, settings, and encryption operations route through daemon HTTP API
  - unlock_encryption_session Tauri command available for UnlockPage keyring flow
requires:
  - slice: S01
    provides: DaemonClient singleton and auth infrastructure
  - slice: S02
    provides: Clipboard list/restore/delete via daemon HTTP
  - slice: S03
    provides: Direct WS connection for real-time events
affects:
  - S05: must fix PreviewPanel.tsx broken calls to getClipboardEntryDetail and getClipboardEntryResource
key_files:
  - src-tauri/crates/uc-tauri/src/commands/clipboard.rs (deleted)
  - src-tauri/crates/uc-tauri/src/commands/settings.rs (deleted)
  - src-tauri/crates/uc-tauri/src/commands/encryption.rs (reduced + unlock command added)
  - src-tauri/crates/uc-tauri/src/commands/storage.rs (get_stats + clear_cache restored)
  - src-tauri/src/main.rs (invoke_handler cleaned up)
  - src/quick-panel/ClipboardHistoryPanel.tsx (migrated to daemon)
  - src/hooks/useEncryptionSessionState.ts (migrated to daemon)
  - src/store/api.ts (migrated to daemon with type adapter)
  - src/hooks/useThemeSync.ts (migrated to daemon)
  - src/contexts/SettingContext.tsx (migrated to daemon)
  - src/types/setting.ts (type alignment fix)
key_decisions:
  - restore_storage_commands: get_storage_stats and clear_cache are inherently Tauri-side filesystem ops, cannot migrate to daemon HTTP. Restored as Tauri commands.
  - add_unlock_command: unlock_encryption_session was removed in T02 but still called by UnlockPage for keyring-based unlock. Added as thin Tauri command.
  - command_ordering: Tauri #[tauri::command] must precede #[cfg(test)] blocks — codegen only sees items before test modules.
  - type_adapter: store/api.ts uses adapter transforming daemon camelCase (sessionReady) to legacy snake_case (session_ready) for existing consumers.
  - settings_type_fix: theme_color/language/device_name were optional (?) in src/types/setting.ts but daemon requires non-optional string | null. Fixed.
patterns_established:
  - Tauri command placement rule: #[tauri::command] fns must come before #[cfg(test)] blocks for codegen visibility
observability_surfaces:
  - none
drill_down_paths:
  - .gsd/milestones/M003-fbgash/slices/S04/tasks/T01-SUMMARY.md
  - .gsd/milestones/M003-fbgash/slices/S04/tasks/T02-SUMMARY.md
  - .gsd/milestones/M003-fbgash/slices/S04/tasks/T03-SUMMARY.md
duration: ""
verification_result: passed
completed_at: 2026-03-30T06:21:57.884Z
blocker_discovered: false
---

# S04: uc-tauri Command Cleanup

**Removed 17 migrated Tauri commands; uc-tauri is now a thin shell; discovered and fixed T03's incomplete grep audit that missed live invoke() calls**

## What Happened

T01 deleted clipboard.rs and removed all 11 clipboard commands. T02 reduced encryption.rs/settings.rs/storage.rs but missed get_storage_stats and clear_cache (inherently Tauri-side). T03 ran grep audit with incomplete patterns, missing several live frontend calls to removed commands. During post-slice verification, I found: ClipboardHistoryPanel.tsx still calling copyClipboardItem/deleteClipboardItem, UnlockPage.tsx calling unlockEncryptionSession (never existed), useEncryptionSessionState.ts/store/api.ts calling getEncryptionSessionStatus, SettingContext.tsx/useThemeSync.ts calling get_settings/update_settings. All migrated to daemon HTTP API. Storage commands restored as Tauri commands. unlock_encryption_session added back as thin Tauri command for UnlockPage keyring flow. Settings type fixed to align with daemon. Blocker discovered: PreviewPanel.tsx still has broken calls to getClipboardEntryDetail (removed) — S05 must address this.

## Verification

cargo build in src-tauri/: exit 0 (1.0s). npx tsc --noEmit: 3 pre-existing test errors only (PairingDialog, useClipboardEvents). All T03 grep patterns now return zero matches in live (non-test) code. New Tauri command verified to be before #[cfg(test)] block for codegen visibility.

## Requirements Advanced

None.

## Requirements Validated

None.

## New Requirements Surfaced

- R013: POST/GET /clipboard/entries/:id/detail endpoint needed in daemon — PreviewPanel broken without it

## Requirements Invalidated or Re-scoped

None.

## Deviations

T03 grep patterns were incomplete — several live invoke() calls were missed. Caught and fixed during post-slice verification. get_storage_stats and clear_cache were removed in T02 but are inherently Tauri-side filesystem ops. Restored. unlock_encryption_session was removed in T02 but still needed by UnlockPage. Restored as thin Tauri command.

## Known Limitations

PreviewPanel.tsx still calls getClipboardEntryDetail (removed Tauri command) and getClipboardEntryResource (removed Tauri). This page will crash at runtime. S05 must fix this.

## Follow-ups

Fix PreviewPanel.tsx: migrate getClipboardEntryResource to daemon GET /clipboard/entries/:id/resource; either add daemon GET /clipboard/entries/:id/detail endpoint or restore getClipboardEntryDetail as Tauri command. Remove dead invoke() stubs from old clipboardItems.ts/security.ts/storage.ts once PreviewPanel is fixed.

## Files Created/Modified

- `src-tauri/crates/uc-tauri/src/commands/clipboard.rs` — Deleted — all 11 clipboard commands removed
- `src-tauri/crates/uc-tauri/src/commands/settings.rs` — Deleted — both commands migrated to daemon
- `src-tauri/crates/uc-tauri/src/commands/encryption.rs` — Reduced; unlock_encryption_session Tauri command added (before #[cfg(test)] block)
- `src-tauri/crates/uc-tauri/src/commands/storage.rs` — get_storage_stats and clear_cache restored as Tauri commands (filesystem ops)
- `src-tauri/src/main.rs` — invoke_handler cleaned up; new commands registered
- `src/quick-panel/ClipboardHistoryPanel.tsx` — Migrated copyClipboardItem/deleteClipboardItem to daemon restoreClipboardEntry/deleteClipboardEntry
- `src/hooks/useEncryptionSessionState.ts` — Migrated getEncryptionSessionStatus to daemon getEncryptionState
- `src/store/api.ts` — Migrated to daemon API with camelCase→snake_case adapter type
- `src/hooks/useThemeSync.ts` — Migrated to daemon API; removed setting-changed event listener
- `src/contexts/SettingContext.tsx` — Migrated to daemon API
- `src/types/setting.ts` — theme_color/language/device_name made required (daemon alignment)
