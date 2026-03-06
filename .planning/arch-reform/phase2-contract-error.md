# Contract & Error Model Reviewer — Phase 2 Reform Proposal

## Root Cause Analysis

All six issues share a common root cause: **the absence of explicit boundary contracts between architectural layers**. When the project began, the fastest path was to reuse domain types directly at the IPC boundary and to use `anyhow::Result` as a universal error wrapper. This worked initially but now creates three systemic problems:

1. **Coupling at the IPC boundary** — domain models leak to the frontend (H10), and domain changes can silently break the API contract.
2. **Untyped error propagation** — `String` errors at the command layer (H11) and `anyhow::Result` in port traits (M11) make error handling purely syntactic, not semantic.
3. **Phantom and overlapping contracts** — `EncryptionState::Initializing` is declared but never produced (M9), `peer_id` in `NetworkEvent` uses `String` instead of `PeerId` (M12), and config/settings have unclear ownership (M13).

---

## Reform Proposals by Issue

### H10: Domain Models Exposed as DTOs

**Current state**: `list_paired_devices` (pairing.rs:72) returns `Vec<PairedDevice>` directly. `PairedDevice` is a domain model in `uc-core::network::paired_device` that derives `Serialize/Deserialize` and contains `PeerId` (a domain newtype), `PairingState` (a domain enum), and internal fields like `identity_fingerprint`. The `get_settings` command returns `Settings` (a core domain model) serialized via `serde_json::to_value`.

**Root cause**: Domain types derive `Serialize/Deserialize` for persistence (infra layer), and the command layer reuses this same serialization for IPC. This means the domain shape IS the API contract — any domain refactoring breaks the frontend.

**Proposed reform**:

1. **Define IPC DTOs in `uc-tauri/src/models/`** (already started for clipboard — extend to pairing and settings):

   ```
   uc-tauri/src/models/
   ├── mod.rs                    # existing clipboard DTOs
   ├── pairing.rs                # PairedDeviceDto, P2PPeerInfo (move from commands)
   └── settings.rs               # SettingsDto (if shape differs from domain)
   ```

2. **Naming convention**: `{DomainConcept}Dto` for IPC types, `{DomainConcept}Projection` for list/summary views.

3. **Mapping responsibility**: The command function performs the mapping. Use `From<DomainModel> for Dto` impls defined in `uc-tauri/src/models/` (the IPC boundary crate).

4. **Incremental migration**:
   - Phase A: Create `PairedDeviceDto` in `uc-tauri/src/models/pairing.rs`, add `From<PairedDevice>` impl, update `list_paired_devices` to map.
   - Phase B: Move `P2PPeerInfo`, `PairedPeer`, `P2PPairingResponse` from `commands/pairing.rs` to `models/pairing.rs`.
   - Phase C: Remove `Serialize`/`Deserialize` from `PairedDevice` in uc-core if no infra layer needs JSON serialization (database uses its own mapper). If infra still needs serde, keep it but never use domain types directly in command return types.

5. **Settings**: `get_settings` currently serializes `Settings` to `serde_json::Value` — this is actually reasonable as a pass-through, but `update_settings` deserializes `Value` directly into the domain `Settings`. A `SettingsDto` should be introduced only if the frontend shape needs to diverge from the persisted shape. For now, the `Settings` model already lives in `uc-core::settings::model` and is designed as a pure data model. This is the lowest-priority H10 fix.

**Boundary rule**: Domain models in `uc-core` should NOT derive `Serialize/Deserialize` unless required by the persistence layer. If persistence and IPC both need serialization, use separate DTOs in each boundary crate.

---

### H11: String-Unified Command Errors

**Current state**: Every Tauri command returns `Result<T, String>`. The centralized `error.rs` contains only `pub fn map_err(err: anyhow::Error) -> String { err.to_string() }`. The frontend cannot distinguish between "encryption not initialized", "entry not found", "network timeout", or "internal server error".

**Root cause**: Tauri's command error boundary requires `Serialize + Into<InvokeError>`. The simplest type satisfying this is `String`. No structured error type was introduced.

**Proposed reform**:

