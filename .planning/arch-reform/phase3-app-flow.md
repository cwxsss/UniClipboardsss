# Application Flow Reviewer — Phase 3 Cross-Review

## Review of: arch-guardian's Phase 2 Proposal

### A. Points of Agreement

1. **H1 StreamDecoderPort is the right approach.** Introducing a port in `uc-core` to break the `uc-platform -> uc-infra` horizontal dependency is architecturally sound. The port exists to enforce the dependency rule, and even a single call site justifies it. This aligns with the application flow principle that use cases and adapters should only communicate through ports.

2. **H2 phased approach is well-structured.** Separating the `deps` visibility fix (Phase A) from absorbing business logic into use cases (Phase B) is pragmatic. The observability carve-out (device_id for spans) correctly identifies that not all `deps` access is a UseCase bypass — some is legitimate cross-cutting concern access.

3. **M5 cohesion-based grouping over merging.** The aggregate port trait pattern (`ClipboardStoragePorts`) that composes individual traits rather than replacing them is exactly right. It preserves ISP for simple use cases while reducing the injection surface for complex ones. This directly supports the app-flow goal of reducing UseCase constructor parameter lists.

4. **Pseudo-solution rejections are thorough.** The rejection of re-exporting uc-infra through uc-core, moving ChunkedDecoder to core, and creating god-traits all demonstrate sound architectural judgment.

### B. Points of Conflict

1. **M6: `futures-core::Stream` replacement has significant UseCase migration cost.**

   The proposal to replace `tokio::sync::mpsc::Receiver` with `Pin<Box<dyn Stream<Item = T> + Send>>` in core port signatures affects every UseCase that consumes events from `NetworkEventPort::subscribe_events()` or `ClipboardTransportPort::subscribe_clipboard()`. Currently these consumers use `receiver.recv().await` — a simple, cancel-safe pattern. After migration, they must use `StreamExt::next().await`, which requires importing `futures::StreamExt` (or `tokio_stream::StreamExt`).

   **The conflict**: The proposal states "In consumers (uc-tauri/uc-app): use `StreamExt::next()` instead of `recv()`" as if this is a trivial change. But `StreamExt` is in `futures-util` or `tokio-stream`, and the proposal only adds `futures-core` to uc-core. The consumers (in uc-app) would need `futures-util` as a dependency to use `StreamExt`. This means the M6 fix doesn't just change uc-core — it propagates a new dependency to uc-app.

   Additionally, `mpsc::Receiver::recv()` is cancel-safe by Tokio's guarantee. `Stream::next()` is also cancel-safe, but this is a property of the specific `ReceiverStream` wrapper, not of `Stream` in general. If a future adapter provides a `Stream` that is NOT cancel-safe, consumers in `tokio::select!` blocks will have subtle bugs.

2. **H2: `pub(crate)` on `AppRuntime.deps()` does NOT work across crate boundaries.**

   The proposal says: "The `UseCases` struct already has `runtime: &'a AppRuntime` — change its field accesses from `self.runtime.deps.X` to a private `AppRuntime::deps(&self) -> &AppDeps` method that is `pub(crate)`."

   **The problem**: `UseCases` is defined in `uc-tauri` (same crate as `AppRuntime`), so `pub(crate)` works for `UseCases` accessing `deps()`. But the proposal's Phase B creates `CheckEncryptionReadiness` use case in `uc-app`. If this use case needs to access ports from `AppDeps`, it cannot use `pub(crate)` methods on `AppRuntime` because `uc-app` is a different crate. The use case must receive its dependencies through constructor injection, not through `AppRuntime`.

   This is actually consistent with the existing pattern (use cases receive `Arc<dyn Port>` directly), but the proposal conflates two access patterns: (1) `UseCases` factory accessor pattern (uc-tauri, same crate), and (2) UseCase instances receiving injected ports (uc-app, different crate). The `pub(crate)` mechanism only applies to pattern (1).

3. **M4: Moving UiPort/AutostartPort to uc-app creates a dependency direction problem.**

   The proposal says these ports should move from `uc-core/ports/` to `uc-app/ports/`. But `uc-platform` currently implements these ports:
   - `uc-platform/src/adapters/ui.rs` implements `UiPort`
   - `uc-platform/src/adapters/autostart.rs` implements `AutostartPort`

   If the trait definitions move to `uc-app`, then `uc-platform` must depend on `uc-app` to implement `impl UiPort for PlaceholderUiPort`. This violates the arch-guardian's own invariant #2: "uc-platform depends only on uc-core."

   The proposal says "Update `uc-tauri` adapter implementations to import from `uc_app::ports`" — implying the implementations would also move to `uc-tauri`. But the existing implementations in `uc-platform` would need to be moved simultaneously, and future platform-specific implementations (e.g., macOS-specific autostart via LaunchAgent) inherently belong in `uc-platform`.

