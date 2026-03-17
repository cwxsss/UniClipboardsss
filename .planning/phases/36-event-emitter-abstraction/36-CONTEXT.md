# Phase 36: Event Emitter Abstraction - Context

**Gathered:** 2026-03-17
**Status:** Ready for planning

<domain>
## Phase Boundary

Replace hardcoded `AppHandle::emit()` calls in uc-tauri with an abstract `HostEventEmitterPort` trait and adapter implementations. Background tasks deliver host events through the abstract port, eliminating direct AppHandle coupling. GUI behavior remains unchanged.

</domain>

<decisions>
## Implementation Decisions

### Trait design

- Single method: `fn emit(&self, event: HostEvent) -> Result<(), EmitError>`
- Trait name: `HostEventEmitterPort`
- Trait and all event types live in `uc-core/ports/`
- Components receive the port via `Arc<dyn HostEventEmitterPort>` constructor injection (consistent with existing port patterns like `Arc<dyn SettingsPort>`)

### Event type system

- Strong-typed `HostEvent` enum with nested sub-enums per domain: `HostEvent::Clipboard(ClipboardHostEvent)`, `HostEvent::Pairing(PairingHostEvent)`, `HostEvent::Transfer(TransferHostEvent)`, etc.
- Event types are newly defined in uc-core — NOT moved from uc-tauri. TauriEventEmitter converts HostEvent variants to Tauri-specific event name strings and payload types internally
- Event name mapping (e.g., `ClipboardHostEvent::NewContent` → `"clipboard://event"`) is the adapter's responsibility — uc-core knows nothing about event name strings
- Directed events (`emit_to` for quick-panel/preview-panel) are excluded from the trait — they remain uc-tauri internal as pure GUI behavior

### Migration strategy

- One-shot replacement: create trait + TauriEventEmitter + LoggingEventEmitter + replace all 30+ emit calls in a single atomic migration
- Clipboard watcher, peer discovery, and sync scheduler accept `Arc<dyn HostEventEmitterPort>` instead of `AppHandle<R>`
- Existing `uc-tauri/events/mod.rs` forward\_\* functions and old event types (ClipboardEvent, EncryptionEvent, SettingChangedEvent) are deleted after migration — TauriEventEmitter fully replaces them
- The compiler must reject any direct `AppHandle` use in clipboard watcher, peer discovery, and sync scheduler components

### LoggingEventEmitter behavior

- Logs all events — filtering controlled by tracing level configuration
- Log levels vary by event type: error events → `warn!`, key business events → `info!`, discovery changes → `debug!`
- Output uses structured tracing fields: `info!(event_type = "clipboard.new_content", entry_id = %id)` — consistent with existing tracing patterns

### Claude's Discretion

- Exact HostEvent sub-enum variant names and field structures
- EmitError type design (simple string vs structured)
- Internal implementation of TauriEventEmitter's event name mapping (match arms, const table, etc.)
- How to handle the transition of existing tests in events/mod.rs
- Specific tracing level assignment per event variant in LoggingEventEmitter

</decisions>

<canonical_refs>

## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Event abstraction requirements

- `.planning/REQUIREMENTS.md` — EVNT-01 through EVNT-04 define the four success criteria for this phase

### Current event implementation

- `src-tauri/crates/uc-tauri/src/events/mod.rs` — Existing event types (ClipboardEvent, EncryptionEvent, SettingChangedEvent) and forward\_\* functions to be replaced
- `src-tauri/crates/uc-tauri/src/events/transfer_progress.rs` — Transfer progress event forwarding
- `src-tauri/crates/uc-tauri/src/events/p2p_pairing.rs` — P2P pairing event types
- `src-tauri/crates/uc-tauri/src/events/p2p_peer.rs` — P2P peer discovery/connection event types

### Primary emit call sites

- `src-tauri/crates/uc-tauri/src/bootstrap/wiring.rs` — Main wiring with 30+ emit calls across clipboard, pairing, transfer, discovery, and sync domains
- `src-tauri/crates/uc-tauri/src/bootstrap/runtime.rs` — AppRuntime clipboard event emission
- `src-tauri/crates/uc-tauri/src/bootstrap/file_transfer_wiring.rs` — File transfer status emit calls

### Port patterns (reference for trait design)

- `src-tauri/crates/uc-core/src/ports/mod.rs` — Existing port trait registry, injection patterns
- `src-tauri/crates/uc-core/src/ports/setup_event_port.rs` — Similar event port pattern (SetupEventPort)
- `src-tauri/crates/uc-core/src/ports/transfer_progress.rs` — TransferProgressPort as event delivery precedent

### Logging patterns

- `src-tauri/crates/uc-observability/` — Tracing configuration, structured logging patterns

</canonical_refs>

<code_context>

## Existing Code Insights

### Reusable Assets

- `SetupEventPort` in uc-core/ports: Existing event port pattern — can serve as structural reference for HostEventEmitterPort
- `TransferProgressPort`: Another event delivery port with structured data — validates the Arc<dyn Port> injection pattern
- `uc-observability` crate: Dual-output tracing already configured — LoggingEventEmitter can use tracing macros directly

### Established Patterns

- Port injection via `Arc<dyn XxxPort>` through constructors — all infrastructure ports follow this pattern
- Event types use `#[serde(tag = "type")]` for discriminated union serialization — TauriEventEmitter should maintain this for frontend compatibility
- Error types in ports use simple string-based errors or dedicated error enums

### Integration Points

- `wiring.rs` is the main site where `app.clone()` is captured and `app.emit()` is called — this is where HostEventEmitterPort injection replaces AppHandle
- `AppRuntime::set_app_handle()` currently stores AppHandle for event emission — will need restructuring to use HostEventEmitterPort instead
- `AppDeps` domain sub-structs may need to carry the emitter port for components that need it

</code_context>

<specifics>
## Specific Ideas

No specific requirements — open to standard approaches

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

_Phase: 36-event-emitter-abstraction_
_Context gathered: 2026-03-17_
