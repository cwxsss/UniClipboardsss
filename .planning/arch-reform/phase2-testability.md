# Testability & Evolution Reviewer -- Phase 2 Reform Proposal

## Root Cause Analysis

### How Cluster E Relates to Clusters A and B

Cluster E (Testing Infrastructure Debt) is not an independent problem. It is a **downstream symptom** of architectural decisions in Clusters A and B:

- **Cluster A (AppDeps god container / M1)**: `AppDeps` is a flat struct with 39 required fields, all `Arc<dyn XxxPort>`. Every test that constructs or touches AppDeps must supply all 39 ports, even if only 1-3 are exercised.
- **Cluster B (Port explosion / M5)**: 56 port traits exist in `uc-core/ports/`. Each port requires a separate mock implementation. The flat structure means mocks grow linearly with port count.

The causal chain:

```
56 port traits (M5)
  --> 39-field AppDeps (M1/H13)
  --> Each test must mock all 39 (H13)
  --> Mocks are hand-rolled & duplicated (H12)
```

**Critical insight**: Fixing M5 (port grouping) and M1 (AppDeps decomposition) from Clusters A/B will **mechanically reduce** the severity of H12 and H13. However, testing infrastructure fixes are still needed independently because:

1. Even with grouped ports, hand-rolled mocks remain unscalable.
2. Even with decomposed AppDeps, test fixture construction still needs builder/factory patterns.
3. Regression safety requires structural guards, not just fewer ports.

### H12: NoopPort Mock Duplication (800+ lines, 30+ copies)

**Evidence from codebase**:

| Location                                          | Duplicate Mock Structs                                                                                                                                                                                                                                                                                                                                                                           |
| ------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `uc-app/tests/snapshot_cache_integration_test.rs` | `InMemoryDeviceIdentity`, `InMemoryThumbnailRepo`, `NoopThumbnailGenerator`, `InMemoryEntryRepo`, `InMemoryRepresentationRepo`, `InMemoryEventWriter`, `InMemoryBlobWriter`, `FixedClock` (8 structs)                                                                                                                                                                                            |
| `uc-app/tests/stress_test.rs`                     | Identical copies of all 8 above                                                                                                                                                                                                                                                                                                                                                                  |
| `uc-app/tests/setup_flow_integration_test.rs`     | `InMemorySecureStorage`, `TestKeyScope`, `MockWatcherControl`, `MockNetworkControl`, `MockSessionReadyEmitter`, `MockLifecycleStatus`, `MockLifecycleEventEmitter`, `NoopPairedDeviceRepository`, `NoopDiscoveryPort`, `NoopSetupEventPort`, `NoopSpaceAccessTransport`, `NoopSpaceAccessPairingTransport`, `DeterministicSpaceAccessCrypto`, `DeterministicSpaceAccessPersistence` (14 structs) |
| `uc-app/tests/app_lifecycle_coordinator_test.rs`  | `MockWatcherControl`, `MockNetworkControl`, `MockSessionReadyEmitter`, `MockLifecycleStatus`, `MockLifecycleEventEmitter` (5 structs, duplicated from setup_flow test)                                                                                                                                                                                                                           |
| `uc-app/tests/app_lifecycle_status_test.rs`       | Same 5 mocks duplicated again                                                                                                                                                                                                                                                                                                                                                                    |
| `uc-app/tests/clipboard_sync_e2e_test.rs`         | `InMemoryClipboard`, `StaticDeviceIdentity` + inline network mocks                                                                                                                                                                                                                                                                                                                               |
| `uc-tauri/src/test_utils.rs`                      | `NoopPort` (implements 4 network port traits)                                                                                                                                                                                                                                                                                                                                                    |

**Root cause**: No shared test utility crate. Each test file is an independent compilation unit in `tests/` directory, so they cannot share `mod` definitions without a shared crate or a `#[cfg(test)]` module in the library itself.

### H13: AppDeps 39-Field Test Setup Hell

**Evidence**: `AppDeps` at `uc-app/src/deps.rs:47-102` requires 39 fields. However, most tests do NOT construct AppDeps at all -- they construct individual use cases directly with their specific port dependencies. The problem manifests differently:

1. **Use case tests** (current pattern): Construct use cases directly with 3-8 ports each. This is actually reasonable but leads to verbose setup code because each port mock is defined per-file.
2. **Integration tests** (e.g., `setup_flow_integration_test.rs`): Construct orchestrators with 14+ dependencies, leading to 60+ lines of setup boilerplate per test function, copy-pasted across 3 test functions in the same file.
3. **AppDeps as a whole**: Currently only constructed in `uc-tauri/bootstrap/wiring.rs` (production) and never in tests. The risk is that as more tests need full-app construction, the 39-field problem will become acute.

**Real pain point**: The 39-field count is a design smell, but the immediate pain is in orchestrator construction (SetupOrchestrator takes 14 parameters in `new()`), not AppDeps itself. The orchestrator parameter lists are a consequence of flat dependency passing.

### M5: Port Explosion (56 Port Traits)

**Evidence**: 56 `pub trait XxxPort` definitions across `uc-core/src/ports/`. Many are single-method traits:

- `ClockPort`: 1 method (`now_ms`)
- `ContentHashPort`: 1 method
- `DeviceIdentityPort`: 1 method
- `AutostartPort`: 2 methods
- `WatcherControlPort`: 2 methods
- `NetworkControlPort`: 1 method

Some are appropriately granular (ISP), but many could be grouped by aggregate/domain:

- 14 clipboard-related ports
- 8 security-related ports
- 4 network-related ports (already grouped in `NetworkPorts`)
- 5 blob-related ports

**Impact on testing**: Every port trait requires at least one mock implementation. At 56 ports with an average of ~15 lines per mock, that is **840+ lines of mock code** that must exist somewhere. Currently these are scattered and duplicated.

---

## Reform Proposals by Issue

### H12: NoopPort Mock Duplication

#### Immediate Fix (Before Cluster A/B)

**Create a shared test utilities crate**: `uc-test-support`

```
src-tauri/crates/uc-test-support/
  Cargo.toml          # depends on uc-core only (NOT uc-infra, uc-app)
  src/
    lib.rs
    mocks/
      mod.rs
      clipboard.rs    # InMemoryEntryRepo, InMemoryRepresentationRepo, etc.
      security.rs     # InMemorySecureStorage, TestKeyScope, etc.
      network.rs      # NoopClipboardTransport, NoopPeerDirectory, etc.
      device.rs       # InMemoryDeviceIdentity
      lifecycle.rs    # MockWatcherControl, MockNetworkControl, etc.
      blob.rs         # InMemoryBlobWriter
      system.rs       # FixedClock
    builders/
      mod.rs          # Test fixture builders (Phase 2)
```

**Key design decisions**:

1. `uc-test-support` depends ONLY on `uc-core` (port traits). It never imports `uc-infra`.
2. All mock structs implement port traits from `uc-core`.
3. Tests in `uc-app/tests/` and `uc-tauri/` add `uc-test-support` as a `dev-dependency`.
4. The crate provides both `Noop*` (do-nothing) and `InMemory*` (stateful) variants.

**Mock organization pattern**:

```rust
// uc-test-support/src/mocks/clipboard.rs

/// A no-op clipboard entry repository that returns empty results.
pub struct NoopClipboardEntryRepo;

#[async_trait]
impl ClipboardEntryRepositoryPort for NoopClipboardEntryRepo {
    async fn save_entry_and_selection(&self, _: &ClipboardEntry, _: &ClipboardSelectionDecision) -> Result<()> {
        Ok(())
    }
    async fn get_entry(&self, _: &EntryId) -> Result<Option<ClipboardEntry>> { Ok(None) }
    async fn list_entries(&self, _: usize, _: usize) -> Result<Vec<ClipboardEntry>> { Ok(vec![]) }
    async fn delete_entry(&self, _: &EntryId) -> Result<()> { Ok(()) }
}

/// A stateful in-memory clipboard entry repository for assertions.
#[derive(Default)]
pub struct InMemoryClipboardEntryRepo {
    entries: std::sync::Mutex<HashMap<EntryId, ClipboardEntry>>,
    // ...
}
```

