# Phase 28: Implement file sync via libp2p for cross-platform file copy-paste - Context

**Gathered:** 2026-03-13
**Updated:** 2026-03-13 (supplemented from multi-dimensional review)
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
- Smart threshold: small files (below configurable threshold) transfer immediately on copy; large files sync metadata first, pull content on-demand
- Threshold is configurable in Settings (default TBD by planner, e.g., 10MB)
- Sequential chunk transmission with whole-file hash verification
- Temporary file + atomic rename on disk (write to `.tmp` suffix, verify hash, then `fs::rename()` on completion)
- Auto-retry with exponential backoff on network interruption; fail after max retries with user notification
- No断点续传 in V1 — retry from current chunk, not from byte offset
- Hash verification failure: directly fail the transfer (no retry), delete temp file, notify user

### Protocol Design
- New dedicated libp2p stream protocol: `/uniclipboard/file-transfer/1.0.0`
- Reuse `libp2p_stream` infrastructure (same pattern as PairingStreamService)
- Independent from clipboard sync channel — file transfers do not block text/image sync
- Message flow: announce → accept → data (chunked) → complete
- Additional message types: `cancel` (explicit cancellation) and `error` (report failures with error code)
- Already-paired devices auto-accept file transfers (no manual confirmation prompt)
- Sender disconnect during transfer: receiver waits for timeout, deletes temp file, marks entry as "transfer failed"

### Receiver Experience (Small Files)
- Small files auto-pulled in background upon receiving metadata
- After pull completes, file reference automatically written to system clipboard
- User can paste immediately with Ctrl+V / Cmd+V — seamless like text/image sync
- Temporary files stored in app data directory under `file-cache/` subdirectory

### Receiver Experience (Large Files)
- Only metadata synced initially — file entry appears in Dashboard list with "not downloaded" status
- Dashboard shows filename, size, source device in detail panel
- User right-clicks file entry → context menu with "Download" action (V1 only action)
- Download triggers pull from sender, progress visible on list item or detail panel
- While downloading: paste/copy actions are disabled on this entry
- After download completes: status changes to "downloaded", user right-clicks → "Copy" to write file reference to clipboard, then Ctrl+V to paste
- No "system busy cursor" — progress shown within Dashboard UI only

### File Cleanup & Conflicts
- Auto-cleanup of expired files (e.g., older than 24h) on app startup
- Same-name conflict at paste destination: auto-rename with suffix (e.g., `file(1).txt`)

### Dashboard Integration
- File entries appear in Dashboard clipboard history showing filename, size, source device
- File items support right-click context menu
- Context menu actions vary by state:
  - Not downloaded (large file): "Download"
  - Downloaded / local (small file): "Copy" (writes file reference to clipboard)
- Clickable to open file location (platform-specific: Explorer/Finder/file manager)

### Progress & Feedback
- Sender: system notification "File xxx syncing to [device]" → "Sync complete"
- Receiver (small file): seamless — auto-pulled and written to clipboard
- Receiver (large file): progress shown in Dashboard list item or detail panel
- Dashboard (if open): show file transfer progress indicator

### Security & File Safety
- No file type restrictions — all file types allowed (already-paired devices have trust relationship)
- Encryption relies on libp2p Noise transport layer only — no additional XChaCha20 encryption on file content
- File size limit configurable in Settings
- Reject symlinks: sender detects symlinks via `symlink_metadata()` and skips them
- Path traversal protection: only transmit basename (not full path), receiver validates filename has no `..` components
- Disk space pre-check: receiver checks available space before accepting file transfer, rejects with notification if insufficient
- Rate limiting: limit file announce frequency per peer (prevent flooding/resource exhaustion)

### Queue & Interruption Behavior
- New file copy during transfer: new file(s) appended to queue tail, current transfer continues uninterrupted
- Text/image copy during file transfer: parallel and independent — file transfer on dedicated protocol, clipboard sync on separate channel, no interference
- Source file deleted after copy but before transfer: sender checks file existence before transfer, fails with user notification if missing, skips to next file in queue
- No paired devices online when copying file: silent ignore, consistent with current text/image sync behavior

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
- Dashboard file entry UI layout details (progress indicator style, context menu implementation)
- Exact retry policy parameters (max retries, backoff intervals)
- Rate limit parameters (announces per peer per second)
- Unicode filename normalization strategy (NFC recommended for cross-platform)

</decisions>

<specifics>
## Specific Ideas

- V1 scope explicitly defined by user: LAN-only, paired devices, single-file protocol (multi-file via serial queue)
- Protocol flow matches user's specification: announce / accept / data / complete, supplemented with cancel / error message types
- Small files behave like text/image sync — seamless, auto-pulled, auto-written to clipboard
- Large files require explicit user action: right-click → Download → then right-click → Copy → then Ctrl+V
- User explicitly excluded from V1: multi-stream concurrency, directory sync, deduplication, instant transfer (秒传), cross-WAN
- "System busy cursor" approach abandoned — replaced with Dashboard-based progress and right-click workflow

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
- File clipboard detection already implemented: `common.rs:180` has `ContentFormat::Files` + `ctx.get_files()` support
- File clipboard write already implemented: `common.rs:463` has `ctx.set_files()` for writing file references to clipboard
- `SpoolManager` (`uc-infra/src/clipboard/spool_manager.rs`): Atomic write pattern with Unix permissions (0o600/0o700)
- `NetworkEvent::TransferProgress` already exists — can be extended for file transfer progress

### Established Patterns
- Port/Adapter pattern: New `FileTransportPort` trait in `uc-core/ports/`, implementation in `uc-platform/adapters/`
- Use case pattern: `SyncOutboundFileUseCase` / `SyncInboundFileUseCase` in `uc-app/usecases/`
- UseCases accessor: New accessor methods on `UseCases` struct in `uc-tauri/bootstrap/runtime.rs`
- Settings model: Extend `SyncSettings` in `uc-core/src/settings/model.rs` with file sync fields
- System notifications: Via Tauri notification plugin or native APIs
- Concurrency control: Semaphore-based per-peer + global limits (PairingStreamService pattern)
- Framing: `write_length_prefixed()` / `read_length_prefixed()` for message framing

### Integration Points
- `ClipboardWatcher` needs file copy classification update: `classify_snapshot()` should map `text/uri-list` to `ContentTypeCategory::File`
- `PlatformRuntime` event loop: Add `PlatformEvent::FileCopied` variant
- `AppRuntime` callback: New handler for file copy events
- `main.rs`: Register new commands in `invoke_handler![]`
- Frontend: New file entry component in Dashboard with right-click context menu, new settings fields in Sync section
- `wiring.rs`: Wire new file transfer service and ports
- `NetworkPorts` struct in `uc-app/src/deps.rs`: Add `file_transfer: Arc<dyn FileTransportPort>`
- `NetworkEvent` enum: Add file transfer event variants (Started, Completed, Failed, Cancelled)
- `ContentTypes.file` toggle: Update `is_content_type_allowed()` to actually check `ct.file` field

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
