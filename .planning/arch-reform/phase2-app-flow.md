# Application Flow Reviewer — Phase 2 Reform Proposal

**Date**: 2026-03-06
**Reviewer**: Application Flow Reviewer
**Scope**: UseCase design, orchestration patterns, application layer responsibilities, command flow
**Issues Covered**: H3, H4, H5, M1, M2, M3

---

## Root Cause Analysis

The application layer issues share a common root cause: **the absence of clear compositional boundaries within the `uc-app` crate**. When the crate was initially structured, UseCases were designed as flat units that receive all their dependencies at construction time. This worked for simple CRUD-style operations (ListClipboardEntries, DeleteClipboardEntry), but broke down as complex multi-step flows were added (Setup, Sync, Pairing).

**Cluster B pattern**: As complexity grew, two failure modes emerged:

1. **Accumulation** — god objects (SetupOrchestrator, AppDeps) kept absorbing new responsibilities because there was no sub-composition mechanism.
2. **Leakage** — UseCases that needed to do "just one more thing" (encoding, runtime detection) pulled in infrastructure concerns rather than going through ports, because adding a new port felt heavyweight.

These two modes reinforce each other: the flat AppDeps container makes it easy to grab any port, which discourages creating focused sub-groupings, which makes the god objects grow further.

---

## Reform Proposals by Issue

### H3: SetupOrchestrator God Object

**Current state**: `SetupOrchestrator` (orchestrator.rs, 860+ lines) holds 15 injected dependencies plus 6 internal state fields. It directly manages pairing sessions, space access flows, encryption initialization, lifecycle boot, discovery, and setup state machine transitions — all in a single struct.

**Root cause**: Setup is a multi-phase workflow (new space vs. join space), and the orchestrator conflates the state machine driver role with the side-effect execution role. Every new setup action added another dependency and another method.

**Proposed decomposition**:

1. **Extract Action Executors** — Each `SetupAction` variant should have a dedicated executor struct:

```
SetupOrchestrator (state machine driver only)
├── CreateSpaceExecutor       — owns: InitializeEncryption
├── MarkCompleteExecutor      — owns: MarkSetupComplete, AppLifecycleCoordinator
├── DiscoveryExecutor         — owns: DiscoveryPort, NetworkControlPort
├── PairingExecutor           — owns: PairingOrchestrator, PairingTransportPort
├── PeerTrustExecutor         — owns: PairingOrchestrator
├── SpaceAccessExecutor       — owns: SpaceAccessOrchestrator, crypto/transport/proof/timer/persistence ports
└── AbortExecutor             — owns: PairingOrchestrator
```

2. **SetupOrchestrator becomes a thin dispatcher**:
   - Owns only: `SetupContext`, `SetupEventPort`, `SetupStatusPort`
   - Holds action executors as `Arc<dyn SetupActionExecutor>`
   - `execute_actions()` dispatches to the appropriate executor
   - Internal mutable state (selected_peer_id, pairing_session_id, passphrase, joiner_offer) moves to `SetupContext`

3. **Define a `SetupActionExecutor` trait**:

```rust
#[async_trait]
trait SetupActionExecutor: Send + Sync {
    async fn execute(
        &self,
        context: &SetupContext,
    ) -> Result<Vec<SetupEvent>, SetupError>;
}
```

**Migration path**:

- Phase A: Move mutable state into SetupContext (pure refactor, no behavior change)
- Phase B: Extract one executor at a time (CreateSpaceExecutor first — simplest)
- Phase C: Extract PairingExecutor and SpaceAccessExecutor (most complex)
- Phase D: Remove direct dependency fields from SetupOrchestrator

**Dependency reduction**: SetupOrchestrator goes from 15 dependencies to 3 (context, event_port, status_port) plus a Vec of executors.

### H4: SyncInbound Bloat & Embedded Sub-UseCases

**Current state**: `SyncInboundClipboardUseCase` (sync_inbound.rs, ~440 lines) performs:

