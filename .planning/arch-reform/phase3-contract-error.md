# Contract & Error Model Reviewer — Phase 3 Cross-Review

## Review of: arch-guardian's Phase 2 Proposal

### A. Points of Agreement

1. **H1 StreamDecoderPort is contract-sound.** The proposed `StreamDecoderPort` trait has a clean signature: `async fn decode_stream(&self, data: Vec<u8>, master_key: &[u8; 32]) -> Result<Vec<u8>>`. The input/output types are plain byte vectors, which is the right abstraction level for a port — it hides the wire format, encryption scheme, and compression details from consumers. The `master_key: &[u8; 32]` parameter is a fixed-size array, not a domain type like `MasterKey`, which is correct for the port boundary (the adapter converts to/from `MasterKey` internally).

2. **H2 phased approach (private deps + pub(crate) accessor) is structurally correct.** Making `deps` field `pub(crate)` instead of `pub` is the minimum-viable contract fix. It uses Rust's visibility system to enforce the boundary at compile time, which is the strongest possible guarantee. The `pub fn current_device_id()` accessor for observability is well-scoped — it exposes a specific cross-cutting value without exposing the entire dependency tree.

3. **M4 decision criteria for port placement is valuable.** The rule "A port belongs in uc-core ONLY if referenced by domain models or domain services" provides a testable predicate for future port placement decisions. This is a contract-level improvement — it defines when and why a type lives at a particular boundary.

4. **Pseudo-solutions are correctly rejected.** The analysis of why re-exporting infra types through core, moving ChunkedDecoder to core, and creating god-traits are all wrong demonstrates strong architectural reasoning. These are exactly the patterns that would degrade type safety.

### B. Points of Conflict

1. **M6: `futures-core::Stream` as port return type creates object safety problems.**

   The proposal recommends `Pin<Box<dyn Stream<Item = T> + Send>>` as the return type via an `EventStream<T>` type alias. While this specific formulation IS object-safe (because it returns a boxed trait object, not `impl Stream`), the proposal text conflates two different approaches. The trait signature works:

   ```rust
   pub trait NetworkEventPort: Send + Sync {
       async fn subscribe_events(&self) -> Result<EventStream<NetworkEvent>>;
   }
   ```

   However, the underlying concern is real: `EventStream<T> = Pin<Box<dyn Stream<Item = T> + Send>>` forces a heap allocation per subscription. The current `tokio::sync::mpsc::Receiver<T>` is a concrete type that is `Unpin` and does not require boxing. The migration adds an indirection cost — small, but nonzero — at every event consumption site.

   **Contract concern:** The `Stream` trait has different cancellation semantics than `mpsc::Receiver`. When a `Receiver` is dropped, the corresponding `Sender` is notified (channel closes). When a `Pin<Box<dyn Stream>>` wrapping a `ReceiverStream` is dropped, the same happens, but intermediate stream combinators (`.map()`, `.filter()`) may buffer items. The port contract should document whether dropping the stream is a valid cancellation signal, and whether buffered items are lost. Currently, consumers call `recv().await` directly on the `Receiver`, which has well-defined semantics. The `Stream` abstraction introduces a new contract surface that isn't specified in the proposal.

2. **M5: Aggregate port traits create opaque dependency contracts.**

   The proposed `ClipboardStoragePorts` aggregate trait:

   ```rust
   pub trait ClipboardStoragePorts: Send + Sync {
       fn entries(&self) -> &dyn ClipboardEntryRepositoryPort;
       fn selections(&self) -> &dyn ClipboardSelectionRepositoryPort;
       fn representations(&self) -> &dyn ClipboardRepresentationRepositoryPort;
       fn events_writer(&self) -> &dyn ClipboardEventWriterPort;
   }
   ```

   This design has a contract ambiguity: when a use case depends on `Arc<dyn ClipboardStoragePorts>`, the caller (wiring code) cannot know from the type signature which specific sub-ports the use case actually uses. A use case that only reads entries and selections still receives `events_writer()` in its aggregate. This violates the Interface Segregation Principle that the proposal itself cites.

   **Specific problem:** If `ClipboardEventWriterPort` changes its signature (e.g., adding a new required method), all implementors of `ClipboardStoragePorts` must be updated, even if the use case consuming the aggregate never calls `events_writer()`. The aggregate trait couples unrelated change vectors.

   The proposal says "Use cases can depend on either the aggregate or individual traits depending on their actual needs" — but this creates two parallel dependency paths, making the contract harder to audit. Which path is canonical?

