# Phase 38: CoreRuntime Extraction - Context

**Gathered:** 2026-03-18
**Status:** Ready for planning

<domain>
## Phase Boundary

Extract a Tauri-free `CoreRuntime` struct from `AppRuntime`. CoreRuntime holds AppDeps, shared orchestrators (SetupOrchestrator), event emitter, task registry, and other non-Tauri state. AppRuntime becomes a thin wrapper adding only Tauri-specific handles (app_handle, watcher_control). SetupOrchestrator assembly is unified into the main composition root (assembly.rs). UseCases accessor moves to uc-app alongside CoreRuntime.

**In scope:**

- Define CoreRuntime struct in uc-app crate (Tauri-free, passes `cargo check -p uc-app`)
- Move fields from AppRuntime to CoreRuntime: deps (AppDeps), event_emitter, lifecycle_status, setup_orchestrator, clipboard_integration_mode, task_registry, storage_paths
- AppRuntime wraps CoreRuntime + adds: app_handle, watcher_control (platform-layer)
- Move SetupOrchestrator assembly from runtime.rs `build_setup_orchestrator` to assembly.rs as standalone function
- Abstract TauriSessionReadyEmitter as a port trait (SessionReadyEmitterPort or similar) in uc-core/uc-app
- Move non-Tauri adapters (DeviceNameAnnouncer, LoggingLifecycleEventEmitter, InMemoryLifecycleStatus) from uc-tauri/adapters to uc-app or uc-infra
- Move UseCases struct and all accessor methods to uc-app, bound to CoreRuntime
- Move facade accessors (device_id, is_encryption_ready, settings_port, etc.) to CoreRuntime
- AppRuntime proxies usecases() and facade calls to inner CoreRuntime (transparent to command layer)
- Existing GUI setup flow continues to work end-to-end (SC#4)

**Out of scope:**

- Creating new workspace crate (no uc-runtime or uc-bootstrap yet — Phase 40)
- Moving assembly.rs out of uc-tauri (Phase 40)
- Modifying command layer code (transparent proxy keeps `runtime.usecases().xxx()` unchanged)
- Daemon/CLI entry points (Phase 41)

</domain>

<decisions>
## Implementation Decisions

### CoreRuntime struct boundary (maximum extraction)

- CoreRuntime holds: AppDeps, event_emitter (RwLock<Arc<dyn HostEventEmitterPort>>), lifecycle_status, setup_orchestrator, clipboard_integration_mode, task_registry, storage_paths
- AppRuntime holds: CoreRuntime + app_handle (Arc<RwLock<Option<AppHandle>>>) + watcher_control (Arc<dyn WatcherControlPort>)
- CoreRuntime lives in uc-app crate — natural home alongside AppDeps and SetupOrchestrator
- Dependency is clean: uc-app already depends on uc-core; CoreRuntime uses only uc-core traits and uc-app types

### Mutability and event emitter

- Arc<CoreRuntime> shared, interior mutability via RwLock (same pattern as current AppRuntime)
- RwLock swap for event_emitter preserved — Tauri mode swaps LoggingEmitter to TauriEmitter at setup time
- daemon/CLI construct CoreRuntime with final emitter, never swap

### SetupOrchestrator assembly location

- Moved from AppRuntime::build_setup_orchestrator (runtime.rs) to assembly.rs as standalone function
- assembly.rs only orchestrates — does NOT create adapters
- All adapters passed in as Arc<dyn Port> parameters (parameterized injection)
- New SetupAssemblyPorts struct bundles all required ports (replaces SetupRuntimePorts which is deprecated/merged)

### Adapter disposition

**Already ports (no change needed):**

- InitializeEncryption, MarkSetupComplete, StartNetworkAfterUnlock, DefaultSpaceAccessCryptoFactory, SpaceAccessNetworkAdapter, HmacProofAdapter, SpaceAccessPersistenceAdapter (all uc-app)
- StartClipboardWatcher (uc-platform, implements StartClipboardWatcherPort)
- Timer (uc-infra, implements TimerPort)
- HostEventSetupPort (wiring.rs, wraps HostEventEmitterPort)

**Need new port abstraction:**

- TauriSessionReadyEmitter → new SessionReadyEmitterPort (or similar) in uc-core/uc-app. Tauri adapter stays in uc-tauri, passed as Arc<dyn Port>

**Move to uc-app/uc-infra (not Tauri-dependent):**

- DeviceNameAnnouncer → move to uc-app (uses PeerDirectoryPort + SettingsPort, both uc-core traits)
- LoggingLifecycleEventEmitter → move to uc-app or uc-infra
- InMemoryLifecycleStatus → move to uc-app (implements LifecycleStatusPort)

### UseCases accessor migration

- UseCases struct and all ~40+ accessor methods move to uc-app, bound to CoreRuntime
- Accessors that referenced uc-tauri adapters are abstracted: accept Arc<dyn Port> injected at CoreRuntime construction
- AppRuntime.usecases() transparently proxies to self.core.usecases() — command layer code unchanged
- Facade accessors (device_id, is_encryption_ready, settings_port, etc.) also move to CoreRuntime with AppRuntime proxy

### Testing strategy

- CoreRuntime unit tests in uc-app using existing noop port implementations (from v0.2.0 test infrastructure)
- Add noop/logging event emitter for tests as needed
- Existing runtime.rs tests split: pure domain tests migrate to uc-app, Tauri-dependent tests stay in uc-tauri
- Both sides must compile and pass independently

### Claude's Discretion

- Exact CoreRuntime field ordering and constructor signature
- Whether SetupAssemblyPorts lives in assembly.rs or a dedicated module
- Specific port trait name for TauriSessionReadyEmitter abstraction
- Which adapters go to uc-app vs uc-infra (based on dependency analysis)
- Commit split granularity (minimum: uc-core port traits, uc-app CoreRuntime+UseCases, uc-tauri AppRuntime wrapper, assembly.rs setup migration)
- Internal implementation of AppRuntime proxy methods

</decisions>

<canonical_refs>

## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Requirements and phase definition

- `.planning/REQUIREMENTS.md` — RNTM-01 (CoreRuntime without Tauri dependency) and RNTM-05 (SetupOrchestrator unified assembly)
- `.planning/ROADMAP.md` — Phase 38 success criteria (4 items): CoreRuntime in Tauri-free crate, AppRuntime as thin wrapper, SetupOrchestrator single assembly point, GUI flow unchanged

### Phase 36 and 37 context (predecessor decisions)

- `.planning/phases/36-event-emitter-abstraction/36-CONTEXT.md` — HostEventEmitterPort design, RwLock swap pattern, app_handle coexistence
- `.planning/phases/37-wiring-decomposition/37-CONTEXT.md` — assembly.rs creation, wiring.rs split, AppHandle removal from start_background_tasks

### Primary code targets

- `src-tauri/crates/uc-tauri/src/bootstrap/runtime.rs` — AppRuntime struct (line 100-128), build_setup_orchestrator (lines 353-441), UseCases struct and accessors, facade methods
- `src-tauri/crates/uc-tauri/src/bootstrap/assembly.rs` — Pure Rust assembly module (wire_dependencies, etc.)
- `src-tauri/crates/uc-app/src/deps.rs` — AppDeps struct definition (line 108-127)
- `src-tauri/crates/uc-app/src/usecases/setup/orchestrator.rs` — SetupOrchestrator constructor signature

### Adapters to move or abstract

- `src-tauri/crates/uc-tauri/src/adapters/lifecycle.rs` — TauriSessionReadyEmitter (needs port), DeviceNameAnnouncer (move), LoggingLifecycleEventEmitter (move), InMemoryLifecycleStatus (move)
- `src-tauri/crates/uc-tauri/src/bootstrap/wiring.rs` — HostEventSetupPort (wraps HostEventEmitterPort for setup events)

### Crate-level rules

- `src-tauri/crates/uc-core/AGENTS.md` — No Tauri/system imports, port conventions
- `src-tauri/crates/uc-tauri/AGENTS.md` — Bootstrap editing rules
- `AGENTS.md` — Atomic commit rules, hex boundary

</canonical_refs>

<code_context>

## Existing Code Insights

### Reusable Assets

- `assembly.rs`: Already a pure Rust module with zero tauri imports — natural home for SetupOrchestrator assembly function
- `AppDeps` (uc-app/src/deps.rs): Clean dependency bundle with 6 port groups, already in uc-app
- Existing noop port implementations in uc-app test modules — reusable for CoreRuntime unit tests
- `SetupRuntimePorts` struct (runtime.rs:131-135) — to be merged into new SetupAssemblyPorts

### Established Patterns

- Port injection via `Arc<dyn XxxPort>` through constructors — CoreRuntime follows same pattern
- Interior mutability via RwLock for late-binding (event_emitter swap) — CoreRuntime preserves this
- UseCases factory accessor pattern (`runtime.usecases().xxx()`) — preserved via transparent proxy
- Facade accessors for simple read-only state — preserved via proxy

### Integration Points

- AppRuntime wraps CoreRuntime: `core: Arc<CoreRuntime>` field, all proxy methods delegate
- assembly.rs gains `build_setup_orchestrator(deps: &AppDeps, ports: SetupAssemblyPorts, ...) -> Arc<SetupOrchestrator>`
- Command layer unchanged: `runtime.usecases().xxx()` still works (AppRuntime proxies)
- wiring.rs: Creates Tauri-specific adapters (TauriSessionReadyEmitter) and passes to assembly.rs

</code_context>

<specifics>
## Specific Ideas

No specific requirements — open to standard approaches

</specifics>

<deferred>
## Deferred Ideas

- Move assembly.rs to uc-bootstrap crate with real `cargo check -p` independence — Phase 40
- Create uc-bootstrap as sole composition root — Phase 40
- Daemon/CLI entry points using CoreRuntime directly — Phase 41
- Move remaining Tauri adapters out of uc-tauri — future phases
- Split UseCases into domain-specific accessor groups if it grows too large — future cleanup

</deferred>

---

_Phase: 38-coreruntime-extraction_
_Context gathered: 2026-03-18_
