# Phase 37: Wiring Decomposition - Context

**Gathered:** 2026-03-17
**Status:** Ready for planning

<domain>
## Phase Boundary

Split `wiring.rs` (6328 lines) into a pure Rust assembly module (`assembly.rs`, no Tauri types) and a Tauri-specific event loop module (retains `wiring.rs` name). Migrate ALL remaining `app.emit()` calls in wiring.rs and file_transfer_wiring.rs to `HostEventEmitterPort`, adding new HostEvent domain variants. After migration, `start_background_tasks` loses its `AppHandle<R>` parameter entirely. Move command registration (`invoke_handler` macro) from main.rs into the Tauri-specific module so it owns event loop setup, app handle wiring, AND command registration per ROADMAP SC#2.

**In scope:**

- File split: wiring.rs → assembly.rs (pure) + wiring.rs (Tauri event loops)
- Migrate all 14 remaining app.emit() calls in wiring.rs to HostEventEmitterPort
- Migrate 5 AppHandle<R> functions in file_transfer_wiring.rs to HostEventEmitterPort
- Add new HostEvent sub-enums: PairingHostEvent, SetupHostEvent, SpaceAccessHostEvent
- Remove AppHandle<R> parameter from start_background_tasks after all emits migrated
- Extend TauriEventEmitter with new event → Tauri event name + payload mappings
- Move command registration (invoke_handler![...] block, currently main.rs:852-927) into Tauri-specific module — wiring.rs or a dedicated commands_registration.rs
- Cargo feature gate (`tauri-runtime`) to enable `cargo check -p uc-tauri --no-default-features` verification that assembly.rs compiles without tauri in its dependency tree

**Out of scope:**

- Moving files to other crates (deferred to Phase 38+)
- Creating independent crate for assembly module (deferred to Phase 40)
- Commands-layer emits (commands/pairing.rs, commands/clipboard.rs, encryption.rs, tray.rs)
- emit_to for quick-panel/preview-panel (window-targeted, not broadcast)
- clipboard monitor heartbeat (clipboard_monitor.rs:43)

</domain>

<decisions>
## Implementation Decisions

### Split boundary

