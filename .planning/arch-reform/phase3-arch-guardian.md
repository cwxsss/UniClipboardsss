# Architecture Guardian — Phase 3 Cross-Review

## Review of: app-flow-reviewer's Phase 2 Proposal

### A. Points of Agreement

1. **SetupOrchestrator decomposition is directionally correct.** The 860-line, 15-dependency god object is the single worst ISP violation in the application layer. Decomposing the _execution_ role from the _state machine driver_ role respects the Single Responsibility Principle and aligns with the hexagonal intent — orchestrators coordinate, they don't implement.

2. **SyncInbound decomposition into InboundPayloadDecoder / InboundDeduplicator / InboundClipboardApplier is well-reasoned.** Each extracted component has a clear input/output contract and is independently testable. The separation of _decode_, _dedup_, and _apply_ phases maps to natural transaction boundaries in the inbound clipboard flow. This is a proper phase decomposition, not an arbitrary split.

3. **SyncOutbound's `executor::block_on` and `tokio::runtime::Handle::try_current()` must be removed.** Use cases should be async-pure. Runtime detection branching is an infrastructure concern that violates the principle that use cases are runtime-agnostic. This is one of the clearest boundary violations in the codebase.

4. **M2 (Pairing responsibility crossover) fix is clean and surgical.** Routing all pairing session operations through `PairingOrchestrator` and removing `pairing_transport` from `SetupOrchestrator` is the correct way to eliminate split-brain risk. Single PR, clear ownership boundary.

5. **M3 domain model enrichment is conservative and correct.** The proposed predicates (`is_stale`, `has_title`, `is_from_device`) are pure functions over the model's own fields with no external dependencies. This is the textbook definition of behavior that belongs on the domain model.

### B. Points of Conflict

**B1. SetupOrchestrator "7 action executors" — crate placement and wiring responsibility are unspecified.**

The proposal says `SetupActionExecutor` trait lives in `uc-app`, and the 7 executor structs (CreateSpaceExecutor, PairingExecutor, etc.) are listed with their owned dependencies, but it never states:

- Where do the executor _structs_ live? If they are in `uc-app` alongside the orchestrator, they need `uc-core` port traits — which is fine. But `SpaceAccessExecutor` "owns: SpaceAccessOrchestrator, crypto/transport/proof/timer/persistence ports" — that is 6+ dependencies on a single executor, which is itself approaching mini-god-object territory.
- Who constructs and wires the executors? Currently, `UseCases` constructs `SetupOrchestrator` with 14 params. After decomposition, someone must construct 7 executors AND the orchestrator. If `UseCases` does this, the factory method balloons. If the orchestrator does this, it needs access to the raw ports again.

This decomposition risks **moving the god object one level down** rather than eliminating it, because the _wiring complexity_ is not reduced — it is redistributed.

**B2. AppDeps port bundles (`ClipboardPorts`, `SecurityPorts`, etc.) — ISP violation is restructured, not resolved.**

The proposal groups 39 fields into ~11 bundles, then says "UseCase factory methods should only receive the relevant port bundle." But the _actual change_ shown is:

```rust
// Before:
self.runtime.deps.clipboard_entry_repo.clone()
// After:
self.runtime.deps.clipboard.entry_repo.clone()
```

This is a namespace change, not an ISP fix. The `UseCases` struct still holds `&AppRuntime`, which holds `AppDeps`, which holds _all_ bundles. A use case factory method can still reach `self.runtime.deps.security.encryption.clone()` even if it only needs clipboard ports. The ISP violation remains — it is just two levels of dot-navigation deeper.

To actually fix ISP, use case constructors must accept `&ClipboardPorts` (not `&AppDeps`), and the `UseCases` factory must only pass the relevant bundle. The proposal hints at this ("UseCase factory methods should only receive the relevant port bundle") but the code example contradicts it by accessing `self.runtime.deps.clipboard.entry_repo` — still going through the full `AppDeps`.