### C. Constraints They Missed

1. **UseCase event consumption patterns vary.** Some UseCases consume events in a loop (`tokio::select!` with `recv()`), others consume a single event. The M6 migration to `Stream` affects both patterns differently. Loop-based consumers need `StreamExt::next()` in select branches; single-event consumers can use `StreamExt::next().await` directly. The proposal does not distinguish these patterns or provide migration examples for each.

2. **Aggregate port traits add a layer of indirection that affects UseCase testing.** When a UseCase depends on `Arc<dyn ClipboardStoragePorts>`, the test must create a struct implementing `ClipboardStoragePorts` that returns references to individual mock ports. This is more complex than directly injecting `Arc<dyn ClipboardEntryRepositoryPort>`. The testability reviewer's `uc-test-support` crate must coordinate with this pattern.

3. **The `CheckEncryptionReadiness` use case (H2 Phase B) may need to be called from platform event handlers.** If the platform runtime needs to check encryption readiness before processing an inbound clipboard message, and this check is encapsulated in a use case in `uc-app`, the platform layer would need to call into the app layer — which reverses the dependency direction.

### D. Risk If Adopted As-Is

1. **M4 (UiPort/AutostartPort move) would break the uc-platform -> uc-core-only invariant.** If uc-platform must implement traits defined in uc-app, the dependency graph becomes `uc-platform -> uc-app -> uc-core`, which is explicitly forbidden by invariant #4 ("uc-app depends only on uc-core") and creates a circular risk if uc-app ever needs platform services.

2. **M6 (Stream migration) without consumer-side guidance risks introducing cancel-safety bugs in uc-app.** UseCases that currently rely on `tokio::select!` with `recv()` may exhibit different behavior with `Stream::next()` if the underlying Stream implementation changes.

3. **H2 Phase A alone (making deps private) is safe, but Phase B (new use cases in uc-app) introduces wiring complexity.** Each new use case must be constructed in `UseCases` factory with the right ports, and there is no mechanism to verify at compile time that the right ports are passed.

### E. Suggested Revisions

1. **M4: Keep UiPort/AutostartPort in uc-core, but add a documentation annotation marking them as "application-level ports."** Alternatively, create a `uc-core/ports/application/` sub-module to physically separate them from domain ports while keeping them in the same crate. This preserves the dependency rule while achieving the conceptual separation the arch-guardian wants. If absolutely insistent on moving them out of uc-core, they should go to a new minimal `uc-ports` crate that both uc-app and uc-platform can depend on — not to uc-app.

2. **M6: Add an explicit "Consumer Migration Guide" section** that:
   - Lists every call site in uc-app and uc-tauri that uses `recv()` on the affected ports
   - Specifies whether `futures-util` or `tokio-stream` should be the `StreamExt` provider for each consumer crate
   - Documents the cancel-safety contract: "All Stream implementations returned by ports MUST be cancel-safe when used with `StreamExt::next()`"
   - Provides before/after examples for both loop-based and single-event consumption patterns

3. **H2: Clarify that `pub(crate)` is specifically for the `UseCases` factory pattern in uc-tauri, not for UseCase instances in uc-app.** Add a note that use cases in uc-app always receive their dependencies via constructor injection and never access `AppRuntime` directly.

4. **M5: Coordinate aggregate trait definitions with the testability reviewer's `uc-test-support` crate.** For every aggregate port trait added, ensure a corresponding `NoopXxxPorts` implementation exists in `uc-test-support` so test complexity does not increase.

---

## Review of: contract-error-reviewer's Phase 2 Proposal

### A. Points of Agreement

1. **H10 IPC DTO separation is necessary and well-motivated.** Domain models should not be the IPC contract. The proposal correctly identifies that `Serialize/Deserialize` on domain models creates coupling between domain refactoring and frontend stability. The `From<DomainModel> for Dto` pattern in `uc-tauri/models/` is the right place for this mapping.

2. **M9 removing `EncryptionState::Initializing` is the right call.** A phantom variant that no code path produces is worse than a missing variant. The recommendation to remove it with a documented design decision is clean. From the application flow perspective, use cases that match on `EncryptionState` currently handle `Initializing` identically to `Uninitialized` (or have unreachable arms), so removing it simplifies UseCase logic.

3. **M12 using `PeerId` consistently across `NetworkEvent`** is a clear improvement. Domain events should use domain types. The mechanical migration is low-risk since `PeerId` already implements the required traits.

4. **M13 analysis is accurate.** The distinction between `AppConfig` (bootstrap) and `Settings` (runtime preferences) is correct. Removing overlapping fields and renaming to `BootstrapConfig` clarifies responsibilities and prevents UseCases from being confused about which source of truth to use.