3. **M4: Moving UiPort/AutostartPort to uc-app creates a reverse dependency problem for uc-tauri.**

   Currently, `uc-tauri` provides concrete implementations of `UiPort` and `AutostartPort`. The trait definitions live in `uc-core`, which `uc-tauri` depends on. If the traits move to `uc-app`, then `uc-tauri` must depend on `uc-app` for the trait definitions it needs to implement.

   Checking the current dependency graph: `uc-tauri` already depends on `uc-app` (it imports `AppDeps`, `UseCases`, etc.), so this is not a circular dependency. However, it tightens the coupling between `uc-tauri` and `uc-app` — `uc-tauri` now depends on `uc-app` for both use case orchestration AND port trait definitions. If `uc-app` is ever split into sub-crates, the port traits would need to be separated from use case implementations.

   **Contract risk:** The proposal does not define whether `uc-app/ports/` is a stable public API or an internal module. If a new adapter crate (e.g., `uc-cli` for a CLI frontend) needs to implement `UiPort`, it would depend on `uc-app` just for the trait definition, pulling in use case code as a transitive dependency. This is a layering violation.

### C. Constraints They Missed

1. **StreamDecoderPort error type is unspecified.** The signature uses `Result<Vec<u8>>` which is `anyhow::Result<Vec<u8>>`. This contradicts my Phase 2 proposal (M11) to replace `anyhow::Result` in port signatures with typed errors. The `StreamDecoderPort` should define its own error type (e.g., `StreamDecodeError`) with variants for wire-format errors, decryption failures, and decompression failures. This allows the caller (libp2p adapter) to distinguish between "corrupted data" (drop connection) and "wrong key" (request re-key).

2. **The `ClipboardStoragePorts` aggregate return `&dyn Trait` references, not `Arc<dyn Trait>`.** The accessor methods return `&dyn ClipboardEntryRepositoryPort`, but the underlying fields are `Arc<dyn ...>`. The returned reference borrows from the aggregate, meaning the caller cannot store or clone the sub-port independently. If a use case needs to pass a sub-port to a spawned task, it cannot get an owned `Arc` from the aggregate trait. The proposal should either return `Arc<dyn Trait>` from the accessors (which requires Clone on the aggregate) or document that sub-ports are borrow-only.

3. **H2 Phase B's `CheckEncryptionReadiness` use case has an ambiguous contract.** The proposal says this use case "encapsulates the `encryption_state + encryption_session.is_ready()` check pattern." But the two checks have different semantics: `encryption_state` is persisted (is the system initialized?) while `encryption_session.is_ready()` is transient (is the key currently loaded in memory?). A use case that combines both needs to define what the combined result means. Is it "ready to encrypt"? What if state is `Initialized` but session is not ready (key not yet loaded after reboot)? The contract for this new use case is not specified.

### D. Risk If Adopted As-Is

1. **Aggregate ports create a "two-speed" dependency model.** Some use cases depend on aggregate traits, others on individual traits. Over time, developers will default to the aggregate (less typing, fewer constructor parameters), even when only one sub-port is needed. This gradually makes all use cases depend on all sub-ports within a cluster, re-creating the god-container problem at a finer granularity. The AppDeps field count drops from 30+ to ~8, but each field becomes a 4-5 method trait instead of a single-purpose one.

