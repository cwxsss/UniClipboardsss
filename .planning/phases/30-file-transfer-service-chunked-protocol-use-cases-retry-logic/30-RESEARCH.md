# Phase 30: File Transfer Service — Research

**Researched:** 2026-03-13
**Status:** Complete
**Phase Goal:** Implement the FileTransferService with libp2p stream protocol, chunked file transfer with Blake3 hash verification, send/receive use cases, serial queue for multi-file operations, and auto-retry with exponential backoff.

## Validation Architecture

This section maps phase requirements to testable validation criteria.

### Requirement Coverage

| Requirement ID | Description                                 | Validation Approach                                               |
| -------------- | ------------------------------------------- | ----------------------------------------------------------------- |
| FSYNC-TRANSFER | File transfer service with chunked protocol | Unit tests for chunking, integration test for end-to-end transfer |

### Validation Criteria

1. **FileTransferService accepts and opens streams** — Unit test: service registers `/uniclipboard/file-transfer/1.0.0` protocol, accept loop processes incoming streams
2. **Chunked transfer protocol works end-to-end** — Unit test: sender chunks file into 256KB blocks, receiver reassembles, Blake3 hash matches
3. **Atomic file write with temp file** — Unit test: file written to `.tmp` suffix, renamed after hash verification, permissions 0600 on Unix
4. **Serial queue processes files in order** — Unit test: queue accepts multiple files, processes sequentially, new files appended during transfer
5. **Retry logic with exponential backoff** — Unit test: failed transfer retries with increasing delay, stops after max retries
6. **Sync policy filtering** — Unit test: `SyncOutboundFileUseCase` uses `apply_sync_policy` to filter eligible peers
7. **Disk space pre-check** — Unit test: inbound transfer rejects when insufficient disk space
8. **Symlink/hardlink rejection** — Unit test: outbound transfer rejects symlinks and hardlinks

## Codebase Analysis

### PairingStreamService as Template (600+ LoC)

**Location:** `src-tauri/crates/uc-platform/src/adapters/pairing_stream/service.rs`

The PairingStreamService provides the canonical libp2p stream service pattern with:

- **Arc<Inner> pattern:** `PairingStreamService` wraps `Arc<PairingStreamServiceInner>` for cloneability
- **Accept loop:** `spawn_accept_loop()` → `run_accept_loop()` listens on `StreamProtocol::new(ProtocolId::X.as_str())`
- **Session management:** HashMap of session handles with write channels and shutdown signals
- **Concurrency control:** Global semaphore (`MAX_PAIRING_CONCURRENCY = 16`) + per-peer semaphore (`PER_PEER_CONCURRENCY = 2`)
- **Read/write loop split:** `tokio::io::split(stream)` → separate `read_loop` and `write_loop` tasks with `tokio::select!`
- **Shutdown coordination:** `watch::channel` for clean shutdown propagation
- **Framing:** `write_length_prefixed()` / `read_length_prefixed()` from a shared `framing` module
- **Error event emission:** Failures emit `NetworkEvent::PairingFailed` through the event channel

**Key adaptation needed for FileTransferService:**

- File transfer is simpler: one-shot stream (no persistent session), no bidirectional messaging
- Instead of session HashMap, track active transfer IDs
- Reuse semaphore pattern (max 2 concurrent per peer already specified in context)
- Replace JSON encode/decode with binary chunk framing

### ProtocolId Registration

**Location:** `src-tauri/crates/uc-core/src/network/protocol_ids.rs`

Current protocols: `Pairing`, `PairingStream`, `Business`. Phase 28 should have added `FileTransfer` variant with `/uniclipboard/file-transfer/1.0.0`. This phase consumes that.

### NetworkEvent Extensions

**Location:** `src-tauri/crates/uc-core/src/network/events.rs`

`NetworkEvent::TransferProgress(TransferProgress)` already exists with full serialization tests. Phase 28 should have added file-specific events. This phase will use them.

### TransferProgress Port

**Location:** `src-tauri/crates/uc-core/src/ports/transfer_progress.rs`

Already defined with `TransferProgressPort` trait, `TransferProgress` struct (transfer_id, peer_id, direction, chunks_completed, total_chunks, bytes_transferred, total_bytes), and `NoopTransferProgressPort` for tests.

