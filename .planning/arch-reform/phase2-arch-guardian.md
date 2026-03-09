# Architecture Guardian — Phase 2 Reform Proposal

## Root Cause Analysis

### Why These Issues Exist

Each confirmed issue traces to a common origin: **the hexagonal migration was executed bottom-up (crate structure first, abstractions second) rather than top-down (dependency rules first, then fill in crates).** The crate boundaries were drawn early, but the rules governing what crosses those boundaries were never formalized. As features were added under time pressure, developers took the shortest path that compiled — creating the violations we see today.

**H1 (uc-platform → uc-infra):** The `ChunkedDecoder` was originally infra code (serialization + crypto). When the libp2p network adapter needed to decode incoming chunked streams, it reached for the existing implementation directly rather than defining a port in `uc-core`. This happened because the chunked transfer protocol is a cross-cutting concern that spans both network transport (platform) and encryption/serialization (infra), and no abstraction existed to mediate between them.

**H2 (Commands bypass UseCases via `runtime.deps`):** `AppDeps` was designed as a "parameter grouping struct" with all fields `pub`. The `UseCases` accessor was added later as the _intended_ path, but making `deps` private would break the `UseCases` implementation itself (which reads `self.runtime.deps.*`). Since there was no compiler-enforced boundary, commands that needed quick access to device_id or encryption_session bypassed the use-case layer because it was syntactically easier and there was no lint or visibility rule to prevent it.

**M4 (UiPort/AutostartPort in uc-core):** These were placed in `uc-core/ports/` during initial architecture setup because the placement guidelines ("Does this represent a business capability?") were ambiguous. Opening a settings window and managing OS autostart are _application-level_ capabilities, not domain concepts. But because `AppDeps` (in `uc-app`) needed to hold `Arc<dyn UiPort>`, and `uc-app` depends on `uc-core`, the path of least resistance was to define the traits in `uc-core`.

**M5 (50+ port traits):** Every adapter capability was given its own trait with no grouping strategy. The initial hexagonal migration translated each legacy interface 1:1 into a port trait. There was no "port consolidation" pass. The result: 48 `*Port` traits, each requiring a separate mock in tests, and `AppDeps` having 30+ fields.

**M6 (tokio in core ports):** When `NetworkEventPort` and `ClipboardTransportPort` were defined, the developers chose `tokio::sync::mpsc::Receiver` because it was the concrete channel type already in use. The `uc-core` crate already had `tokio = { features = ["sync"] }` as a dependency for other reasons, so adding `mpsc::Receiver` to the trait signatures felt natural. The consequence is that core domain contracts are now bound to a specific async runtime.

---

## Reform Proposals by Issue

### H1: uc-platform → uc-infra Horizontal Dependency

**Current violation:** `uc-platform/src/adapters/libp2p_network.rs:972` calls `uc_infra::clipboard::ChunkedDecoder::decode_from(reader, &master_key)` directly.

**Root operation:** `ChunkedDecoder::decode_from` takes a `Read` implementor and a `MasterKey`, performs V3 wire-format parsing, chunk decryption (XChaCha20-Poly1305), and optional zstd decompression. The symmetric operation `ChunkedEncoder::encode_to` is used from `uc-tauri` wiring.

**Proposed fix — introduce a `StreamDecoderPort` in uc-core:**

1. Define a new port trait in `uc-core/src/ports/clipboard_transport.rs` (co-located with existing transport ports):

```rust
/// Port for decoding an incoming encrypted clipboard stream into plaintext.
///
/// This abstracts the wire-format parsing, decryption, and decompression
/// so that transport adapters (uc-platform) do not depend on infra (uc-infra).
#[async_trait]
pub trait StreamDecoderPort: Send + Sync {
    /// Decode an encrypted chunked stream into plaintext bytes.
    ///
    /// Implementations handle wire-format parsing, chunk reassembly,
    /// decryption, and optional decompression.
    async fn decode_stream(&self, data: Vec<u8>, master_key: &[u8; 32]) -> Result<Vec<u8>>;
}
```