2. **The `futures-core` migration changes the event consumption contract across all subscribers.** Every consumer of `subscribe_events()` and `subscribe_clipboard()` must change from `receiver.recv().await` to `stream.next().await` (via `StreamExt`). This is a mechanical change but it alters error handling: `recv()` returns `Option<T>` (None = channel closed), while `next()` on a `Stream` also returns `Option<T>` but with potentially different semantics depending on the stream implementation. If the adapter wraps the receiver in a `ReceiverStream`, behavior is identical, but the port contract doesn't mandate which `Stream` implementation is used. A faulty adapter could return a stream that yields `None` without the channel being closed.

3. **Moving ports to uc-app without a stable `uc-app/ports` sub-crate means the port definitions are entangled with use case code in the same crate.** Any consumer of the port traits (uc-tauri adapters) must compile against the entire uc-app crate, increasing compile times and coupling.

### E. Suggested Revisions

1. **For M6 (Stream migration):** Add explicit contract documentation to the `EventStream<T>` type alias specifying that: (a) the stream must yield `None` only when the underlying event source is permanently closed, (b) dropping the stream must signal the producer to stop, and (c) the stream must be fused (no items after first `None`). Consider adding a `FusedStream` bound: `Pin<Box<dyn Stream<Item = T> + FusedStream + Send>>`. This makes the contract self-documenting via the type system.

2. **For M5 (Aggregate ports):** Change the aggregate trait accessors to return `Arc<dyn Trait>` instead of `&dyn Trait`, enabling spawned tasks to hold owned references. Add a lint rule or architectural decision that use cases with 2 or fewer port dependencies should use individual traits, not aggregates. This prevents the "convenience gravity" toward aggregates.

3. **For M4 (UiPort/AutostartPort):** Instead of moving to `uc-app`, consider creating a minimal `uc-app-ports` sub-crate (or a `ports` module in `uc-app` that is feature-gated to compile independently). Alternatively, keep these traits in `uc-core` but in a clearly demarcated `uc-core/ports/app/` subdirectory with documentation that they are "application-level ports co-located in core for dependency convenience." The pragmatic cost of a slightly impure core is lower than the architectural cost of entangling port definitions with use case code.

4. **For H1 (StreamDecoderPort):** Replace `anyhow::Result` with a typed `Result<Vec<u8>, StreamDecodeError>` where `StreamDecodeError` distinguishes corruption from key errors.

---

## Review of: infra-runtime-reviewer's Phase 2 Proposal

### A. Points of Agreement

1. **H8 canonical ownership designation is correct.** Assigning `uc-infra` as the canonical owner for `EncryptionSessionPort` implementation resolves the duplication decisively. The reasoning is sound — encryption sessions are security infrastructure, not platform-specific. The upgrade path (add `Clone` via `Arc<RwLock<>>`) is the right approach; it preserves async safety while matching the usage pattern (shared across multiple consumers).

2. **H9 elimination via H8 is elegant.** Rather than patching the `expect()` calls individually, deleting the entire file that contains them is the correct fix. The 4 `expect()` calls on `std::Mutex::lock()` are eliminated by removing the `std::Mutex`-based implementation entirely. This demonstrates the value of addressing root causes over symptoms.

3. **M10 PlatformRuntime Drop implementation is well-reasoned.** The proposal correctly identifies that `Drop` cannot be async and proposes the right compromise: stop the watcher synchronously (signal-based), log a warning for observability, and rely on CancellationToken (from H7) for coordinated async shutdown. The caveat about `JoinHandle` being non-awaitable in `Drop` is honest and well-documented.

4. **H6 StagedDeviceStore as injected port is the correct fix.** Replacing a global `OnceLock<Mutex<HashMap>>` static with constructor injection is exactly right. The proposal places the trait in `uc-app` (not `uc-core`), correctly identifying that staged device storage is orchestration-level state, not a domain concept.

5. **Pseudo-solutions are properly rejected.** The rejection of `catch_unwind` around `expect()`, `AbortHandle` for shutdown, and "better locking on static" are all well-reasoned from a contract perspective.

### B. Points of Conflict