1. Echo prevention (device identity check)
2. Encryption session readiness check
3. Message deduplication (recent_ids management)
4. Payload decryption (master key retrieval + chunked decode)
5. Binary payload parsing (ClipboardBinaryPayload decode)
6. Representation priority selection
7. Representation conversion (BinaryRepresentation -> ObservedClipboardRepresentation)
8. OS clipboard write (Full mode)
9. Capture persistence via embedded CaptureClipboardUseCase (Passive mode)
10. Origin tracking

It also holds an `Option<CaptureClipboardUseCase>` — a full sub-UseCase embedded as a field.

**Root cause**: The inbound flow was built incrementally as a monolithic async function. The V3 protocol upgrade added decryption and binary decoding inline. The Passive mode support grafted capture persistence onto the same UseCase.

**Proposed decomposition**:

1. **Extract `InboundPayloadDecoder`** — a domain service (not a UseCase) responsible for:
   - Decryption via `TransferPayloadDecryptorPort`
   - Binary payload parsing (`ClipboardBinaryPayload::decode_from`)
   - Input: `(ClipboardMessage, Option<Vec<u8>>)` + encryption session
   - Output: `Result<ClipboardBinaryPayload>` or typed error

2. **Extract `InboundDeduplicator`** — owns recent_ids state:
   - `is_duplicate(message_id) -> bool`
   - `record(message_id)`
   - `rollback(message_id)`
   - Pruning happens internally

3. **Extract `RepresentationSelector`** — the `select_highest_priority_repr_index` function already exists as a free function. Promote to a domain service behind a port so selection strategy can evolve.

4. **SyncInboundClipboardUseCase becomes an orchestrator of these services**:

```rust
pub struct SyncInboundClipboardUseCase {
    mode: ClipboardIntegrationMode,
    deduplicator: InboundDeduplicator,
    decoder: InboundPayloadDecoder,
    applier: InboundClipboardApplier,  // handles Full vs Passive mode
    device_identity: Arc<dyn DeviceIdentityPort>,
    encryption_session: Arc<dyn EncryptionSessionPort>,
}
```

5. **`InboundClipboardApplier`** — handles the mode-specific write logic:
   - Full mode: clipboard_change_origin + local_clipboard.write_snapshot
   - Passive mode: delegates to CaptureClipboardUseCase

**Migration path**:

- Phase A: Extract InboundDeduplicator (pure extract, no behavior change)
- Phase B: Extract InboundPayloadDecoder (moves decryption/decode logic)
- Phase C: Extract InboundClipboardApplier (separates mode-specific application)
- Phase D: Wire into simplified SyncInboundClipboardUseCase

### H5: SyncOutbound Infra Leakage

**Current state**: `SyncOutboundClipboardUseCase` (sync_outbound.rs, ~360 lines) contains:

1. **`tokio::runtime::Handle::try_current()` check** (line 219) — runtime detection logic that belongs in infrastructure
2. **`ProtocolMessage::frame_to_bytes()`** call (line 211) — wire encoding is transport-layer concern
3. **`ClipboardBinaryPayload::encode_to_vec()`** (line 141-143) — binary serialization
4. **`executor::block_on()`** (line 70) — blocking bridge in `execute()` method

**Root cause**: The UseCase was designed to do "everything from snapshot to wire bytes" in one place. The `TransferPayloadEncryptorPort` only covers encryption, but encoding and framing were left in the UseCase.

**Proposed reform**:

1. **Define `OutboundPayloadPreparerPort`** — a port that encapsulates:
   - Binary payload encoding
   - Encryption
   - Wire framing
   - Input: `(SystemClipboardSnapshot, ClipboardMessage header)`
   - Output: `Arc<[u8]>` (ready-to-send bytes)

```rust
#[async_trait]
pub trait OutboundPayloadPreparerPort: Send + Sync {
    async fn prepare(
        &self,
        snapshot: &SystemClipboardSnapshot,
        header: &ClipboardMessage,
    ) -> Result<Arc<[u8]>>;
}
```