2. Create an adapter in `uc-infra/src/clipboard/stream_decoder_adapter.rs` that wraps `ChunkedDecoder::decode_from`:

```rust
pub struct ChunkedStreamDecoderAdapter;

#[async_trait]
impl StreamDecoderPort for ChunkedStreamDecoderAdapter {
    async fn decode_stream(&self, data: Vec<u8>, master_key: &[u8; 32]) -> Result<Vec<u8>> {
        let key = MasterKey::from_bytes(master_key)?;
        tokio::task::spawn_blocking(move || {
            let cursor = std::io::Cursor::new(data);
            ChunkedDecoder::decode_from(cursor, &key)
                .map_err(|e| anyhow::anyhow!("{}", e))
        }).await?
    }
}
```

3. Inject `Arc<dyn StreamDecoderPort>` into the libp2p network adapter via constructor, wired in `uc-tauri` composition root.

4. Remove `uc-infra` from `uc-platform/Cargo.toml`.

**Files changed:**

- `src-tauri/crates/uc-core/src/ports/clipboard_transport.rs` — add `StreamDecoderPort` trait
- `src-tauri/crates/uc-core/src/ports/mod.rs` — re-export `StreamDecoderPort`
- `src-tauri/crates/uc-infra/src/clipboard/stream_decoder_adapter.rs` — new file, adapter impl
- `src-tauri/crates/uc-infra/src/clipboard/mod.rs` — export new adapter
- `src-tauri/crates/uc-platform/src/adapters/libp2p_network.rs` — accept `Arc<dyn StreamDecoderPort>` in constructor, replace direct `uc_infra` call
- `src-tauri/crates/uc-platform/Cargo.toml` — remove `uc-infra` dependency
- `src-tauri/crates/uc-tauri/src/bootstrap/wiring.rs` — wire `ChunkedStreamDecoderAdapter` into libp2p adapter

**Verification:** After change, `cargo check -p uc-platform` must succeed without `uc-infra` in its dependency tree. Add a CI check: `cargo tree -p uc-platform | grep uc-infra` must return empty.

---

### H2: Commands Bypassing UseCases via runtime.deps

**Current state:** `AppRuntime.deps` is `pub`, and commands access `runtime.deps.device_identity`, `runtime.deps.encryption_session`, etc. directly from 13+ call sites in `uc-tauri/src/commands/`.

**The two categories of bypass:**

1. **Span metadata** (device_id in tracing spans) — 8 occurrences. Commands use `runtime.deps.device_identity.current_device_id()` purely for observability context, not business logic. This is legitimate cross-cutting concern access, not a use-case bypass.

2. **Business logic bypass** — 5 occurrences:
   - `clipboard.rs:29` — `runtime.deps.device_identity.current_device_id()` used to filter entries
   - `clipboard.rs:53` — `runtime.deps.encryption_session.is_ready()` for readiness check
   - `settings.rs:107` — `runtime.deps.settings.clone()` to resolve device name
   - `encryption.rs:957` — `runtime.deps.encryption_session.is_ready()` for state check

**Proposed fix — phased approach:**

**Phase A: Make `deps` private, expose needed accessors (immediate)**

1. Change `pub deps: AppDeps` to `deps: AppDeps` in `AppRuntime`.
2. Add targeted accessor methods on `AppRuntime` for legitimate cross-cutting needs:

```rust
impl AppRuntime {
    /// Device ID for observability spans (cross-cutting, not business logic).
    pub fn current_device_id(&self) -> String {
        self.deps.device_identity.current_device_id()
    }
}
```

3. The `UseCases` struct already has `runtime: &'a AppRuntime` — change its field accesses from `self.runtime.deps.X` to a private `AppRuntime::deps(&self) -> &AppDeps` method that is `pub(crate)`:

```rust
impl AppRuntime {
    /// Internal access for UseCases wiring only.
    pub(crate) fn deps(&self) -> &AppDeps {
        &self.deps
    }
}
```

**Phase B: Absorb business-logic bypasses into use cases (next sprint)**

4. Create `CheckEncryptionReadiness` use case in `uc-app` that encapsulates the `encryption_state + encryption_session.is_ready()` check pattern (used in clipboard.rs:53 and encryption.rs:957).
5. Add device_id as a return field on `ListClipboardEntries` use case output, removing the need for commands to fetch it separately.
6. The settings.rs:107 access should go through `UpdateSettings` use case (add device-name resolution as part of update flow).

**Files changed (Phase A):**

- `src-tauri/crates/uc-tauri/src/bootstrap/runtime.rs` — make `deps` private, add `pub(crate) fn deps()`, add `pub fn current_device_id()`
- `src-tauri/crates/uc-tauri/src/commands/pairing.rs` — replace `runtime.deps.device_identity` with `runtime.current_device_id()`
- `src-tauri/crates/uc-tauri/src/commands/encryption.rs` — same replacement
- `src-tauri/crates/uc-tauri/src/commands/settings.rs` — same replacement
- `src-tauri/crates/uc-tauri/src/commands/clipboard.rs` — same replacement

**Files changed (Phase B):**

- `src-tauri/crates/uc-app/src/usecases/` — new `check_encryption_readiness.rs`
- Commands using `runtime.deps.encryption_session.is_ready()` — replace with use case

---

### M4: Non-Domain Ports in uc-core

**Current state:** `UiPort` (open settings window) and `AutostartPort` (OS launch management) are defined in `uc-core/src/ports/`. They represent platform/application capabilities, not domain business logic. They violate the guideline "Does this port represent a business capability?" — NO.

**Why they ended up here:** `AppDeps` (in `uc-app`) holds `Arc<dyn UiPort>` and `Arc<dyn AutostartPort>`. Since `uc-app` depends on `uc-core`, defining them in `uc-core` was the only way to make the type available to `uc-app` without circular deps.

**Proposed fix — move to uc-app:**

Since `uc-app` is the consumer and `uc-tauri` provides the concrete implementations, define these traits in `uc-app` directly:

1. Move `uc-core/src/ports/ui_port.rs` → `uc-app/src/ports/ui_port.rs`
2. Move `uc-core/src/ports/autostart.rs` → `uc-app/src/ports/autostart.rs`
3. Create `uc-app/src/ports/mod.rs` to organize app-level ports
4. Update `AppDeps` and `App` to import from `uc_app::ports::*` instead of `uc_core::ports::*`
5. Update `uc-tauri` adapter implementations to import from `uc_app::ports`
6. Remove from `uc-core/src/ports/mod.rs`

**Candidate ports to also evaluate for move** (do NOT move yet — flag for review):

- `AppDirsPort` — provides file system paths, may be platform/infra
- `AppRuntimePort` — generic runtime abstraction, may belong in uc-app
- `WatcherControlPort` — controls clipboard watcher, arguably platform

**Decision criteria for future moves:** A port belongs in `uc-core` ONLY if it is referenced by domain models or domain services. If it is only referenced by use cases, it can live in `uc-app`. If only by adapters, it belongs in the same crate as the adapter interface.

**Files changed:**

- `src-tauri/crates/uc-core/src/ports/mod.rs` — remove `ui_port`, `autostart` modules and re-exports
- `src-tauri/crates/uc-app/src/ports/mod.rs` — new, defines `UiPort` and `AutostartPort`
- `src-tauri/crates/uc-app/src/lib.rs` — add `pub mod ports;`, update `App` struct imports
- `src-tauri/crates/uc-app/src/deps.rs` — update imports for `UiPort`, `AutostartPort`
- `src-tauri/crates/uc-tauri/src/adapters/` — update trait imports