1. **Define a `CommandError` enum in `uc-tauri/src/commands/error.rs`**:

   ```rust
   #[derive(Debug, Serialize)]
   #[serde(tag = "code", content = "detail")]
   pub enum CommandError {
       NotFound { resource: String, id: String },
       NotReady { reason: String },
       InvalidInput { field: String, message: String },
       Unauthorized { message: String },
       Internal { message: String },
   }

   impl From<CommandError> for tauri::ipc::InvokeError {
       fn from(e: CommandError) -> Self {
           // Serialize to JSON for structured frontend consumption
           tauri::ipc::InvokeError::from(serde_json::to_value(e).unwrap_or_default())
       }
   }
   ```

2. **Frontend contract**: Errors arrive as JSON with a `code` discriminator. The frontend can pattern-match on `code` for retry logic, user messaging, and error categorization.

3. **Incremental migration**:
   - Phase A: Define `CommandError` enum. Add `impl From<anyhow::Error> for CommandError` that maps to `Internal`.
   - Phase B: Migrate one command family (e.g., clipboard commands) from `Result<T, String>` to `Result<T, CommandError>`, mapping specific use case errors to specific variants.
   - Phase C: Migrate remaining command families one by one.
   - Phase D: Remove `map_err(|e| e.to_string())` calls.

4. **Error translation pattern** (command layer maps use case errors to command errors):
   ```rust
   // In clipboard commands:
   use_case.execute(&parsed_id).await.map_err(|e| match e.downcast_ref::<ClipboardError>() {
       Some(ClipboardError::EntryNotFound(id)) => CommandError::NotFound {
           resource: "clipboard_entry".into(), id: id.to_string()
       },
       _ => CommandError::Internal { message: e.to_string() },
   })?;
   ```

---

### M9: EncryptionState Contract Mismatch

**Current state**: `EncryptionState` enum in `uc-core/src/security/state.rs` has three variants: `Uninitialized`, `Initializing`, `Initialized`. The only implementation (`EncryptionStateRepository` in `uc-infra/src/security/encryption_state.rs`) checks for a marker file and returns only `Uninitialized` or `Initialized`. Line 45 has an explicit `// TODO: 需要识别出 Initializing 的情况`.

**Root cause**: The `Initializing` variant was added speculatively for a future transient state but was never wired into any state transition. The marker-file approach is inherently binary (file exists or not).

**Two valid fixes (choose one)**:

**Option A: Remove `Initializing` variant** (recommended if no transient state is needed):

- Remove `Initializing` from `EncryptionState` enum.
- Remove the TODO comment from the infra implementation.
- Audit all `match` arms that handle `Initializing` (there are none currently — confirm via grep).
- This is the simplest and most honest fix.

**Option B: Implement `Initializing` as a process-scoped transient state**:

- Keep `Initializing` in the enum.
- Track it in-memory (not persisted to disk) — e.g., set before the key derivation begins, clear it on success/failure.
- This requires adding a `set_initializing()` method to `EncryptionStatePort`.
- Only do this if there is a concrete use case (e.g., showing a progress indicator during first-time setup).

**Recommendation**: Option A. Remove the variant. If a transient state is needed later, it can be re-added with a proper implementation. A phantom variant in the contract is worse than a missing variant.

**Migration**: Single commit — remove variant, remove TODO, add a comment explaining the design decision.

---

### M11: anyhow::Result in 33 Port Traits

**Current state**: 33+ port trait methods in `uc-core/src/ports/` use `anyhow::Result`. This includes:

- Repository ports (clipboard, blob, thumbnail, settings, paired device, etc.)
- Infrastructure ports (blob_store, hash, autostart, UI, network, pairing transport)
- Platform ports (system clipboard, watcher control, discovery)

Some ports already use typed errors: `DeviceRepositoryPort` uses `DeviceRepositoryError`, `PairedDeviceRepositoryPort` uses `PairedDeviceRepositoryError`, `EncryptionPort` uses `EncryptionError`, `WatcherControlPort` uses `WatcherControlError`.

**Root cause**: `anyhow::Result` was the fastest way to get the port trait compiling during the hexagonal architecture migration. Typed errors require defining error enums for each port family, which was deferred.

**Proposed reform**:

1. **Error taxonomy by domain cluster**:

   ```
   uc-core/src/ports/errors.rs (extend existing file):
   ├── ClipboardRepositoryError    — for all clipboard repo ports
   ├── BlobError                   — for blob_store, blob_repository, blob_writer
   ├── SettingsError               — for settings port
   ├── NetworkError                — for peer_directory, clipboard_transport, pairing_transport
   ├── SecurityError               — already exists partially (EncryptionError, EncryptionStateError)
   ```

