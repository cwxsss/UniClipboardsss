# Phase 33: Fix file sync eventual consistency - Context

**Gathered:** 2026-03-15
**Status:** Ready for planning

<domain>
## Phase Boundary

Fix the eventual consistency problem in file sync where metadata arrives before the file blob, causing phantom entries in the UI and potential orphaned DB records. Introduce a transfer state machine (DB-backed) with clear status transitions, timeout handling, failure recovery, and startup cleanup — without changing the two-phase sync protocol.

</domain>

<decisions>
## Implementation Decisions

### Sync Atomicity Model

- Keep the current two-phase model: metadata (ClipboardMessage with file_transfers) sent first, blob sent separately via FileTransferService
- Receiver creates DB entry immediately on metadata arrival with status=pending (allows UI to show "receiving..." state)
- Continue using ClipboardEntry model for file entries; file_transfer table tracks transfer status
- No protocol changes needed — fix is entirely on the receiver side via state machine

### Transfer State Machine

- 4 states: pending → transferring → completed / failed
- State stored as DB field in file_transfer table (persisted, survives restart)
- State transitions:
  - pending: metadata received, waiting for blob transfer to start
  - transferring: first data chunk received, blob transfer in progress
  - completed: all chunks received, Blake3 hash verified, file ready
  - failed: transfer failed (timeout, hash mismatch, network error, or orphaned on restart)

### Receiver State Management — UI

- Dashboard shows all file entries including pending/transferring/failed states
- Use badge/icon to indicate transfer status (e.g., "接收中...", "传输失败")
- Copy action disabled for non-completed entries
- Delete available for all states (cleans up temp files too)

### Frontend Notification

- New event: file-transfer://status-changed carrying transfer_id + new_status
- Frontend Redux updates corresponding entry status on each event
- Replaces or supplements existing file-transfer://completed event

### OS Clipboard Write Timing

- No change: auto-write to OS clipboard when status transitions to completed
- Clipboard race detection preserved (Phase 32.1 decision)

### Failure Recovery

- Failed transfers: ClipboardEntry preserved in history, marked as failed with reason
- No retry support — user must re-copy file on sender to trigger new transfer
- Temp files cleaned up on failure (delete partial downloads)
- Consistent with Phase 30 decision: Blake3 hash mismatch = delete temp file, no retry

### Timeout Policy

- pending → failed: if no data chunk arrives within 60 seconds of metadata receipt
- transferring → failed: if no new chunk arrives within 5 minutes
- Timeout transitions clean up temp files and notify frontend via status-changed event

### Startup Cleanup

- On app startup: scan file_transfer table for pending/transferring entries
- Batch-mark all as failed (these are orphaned from previous session)
- Clean up any associated temp files in file-cache/

### Multi-Device Consistency

- Per-device independent processing (fire-and-forget from sender perspective)
- Sender does not track receiver success/failure (consistent with text/image sync)
- Partial success is normal: B succeeds, C fails — each device handles independently
- Concurrent reception from multiple devices: last-completed transfer overwrites clipboard (Phase 32.1 decision preserved)

### Claude's Discretion

- Exact DB migration for status field on file_transfer table
- Status badge/icon visual design in Dashboard
- Timeout implementation approach (tokio timer vs periodic check)
- Event payload structure for file-transfer://status-changed
- Startup cleanup integration point (bootstrap sequence)

</decisions>

<specifics>
## Specific Ideas

- State machine should feel invisible to the user — happy path experience unchanged (file arrives, auto-writes to clipboard)
- Failed entries should use the same visual treatment as stale files from Phase 32.1 (grey + strikethrough pattern)
- Startup cleanup should be fast and not block app launch

</specifics>

<code_context>

## Existing Code Insights

### Reusable Assets

- `file_transfer` table already exists in DB schema — add status column via migration
- Phase 32.1 stale file UI pattern (grey + strikethrough) — reuse for failed entries
- `NetworkEvent::FileTransferCompleted` event — extend or add `FileTransferStatusChanged` variant
- `wiring.rs:2226-2318` — file transfer completion event handler, integration point for state transitions
- `sync_inbound.rs:339-432` — metadata receipt flow, integration point for pending state creation

### Established Patterns

- DB migrations: manual schema.rs update (Phase 28 decision — diesel CLI not available)
- Tauri events: `app_handle.emit()` for frontend notification
- Redux slices: existing transfer state management in ClipboardContent slice

### Integration Points

- `SyncInboundFileUseCase` — add status transitions on transfer events
- `FileTransferService::handle_incoming` — emit status-changed events during transfer
- App bootstrap sequence — add startup cleanup step
- Dashboard file entry component — read and display transfer status

</code_context>

<deferred>
## Deferred Ideas

- Receiver-initiated retry (request re-send from sender) — needs bidirectional protocol extension
- Transfer progress percentage in status-changed events — could enhance UX but adds complexity
- Cross-device transfer status synchronization — not needed for LAN-first model

</deferred>

---

_Phase: 33-fix-file-sync-eventual-consistency-ensure-atomic-sync-with-metadata-and-blob-together_
_Context gathered: 2026-03-15_
