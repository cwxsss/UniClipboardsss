# Phase 40: uc-bootstrap Crate - Context

**Gathered:** 2026-03-18
**Status:** Ready for planning
**Mode:** Auto-resolved (all recommended defaults)

<domain>
## Phase Boundary

Create `uc-bootstrap` as the sole composition root crate. Move Tauri-free assembly, config resolution, and logging init from `uc-tauri/bootstrap/` into `uc-bootstrap`. Provide scene-specific builders (`build_gui_app`, `build_cli_context`, `build_daemon_app`) so all entry points depend on uc-bootstrap instead of wiring uc-infra and uc-platform directly. After this phase, uc-tauri no longer directly depends on uc-infra or uc-platform for composition — only for Tauri-specific adapters.

</domain>

<decisions>
## Implementation Decisions

### Crate boundary — what moves to uc-bootstrap

- **Moves:** `assembly.rs` (42K, zero tauri imports), `config_resolution.rs` (7K, pure), `config.rs` (6K, config loading), `init.rs` (4.8K, ensure_default_device_name), tracing init delegation
- **Stays in uc-tauri:** `runtime.rs` (AppRuntime, Tauri-specific), `wiring.rs` (Tauri event loops), `file_transfer_wiring.rs` (Tauri event loop), `run.rs` (Tauri builder), `logging.rs` (Tauri-specific log plugin), `clipboard_integration_mode.rs` (used by runtime.rs)
- `task_registry.rs` is a 128B re-export stub (already moved to uc-app in Phase 38) — stays or is deleted
- `WiredDependencies` struct moves with assembly.rs
- `BackgroundRuntimeDeps` stays in uc-tauri/wiring.rs (only used by start_background_tasks which is Tauri-specific)

### Builder API surface — scene-specific constructors

- `build_gui_app()` — equivalent to current `wire_dependencies()`, returns WiredDependencies for Tauri runtime. Called from main.rs via uc-tauri
- `build_cli_context()` — returns CLI-ready dependency set (CoreRuntime + UseCases) WITHOUT starting background workers. No event emitter swap needed (uses LoggingEventEmitter permanently)
- `build_daemon_app()` — returns daemon-ready dependencies WITH worker handles. Uses LoggingEventEmitter (no Tauri). Workers are registered but not started (caller starts them)
- All three builders share the core assembly path internally (wire_dependencies or a shared `build_core(config) -> CoreRuntime` helper)
- UseCases accessor instantiated inside uc-bootstrap and shared — no entry point constructs its own (RNTM-04)

### Logging unification (BOOT-05)

- `init_tracing_subscriber()` implementation stays in `uc-observability` (already there)
- uc-bootstrap re-exports it and calls it exactly once inside each builder function
- main.rs removes its direct call to init_tracing_subscriber — uc-bootstrap handles it
- Each builder can pass a logging profile (Dev/Prod/DebugClipboard) to customize output

### Dependency graph restructuring (BOOT-04)

- uc-bootstrap depends on: uc-core, uc-app, uc-infra, uc-platform, uc-observability
- uc-tauri depends on: uc-bootstrap (for composition), uc-core (for types), uc-app (for CoreRuntime/UseCases types)
- uc-tauri REMOVES direct dependency on uc-infra and uc-platform for composition purposes
- uc-tauri MAY retain uc-platform dependency if TauriEventEmitter or other Tauri adapters need platform types — evaluate during planning
- main.rs (uniclipboard binary) depends on uc-tauri (which transitively gets uc-bootstrap)
- Future uc-daemon and uc-cli (Phase 41) depend on uc-bootstrap directly, NOT uc-tauri

### uc-tauri bootstrap/ module after extraction

- `mod.rs` re-exports from uc-bootstrap for backward compatibility during transition (main.rs import paths may change)
- Remaining bootstrap/ files: runtime.rs, wiring.rs, file_transfer_wiring.rs, run.rs, logging.rs, clipboard_integration_mode.rs
- assembly.rs replaced by re-export: `pub use uc_bootstrap::assembly;` (or imports updated directly)