2. **Migration strategy (incremental, port-by-port)**:
   - **Priority 1**: Ports called from use cases that need error discrimination (settings, blob, clipboard repos).
   - **Priority 2**: Ports with existing typed errors nearby (extend existing error enums).
   - **Priority 3**: Infrastructure-only ports where callers currently just log and propagate (autostart, UI, hash).

3. **Migration pattern for one port**:

   ```rust
   // Before (anyhow):
   async fn load(&self) -> anyhow::Result<Settings>;

   // After (typed):
   async fn load(&self) -> Result<Settings, SettingsError>;
   ```

   Each infra implementation changes its `?` operators to use `.map_err()` or `impl From<IoError> for SettingsError`.

4. **Constraint**: `anyhow` remains acceptable INSIDE use case bodies as an implementation detail for ad-hoc error chaining. It is NOT acceptable in port trait signatures because ports define the contract between layers.

5. **Estimated effort**: ~2-3 PRs per port cluster, each self-contained and independently mergeable.

---

### M12: NetworkEvent peer_id as String

**Current state**: In `uc-core/src/network/events.rs`, `NetworkEvent` variants use `String` for `peer_id`:

```rust
PeerLost(String),
PeerNameUpdated { peer_id: String, device_name: String },
PeerDisconnected(String),
PeerReady { peer_id: String },
// ... 8+ more variants with String peer_id
```

Meanwhile, `PairedDevice` uses the typed `PeerId` newtype, and `DiscoveredPeer`/`ConnectedPeer` use `String`.

**Root cause**: `NetworkEvent` originated in the infrastructure layer (libp2p adapter) where `peer_id` was a string from the wire. When it was promoted to uc-core as a domain event, the `String` type was not upgraded to `PeerId`.

**Proposed reform**:

1. **Phase A**: Change `DiscoveredPeer.peer_id` and `ConnectedPeer.peer_id` from `String` to `PeerId`.
2. **Phase B**: Change all `NetworkEvent` variants from `String` to `PeerId` for peer_id fields.
3. **Phase C**: Update the infra adapter (libp2p event handler) to construct `PeerId::from(string)` at the boundary.
4. **Phase D**: Update command layer mapping (pairing.rs) to use `peer_id.as_str()` where frontend-bound strings are needed.

**Risk**: `PeerId` derives `Serialize/Deserialize`, so JSON compatibility is preserved. The main risk is the number of call sites that construct or destructure `NetworkEvent` — each needs updating. Estimate: 1 PR, touching ~15-20 files across uc-core, uc-platform, and uc-tauri.

**Note**: `PeerId` already implements `From<String>`, `From<&str>`, `Display`, and `Serialize/Deserialize`, so the migration is mechanical.

---

### M13: config/settings Responsibility Overlap

**Current state**: There are three configuration/settings systems:

1. **`uc-core::config::AppConfig`** — TOML-based config DTO for infrastructure paths (vault_key_path, database_path, webserver_port). Loaded at startup from `config.toml` (development) or system defaults (production). Used in `main.rs` to wire dependencies.

2. **`uc-core::settings::model::Settings`** — JSON-based user-facing settings (theme, sync, security, pairing). Persisted via `SettingsPort` (implemented by `FileSettingsRepository` in uc-infra). Used by use cases and the frontend.

3. **Legacy `config/` directory** — `src-tauri/src/config/` no longer exists (already removed). The legacy `SETTING` RwLock referenced in CLAUDE.md appears to be from the old architecture and is no longer present.

**Analysis**: The overlap is less severe than initially suspected. The two remaining config systems serve genuinely different purposes:

- `AppConfig`: Infrastructure bootstrap (paths, ports) — read once at startup, immutable during runtime.
- `Settings`: User preferences — mutable at runtime, persisted to JSON, exposed to frontend.

**Remaining concerns**:

- `AppConfig.device_name` overlaps with `Settings.general.device_name`. Both can provide a device name, and `main.rs` resolves priority between them.
- `AppConfig.silent_start` overlaps with `Settings.general.silent_start`.

**Proposed reform**:

1. **Clarify ownership**: `AppConfig` owns infrastructure/bootstrap configuration (paths, ports). `Settings` owns user-visible preferences. Neither should duplicate the other.
2. **Remove overlapping fields from `AppConfig`**: `device_name` and `silent_start` should be read exclusively from `Settings`. `AppConfig` should only contain fields that the `Settings` model does not cover (paths, ports).
3. **Rename `AppConfig` to `BootstrapConfig`** to clarify its role as a startup-only, infrastructure-level configuration.
4. **Document the boundary**: Add a module-level doc comment explaining which system owns what.

