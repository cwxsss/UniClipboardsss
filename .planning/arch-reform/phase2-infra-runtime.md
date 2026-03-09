# Infra & Runtime Reviewer — Phase 2 Reform Proposal

## Root Cause Analysis

The seven issues in this cluster share two systemic root causes:

1. **Missing lifecycle abstraction layer**: The codebase has no unified mechanism for task lifecycle management. Every `tokio::spawn()` is fire-and-forget with no cancellation token, no handle tracking, and no shutdown coordination. This affects H7, M8, and M10 — they are all symptoms of the same missing abstraction.

2. **Ownership ambiguity at crate boundaries**: The hexagonal architecture defines ports in `uc-core`, but does not clearly designate which adapter crate owns the canonical implementation for cross-cutting concerns. This leads to duplicate implementations (H8), global static workarounds (H6), and scattered responsibilities (M7). H9 (`expect()` in production) is a localized code quality issue rooted in using `std::Mutex` in async contexts where the lock API forces `expect()` rather than `?`.

---

## Reform Proposals by Issue

### H6: staged_paired_device_store Global Static

**Current state**: `src-tauri/crates/uc-app/src/usecases/pairing/staged_paired_device_store.rs` uses a `static OnceLock<Mutex<HashMap<String, PairedDevice>>>` as a global store. This is accessed from both `PairingOrchestrator` and `SpaceAccessPersistenceAdapter` — two separate use cases sharing mutable state through a module-level static.

**Root cause**: During pairing, a `PairedDevice` is "staged" (validated but not yet persisted) and needs to be accessible from the space-access flow that runs concurrently. Rather than threading this through proper dependency injection, a global static was used as a shortcut.

**Proposed fix**:

1. Define a `StagedDeviceStore` trait in `uc-app` (not `uc-core`, since this is orchestration-level state):

```rust
// uc-app/src/usecases/pairing/staged_device_store.rs
#[async_trait]
pub trait StagedDeviceStorePort: Send + Sync {
    async fn stage(&self, session_id: &str, device: PairedDevice);
    async fn take_by_peer_id(&self, peer_id: &str) -> Option<PairedDevice>;
    async fn get_by_peer_id(&self, peer_id: &str) -> Option<PairedDevice>;
}
```

2. Create an in-memory implementation using `tokio::sync::RwLock<HashMap<...>>` (no static).

3. Inject `Arc<dyn StagedDeviceStorePort>` into both `PairingOrchestrator::new()` and `SpaceAccessPersistenceAdapter::new()` at wiring time in `uc-tauri`.

4. Remove the `staged_paired_device_store` module with its `OnceLock` static.

**Migration path**: Create the trait and implementation first, then update `PairingOrchestrator` constructor to accept it, then update `SpaceAccessPersistenceAdapter`, then delete the static module. Each step compiles independently.

---

### H7: Tasks Lacking Cancellation / Graceful Shutdown

**Current state**: 20+ production `tokio::spawn()` calls across `uc-platform`, `uc-app`, `uc-tauri`, and `uc-infra` — none with cancellation tokens or handle tracking. Key locations:

- `src-tauri/src/main.rs:709-782` — main backend init task (spawns nested tasks)
- `src-tauri/crates/uc-platform/src/runtime/runtime.rs:363` — runtime event loop
- `src-tauri/crates/uc-platform/src/adapters/libp2p_network.rs` — swarm tasks (lines 395, 889, 897, 1430)
- `src-tauri/crates/uc-platform/src/adapters/pairing_stream/service.rs` — pairing session tasks (lines 110, 130, 151, 317, 414, 421)
- `src-tauri/crates/uc-app/src/usecases/pairing/orchestrator.rs:951,1511` — pairing orchestrator spawns
- `src-tauri/crates/uc-app/src/usecases/setup/orchestrator.rs:478,761` — setup orchestrator spawns
- `src-tauri/crates/uc-tauri/src/bootstrap/wiring.rs:1158+` — background spooler/worker tasks