**Migration path** (incremental):

1. Create `uc-test-support` crate with mocks extracted from `snapshot_cache_integration_test.rs` (largest mock surface).
2. Migrate `stress_test.rs` to use shared mocks (exact same types, just different import path).
3. Migrate `setup_flow_integration_test.rs` mocks.
4. Migrate `app_lifecycle_*.rs` mocks.
5. Migrate `uc-tauri/test_utils.rs` `NoopPort` to the shared crate.
6. Delete all inline mock definitions.

**Estimated reduction**: ~800 lines of duplicated mock code eliminated. Single source of truth for mock behavior.

#### Long-term Fix (After Cluster A/B)

**Consider `mockall` for ports that need behavioral verification**:

```rust
// Only for ports where tests need to assert call patterns
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait NetworkControlPort: Send + Sync {
    async fn start_network(&self) -> Result<()>;
}
```

**Trade-off**: `mockall` adds compile-time cost and macro complexity. Use it selectively for ports that need `expect_*` / `times(n)` assertions. Keep hand-written `InMemory*` mocks for stateful test doubles (repositories, caches).

**Decision matrix**:

| Mock Type         | When to Use                           | Implementation                    |
| ----------------- | ------------------------------------- | --------------------------------- |
| `Noop*`           | Port is irrelevant to the test        | Hand-written in `uc-test-support` |
| `InMemory*`       | Test needs to verify persisted state  | Hand-written in `uc-test-support` |
| `Mock*` (mockall) | Test needs to verify call count/order | `#[automock]` on trait definition |
| `Fake*`           | Test needs realistic behavior         | Hand-written, domain-specific     |

### H13: AppDeps 39-Field Test Setup Hell

#### Immediate Fix (Before Cluster A/B)

**Builder pattern for test fixture construction** in `uc-test-support`:

```rust
// uc-test-support/src/builders/mod.rs

pub struct TestAppDepsBuilder {
    clipboard: Option<Arc<dyn PlatformClipboardPort>>,
    system_clipboard: Option<Arc<dyn SystemClipboardPort>>,
    // ... all 39 fields as Option<...>
}

impl TestAppDepsBuilder {
    pub fn new() -> Self {
        Self {
            clipboard: None,
            system_clipboard: None,
            // ...
        }
    }

    pub fn with_clipboard(mut self, clipboard: Arc<dyn PlatformClipboardPort>) -> Self {
        self.clipboard = Some(clipboard);
        self
    }

    // ... builder methods for each field

    pub fn build(self) -> AppDeps {
        AppDeps {
            clipboard: self.clipboard.unwrap_or_else(|| Arc::new(NoopPlatformClipboard)),
            system_clipboard: self.system_clipboard.unwrap_or_else(|| Arc::new(NoopSystemClipboard)),
            // ... default to Noop for all unset fields
        }
    }
}
```

**Key principle**: Tests only set the ports they care about. Everything else defaults to `Noop`.

**Orchestrator-specific builders** for the immediate pain (SetupOrchestrator with 14 params):

```rust
pub struct TestSetupOrchestratorBuilder {
    // only the 14 params SetupOrchestrator::new() takes
    initialize_encryption: Option<Arc<InitializeEncryption>>,
    // ...
}

impl TestSetupOrchestratorBuilder {
    pub fn minimal() -> Self {
        // Pre-fills all fields with noop/default implementations
        Self { /* ... */ }
    }

    pub fn with_setup_status(mut self, status: Arc<dyn SetupStatusPort>) -> Self {
        self.setup_status = Some(status);
        self
    }

    pub fn build(self) -> SetupOrchestrator {
        // ...
    }
}
```

#### Long-term Fix (After Cluster A/B AppDeps Decomposition)

Once AppDeps is decomposed into domain-scoped bundles (e.g., `ClipboardDeps`, `SecurityDeps`, `NetworkDeps`), the builder pattern naturally shrinks:

```rust
// After Cluster A/B reforms
pub struct ClipboardDeps {
    pub entry_repo: Arc<dyn ClipboardEntryRepositoryPort>,
    pub event_repo: Arc<dyn ClipboardEventWriterPort>,
    pub representation_repo: Arc<dyn ClipboardRepresentationRepositoryPort>,
    // ~8 fields instead of 39
}

// Test builder is now trivial
pub fn test_clipboard_deps() -> ClipboardDeps {
    ClipboardDeps {
        entry_repo: Arc::new(NoopClipboardEntryRepo),
        event_repo: Arc::new(NoopClipboardEventWriter),
        representation_repo: Arc::new(NoopRepresentationRepo),
        // ...
    }
}
```

### M5: Port Explosion Compounds Testing Cost

#### Proposed Port Grouping (Coordinates with Cluster B)

The 56 ports can be grouped into ~8-10 aggregate-aligned port bundles:

| Bundle                  | Ports                                                                                                                                                                                               | Count |
| ----------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----- |
| `ClipboardPorts`        | EntryRepo, EventWriter/Reader, RepresentationRepo, SelectionRepo, RepresentationPolicy, RepresentationNormalizer, RepresentationCache, SpoolQueue, ChangeOrigin, PayloadResolver, SelectionResolver | 11    |
| `ClipboardSystemPorts`  | PlatformClipboard, SystemClipboard, ThumbnailRepo, ThumbnailGenerator                                                                                                                               | 4     |
| `SecurityPorts`         | Encryption, EncryptionSession, EncryptionState, KeyScope, SecureStorage, KeyMaterial, TransferEncryptor, TransferDecryptor                                                                          | 8     |
| `NetworkPorts` (exists) | ClipboardTransport, PeerDirectory, PairingTransport, NetworkEvent                                                                                                                                   | 4     |
| `NetworkControlPorts`   | NetworkControl, WatcherControl, Discovery, ConnectionPolicy                                                                                                                                         | 4     |
| `DevicePorts`           | DeviceRepo, DeviceIdentity, PairedDeviceRepo, IdentityStore                                                                                                                                         | 4     |
| `BlobPorts`             | BlobStore, BlobRepo, BlobWriter                                                                                                                                                                     | 3     |
| `SystemPorts`           | Clock, ContentHash, AppDirs, Settings, SettingsMigration, Timer                                                                                                                                     | 6     |
| `UiPorts`               | UiPort, Autostart, SetupEvent, SetupStatus                                                                                                                                                          | 4     |
| `SpaceAccessPorts`      | Crypto, Transport, Persistence, Proof                                                                                                                                                               | 4     |

**Testing impact**: Instead of 39 individual mock fields, tests would set ~3-4 port bundles. A `NoopClipboardPorts` struct covers 11 mocks in one implementation.

**Important**: This grouping must be proposed and validated by the Architecture Guardian (Cluster B owner). The testability reviewer's role is to validate that the grouping reduces mock surface effectively.

---

## Dependency on Other Clusters

### What Must Cluster A/B Fix FIRST

| Cluster A/B Fix                                       | Testability Benefit                                     | Blocking?                                                                 |
| ----------------------------------------------------- | ------------------------------------------------------- | ------------------------------------------------------------------------- |
| Decompose `AppDeps` into domain bundles               | Builder pattern shrinks from 39 to ~8 fields per bundle | **No** -- builder works on current AppDeps too, just more verbose         |
| Group ports into aggregate bundles (M5)               | Mock count drops from 56 to ~10 bundle-level mocks      | **No** -- shared mock crate works at individual port level too            |
| Extract orchestrator constructors to use bundle types | Orchestrator test builders become trivial               | **Yes** -- orchestrator builder pattern depends on constructor signatures |

### What Can Be Improved NOW

1. **`uc-test-support` crate** -- Zero dependency on A/B fixes. Just extracts and deduplicates existing mocks.
2. **Builder pattern for existing orchestrators** -- Works with current 14-param constructors.
3. **Shared mock module** -- Eliminates the copy-paste problem immediately.
4. **`#[automock]` on select traits** -- Independent of structural changes.

### What Becomes Possible AFTER A/B Fixes