1. **H6: StagedDeviceStorePort contract is under-specified.**

   The proposed trait:

   ```rust
   pub trait StagedDeviceStorePort: Send + Sync {
       async fn stage(&self, session_id: &str, device: PairedDevice);
       async fn take_by_peer_id(&self, peer_id: &str) -> Option<PairedDevice>;
       async fn get_by_peer_id(&self, peer_id: &str) -> Option<PairedDevice>;
   }
   ```

   **Missing contract elements:**
   - **No error type.** `stage()` returns `()` — what if the store is full, or the session_id is already staged? The current implementation silently overwrites via `HashMap::insert`. The port should define whether overwrite is the intended behavior (document it) or an error.
   - **No transactional semantics.** The current code in `persistence_adapter.rs` (line 35-38) does `get_by_peer_id` then `take_by_peer_id` as separate calls. Between these calls, another task could `take` the same device. The port needs to define atomicity guarantees. Since pairing is a protocol state machine, a `take_and_promote` atomic operation would be safer.
   - **Key type is `session_id: &str` for `stage()` but lookup is by `peer_id`.** This is a semantic mismatch carried over from the current implementation. The store is keyed by session_id but searched by peer_id (requiring a linear scan). The port contract should clarify the primary key and whether peer_id lookup is O(1) or O(n). For correctness, consider a dual-indexed contract or change the primary key.
   - **Missing `clear()` or `remove_by_session_id()`.** The current implementation has `clear()` (test-only), but production code in `persistence_adapter.rs:202` also calls `clear()`. If the port does not expose a removal mechanism, how is cleanup handled after space access completes?

2. **H8: EncryptionSession consolidation port contract is unspecified.**

   The proposal deletes `uc-platform`'s implementation and keeps `uc-infra`'s. But it doesn't address the exact port signature. Looking at the current `EncryptionSessionPort`:

   ```rust
   pub trait EncryptionSessionPort: Send + Sync {
       async fn is_ready(&self) -> bool;
       async fn get_key(&self) -> Result<MasterKey, EncryptionError>;
       async fn set_key(&self, key: MasterKey);
       async fn clear_key(&self);
   }
   ```

   The port is async, and the infra implementation uses `tokio::RwLock`. The platform implementation used `std::Mutex`. The contract question is: does the port NEED to be async? `is_ready()` and `set_key()` are trivial operations on an in-memory value. Making them async adds `.await` overhead at every call site. If the canonical implementation always uses in-memory storage (no disk I/O, no network), synchronous access would be sufficient, and a non-async trait would be simpler.

   However, changing the trait to sync would require all callers in async contexts to handle it differently. The pragmatic answer is to keep it async (as the infra implementation already does), but the proposal should explicitly state: "The EncryptionSessionPort contract is async because future implementations may involve secure enclave access or HSM communication, not because the current in-memory implementation requires it."

3. **H7: TaskRegistryPort in uc-core violates the proposal's own boundary rule.**

   The proposal defines:

   ```rust
   // uc-core/src/ports/lifecycle.rs
   pub trait TaskRegistryPort: Send + Sync {
       fn child_token(&self) -> CancellationToken;
       fn register(&self, name: &str, handle: tokio::task::JoinHandle<()>);
       async fn shutdown(&self, timeout: std::time::Duration);
   }
   ```

   This trait uses `tokio::task::JoinHandle<()>` and `CancellationToken` (from `tokio_util`) — both Tokio-specific types. Placing this in `uc-core/ports/` directly contradicts the arch-guardian's boundary rule #1: "uc-core has ZERO workspace crate dependencies, only external crates allowed: serde, async-trait, anyhow, chrono, futures-core, uuid, thiserror. No tokio."

   The proposal's own "Boundaries to Protect" section (point 1) acknowledges this concern but offers a hand-wavy revision: "Define a minimal ShutdownSignal trait in uc-core if needed, but prefer keeping lifecycle management entirely in uc-tauri and uc-platform." This self-contradiction needs resolution. The `TaskRegistryPort` should NOT be in `uc-core`. It should be in `uc-tauri` (composition root) since task lifecycle is a runtime concern, not a domain concern.

