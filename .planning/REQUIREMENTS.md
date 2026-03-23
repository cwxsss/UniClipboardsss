# Requirements: UniClipboard Desktop

**Defined:** 2026-03-17
**Core Value:** Seamless clipboard synchronization across devices — copy on one, paste on another

## v0.4.0 Requirements

Requirements for runtime mode separation. Each maps to roadmap phases.

### Event Abstraction

- [x] **EVNT-01**: System can deliver host events through an abstract HostEventEmitterPort trait in uc-core/ports
- [x] **EVNT-02**: GUI mode can emit events to Tauri frontend via TauriEventEmitter adapter implementing HostEventEmitterPort
- [x] **EVNT-03**: Non-GUI modes can deliver events to logging output via LoggingEventEmitter adapter
- [x] **EVNT-04**: Background tasks (clipboard watcher, peer discovery, sync scheduler) accept HostEventEmitterPort instead of AppHandle<R: Runtime>

### Runtime Extraction

- [x] **RNTM-01**: CoreRuntime struct exists without Tauri dependency, holding AppDeps and shared orchestrators
- [x] **RNTM-02**: wiring.rs is decomposed into pure assembly module (Tauri-free) and Tauri-specific event loop module
- [x] **RNTM-03**: Configuration resolution functions (path resolve, profile suffix, keyslot dir) extracted from main.rs to reusable module
- [x] **RNTM-04**: UseCases accessor is shared across all entry points (not duplicated per runtime mode)
- [x] **RNTM-05**: SetupOrchestrator assembly unified into main composition root (no secondary wiring in runtime.rs)

### Bootstrap Crate

- [x] **BOOT-01**: uc-bootstrap crate exists as sole composition root, depending on uc-core + uc-app + uc-infra + uc-platform
- [x] **BOOT-02**: uc-bootstrap provides build_cli_context() returning CLI-ready dependencies
- [x] **BOOT-03**: uc-bootstrap provides build_daemon_app() returning daemon-ready dependencies with workers
- [ ] **BOOT-04**: uc-tauri depends on uc-bootstrap instead of directly on uc-infra + uc-platform
- [x] **BOOT-05**: Logging initialization unified in uc-bootstrap (not duplicated per entry point)

### Daemon Skeleton

- [x] **DAEM-01**: uc-daemon crate exists with DaemonApp struct supporting startup and graceful shutdown
- [x] **DAEM-02**: Daemon exposes local RPC server with ping and status commands
- [x] **DAEM-03**: Daemon has DaemonWorker trait abstraction with placeholder workers (clipboard watcher, peer discovery)
- [x] **DAEM-04**: Daemon maintains RuntimeState with uptime, worker health, and connected peers summary

### CLI Skeleton

- [x] **CLI-01**: uc-cli crate exists with clap-based argument parsing and subcommand routing
- [x] **CLI-02**: CLI supports daemon status command via RPC connection to daemon
- [x] **CLI-03**: CLI supports direct app commands (space status, device list) via uc-bootstrap
- [x] **CLI-04**: CLI supports --json output mode for machine-consumable output
- [x] **CLI-05**: CLI uses stable exit codes (0=success, 1=error, 5=daemon unreachable)

## Future Requirements

### Daemon Full Implementation

- **DAEM-F01**: Daemon clipboard watcher worker with full event loop
- **DAEM-F02**: Daemon peer discovery worker with libp2p integration
- **DAEM-F03**: Daemon sync scheduler worker
- **DAEM-F04**: Daemon transfer manager worker
- **DAEM-F05**: Daemon config hot-reload

### CLI Full Implementation

- **CLI-F01**: CLI clipboard history list/show commands
- **CLI-F02**: CLI sync trigger command
- **CLI-F03**: CLI transfer list/show commands
- **CLI-F04**: CLI daemon start/stop commands with process management

### Daemon API Foundation

- [x] **PH45-01**: Daemon auth token and local connection metadata are persisted outside frontend storage and reusable by local daemon clients
- [x] **PH45-02**: Daemon exposes read-only transport DTOs and query helpers for status, peers, paired devices, and pairing-session summaries without leaking secrets
- [x] **PH45-05**: CLI status and paired-device reads use a shared daemon HTTP client and preserve stable exit semantics for unreachable daemon scenarios
- [x] **PH45-06**: Tauri shell can probe or start the daemon, keep connection info in memory, and emit runtime-only connection metadata without persisting the bearer token in frontend storage

### Daemon Pairing Host Migration