### B. Points of Conflict

1. **H10: DTO mapping responsibility creates a hidden coupling in command handlers.**

   The proposal says "The command function performs the mapping" using `From<DomainModel> for Dto` impls. But this means command handlers must understand the structure of domain models to invoke `.into()` or `Dto::from(domain_model)`. If a UseCase returns a complex nested domain type (e.g., `ClipboardEntry` with associated `ClipboardRepresentation` list), the command handler needs to know the full structure to map it.

   **The application flow concern**: Currently, use cases like `ListClipboardEntries` return domain types directly. If we add a DTO layer, should the UseCase return the DTO (leaking IPC concerns into the app layer), or should the command handler do a potentially complex multi-step mapping? The proposal says "command layer performs the mapping" but doesn't address the case where mapping requires additional data fetching (e.g., resolving a blob URL from a blob ID to include in the DTO).

   **Suggested resolution**: For simple flat mappings, `From` impls in `uc-tauri/models/` are fine. For mappings that require additional data (e.g., URL resolution, thumbnail lookup), define a `Projector` service in `uc-tauri` that takes the UseCase output plus supplementary data and produces the DTO. This keeps mapping logic out of the command handler itself.

2. **H11: `CommandError` interaction with `anyhow::Result` in UseCase signatures creates a translation gap.**

   The proposal defines `CommandError` with variants like `NotFound`, `NotReady`, `InvalidInput`, etc. The translation pattern shown is:

   ```rust
   use_case.execute(&parsed_id).await.map_err(|e| match e.downcast_ref::<ClipboardError>() {
       Some(ClipboardError::EntryNotFound(id)) => CommandError::NotFound { ... },
       _ => CommandError::Internal { message: e.to_string() },
   })
   ```

   **The problem**: Most use cases return `anyhow::Result`, and `downcast_ref` only works if the use case explicitly wraps a typed error inside anyhow. If the error comes from a port method that also uses `anyhow::Result`, the original error type is lost. The `downcast_ref` will return `None`, and everything falls through to `CommandError::Internal` — which is exactly the same behavior as `e.to_string()`, just wrapped in a struct.

   This means **the CommandError migration is largely cosmetic unless M11 (typed port errors) lands first.** Without typed errors at the port and use case level, the command layer cannot meaningfully discriminate errors.

3. **M11: Cross-cluster port usage in UseCases creates error type complexity.**

   The proposal groups errors by domain cluster: `ClipboardRepositoryError`, `BlobError`, `SettingsError`, `NetworkError`. But application-layer UseCases routinely cross cluster boundaries:
   - `CaptureClipboardUseCase` uses clipboard repos (ClipboardRepositoryError), blob writer (BlobError), content hash (SystemError?), and spool queue (ClipboardRepositoryError or its own type?)
   - `SyncInboundClipboardUseCase` uses encryption session (SecurityError), clipboard transport (NetworkError), clipboard repos (ClipboardRepositoryError), and local clipboard (PlatformError?)
   - `UpdateSettings` use case calls `SettingsPort` (SettingsError), `AutostartPort` (AutostartError), and `WatcherControlPort` (WatcherControlError)

   Each of these UseCases must handle 3-4 different error enum types. The use case either needs its own error enum that wraps all of them (creating yet another error type), or it uses `anyhow::Error` internally and maps at the boundary (which is what the proposal says is acceptable but undermines the goal).

   **The practical impact**: A UseCase like `CaptureClipboard` would need:

   ```rust
   pub enum CaptureClipboardError {
       Clipboard(ClipboardRepositoryError),
       Blob(BlobError),
       System(SystemError),
       // ... one variant per cluster it touches
   }
   ```

   This is correct in principle but creates significant boilerplate, especially since some use cases touch 4+ clusters.

### C. Constraints They Missed

1. **UseCase error return types affect the `UseCases` factory accessor pattern.** Currently, `UseCases::capture_clipboard()` returns a `CaptureClipboardUseCase` that has `execute() -> anyhow::Result<...>`. If the use case switches to `Result<..., CaptureClipboardError>`, the command handler must import `CaptureClipboardError`. This is fine, but it means the error type becomes part of the public API surface of uc-app. The proposal doesn't address whether use case error types should be re-exported from a central location or imported from individual use case modules.

2. **IPC DTO evolution over time.** The proposal creates DTOs in `uc-tauri/models/` but doesn't establish a versioning or backward-compatibility strategy. If the frontend caches error codes (`NotFound`, `NotReady`), adding a new variant to `CommandError` could break frontend match exhaustiveness. A `#[non_exhaustive]` annotation on `CommandError` should be considered.