4. **M8: run_app decomposition's `CompositionResult` is a type-contract concern.**

   The proposed `CompositionResult` struct:

   ```rust
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
   ```

   This struct has 9 fields with mixed ownership semantics — some are `Arc` (shared), some are channels (single-owner, move-only). The `platform_event_rx` and `platform_cmd_rx` are receivers that can only be consumed once. After `compose_application()` returns, the caller must carefully move these receivers into the right places. If a receiver is accidentally used twice (cloned or borrowed), it's a logic error that the type system doesn't prevent.

   **Contract problem:** The struct combines "things that are shared" (Arc<AppRuntime>) with "things that are moved once" (receivers). This violates the principle that a type's contract should be uniform. Consider splitting into `SharedState` (Arc'd values) and `OwnedChannels` (move-only receivers), or using a builder that consumes the channels on use.

   More fundamentally: the decomposition moves the same 370 lines of code into 3 functions with the same total parameter surface. The `CompositionResult` struct IS the 39-field `AppDeps` plus orchestrators plus channels — it's a larger god container than `AppDeps` itself. The proposal adds a function call boundary without reducing the contract complexity.

### C. Constraints They Missed

1. **H6: The StagedDeviceStore has a concurrency contract with the pairing state machine.** The current `Mutex<HashMap>` is synchronous and blocking (via `std::Mutex`). The proposal replaces it with `tokio::sync::RwLock<HashMap>`. But the pairing protocol has a specific ordering requirement: `stage()` must happen-before `take_by_peer_id()` for the same device. The async `RwLock` doesn't guarantee FIFO ordering — under contention, a `take` could execute before a concurrent `stage` completes. The port contract needs to specify the happens-before relationship, or the implementation needs to ensure it (e.g., via a sequenced channel rather than a shared map).

2. **H7: CancellationToken threading changes the shutdown contract for ALL spawned tasks.** Currently, tasks run until the process exits. After the reform, tasks must cooperatively check for cancellation via `token.cancelled().await` in a `select!` block. Any task that forgets to check the token will block shutdown. The proposal identifies 20+ spawn sites but doesn't define the contract for what "cooperative cancellation" means: must each task complete its current operation before exiting? Can it drop in-progress work? What about tasks holding database transactions? The shutdown contract is unspecified.

3. **M8: The `compose_application` function's error type is unspecified.** It returns `Result<CompositionResult>` which is `anyhow::Result`. Composition errors (missing config, database connection failure, key store initialization failure) should be typed so that `run_app` can provide specific error messages to the user. A `CompositionError` enum would be appropriate.

### D. Risk If Adopted As-Is

1. **StagedDeviceStorePort without transactional semantics could introduce race conditions in the pairing flow.** The current global static actually avoids certain races because `std::Mutex::lock()` provides exclusive access. The async `RwLock` read/write split means `get_by_peer_id` (read) and `take_by_peer_id` (write) can interleave differently. If two space-access flows query the same peer_id concurrently, one gets the device and the other gets `None` — but the proposal doesn't define which wins or how the loser should recover.

2. **TaskRegistryPort in uc-core would add tokio types to the domain layer.** This directly contradicts the M6 fix (removing tokio from core ports). If both H7 and M6 are adopted as-is, uc-core would simultaneously be removing `tokio::sync::mpsc::Receiver` from port signatures (M6) while adding `tokio::task::JoinHandle` to a new port signature (H7). This is contradictory.

3. **CompositionResult with mixed-ownership fields creates a fragile API.** If any future consumer needs to clone `CompositionResult` (e.g., for testing), they can't — the receivers are not Clone. If any field is removed or added, all callers of `compose_application` break. The struct has no stability guarantee and serves only as an ad-hoc tuple.

### E. Suggested Revisions

1. **For H6 (StagedDeviceStore):** Add error return types to `stage()`. Add `take_and_promote()` as an atomic operation that combines lookup, removal, and state transition. Define the concurrency contract explicitly: "Implementations must ensure that a `stage()` followed by `take_by_peer_id()` from the same logical flow always succeeds if no other flow has taken the device." Consider whether the store should return `Result<PairedDevice, StagedDeviceError>` instead of `Option<PairedDevice>`.

