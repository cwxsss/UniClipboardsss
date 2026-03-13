# Phase 30: File sync UI — Research

**Researched:** 2026-03-13
**Status:** RESEARCH COMPLETE

## 1. Existing Dashboard Architecture

### Layout Structure
The Dashboard uses a two-panel resizable layout (`ResizablePanelGroup`):
- **Left panel (40%):** Scrollable item list grouped by date (Today/Yesterday/Earlier), rendered via `ClipboardItemRow`
- **Right panel (60%):** Detail preview (`ClipboardPreview`) + action bar (`ClipboardActionBar`)

### Data Flow
```
useClipboardEvents hook
  → listens to "clipboard://event" (Tauri event channel)
  → dispatches fetchClipboardItems / prependItem / removeItem to Redux
  → clipboardSlice stores ClipboardItemResponse[]
  → ClipboardContent converts to DisplayClipboardItem[] for rendering
```

### Key Types
- `ClipboardItemResponse`: Redux store item (`id`, `is_downloaded`, `is_favorited`, `item: ClipboardItem`, timestamps)
- `ClipboardItem`: Union of `text`, `image`, `file`, `link`, `code`, `unknown` nullable fields
- `ClipboardFileItem`: `{ file_names: string[], file_sizes: number[] }`
- `DisplayClipboardItem`: UI display type with `type`, `time`, `content`, `isDownloaded`, `isFavorited`

### Existing File Support (Partial)
- `ClipboardFileItem` interface already exists with `file_names` and `file_sizes`
- `ClipboardItem` has a `file` field (currently always `null` from `transformProjectionToResponse`)
- `ClipboardItemRow` has a `File` icon in `typeIcons` map
- `ClipboardItem.tsx` has a `case 'file'` renderer showing file names with File icons
- `ClipboardPreview.tsx` has a `case 'file'` renderer showing file names with sizes
- `Filter.File` enum value already exists
- `getDisplayType()` already handles `item.file` correctly

**Gap:** `transformProjectionToResponse` hardcodes `file: null` — needs backend to return file data via `ClipboardEntryProjection`.

## 2. Context Menu Implementation

### Current State
- No context menu component exists (`src/components/ui/` has `dropdown-menu.tsx` but no `context-menu.tsx`)
- Actions are currently in `ClipboardActionBar` (bottom bar with Copy/Delete buttons)
- Keyboard shortcuts exist: `C` for copy, `D` for delete

### Shadcn Context Menu
The project uses Shadcn/ui. A `context-menu` component is available from the Shadcn registry (built on Radix UI `@radix-ui/react-context-menu`). It needs to be installed:
```bash
npx shadcn@latest add context-menu
```

This will create `src/components/ui/context-menu.tsx` with `ContextMenu`, `ContextMenuTrigger`, `ContextMenuContent`, `ContextMenuItem`, etc.

### Integration Points
- Wrap `ClipboardItemRow` with `ContextMenuTrigger`
- Context menu items vary by item state (file download status, item type)
- For file items: "Sync to Clipboard" (not downloaded) or "Copy" (downloaded)
- Common actions: Copy, Delete, (future: Favorite)

## 3. Transfer Progress Infrastructure

### Backend (Already Exists)
- `TransferProgress` struct in `uc-core/src/ports/transfer_progress.rs`:
  - `transfer_id`, `peer_id`, `direction` (Sending/Receiving), `chunks_completed`, `total_chunks`, `bytes_transferred`, `total_bytes`
- `TransferProgressPort` trait with `report_progress()` method
- `TransferProgressEvent` DTO in `uc-tauri/src/events/transfer_progress.rs` (camelCase serialization)
- Frontend event channel: `"transfer://progress"` via `app.emit()`
- `forward_transfer_progress_event()` function already wired

### Frontend (Needs Implementation)
- Need a `useTransferProgress` hook listening to `"transfer://progress"` Tauri events
- Redux slice or local state to track active transfers by `transfer_id`
- Progress display in `ClipboardItemRow` and/or `ClipboardPreview` for active transfers
- Shadcn `Progress` component already installed (`src/components/ui/progress.tsx`)

### Progress Display Strategy
- In the item list (left panel): Show a small progress bar or percentage on the `ClipboardItemRow` for items being transferred
- In the preview panel (right panel): Show detailed progress with bytes transferred / total bytes
- Link `transfer_id` to clipboard entry ID to map progress to the correct list item

## 4. System Notifications

