# Phase 30: File sync UI — Dashboard file entries, context menu, progress, notifications - Context

**Gathered:** 2026-03-13
**Updated:** 2026-03-13 (split from monolithic Phase 28)
**Status:** Ready for planning
**Scope:** ~800-1,200 LoC
**Depends on:** Phase 29 (file transfer engine must be functional)

<domain>
## Phase Boundary

Add file sync visibility to the frontend: file entries in Dashboard clipboard history, right-click context menu with state-dependent actions, progress indicators for transfers, system notification merging for multi-file batches, and error feedback display. After this phase, users can see and interact with file transfers in the UI.

</domain>

<decisions>
## Implementation Decisions

### Dashboard File Entries
- File entries appear in Dashboard clipboard history showing filename, size, source device
- File items support right-click context menu
- Clickable to open file location (platform-specific: Explorer/Finder/file manager)

### Context Menu Actions (State-Dependent)
- **Not downloaded (large file):** "Sync to Clipboard" — auto-downloads file + writes file reference to clipboard on completion
- **Downloaded / local (small file):** "Copy" — writes file reference to clipboard
- While downloading: "Sync to Clipboard" action is disabled on this entry

### Large File UX Flow
- Only metadata synced initially — file entry appears in Dashboard list with "not downloaded" status
- Dashboard shows filename, size, source device in detail panel
- Simplified 1-step flow: user right-clicks file entry → "Sync to Clipboard" (auto-downloads + writes file reference to clipboard on completion)
- After download + clipboard write: user pastes with Ctrl+V / Cmd+V
- No "system busy cursor" — progress shown within Dashboard UI only

### Progress Indicators
- Dashboard (if open): show file transfer progress indicator on list item or detail panel
- Receiver (large file): progress visible during download

### System Notifications
- Sender: system notification "File xxx syncing to [device]" → "Sync complete"
- Multi-file notification merging: batch operations produce only 2 notifications ("Syncing N files to [device]" → "All N files synced") — not per-file
- Error feedback: both sender and receiver get system notification on failure with reason
- Dashboard entry shows "transfer failed" status with error detail

### Clipboard Race Handling (UI Side)
- If user performs a new copy during file transfer, auto-write to clipboard cancelled
- Files remain accessible in Dashboard for manual "Copy" action

### Claude's Discretion
- System notification implementation details per platform
- Dashboard file entry UI layout details (progress indicator style, context menu implementation)
- File entry component design within existing Dashboard architecture

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets (This Phase)
- System notifications: Via Tauri notification plugin or native APIs
- Existing Dashboard clipboard history component: extend for file entries
- `NetworkEvent::TransferProgress` already exists — can drive progress UI

### Established Patterns
- Frontend: React 18 + TypeScript + Redux Toolkit + Shadcn/ui
- State management: Redux Toolkit slices
- Tauri commands: `src-tauri/crates/uc-tauri/src/commands/`
- UI components: `src/components/` (Shadcn/ui based)

### Integration Points
- Phase 29 outputs consumed: working file transfer service, use cases, file entry data in database
- New Tauri commands for: listing file entries, triggering on-demand download, opening file location
- Redux slice for file transfer state (progress, status)
- Dashboard component extension for file entry rendering
- Right-click context menu integration

</code_context>

<deferred>
## Deferred Ideas

- Drag-and-drop file sync trigger — future UX enhancement
- Advanced file preview in Dashboard — future enhancement

</deferred>

---

*Phase: 30 (UI) — split from original monolithic Phase 28*
*Context gathered: 2026-03-13*