2. **For H7 (TaskRegistry):** Move `TaskRegistryPort` out of `uc-core`. Place it in `uc-tauri/src/bootstrap/` as a concrete struct (not a port trait). It is a composition-root concern, not a domain port. Core and app layers should receive a simple `CancellationToken` (or an abstract `ShutdownSignal` trait with a single `async fn cancelled()` method) rather than the full registry.

3. **For H8 (EncryptionSession):** Add a doc comment to the port trait explaining why the contract is async. Specify that implementations must be infallible for `is_ready()` and `set_key()` (these should never return errors under normal operation — if they do, it indicates a bug, not a recoverable error).

4. **For M8 (run_app decomposition):** Split `CompositionResult` into `SharedServices` (Arc'd values) and consume channels directly in the setup function rather than passing them through a struct. Alternatively, use a builder pattern where `compose_application()` returns a builder that is consumed by `build_tauri_app()`:

   ```rust
   let composition = compose_application(&config)?;
   // composition.into_tauri_app() consumes the receivers
   composition.build_tauri_app()?.run();
   ```

   This makes the ownership transfer explicit in the type system.

---

## Cross-Cutting Observations

### 1. anyhow::Result proliferation in new port proposals

Both the arch-guardian (StreamDecoderPort) and infra-runtime-reviewer (StagedDeviceStorePort, TaskRegistryPort) define new ports using `anyhow::Result` or `Result<T>` without specifying error types. This directly conflicts with my Phase 2 proposal (M11) to replace `anyhow::Result` in port signatures with typed errors. If we are going to reform the error model, ALL new ports introduced during the reform should use typed errors from day one. Adding new `anyhow::Result` ports while simultaneously trying to migrate existing ports away from `anyhow::Result` is contradictory.

**Recommendation:** Establish a rule that no new port trait may use `anyhow::Result`. Every new port introduced as part of the reform must define its own error type, even if it starts as a simple enum with 2-3 variants.

### 2. Aggregate traits vs. typed error boundaries

The arch-guardian's aggregate port proposal (ClipboardStoragePorts) groups ports that may have DIFFERENT error types. Currently, each clipboard repository port uses `anyhow::Result`, so this isn't a problem yet. But after M11 migration, `ClipboardEntryRepositoryPort` might return `Result<..., ClipboardRepositoryError>` while `ClipboardEventWriterPort` returns `Result<..., EventWriteError>`. The aggregate trait's accessor methods return `&dyn Trait`, preserving the individual error types. This is good — but it means the aggregate doesn't actually reduce the error handling complexity for callers. They still need to handle different error types per sub-port.

### 3. Contract documentation is universally missing

Both proposals (and all five Phase 2 proposals, including mine) define new traits and types without formal contract documentation. No proposal specifies preconditions, postconditions, or invariants for its new abstractions. Rust's type system catches many contract violations at compile time, but semantic contracts (ordering, atomicity, cancellation behavior) require documentation. I recommend that every new port trait introduced in the reform includes:

- A doc comment on the trait itself stating its semantic contract
- A doc comment on each method stating preconditions and postconditions
- Where applicable, a note on concurrency guarantees (thread-safe? async-safe? ordering?)

### 4. Tension between removing tokio from uc-core (M6) and adding lifecycle management (H7)

The arch-guardian wants to remove `tokio` from `uc-core` by replacing `mpsc::Receiver` with `futures-core::Stream`. The infra-runtime-reviewer wants to add a `TaskRegistryPort` to `uc-core` that uses `tokio::task::JoinHandle` and `CancellationToken`. These two proposals directly conflict on the question of whether `uc-core` should contain Tokio types. The resolution should be: uc-core is Tokio-free (arch-guardian wins), and lifecycle management stays in the composition root (uc-tauri) or uses a minimal abstract signal trait in uc-core that has no Tokio dependency.