- Pure assembly module contains dependency construction AND Tauri-free utility functions: `wire_dependencies`, `wire_dependencies_with_identity_store`, `get_storage_paths`, `create_infra_layer`, `resolve_pairing_device_name`, `resolve_pairing_config`, and related helper functions
- `resolve_pairing_device_name` and `resolve_pairing_config` belong in assembly.rs because they are pure helpers (take `Arc<dyn SettingsPort>`, no Tauri types) and are called from multiple non-event-loop sites: commands/settings.rs, adapters/lifecycle.rs, AND wiring.rs event loops
- `start_background_tasks` and all event loop code stay in wiring.rs (Tauri module) — but after all app.emit() calls are migrated, AppHandle<R> is removed from its signature
- `WiredDependencies` struct definition lives in assembly.rs (it's the return type of wire_dependencies)
- `BackgroundRuntimeDeps` struct definition stays in wiring.rs (only used by start_background_tasks)

### app.emit() migration — complete

- ALL 14 remaining app.emit() calls in wiring.rs are migrated to HostEventEmitterPort in this phase
- This includes: setup-state-changed (1), space-access-completed + p2p-space-access-completed (2), pairing-verification (7), pairing-events-subscribe-failure/recovered (2), inbound-clipboard-subscribe-error/retry (2)
- New HostEvent sub-enums follow Phase 36 pattern: `HostEvent::Pairing(PairingHostEvent)`, `HostEvent::Setup(SetupHostEvent)`, `HostEvent::SpaceAccess(SpaceAccessHostEvent)`
- TauriEventEmitter is extended with event name mapping and camelCase payload DTOs for each new domain
- Frontend contract tests mandatory for every new event mapping (same pattern as Phase 36)

### file_transfer_wiring.rs migration — complete

- All 5 functions (handle_transfer_progress, handle_transfer_completed, handle_transfer_failed, spawn_timeout_sweep, reconcile_on_startup) migrated from AppHandle<R> to HostEventEmitterPort
- File stays in uc-tauri/bootstrap/ — no location change in this phase
- FileTransferStatusPayload remains as-is (already has serde annotations for Tauri)
- After migration, file_transfer_wiring.rs has zero Tauri imports (but stays in uc-tauri crate for now)

### Module naming and organization

- Pure assembly: `assembly.rs` in uc-tauri/src/bootstrap/
- Tauri event loops: retains `wiring.rs` name in uc-tauri/src/bootstrap/
- mod.rs uses `pub use assembly::*` and `pub use wiring::*` — external import paths unchanged (transparent refactor)

### Command registration ownership

- ROADMAP SC#2 requires the Tauri-specific module to own "event loop setup, app handle wiring, and command registration"
- Currently command registration lives in main.rs:852-927 (`invoke_handler![...]` macro with ~60 commands)
- Move the `invoke_handler` generation into the Tauri module (wiring.rs or a dedicated helper function) so main.rs delegates to it
- main.rs becomes a thin entry point: config → assembly → tauri-module (which provides event loops + command handler)

### Tauri-purity verification (ROADMAP SC#4 — not downgraded)

- assembly.rs must pass `cargo check` without tauri in its dependency tree — per ROADMAP.md:93,96 verbatim
- Mechanism: Cargo feature gate in uc-tauri's Cargo.toml (e.g., `tauri-runtime` feature, default-enabled). assembly.rs code is NOT gated behind the feature; wiring.rs and all Tauri-specific code IS gated. `cargo check -p uc-tauri --no-default-features` compiles assembly.rs without tauri deps
- This is a real `cargo check` verification, NOT a grep-only check
- The feature gate is the lightest mechanism that satisfies SC#4 without extracting to a separate crate (which happens in Phase 40)
- grep/CI as supplementary check: also verify assembly.rs has zero `tauri::`, `AppHandle`, `Emitter`, `Runtime` imports as a belt-and-suspenders guard

### Commit split strategy (MANDATORY — hex boundary + atomic commit rules)

- Commits MUST respect hex boundaries per AGENTS.md: uc-core changes in separate commits from uc-tauri changes
- Minimum commit structure:
  1. `arch:` New HostEvent sub-enums (PairingHostEvent, SetupHostEvent, SpaceAccessHostEvent) in uc-core — `cargo check -p uc-core` passes
  2. `impl:` TauriEventEmitter + LoggingEventEmitter extended with new variants + contract tests — `cargo check -p uc-tauri` passes
  3. `refactor:` Migrate app.emit() calls + file_transfer_wiring.rs to HostEventEmitterPort, remove AppHandle<R> from start_background_tasks — `cargo test` passes
  4. `refactor:` Split wiring.rs → assembly.rs + wiring.rs, add feature gate, move command registration — `cargo check -p uc-tauri --no-default-features` passes
- Planner may further split these if individual commits are too large, but must NOT merge uc-core and uc-tauri changes into a single commit

### Claude's Discretion

- Exact PairingHostEvent / SetupHostEvent / SpaceAccessHostEvent variant names and field structures
- Internal refactoring of wiring.rs closure patterns to accommodate emitter injection
- Order of migration (which event domain first)
- Whether command registration moves into wiring.rs or a separate helper function
- Exact Cargo feature name and gating pattern

</decisions>

<canonical_refs>

## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Requirements and phase definition

- `.planning/REQUIREMENTS.md` — RNTM-02 defines the success criteria for this phase
- `.planning/ROADMAP.md` — Phase 37 success criteria (4 items)

### Phase 36 context (predecessor decisions)

- `.planning/phases/36-event-emitter-abstraction/36-CONTEXT.md` — HostEventEmitterPort design, event model identity, TauriEventEmitter pattern, migration strategy, commit split approach
- Key decisions carried forward: HostEvent is core semantic model (not DTO), TauriEventEmitter owns payload conversion, best-effort emit (warn + continue)

### Primary code targets

- `src-tauri/crates/uc-tauri/src/bootstrap/wiring.rs` — 6328-line file to be split; contains all 14 remaining app.emit() calls
- `src-tauri/crates/uc-tauri/src/bootstrap/file_transfer_wiring.rs` — 5 functions with AppHandle<R> to migrate
- `src-tauri/crates/uc-tauri/src/bootstrap/mod.rs` — Module declarations and re-exports (needs updating)
- `src-tauri/src/main.rs` — Lines 852-927: invoke_handler![...] command registration to be moved into Tauri module
- `src-tauri/crates/uc-tauri/Cargo.toml` — Feature gate (`tauri-runtime`) to be added for assembly purity verification

### Existing HostEvent implementation (from Phase 36)

- `src-tauri/crates/uc-core/src/ports/host_event_emitter.rs` — HostEvent enum, sub-enums (ClipboardHostEvent, PeerDiscoveryHostEvent, PeerConnectionHostEvent, TransferHostEvent), EmitError
- `src-tauri/crates/uc-tauri/src/adapters/tauri_event_emitter.rs` — TauriEventEmitter with event name mapping and payload DTOs

### Current event types (to be replaced by HostEvent variants)

- `src-tauri/crates/uc-tauri/src/events/mod.rs` — SettingChangedEvent, forward\_\* functions
- `src-tauri/crates/uc-tauri/src/events/p2p_pairing.rs` — P2PPairingVerificationEvent and related types
- `src-tauri/crates/uc-tauri/src/events/p2p_peer.rs` — P2P peer event types (likely already migrated in Phase 36)

### Crate-level rules

- `src-tauri/crates/uc-core/AGENTS.md` — No Tauri/system imports, port conventions
- `src-tauri/crates/uc-tauri/AGENTS.md` — Bootstrap editing rules, event payload conventions, camelCase mandate
- `AGENTS.md` — Atomic commit rules, hex boundary, revert safety

</canonical_refs>

<code_context>

## Existing Code Insights

### Reusable Assets

- `HostEventEmitterPort` trait + `HostEvent` enum: Already defined in uc-core/ports from Phase 36 — extend with new domain sub-enums
- `TauriEventEmitter`: Already has pattern for event name mapping + payload DTO conversion — extend with new match arms
- `LoggingEventEmitter`: Already handles all existing HostEvent variants — extend with new variants
- `RecordingEmitter` (test helper in file_transfer_wiring.rs): Reusable test pattern for verifying emitted events
- Frontend contract test pattern from Phase 36: Assert exact event name string + camelCase payload fields

### Established Patterns

- Port injection via `Arc<dyn HostEventEmitterPort>` through closures — used throughout wiring.rs event loops
- `emit_pending_status` in file_transfer_wiring.rs: Already migrated in Phase 36, serves as reference pattern for the remaining 5 functions
- Best-effort emit convention: `if let Err(err) = emitter.emit(...) { warn!(...) }` — consistent across all migrated sites

### Integration Points

- wiring.rs line 1166-1170: `app_handle.clone()` captured in multiple closures — these clones become `event_emitter.clone()` after migration
- wiring.rs `start_background_tasks` signature: `AppHandle<R>` parameter removed after all emits migrated; `R: Runtime` generic parameter also removed
- mod.rs: Needs `pub mod assembly;` declaration and `pub use assembly::*` re-export
- AppRuntime (runtime.rs): May still hold app_handle for commands-layer (out of scope), but event loop code no longer needs it
- main.rs:852-927: `invoke_handler![...]` macro moved into Tauri module; main.rs calls a function that returns the handler
- `resolve_pairing_device_name` callers outside event loops: commands/settings.rs:4,140 and adapters/lifecycle.rs:18,148 — these import from bootstrap::assembly after the move
- uc-tauri/Cargo.toml: `tauri-runtime` feature gate added; `cargo check -p uc-tauri --no-default-features` must pass

</code_context>

<specifics>
## Specific Ideas

No specific requirements — open to standard approaches

</specifics>

<deferred>
## Deferred Ideas

- Move assembly.rs to independent crate (uc-bootstrap or similar) — Phase 40
- Move file_transfer_wiring.rs out of uc-tauri — Phase 38+ when CoreRuntime is extracted
- Migrate commands-layer emits (pairing.rs, clipboard.rs, encryption.rs, tray.rs) — future phase
- Migrate emit_to for quick-panel/preview-panel (window-targeted, requires different abstraction) — future phase
- Split wiring.rs further by domain (clipboard_loop.rs, pairing_loop.rs, etc.) — optional future cleanup

</deferred>

---

_Phase: 37-wiring-decomposition_
_Context gathered: 2026-03-17_