**Root cause**: No application-level task management abstraction exists. Each subsystem spawns tasks independently.

**Proposed fix — introduce `TaskRegistry` in `uc-core/ports`**:

```rust
// uc-core/src/ports/lifecycle.rs
use tokio_util::sync::CancellationToken;

pub trait TaskRegistryPort: Send + Sync {
    /// Returns a child token derived from the application-level root token.
    fn child_token(&self) -> CancellationToken;

    /// Register a named task handle for lifecycle tracking.
    fn register(&self, name: &str, handle: tokio::task::JoinHandle<()>);

    /// Initiate graceful shutdown: cancel all tokens, then await all handles with timeout.
    async fn shutdown(&self, timeout: std::time::Duration);
}
```

**Implementation strategy (3 phases)**:

**Phase 1 — Root token threading (low risk, high value)**:

- Create a `CancellationToken` in `run_app` before the Builder.
- Pass it to `PlatformRuntime`, `start_background_tasks`, and the main init spawn.
- Modify `PlatformRuntime::start()` to select on `token.cancelled()` alongside events/commands.
- Wire `Shutdown` command to cancel the root token.
- Wire Tauri's `on_window_event` `Destroyed` to cancel the root token.

**Phase 2 — Handle tracking for critical tasks**:

- Implement `TaskRegistry` holding `Vec<(String, JoinHandle<()>)>` + root `CancellationToken`.
- Register the libp2p swarm task, spooler task, blob worker task, and platform runtime task.
- Implement `shutdown()` as: cancel token, then `tokio::time::timeout` on `join_all`.

**Phase 3 — Per-subsystem tokens**:

- Derive child tokens for pairing sessions, so individual sessions can be cancelled without full shutdown.
- Pairing stream service uses child tokens for read/write loop tasks.

**Key constraint**: The `CancellationToken` must be created at the composition root (`run_app`) and threaded downward — never stored as a global static.

---

### H8: Duplicate EncryptionSession Implementations

**Current state**: Two independent implementations of `EncryptionSessionPort`:

| Aspect     | `uc-infra` (`encryption_session.rs`) | `uc-platform` (`adapters/encryption.rs`) |
| ---------- | ------------------------------------ | ---------------------------------------- |
| Type name  | `InMemoryEncryptionSession`          | `InMemoryEncryptionSessionPort`          |
| Lock       | `tokio::sync::RwLock`                | `std::sync::Mutex`                       |
| Panic risk | None (async `.await`)                | 4x `.expect("lock state")`               |
| Clone      | Not Clone                            | Clone (via `Arc<Mutex<>>`)               |

**Root cause**: The `EncryptionSessionPort` trait is defined in `uc-core`, but neither `uc-infra` nor `uc-platform` was designated as the canonical adapter owner. Both crates independently implemented it.

**Proposed fix**:

1. **Canonical owner: `uc-infra`**. Rationale: `uc-infra` already houses all security-related adapters (encryption repository, encrypted blob store, decrypting repos). The encryption session is a security concern, not a platform concern.

2. **Upgrade `uc-infra`'s implementation** to support `Clone` (wrap in `Arc<RwLock<>>`):

```rust
// uc-infra/src/security/encryption_session.rs
#[derive(Clone)]
pub struct InMemoryEncryptionSession {
    key: Arc<RwLock<Option<MasterKey>>>,
}
```

3. **Delete** `uc-platform/src/adapters/encryption.rs` entirely. Update `uc-platform/src/adapters/mod.rs` to remove the re-export.

4. **Update wiring** in `uc-tauri/src/bootstrap/wiring.rs` to construct `uc_infra::security::InMemoryEncryptionSession` and inject it as `Arc<dyn EncryptionSessionPort>` into both `uc-infra` decorators and `uc-platform` adapters (e.g., `Libp2pNetworkAdapter`).