3. **Settings DTO complexity.** The proposal notes that Settings is "lowest-priority H10 fix" and suggests a DTO "only if the frontend shape needs to diverge." But `get_settings` returns `serde_json::Value` which is already an untyped DTO. The real problem is `update_settings` which accepts `serde_json::Value` and deserializes into the domain `Settings` — this means the frontend can send malformed data that only fails at deserialization time, with no typed validation. The contract-error proposal should address this validation gap.

### D. Risk If Adopted As-Is

1. **H11 without M11 creates "structured nothing."** The `CommandError` enum would exist, but 90%+ of errors would map to `CommandError::Internal { message: e.to_string() }` because the underlying `anyhow::Result` cannot be downcast. The frontend would see a `code: "Internal"` discriminator on almost every error, gaining no actionable information compared to the current `String` approach. This is effort for minimal benefit until typed port errors exist.

2. **M11 error proliferation in cross-cluster UseCases.** Without a strategy for composing errors across clusters, each UseCase author will independently decide how to handle multi-cluster errors. Some will create per-UseCase error enums, others will use anyhow internally, leading to inconsistent error handling patterns across the application layer.

3. **H10 DTO proliferation without clear boundaries.** The proposal says "only creating DTOs where the domain model shape differs from the IPC shape." But in practice, developers will default to creating DTOs for everything (defensive) or nothing (lazy), unless there is a clear decision framework. The proposal should specify exactly which current command return types need DTOs and which can remain as-is.

### E. Suggested Revisions

1. **Sequence M11 before H11.** Typed port errors should land before `CommandError`. This ensures that when the command layer maps errors, there are actually typed errors to map FROM. Without this sequencing, H11 is largely cosmetic.

2. **Define a UseCase error convention.** UseCases that cross cluster boundaries should use their own error enum that wraps the relevant cluster errors. Provide a macro or pattern to reduce boilerplate:

   ```rust
   // Convention: each UseCase defines its error type in the same module
   #[derive(Debug, thiserror::Error)]
   pub enum CaptureClipboardError {
       #[error(transparent)]
       Repository(#[from] ClipboardRepositoryError),
       #[error(transparent)]
       Blob(#[from] BlobError),
       #[error("content hash failed: {0}")]
       Hash(String),
   }
   ```

   This makes the UseCase's error surface explicit and allows the command layer to meaningfully map each variant.

3. **Add a "DTO Required" decision matrix** to the H10 proposal:

   | Command                 | Current Return           | DTO Needed?            | Reason                               |
   | ----------------------- | ------------------------ | ---------------------- | ------------------------------------ |
   | `list_paired_devices`   | `Vec<PairedDevice>`      | Yes                    | Domain model exposes internal fields |
   | `get_clipboard_entries` | `Vec<ClipboardEntryDto>` | No (already DTO)       | Already migrated                     |
   | `get_settings`          | `serde_json::Value`      | No (already decoupled) | Value acts as untyped DTO            |
   | `capture_clipboard`     | `()`                     | No                     | No data returned                     |

4. **Add `#[non_exhaustive]` to `CommandError`** to allow future variant additions without breaking frontend match patterns.

---

## Cross-Cutting Observations

### 1. Dependency Direction is the Hardest Problem

Both proposals introduce changes that risk violating the dependency direction rule:

- Arch-guardian's M4 (moving ports to uc-app) would make uc-platform depend on uc-app
- Contract-error's M11 (typed errors in ports) forces uc-app UseCases to handle multiple error types, potentially pulling in error types from clusters they shouldn't know about

The dependency rule (`uc-app -> uc-core <- uc-infra / uc-platform`) is the single most important architectural invariant. Both proposals should be re-evaluated through this lens, and any change that even temporarily violates this rule should be rejected or redesigned.

### 2. Sequencing Matters More Than Individual Proposals

The arch-guardian and contract-error proposals have implicit dependencies:

- **M11 (typed errors) should precede H11 (CommandError)** — otherwise CommandError is cosmetic
- **M5 (port grouping) should precede M11 (typed errors)** — otherwise error enums must be defined per-port rather than per-cluster
- **H2 (deps private) should precede M4 (port relocation)** — otherwise deps visibility changes interact with port import changes

Neither proposal addresses sequencing. The committee should establish a merge order that respects these dependencies.

### 3. Both Proposals Underestimate Application-Layer Complexity

The arch-guardian treats use cases as simple consumers of ports. The contract-error reviewer treats error handling as a mechanical translation exercise. In reality, application-layer use cases are the most complex components: they orchestrate multiple ports, handle partial failures, manage transactional boundaries, and must present coherent error information upstream. Both proposals would benefit from consulting the application flow inventory to understand how many use cases touch 3+ port clusters, and designing their grouping/error strategies around actual usage patterns rather than theoretical domain boundaries.
