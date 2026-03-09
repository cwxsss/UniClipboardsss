# Infra & Runtime Reviewer — Phase 3 Cross-Review

## Review of: app-flow-reviewer's Phase 2 Proposal

### A. Points of Agreement

1. **SetupOrchestrator decomposition is directionally correct.** From a runtime perspective, a 15-dependency god object is a lifecycle management nightmare. If SetupOrchestrator panics mid-setup, all 15 dependencies must be in a consistent state for recovery. Decomposing into focused executors reduces the blast radius of individual failures. The phased migration approach (move mutable state first, then extract executors) is sound because it maintains runtime invariants at each step.

2. **SyncOutbound's `execute()` must be purely async.** The `executor::block_on()` call inside a UseCase is a runtime-layer concern leaking into the application layer. If `block_on()` is called from within an existing Tokio runtime, it will panic. The proposal correctly identifies that the caller — not the UseCase — should handle async-to-sync bridging. This aligns with the infra-runtime principle that runtime detection belongs in infrastructure/platform code.

3. **Removing `tokio::runtime::Handle::try_current()` from SyncOutbound.** Runtime detection is an infrastructure concern. The current two-branch pattern (spawn if runtime exists, join if not) is fragile and creates different concurrency profiles depending on the execution context. A UseCase should have a single, predictable execution model.

4. **Port bundle approach for AppDeps.** Grouping ports by domain reduces the cognitive overhead of wiring and makes it easier to reason about shutdown order. When `SecurityPorts` is a single unit, we can ensure all security-related resources are initialized together and torn down together.

5. **M2 (Setup/Pairing responsibility crossover) fix.** Routing all pairing session operations through PairingOrchestrator eliminates split-brain risk. From a lifecycle perspective, having a single owner for session state makes cancellation and cleanup well-defined.

### B. Points of Conflict