**Migration**: 1 PR to remove overlapping fields and rename. Low risk since `AppConfig` is only used in `main.rs` and `bootstrap/init.rs`.

---

## Boundaries to Protect

1. **IPC Boundary** (`uc-tauri/commands/` → frontend): Only DTOs from `uc-tauri/models/` cross this boundary. Domain types from `uc-core` never appear in command return types. Errors are `CommandError`, not `String`.

2. **Port Boundary** (`uc-core/ports/` ← `uc-infra/` and `uc-platform/`): Port trait signatures use typed domain errors, not `anyhow::Result`. Implementations translate infrastructure errors to domain errors at this boundary.

3. **Domain Event Boundary** (`NetworkEvent` and other domain events): All identity types use domain newtypes (`PeerId`, `DeviceId`, `EntryId`), not raw `String`.

4. **Settings Boundary**: `Settings` is the single source of truth for user preferences. `AppConfig`/`BootstrapConfig` is the single source of truth for infrastructure paths. No field exists in both.

---

## Abstractions to Add / Remove / Split

### Add

- `CommandError` enum in `uc-tauri/src/commands/error.rs` — structured error type for the IPC boundary.
- Per-cluster error enums in `uc-core/src/ports/errors.rs` — `ClipboardRepositoryError`, `BlobError`, `SettingsError`, `NetworkError`.
- DTO types in `uc-tauri/src/models/pairing.rs` — `PairedDeviceDto` as IPC representation of `PairedDevice`.

### Remove

- `EncryptionState::Initializing` variant (M9 — phantom variant).
- `AppConfig.device_name` and `AppConfig.silent_start` (M13 — duplicate fields).
- `pub fn map_err(err: anyhow::Error) -> String` in `error.rs` — replaced by `CommandError::from()`.

### Split

- Nothing to split. The existing module boundaries are correct; the problem is missing types within those modules, not wrong module structure.

---

## Risks & Trade-offs

1. **DTO proliferation**: Adding DTOs for every domain model increases boilerplate. Mitigated by: only creating DTOs where the domain model shape differs from the IPC shape, or where domain models contain internal/sensitive fields.

2. **Error enum explosion**: Per-port error enums could lead to many small error types. Mitigated by: grouping errors by domain cluster (not per-port), and keeping variant sets small.

3. **Migration churn**: Changing 33 port traits from `anyhow::Result` to typed errors touches many files. Mitigated by: doing it incrementally per cluster, and accepting temporary coexistence of anyhow and typed errors.

4. **Frontend breaking changes**: Changing error format from `String` to structured JSON will break existing frontend error handling. Mitigated by: coordinating with frontend, and initially supporting both formats via a feature flag or wrapper.

5. **`PeerId` serialization**: Changing `String` to `PeerId` in `NetworkEvent` may affect event serialization format if `PeerId` serializes differently from `String`. Verified: `PeerId` is a newtype with `#[derive(Serialize, Deserialize)]` over `String`, so JSON format is identical.

---

## Pseudo-Solutions to Reject

1. **"Add `#[derive(Serialize)]` to all domain models"** — This is the current state and the root cause of H10. Adding serde derives to domain models for IPC convenience couples the domain to serialization format. Reject.

2. **"Use `thiserror` in port traits but keep `anyhow::Result` as the return type"** — This doesn't fix M11. The problem is the return type signature, not the error implementation. Port callers still can't match on error variants. Reject.

3. **"Create a single `AppError` enum for all errors"** — A monolithic error enum violates separation of concerns. Each layer should have its own error type that maps to the next layer's type. A single `AppError` would grow to 50+ variants and couple all layers. Reject.

4. **"Implement `EncryptionState::Initializing` via a second marker file"** — Over-engineering for a state that has no current consumer. File-based state is inherently binary. If transient state is needed, use in-memory state. Reject the marker-file approach.

5. **"Use `String` consistently everywhere instead of `PeerId`"** — This makes M12 worse, not better. The typed `PeerId` newtype exists specifically for type safety. The fix is to use it consistently, not abandon it. Reject.

6. **"Merge `AppConfig` and `Settings` into a single model"** — These serve fundamentally different purposes (bootstrap vs. runtime preferences). Merging them would create a god-object that must be available at two different lifecycle phases. Reject.