---

### M5: Port Explosion (48 traits)

**Current state:** 48 `*Port` traits across `uc-core/src/ports/`. `AppDeps` has 30+ fields, each an `Arc<dyn SomePort>`. Testing requires mocking each independently.

**The real problem is not trait count but lack of cohesion grouping.** Some traits that always appear together should be consolidated. Others are fine as separate traits because they have independent lifecycles.

**Proposed fix — cohesion-based grouping, not arbitrary merging:**

**Step 1: Identify co-occurring trait clusters**

Analysis of `AppDeps` and use-case constructors reveals these clusters:

| Cluster           | Traits                                                                                                                                                              | Always co-injected?                                   |
| ----------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------- |
| Clipboard Storage | `ClipboardEntryRepositoryPort`, `ClipboardSelectionRepositoryPort`, `ClipboardRepresentationRepositoryPort`, `ClipboardEventWriterPort`, `ClipboardEventReaderPort` | Yes — every clipboard use case needs 3+ of these      |
| Encryption        | `EncryptionPort`, `EncryptionSessionPort`, `EncryptionStatePort`, `KeyScopePort`, `KeyMaterialPort`                                                                 | Yes — always wired together                           |
| Blob              | `BlobStorePort`, `BlobRepositoryPort`, `BlobWriterPort`                                                                                                             | Yes — blob operations always need all three           |
| Transfer Crypto   | `TransferPayloadEncryptorPort`, `TransferPayloadDecryptorPort`                                                                                                      | Separate — encryptor used outbound, decryptor inbound |

**Step 2: Create aggregate port traits (NOT merging — composing)**

```rust
// uc-core/src/ports/clipboard/repository_group.rs

/// Aggregate access to clipboard persistence.
///
/// Use cases that need clipboard storage should depend on this
/// rather than 4 separate port traits.
pub trait ClipboardStoragePorts: Send + Sync {
    fn entries(&self) -> &dyn ClipboardEntryRepositoryPort;
    fn selections(&self) -> &dyn ClipboardSelectionRepositoryPort;
    fn representations(&self) -> &dyn ClipboardRepresentationRepositoryPort;
    fn events_writer(&self) -> &dyn ClipboardEventWriterPort;
}
```

**Step 3: Provide a concrete grouping struct wired in uc-tauri**

```rust
// uc-infra or uc-tauri
pub struct ClipboardStorageGroup {
    pub entries: Arc<dyn ClipboardEntryRepositoryPort>,
    pub selections: Arc<dyn ClipboardSelectionRepositoryPort>,
    pub representations: Arc<dyn ClipboardRepresentationRepositoryPort>,
    pub events_writer: Arc<dyn ClipboardEventWriterPort>,
}

impl ClipboardStoragePorts for ClipboardStorageGroup {
    fn entries(&self) -> &dyn ClipboardEntryRepositoryPort { &*self.entries }
    // ...
}
```

**Step 4: Simplify AppDeps gradually**

Replace individual fields with grouped ports:

```rust
// Before: 5 clipboard fields
pub clipboard_entry_repo: Arc<dyn ClipboardEntryRepositoryPort>,
pub clipboard_event_repo: Arc<dyn ClipboardEventWriterPort>,
pub representation_repo: Arc<dyn ClipboardRepresentationRepositoryPort>,
pub selection_repo: Arc<dyn ClipboardSelectionRepositoryPort>,

// After: 1 grouped field
pub clipboard_storage: Arc<dyn ClipboardStoragePorts>,
```

**Important:** Individual traits remain defined — they are NOT deleted. The aggregate trait composes them. Use cases can depend on either the aggregate or individual traits depending on their actual needs. This is additive, not breaking.

**Apply same pattern to Encryption cluster and Blob cluster in subsequent PRs.**

**Files changed (first PR — clipboard storage grouping):**