### Claude's Discretion

- Internal module organization within uc-bootstrap (single lib.rs vs sub-modules)
- Whether to use `pub use` re-exports in uc-tauri for migration or update all import paths at once
- Exact Cargo.toml feature flags (if any) for uc-bootstrap
- Whether build_gui_app is a distinct function or just wire_dependencies renamed

</decisions>

<canonical_refs>

## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Requirements

- `.planning/REQUIREMENTS.md` — BOOT-01 through BOOT-05 and RNTM-04 define success criteria

### Phase context (prior decisions)

- `.planning/phases/37-wiring-decomposition/37-CONTEXT.md` — assembly.rs creation, zero-tauri-import guarantee, SC#4 staged approach
- `.planning/phases/38-coreruntime-extraction/38-CONTEXT.md` — CoreRuntime in uc-app, UseCases split (CoreUseCases/AppUseCases), TaskRegistry migration
- `.planning/phases/39-config-resolution-extraction/39-CONTEXT.md` — config_resolution.rs location rationale, resolve_app_config() API, SC#1 partial satisfaction note

### Roadmap

- `.planning/ROADMAP.md` — Phase 40 success criteria (6 items), Phase 41 dependency on Phase 40

### Codebase entry points

- `src-tauri/crates/uc-tauri/src/bootstrap/mod.rs` — Current re-exports that define the public API surface being extracted
- `src-tauri/crates/uc-tauri/src/bootstrap/assembly.rs` — Primary extraction target (42K, zero tauri imports)
- `src-tauri/crates/uc-tauri/src/bootstrap/config_resolution.rs` — Config resolution extraction target
- `src-tauri/crates/uc-tauri/Cargo.toml` — Current dependency graph to restructure
- `src-tauri/src/main.rs` — Entry point that imports from uc-tauri::bootstrap (import paths will change)

</canonical_refs>

<code_context>

## Existing Code Insights

### Reusable Assets

- `assembly.rs` — Already Tauri-free composition root with `wire_dependencies()`, `build_setup_orchestrator()`, `get_storage_paths()`, etc. This is the primary extraction target.
- `config_resolution.rs` — Pure `resolve_app_config()` with no Tauri deps. Ready to move as-is.
- `uc-observability` crate — Already owns `init_tracing_subscriber()` implementation. uc-bootstrap just needs to re-export and call it.
- `CoreRuntime` (in uc-app) — Tauri-free runtime core created in Phase 38. uc-bootstrap builders will construct this.
- `CoreUseCases` (in uc-app) — Pure domain use case accessors. Shared across all entry points.

### Established Patterns

- Crate-based modularization: each concern in its own crate under `src-tauri/crates/`
- Port/adapter pattern: composition happens in assembly, traits in uc-core/ports
- Re-export pattern: `pub use` in mod.rs for backward compatibility (used in Phase 38 for TaskRegistry)
- Workspace Cargo.toml at `src-tauri/Cargo.toml` defines workspace members

### Integration Points

- `src-tauri/Cargo.toml` workspace members array — must add `crates/uc-bootstrap`
- `src-tauri/crates/uc-tauri/Cargo.toml` — must add uc-bootstrap dep, evaluate removing uc-infra/uc-platform
- `src-tauri/src/main.rs` — import paths change from `uc_tauri::bootstrap::*` to either `uc_bootstrap::*` or uc-tauri re-exports
- `uc-tauri/bootstrap/mod.rs` — re-exports must be updated to delegate to uc-bootstrap

</code_context>

<specifics>
## Specific Ideas

No specific requirements — open to standard approaches. The phase is a straightforward crate extraction following the patterns established in Phases 37-39.

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---

_Phase: 40-uc-bootstrap-crate_
_Context gathered: 2026-03-18_