**Migration path**: Add `Clone` to uc-infra's version first, then update all import sites in uc-platform tests to use `uc_infra::security::InMemoryEncryptionSession`, then delete the uc-platform file.

---

### H9: expect() in Production Code

**Current state**: 4 production `expect()` calls in `uc-platform/src/adapters/encryption.rs` (lines 13, 18, 29, 41) — all on `std::Mutex::lock()`. Plus 1 in `main.rs:845` on `.run()`.

Additional production occurrences:

- `uc-platform/src/clipboard/watcher.rs:137` — test-only (`#[cfg(test)]` module), acceptable.

**Inventory of production `expect()` calls requiring fixes**:

| Location                                | Expression                                         | Risk                       |
| --------------------------------------- | -------------------------------------------------- | -------------------------- |
| `uc-platform/adapters/encryption.rs:13` | `self.state.lock().expect("lock state")`           | Panic if mutex poisoned    |
| `uc-platform/adapters/encryption.rs:18` | same                                               | same                       |
| `uc-platform/adapters/encryption.rs:29` | same                                               | same                       |
| `uc-platform/adapters/encryption.rs:41` | same                                               | same                       |
| `main.rs:845`                           | `.expect("error while running tauri application")` | Panic on Tauri fatal error |

**Proposed fixes**:

1. **encryption.rs lines 13/18/29/41**: These are all eliminated by H8's fix (deleting this file). The `uc-infra` implementation uses `tokio::RwLock` which returns the guard directly via `.await` — no `expect()` needed.

2. **main.rs:845**: This is the Tauri `.run()` call. Tauri's `.run()` returning an `Err` means the event loop failed fatally. Replacing with:

```rust
if let Err(e) = builder.run(tauri::generate_context!()) {
    error!(error = %e, "Tauri application failed");
    std::process::exit(1);
}
```

This logs the error before exiting, giving operators visibility into what went wrong.

3. **main.rs:491** (`panic!("Dependency wiring failed")`) — Replace with:

```rust
Err(e) => {
    error!("Failed to wire dependencies: {}", e);
    std::process::exit(1);
}
```

---

### M7: uc-infra/clipboard Scattered Responsibilities

**Current state**: 14 files in `src-tauri/crates/uc-infra/src/clipboard/`:

```
background_blob_worker.rs   # Background task: blob persistence
change_origin.rs            # In-memory change origin tracking
chunked_transfer.rs         # Chunked encryption/decryption for transfer
normalizer.rs               # Representation normalization
payload_resolver.rs         # Clipboard payload resolution
representation_cache.rs     # In-memory representation cache
selection_resolver.rs       # Selection resolution
spool_janitor.rs            # Cleanup of old spool entries
spool_manager.rs            # Spool directory management
spool_queue.rs              # MPSC-based spool queue
spool_scanner.rs            # Filesystem spool scanning
spooler_task.rs             # Background task: spool processing
thumbnail_generator.rs      # Image thumbnail generation
mod.rs                      # Re-exports everything flat
```

**Root cause**: All clipboard-related infrastructure was placed in a single module without sub-categorization.

**Proposed reorganization** — group by responsibility domain:

```
uc-infra/src/clipboard/
├── mod.rs                      # Re-exports from submodules
├── spool/                      # Spool subsystem (5 files)
│   ├── mod.rs
│   ├── manager.rs              # SpoolManager (directory operations)
│   ├── queue.rs                # MpscSpoolQueue (channel-based queue)
│   ├── scanner.rs              # SpoolScanner (filesystem walk)
│   ├── janitor.rs              # SpoolJanitor (TTL cleanup)
│   └── task.rs                 # SpoolerTask (background loop)
├── blob/                       # Blob subsystem (2 files)
│   ├── mod.rs
│   └── worker.rs               # BackgroundBlobWorker
├── transform/                  # Data transformation (3 files)
│   ├── mod.rs
│   ├── normalizer.rs           # ClipboardRepresentationNormalizer
│   ├── thumbnail.rs            # InfraThumbnailGenerator
│   └── payload_resolver.rs     # ClipboardPayloadResolver
├── transfer/                   # Network transfer (1 file)
│   ├── mod.rs
│   └── chunked.rs              # ChunkedEncoder/Decoder + encryption adapters
├── cache.rs                    # RepresentationCache (standalone)
├── change_origin.rs            # InMemoryClipboardChangeOrigin (standalone)
└── selection_resolver.rs       # SelectionResolver (standalone)
```