1. **Bundle-level mock structs** -- One `NoopClipboardPorts` replaces 11 individual Noop structs.
2. **Trivial test fixture construction** -- `TestClipboardDeps::default()` instead of 11-field builder.
3. **Use case isolation testing** -- Use case takes `ClipboardDeps` bundle, test only supplies that bundle.
4. **Compile-time enforcement** -- If a use case only depends on `ClipboardPorts`, it physically cannot access `SecurityPorts`, verified by Rust's type system.

---

## Regression Safety Strategy

### Structural Guards

1. **Lint rule: No mock definitions in `tests/` files**
   - CI check: `grep -r "impl.*Port for" src-tauri/crates/uc-app/tests/ | grep -v "uc_test_support"` should return empty.
   - Enforces that all mocks live in `uc-test-support`.

2. **Dependency boundary check**
   - `uc-test-support` must NOT depend on `uc-infra` or `uc-platform` or `uc-app`.
   - This ensures test mocks are pure port implementations, not infrastructure wrappers.
   - Enforce via CI: `cargo metadata` check on `uc-test-support` dependencies.

3. **AppDeps field count tracking**
   - Add a compile-time or test assertion that `AppDeps` field count does not exceed a threshold.
   - This prevents silent field accumulation.

```rust
#[test]
fn appdeps_field_count_budget() {
    // If this fails, you need to decompose AppDeps before adding more fields
    let field_count = std::mem::size_of::<AppDeps>() / std::mem::size_of::<usize>();
    assert!(field_count <= 45, "AppDeps has too many fields ({}), decompose before adding more", field_count);
}
```

4. **Mock coverage tracking**
   - Maintain a manifest in `uc-test-support` listing which ports have mocks.
   - CI step: verify every port trait in `uc-core/ports/` has at least a `Noop*` mock in `uc-test-support`.

### Process Guards

1. **PR template checklist**: "If you added a new port trait, did you add a corresponding mock in `uc-test-support`?"
2. **Architecture Decision Record**: Document that all new port mocks must go in `uc-test-support`, never inline in test files.

---

## Boundaries to Protect

1. **`uc-test-support` depends only on `uc-core`**: This is the most critical boundary. If test utilities start depending on infra implementations, the mock/real boundary blurs and tests lose their isolation value.

2. **Use cases take port traits, never concrete types**: Already enforced by current architecture. Must remain so after any refactoring.

3. **Test files must not define port implementations**: All mock/fake/stub implementations belong in `uc-test-support`. Test files should only contain test functions and test-specific helper functions (e.g., `drive_space_access_to_waiting_decision`).

4. **`InMemory*` mocks must not leak production behavior**: In-memory mocks should implement the simplest correct behavior, not replicate production logic. If a test needs production-like behavior, it should use the real infra implementation explicitly (integration test).

---

## Abstractions to Add / Remove / Split

### Add

| Abstraction                                    | Purpose                                            |
| ---------------------------------------------- | -------------------------------------------------- |
| `uc-test-support` crate                        | Centralized mock/fake/builder definitions          |
| `TestAppDepsBuilder`                           | Builder pattern for AppDeps construction in tests  |
| `TestSetupOrchestratorBuilder`                 | Builder for orchestrator test construction         |
| Port bundle types (coordinated with Cluster B) | `ClipboardPorts`, `SecurityPorts`, etc.            |
| Bundle-level Noop structs (after bundles land) | `NoopClipboardPorts` replacing 11 individual Noops |

### Remove

| Abstraction                             | Reason                                |
| --------------------------------------- | ------------------------------------- |
| Inline mock structs in each test file   | Replaced by `uc-test-support` imports |
| `uc-tauri/src/test_utils.rs` `NoopPort` | Merged into `uc-test-support`         |

### Split

| Current                                                                         | Proposed Split                                                                                       |
| ------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| `AppDeps` (39 fields)                                                           | Domain-scoped dep bundles (coordinated with Cluster A)                                               |
| Monolithic test setup functions (e.g., 60-line `create_space_flow...` preamble) | Reusable builder calls (`TestSetupOrchestratorBuilder::minimal().with_setup_status(custom).build()`) |