2. **Move `executor::block_on` to caller** — The `execute()` method should be purely async. If a synchronous entry point is needed, the caller (platform layer callback) should handle the async-to-sync bridge, not the UseCase.

3. **Remove `tokio::runtime::Handle::try_current()` branching** — The UseCase should always assume it runs in an async context. The caller is responsible for ensuring a runtime exists. The current two-branch pattern (spawn vs. join) is a runtime environment detection hack.

4. **Extract `PeerFanoutService`** — the peer iteration, ensure_business_path, and send logic is a transport concern:

```rust
#[async_trait]
pub trait ClipboardFanoutPort: Send + Sync {
    async fn fanout(
        &self,
        peers: &[DiscoveredPeer],
        payload: Arc<[u8]>,
    ) -> FanoutResult;
}
```

**Migration path**:

- Phase A: Make `execute()` purely async, move block_on to callers
- Phase B: Extract OutboundPayloadPreparerPort implementation in uc-infra
- Phase C: Extract ClipboardFanoutPort implementation in uc-infra/uc-platform
- Phase D: Remove tokio::runtime::Handle::try_current() branching

### M1: AppDeps God Container

**Current state**: `AppDeps` (deps.rs) is a flat struct with 30+ fields spanning clipboard (12 ports), security (7 ports), device (2), pairing (1), network (3), setup (1), storage (5), settings (1), UI (2), and system (2) domains.

Every UseCase factory method in `UseCases<'a>` has access to the entire `AppDeps` through `self.runtime.deps`, violating ISP — a UseCase that only needs `clipboard_entry_repo` can still reach `encryption`, `pairing_transport`, etc.

**Root cause**: AppDeps was designed as a "just group everything" container (the comments explicitly say "NOT a Builder, just parameter grouping"). This was pragmatic early on but scales poorly.

**Proposed reform — Domain-scoped port bundles**:

```rust
pub struct ClipboardPorts {
    pub platform_clipboard: Arc<dyn PlatformClipboardPort>,
    pub system_clipboard: Arc<dyn SystemClipboardPort>,
    pub entry_repo: Arc<dyn ClipboardEntryRepositoryPort>,
    pub event_writer: Arc<dyn ClipboardEventWriterPort>,
    pub representation_repo: Arc<dyn ClipboardRepresentationRepositoryPort>,
    pub representation_normalizer: Arc<dyn ClipboardRepresentationNormalizerPort>,
    pub selection_repo: Arc<dyn ClipboardSelectionRepositoryPort>,
    pub representation_policy: Arc<dyn SelectRepresentationPolicyPort>,
    pub representation_cache: Arc<dyn RepresentationCachePort>,
    pub spool_queue: Arc<dyn SpoolQueuePort>,
    pub change_origin: Arc<dyn ClipboardChangeOriginPort>,
    pub worker_tx: mpsc::Sender<RepresentationId>,
}

pub struct SecurityPorts {
    pub encryption: Arc<dyn EncryptionPort>,
    pub encryption_session: Arc<dyn EncryptionSessionPort>,
    pub encryption_state: Arc<dyn EncryptionStatePort>,
    pub key_scope: Arc<dyn KeyScopePort>,
    pub secure_storage: Arc<dyn SecureStoragePort>,
    pub key_material: Arc<dyn KeyMaterialPort>,
    pub watcher_control: Arc<dyn WatcherControlPort>,
}

pub struct StoragePorts {
    pub blob_store: Arc<dyn BlobStorePort>,
    pub blob_repository: Arc<dyn BlobRepositoryPort>,
    pub blob_writer: Arc<dyn BlobWriterPort>,
    pub thumbnail_repo: Arc<dyn ThumbnailRepositoryPort>,
    pub thumbnail_generator: Arc<dyn ThumbnailGeneratorPort>,
}

pub struct DevicePorts {
    pub device_repo: Arc<dyn DeviceRepositoryPort>,
    pub device_identity: Arc<dyn DeviceIdentityPort>,
}

pub struct UiPorts {
    pub ui_port: Arc<dyn UiPort>,
    pub autostart: Arc<dyn AutostartPort>,
}

pub struct SystemPorts {
    pub clock: Arc<dyn ClockPort>,
    pub hash: Arc<dyn ContentHashPort>,
}

/// Restructured AppDeps with domain-scoped bundles
pub struct AppDeps {
    pub clipboard: ClipboardPorts,
    pub security: SecurityPorts,
    pub device: DevicePorts,
    pub network: Arc<NetworkPorts>,     // already exists
    pub network_control: Arc<dyn NetworkControlPort>,
    pub pairing: PairingPorts,
    pub setup: SetupPorts,
    pub storage: StoragePorts,
    pub settings: Arc<dyn SettingsPort>,
    pub ui: UiPorts,
    pub system: SystemPorts,
}
```

