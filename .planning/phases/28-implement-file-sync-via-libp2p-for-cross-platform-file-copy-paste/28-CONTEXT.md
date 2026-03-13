# Phase 28: File sync foundation — message types, ports, classification fix, schema, settings - Context

**Gathered:** 2026-03-13
**Updated:** 2026-03-13 (split from monolithic Phase 28)
**Status:** Ready for planning
**Scope:** ~1,500-2,000 LoC

<domain>
## Phase Boundary

Establish the file sync foundation layer. This phase creates the data structures, port traits, database schema, and settings model that phases 29-31 build upon. Also fixes the critical file classification bug where `file://` URIs are misclassified as `Link` instead of `File`.

No actual file transfer logic in this phase — only the contracts, types, and storage layer.

</domain>

<decisions>
## Implementation Decisions

### Message Types
- Define file transfer message enum: `Announce`, `Accept`, `Data`, `Complete`, `Cancel`, `Error`
- `Announce` contains: filename, file size, Blake3 hash, batch_id (for multi-file grouping)
- `Data` contains: chunk index, chunk bytes
- `Complete` contains: final Blake3 hash for whole-file verification
- `Cancel` contains: reason string
- `Error` contains: error code + message
- Message format: binary length-prefixed framing (consistent with clipboard transfer protocol, not JSON)

### FileTransportPort Trait
- New `FileTransportPort` trait in `uc-core/ports/` following `ClipboardTransportPort` pattern
- Methods: `send_file_announce()`, `send_file_data()`, `send_file_complete()`, `cancel_transfer()`
- Implementation will be in `uc-platform/adapters/` (Phase 29)

### Protocol Registration
- New `ProtocolId::FileTransfer` variant for `/uniclipboard/file-transfer/1.0.0`
- Add to `uc-core/src/network/protocol_ids.rs`

### File Classification Fix (CRITICAL)
- Current bug: `text/uri-list` MIME is classified as `ContentTypeCategory::Link`, not `File`
- Fix: add sub-classification logic to distinguish `file://` paths → `File` vs `http(s)://` URLs → `Link`
- Update `classify_snapshot()` and `is_content_type_allowed()` accordingly
- Update `ContentTypes.file` toggle to actually check `ct.file` field

### Database Schema
- New table for file transfer entries (filename, size, hash, status, source_device, batch_id, cached_path)
- Status enum: `Pending`, `Transferring`, `Completed`, `Failed`, `Expired`

### Settings Model Extension
- Extend `SyncSettings` in `uc-core/src/settings/model.rs` with file sync fields:
  - `file_sync_enabled: bool` (default true)
  - `small_file_threshold: u64` (default 10MB)
  - `max_file_size: u64` (default 5GB)
  - `file_cache_quota_per_device: u64` (default 500MB)
  - `file_retention_hours: u32` (default 24)
  - `file_auto_cleanup: bool` (default true)

### Security — Filename Validation Module
- Create reusable filename validation function for receiver side:
  - Reject null bytes (`\0`), control characters (`\x01-\x1F`)
  - Reject Windows reserved names (`CON`, `PRN`, `AUX`, `NUL`, `COM1-COM9`, `LPT1-LPT9`)
  - Reject filenames exceeding 255 bytes
  - Reject leading dots (hidden files)
  - Reject whitespace-only or empty filenames
  - Reject Unicode tricks (RTL override `\u202e`, zero-width characters)
  - Path traversal protection: validate no `..` components, basename only

### NetworkEvent Extension
- Add file transfer event variants to `NetworkEvent` enum: `FileTransferStarted`, `FileTransferCompleted`, `FileTransferFailed`, `FileTransferCancelled`

### Wiring Preparation
- Add `file_transfer: Arc<dyn FileTransportPort>` to `NetworkPorts` struct in `uc-app/src/deps.rs`
- Add `PlatformEvent::FileCopied` variant to platform event loop

### Claude's Discretion
- Exact database migration implementation details
- Filename validation edge cases beyond the listed rules
- Unicode filename normalization strategy (NFC recommended for cross-platform)

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets (This Phase)
- `ContentTypeCategory::File` (`uc-core/src/settings/content_type_filter.rs`): Already defined, currently always-sync. Must be made filterable
- `ProtocolId` enum (`uc-core/src/network/protocol_ids.rs`): Add `FileTransfer` variant
- `ClipboardTransportPort` (`uc-core/src/ports/clipboard_transport.rs`): Reference for designing `FileTransportPort`
- `NetworkEvent::TransferProgress` already exists — extend for file transfer progress
- File clipboard detection already implemented: `common.rs:180` has `ContentFormat::Files` + `ctx.get_files()` support

### Established Patterns
- Port/Adapter pattern: New `FileTransportPort` trait in `uc-core/ports/`, implementation in `uc-platform/adapters/`
- Settings model: Extend `SyncSettings` in `uc-core/src/settings/model.rs` with file sync fields

### Integration Points
- **CRITICAL**: `ClipboardWatcher` file classification conflict — current `text/uri-list` MIME is classified as `ContentTypeCategory::Link`, not `File`. **Must fix in this phase**
- `NetworkPorts` struct in `uc-app/src/deps.rs`: Add `file_transfer: Arc<dyn FileTransportPort>`
- `NetworkEvent` enum: Add file transfer event variants
- `ContentTypes.file` toggle: Update `is_content_type_allowed()` to actually check `ct.file` field

### Verified Code References
| Reference | Status | Finding |
|-----------|--------|---------|
| File clipboard detection (common.rs:180) | ✅ Verified | `ContentFormat::Files` + `ctx.get_files()` cross-platform |
| apply_sync_policy File filtering | ⚠️ Gap | Files classified as `Link` not `File` — `ct.file` toggle never checked. **Must fix here** |
| Platform abstraction (CF_HDROP/NSFilenamesPboardType) | ✅ Verified | Fully abstracted via clipboard-rs |

</code_context>

<deferred>
## Deferred Ideas

- Directory/folder sync — future phase
- XChaCha20-Poly1305 encryption on file content at rest — deferred for performance (V1 uses filesystem permissions 0600)
- Per-transfer cryptographic signature verification — defer to WAN phase (V1 trusts paired devices)

</deferred>

---

*Phase: 28 (Foundation) — split from original monolithic Phase 28*
*Context gathered: 2026-03-13*