---

## Risks & Trade-offs

### Risk 1: `uc-test-support` becomes a dumping ground

**Mitigation**: Strict module organization (mocks/, builders/, fixtures/). Code review enforces that only port implementations and test helpers go here. No business logic, no test assertions.

### Risk 2: Noop mocks hide bugs by silently succeeding

**Mitigation**: Distinguish between `Noop*` (returns Ok/empty, used when port is irrelevant) and `Panicking*` (panics if called, used to verify a port is NOT touched). The `Panicking*` variant catches unexpected interactions.

```rust
pub struct PanickingClipboardEntryRepo;

#[async_trait]
impl ClipboardEntryRepositoryPort for PanickingClipboardEntryRepo {
    async fn save_entry_and_selection(&self, _: &ClipboardEntry, _: &ClipboardSelectionDecision) -> Result<()> {
        panic!("ClipboardEntryRepositoryPort::save_entry_and_selection was unexpectedly called");
    }
    // ...
}
```

### Risk 3: `mockall` compile time overhead

**Mitigation**: Only use `#[automock]` on ports where call-count/order verification is needed (estimated <10 ports). Keep hand-written mocks for the majority. Measure compile time delta before committing.

### Risk 4: Builder pattern becomes stale when AppDeps changes

**Mitigation**: The builder's `build()` method must construct AppDeps directly (not via `Default`), so adding a new field to AppDeps will cause a compile error in the builder, forcing an update.

### Risk 5: Over-investment in test infrastructure before A/B fixes land

**Mitigation**: Phase the work:

- **Phase 1** (now): Create `uc-test-support` with existing mock extractions. ~2 days.
- **Phase 2** (after A/B): Add bundle-level mocks and simplified builders. ~1 day.
- Phase 1 investment is not wasted -- the shared crate persists through A/B changes.

---

## Pseudo-Solutions to Reject

### 1. "Just use `mockall` everywhere"

**Why reject**: `mockall` generates significant macro expansion, increasing compile times. For 56 ports, this would add measurable CI overhead. Most ports need simple no-op or in-memory behavior, not call-count verification. `mockall` is a precision tool, not a blanket solution.

### 2. "Add a `Default` impl to `AppDeps`"

**Why reject**: `AppDeps` contains `Arc<dyn Trait>` fields which cannot implement `Default`. Even with a custom Default that uses Noop ports, this hides which dependencies a test actually uses, making tests harder to reason about. The builder pattern is explicit about what is set vs. defaulted.

### 3. "Just add more integration tests to avoid the mock problem"

**Why reject**: Integration tests are slower, require real infra setup (database, filesystem, etc.), and test multiple behaviors simultaneously, making failure diagnosis harder. Unit tests with proper mocks catch bugs faster and at a finer granularity. The two are complementary, not substitutable.

### 4. "Create a single `NoopPort` struct that implements all 56 traits"

**Why reject**: This exists partially in `uc-tauri/test_utils.rs` (implements 4 network traits). Extending it to 56 traits creates a god-mock that:

- Cannot be customized per-test (all behaviors are hardcoded)
- Violates ISP (the very principle the port architecture was built on)
- Makes it impossible to use `InMemory*` behavior for some ports and `Noop` for others

### 5. "Use `#[cfg(test)]` mocks inside `uc-core` itself"

**Why reject**: Pollutes the core domain crate with test infrastructure. `uc-core` should define contracts (traits), not implementations (mocks). Mocks are an implementation concern belonging to the test layer.

### 6. "Just reduce port count by merging traits"

**Why reject as sole fix**: Port count reduction (M5) is a Cluster B concern. Even with fewer ports, without a shared mock crate, duplication would persist at a smaller scale. Both fixes are needed. Port reduction without mock centralization just makes the problem smaller, not eliminated.

### 7. "Inline test helpers via `#[path = ...]` or `include!` macros"

**Why reject**: These are fragile, non-idiomatic Rust patterns that confuse IDE tooling, make refactoring harder, and don't provide proper module scoping. A proper crate is cleaner and more maintainable.