**Key principle**: UseCase factory methods should only receive the relevant port bundle, not the entire AppDeps:

```rust
// Before (ISP violation):
pub fn list_clipboard_entries(&self) -> ListClipboardEntries {
    ListClipboardEntries::from_arc(self.runtime.deps.clipboard_entry_repo.clone())
}

// After (ISP-aligned):
pub fn list_clipboard_entries(&self) -> ListClipboardEntries {
    ListClipboardEntries::from_arc(self.runtime.deps.clipboard.entry_repo.clone())
}
```

**Migration path**:

- Phase A: Create port bundle structs, make AppDeps hold bundles (backward compat: add accessor methods that delegate to bundles)
- Phase B: Update UseCases factory methods one domain at a time
- Phase C: Remove flat field accessors once all consumers migrate

**Note**: NetworkPorts already exists as a bundle — this reform extends the pattern to all domains.

### M2: Setup/Pairing Responsibility Crossover

**Current state**: `SetupOrchestrator` directly holds `pairing_transport: Arc<dyn PairingTransportPort>` and calls `open_pairing_session()` (line 317-327) during `start_join_space_access()`. Meanwhile, `PairingOrchestrator` is the designated owner of pairing session lifecycle.

This means two orchestrators can independently manipulate pairing sessions — Setup opens sessions, Pairing manages sessions. This is a split-brain risk.

**Root cause**: The space access flow required reopening a pairing session after the pairing handshake completed. Rather than routing this through PairingOrchestrator, the transport port was directly injected into SetupOrchestrator.

**Proposed reform**:

1. **All pairing session operations must go through PairingOrchestrator**. Add a method:

```rust
impl PairingOrchestrator {
    /// Reopen a completed pairing session for space access data transfer.
    pub async fn reopen_session_for_space_access(
        &self,
        peer_id: String,
        session_id: String,
    ) -> Result<()> { ... }
}
```

2. **Remove `pairing_transport` from SetupOrchestrator**. Setup delegates to PairingOrchestrator for all pairing transport operations.

3. **SetupOrchestrator's `start_join_space_access()` changes from**:

```rust
self.pairing_transport
    .open_pairing_session(peer_id, pairing_session_id.clone())
    .await
```

**to**:

```rust
self.pairing_orchestrator
    .reopen_session_for_space_access(peer_id, pairing_session_id.clone())
    .await
```

**Migration path**: Single PR — add method to PairingOrchestrator, update SetupOrchestrator, remove pairing_transport field.

### M3: Anemic Domain Models

**Current state**: `ClipboardEntry` and `ClipboardEvent` are pure data bags:

- `ClipboardEntry`: 6 fields, 2 constructors (`new`, `new_with_active_time`), no domain behavior
- `ClipboardEvent`: 4 fields, 1 constructor, no domain behavior

All business logic that operates on these models lives in UseCases or repository implementations.

**Root cause**: The models were designed as DTOs for database mapping. Domain behavior was never placed on them because the "Clean Architecture" approach treated them as passive data carriers.

