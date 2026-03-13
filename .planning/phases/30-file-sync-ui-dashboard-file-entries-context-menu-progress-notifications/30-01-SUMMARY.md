---
phase: 30-file-sync-ui-dashboard-file-entries-context-menu-progress-notifications
plan: 01
subsystem: ui
tags: [react, context-menu, radix-ui, shadcn, tauri, i18n]

requires:
  - phase: 28-file-sync-foundation
    provides: ClipboardFileItem type and file classification
  - phase: 29-file-transfer-service
    provides: File transfer service and transport port

provides:
  - Right-click context menu on all clipboard item types
  - State-dependent file actions (Sync to Clipboard vs Copy)
  - downloadFileEntry and openFileLocation Tauri command stubs
  - FileContextMenu reusable component

affects: [30-02, 30-03, 31-file-sync-settings]

tech-stack:
  added: [@radix-ui/react-context-menu via shadcn]
  patterns: [context-menu-wrapper, transfer-tracking-state]

key-files:
  created:
    - src/components/ui/context-menu.tsx
    - src/components/clipboard/FileContextMenu.tsx
  modified:
    - src/components/clipboard/ClipboardContent.tsx
    - src/components/clipboard/ClipboardActionBar.tsx
    - src/components/clipboard/ClipboardPreview.tsx
    - src/api/clipboardItems.ts
    - src/i18n/locales/en-US.json
    - src/i18n/locales/zh-CN.json

key-decisions:
  - "FileContextMenu wraps children via ContextMenuTrigger asChild for zero-DOM-overhead"
  - "Transfer tracking uses Set<string> state in ClipboardContent for transferringEntries"
  - "Action bar conditionally swaps Copy for Sync to Clipboard based on file download status"

patterns-established:
  - "Context menu wrapper pattern: FileContextMenu wraps item rows with asChild trigger"
  - "Transfer state tracking: Set<string> for in-progress entry IDs"

requirements-completed: [FSYNC-UI]

duration: 4min
completed: 2026-03-13
---

# Phase 30 Plan 01: File Entries Context Menu Summary

**Right-click context menu with state-dependent file actions (Sync to Clipboard / Copy) using Shadcn Radix context-menu component**

## Performance

- **Duration:** 4 min
- **Started:** 2026-03-13T13:59:40Z
- **Completed:** 2026-03-13T14:03:31Z
- **Tasks:** 2
- **Files modified:** 8

## Accomplishments

- Installed Shadcn context-menu (Radix UI) and created FileContextMenu wrapper component
- Integrated context menu into Dashboard: right-click any clipboard item shows Copy/Delete actions
- File items show state-dependent actions: "Sync to Clipboard" when not downloaded, "Copy" when downloaded
- Action bar adapts for file items with Sync to Clipboard button and transfer spinner
- File preview enhanced with download status badge and source device display

## Task Commits

Each task was committed atomically:

1. **Task 1: Install Shadcn context-menu and create FileContextMenu component** - `b6eaccda` (feat)
2. **Task 2: Integrate context menu into ClipboardContent and update action bar** - `23386607` (feat)

## Files Created/Modified

- `src/components/ui/context-menu.tsx` - Shadcn context-menu primitives (Radix UI)
- `src/components/clipboard/FileContextMenu.tsx` - Context menu wrapper with state-dependent file actions
- `src/components/clipboard/ClipboardContent.tsx` - Wraps item rows with FileContextMenu, adds transfer tracking
- `src/components/clipboard/ClipboardActionBar.tsx` - Conditional Sync to Clipboard button for file items
- `src/components/clipboard/ClipboardPreview.tsx` - Download status badge and source device for file entries
- `src/api/clipboardItems.ts` - downloadFileEntry and openFileLocation Tauri command stubs
- `src/i18n/locales/en-US.json` - Context menu, action bar sync, preview status i18n keys
- `src/i18n/locales/zh-CN.json` - Same keys in Chinese

## Decisions Made

- FileContextMenu uses ContextMenuTrigger with asChild to avoid extra DOM wrapper elements
- Transfer tracking uses Set<string> in ClipboardContent state, updated on download start/error
- Action bar conditionally replaces Copy with Sync to Clipboard for undownloaded file items

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Context menu functional, ready for Plan 02 (transfer progress notifications)
- downloadFileEntry stub returns transfer_id for future progress event integration
- transferringEntries state ready to be updated by transfer progress events

---

_Phase: 30-file-sync-ui-dashboard-file-entries-context-menu-progress-notifications_
_Completed: 2026-03-13_
