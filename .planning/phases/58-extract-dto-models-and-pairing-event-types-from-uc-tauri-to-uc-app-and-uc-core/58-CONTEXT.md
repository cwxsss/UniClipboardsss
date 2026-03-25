# Phase 58: Extract DTO models and pairing event types from uc-tauri to uc-app and uc-core - Context

**Gathered:** 2026-03-25
**Status:** Ready for planning

<domain>
## Phase Boundary

Extract all shared DTO models, pairing event types, and related error types from `uc-tauri` to their proper architectural homes (`uc-app`, `uc-core`, `uc-daemon-client`), continuing v0.4.0 runtime mode separation. After this phase, `uc-tauri` retains only Tauri-specific command handlers and wiring — no shared business DTOs.

**In scope:**

- Move pairing aggregation DTOs (`P2PPeerInfo`, `PairedPeer`) to uc-app
- Move pairing event types (`P2PPairingVerificationEvent`, `P2PPairingVerificationKind`) to uc-core
- Move pairing request/response DTOs (`P2PPairingRequest`, `P2PPairingResponse`, `P2PPinVerifyRequest`) to uc-app
- Move 11 clipboard DTOs (`ClipboardEntryProjection`, `ClipboardEntriesResponse`, `ClipboardEntryDetail`, `ClipboardEntryResource`, `ClipboardStats`, `ClipboardItemDto`, `ClipboardTextItemDto`, `ClipboardImageItemDto`, `ClipboardLinkItemDto`, `ClipboardItemResponse`, `LifecycleStatusDto`) to uc-app
- Move `DaemonPairingRequestError` to uc-daemon-client
- Delete original files from uc-tauri and update all import paths

**Out of scope:**

- New functionality or API changes
- Daemon API transport DTOs (already properly placed in uc-daemon)
- Frontend code changes
- Modifying business logic

</domain>

<decisions>
## Implementation Decisions

### Type placement strategy

- **D-01:** `P2PPeerInfo` and `PairedPeer` → uc-app (application layer, alongside existing `P2pPeerSnapshot` and `LocalDeviceInfo`)
- **D-02:** `P2PPairingVerificationEvent` and `P2PPairingVerificationKind` → uc-core (cross-layer event contract, alongside `HostEvent`/`RealtimeFrontendEvent`)

### Extraction scope

- **D-03:** All 11 clipboard DTOs from `uc-tauri/src/models/mod.rs` → uc-app (keeping uc-tauri models-free)
- **D-04:** Pairing request/response DTOs (`P2PPairingRequest`, `P2PPairingResponse`, `P2PPinVerifyRequest`) → uc-app
- **D-06:** `DaemonPairingRequestError` → uc-daemon-client (daemon client error type belongs with client crate)

### Migration strategy

- **D-05:** Direct delete + update all imports. No re-export stubs. Clean cut — all consumer code updated to import from new locations.

### Claude's Discretion

- Exact module organization within uc-app for the extracted DTOs (e.g., `models/` module, or colocate with use cases)
- Exact file within uc-core for pairing event types (e.g., new file in `network/` or `ports/`)
- Order of extraction (pairing first vs clipboard first)
- Whether to consolidate related types in the same file or keep them separate
- Helper method placement (e.g., `P2PPairingVerificationEvent` constructors like `request()`, `verification()`, etc.)

</decisions>

<canonical_refs>

## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Source files (extraction origins)

- `src-tauri/crates/uc-tauri/src/models/mod.rs` — 11 clipboard DTOs to extract
- `src-tauri/crates/uc-tauri/src/commands/pairing.rs` — P2PPeerInfo, PairedPeer, P2PPairingRequest/Response/PinVerifyRequest + mapping functions
- `src-tauri/crates/uc-tauri/src/events/p2p_pairing.rs` — P2PPairingVerificationEvent and P2PPairingVerificationKind with constructor helpers
- `src-tauri/crates/uc-tauri/src/events/mod.rs` — Event module exports

### Target crate structures (destination patterns)

- `src-tauri/crates/uc-app/src/usecases/pairing/mod.rs` — Existing pairing exports (P2pPeerSnapshot, LocalDeviceInfo)
- `src-tauri/crates/uc-app/src/usecases/pairing/get_p2p_peers_snapshot.rs` — P2pPeerSnapshot pattern reference
- `src-tauri/crates/uc-core/src/network/` — Existing network domain types and daemon_api_strings
- `src-tauri/crates/uc-core/src/ports/` — Existing port definitions including HostEvent

### Error types

- `src-tauri/crates/uc-tauri/src/commands/pairing.rs` — DaemonPairingRequestError definition
- `src-tauri/crates/uc-daemon-client/src/http/pairing.rs` — Current daemon client pairing types

### Consumer files (import update targets)

- `src-tauri/crates/uc-tauri/src/adapters/host_event_emitter.rs` — Uses P2PPairingVerificationEvent
- `src-tauri/crates/uc-tauri/tests/daemon_command_shell.rs` — Imports PairedPeer

</canonical_refs>

<code_context>

## Existing Code Insights

### Reusable Assets

- `P2pPeerSnapshot` in uc-app: Established pattern for aggregation DTOs in pairing use case module
- `HostEvent` / `RealtimeFrontendEvent` in uc-core: Established pattern for cross-layer event types
- `daemon_api_strings` module in uc-core/network: Reference for organizing shared constants/types in core

### Established Patterns

- uc-app pairing module exports types via `pub use` in `mod.rs`
- Serde derives (`Serialize`, `Deserialize`, `Clone`, `Debug`) on all DTOs
- `specta::Type` derive on frontend-facing DTOs for TypeScript binding generation
- Constructor helper methods on event types (e.g., `P2PPairingVerificationEvent::request()`)

### Integration Points

- `uc-tauri/src/commands/pairing.rs` — Primary consumer of pairing DTOs, uses mapping functions between domain and DTO types
- `uc-tauri/src/adapters/host_event_emitter.rs` — Consumes P2PPairingVerificationEvent for Tauri emit
- `uc-tauri/src/commands/clipboard.rs` — Primary consumer of clipboard DTOs
- `uc-daemon-client/src/http/pairing.rs` — Will own DaemonPairingRequestError after extraction

</code_context>

<specifics>
## Specific Ideas

No specific requirements — open to standard approaches

</specifics>

<deferred>
## Deferred Ideas

### Reviewed Todos (not folded)

- "修复 setup 配对确认提示缺失" — UI bug, unrelated to DTO extraction. Belongs in a separate UI fix phase.

</deferred>

---

_Phase: 58-extract-dto-models-and-pairing-event-types-from-uc-tauri-to-uc-app-and-uc-core_
_Context gathered: 2026-03-25_