- `src-tauri/crates/uc-core/src/ports/clipboard/mod.rs` — add `ClipboardStoragePorts` trait
- `src-tauri/crates/uc-tauri/src/bootstrap/wiring.rs` — create `ClipboardStorageGroup`
- `src-tauri/crates/uc-app/src/deps.rs` — add `clipboard_storage` field (keep old fields during transition)
- Use cases — migrate one at a time to accept `Arc<dyn ClipboardStoragePorts>`

---

### M6: tokio in Core Port Traits

**Current state:** Two port traits in `uc-core` return `tokio::sync::mpsc::Receiver<T>`:

- `NetworkEventPort::subscribe_events()` → `Result<tokio::sync::mpsc::Receiver<NetworkEvent>>`
- `ClipboardTransportPort::subscribe_clipboard()` → `Result<tokio::sync::mpsc::Receiver<(ClipboardMessage, Option<Vec<u8>>)>>`

`uc-core/Cargo.toml` has `tokio = { version = "1", features = ["sync"] }`.

**Why this matters:** Core domain should be runtime-agnostic. If we ever need to run core logic in a WASM environment or with a different async runtime, the tokio dependency blocks it. More practically, it makes the port contract tied to a specific channel implementation rather than expressing the semantic intent (a stream of events).

**Proposed fix — abstract with a core-defined stream type:**

**Option A (recommended): Use `futures::Stream` trait**

`futures-core` is a minimal crate (no runtime dependency) that defines the `Stream` trait. Replace `tokio::sync::mpsc::Receiver<T>` with `Pin<Box<dyn Stream<Item = T> + Send>>`:

```rust
// uc-core/src/ports/network_events.rs
use futures_core::Stream;
use std::pin::Pin;

pub type EventStream<T> = Pin<Box<dyn Stream<Item = T> + Send>>;

#[async_trait]
pub trait NetworkEventPort: Send + Sync {
    async fn subscribe_events(&self) -> Result<EventStream<NetworkEvent>>;
}
```

Adapters convert their internal `mpsc::Receiver` to a `Stream` via `tokio_stream::wrappers::ReceiverStream` (adapter-side dependency, not core).

**Option B (alternative): Define a custom `PortReceiver` in uc-core**

```rust
// uc-core/src/ports/channel.rs
#[async_trait]
pub trait PortReceiver<T: Send>: Send {
    async fn recv(&mut self) -> Option<T>;
}
```

Then implement it for `tokio::sync::mpsc::Receiver` in `uc-infra` or `uc-platform`.

**Recommendation:** Option A. `futures-core` is the de-facto standard, has zero transitive deps beyond `core`, and is already an indirect dependency via `async-trait`. It provides `Stream` which integrates with the entire async ecosystem.

**Migration steps:**

1. Add `futures-core = "0.3"` to `uc-core/Cargo.toml`
2. Define `EventStream<T>` type alias in `uc-core/src/ports/mod.rs`
3. Update `NetworkEventPort` and `ClipboardTransportPort` signatures
4. In `uc-platform` adapter: wrap `mpsc::Receiver` in `ReceiverStream` before returning
5. In consumers (`uc-tauri`/`uc-app`): use `StreamExt::next()` instead of `recv()`
6. Remove `tokio` from `uc-core/Cargo.toml` (if no other tokio usage remains)

**Files changed:**

- `src-tauri/crates/uc-core/Cargo.toml` — add `futures-core`, remove `tokio`
- `src-tauri/crates/uc-core/src/ports/mod.rs` — add `EventStream<T>` type alias
- `src-tauri/crates/uc-core/src/ports/network_events.rs` — change return type
- `src-tauri/crates/uc-core/src/ports/clipboard_transport.rs` — change return type
- `src-tauri/crates/uc-platform/src/adapters/libp2p_network.rs` — wrap receivers in `ReceiverStream`
- All consumers of `subscribe_events()`/`subscribe_clipboard()` — use `StreamExt`