**Migration path**: Move files one subdirectory at a time. Update `mod.rs` re-exports to maintain the same public API. Internal callers use the same types — only import paths change. This is a purely mechanical refactor with no behavioral changes.

---

### M8: run_app Monolithic ~370 Lines

**Current state**: `run_app()` in `src-tauri/src/main.rs` (lines 475-846) is a single function that:

1. Creates event channels (479-484)
2. Wires dependencies (487-497)
3. Extracts pairing config and constructs orchestrators (498-519)
4. Constructs key slot store (520-544)
5. Creates AppRuntime (546-553)
6. Creates startup barrier (562)
7. Configures Tauri Builder with plugins, event handlers, URI protocol (574-627)
8. Registers 40+ command handlers (787-843)
9. Runs setup block with initialization, tray, auto-unlock, platform runtime (642-786)

**Root cause**: Composition root logic was written incrementally without decomposition.

**Proposed decomposition** — extract into named setup functions in `uc-tauri/src/bootstrap/`:

```rust
// uc-tauri/src/bootstrap/composition.rs (new file)

pub struct CompositionResult {
    pub runtime: Arc<AppRuntime>,
    pub pairing_orchestrator: Arc<PairingOrchestrator>,
    pub space_access_orchestrator: Arc<SpaceAccessOrchestrator>,
    pub key_slot_store: Arc<dyn KeySlotStore>,
    pub background: BackgroundRuntimeDeps,
    pub platform_event_tx: PlatformEventSender,
    pub platform_event_rx: PlatformEventReceiver,
    pub platform_cmd_tx: Sender<PlatformCommand>,
    pub platform_cmd_rx: PlatformCommandReceiver,
}

pub fn compose_application(config: &AppConfig) -> Result<CompositionResult> {
    // Steps 1-6 from current run_app
}
```

Then `run_app` becomes:

```rust
fn run_app(config: AppConfig) {
    let composed = compose_application(&config)
        .unwrap_or_else(|e| { error!(...); std::process::exit(1); });

    build_tauri_app(config, composed)
        .unwrap_or_else(|e| { error!(...); std::process::exit(1); });
}
```

**Phase 1**: Extract `compose_application()` — channels, wiring, orchestrators, key slot store, runtime creation.
**Phase 2**: Extract `build_tauri_app()` — Builder configuration, plugins, setup block.
**Phase 3**: Extract `setup_callback()` — the closure passed to `.setup()`, covering tray init, background tasks, platform runtime spawn.

Each phase produces a compilable, testable intermediate state.

---

### M10: PlatformRuntime Missing Drop

**Current state**: `PlatformRuntime<E>` (in `uc-platform/src/runtime/runtime.rs`) holds:

- `watcher_handle: Option<WatcherShutdown>` — can stop the clipboard watcher
- `watcher_join: Option<JoinHandle<()>>` — the watcher's blocking task handle
- `event_tx` / `event_rx` / `command_rx` — channels that should be closed
- `local_clipboard: Arc<dyn SystemClipboardPort>` — shared reference

When `PlatformRuntime::start()` completes (by `self.shutting_down = true`), the struct is consumed (it takes `self`). However, if the runtime is dropped before `start()` is called, or if the task holding it is aborted, no cleanup occurs.

**Proposed fix**:

```rust
impl<E: PlatformCommandExecutorPort> Drop for PlatformRuntime<E> {
    fn drop(&mut self) {
        // Stop clipboard watcher if running
        if let Some(handle) = self.watcher_handle.take() {
            handle.stop();
        }
        // JoinHandle is dropped automatically (task continues but we can't await it in Drop)
        // Log for observability
        if self.watcher_running {
            tracing::warn!("PlatformRuntime dropped while watcher was still running");
        }
    }
}
```

**Important caveat**: `Drop` cannot be async, so we cannot `.await` the `JoinHandle`. The watcher stop is synchronous (it signals a channel), so that part works. The join handle will be cleaned up by the Tokio runtime when the task completes.

For proper async cleanup, `PlatformRuntime` should also accept a `CancellationToken` (from H7's fix) and cancel it in `Drop`, signaling all child tasks to wind down.

---

## Boundaries to Protect

1. **`uc-core` remains dependency-free**: No new infrastructure types (CancellationToken, JoinHandle) should leak into `uc-core` port definitions. The `TaskRegistryPort` should use `tokio_util::sync::CancellationToken` only in the implementation, not the trait. The trait can accept/return opaque types or use a simpler signal mechanism.

   _Revision_: Define a minimal `ShutdownSignal` trait in `uc-core` if needed, but prefer keeping lifecycle management entirely in `uc-tauri` (composition root) and `uc-platform` (runtime).

2. **`uc-infra` does not depend on `uc-platform`**: The EncryptionSession canonical implementation in `uc-infra` must not import anything from `uc-platform`. Both crates depend only on `uc-core`.

3. **`uc-app` does not depend on Tokio directly for task management**: Use case orchestrators should not call `tokio::spawn` directly. Instead, accept a `Spawner` or `TaskRegistryPort` if they need to spawn background work. However, this is a longer-term goal — for now, the existing spawns in orchestrators are acceptable as long as they receive cancellation tokens.

4. **No new global statics**: The H6 fix must not introduce a new `OnceLock` or `lazy_static`. All state must flow through constructor injection.

5. **Spool subsystem public API stability**: The M7 reorganization must not change the public API of `uc-infra::clipboard`. The `mod.rs` must continue to re-export all existing public types at the same paths.

---

## Abstractions to Add / Remove / Split

### Add

| Abstraction                   | Location              | Purpose                                      |
| ----------------------------- | --------------------- | -------------------------------------------- |
| `StagedDeviceStorePort`       | `uc-app`              | Replace global static for pairing state (H6) |
| `CancellationToken` threading | `run_app` composition | Enable graceful shutdown (H7)                |
| `TaskRegistry`                | `uc-tauri/bootstrap`  | Track and await spawned tasks (H7)           |
| `CompositionResult` struct    | `uc-tauri/bootstrap`  | Decompose run_app (M8)                       |
| `Drop` for `PlatformRuntime`  | `uc-platform/runtime` | Resource cleanup (M10)                       |

### Remove

| Abstraction                                   | Location                             | Reason                             |
| --------------------------------------------- | ------------------------------------ | ---------------------------------- |
| `InMemoryEncryptionSessionPort`               | `uc-platform/adapters/encryption.rs` | Duplicate of uc-infra version (H8) |
| `PlaceholderEncryptionSessionPort` type alias | same file                            | Removed with the file              |
| `staged_paired_device_store` module           | `uc-app/usecases/pairing/`           | Replaced by injected port (H6)     |
| `STAGED_PAIRED_DEVICES` static                | same module                          | Global state eliminated (H6)       |

### Split

| Current                                   | Into                                                             | Reason                           |
| ----------------------------------------- | ---------------------------------------------------------------- | -------------------------------- |
| `uc-infra/src/clipboard/` (flat 14 files) | `spool/`, `blob/`, `transform/`, `transfer/` subdirs             | Responsibility grouping (M7)     |
| `run_app()` (370 lines)                   | `compose_application()`, `build_tauri_app()`, `setup_callback()` | Testability and readability (M8) |

---

## Risks & Trade-offs

### Risk 1: CancellationToken adoption scope creep

Adding `CancellationToken` to every `tokio::spawn` across the codebase is a large change. **Mitigation**: Phase 1 targets only the 5 critical long-running tasks (platform runtime, libp2p swarm, spooler, blob worker, pairing event loop). Short-lived tasks (single request handlers) can remain fire-and-forget.

### Risk 2: EncryptionSession migration breaks wiring

Multiple crates import the uc-platform version. **Mitigation**: Keep the uc-platform file as a re-export alias during transition:

```rust
// Temporary: uc-platform/src/adapters/encryption.rs
pub use uc_infra::security::InMemoryEncryptionSession as InMemoryEncryptionSessionPort;
pub use uc_infra::security::InMemoryEncryptionSession as PlaceholderEncryptionSessionPort;
```

Remove re-exports in a follow-up PR after all call sites are updated.

### Risk 3: M7 reorganization causes merge conflicts

Moving 14 files into subdirectories will conflict with any in-flight changes. **Mitigation**: Do it as a single atomic PR with no behavioral changes. Coordinate with the team to merge during a quiet period.

### Risk 4: run_app decomposition makes debugging harder

Splitting the monolithic function into multiple files means stack traces span more locations. **Mitigation**: Use `tracing::info_span!` in each extracted function so the structured logs show the logical composition phase.

### Risk 5: StagedDeviceStore injection adds constructor parameters

Both `PairingOrchestrator` and `SpaceAccessPersistenceAdapter` gain a new dependency. **Mitigation**: This is a one-time wiring change in `uc-tauri`. The constructors already take multiple `Arc<dyn ...>` parameters — one more is not a burden.

### Trade-off: std::Mutex vs tokio::RwLock for EncryptionSession

The uc-platform version used `std::Mutex` (no `.await` needed, lower overhead) but introduced panic risk. The uc-infra version uses `tokio::RwLock` (safe, async-compatible) but has slight overhead. **Decision**: Use `tokio::RwLock`. The encryption session is accessed infrequently (initialization, per-clipboard-event check). The overhead is negligible compared to the safety guarantee.

---

## Pseudo-Solutions to Reject

### 1. "Just add `catch_unwind` around `expect()` calls"

This masks the problem. Poisoned mutexes indicate a deeper issue (panic in critical section). The correct fix is to eliminate `std::Mutex` in async code entirely, not to catch the panics.

### 2. "Use `tokio::spawn` with `AbortHandle` for shutdown"

Aborting tasks is not graceful shutdown — it's the async equivalent of `kill -9`. Aborted tasks cannot flush buffers, close connections, or persist state. The correct approach is `CancellationToken` with cooperative cancellation via `tokio::select!`.

### 3. "Merge uc-infra and uc-platform EncryptionSession into uc-core"

`uc-core` defines ports (traits), not implementations. Putting an in-memory implementation in `uc-core` violates the hexagonal architecture boundary. Implementations belong in adapter crates.

### 4. "Make staged_paired_device_store thread-safe with Arc<RwLock<>> but keep it static"

A "better" global static is still a global static. It bypasses dependency injection, makes testing non-deterministic (shared state between tests), and hides dependencies. The fix is injection, not better locking.

### 5. "Split run_app into sub-closures within the same function"

This reduces line count but not complexity. Each sub-closure still captures the same variables and cannot be tested independently. The decomposition must produce separate functions with explicit parameters.

### 6. "Add `#[allow(clippy::expect_used)]` to suppress warnings"

Suppressing the warning doesn't fix the panic risk. This is security-sensitive code (encryption key management). Production panics in this code path could leave keys in inconsistent states.

### 7. "Use a process supervisor to restart after panics"

This treats the application as unreliable and compensates externally. A desktop application cannot rely on process supervisors — users expect the app to not crash. Fix the code, don't build infrastructure around crashes.