**Proposed enrichment** (conservative — only behavior that genuinely belongs on the model):

1. **ClipboardEntry**:

```rust
impl ClipboardEntry {
    /// Whether this entry has been viewed/touched since creation.
    pub fn is_stale(&self, now_ms: i64, staleness_threshold_ms: i64) -> bool {
        now_ms - self.active_time_ms > staleness_threshold_ms
    }

    /// Update the active time to "now", indicating user interaction.
    pub fn touch(&mut self, now_ms: i64) {
        self.active_time_ms = now_ms;
    }

    /// Whether this entry has a displayable title.
    pub fn has_title(&self) -> bool {
        self.title.as_ref().map_or(false, |t| !t.trim().is_empty())
    }
}
```

2. **ClipboardEvent**:

```rust
impl ClipboardEvent {
    /// Whether this event originated from the given device.
    pub fn is_from_device(&self, device_id: &DeviceId) -> bool {
        self.source_device == *device_id
    }

    /// Whether this event is local (from the current device).
    pub fn is_local(&self, local_device_id: &DeviceId) -> bool {
        self.is_from_device(local_device_id)
    }
}
```

3. **SystemClipboardSnapshot** already has `snapshot_hash()` — this is a good example of behavior on a domain model. Extend this pattern.

**What NOT to do**:

- Do not add repository/persistence methods to domain models
- Do not add serialization logic to domain models
- Do not add network/transport concerns to domain models
- Do not create "rich" domain models that import infrastructure types

**Migration path**: Add methods incrementally. Find existing UseCase code that performs these checks and replace with model method calls.

---

## Boundaries to Protect

1. **UseCase -> Port boundary**: UseCases must NEVER import `uc-infra` or `uc-platform` types directly. All external operations go through ports defined in `uc-core`.

2. **Orchestrator -> UseCase boundary**: Orchestrators coordinate UseCases. They should not contain business logic that belongs in a UseCase. The line is: orchestrators decide _what_ to do next; UseCases decide _how_ to do it.

3. **Domain model purity**: Types in `uc-core/src/clipboard/` must not import anything from `uc-app`, `uc-infra`, or `uc-platform`. They can depend on `uc-core` types only.

4. **Port bundle cohesion**: Each port bundle must group ports that change together. Do not mix clipboard ports with security ports in the same bundle.

5. **Event-driven boundaries**: Setup state transitions must remain in the state machine (`SetupStateMachine::transition`). Action executors must not bypass the state machine.

---

## Abstractions to Add / Remove / Split

### Add

| Abstraction                                  | Location                                | Justification                                                      |
| -------------------------------------------- | --------------------------------------- | ------------------------------------------------------------------ |
| `SetupActionExecutor` trait                  | `uc-app`                                | Enables decomposition of SetupOrchestrator into testable units     |
| `InboundPayloadDecoder` service              | `uc-app`                                | Extracts decryption+decode from SyncInbound, testable in isolation |
| `InboundDeduplicator`                        | `uc-app`                                | Extracts dedup state management, testable independently            |
| `InboundClipboardApplier`                    | `uc-app`                                | Separates mode-specific clipboard write logic                      |
| `OutboundPayloadPreparerPort`                | `uc-core` (port) / `uc-infra` (impl)    | Moves encoding+encryption+framing out of UseCase                   |
| `ClipboardFanoutPort`                        | `uc-core` (port) / `uc-platform` (impl) | Moves peer iteration and send logic out of UseCase                 |
| Domain port bundles (`ClipboardPorts`, etc.) | `uc-app`                                | ISP-aligned dependency grouping                                    |

### Remove

| Abstraction                                             | Justification                                          |
| ------------------------------------------------------- | ------------------------------------------------------ |
| `pairing_transport` field from SetupOrchestrator        | Crossover violation; route through PairingOrchestrator |
| `executor::block_on` in SyncOutbound                    | UseCase should be purely async                         |
| `tokio::runtime::Handle::try_current()` in SyncOutbound | Runtime detection is infra concern                     |

