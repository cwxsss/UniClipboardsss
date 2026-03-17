# Phase 28: File sync foundation — Research

**Researched:** 2026-03-13
**Status:** Complete

## Codebase Analysis

### 1. Existing Protocol Architecture

**ProtocolId enum** (`uc-core/src/network/protocol_ids.rs`):

- Currently has 3 variants: `Pairing`, `PairingStream`, `Business`
- Each maps to a string like `/uniclipboard/business/1.0.0`
- Adding `FileTransfer` follows the same pattern

**ClipboardBinaryPayload** (`uc-core/src/network/protocol/clipboard_payload_v3.rs`):

- Uses binary length-prefixed framing (not JSON) with `std::io::Read/Write`
- Safety limits: MAX_REPRESENTATIONS=1024, MAX_DATA_LEN=256MB
- File transfer messages should follow the same binary codec style

### 2. Existing Port Architecture

**ClipboardTransportPort** (`uc-core/src/ports/clipboard_transport.rs`):

- Pattern: `async_trait`, `Send + Sync`, `Arc<[u8]>` for zero-copy
- Methods: `send_clipboard()`, `broadcast_clipboard()`, `subscribe_clipboard()`, `ensure_business_path()`
- FileTransportPort should follow this trait pattern

**TransferProgress** (`uc-core/src/ports/transfer_progress.rs`):

- Already has `TransferDirection` (Sending/Receiving), progress fields
- Can be reused for file transfers (already generic enough)

**NetworkPorts** (`uc-app/src/deps.rs`):

- Bundles clipboard, peers, pairing, events
- Adding `file_transfer: Arc<dyn FileTransportPort>` here

### 3. Settings Model

**SyncSettings** (`uc-core/src/settings/model.rs`):

- Currently has: `auto_sync`, `sync_frequency`, `content_types`, `max_file_size_mb`
- File sync settings should be a NEW nested struct `FileSyncSettings` within `SyncSettings` (or as a separate field on `Settings`) to avoid polluting the existing sync namespace
- Recommendation: Add as `file_sync: FileSyncSettings` on `Settings` with `#[serde(default)]` for backward compatibility

**Defaults** (`uc-core/src/settings/defaults.rs`):

- Every settings struct has a `Default` impl
- Must add `Default` for `FileSyncSettings`

**TypeScript mirror** (`src/types/setting.ts`):

- Must add `FileSyncSettings` interface
- Must add `file_sync?: FileSyncSettings` to `Settings` interface

### 4. Content Type Classification

**classify_snapshot()** (`uc-core/src/settings/content_type_filter.rs`):

- Bug: `text/uri-list` always returns `Link`, even for `file://` URIs
- Fix approach: When `text/uri-list` is detected, inspect the representation data bytes to check if content starts with `file://`
- `is_content_type_allowed()` must also check `ct.file` for `File` category (currently hardcoded to `true`)

**Representation data access**: `classify_snapshot()` receives `&SystemClipboardSnapshot` which includes `representations[].data: Vec<u8>`. The data can be inspected as UTF-8 to check for `file://` prefix.

### 5. Database Schema

**Diesel migrations** (`uc-infra/migrations/`):

- Sequential naming: `YYYY-MM-DD-NNNNNN_description/`
- Each has `up.sql` and `down.sql`
- Schema auto-generated at `uc-infra/src/db/schema.rs`

**New table needed**: `file_transfer` with columns for tracking file transfer state. The Diesel schema will be auto-regenerated after `diesel migration run`.

### 6. NetworkEvent Extension

**NetworkEvent** (`uc-core/src/network/events.rs`):

- Already has: PeerDiscovered, PeerConnected, ClipboardReceived, TransferProgress, etc.
- File transfer events to add: `FileTransferStarted`, `FileTransferCompleted`, `FileTransferFailed`, `FileTransferCancelled`
- Each needs fields for transfer_id, peer_id, filename, size

### 7. PlatformEvent Extension

**PlatformEvent** (`uc-platform/src/ipc/event.rs`):

- Currently has: Started, Stopped, ClipboardChanged, ClipboardSynced, Error
- Adding `FileCopied` variant for file clipboard detection

## Validation Architecture

### Automated Test Coverage

1. **File transfer message encode/decode** — Round-trip tests for all message variants (Announce, Accept, Data, Complete, Cancel, Error)
2. **Filename validation** — Unit tests for all rejection rules (null bytes, reserved names, path traversal, Unicode tricks, length limits)
3. **Content type classification fix** — Tests for `file://` URIs classified as File, `http://` as Link, mixed content
4. **Settings backward compatibility** — Deserialize old JSON (without file_sync) into new Settings struct
5. **Database migration** — Verify schema creates table with correct columns
6. **is_content_type_allowed with File** — Verify File category respects `ct.file` toggle

### Integration Points to Verify

- NetworkPorts compiles with new `file_transfer` field
- PlatformEvent::FileCopied matches existing event bus patterns
- Settings TOML/JSON round-trip with new fields

## Key Design Decisions

### Message Types Location

Place in `uc-core/src/network/protocol/file_transfer.rs` alongside existing protocol messages.

### FileSyncSettings Placement

Add as `file_sync: FileSyncSettings` on `Settings` struct (top-level, not nested under SyncSettings) to keep file-specific settings cleanly separated.

### Filename Validation Location

Create `uc-core/src/security/filename_validation.rs` as a reusable validation module — security concern, not network concern.

### Database Table Design

```sql
CREATE TABLE file_transfer (
    transfer_id TEXT PRIMARY KEY NOT NULL,
    filename TEXT NOT NULL,
    file_size BIGINT NOT NULL,
    content_hash TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    source_device TEXT NOT NULL,
    batch_id TEXT,
    cached_path TEXT,
    created_at_ms BIGINT NOT NULL,
    updated_at_ms BIGINT NOT NULL
);
```

## RESEARCH COMPLETE
