# UniClipboard Architecture Reform — Final Unified Plan

**Author**: Chief Architect (Moderator)
**Date**: 2026-03-06
**Process**: 5-specialist cross-review with 3-phase debate (10 agent outputs synthesized)
**Status**: FINAL — ready for phased execution

---

## 1. Executive Summary

After four stages of parallel analysis and structured cross-debate, the Architecture Reform Committee has converged on a unified plan to remediate 38 confirmed architectural violations across 5 root-cause clusters. This document is the single authoritative source of truth: it supersedes all individual Phase 2 and Phase 3 reports, resolves every identified conflict, and rejects proposals that fail architectural soundness.

**The core diagnosis**: The hexagonal migration was executed bottom-up (crate structure first, dependency rules second). Boundaries exist nominally but are not enforced — compilers accept code that violates architectural intent. The result is a codebase where the architectural map is aspirational but the territory is not.

**The core prescription**: Restore compiler-enforced boundaries, kill god objects by pushing responsibility to the right layer, establish typed contracts at every cross-layer boundary, and build test infrastructure that reflects — and constrains — the architecture.

---

## 2. Consolidated Root-Cause Clusters

_After cross-interrogation, the 38 issues reduce to 5 mechanically distinct clusters. Solving the first three clusters automatically reduces the severity of the remaining two._

### Cluster A: Boundary Violation & Layer Penetration ⬛ [Highest Risk]

**Root**: Missing port abstractions forced adapters to call each other; missing visibility enforcement let commands bypass use cases.

| Issue | One-line diagnosis                                                               |
| ----- | -------------------------------------------------------------------------------- |
| H1    | `uc-platform` → `uc-infra` direct call — no transport decoder port               |
| H2    | `AppRuntime.deps` is `pub` — commands reach ports without use cases              |
| M6    | `tokio::mpsc::Receiver` in `uc-core` port signatures — runtime leaks into domain |

### Cluster B: God Objects & Responsibility Bloat ⬛ [Maintainability Risk]

**Root**: No sub-composition mechanism within `uc-app`; flat AppDeps gave every UseCase access to every port.

| Issue | One-line diagnosis                                                          |
| ----- | --------------------------------------------------------------------------- |
| H3    | SetupOrchestrator: 15 deps, 860 lines — state machine + side-effects fused  |
| H4    | SyncInbound: 10 inline steps, embedded sub-UseCase                          |
| H5    | SyncOutbound: tokio runtime detection + wire encoding inside UseCase        |
| M1    | AppDeps: 39-field flat container — ISP violation by design                  |
| M2    | Setup directly holds `pairing_transport` — splits pairing session ownership |
| M3    | ClipboardEntry/ClipboardEvent: no domain behavior                           |

### Cluster C: Missing Type Safety & Contract Isolation ⬛ [API Stability Risk]

**Root**: Domain models used as IPC DTOs; string errors at every boundary; anyhow masks error structure.

| Issue | One-line diagnosis                                                      |
| ----- | ----------------------------------------------------------------------- |
| H10   | `list_paired_devices` returns `Vec<PairedDevice>` directly to frontend  |
| H11   | All command errors serialize as `String` — frontend cannot discriminate |
| M9    | `EncryptionState::Initializing` declared but never produced             |
| M11   | 33 port traits use `anyhow::Result` — typed error information lost      |
| M12   | `NetworkEvent.peer_id` is `String` — bypasses `PeerId` newtype          |
| M13   | `AppConfig.device_name` overlaps `Settings.general.device_name`         |

### Cluster D: Lifecycle & State Management Defects ⬛ [Runtime Risk]

**Root**: No task lifecycle abstraction; no designated ownership of cross-cutting shared state.

| Issue | One-line diagnosis                                                            |
| ----- | ----------------------------------------------------------------------------- |
| H6    | `staged_paired_device_store`: global static `OnceLock<Mutex<HashMap>>`        |
| H7    | 20+ `tokio::spawn` without `CancellationToken` — no graceful shutdown         |
| H8    | `EncryptionSession` implemented twice (infra RwLock vs platform Mutex+expect) |
| H9    | 4x `expect()` in production encryption code                                   |
| M8    | `run_app` ~370 lines monolithic — untestable wiring                           |
| M10   | `PlatformRuntime` has no `Drop` — watcher resources not cleaned up            |

### Cluster E: Testing Infrastructure Debt ⬛ [Velocity Risk]

**Root**: Direct downstream consequence of Clusters A+B. Fixing A+B mechanically reduces E.

| Issue | One-line diagnosis                                                           |
| ----- | ---------------------------------------------------------------------------- |
| H12   | NoopPort mocks duplicated 30+ times (~800 lines) across 6 test files         |
| H13   | AppDeps 39 fields → SetupOrchestrator 14-param constructor → test setup hell |
| M5    | 56 port traits — mock count grows proportionally                             |

---

## 3. Architectural Decisions (with Conflict Resolutions)

The following decisions are FINAL. Alternatives are listed and rejected.

### Decision 1: `uc-core` is Tokio-free

**Ruling**: Remove all `tokio` types from `uc-core` port signatures. Use `futures-core::Stream` (via a `Pin<Box<dyn Stream<Item=T> + Send>>` type alias) as the replacement.

**Resolution of conflicts**:

- **Contract concern** (from phase3-contract-error): The `EventStream<T>` type alias IS object-safe because it returns a boxed trait object. Add explicit contract docs: stream must yield `None` only on permanent source closure, must be cancel-safe when used with `StreamExt::next()`.
- **infra-runtime's `TaskRegistryPort` with `CancellationToken`**: **REJECTED**. TaskRegistryPort does NOT belong in `uc-core`. Place it in `uc-tauri/src/bootstrap/` as a concrete struct. If `uc-app` orchestrators need to spawn tasks, define a minimal `SpawnerPort` in `uc-core` using only runtime-agnostic types: `fn spawn(&self, name: &str, future: Pin<Box<dyn Future<Output=()> + Send>>)`.