### SyncOutboundClipboardUseCase and apply_sync_policy

**Location:** `src-tauri/crates/uc-app/src/usecases/clipboard/sync_outbound.rs`

`apply_sync_policy` is a method on `SyncOutboundClipboardUseCase` that:

1. Loads global settings, checks `auto_sync` master toggle
2. Classifies snapshot content type once
3. For each peer: loads paired device settings, checks per-device auto_sync and content type filter
4. Returns filtered peer list

**For file transfer:** The outbound file use case needs to reuse this logic. Since `apply_sync_policy` is on the clipboard use case, the file use case should either:

- Extract the policy logic into a shared utility (preferred — avoids coupling to clipboard use case)
- Duplicate with adaptation for file content type

The function signature takes `&[DiscoveredPeer]` and `&SystemClipboardSnapshot`. For file sync, we need a variant that works with `ContentTypeCategory::File` directly (no snapshot needed since we already know it's a file).

### SpoolManager Atomic Write Pattern

**Location:** `src-tauri/crates/uc-infra/src/clipboard/spool_manager.rs`

Provides the atomic write pattern:

- Directory creation with `create_dir_all`
- Unix permissions: 0o700 for directory, 0o600 for files
- `#[cfg(unix)]` gating for permission operations

For file transfer, adapt this pattern for temp file handling:

- Write to `{file-cache}/{transfer_id}.tmp`
- Verify Blake3 hash
- `fs::rename()` to final path

### UseCases Accessor Pattern

**Location:** `src-tauri/crates/uc-tauri/src/bootstrap/runtime.rs` (line 462+)

`UseCases<'a>` struct provides factory methods for each use case, wired from `self.runtime.deps.*`. New use cases need:

1. Accessor method on `UseCases` (e.g., `sync_outbound_file()`)
2. Constructed from `runtime.deps` port references
3. Called from Tauri commands via `runtime.usecases().sync_outbound_file()`

### Wiring Module

**Location:** `src-tauri/crates/uc-tauri/src/bootstrap/wiring.rs`

The wiring module is the single assembly point for all dependencies. FileTransferService needs to be:

1. Constructed in wiring with `stream::Control` and `event_tx`
2. Accept loop spawned during app setup
3. Registered as a dependency accessible from AppRuntime

### AppDeps Structure

The runtime organizes deps by domain: `ClipboardPorts`, `DevicePorts`, `SecurityPorts`, `StoragePorts`, `NetworkPorts`, `SystemPorts`. File transfer will need ports added (likely to `NetworkPorts` or a new `FileSyncPorts` group).

## Technical Design Decisions

### Chunked Transfer Protocol Design

The protocol flow per the context: `announce → accept → data (chunked) → complete`.

**Message types** (defined in Phase 28):

1. `FileAnnounce`: sender → receiver (filename, size, hash, transfer_id)
2. `FileAccept` / `FileReject`: receiver → sender
3. `FileChunk`: sender → receiver (chunk_index, data)
4. `FileComplete`: sender → receiver (final hash)

**Binary framing:** Reuse `write_length_prefixed()` / `read_length_prefixed()` pattern. Each message is length-prefixed, with a type tag byte followed by payload.

### Serial Queue Design

```
TransferQueue {
    queue: VecDeque<FileTransferRequest>,
    active: Option<ActiveTransfer>,
}
```

- `mpsc::Sender<FileTransferRequest>` for enqueueing
- Background task drains queue, processes one at a time
- New copies during transfer: append to queue tail
- Text/image copies: completely independent (different protocol)

### Retry Logic Design

```rust
struct RetryPolicy {
    max_retries: u32,        // e.g., 3
    initial_delay: Duration, // e.g., 1s
    max_delay: Duration,     // e.g., 30s
    multiplier: f64,         // e.g., 2.0
}
```

- Retry from current chunk (not byte offset — no resume in V1)
- On network interruption: exponential backoff
- On hash mismatch: delete temp file, notify user, NO retry
- On max retries exceeded: emit error event for UI notification

### File Safety Checks

Before sending:

- `symlink_metadata()` to detect symlinks (reject if `is_symlink()`)
- `metadata().nlink() > 1` to detect hardlinks (reject)
- File existence check (reject if deleted between copy detection and transfer)

Before receiving:

- Disk space check: `fs2::available_space()` or platform-specific API
- Per-device quota check (500MB default from context)
- Filename validation (module from Phase 28)

### Batch Clipboard Write

For small files:

- Track batch ID (group of files copied together)
- Wait for ALL files in batch to complete
- Write all file references via `ctx.set_files()` at once
- Race detection: if user copies something else during transfer, cancel auto-write; files remain in Dashboard

## Dependency Map

### Phase 28 Outputs Consumed

| Artifact                    | Expected Location                     | Usage                                             |
| --------------------------- | ------------------------------------- | ------------------------------------------------- |
| File transfer message types | `uc-core/src/network/protocol.rs`     | FileAnnounce, FileAccept, FileChunk, FileComplete |
| FileTransportPort trait     | `uc-core/src/ports/`                  | Port for file transfer operations                 |
| ProtocolId::FileTransfer    | `uc-core/src/network/protocol_ids.rs` | Protocol registration                             |
| DB schema for file entries  | `uc-infra/src/db/`                    | Persist file transfer records                     |
| Settings model extensions   | `uc-core/src/settings/`               | File sync settings (thresholds, quotas)           |
| Filename validation module  | `uc-core/src/` or `uc-infra/src/`     | Sanitize incoming filenames                       |
| NetworkEvent file variants  | `uc-core/src/network/events.rs`       | FileTransferStarted, FileTransferCompleted, etc.  |
| PlatformEvent::FileCopied   | `uc-platform/src/`                    | Triggers outbound file sync                       |
| ContentTypeCategory::File   | `uc-core/src/settings/`               | Content type classification                       |

### New Crate Dependencies

| Crate            | Purpose                | Version Guidance             |
| ---------------- | ---------------------- | ---------------------------- |
| blake3           | File hash verification | Latest stable                |
| fs2 (or sysinfo) | Disk space checking    | For pre-transfer space check |

### Files Modified (Estimated)

| File                                                 | Change                                  |
| ---------------------------------------------------- | --------------------------------------- |
| `uc-platform/src/adapters/file_transfer/service.rs`  | NEW: FileTransferService (main service) |
| `uc-platform/src/adapters/file_transfer/mod.rs`      | NEW: Module declaration                 |
| `uc-platform/src/adapters/file_transfer/protocol.rs` | NEW: Chunked protocol implementation    |
| `uc-platform/src/adapters/file_transfer/queue.rs`    | NEW: Serial transfer queue              |
| `uc-platform/src/adapters/mod.rs`                    | Add file_transfer module                |
| `uc-app/src/usecases/file_sync/sync_outbound.rs`     | NEW: SyncOutboundFileUseCase            |
| `uc-app/src/usecases/file_sync/sync_inbound.rs`      | NEW: SyncInboundFileUseCase             |
| `uc-app/src/usecases/file_sync/mod.rs`               | NEW: Module declaration                 |
| `uc-app/src/usecases/mod.rs`                         | Add file_sync module                    |
| `uc-tauri/src/bootstrap/runtime.rs`                  | Add use case accessors                  |
| `uc-tauri/src/bootstrap/wiring.rs`                   | Wire FileTransferService                |
| `src-tauri/Cargo.toml`                               | Add blake3, fs2 deps                    |

## Risk Assessment

### High Risk

- **Phase 28 dependency:** All message types, ports, and schema from Phase 28 must exist. If Phase 28 is incomplete, this phase cannot proceed.

### Medium Risk

- **Binary protocol correctness:** Length-prefixed framing with large chunks needs careful boundary handling. Off-by-one in chunk indexing or size calculation could corrupt files.
- **Concurrent transfer state:** Serial queue with async operations needs careful lock management to avoid deadlocks.

### Low Risk

- **Blake3 performance:** Blake3 is extremely fast (multi-GB/s), will not be a bottleneck.
- **Temp file cleanup:** If app crashes mid-transfer, orphan temp files remain. Phase 32 handles auto-cleanup.

## RESEARCH COMPLETE