- [x] **PH46-01**: daemon owns pairing orchestrator/session lifecycle
- [x] **PH46-01A**: discoverability and participant readiness are separately controlled
- [x] **PH46-01B**: headless/CLI daemon remains non-discoverable until explicit opt-in
- [x] **PH46-02**: pairing action execution/event handling run daemon-side and survive Tauri/webview disconnects
- [x] **PH46-03**: daemon exposes pairing mutation surface (initiate/accept/reject/cancel/verify) with sessionId follow-up semantics
- [x] **PH46-03A**: discoverability/readiness mutation APIs support explicit lease semantics
- [x] **PH46-04**: read models remain metadata-only; verification secrets remain authenticated realtime-only
- [x] **PH46-05**: Tauri remains a compatibility bridge that forwards daemon pairing/peer updates into existing frontend event contract
- [x] **PH46-05A**: GUI-hosted daemon is discoverable by default while readiness stays flow-scoped
- [x] **PH46-06**: regression tests validate daemon continuity and setup compatibility

### Unified Realtime Subscriptions On Single DaemonWsBridge

- [ ] **PH461-01**: one authenticated daemon websocket exists per Tauri runtime and is owned only by DaemonWsBridge
- [x] **PH461-02**: uc-core exposes RealtimeTopicPort and typed RealtimeEvent/RealtimeFrontendEvent contracts without daemon transport DTO dependencies
- [ ] **PH461-03**: uc-app owns topic consumers for pairing, peers, and setup without consumer-managed websocket lifecycle
- [ ] **PH461-04**: frontend cuts over in one pass to a single daemon://realtime contract with camelCase payloads and stable ordering
- [ ] **PH461-05**: reconnect, resubscribe, and bounded backpressure behavior are centralized and regression-tested
- [ ] **PH461-06**: legacy PairingBridge, setup websocket subscription path, and p2p-\* realtime listeners/emissions are deleted without compatibility flags

### Daemon Pairing Hard Cutover

- [x] **R46.2-1**: all pairing business logic and session lifecycle execute on the daemon, not in `uc-tauri`
- [x] **R46.2-2**: any Tauri-owned pairing hosts or loops are removed or neutralized while GUI flow semantics stay unchanged
- [x] **R46.2-3**: the user-visible pairing flow remains behaviorally equivalent as `request -> verification -> verifying -> complete/failed`
- [x] **R46.2-4**: GUI pairing entrypoints (`PairingDialog`, passive accept/reject, setup flows) route through daemon APIs and realtime topics
- [x] **R46.2-5**: metadata and sensitivity boundaries from Phases 46/46.1 are preserved, with verification data only on authenticated realtime and not in generic read models
- [x] **R46.2-6**: admission, concurrency, and participant-readiness rules remain daemon-owned, including single-session and no headless auto-pairing behavior
- [x] **R46.2-7**: `DaemonWsBridge` remains the single realtime path for pairing topics
- [x] **R46.2-8**: regression coverage proves the daemon-based pairing flow is feature-complete versus the legacy Tauri host across API, websocket, setup, and GUI flows

### GUI Daemon Startup Compatibility And Recovery

- [ ] **GUI-DMN-01**: GUI startup must classify the expected local UniClipboard daemon endpoint as absent, compatible, or incompatible before deciding to continue or replace it
- [ ] **GUI-DMN-02**: when the expected local daemon is incompatible, GUI may terminate that local daemon once and replace it with the current bundled daemon, with bounded retry and deterministic failure
- [ ] **GUI-DMN-03**: GUI must not silently enter the main interface unless daemon bootstrap reaches compatible-ready; failed bootstrap must land in a dedicated startup failure state
- [ ] **GUI-DMN-04**: startup failure must expose both bounded automatic recovery polling and a user-triggered retry path, and automatically resume normal startup once daemon becomes compatible-ready
- [ ] **GUI-DMN-05**: daemon `/health` and `/status` must expose shared `packageVersion` and `apiRevision` metadata that come from the same release contract as the GUI bundle

### GUI-Owned Daemon Exit Lifecycle

- [x] **P46.6-01**: GUI bootstrap records a live daemon owner contract only when the current GUI process actually spawns the daemon; connecting to an already-compatible daemon must not register exit ownership
- [x] **P46.6-02**: on real application exit, GUI must boundedly terminate the GUI-owned daemon exactly once and allow process exit only after cleanup settles
- [x] **P46.6-03**: main-window `CloseRequested` must continue to hide to tray and must never trigger daemon cleanup
- [x] **P46.6-04**: a daemon spawned after incompatible replacement is treated as GUI-owned for later exit cleanup, while independently started daemons remain non-owned
- [x] **P46.6-05**: regression coverage must prove spawned-owned, replacement-owned, compatible-existing, and exit-idempotency paths using exact `cargo test --test ...` commands