### Tauri Notification Plugin
Not yet installed. Need `tauri-plugin-notification`:
```bash
# Rust side
cargo add tauri-plugin-notification
# JS side (optional, for frontend-triggered notifications)
bun add @tauri-apps/plugin-notification
```

Configuration in `tauri.conf.json`:
```json
{
  "plugins": {
    "notification": {
      "enabled": true
    }
  }
}
```

### Notification Strategy
- **Backend-driven notifications** (preferred for sync events): Emit via Rust `tauri::notification::Notification::new()`
- **Frontend-driven notifications** (fallback): Use `@tauri-apps/plugin-notification` JS API
- **Multi-file batching**: Accumulate file events within a time window (e.g., 500ms), then emit a single "Syncing N files to [device]" notification
- **Completion**: "All N files synced" or "Sync complete" notification
- **Error**: "File sync failed: [reason]" notification

### Notification Merging Pattern
```
Timer-based batching:
  file_sync_start event → start 500ms timer, collect entries
  more events within 500ms → add to batch
  timer fires → "Syncing N files to Device-X"
  all complete → "All N files synced to Device-X"
  any error → "Failed to sync [filename]: [reason]"
```

## 5. State-Dependent Context Menu Actions

### File Entry States
Based on CONTEXT.md decisions:

| State | `is_downloaded` | Context Menu | Action |
|-------|-----------------|--------------|--------|
| Not downloaded (large file, metadata only) | `false` | "Sync to Clipboard" | Download file → write file ref to clipboard |
| Downloaded / local (small file) | `true` | "Copy" | Write file ref to clipboard |
| Currently downloading | (transfer in progress) | "Sync to Clipboard" (disabled) | No action |

### Clipboard Race Handling
- Track active transfers by entry ID
- If user copies something else during transfer, cancel the auto-write-to-clipboard
- Files remain in Dashboard for manual "Copy" after transfer completes

## 6. New Tauri Commands Needed

Based on CONTEXT.md integration points:

1. **`download_file_entry`** — Trigger on-demand download of a file entry (for "Sync to Clipboard" action)
   - Input: `entry_id: String`
   - Output: Result with transfer_id for progress tracking
   - Phase 29 should provide the use case; this phase wires the command

2. **`open_file_location`** — Open file in platform file manager (Explorer/Finder)
   - Input: `entry_id: String`
   - Uses: `tauri::api::shell::open()` or `opener` crate
   - Platform-specific: Opens containing folder with file selected

3. **Existing commands** that may need modification:
   - `get_clipboard_entries` — Must return file entries with proper `content_type` and file metadata
   - `restore_clipboard_entry` — Must handle file type (write file reference to clipboard)

## 7. Redux State for File Transfers

### New Slice: `fileTransferSlice`
```typescript
interface FileTransferState {
  activeTransfers: Record<string, TransferProgressInfo>
  // Map entry_id → transfer_id for linking UI items to transfers
  entryTransferMap: Record<string, string>
}

interface TransferProgressInfo {
  transferId: string
  entryId: string
  peerId: string
  direction: 'Sending' | 'Receiving'
  chunksCompleted: number
  totalChunks: number
  bytesTransferred: number
  totalBytes: number | null
  status: 'active' | 'completed' | 'failed'
}
```

### Hook: `useTransferProgress`
- Listens to `"transfer://progress"` Tauri events
- Updates `fileTransferSlice` state
- Components read from Redux to display progress

## 8. Validation Architecture

### Testable Boundaries
1. **Context menu rendering**: Given item type + download status → correct menu items shown
2. **Progress display**: Given transfer progress event → progress bar updated correctly
3. **Notification merging**: Given multiple file events → single batched notification
4. **State transitions**: file entry state machine (not downloaded → downloading → downloaded)

### Integration Points to Verify
1. `"transfer://progress"` event → Redux state update → UI progress display
2. Context menu "Sync to Clipboard" → `download_file_entry` command → transfer starts → progress events flow
3. Transfer complete → notification + clipboard write + entry state update
4. Error during transfer → error notification + entry shows failed state

### Test Strategy
- Unit tests for context menu item logic (pure function: state → menu items)
- Unit tests for notification batching logic
- Component tests for `ClipboardItemRow` with file type and progress
- Integration: Tauri event → Redux → UI update cycle

---

*Phase: 30-file-sync-ui-dashboard-file-entries-context-menu-progress-notifications*
*Research completed: 2026-03-13*