---

## Boundaries to Protect

After all reforms are complete, the following invariants MUST hold:

1. **uc-core has ZERO workspace crate dependencies.** Only external crates allowed: `serde`, `async-trait`, `anyhow`, `chrono`, `futures-core`, `uuid`, `thiserror`. No `tokio`, no `uc-*`.

2. **uc-platform depends only on uc-core** (plus external crates). `cargo tree -p uc-platform` must NOT contain `uc-infra` or `uc-app`.

3. **uc-infra depends only on uc-core** (plus external crates). No `uc-platform`, no `uc-app`.

4. **uc-app depends only on uc-core.** No infra or platform crates.

5. **uc-tauri is the SOLE composition root.** It is the only crate that depends on all four others and wires concrete implementations to abstract ports.

6. **No command in uc-tauri accesses port implementations directly** — all business logic goes through `runtime.usecases().xxx()`. The only exception is observability metadata (device_id for spans), which uses dedicated `AppRuntime` accessor methods.

7. **All ports in uc-core represent domain/business capabilities.** Application-level ports (UI, autostart, app dirs) live in `uc-app/ports/`.

**CI enforcement:**

- Add `cargo tree` assertions to CI pipeline checking invariants 1-5
- Add a clippy lint or custom check that `runtime.deps` is not accessed outside `runtime.rs`

---

## Abstractions to Add / Remove / Split

### Add

| Abstraction                         | Location                               | Justification                                         |
| ----------------------------------- | -------------------------------------- | ----------------------------------------------------- |
| `StreamDecoderPort`                 | `uc-core/ports/clipboard_transport.rs` | Breaks H1 horizontal dependency                       |
| `EventStream<T>` type alias         | `uc-core/ports/mod.rs`                 | Replaces tokio channel in port signatures (M6)        |
| `ClipboardStoragePorts` aggregate   | `uc-core/ports/clipboard/`             | Reduces port explosion for clipboard persistence (M5) |
| `EncryptionPorts` aggregate         | `uc-core/ports/security/`              | Groups 5 always-co-injected encryption ports (M5)     |
| `CheckEncryptionReadiness` use case | `uc-app/usecases/`                     | Absorbs bypass pattern from commands (H2 Phase B)     |

### Move

| Abstraction     | From             | To              | Justification             |
| --------------- | ---------------- | --------------- | ------------------------- |
| `UiPort`        | `uc-core/ports/` | `uc-app/ports/` | Not a domain concept (M4) |
| `AutostartPort` | `uc-core/ports/` | `uc-app/ports/` | Not a domain concept (M4) |

### Remove (eventually)

| Abstraction                                | Justification                        |
| ------------------------------------------ | ------------------------------------ |
| `tokio` dep in `uc-core/Cargo.toml`        | After M6 migration, no longer needed |
| `uc-infra` dep in `uc-platform/Cargo.toml` | After H1 fix                         |
| `pub` on `AppRuntime.deps`                 | After H2 Phase A                     |

### Do NOT Remove

| Abstraction                                    | Why keep                                                   |
| ---------------------------------------------- | ---------------------------------------------------------- |
| Individual port traits (when aggregate exists) | Aggregate composes them — some use cases need only one     |
| `AppDeps` struct                               | Still useful as a flat wiring manifest in composition root |

---

## Risks & Trade-offs

### Risk 1: Aggregate ports add indirection without clear benefit for simple use cases

**Mitigation:** Aggregates are opt-in. Use cases that only need one trait can still depend on it directly. The aggregate is for use cases that need 3+ ports from the same cluster.

### Risk 2: Moving UiPort/AutostartPort to uc-app may create pressure to put more things there

**Mitigation:** Document clear criteria: "A port belongs in uc-core only if referenced by domain models or domain services." Establish a lightweight review gate for new port additions.

### Risk 3: StreamDecoderPort may be too narrow — only one call site

