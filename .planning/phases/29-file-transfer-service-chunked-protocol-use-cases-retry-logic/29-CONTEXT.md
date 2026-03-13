# Phase 29: File transfer service — chunked protocol, use cases, retry logic - Context

**Gathered:** 2026-03-13
**Updated:** 2026-03-13 (split from monolithic Phase 28)
**Status:** Ready for planning
**Scope:** ~1,500-2,000 LoC
**Depends on:** Phase 28 (message types, ports, schema must exist)

<domain>
## Phase Boundary

Implement the actual file transfer engine: FileTransferService (libp2p stream handler), chunked transfer with Blake3 hash verification, send/receive use cases, serial queue for multi-file operations, and auto-retry with exponential backoff. After this phase, files can be transferred between paired devices over LAN.

</domain>

<decisions>
## Implementation Decisions

### FileTransferService
- New libp2p stream service following `PairingStreamService` pattern (600+ LoC template)
- Registers protocol `/uniclipboard/file-transfer/1.0.0`
- Handles incoming file transfer streams
- Manages outbound file transfer initiation
- Concurrency limit: max 2 concurrent file transfers per peer (semaphore-based, PairingStreamService pattern)

### Chunked Transfer Protocol
- Sequential chunk transmission using 256KB chunk size (reference existing pattern)
- Binary length-prefixed framing: `write_length_prefixed()` / `read_length_prefixed()`
- Message flow: announce → accept → data (chunked) → complete
- Whole-file Blake3 hash verification on completion
- Temporary file + atomic rename on disk (write to `.tmp` suffix, verify hash, then `fs::rename()`)
- Temp files stored in app data directory under `file-cache/` subdirectory
- Filesystem permissions: 0600 on Unix for temp files

### Use Cases
- `SyncOutboundFileUseCase`: Triggered on file copy detection, sends file to eligible peers
  - Reuse `apply_sync_policy` logic for peer selection (global auto_sync + per-device auto_sync + ContentTypeCategory::File)
  - File synced to all eligible paired devices that are online
  - Checks file existence before transfer, fails with notification if source file deleted
  - Rejects symlinks via `symlink_metadata()`, rejects hardlinks via `metadata().nlink() > 1`
- `SyncInboundFileUseCase`: Handles incoming file transfer
  - Small files (below threshold): auto-pull in background
  - Large files (above threshold): sync metadata only, content pulled on-demand (Phase 30 UI trigger)
  - Disk space pre-check before accepting transfer
  - Per-device file-cache quota enforcement (default 500MB)
  - Filename validation (using module from Phase 28)
  - Hash verification on completion; failure = delete temp file, notify user, no retry

### Serial Queue for Multi-File
- When multiple files are copied, transfer serially (one file completes announce→data→complete before next starts)
- New file copy during transfer: new file(s) appended to queue tail, current transfer continues
- Text/image copy during file transfer: independent — file transfer on dedicated protocol, clipboard sync on separate channel

### Retry Logic
- Auto-retry with exponential backoff on network interruption
- Fail after max retries with user notification
- No断点续传 in V1 — retry from current chunk, not from byte offset
- Sender disconnect during transfer: receiver waits for timeout, deletes temp file, marks "transfer failed"

### Receiver Clipboard Write
- Small files: multi-file batch waits for ALL files in batch to complete, then write all file references via `set_files()` at once
- Clipboard race detection: if user performs new copy during file transfer, cancel auto-write to clipboard; files remain in Dashboard for manual copy
- File clipboard write already implemented: `common.rs:463` has `ctx.set_files()`

### Multi-device Broadcast
- Reuse existing `apply_sync_policy` logic: global auto_sync + per-device auto_sync + content type filter
- Offline devices skipped — no queuing or deferred delivery
- No paired devices online: silent ignore (consistent with text/image sync)

### Rate Limiting
- Limit file announce frequency per peer (prevent flooding/resource exhaustion)

### Claude's Discretion
- Exact chunk size (can reference existing 256KB pattern)
- Temp file naming convention
- Exact retry policy parameters (max retries, backoff intervals)
- Rate limit parameters (announces per peer per second)

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets (This Phase)
- `PairingStreamService` (`uc-platform/src/adapters/pairing_stream/service.rs`): Full libp2p_stream pattern — FileTransferService follows same architecture
- `apply_sync_policy` (`uc-app/src/usecases/clipboard/sync_outbound.rs`): Reuse for determining which peers receive file
- V3 binary protocol with 256KB chunking: Reference for chunk-based transfer design
- File clipboard write: `common.rs:463` has `ctx.set_files()` for writing file references to clipboard
- `SpoolManager` (`uc-infra/src/clipboard/spool_manager.rs`): Atomic write pattern with Unix permissions (0o600/0o700)
- Concurrency control: Semaphore-based per-peer + global limits (PairingStreamService pattern)
- Framing: `write_length_prefixed()` / `read_length_prefixed()` for message framing

### Established Patterns
- Use case pattern: `SyncOutboundFileUseCase` / `SyncInboundFileUseCase` in `uc-app/usecases/`
- UseCases accessor: New accessor methods on `UseCases` struct in `uc-tauri/bootstrap/runtime.rs`

### Integration Points
- Phase 28 outputs consumed: message types, FileTransportPort trait, protocol ID, database schema, settings model, filename validation, NetworkEvent variants
- `PlatformRuntime` event loop: Handle `PlatformEvent::FileCopied` variant
- `AppRuntime` callback: New handler for file copy events
- `wiring.rs`: Wire FileTransferService and ports
- `main.rs`: Register new commands in `invoke_handler![]`

### Verified Code References
| Reference | Status | Finding |
|-----------|--------|---------|
| PairingStreamService pattern | ✅ Verified | 600+ LoC, excellent template for FileTransferService |
| V3 binary protocol 256KB chunking | ✅ Verified | `CHUNK_SIZE = 256 * 1024`, architectural reference |
| SpoolManager atomic writes | ✅ Verified | Unix permissions 0o600/0o700; needs `.tmp` + rename adaptation |
| File clipboard write (common.rs:463) | ✅ Verified | `ctx.set_files()` working on Windows/macOS |
| NetworkEvent::TransferProgress | ✅ Verified | Complete port + struct + serialization tests |

</code_context>

<deferred>
## Deferred Ideas

- Multi-stream concurrent file transfer — future optimization
- Breakpoint resume (断点续传) at byte offset level — future reliability improvement
- Per-peer bandwidth throttling — defer (LAN scenario, max 2 concurrent transfers sufficient)

</deferred>

---

*Phase: 29 (Transfer) — split from original monolithic Phase 28*
*Context gathered: 2026-03-13*