### Split

| From                          | Into                                 | Justification                |
| ----------------------------- | ------------------------------------ | ---------------------------- |
| `SetupOrchestrator`           | Thin dispatcher + 7 action executors | God object decomposition     |
| `SyncInboundClipboardUseCase` | Orchestrating UseCase + 3 services   | Bloat reduction, testability |
| `AppDeps` (flat)              | `AppDeps` with nested port bundles   | ISP alignment                |

---

## Risks & Trade-offs

### Risk 1: Over-extraction creating "ravioli architecture"

**Mitigation**: Only extract when a component has 3+ responsibilities or when a component is independently testable. Do not extract single-method services.

### Risk 2: SetupOrchestrator refactor breaking the state machine

**Mitigation**: SetupStateMachine is a pure function — it does not change. Only the action execution side is being restructured. Keep the dispatch loop intact, only change how actions are routed.

### Risk 3: Port bundle refactor causing widespread import changes

**Mitigation**: Introduce bundles as nested structs within AppDeps. Keep flat accessor methods as deprecated compatibility shims during migration. Remove shims only after all consumers migrate.

### Risk 4: InboundPayloadDecoder creating indirection without value

**Mitigation**: The decoder encapsulates a genuine multi-step process (decrypt -> decode -> validate). It has clear input/output types and is independently testable. This is not premature abstraction.

### Risk 5: SyncOutbound becoming too thin after extraction

**Mitigation**: SyncOutbound's core responsibility — deciding when and to whom to send — remains meaningful even after encoding and fanout extraction. The precondition checks (encryption ready, peers available, origin filtering) are genuine business rules.

### Trade-off: Port proliferation

Adding `OutboundPayloadPreparerPort` and `ClipboardFanoutPort` increases the port surface. This is acceptable because:

- Each port has a clear, single responsibility
- They replace inline infrastructure code in UseCases
- They make the UseCase testable without transport or encoding dependencies

---

## Pseudo-Solutions to Reject

### 1. "Just split the file into multiple modules"

Moving methods to separate files without changing the struct's dependency list does not solve H3. The god object problem is about responsibility concentration, not file size. `SetupOrchestrator` with 15 dependencies in 3 files is still a god object.

### 2. "Use #[cfg(test)] mocks on AppDeps"

Adding test-only factory methods to AppDeps (e.g., `AppDeps::test_clipboard_only()`) appears to solve the ISP problem for tests but leaves production code unchanged. The real issue is that production UseCases receive ports they don't need.

### 3. "Make SyncInbound generic over mode"

Creating `SyncInboundFull` and `SyncInboundPassive` as separate types splits the struct but duplicates the shared logic (dedup, decode, echo prevention). The correct decomposition separates _phases_ (decode vs. apply), not _modes_.

### 4. "Add a PayloadService that does encode + encrypt + frame"

This would be a convenience wrapper, not a proper port. If it lives in `uc-app` and directly calls `ProtocolMessage::frame_to_bytes()`, it just relocates the infra leakage. The fix must define a **port** in `uc-core` and implement it in `uc-infra`.

### 5. "Flatten SetupOrchestrator's action handling with a macro"

Using a dispatch macro for `execute_actions()` reduces boilerplate but does not reduce the dependency count or improve testability. The 15-dependency constructor remains. Macros hide complexity; they don't eliminate it.

### 6. "Add behavior to ClipboardEntry via extension traits in uc-app"

Extension traits would keep the model in `uc-core` pure but scatter behavior across crates. Simple domain predicates (`is_stale`, `has_title`) genuinely belong on the model itself. Reserve extension traits for crate-specific behavior, not core domain logic.

### 7. "Replace AppDeps with a DI container (e.g., shaku)"

A DI framework adds runtime complexity and makes dependency wiring implicit. The current explicit construction is correct — it just needs better grouping. The problem is structural (flat vs. nested), not a missing framework.