**Mitigation:** This is acceptable. The port exists to enforce the dependency rule, not for reuse. One port breaking one illegal dependency is a good trade-off. If more decode/encode operations emerge, the port can be generalized.

### Risk 4: futures-core adds a new dependency to uc-core

**Mitigation:** `futures-core` is effectively zero-cost (no-std compatible, no transitive deps beyond core). It is the Rust ecosystem standard for the `Stream` trait and is already an indirect dependency.

### Risk 5: Making `deps` private on AppRuntime may break downstream code we haven't audited

**Mitigation:** Use `pub(crate)` first, which allows `UseCases` (same crate) to still access deps while blocking external access. Compile check will reveal all breakage immediately.

### Trade-off: Incremental vs. clean-break migration

All proposals above support incremental migration. The trade-off is that during transition, both old patterns (individual ports in `AppDeps`) and new patterns (aggregate ports) coexist. This adds temporary complexity but avoids a risky big-bang refactor. The old patterns can be removed once all use cases are migrated.

---

## Pseudo-Solutions to Reject

### Pseudo-fix 1: "Re-export uc-infra types through uc-core"

**Temptation:** Add `pub use uc_infra::clipboard::ChunkedDecoder` in `uc-core` so `uc-platform` can import it "through" core.
**Why it fails:** This makes `uc-core` depend on `uc-infra`, which inverts the fundamental dependency rule. Core must have zero workspace dependencies. The symptom (platform importing infra) would be hidden but the actual coupling would be worse.

### Pseudo-fix 2: "Move ChunkedDecoder to uc-core"

**Temptation:** Since both platform and infra need it, put it in the shared core crate.
**Why it fails:** `ChunkedDecoder` contains XChaCha20-Poly1305 encryption logic and zstd compression — these are infrastructure concerns, not domain logic. Moving them to core would bloat the domain crate with crypto dependencies (`chacha20poly1305`, `zstd`).

### Pseudo-fix 3: "Add `#[allow(unused)]` or runtime.deps access behind a feature flag"

**Temptation:** Keep `deps` public but add a clippy configuration or feature flag to suppress warnings.
**Why it fails:** Feature flags don't prevent misuse — they hide it. The problem is structural (public field enabling bypass), not a linting issue. Compiler-enforced visibility (`pub(crate)`) is the correct tool.

### Pseudo-fix 4: "Merge all 48 ports into 5 god-traits"

**Temptation:** Create `ClipboardPort` with 30 methods to replace 15 clipboard-related traits.
**Why it fails:** God traits violate ISP (Interface Segregation Principle). A use case that only needs to read entries would be forced to depend on a trait that also includes write, thumbnail, and blob operations. Mocking becomes harder, not easier. The correct approach is cohesion-based grouping (aggregate traits that compose focused traits), not monolithic merging.

### Pseudo-fix 5: "Replace tokio::mpsc with std::sync::mpsc in port signatures"

**Temptation:** Use the standard library channel instead of tokio's.
**Why it fails:** `std::sync::mpsc::Receiver` is blocking, not async. Using it in async port traits would require `spawn_blocking` at every receive site, introducing unnecessary overhead and defeating the purpose of async ports. The correct abstraction is `Stream` from `futures-core`, which is runtime-agnostic and async-native.

### Pseudo-fix 6: "Make AppDeps fields private and add getters for each field"

**Temptation:** Add 30 getter methods like `fn clipboard_entry_repo(&self) -> Arc<dyn ClipboardEntryRepositoryPort>`.
**Why it fails:** This is a mechanical transformation that changes nothing architecturally. Commands would call `runtime.deps().clipboard_entry_repo()` instead of `runtime.deps.clipboard_entry_repo` — the bypass path is equally easy. The real fix is making `deps` accessible only to `UseCases` via `pub(crate)`, and providing purpose-specific methods (like `current_device_id()`) for legitimate cross-cutting needs.
