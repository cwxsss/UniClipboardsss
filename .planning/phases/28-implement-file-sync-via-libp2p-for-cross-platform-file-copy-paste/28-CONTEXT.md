# Phase 28: Implement file sync via libp2p for cross-platform file copy-paste - Context

**Gathered:** 2026-03-13
**Status:** Ready for planning

<domain>
## Phase Boundary

Enable cross-platform file copy-paste between paired devices over LAN via libp2p. User copies file(s) on one device (e.g., Windows), and can paste them on another device (e.g., macOS). V1 targets flat files only (no directories), LAN-only, already-paired devices.

</domain>

<decisions>
## Implementation Decisions

### File Capture
- Clipboard monitoring via existing ClipboardWatcher — detect file copy events (CF_HDROP on Windows, NSFilenamesPboardType on macOS)
- Support multiple files in a single copy operation (multi-file Ctrl+C)
- Flat files only — no directory/folder sync in V1
- When multiple files are copied, transfer them serially in a queue (one file completes announce→data→complete before the next starts)

### Transfer Strategy
- Smart threshold: small files (below configurable threshold) transfer immediately on copy; large files sync metadata first, pull content on-demand when receiver pastes
- Threshold is configurable in Settings (default TBD by planner, e.g., 10MB)
- Sequential chunk transmission with whole-file hash verification
- Temporary file + atomic rename on disk (write to temp, rename on completion)
- Auto-retry with exponential backoff on network interruption; fail after max retries with user notification
- No断点续传 in V1 — retry from current chunk, not from byte offset

### Protocol Design
- New dedicated libp2p stream protocol: `/uniclipboard/file-transfer/1.0.0`
- Reuse `libp2p_stream` infrastructure (same pattern as PairingStreamService)
- Independent from clipboard sync channel — file transfers do not block text/image sync
- Message flow: announce → accept → data (chunked) → complete
- Already-paired devices auto-accept file transfers (no manual confirmation prompt)

### Receiver Paste Experience
- File content written to system clipboard as file references (temp directory path)
- User pastes with standard Ctrl+V / Cmd+V — behaves like a local file copy
- For on-demand pull (large files): paste operation blocks with system busy cursor until transfer completes
- Temporary files stored in app data directory under `file-cache/` subdirectory
- Auto-cleanup of expired files (e.g., older than 24h) on app startup
- Same-name conflict at paste destination: auto-rename with suffix (e.g., `file(1).txt`)

### Dashboard Integration
- File entries appear in Dashboard clipboard history showing filename, size, source device
- Clickable to re-copy to clipboard or open file location

### Progress & Feedback
- Sender: system notification "File xxx syncing to [device]" → "Sync complete"
- Receiver paste (on-demand): system busy cursor during transfer
- Dashboard (if open): show file transfer progress bar

### Security & File Types
- No file type restrictions — all file types allowed (already-paired devices have trust relationship)
- Encryption relies on libp2p Noise transport layer only — no additional XChaCha20 encryption on file content
- File size limit configurable in Settings

### Multi-device Broadcast
- Reuse existing `apply_sync_policy` logic: global auto_sync + per-device auto_sync + content type filter (ContentTypeCategory::File)
- File synced to all eligible paired devices that are online

### Offline Handling
- Consistent with current clipboard sync: only send to online devices
- Offline devices are skipped — no queuing or deferred delivery

### Settings UI
- Settings > Sync section:
  - "Enable file sync" toggle
  - "Small file immediate transfer threshold" (size input)
  - "Maximum file size limit" (size input)
  - "Temporary file retention period" (duration)
  - "Auto-cleanup" toggle

### Claude's Discretion
- Exact chunk size for file transfer (can reference existing 256KB pattern)
- Hash algorithm choice (SHA-256 vs Blake3)
- Temp file naming convention
- System notification implementation details per platform
- Dashboard file entry UI layout and interactions
- Exact retry policy parameters (max retries, backoff intervals)

</decisions>

<specifics>
## Specific Ideas

- V1 scope explicitly defined by user: LAN-only, paired devices, single-file protocol (multi-file via serial queue)
- Protocol flow matches user's specification: announce / accept / data / complete
- User wants system-level busy cursor during paste-triggered transfers — "like copying from a network drive"
- User explicitly excluded from V1: multi-stream concurrency, directory sync, deduplication, instant transfer (秒传), cross-WAN

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `PairingStreamService` (`uc-platform/src/adapters/pairing_stream/service.rs`): Full libp2p_stream pattern — new FileTransferService can follow same architecture
- `ContentTypeCategory::File` (`uc-core/src/settings/content_type_filter.rs`): Already defined, currently always-sync. Can be made filterable
- `ProtocolId` enum (`uc-core/src/network/protocol_ids.rs`): Add `FileTransfer` variant for `/uniclipboard/file-transfer/1.0.0`
- `ClipboardTransportPort` (`uc-core/src/ports/clipboard_transport.rs`): Reference for designing `FileTransportPort`
- `apply_sync_policy` (`uc-app/src/usecases/clipboard/sync_outbound.rs`): Reuse for determining which peers receive file
- V3 binary protocol with 256KB chunking: Reference for chunk-based transfer design

### Established Patterns
- Port/Adapter pattern: New `FileTransportPort` trait in `uc-core/ports/`, implementation in `uc-platform/adapters/`
- Use case pattern: `SyncOutboundFileUseCase` / `SyncInboundFileUseCase` in `uc-app/usecases/`
- UseCases accessor: New accessor methods on `UseCases` struct in `uc-tauri/bootstrap/runtime.rs`
- Settings model: Extend `SyncSettings` in `uc-core/src/settings/model.rs` with file sync fields
- System notifications: Via Tauri notification plugin or native APIs

### Integration Points
- `ClipboardWatcher` needs to detect file copy events (currently handles text/image)
- `PlatformRuntime` event loop: Add `PlatformEvent::FileCopied` variant
- `AppRuntime` callback: New handler for file copy events
- `main.rs`: Register new commands in `invoke_handler![]`
- Frontend: New file entry component in Dashboard, new settings fields in Sync section
- `wiring.rs`: Wire new file transfer service and ports

</code_context>

<deferred>
## Deferred Ideas

- Directory/folder sync — future phase
- Multi-stream concurrent file transfer — future optimization
- Cross-WAN file sync (beyond LAN) — future phase
- File deduplication / instant transfer (秒传) — future optimization
- Drag-and-drop file sync trigger — future UX enhancement
- Breakpoint resume (断点续传) at byte offset level — future reliability improvement

</deferred>

---

*Phase: 28-implement-file-sync-via-libp2p-for-cross-platform-file-copy-paste*
*Context gathered: 2026-03-13*