**B3. `OutboundPayloadPreparerPort` and `ClipboardFanoutPort` — port placement inconsistency.**

The abstractions-to-add table says:

- `OutboundPayloadPreparerPort`: port in `uc-core`, impl in `uc-infra`
- `ClipboardFanoutPort`: port in `uc-core`, impl in `uc-platform`

But `ClipboardFanoutPort` described as handling "peer iteration, ensure_business_path, and send logic" — this is transport-layer behavior. Is "which peers to send to" a core domain concern or a platform/transport concern? If the port is in `uc-core`, it means the core domain knows about `DiscoveredPeer` and `FanoutResult` as types. `DiscoveredPeer` already exists in `uc-core`, so this is acceptable, but `FanoutResult` is a new type that must also be defined in `uc-core`. The proposal doesn't specify this.

### C. Constraints They Missed

**C1. Executor wiring must not introduce new `uc-app` → `uc-infra` dependencies.**

The proposal does not mention the dependency constraint for executor construction. If executors are constructed in `uc-tauri` (composition root) and passed as `Arc<dyn SetupActionExecutor>` to the orchestrator, this is fine. But if executors are constructed inside `uc-app` using port implementations, this would require `uc-app` to import concrete types from `uc-infra` — violating the dependency rule.

Currently `uc-app/Cargo.toml` has `uc-infra` only in `[dev-dependencies]`. This MUST remain the case after the SetupOrchestrator decomposition. The proposal should explicitly state that executor construction happens in `uc-tauri`.

**C2. Port bundle types create a layering question: where do they live?**

The proposal places port bundles (`ClipboardPorts`, `SecurityPorts`, etc.) in `uc-app`. But these are just groupings of `uc-core` port traits. If they are in `uc-app`, then `uc-tauri` (which constructs them) depends on `uc-app` for the struct definitions — this is already the case for `AppDeps`, so it works. However, the arch-guardian's Phase 2 proposal (my own) proposes `ClipboardStoragePorts` as an _aggregate trait_ in `uc-core`, not a struct in `uc-app`. These two approaches need reconciliation:

- **app-flow**: Plain structs with `pub` fields in `uc-app` (concrete bundles)
- **arch-guardian**: Aggregate traits in `uc-core` (abstract bundles)

If both are implemented, we get _both_ a trait in `uc-core` and a struct in `uc-app` for the same concept — unnecessary duplication.

**C3. `InboundPayloadDecoder` and `InboundClipboardApplier` are described as "domain services" and "services" but have no port trait.**

The proposal places these in `uc-app` as concrete types, not behind ports. This means they cannot be mocked independently in tests without the testability reviewer's `uc-test-support` crate having a dependency on `uc-app` (to import the concrete types). This conflicts with the testability proposal's boundary rule that `uc-test-support` depends only on `uc-core`.

### D. Risk If Adopted As-Is

1. **SetupOrchestrator decomposition without explicit wiring strategy will produce a "distributed god object"** — 7 executors, each with 2-6 dependencies, all constructed in one factory method that still needs all 15 original dependencies available. The total coupling is unchanged; it is just spread across more types.

2. **AppDeps port bundles without enforcing that use cases accept bundles (not AppDeps) will give the illusion of ISP compliance** while every use case retains full access to all ports through `self.runtime.deps`. This is a namespace refactor, not an architectural fix.

3. **M3 model enrichment without guard rails risks gradual scope creep.** The proposed predicates are safe, but the pattern — "add methods to domain models" — could be misapplied by future developers who add predicates that need infrastructure context (e.g., `is_synced()` requiring a network check). The proposal says "What NOT to do" but provides no compile-time enforcement.

### E. Suggested Revisions

**E1. Specify that executor structs live in `uc-app` and are constructed in `uc-tauri`.**

Add to the SetupOrchestrator proposal:

- Executor structs: `uc-app/src/usecases/setup/executors/{create_space,mark_complete,...}.rs`
- Executor trait: `uc-app/src/usecases/setup/executor_trait.rs`
- Construction: `uc-tauri/src/bootstrap/wiring.rs` constructs each executor with its required ports, wraps in `Arc<dyn SetupActionExecutor>`, and passes a `Vec<(SetupAction, Arc<dyn SetupActionExecutor>)>` to `SetupOrchestrator::new()`.
- `SetupOrchestrator::new()` signature changes from 15 raw ports to: `(context: SetupContext, event_port: Arc<dyn SetupEventPort>, status_port: Arc<dyn SetupStatusPort>, executors: HashMap<SetupActionKind, Arc<dyn SetupActionExecutor>>)`.

**E2. Enforce ISP at the type level, not just the naming level.**

Change the `UseCases` factory pattern from:

```rust
pub fn list_clipboard_entries(&self) -> ListClipboardEntries {
    ListClipboardEntries::from_arc(self.runtime.deps.clipboard.entry_repo.clone())
}
```

To:

```rust
pub fn list_clipboard_entries(&self) -> ListClipboardEntries {
    ListClipboardEntries::new(&self.runtime.deps.clipboard)
}
// where ListClipboardEntries stores &ClipboardPorts, not individual Arc<dyn ...>
```

And critically, `ListClipboardEntries` should accept `&ClipboardPorts` (or `Arc<ClipboardPorts>`), not `&AppDeps`. This makes the ISP constraint type-checked: the use case literally cannot access `SecurityPorts` because it never receives them.

**E3. Reconcile port bundle approach with arch-guardian's aggregate trait approach.**

Choose ONE approach:

- **Option A**: Aggregate traits in `uc-core` (arch-guardian's approach) — use cases depend on `dyn ClipboardStoragePorts`, composition root implements the trait.
- **Option B**: Concrete structs in `uc-app` (app-flow's approach) — use cases depend on `ClipboardPorts` struct, composition root constructs the struct.

Recommendation: **Option B** is simpler and more Rust-idiomatic (structs with pub fields avoid the vtable overhead of trait objects). Reserve aggregate traits for cases where multiple implementations are needed. Update the arch-guardian proposal (my own) to align.

---

## Review of: infra-runtime-reviewer's Phase 2 Proposal

### A. Points of Agreement

1. **H6 (global static elimination) is critical and the proposed fix is correct.** The `OnceLock<Mutex<HashMap<...>>>` pattern is the antithesis of dependency injection. Defining `StagedDeviceStorePort` as an injected trait and constructing a single `Arc<dyn StagedDeviceStorePort>` in the composition root is the textbook hexagonal fix. The async `RwLock` over `HashMap` implementation is appropriate given the concurrent access pattern (pairing writes, space-access reads).

2. **H8 (duplicate EncryptionSession) fix is straightforward.** Consolidating to `uc-infra`'s `tokio::RwLock` version and deleting `uc-platform`'s `std::Mutex` version eliminates the panic risk from `expect()` and removes the duplicate. The temporary re-export alias strategy is pragmatic for migration.

3. **H9 (expect in production) is correctly identified and the fixes are surgical.** Eliminating the encryption.rs file (via H8) handles 4 of 5 occurrences. The `main.rs` `.expect()` → `error!() + process::exit(1)` is the right pattern for fatal startup errors.

4. **M7 (uc-infra/clipboard reorganization) is a clean refactor.** The spool/blob/transform/transfer grouping reflects natural responsibility boundaries. The constraint that public API (re-exports in mod.rs) remains unchanged makes this a safe, mechanical refactor.

5. **M8 (run_app decomposition) is well-structured.** The `CompositionResult` struct cleanly separates "what was created" from "how it is used." The 3-phase extraction (compose → build_tauri → setup_callback) produces testable intermediates.

### B. Points of Conflict

**B1. `StagedDeviceStorePort` placed in `uc-app` — but the port is used by `SpaceAccessPersistenceAdapter` which may live in `uc-platform` or `uc-infra`.**

The proposal says the port is defined in `uc-app/src/usecases/pairing/staged_device_store.rs`. But `SpaceAccessPersistenceAdapter` is described as a consumer. Where does this adapter live?

- If in `uc-app`: Fine, `uc-app` defines and consumes its own port.
- If in `uc-platform`: `uc-platform` would need to import a trait from `uc-app`. But `uc-platform` does NOT depend on `uc-app` (and must not — this would create a circular dependency since `uc-app` → `uc-core` and `uc-platform` → `uc-core`, and `uc-tauri` → both). This is a **dependency violation**.
- If in `uc-infra`: Same problem — `uc-infra` does not depend on `uc-app`.

The proposal must clarify the adapter's crate location. If `SpaceAccessPersistenceAdapter` lives outside `uc-app`, the port must be defined in `uc-core` (the shared dependency), which raises the question: does "staged pairing device" belong in the core domain?

**B2. `TaskRegistryPort` in `uc-core/ports/lifecycle.rs` uses `tokio::task::JoinHandle<()>` and `tokio_util::sync::CancellationToken` in the trait signature.**

The proposal shows:

```rust
pub trait TaskRegistryPort: Send + Sync {
    fn child_token(&self) -> CancellationToken;
    fn register(&self, name: &str, handle: tokio::task::JoinHandle<()>);
    async fn shutdown(&self, timeout: std::time::Duration);
}
```

This directly contradicts the boundary invariant from the arch-guardian's Phase 2: **"uc-core has ZERO workspace crate dependencies. Only external crates allowed: `serde`, `async-trait`, `anyhow`, `chrono`, `futures-core`, `uuid`, `thiserror`. No `tokio`, no `uc-*`."**

The trait uses `CancellationToken` from `tokio-util` and `JoinHandle` from `tokio`. Both are tokio-ecosystem types. This is exactly the M6 problem (tokio in core ports) that the arch-guardian proposal aims to eliminate — and the infra-runtime proposal _reintroduces_ it via a new port.

The proposal does self-correct in the "Boundaries to Protect" section: "No new infrastructure types (CancellationToken, JoinHandle) should leak into `uc-core` port definitions... prefer keeping lifecycle management entirely in `uc-tauri` (composition root) and `uc-platform` (runtime)." But the code example contradicts this boundary statement. The two are inconsistent.

**B3. EncryptionSession consolidation to uc-infra — uc-platform currently imports it. After consolidation, what changes?**

Currently `uc-platform` has its own `InMemoryEncryptionSessionPort` and does not import from `uc-infra`. After H8:

- `uc-platform` deletes its local implementation.
- `uc-platform` receives `Arc<dyn EncryptionSessionPort>` via constructor injection from `uc-tauri`.
- `uc-platform` does NOT need to import `uc-infra` — it depends only on the `EncryptionSessionPort` trait from `uc-core`.

This is correct IF the wiring in `uc-tauri` constructs the `uc-infra` implementation and passes it through. The proposal's temporary re-export strategy (`pub use uc_infra::security::InMemoryEncryptionSession`) would make `uc-platform` depend on `uc-infra` at the import level, even if only transitively. This re-export should be in `uc-tauri`, not in `uc-platform`.

**Correction**: The re-export in the proposal is placed in `uc-platform/src/adapters/encryption.rs` — this means `uc-platform` now imports from `uc-infra` during the transition period, which is the H1 violation (uc-platform → uc-infra) that the arch-guardian's H1 fix aims to remove.

### C. Constraints They Missed

**C1. CancellationToken threading into PlatformRuntime changes the adapter's initialization interface.**

Phase 1 of H7 says: "Pass [CancellationToken] to `PlatformRuntime`, `start_background_tasks`, and the main init spawn."

`PlatformRuntime` is defined in `uc-platform`. If its constructor or `start()` method gains a `CancellationToken` parameter, this is a `tokio_util` type in `uc-platform`'s public API. While `uc-platform` already depends on `tokio` and `tokio-util`, the question is: should the _platform abstraction layer_ be coupled to tokio's specific cancellation mechanism?

If we later want to run platform adapters in a non-tokio context (e.g., testing with a synchronous executor), the `CancellationToken` parameter forces tokio-util as a dependency even in that context. A more portable approach would be to accept a `Box<dyn Fn() -> bool + Send + Sync>` (a shutdown-check closure) or a `futures::future::Abortable` wrapper.

However, pragmatically, `uc-platform` already deeply depends on tokio (libp2p requires it), so this is a theoretical concern. The real risk is if the `CancellationToken` appears in `uc-core` port trait signatures.

**C2. The M10 PlatformRuntime Drop implementation is correct but incomplete without CancellationToken.**

The `Drop` impl calls `handle.stop()` on the watcher, but other resources (`event_tx`, `command_rx` channels) are just dropped, causing channel closures that may or may not be handled by receivers. If the platform runtime is dropped unexpectedly (e.g., task abort), channel closures without a cancellation signal can cause "channel closed" errors in other tasks that are still running.

The proposal acknowledges this ("For proper async cleanup, PlatformRuntime should also accept a CancellationToken") but does not make it a hard requirement. It should be — Drop without CancellationToken is a partial fix that can produce confusing error messages in other tasks.

**C3. `compose_application()` return type `CompositionResult` contains concrete types from multiple crates.**

The struct includes `PairingOrchestrator` (from `uc-app`), `SpaceAccessOrchestrator` (from `uc-app`), `KeySlotStore` (from `uc-infra`), channel types (from `uc-platform`). This means `CompositionResult` must be defined in a crate that depends on all of them — which is `uc-tauri`. This is fine, but the proposal doesn't explicitly state this. If someone tries to define `CompositionResult` in `uc-app` or `uc-core`, it would create illegal cross-crate dependencies.

### D. Risk If Adopted As-Is

1. **`TaskRegistryPort` in `uc-core` would deepen the tokio coupling** that the M6 fix (from the arch-guardian proposal) aims to remove. This is the most critical conflict between the two proposals: the infra-runtime proposal adds tokio types to core at the same time the arch-guardian proposal removes them. If both are implemented sequentially, the second will undo part of the first.

2. **The temporary EncryptionSession re-export in `uc-platform` creates a new H1 violation** during the transition. If the transition takes multiple sprints, this "temporary" `use uc_infra::*` in `uc-platform` becomes a regression in the dependency topology.

3. **H7's CancellationToken approach without a clear `uc-core` boundary rule will lead to token types leaking into port signatures over time.** The proposal says "keep lifecycle management in uc-tauri and uc-platform" but provides no enforcement mechanism. Without a CI check, future developers will add `CancellationToken` to port traits when it's convenient.

### E. Suggested Revisions

**E1. Move `TaskRegistryPort` out of `uc-core` entirely. Place it in `uc-tauri/src/bootstrap/`.**

Task lifecycle management is a composition-root concern, not a domain concern. The trait does not need to be in `uc-core` because:

- Use cases do not spawn tasks (they delegate to ports for async operations).
- Orchestrators that spawn tasks live in `uc-app` but can receive a `Spawner` from the composition root.
- The registry itself is only queried at shutdown, which happens in `uc-tauri`.

If `uc-app` orchestrators need to spawn tasks with cancellation, define a minimal spawner trait:

```rust
// uc-core/src/ports/spawner.rs (optional, only if needed)
pub trait SpawnerPort: Send + Sync {
    fn spawn(&self, name: &str, future: Pin<Box<dyn Future<Output = ()> + Send>>);
}
```

This is runtime-agnostic (no tokio types). The `uc-tauri` implementation can internally use `CancellationToken` + `JoinHandle` tracking.

**E2. For the EncryptionSession migration, skip the re-export step. Do a direct migration.**

Instead of the temporary `pub use uc_infra::*` re-export in `uc-platform`:

1. Update `uc-tauri/src/bootstrap/wiring.rs` to construct `uc_infra::security::InMemoryEncryptionSession`.
2. Pass it as `Arc<dyn EncryptionSessionPort>` to all consumers (both in `uc-infra` and `uc-platform`).
3. Delete `uc-platform/src/adapters/encryption.rs`.
4. Done in one PR. No temporary re-export, no transitional H1 violation.

The re-export strategy adds risk for no benefit — the migration is small enough to do atomically.

**E3. Make M10 (PlatformRuntime Drop) contingent on H7 Phase 1 (CancellationToken threading).**

Do not implement Drop alone. Implement them together:

1. Add `CancellationToken` field to `PlatformRuntime`.
2. In `Drop`, cancel the token (this signals all child tasks).
3. In `start()`, select on `self.token.cancelled()`.

This ensures Drop is not a partial fix that introduces "channel closed" error noise.

**E4. Explicitly state that `CompositionResult` lives in `uc-tauri/src/bootstrap/composition.rs`.**

Add a note that this struct cannot be defined in any crate other than `uc-tauri` because it holds types from multiple downstream crates.

---

## Cross-Cutting Observations

### Observation 1: Port Bundle Approach Conflict

The app-flow proposal and the arch-guardian proposal (my own Phase 2) propose different mechanisms for port grouping:

- **App-flow**: Concrete structs (`ClipboardPorts`) with `pub` fields in `uc-app`
- **Arch-guardian**: Aggregate traits (`ClipboardStoragePorts`) with accessor methods in `uc-core`

These must be reconciled before implementation. Having both creates redundant abstractions. Recommendation: Use concrete structs (app-flow approach) for simplicity, and only introduce aggregate traits where polymorphism is genuinely needed.

### Observation 2: Tokio Coupling Direction Conflict

The arch-guardian proposal (M6) removes tokio from `uc-core` port signatures. The infra-runtime proposal (H7) adds `CancellationToken` and `JoinHandle` to a new `uc-core` port trait. These are directly contradictory. Resolution: Keep all tokio-specific lifecycle management in `uc-tauri` and `uc-platform`, never in `uc-core`. The `TaskRegistryPort` trait either moves out of `uc-core` or uses runtime-agnostic types.

### Observation 3: Transition State Management

Both proposals have transition strategies that temporarily introduce violations:

- **App-flow**: "Keep flat accessor methods as deprecated compatibility shims during migration" — acceptable, no cross-crate violation.
- **Infra-runtime**: "Keep uc-platform file as a re-export alias during transition" — introduces a new uc-platform → uc-infra import, which is a cross-crate violation.

The standard must be: **transition strategies must not introduce new cross-crate dependency violations**, even temporarily. If a transition requires a temporary violation, the scope must be a single PR (atomic migration), not a multi-sprint "temporary" state.

### Observation 4: Shared Assumption — "uc-app depends only on uc-core"

Both proposals assume `uc-app` depends only on `uc-core` at the Cargo.toml level. However, the actual `uc-app/Cargo.toml` already has `uc-infra` as a **dev-dependency** (for tests). This is acceptable (dev-deps don't affect production builds), but both proposals should acknowledge this when discussing test-time access patterns.

The app-flow proposal's `StagedDeviceStorePort` in `uc-app` works if all consumers are in `uc-app`. But the infra-runtime proposal says `SpaceAccessPersistenceAdapter` also consumes it — if that adapter is outside `uc-app`, the port must move to `uc-core`. Neither proposal coordinates on where `SpaceAccessPersistenceAdapter` lives, and this is a cross-cutting concern that must be resolved jointly.