### Daemon Encryption State Recovery

- [ ] **PH50-01**: DaemonApp::run() calls AutoUnlockEncryptionSession before starting workers so encryption session is available immediately after daemon startup
- [ ] **PH50-02**: When EncryptionState is Uninitialized (first run), daemon starts normally without attempting recovery
- [ ] **PH50-03**: When EncryptionState is Initialized but recovery fails (keyslot corrupt, KEK missing, unwrap failure), daemon refuses to start with a descriptive error

## Out of Scope

| Feature                                | Reason                                                            |
| -------------------------------------- | ----------------------------------------------------------------- |
| Cross-machine remote daemon control    | v0.4.0 is local single-machine only                               |
| Stable public RPC API for third-party  | Internal protocol only, can change                                |
| Plugin system                          | Not needed for runtime mode separation                            |
| uc-tauri full decoupling from uc-infra | Incremental — uc-tauri keeps uc-infra as dev-dependency for tests |
| GUI feature changes                    | Pure architecture milestone, no UI changes                        |

## Traceability

| Requirement | Phase | Status   |
| ----------- | ----- | -------- |
| EVNT-01     | 36    | Complete |
| EVNT-02     | 36    | Complete |
| EVNT-03     | 36    | Complete |
| EVNT-04     | 36    | Complete |
| RNTM-02     | 37    | Complete |
| RNTM-01     | 38    | Complete |
| RNTM-05     | 38    | Complete |
| RNTM-03     | 39    | Complete |
| BOOT-01     | 40    | Complete |
| BOOT-02     | 40    | Complete |
| BOOT-03     | 40    | Complete |
| BOOT-04     | 40    | Pending  |
| BOOT-05     | 40    | Complete |
| RNTM-04     | 40    | Complete |
| DAEM-01     | 41    | Complete |
| DAEM-02     | 41    | Complete |
| DAEM-03     | 41    | Complete |
| DAEM-04     | 41    | Complete |
| CLI-01      | 41    | Complete |
| CLI-02      | 41    | Complete |
| CLI-03      | 41    | Complete |
| CLI-04      | 41    | Complete |
| CLI-05      | 41    | Complete |
| PH45-01     | 45    | Complete |
| PH45-02     | 45    | Complete |
| PH45-05     | 45    | Complete |
| PH45-06     | 45    | Complete |
| PH46-01     | 46    | Complete |
| PH46-01A    | 46    | Complete |
| PH46-01B    | 46    | Complete |
| PH46-02     | 46    | Complete |
| PH46-03     | 46    | Complete |
| PH46-03A    | 46    | Complete |
| PH46-04     | 46    | Complete |
| PH46-05     | 46    | Complete |
| PH46-05A    | 46    | Complete |
| PH46-06     | 46    | Complete |
| PH461-01    | 46.1  | Pending  |
| PH461-02    | 46.1  | Complete |
| PH461-03    | 46.1  | Pending  |
| PH461-04    | 46.1  | Pending  |
| PH461-05    | 46.1  | Pending  |
| PH461-06    | 46.1  | Pending  |
| R46.2-1     | 46.2  | Complete |
| R46.2-2     | 46.2  | Complete |
| R46.2-3     | 46.2  | Complete |
| R46.2-4     | 46.2  | Complete |
| R46.2-5     | 46.2  | Complete |
| R46.2-6     | 46.2  | Complete |
| R46.2-7     | 46.2  | Complete |
| R46.2-8     | 46.2  | Complete |
| GUI-DMN-01  | 46.3  | Pending  |
| GUI-DMN-02  | 46.3  | Pending  |
| GUI-DMN-03  | 46.3  | Pending  |
| GUI-DMN-04  | 46.3  | Pending  |
| GUI-DMN-05  | 46.3  | Pending  |
| P46.6-01    | 46.6  | Complete |
| P46.6-02    | 46.6  | Complete |
| P46.6-03    | 46.6  | Complete |
| P46.6-04    | 46.6  | Complete |
| P46.6-05    | 46.6  | Complete |
| PH50-01     | 50    | Pending  |
| PH50-02     | 50    | Pending  |
| PH50-03     | 50    | Pending  |

**Coverage:**

- v0.4.0 requirements: 44 total
- Mapped to phases: 44
- Unmapped: 0

---

_Requirements defined: 2026-03-17_
_Last updated: 2026-03-23 after Phase 50 planning_