1. **SetupOrchestrator decomposition into 7 action executors — lifecycle ownership is unspecified.**

   The proposal states SetupOrchestrator becomes a "thin dispatcher" holding `Arc<dyn SetupActionExecutor>` instances. But the current SetupOrchestrator holds long-lived mutable state (selected_peer_id, pairing_session_id, passphrase, joiner_offer) and spawns async tasks (orchestrator.rs:478, 761). Critical questions:
   - **Who manages task lifecycle for each executor?** If CreateSpaceExecutor spawns an async task that outlives the executor's `execute()` call, the task is orphaned. The current SetupOrchestrator at least implicitly owns these tasks via `&self` lifetime. Decomposition severs this ownership.
   - **CancellationToken propagation:** The proposal does not mention cancellation. Each executor will need a `CancellationToken` (from H7's fix) to cancel its spawned tasks. Does SetupContext carry the token? Does each executor get its own child token? If one executor panics, are sibling executors cancelled?
   - **Error recovery across executors:** If PairingExecutor fails mid-execution, SpaceAccessExecutor may hold stale state from a previous successful pairing. The dispatcher must handle rollback across executors, which re-introduces coordination logic into the "thin" dispatcher.

2. **SyncInbound decomposition into 3 services — shared mutable state management is unaddressed.**

   The proposal extracts `InboundDeduplicator` which owns `recent_ids` state, and `InboundClipboardApplier` which interacts with `clipboard_change_origin` (also mutable). In the current monolithic implementation, these share the same `&self` scope, so lifetime and ordering are implicit.

   After decomposition:
   - `InboundDeduplicator` likely needs `Arc<RwLock<HashSet<MessageId>>>` for the recent_ids cache. The proposal does not specify this.
   - If `InboundDeduplicator::record()` is called but the subsequent `InboundClipboardApplier::apply()` fails, the deduplicator must `rollback()`. The proposal mentions `rollback()` as a method but does not specify who calls it or what happens if rollback itself fails.
   - These three services are called sequentially in a pipeline, but concurrency is possible if multiple inbound messages arrive simultaneously. The concurrency model (serial processing? per-message parallelism? bounded concurrency?) is not specified.

3. **Port bundle structs — memory overhead and shutdown ordering.**

   The proposal defines bundles as structs containing `Arc<dyn Port>` references (e.g., `ClipboardPorts` with 12 `Arc` fields). Each `Arc` is 8 bytes (pointer) + reference count overhead.
   - **Memory overhead** is negligible for 39 `Arc`s, so this is not a real concern.
   - **Shutdown ordering** IS a concern. If `AppDeps` holds `ClipboardPorts` and `SecurityPorts` as nested structs, when `AppDeps` is dropped, Rust drops fields in declaration order. If `ClipboardPorts` contains ports that depend on `SecurityPorts` (e.g., encrypted clipboard repos that hold a reference to the encryption session), dropping `ClipboardPorts` first may trigger use-after-drop on the encryption session Arc. This is safe due to `Arc` reference counting (the encryption session won't be deallocated until all Arcs are dropped), but it means cleanup callbacks or `Drop` impls in individual ports may execute in an unexpected order.
   - The proposal does not specify whether bundles are `Clone`. If bundles are `Arc<ClipboardPorts>` (shared), modifying a bundle's contents requires interior mutability. If they are owned structs within `AppDeps`, each `UseCases` factory method that needs the bundle must borrow from `AppDeps`, creating lifetime coupling.

### C. Constraints They Missed

1. **Async task lifecycle in extracted executors.** The proposal treats executor extraction as a pure structural refactor, but SetupOrchestrator currently spawns `tokio::spawn` tasks that run concurrently with the state machine. After extraction, each executor's spawned tasks must be tracked (via H7's TaskRegistry) and cancelled on executor disposal. This is a hard runtime constraint that must be addressed before the refactor, not after.

2. **InboundPayloadDecoder runs decryption, which is CPU-intensive.** The current monolithic implementation runs decryption inline on the Tokio runtime. After extraction into a separate service, there is an opportunity (and arguably a requirement) to run decryption via `spawn_blocking` to avoid starving the Tokio worker pool. The proposal does not mention this, but it directly affects runtime performance.

3. **OutboundPayloadPreparerPort and ClipboardFanoutPort both handle `Arc<[u8]>` payloads that can be large (multi-MB images).** The proposal does not specify memory lifetime for these payloads. If `prepare()` returns an `Arc<[u8]>` and `fanout()` sends it to N peers, the allocation lives until the last send completes. For large payloads with many peers, this is a memory pressure point that the port abstraction should document.

4. **SyncOutbound's `execute()` becoming purely async has a caller-side impact.** The proposal says "move block_on to callers" but the primary caller is the `ClipboardChangeHandler` callback, which is invoked from the platform layer's watcher. If the watcher runs on a non-Tokio thread (e.g., macOS NSWorkspace notification thread), the caller cannot simply `.await` the UseCase. The proposal must specify HOW the caller bridges async/sync — e.g., by having the platform layer always dispatch clipboard changes through a Tokio-backed channel, never calling the UseCase directly from a platform thread.

### D. Risk If Adopted As-Is

1. **SetupOrchestrator decomposition without task lifecycle management could create orphaned tasks.** If a setup flow is aborted mid-execution (user closes app, network drops), the current monolithic orchestrator at least drops all its state together. After decomposition, individual executors may hold references to running tasks with no cancellation path. This would manifest as tasks that continue running after the setup UI has been dismissed, potentially writing state that conflicts with a subsequent setup attempt.

2. **SyncInbound decomposition without explicit concurrency model could introduce race conditions.** If two inbound clipboard messages arrive near-simultaneously, the deduplicator and applier may interleave in unexpected ways. The current monolithic implementation implicitly serializes processing because it takes `&self` (single logical owner). After decomposition, if the three services are shared via `Arc`, concurrent access is possible without explicit serialization.

3. **Port bundles without shutdown ordering specification could cause non-deterministic cleanup behavior on application exit.** While `Arc` prevents use-after-free, `Drop` implementations on individual port adapters (e.g., flushing a database connection pool) may execute in an order that loses data.

### E. Suggested Revisions

1. **Add a `CancellationToken` parameter to the `SetupActionExecutor::execute()` trait method:**

   ```rust
   #[async_trait]
   trait SetupActionExecutor: Send + Sync {
       async fn execute(
           &self,
           context: &SetupContext,
           cancel: CancellationToken,
       ) -> Result<Vec<SetupEvent>, SetupError>;
   }
   ```

   This ensures every executor can propagate cancellation to its spawned tasks. The dispatcher derives a child token for each execution and cancels it on abort.

2. **Specify the concurrency model for SyncInbound pipeline.** Add an explicit statement: "SyncInboundClipboardUseCase processes messages serially via an `mpsc` channel. The deduplicator, decoder, and applier are NOT shared across concurrent invocations. Each invocation owns exclusive access to the pipeline." This prevents the concurrent access issue.

3. **Add a note on shutdown ordering for port bundles.** Specify that `AppDeps` should implement a `shutdown()` method that tears down bundles in reverse-dependency order (security last, UI first). Do not rely on Rust's field-order `Drop` for anything with side effects.

4. **For SyncOutbound's async migration, specify the caller-side bridging mechanism.** Add: "The `ClipboardChangeHandler` callback implementation in `uc-app` must send snapshot events through a Tokio `mpsc` channel. The SyncOutbound UseCase is invoked from the channel consumer task, which always runs on the Tokio runtime. Platform-layer clipboard watchers never invoke the UseCase directly."

---

## Review of: testability-reviewer's Phase 2 Proposal

### A. Points of Agreement

1. **The causal chain analysis is excellent.** The observation that "56 port traits -> 39-field AppDeps -> each test mocks all 39 -> mocks are duplicated" correctly identifies testing pain as a downstream symptom of architectural decisions. This avoids the trap of over-investing in test infrastructure that will be invalidated by upstream structural changes.

2. **`uc-test-support` as a dedicated crate is the right approach.** A shared crate with `dev-dependency` linkage ensures mocks are compiled only for tests, keeps uc-core free of test concerns, and provides a single source of truth for mock behavior. The module organization (mocks/, builders/) is well-structured.

3. **The `Noop*` vs `InMemory*` vs `Mock*` vs `Fake*` decision matrix is valuable.** Clearly distinguishing mock categories prevents the common anti-pattern of using heavyweight mocking (mockall) for simple no-op ports. The `Panicking*` variant for asserting non-interaction is a good addition that many projects miss.

4. **Regression safety guards are well-designed.** The CI check `grep -r "impl.*Port for" tests/ | grep -v "uc_test_support"` is a practical, low-cost way to prevent mock duplication from recurring. The AppDeps field count budget test is creative (though see concerns below).

5. **Rejecting "use `mockall` everywhere" is correct.** For 56 ports, blanket `#[automock]` would add significant compile-time cost. The selective approach (only for ports needing call-count verification) is the right trade-off.

### B. Points of Conflict

1. **`uc-test-support` depends "only on uc-core" — but realistic mocks need infra-level data models.**

   The proposal states `uc-test-support` depends ONLY on `uc-core`. But many port trait methods return or accept types that, while defined in `uc-core`, require realistic construction that mirrors infra behavior. For example:
   - `ClipboardEntryRepositoryPort::list_entries()` returns `Vec<ClipboardEntry>`. An `InMemoryClipboardEntryRepo` must construct `ClipboardEntry` instances with valid `EntryId`, `timestamp`, etc. If `EntryId` generation logic lives in `uc-infra` (e.g., UUID generation wrapped in a domain type), the test support crate either duplicates this logic or produces unrealistic test data.
   - `EncryptionPort::encrypt()` and `decrypt()` — a test double that returns ciphertext must produce something that the corresponding decrypt can reverse. A `Noop` implementation that returns the plaintext as "ciphertext" works for unit tests but fails for integration tests that cross the encryption boundary.

   The constraint "never depend on uc-infra" is correct for MOCK implementations. But some FAKE implementations (realistic simulations) may legitimately need to reuse infra logic. The proposal should acknowledge this and define when a test should use a fake from uc-infra vs. a mock from uc-test-support.

2. **Builder pattern for AppDeps — `Arc` sharing semantics.**

   The builder creates `AppDeps` with `self.clipboard.unwrap_or_else(|| Arc::new(NoopPlatformClipboard))`. But `AppDeps` contains `Arc<dyn Port>` fields that are shared across multiple consumers (UseCases, Orchestrators, etc.). The builder pattern typically creates owned values.

   Consider this scenario:

   ```rust
   let deps = TestAppDepsBuilder::new()
       .with_encryption_session(my_session.clone())
       .build();
   ```

   After `build()`, the test holds `my_session` (an `Arc`) and `deps.security.encryption_session` (another `Arc` pointing to the same object). This is correct behavior, but the builder documentation should explicitly state that `Arc` sharing is preserved (not cloned deeply). If a test modifies the encryption session state via `my_session`, the change is visible through `deps.security.encryption_session`. This is usually desired but can be surprising.

   More critically: if the builder is used to construct AppDeps for AppRuntime, and AppRuntime's `UseCases` factory methods extract individual `Arc`s from AppDeps, the `Arc` reference counts grow with each extraction. This is functionally correct but means the builder-constructed AppDeps cannot be dropped to "reset" state — the UseCases still hold references. The proposal should note this lifecycle implication.

3. **Claiming `uc-test-support` can be built NOW, before Cluster A/B fixes land — sequencing risk.**

   The proposal says `uc-test-support` has "zero dependency on A/B fixes." This is true for the mock extraction phase. However:
   - If app-flow-reviewer's proposal to decompose AppDeps into domain-scoped bundles (ClipboardPorts, SecurityPorts, etc.) lands, every mock in `uc-test-support` that implements individual port traits will need to also implement bundle-level traits, or the bundle-level `Noop*` structs will need to be added.
   - If arch-guardian's proposal to create aggregate port traits (ClipboardStoragePorts) lands, the `InMemory*` mocks may need to implement the new aggregate traits in addition to individual traits.
   - The `TestAppDepsBuilder` with 39 fields will need restructuring if AppDeps is decomposed into nested bundles.

   The mock extraction (Phase 1) is genuinely safe to do now. But the builder infrastructure (which the proposal calls "Phase 2, after A/B") is actually sequencing-critical. The proposal should explicitly state: "Do NOT build `TestAppDepsBuilder` until AppDeps decomposition is finalized. Build individual orchestrator builders (TestSetupOrchestratorBuilder) instead, as orchestrator constructors are less likely to change."

### C. Constraints They Missed

1. **Async runtime in tests.** Many port traits are `#[async_trait]`. Tests that use `InMemory*` mocks need a Tokio runtime. The proposal does not specify whether `uc-test-support` provides test runtime setup utilities (e.g., a `#[tokio::test]` re-export or a shared runtime configuration). This is a practical friction point — every test file currently sets up its own runtime.

2. **Thread safety of `InMemory*` mocks.** The proposal shows `InMemoryClipboardEntryRepo` using `std::sync::Mutex<HashMap<...>>`. But port traits require `Send + Sync`. Using `std::sync::Mutex` in async test code has the same poisoning risk that the infra-runtime proposal (H9) explicitly addresses for production code. The test support crate should use `tokio::sync::Mutex` or `parking_lot::Mutex` for consistency with the project's anti-`std::Mutex`-in-async-code stance.

3. **The `appdeps_field_count_budget` test uses `size_of` heuristic.** This test divides `size_of::<AppDeps>()` by `size_of::<usize>()` to estimate field count. But `Arc<dyn Trait>` is a fat pointer (2 words: pointer + vtable), so each field is `2 * size_of::<usize>()`. The heuristic will report roughly half the actual field count. Additionally, struct padding may skew the result. A more reliable approach is a compile-time macro that counts fields, or simply a comment-enforced policy.

4. **Mock behavior consistency.** If `NoopClipboardEntryRepo::list_entries()` always returns `Ok(vec![])`, but a future port contract change makes an empty list semantically invalid (e.g., a port guarantees at least a "default" entry), the noop mock silently violates the contract. The proposal does not specify how mock behavior is validated against evolving port contracts. Consider adding contract tests: a trait test suite that any implementation (real or mock) must pass.

### D. Risk If Adopted As-Is

1. **`TestAppDepsBuilder` becomes immediately stale.** If built now with 39 `Option` fields mapping to current AppDeps, and AppDeps is decomposed into nested bundles within 1-2 sprints, the builder must be rewritten. The effort of building and migrating all tests to use it is wasted. This is the most likely concrete risk.

2. **`std::sync::Mutex` in InMemory mocks introduces the same panic risk the project is trying to eliminate.** If a test panics while holding the lock (e.g., an assertion failure inside a mock method), subsequent mock calls in the same test will also panic due to mutex poisoning, producing confusing cascading failures. Using `parking_lot::Mutex` (which does not poison) or `tokio::sync::Mutex` (async-safe) would be consistent with the project's direction.

3. **Missing contract tests mean noop mocks may diverge from port semantics over time.** As ports evolve (new methods added, return type semantics change), noop mocks that were correct at creation time may silently become incorrect. Tests pass but are not testing real behavior.

### E. Suggested Revisions

1. **Defer `TestAppDepsBuilder` until after AppDeps decomposition is finalized.** Instead, invest in orchestrator-specific builders (TestSetupOrchestratorBuilder, TestSyncInboundBuilder) which are less likely to be invalidated by structural changes. These target the immediate pain (14-param constructor setup) without coupling to AppDeps shape.

2. **Use `parking_lot::Mutex` (or `tokio::sync::Mutex`) in all InMemory mocks.** This aligns with the project-wide stance against `std::sync::Mutex` in async contexts and eliminates mutex poisoning issues in tests.

3. **Add a "contract test" pattern to `uc-test-support`.** For each port trait, define a test suite function that verifies basic contract compliance:

   ```rust
   pub async fn verify_clipboard_entry_repo_contract(
       repo: &dyn ClipboardEntryRepositoryPort
   ) {
       // Save an entry, list it, verify it appears
       // Delete it, verify it's gone
       // These tests run against both InMemory and real implementations
   }
   ```

   This ensures mocks and real implementations remain behaviorally aligned.

4. **Explicitly scope `uc-test-support` Phase 1 to mock extraction only.** Remove any mention of builders or AppDeps-level utilities from Phase 1. Phase 1 deliverable: a crate with `Noop*` and `InMemory*` mocks organized by domain, plus migration of existing test files. Phase 2 (builder infrastructure) gates on AppDeps decomposition completion.

5. **Fix the `appdeps_field_count_budget` test.** Replace the `size_of` heuristic with a macro-based field counter or a simple constant assertion that is manually updated:
   ```rust
   const APPDEPS_FIELD_BUDGET: usize = 45;
   const APPDEPS_ACTUAL_FIELDS: usize = 39; // Update when adding fields
   #[test]
   fn appdeps_within_budget() {
       assert!(APPDEPS_ACTUAL_FIELDS <= APPDEPS_FIELD_BUDGET);
   }
   ```

---

## Cross-Cutting Observations

1. **Cancellation tokens are a prerequisite for multiple proposals.** The app-flow proposal decomposes SetupOrchestrator into executors that spawn tasks. The infra-runtime proposal (my own) introduces `CancellationToken` threading. The testability proposal needs to mock lifecycle behavior. These three proposals must coordinate on cancellation token design BEFORE any of them proceeds. Specifically: the `SetupActionExecutor` trait signature must include cancellation support from day one, not as a retrofit.

2. **AppDeps decomposition is the single highest-impact change, but also the most coordination-intensive.** The arch-guardian proposes aggregate port traits (ClipboardStoragePorts), the app-flow reviewer proposes domain-scoped port bundles (ClipboardPorts), and the testability reviewer proposes builder patterns for AppDeps. These three proposals all touch AppDeps structure but propose slightly different decomposition strategies. If they proceed independently, AppDeps could be restructured three times. **Recommendation: Agree on a single decomposition strategy (bundles vs. aggregates vs. both) before any implementation begins.**

3. **The sequencing of test infrastructure vs. structural reform is critical.** The testability reviewer wants to build `uc-test-support` NOW. The app-flow reviewer wants to decompose AppDeps and orchestrators. If test infrastructure is built against current interfaces and those interfaces change within weeks, the test infrastructure becomes a migration burden rather than an accelerator. The safe path: extract existing mocks into `uc-test-support` (zero structural change), but defer builders and AppDeps-level utilities until the structural reforms stabilize.