**Invariant**: `cargo tree -p uc-core | grep tokio` must return empty.

---

### Decision 2: `UiPort`/`AutostartPort` stay in `uc-core`, but in a sub-module

**Ruling**: Do NOT move these ports to `uc-app`.

**Rejected alternative**: arch-guardian Phase 2 proposed moving to `uc-app`. **Rejected** because `uc-platform` implements `UiPort` and `AutostartPort`, and `uc-platform` must not depend on `uc-app` (would invert the dependency direction). The app-flow Phase 3 review correctly caught this.

**Instead**: Create `uc-core/src/ports/application/` subdirectory and move both traits there. Add doc comment: "These ports represent application-level integration concerns, not core domain concepts. They are co-located in uc-core solely to avoid circular dependencies." This achieves conceptual separation without breaking any dependency rule.

---

### Decision 3: AppDeps decomposition uses concrete bundle structs (not aggregate traits)

**Ruling**: Implement domain-scoped port bundles as concrete structs in `uc-app` (app-flow's approach). Do NOT implement aggregate port traits in `uc-core` (arch-guardian Phase 2 approach).

**Rejected alternative**: Aggregate traits (`ClipboardStoragePorts`) in `uc-core`. **Rejected** because:

1. Accessors returning `&dyn Trait` prevent owned `Arc` extraction for spawned tasks (contract-error Phase 3 catch)
2. Aggregate traits create opaque dependency contracts — callers cannot tell which sub-ports are used
3. Concrete structs are simpler, more Rust-idiomatic, and were independently recommended by arch-guardian in Phase 3 self-correction

**Critical ISP constraint**: Use cases with ≤2 required ports must accept **individual ports** (not bundles) at their constructor. Bundles are for AppDeps storage organization and for use cases genuinely needing 3+ co-injected ports from the same domain. The `UseCases` factory can extract individual ports from bundles.

---

### Decision 4: EncryptionSession consolidation is atomic, no re-export shim

**Ruling**: Migrate in a single PR. Delete `uc-platform/src/adapters/encryption.rs` directly after updating all wiring in `uc-tauri`.

**Rejected alternative**: Temporary `pub use uc_infra::*` re-export in `uc-platform`. **Rejected** because this creates the exact H1 violation (uc-platform → uc-infra) we are trying to eliminate, even if temporarily. One PR, atomic migration.

---

### Decision 5: H11 (CommandError) gates on M11 (typed port errors)

**Ruling**: Implement M11 (typed errors in port traits) BEFORE H11 (structured CommandError). Without typed port errors, `downcast_ref` in command handlers will fail for most errors, making `CommandError` functionally identical to the current `String` approach.

**Sequencing dependency chain**:

```
M5 port grouping (optional but helpful)
  → M11 typed port errors (mandatory prerequisite)
  → H11 CommandError structured enum
```

---

### Decision 6: No new port traits may use `anyhow::Result`

**Ruling**: Effective immediately, all new `trait XxxPort` definitions must specify typed error types. This applies to ALL new ports introduced during this reform: `StreamDecoderPort`, `StagedDeviceStorePort`, `SpawnerPort`, etc.

**Existing ports**: Migrate incrementally per cluster, as specified in the contract-error Phase 2 proposal.

---

### Decision 7: SetupOrchestrator decomposition requires `CancellationToken` from day one

**Ruling**: The `SetupActionExecutor` trait MUST include a `CancellationToken` parameter at initial definition. Do not add it as a retrofit.

```rust
#[async_trait]
pub trait SetupActionExecutor: Send + Sync {
    async fn execute(
        &self,
        context: &mut SetupContext,  // &mut per testability reviewer recommendation
        cancel: CancellationToken,
    ) -> Result<Vec<SetupEvent>, SetupError>;
}
```

The dispatcher derives a child token per execution and cancels on abort. This resolves the infra-runtime Phase 3 concern about orphaned tasks.

---

### Decision 8: SyncInbound decomposition uses serial pipeline model

**Ruling**: `SyncInboundClipboardUseCase` processes messages serially. The deduplicator, decoder, and applier are NOT shared across concurrent invocations. Each invocation owns exclusive access to the pipeline. The orchestrating use case is a single async task that consumes from an `mpsc` channel.

This resolves the race condition concern raised by infra-runtime Phase 3.

---

### Decision 9: `TestAppDepsBuilder` deferred until AppDeps decomposition is complete

**Ruling**: Do NOT build `TestAppDepsBuilder` now. Phase 1 of Cluster E delivers ONLY:

1. `uc-test-support` crate with mock extraction
2. Orchestrator-specific builders (`TestSetupOrchestratorBuilder`) — these target the immediate pain and are less likely to be invalidated

The 39-field `TestAppDepsBuilder` is built only after AppDeps bundle decomposition (Phase 2) is finalized.

---

### Decision 10: `StagedDeviceStorePort` requires full contract specification

**Ruling**: The port (to be defined in `uc-app`) must include:

1. Error return types on `stage()` (overwrite semantics must be explicit)
2. `take_and_promote()` as an atomic operation combining lookup + removal
3. Clarified primary key semantics (session_id vs peer_id)
4. `remove_by_session_id()` for post-setup cleanup

---

## 4. Final Phased Implementation Roadmap

### Phase 0: Immediate Guardrails (Do NOW, before any feature work)

_Goal: Stop the bleeding. No new violations can be introduced._

**P0.1 — CI dependency checks** (2-3 hours)
Add to CI pipeline:

```bash
# Must all return empty
cargo tree -p uc-platform 2>/dev/null | grep uc-infra
cargo tree -p uc-core 2>/dev/null | grep tokio
cargo tree -p uc-app 2>/dev/null | grep uc-infra  # production deps only
```

**P0.2 — Forbid `runtime.deps` access outside `uc-tauri/bootstrap/runtime.rs`**
Add a custom `clippy.toml` rule or a `#[forbid(unreachable_pub)]` annotation strategy. Minimum viable: add a code comment and PR review checklist item until P1.2 lands.

**P0.3 — No new `anyhow::Result` in port signatures** (rule enforcement)
Add to CLAUDE.md and PR template:

> "New port traits MUST use typed error enums. `anyhow::Result` is forbidden in `uc-core/src/ports/` trait signatures."

**P0.4 — No new `expect()` in production code**
Add to clippy configuration: `#![deny(clippy::expect_used)]` in `src-tauri/crates/uc-platform/` and `src-tauri/crates/uc-app/`.

---

### Phase 1: Restore Layer Boundaries

_Goal: Enforce dependency invariants at the compiler level._

**P1.1 — H1: Break uc-platform → uc-infra horizontal dependency**

- Define `StreamDecoderPort` in `uc-core/src/ports/clipboard_transport.rs`:

  ```rust
  #[derive(Debug, thiserror::Error)]
  pub enum StreamDecodeError {
      #[error("wire format corrupted: {0}")] Corruption(String),
      #[error("decryption failed: {0}")] DecryptionFailed(String),
      #[error("decompression failed: {0}")] DecompressionFailed(String),
  }

  #[async_trait]
  pub trait StreamDecoderPort: Send + Sync {
      async fn decode_stream(
          &self,
          data: Vec<u8>,
          master_key: &[u8; 32],
      ) -> Result<Vec<u8>, StreamDecodeError>;
  }
  ```

- Implement `ChunkedStreamDecoderAdapter` in `uc-infra/src/clipboard/transfer/stream_decoder.rs`
- Inject into `Libp2pNetworkAdapter` via constructor in `uc-tauri/bootstrap/wiring.rs`
- Remove `uc-infra` from `uc-platform/Cargo.toml`
- Add CI check: `cargo tree -p uc-platform | grep uc-infra` returns empty

**Files**: `uc-core/ports/clipboard_transport.rs`, `uc-infra/src/clipboard/transfer/stream_decoder.rs`, `uc-platform/src/adapters/libp2p_network.rs`, `uc-platform/Cargo.toml`, `uc-tauri/src/bootstrap/wiring.rs`

---

**P1.2 — H2 Phase A: Make `AppRuntime.deps` inaccessible from commands**

- Change `pub deps: AppDeps` → `deps: AppDeps` (private field)
- Add `pub(crate) fn deps(&self) -> &AppDeps` method in `AppRuntime` (accessible to `UseCases` in same crate)
- Add `pub fn current_device_id(&self) -> String` for legitimate observability access
- Fix all breakage (13+ command call sites) to use `runtime.current_device_id()` or to go through use cases
- Phase B (next sprint): Create `CheckEncryptionReadiness` use case in `uc-app`; eliminate remaining business-logic bypasses

**Files**: `uc-tauri/src/bootstrap/runtime.rs`, `uc-tauri/src/commands/*.rs`

---

**P1.3 — M6: Remove tokio from uc-core port signatures**

- Add `futures-core = "0.3"` to `uc-core/Cargo.toml`
- Define `pub type EventStream<T> = Pin<Box<dyn futures_core::Stream<Item=T> + Send>>;` in `uc-core/src/ports/mod.rs`
- Update `NetworkEventPort::subscribe_events()` and `ClipboardTransportPort::subscribe_clipboard()` to return `Result<EventStream<T>, ...>`
- In `uc-platform` adapters: wrap `mpsc::Receiver` in `tokio_stream::wrappers::ReceiverStream` before boxing
- Update consumers to use `StreamExt::next()` (add `tokio-stream` or `futures-util` dep to consumer crates)
- Remove `tokio` from `uc-core/Cargo.toml` after verifying no other usage
- Contract documentation on `EventStream<T>`: (a) yields `None` only on permanent source closure, (b) cancel-safe with `next()`, (c) fused behavior after `None`

**Files**: `uc-core/Cargo.toml`, `uc-core/src/ports/mod.rs`, `uc-core/src/ports/network_events.rs`, `uc-core/src/ports/clipboard_transport.rs`, `uc-platform/src/adapters/libp2p_network.rs`, all subscribers

---

**P1.4 — H6: Eliminate global static `staged_paired_device_store`**

- Define `StagedDeviceStorePort` in `uc-app/src/ports/staged_device_store.rs`:

  ```rust
  #[derive(Debug, thiserror::Error)]
  pub enum StagedDeviceError {
      #[error("session {0} already staged")] AlreadyStaged(String),
      #[error("device not found for peer {0}")] NotFound(String),
  }

  #[async_trait]
  pub trait StagedDeviceStorePort: Send + Sync {
      async fn stage(&self, session_id: &str, device: PairedDevice)
          -> Result<(), StagedDeviceError>;
      /// Atomic: lookup by peer_id + remove if found
      async fn take_and_promote(&self, peer_id: &str)
          -> Result<PairedDevice, StagedDeviceError>;
      async fn remove_by_session_id(&self, session_id: &str);
  }
  ```

- Implement `InMemoryStagedDeviceStore` using `tokio::sync::RwLock<HashMap<String, (String, PairedDevice)>>` (keyed by session_id, indexed by peer_id)
- Inject into `PairingOrchestrator` and `SpaceAccessPersistenceAdapter` via `uc-tauri` wiring
- Delete `staged_paired_device_store.rs` static module

**Note**: `StagedDeviceStorePort` lives in `uc-app/src/ports/` (NOT `uc-core`) because `SpaceAccessPersistenceAdapter` also lives in `uc-app`. This is architecturally sound as long as the adapter is never moved to `uc-infra` or `uc-platform`.

**Files**: `uc-app/src/ports/staged_device_store.rs` (new), `uc-app/src/usecases/pairing/orchestrator.rs`, `uc-app/src/usecases/setup/space_access_persistence_adapter.rs`, `uc-app/src/usecases/pairing/staged_paired_device_store.rs` (delete), `uc-tauri/src/bootstrap/wiring.rs`

---

**P1.5 — H8 + H9: Consolidate EncryptionSession (atomic PR)**

- Add `#[derive(Clone)]` to `uc-infra/src/security/encryption_session.rs`'s `InMemoryEncryptionSession` (wrapped in `Arc<RwLock<>>` — already there)
- Update `uc-tauri/src/bootstrap/wiring.rs` to construct exactly one `Arc<InMemoryEncryptionSession>` and inject it into all consumers (both `uc-infra` decorators and `uc-platform` adapters)
- Delete `uc-platform/src/adapters/encryption.rs` entirely (eliminates all 4 `expect()` calls)
- Fix `main.rs:845` `.expect()` → `if let Err(e) { error!(...); std::process::exit(1); }`
- **No re-export shim** — atomic migration only

**Files**: `uc-infra/src/security/encryption_session.rs`, `uc-platform/src/adapters/encryption.rs` (delete), `uc-tauri/src/bootstrap/wiring.rs`, `src-tauri/src/main.rs`

---

### Phase 2: Decompose God Objects & Restore Application Layer

_Goal: Each component has a single responsibility; orchestration is separate from execution._

**P2.1 — M4: Relocate non-domain ports within uc-core**

- Create `uc-core/src/ports/application/` subdirectory
- Move `UiPort` → `uc-core/src/ports/application/ui.rs`
- Move `AutostartPort` → `uc-core/src/ports/application/autostart.rs`
- Add module-level doc: "Application-level integration ports. Co-located in uc-core to preserve dependency rules; not core domain concepts."
- All existing imports continue to work via re-export in `uc-core/src/ports/mod.rs`

**Files**: `uc-core/src/ports/mod.rs`, new `uc-core/src/ports/application/` directory

---

**P2.2 — M1: Decompose AppDeps into domain-scoped bundles**

Introduce nested structs in `uc-app/src/deps.rs`. Keep ALL existing individual `Arc<dyn XxxPort>` fields accessible via bundle subfield access during transition (no breaking changes to `UseCases` factory methods until P2.3):

```
AppDeps {
    clipboard: ClipboardPorts,   // 12 fields
    security: SecurityPorts,     // 7 fields
    device: DevicePorts,         // 2 fields
    network: Arc<NetworkPorts>,  // already exists — keep as-is
    network_control: Arc<dyn NetworkControlPort>,
    pairing: PairingPorts,
    setup: SetupPorts,
    storage: StoragePorts,       // blob-related
    settings: Arc<dyn SettingsPort>,
    ui: UiPorts,
    system: SystemPorts,
}
```

Migration: Create bundle structs, migrate `AppDeps` to hold them, update all `UseCases` factory methods to access `self.runtime.deps.clipboard.entry_repo` etc.

**ISP enforcement**: In P2.3, update UseCase constructors to accept individual ports (not bundles) for use cases needing ≤2 ports. Use cases needing 3+ ports from the same bundle may accept `&ClipboardPorts`.

---

**P2.3 — H3: Decompose SetupOrchestrator**

Migration path (4 phases, each independently mergeable):

1. **Phase A**: Extract `SetupContext` — move mutable state fields (`selected_peer_id`, `pairing_session_id`, `passphrase`, `joiner_offer`) into a `SetupContext` struct. Make `SetupActionExecutor::execute` accept `&mut SetupContext`.

2. **Phase B**: Define `SetupActionExecutor` trait (with `cancel: CancellationToken`). Extract `CreateSpaceExecutor` (simplest, 2 deps). Wire in `uc-tauri`.

3. **Phase C**: Extract `PairingExecutor`, `SpaceAccessExecutor`, `DiscoveryExecutor`. Each receives only its required ports via `uc-tauri` constructor.

4. **Phase D**: `SetupOrchestrator` becomes thin dispatcher (3 deps: SetupContext, event_port, status_port) + `HashMap<SetupActionKind, Arc<dyn SetupActionExecutor>>`. Remove all direct port fields from SetupOrchestrator.

**Executor wiring**: All 7 executors are constructed in `uc-tauri/src/bootstrap/wiring.rs`. `SetupOrchestrator::new()` receives a `HashMap<SetupActionKind, Arc<dyn SetupActionExecutor>>`.

**Dispatcher routing function** (testable independently):

```rust
fn route_action(
    &self,
    state: &SetupState,
    action: &SetupAction,
) -> Option<&dyn SetupActionExecutor> {
    match (state, action) {
        (SetupState::Initial, SetupAction::CreateSpace { .. }) => Some(&*self.create_space),
        // ... exhaustive routing table
    }
}
```

---

**P2.4 — H4: Decompose SyncInbound**

Extract in order:

1. `InboundDeduplicator` (owns `recent_ids: Arc<tokio::sync::Mutex<HashSet<MessageId>>>`)
2. `InboundPayloadDecoder` (owns `TransferPayloadDecryptorPort`)
3. `InboundClipboardApplier` (handles Full vs Passive mode write)

Serial pipeline model: SyncInboundClipboardUseCase processes one message at a time. The three services are NOT shared across concurrent calls. The orchestrating use case calls decoder → deduplicator → applier sequentially, with explicit rollback on applier failure:

```rust
async fn execute(&self, msg: InboundMessage) -> Result<(), SyncInboundError> {
    if self.deduplicator.is_duplicate(&msg.id) { return Ok(()); }
    let payload = self.decoder.decode(msg).await?;
    self.deduplicator.record(&msg.id);
    match self.applier.apply(payload).await {
        Ok(()) => Ok(()),
        Err(e) => {
            self.deduplicator.rollback(&msg.id);
            Err(e)
        }
    }
}
```

---

**P2.5 — H5: Restore SyncOutbound UseCase purity**

- Define `OutboundPayloadPreparerPort` in `uc-core` (typed errors: `PayloadPrepareError` with encryption/encoding variants)
- Define `ClipboardFanoutPort` in `uc-core` with typed `FanoutResult { outcomes: Vec<(PeerId, Result<(), FanoutError>)> }`
- Implement both in `uc-infra` / `uc-platform` respectively
- Remove `tokio::runtime::Handle::try_current()` from SyncOutbound
- Remove `executor::block_on` from SyncOutbound
- `ClipboardChangeHandler` implementation bridges async/sync via Tokio `mpsc` channel — platform watcher sends to channel, SyncOutbound UseCase runs in the channel consumer task on Tokio runtime

---

**P2.6 — M2: Fix Setup/Pairing responsibility crossover**

Single PR:

- Add `reopen_session_for_space_access(peer_id: String, session_id: String) -> Result<()>` to `PairingOrchestrator`
- Update `SetupOrchestrator::start_join_space_access()` to call this method
- Remove `pairing_transport: Arc<dyn PairingTransportPort>` field from `SetupOrchestrator`

---

**P2.7 — H7: Thread CancellationToken for graceful shutdown**

Phase 1 (high-value, low-risk):

- Create root `CancellationToken` in `run_app` before Builder
- Pass to `PlatformRuntime::new()` and `start_background_tasks()`
- `PlatformRuntime::start()`: add `select!` branch on `token.cancelled()`
- Wire `Shutdown` command and `Destroyed` window event to `root_token.cancel()`

Phase 2 (handle tracking):

- Create `TaskRegistry` struct in `uc-tauri/src/bootstrap/task_registry.rs` (NOT a port trait, NOT in uc-core)
  - Holds `Vec<(String, JoinHandle<()>)>` + root `CancellationToken`
  - `shutdown(timeout)`: cancel token, then `join_all` with timeout
- Register 5 critical long-lived tasks: libp2p swarm, spooler, blob worker, platform runtime, pairing event loop

Phase 3 (per-session tokens):

- Pairing session tasks use child tokens derived from root

---

**P2.8 — M8: Decompose run_app**

Extract 3 functions in `uc-tauri/src/bootstrap/composition.rs`:

- `compose_application(config: &BootstrapConfig) -> Result<ComposedApplication, CompositionError>` — channels, deps, orchestrators, key store, runtime
- `build_tauri_app(app: ComposedApplication) -> Result<TauriApp, CompositionError>` — Builder config, plugins
- `setup_callback(...)` — tray, background tasks, platform runtime spawn

`ComposedApplication` splits into `SharedServices` (Arc values) + owned channels consumed separately (not bundled into one god struct with mixed ownership semantics — per contract-error Phase 3 recommendation).

---

**P2.9 — M10: Add Drop to PlatformRuntime (with CancellationToken)**

Implement after P2.7 Phase 1 (token threading) is complete. Drop cancels the token (signals all child tasks) and calls watcher `stop()` synchronously.

---

**P2.10 — M3: Enrich anemic domain models**

Conservative additions to `ClipboardEntry` and `ClipboardEvent` as specified in app-flow Phase 2. Predicates only — no infrastructure types imported. No serialization logic.

---

### Phase 3: Clean Up Contracts, DTOs, and Error Model

_Goal: Every cross-layer boundary has a typed, explicit contract._

**P3.1 — M11: Migrate port traits to typed errors (by cluster)**

Priority order (most valuable first):

1. **Clipboard repositories** → `ClipboardRepositoryError { EntryNotFound(EntryId), Internal { source: ... } }`
2. **Blob ports** → `BlobError`
3. **Settings port** → `SettingsError`
4. **Network/transport ports** → `NetworkError`
5. **Infrastructure-only ports** (autostart, hash, UI) → simple typed errors

Rule: `anyhow` remains acceptable INSIDE use case bodies. Forbidden in port trait signatures.

Each port cluster migration is an independent PR. Each PR includes updated infra implementations with `.map_err()` conversions AND tests verifying error mapping behavior.

---

**P3.2 — H10: Introduce IPC DTO layer**

- Create `uc-tauri/src/models/pairing.rs` with `PairedDeviceDto`, implementing `From<PairedDevice>`
- **DTO location for testability**: DTOs that can be tested independently of Tauri should be in a `models` module that only depends on `serde` and basic types. Heavy Tauri-specific DTOs stay in `uc-tauri`.
- Priority: `list_paired_devices` (exposes internal domain fields) → `get_settings` DTO review → other commands
- Boundary rule: No domain model from `uc-core` appears as a command return type in `uc-tauri`

---

**P3.3 — H11: Introduce structured CommandError** (gates on P3.1)

After at least clipboard+blob+settings ports have typed errors:

- Define `CommandError` enum in `uc-tauri/src/commands/error.rs`
- Add `#[non_exhaustive]` to allow future variants without breaking frontend
- Include `#[source]` on `Internal` variant to preserve error chains
- Add golden-file snapshot test for JSON serialization format
- Migrate one command family at a time

---

**P3.4 — M9: Remove phantom EncryptionState::Initializing**

Single commit. Remove variant, update all match arms (confirm zero unreachable arms via grep), add comment documenting design decision.

---

**P3.5 — M12: PeerId type consistency in NetworkEvent**

Mechanical migration — 4 phases per contract-error Phase 2 plan. Include serialization compatibility test: `assert_eq!(serde_json::to_value(&PeerId::from("x")).unwrap(), serde_json::to_value("x").unwrap())`.

---

**P3.6 — M13: Rename AppConfig and remove overlapping fields**

- Rename `AppConfig` → `BootstrapConfig` in all usages
- Remove `device_name` and `silent_start` from `BootstrapConfig` (read from `Settings` only)
- Single PR, low risk

---

### Phase 4: Test Infrastructure & Stabilization

_Goal: Tests reflect architecture; architecture drift is automatically detectable._

**P4.1 — H12: Create `uc-test-support` crate (Phase 1 only)**

Create `src-tauri/crates/uc-test-support/`:

- `Cargo.toml`: depends only on `uc-core`, `async-trait`, `tokio` (for async test runtime), `parking_lot`
- Extract and centralize all mock/fake/noop implementations from 6 test files
- Use `parking_lot::Mutex` (not `std::sync::Mutex`) for all `InMemory*` mocks — no poisoning
- Mock taxonomy: `Noop*` (return Ok/empty), `InMemory*` (stateful), `Panicking*` (assert non-interaction), `Fake*` (realistic)
- Add contract test suites: `async fn verify_clipboard_entry_repo_contract(repo: &dyn ClipboardEntryRepositoryPort)`

**Do NOT implement `TestAppDepsBuilder` in this phase.**

---

**P4.2 — H13: Orchestrator-specific builders**

Add to `uc-test-support/src/builders/`:

- `TestSetupOrchestratorBuilder` — targets immediate 14-param pain
- `TestSyncInboundBuilder`
- `TestPairingOrchestratorBuilder`

These are scoped to orchestrator constructors and are immune to AppDeps restructuring.

---

**P4.3 — AppDeps builder (after P2.2 is complete)**

After AppDeps bundle decomposition lands, add `TestAppDepsBuilder` targeting the NEW bundle structure. At this point the builder is small: ~8 bundles instead of 39 individual fields.

---

**P4.4 — Regression safety CI guardrails**

```bash
# 1. No inline mock definitions in test files
grep -r "impl.*Port for" src-tauri/crates/uc-app/tests/ | grep -v "uc_test_support"
# Must return empty

# 2. uc-test-support dependency check
cargo metadata --format-version 1 | jq '.packages[] | select(.name=="uc-test-support") | .dependencies[].name' | grep -E "uc-infra|uc-app|uc-platform"
# Must return empty

# 3. Crate topology checks
cargo tree -p uc-platform | grep uc-infra     # empty
cargo tree -p uc-core | grep tokio            # empty
cargo tree -p uc-app | grep uc-infra          # empty (production only)
cargo tree -p uc-infra | grep uc-platform     # empty

# 4. No expect() in production (non-test) code in listed crates
```

**AppDeps field budget test** (replace size_of heuristic with named constant):

```rust
const APPDEPS_FIELD_BUDGET: usize = 45;
const APPDEPS_ACTUAL_FIELDS: usize = 12; // count bundle-level fields after P2.2
#[test]
fn appdeps_within_field_budget() {
    assert!(APPDEPS_ACTUAL_FIELDS <= APPDEPS_FIELD_BUDGET,
        "AppDeps too large — decompose before adding more");
}
```

---

**P4.5 — M7: Reorganize uc-infra/clipboard**

Mechanical file move to subdirectories `spool/`, `blob/`, `transform/`, `transfer/`. Public API via `mod.rs` re-exports remains unchanged. Done as a single atomic PR with no behavioral changes.

---

## 5. Module / Dependency Boundary Adjustments Summary

### Crate Topology After Reform

```
uc-tauri (Composition Root)
  deps: uc-app, uc-infra, uc-platform, uc-core
  owns: TaskRegistry (concrete struct), ComposedApplication, DTOs, CommandError

uc-app (Application Layer)
  deps: uc-core ONLY (prod), uc-core + uc-test-support (dev)
  owns: UseCases, Orchestrators, AppDeps bundles, StagedDeviceStorePort (uc-app-level port)

uc-infra (Infrastructure Implementations)
  deps: uc-core ONLY
  owns: DB repos, encryption, blob, spool, ChunkedStreamDecoderAdapter

uc-platform (Platform Adapters)
  deps: uc-core ONLY  ← FIXED (was: uc-core + uc-infra)
  owns: Libp2pNetworkAdapter, ClipboardWatcher, PlatformRuntime, IPC

uc-core (Domain + Ports)
  deps: ZERO workspace crates (serde, async-trait, anyhow, chrono, futures-core, uuid, thiserror only)
  owns: Domain models, all port traits incl. application/ sub-module, EventStream<T> type alias
  NOT: tokio types in port signatures, infrastructure implementations, test mocks

uc-test-support (Test Utilities)
  deps: uc-core ONLY
  owns: Noop*/InMemory*/Panicking*/Fake* mocks, orchestrator builders, contract test suites
  NOT: uc-infra, uc-app, uc-platform, uc-tauri types
```

### New Files / Modules (key additions)

| File                                                | Phase | Purpose                                  |
| --------------------------------------------------- | ----- | ---------------------------------------- |
| `uc-core/src/ports/application/`                    | P2.1  | UiPort, AutostartPort sub-module         |
| `uc-core/src/ports/clipboard_transport.rs`          | P1.1  | StreamDecoderPort                        |
| `uc-core/src/ports/spawner.rs`                      | P2.7  | SpawnerPort (runtime-agnostic)           |
| `uc-infra/src/clipboard/transfer/stream_decoder.rs` | P1.1  | ChunkedStreamDecoderAdapter              |
| `uc-app/src/ports/staged_device_store.rs`           | P1.4  | StagedDeviceStorePort                    |
| `uc-app/src/usecases/setup/executors/`              | P2.3  | 7 SetupActionExecutor impls              |
| `uc-tauri/src/bootstrap/composition.rs`             | P2.8  | compose_application, ComposedApplication |
| `uc-tauri/src/bootstrap/task_registry.rs`           | P2.7  | TaskRegistry (concrete struct)           |
| `uc-tauri/src/models/pairing.rs`                    | P3.2  | PairedDeviceDto                          |
| `uc-tauri/src/commands/error.rs`                    | P3.3  | CommandError enum                        |
| `uc-test-support/`                                  | P4.1  | New crate                                |

### Files / Modules to Delete

| File                                                        | Phase | Reason                      |
| ----------------------------------------------------------- | ----- | --------------------------- |
| `uc-platform/src/adapters/encryption.rs`                    | P1.5  | Duplicate EncryptionSession |
| `uc-app/src/usecases/pairing/staged_paired_device_store.rs` | P1.4  | Global static eliminated    |

---

## 6. DTO / Error Model / Port Contract Migration

### Error Migration Sequence

```
Phase 0: Rule — no new anyhow::Result in port signatures
Phase 3.1: Typed errors per cluster (clipboard → blob → settings → network → platform)
Phase 3.3: CommandError structured enum (gates on Phase 3.1 clipboard cluster)
```

### Port Error Types to Define (in uc-core/src/ports/errors.rs)

```rust
// Cluster 1 (highest priority)
pub enum ClipboardRepositoryError { EntryNotFound(EntryId), Internal { source: anyhow::Error } }
pub enum BlobError { NotFound(BlobId), StorageFull, Internal { source: anyhow::Error } }

// Cluster 2
pub enum SettingsError { NotFound, Corrupted, Io { source: anyhow::Error } }

// Cluster 3
pub enum NetworkError { ConnectionLost, PeerNotFound(PeerId), Protocol(String), Internal { source: anyhow::Error } }
pub enum SecurityError { /* already partially exists — extend */ }

// New ports (immediate)
pub enum StreamDecodeError { Corruption(String), DecryptionFailed(String), DecompressionFailed(String) }
pub enum StagedDeviceError { AlreadyStaged(String), NotFound(String) }
pub enum SpawnError { RuntimeNotAvailable }
```

### DTO Pattern (IPC boundary)

- `uc-tauri/src/models/`: IPC DTOs only, depend on `serde` and `uc-core` types
- `From<DomainModel> for Dto` implemented in `uc-tauri/src/models/`
- For mappings requiring additional data: define `XxxProjector` service in `uc-tauri`
- Domain models in `uc-core` MUST NOT derive `Serialize/Deserialize` solely for IPC

---

## 7. Testing and Regression Strategy

### Test Architecture After Reform

```
uc-test-support/
  mocks/       — Noop*, InMemory*, Panicking*, Fake* per port domain
  builders/    — TestSetupOrchestratorBuilder, TestSyncInboundBuilder, etc.
  contracts/   — Contract test suites verifying port semantic correctness
  assertions/  — Error assertion helpers (assert_command_error_not_found, etc.)
```

### What to Test at Each Layer

| Layer                      | Test type                                    | Tools                                  |
| -------------------------- | -------------------------------------------- | -------------------------------------- |
| Domain models              | Pure unit tests                              | No mocks needed                        |
| Port trait implementations | Contract tests from uc-test-support          | InMemory\* + real impls run same suite |
| UseCases                   | Unit tests with Noop*/InMemory*              | uc-test-support mocks                  |
| Orchestrators              | Integration tests with orchestrator builders | TestSetupOrchestratorBuilder           |
| Commands                   | Integration tests via Tauri test harness     | Real app context                       |
| Error mappings             | Unit tests for each `.map_err()` chain       | assert_maps_to! helper                 |

### Specific Tests to Add

1. **SyncInbound composition test**: `applier_failure_triggers_dedup_rollback` (per testability Phase 3 suggestion)
2. **SetupOrchestrator dispatch table test**: `route_action()` function tested as a pure lookup table
3. **PeerId serialization compatibility test**: M12 migration guard
4. **CommandError JSON golden-file test**: contract stability guard
5. **EncryptionState reachability test**: ensure all enum variants have a code path

---

## 8. Guardrails to Prevent Future Drift

### Compile-Time Enforcement

- `AppRuntime.deps` is `pub(crate)` — commands cannot access it from outside `uc-tauri`
- `SpawnerPort` uses no tokio types — `uc-core` stays tokio-free automatically
- UseCase constructors accept specific ports / bundles — type system enforces ISP
- `uc-test-support` depends only on `uc-core` — mocks stay pure

### CI Enforcement (add to `.github/workflows/`)

```yaml
- name: Check crate topology invariants
  run: |
    cargo tree -p uc-platform | grep uc-infra && exit 1 || true
    cargo tree -p uc-core | grep "^tokio " && exit 1 || true
    cargo tree -p uc-app --edges normal | grep uc-infra && exit 1 || true
    echo "Topology invariants OK"

- name: Check no inline mock definitions in test directories
  run: |
    if grep -r "impl.*Port for" src-tauri/crates/uc-app/tests/ | grep -v "uc_test_support"; then
      echo "ERROR: Mock definitions must be in uc-test-support, not inline in test files"
      exit 1
    fi

- name: Check no expect() in production code
  run: |
    cd src-tauri
    cargo clippy -p uc-platform -p uc-app -p uc-infra -- -D clippy::expect_used
```

### Architectural Decision Records (ADRs)

Create `docs/architecture/decisions/` with ADRs for:

1. Port placement rule: `uc-core` for domain-referenced ports, `uc-app` for application-only ports
2. Error handling rule: typed errors in port signatures, anyhow acceptable in use case bodies
3. Bundle vs individual port rule: bundles for storage (AppDeps), individual for use case injection
4. No new global statics: all shared mutable state via constructor injection
5. Task lifecycle rule: CancellationToken from composition root, TaskRegistry in uc-tauri only

### PR Review Checklist

Embed in `.github/pull_request_template.md`:

- [ ] If adding a new `Port` trait: typed error type defined, mock in uc-test-support
- [ ] If adding to AppDeps: went through bundle group, not as individual field
- [ ] If adding `tokio::spawn`: passes `CancellationToken`, registered with TaskRegistry
- [ ] If adding a Tauri command: uses `runtime.usecases().xxx()`, returns `Result<DTO, CommandError>`
- [ ] If changing a port signature: infra implementation updated, contract test updated

---

## 9. Implementation Priority Index

_For sprint planning: items within each phase can be parallelized; phases themselves are sequential for the critical path items._

| Priority | Item                            | Phase | Risk   | Effort | Blocking?                               |
| -------- | ------------------------------- | ----- | ------ | ------ | --------------------------------------- |
| P0       | CI topology checks              | 0     | Low    | 2h     | Blocks nothing                          |
| P1       | H1 StreamDecoderPort            | 1     | Low    | 4h     | Unblocks uc-platform dependency cleanup |
| P2       | H2 make deps private            | 1     | Medium | 4h     | Enables ISP enforcement                 |
| P3       | H8+H9 EncryptionSession         | 1     | Low    | 3h     | Eliminates expect()                     |
| P4       | H6 StagedDeviceStore            | 1     | Low    | 4h     | Eliminates global static                |
| P5       | H7 Phase 1 CancellationToken    | 2     | Medium | 1d     | Enables M10 Drop                        |
| P6       | M8 run_app decompose            | 2     | Medium | 1d     | Testability of wiring                   |
| P7       | M11 clipboard repo typed errors | 3     | Medium | 1d     | Enables H11 CommandError                |
| P8       | H12 uc-test-support crate       | 4     | Low    | 2d     | Reduces test debt immediately           |
| P9       | H3 SetupOrchestrator Phase A+B  | 2     | High   | 3d     | Most complex, do after H7               |
| P10      | H4 SyncInbound decompose        | 2     | Medium | 2d     |                                         |
| P11      | M1 AppDeps bundles              | 2     | Medium | 1d     | Enables P4.3 builder                    |
| P12      | M6 Stream migration             | 1     | Medium | 1d     | Removes tokio from core                 |
| P13      | H10 IPC DTOs                    | 3     | Low    | 1d     |                                         |
| P14      | H11 CommandError                | 3     | Low    | 1d     | Gates on M11                            |
| P15      | H5 SyncOutbound                 | 2     | Medium | 1d     |                                         |

---

## 10. Summary of Rejected Proposals

| Proposal                                                         | Source            | Reason Rejected                                                                                                                                               |
| ---------------------------------------------------------------- | ----------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| TaskRegistryPort in uc-core (with CancellationToken, JoinHandle) | infra-runtime P2  | Reintroduces tokio in uc-core — contradicts M6. Use concrete TaskRegistry in uc-tauri instead.                                                                |
| Temporary re-export shim for EncryptionSession migration         | infra-runtime P2  | Creates transitional H1 violation. Atomic migration is feasible.                                                                                              |
| UiPort/AutostartPort moved to uc-app                             | arch-guardian P2  | uc-platform implements these and cannot depend on uc-app. Sub-module in uc-core is the correct solution.                                                      |
| Aggregate traits (ClipboardStoragePorts) in uc-core              | arch-guardian P2  | Opaque dependency contracts; accessor `&dyn Trait` prevents Arc extraction for tasks; concrete structs are simpler and self-corrected by arch-guardian in P3. |
| TestAppDepsBuilder before AppDeps decomposition                  | testability P2    | Becomes stale within 1-2 sprints. Defer until after P2.2.                                                                                                     |
| CommandError without typed port errors (M11)                     | contract-error P2 | Functionally identical to current String errors — cosmetic. M11 is the prerequisite.                                                                          |
| SyncInbound with concurrent service access (shared Arc)          | app-flow P2       | Introduces race conditions. Serial pipeline model enforced.                                                                                                   |
| SetupActionExecutor without CancellationToken                    | app-flow P2       | Orphaned tasks risk on setup abort. Token required from day one.                                                                                              |
| std::sync::Mutex in InMemory mocks                               | testability P2    | Same poisoning risk as H9. Use parking_lot::Mutex.                                                                                                            |
| size_of heuristic for AppDeps field count test                   | testability P2    | Counts ~half actual fields due to fat pointers. Use named constant.                                                                                           |
