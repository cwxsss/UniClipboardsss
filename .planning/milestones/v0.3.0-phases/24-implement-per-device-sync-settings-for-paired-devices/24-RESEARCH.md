# Phase 24: Implement Per-Device Sync Settings for Paired Devices - Research

**Researched:** 2026-03-11
**Domain:** Full-stack per-device settings (Rust domain model + Diesel migration + Tauri commands + React UI wiring)
**Confidence:** HIGH

## Summary

This phase adds per-device sync settings to the existing paired device infrastructure. The current `PairedDevice` domain model has 6 fields (peer_id, pairing_state, identity_fingerprint, paired_at, last_seen_at, device_name) with no sync settings. The global `SyncSettings` struct in `uc-core/src/settings/model.rs` already defines the exact shape needed: `auto_sync`, `sync_frequency`, `content_types` (text, image, link, file, code_snippet, rich_text), and `max_file_size_mb`.

The implementation spans all layers of the hexagonal architecture: domain model extension in uc-core, Diesel migration + repository update in uc-infra, new use cases in uc-app, Tauri commands in uc-tauri, and frontend wiring in the existing `DeviceSettingsPanel.tsx` component (which currently has hardcoded placeholder data). The outbound sync engine (`SyncOutboundClipboardUseCase`) currently broadcasts to all sendable peers without per-device filtering -- this is the key integration point where per-device settings need to be checked.

**Primary recommendation:** Use `Option<SyncSettings>` on `PairedDevice` with a nullable JSON column in SQLite. When `None`, fall back to global settings. Reuse the existing `SyncSettings` / `ContentTypes` structs from `uc-core::settings::model` to avoid duplication.

<user_constraints>

## User Constraints (from CONTEXT.md)

### Locked Decisions

- Full per-device control: each paired device gets its own complete set of sync settings (auto_sync, content_types, sync_frequency)
- New devices inherit global settings as default when first paired
- Users can customize any device independently after pairing
- Add a JSON column (`sync_settings`) to the existing `paired_device` database table
- JSON stores a serialized `DeviceSyncSettings` struct (or null when using global defaults)
- Diesel migration required to add the column
- Device settings override global: if a device has custom settings, use those; otherwise fall back to global settings
- Clear mental model: global = default, per-device = override
- Users can "reset to global" by clearing per-device settings (setting JSON to null)
- Runtime check: sync logic queries per-device settings before each sync operation
- Falls back to global settings when no per-device override exists
- Settings changes take effect immediately -- no restart or reconnect required
- Settings are loaded from storage (not cached in memory with stale risk)

### Claude's Discretion

- Exact JSON schema for the sync_settings column
- Whether to use Option<DeviceSyncSettings> or a separate "use_global" flag
- Diesel migration strategy details
- How to structure the settings resolution logic (trait vs function)
- Frontend state management approach (extend devicesSlice vs new slice)

### Deferred Ideas (OUT OF SCOPE)

None -- discussion stayed within phase scope
</user_constraints>

## Standard Stack

### Core (Already in Project)

| Library                | Purpose                                     | Location                 |
| ---------------------- | ------------------------------------------- | ------------------------ |
| `diesel` (SQLite)      | ORM + migrations for paired_device table    | `uc-infra/src/db/`       |
| `serde` / `serde_json` | JSON serialization for sync_settings column | `uc-core`                |
| Redux Toolkit          | Frontend state management for devicesSlice  | `src/store/slices/`      |
| Tauri commands         | Frontend-backend IPC                        | `uc-tauri/src/commands/` |

### No New Dependencies Required

This phase uses only existing project dependencies. No new crates or npm packages needed.

## Architecture Patterns

### Recommended Approach

#### 1. Domain Model: Reuse `SyncSettings` directly

**Recommendation:** Use `Option<SyncSettings>` on `PairedDevice` rather than creating a new `DeviceSyncSettings` struct.

Rationale: The existing `SyncSettings` struct in `uc-core::settings::model` already contains exactly the fields needed (auto_sync, sync_frequency, content_types, max_file_size_mb). Creating a duplicate struct adds maintenance burden without benefit.

```rust
// uc-core/src/network/paired_device.rs
use crate::settings::model::SyncSettings;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairedDevice {
    pub peer_id: PeerId,
    pub pairing_state: PairingState,
    pub identity_fingerprint: String,
    pub paired_at: DateTime<Utc>,
    pub last_seen_at: Option<DateTime<Utc>>,
    pub device_name: String,
    #[serde(default)]
    pub sync_settings: Option<SyncSettings>,  // None = use global
}
```

Note: `SyncSettings` currently derives `Clone` and `Serialize/Deserialize` but NOT `PartialEq/Eq`. The `PairedDevice` struct derives `PartialEq, Eq`. Either: (a) add `PartialEq, Eq` to `SyncSettings`, `SyncFrequency`, and `ContentTypes`, or (b) remove `PartialEq, Eq` from `PairedDevice`. Option (a) is preferred since `SyncFrequency` already derives `PartialEq, Eq`, and `ContentTypes` with its bool fields can trivially derive it.

#### 2. Settings Resolution: Pure Function

**Recommendation:** A standalone function rather than a trait, since the logic is simple and stateless.

```rust
// uc-core/src/network/paired_device.rs (or a new resolve module)
pub fn resolve_sync_settings<'a>(
    device: &'a PairedDevice,
    global: &'a SyncSettings,
) -> &'a SyncSettings {
    device.sync_settings.as_ref().unwrap_or(global)
}
```

#### 3. Database: Nullable JSON Column

**Migration SQL:**

```sql
ALTER TABLE paired_device ADD COLUMN sync_settings TEXT DEFAULT NULL;
```

The column stores `NULL` (use global) or a JSON string (custom settings). This aligns with how SQLite handles JSON -- as plain TEXT with serde doing the parsing in Rust.

#### 4. Diesel Schema + Row Model Updates

The `PairedDeviceRow` and `NewPairedDeviceRow` need a new `sync_settings: Option<String>` field. The mapper handles JSON serialization/deserialization:

```rust
// In PairedDeviceRowMapper::to_row
sync_settings: domain.sync_settings.as_ref()
    .map(|s| serde_json::to_string(s))
    .transpose()
    .map_err(|e| anyhow!("failed to serialize sync_settings: {}", e))?,

// In PairedDeviceRowMapper::to_domain
sync_settings: row.sync_settings.as_ref()
    .map(|json| serde_json::from_str(json))
    .transpose()
    .map_err(|e| anyhow!("failed to deserialize sync_settings: {}", e))?,
```

#### 5. Repository Port Extension

Add two new methods to `PairedDeviceRepositoryPort`:

```rust
async fn update_sync_settings(
    &self,
    peer_id: &PeerId,
    settings: Option<SyncSettings>,
) -> Result<(), PairedDeviceRepositoryError>;

async fn get_sync_settings(
    &self,
    peer_id: &PeerId,
) -> Result<Option<SyncSettings>, PairedDeviceRepositoryError>;
```

Alternative: The existing `upsert` already updates all fields, so `get_sync_settings` could simply use `get_by_peer_id`. But a dedicated `update_sync_settings` avoids needing to load-modify-save the entire device when only settings change.

#### 6. Use Cases

Two new use cases:

- `GetDeviceSyncSettings` -- returns resolved settings (per-device or global fallback)
- `UpdateDeviceSyncSettings` -- updates per-device settings (or clears to null for reset-to-global)

#### 7. Tauri Commands

Two new commands:

- `get_device_sync_settings(peer_id: String)` -> returns resolved `SyncSettings`
- `update_device_sync_settings(peer_id: String, settings: Option<SyncSettings>)` -> updates or clears

#### 8. Sync Engine Integration

The `SyncOutboundClipboardUseCase` currently iterates `sendable_peers` and sends to all. The per-device settings check should happen in the send loop:

```rust
// For each peer in sendable_peers:
// 1. Load device from paired_device_repo
// 2. Resolve sync settings (per-device or global)
// 3. Check auto_sync flag
// 4. Check content_types against current clipboard content type
// 5. Skip peer if settings say don't sync
```

**Important:** The use case needs access to `PairedDeviceRepositoryPort` and `SettingsPort` (already has `SettingsPort`). Add `PairedDeviceRepositoryPort` as a dependency.

#### 9. Frontend State Management

**Recommendation:** Extend `devicesSlice` with new thunks rather than creating a new slice. The device sync settings are closely tied to the device entity.

```typescript
// New thunks in devicesSlice
export const fetchDeviceSyncSettings = createAsyncThunk(...)
export const updateDeviceSyncSettings = createAsyncThunk(...)

// New API functions in src/api/p2p.ts
export async function getDeviceSyncSettings(peerId: string): Promise<SyncSettings> { ... }
export async function updateDeviceSyncSettings(peerId: string, settings: SyncSettings | null): Promise<void> { ... }
```

#### 10. Frontend Component Wiring

`DeviceSettingsPanel.tsx` currently renders hardcoded sync rules. Wire it to:

1. Load per-device settings via `getDeviceSyncSettings(peerId)` on mount
2. Use controlled state (not `defaultChecked`) for toggle inputs
3. Call `updateDeviceSyncSettings` on toggle change
4. Implement "Restore Defaults" button to set settings to `null`

### Project Structure (Files to Create/Modify)

```
src-tauri/crates/
  uc-core/src/
    network/paired_device.rs           # Add sync_settings field
    settings/model.rs                  # Add PartialEq/Eq derives to SyncSettings, ContentTypes
  uc-infra/
    migrations/2026-03-11-000001_add_paired_device_sync_settings/
      up.sql                           # ALTER TABLE ADD COLUMN
      down.sql                         # ALTER TABLE DROP COLUMN
    src/db/
      schema.rs                        # Auto-regenerated by diesel
      models/paired_device_row.rs      # Add sync_settings field
      mappers/paired_device_mapper.rs  # JSON ser/de logic
      repositories/paired_device_repo.rs  # New update_sync_settings impl
  uc-app/src/usecases/
    pairing/
      get_device_sync_settings.rs      # New use case
      update_device_sync_settings.rs   # New use case
      mod.rs                           # Register new use cases
  uc-tauri/src/
    commands/pairing.rs                # New commands + PairedPeer DTO update
    bootstrap/runtime.rs               # Register new use case accessors

src/
  api/p2p.ts                          # New API functions
  store/slices/devicesSlice.ts         # New thunks
  components/device/DeviceSettingsPanel.tsx  # Wire to real data
```

### Anti-Patterns to Avoid

- **Duplicating SyncSettings struct:** Don't create `DeviceSyncSettings` as a copy of `SyncSettings`. Reuse the existing struct directly.
- **Caching settings in memory:** The user decision explicitly states "loaded from storage, not cached." Always query the repo.
- **Modifying the upsert ON CONFLICT SET for sync_settings:** The existing `upsert` method updates all fields. When adding `sync_settings` to the upsert, be careful not to accidentally overwrite custom settings with `None` during pairing upserts. Best practice: the pairing flow's upsert should preserve existing sync_settings (use `COALESCE` in SQL or handle in code).

## Don't Hand-Roll

| Problem               | Don't Build                 | Use Instead                                  | Why                                                             |
| --------------------- | --------------------------- | -------------------------------------------- | --------------------------------------------------------------- |
| JSON in SQLite        | Custom binary encoding      | `serde_json` + TEXT column                   | Standard pattern, human-readable, debuggable                    |
| Settings resolution   | Complex inheritance chain   | Simple `Option::unwrap_or`                   | The override model is flat (device or global), not hierarchical |
| Form state management | Manual React state tracking | Redux Toolkit thunks + controlled components | Consistent with project patterns                                |

## Common Pitfalls

### Pitfall 1: Diesel Schema Column Ordering

**What goes wrong:** Diesel `Queryable` derives fields by position, not name. If `sync_settings` is added to the schema but the struct field order doesn't match the column order in `schema.rs`, queries will silently read wrong data or fail.
**How to avoid:** After running `diesel migration run`, regenerate schema with `diesel print-schema > schema.rs`. Ensure `PairedDeviceRow` field order matches the schema column order exactly. The new column will be last.

### Pitfall 2: Upsert Overwriting Sync Settings

**What goes wrong:** The existing `upsert` method's `ON CONFLICT DO UPDATE SET` clause will be updated to include `sync_settings`. During pairing flow, `upsert` is called with a new `PairedDevice` that has `sync_settings: None`, which would overwrite any existing custom settings.
**How to avoid:** Either: (a) make upsert exclude sync_settings from the update set, or (b) ensure pairing code loads existing settings before upserting, or (c) use a separate `update_sync_settings` method that only touches that column.
**Recommendation:** Option (c) -- keep `upsert` focused on pairing lifecycle fields. Add a dedicated `update_sync_settings` that only touches the sync_settings column.

### Pitfall 3: SyncSettings PartialEq Derivation

**What goes wrong:** `PairedDevice` derives `PartialEq, Eq`. Adding `Option<SyncSettings>` requires `SyncSettings` to also derive these. `SyncSettings` contains `max_file_size_mb: u32` and enum types which can trivially derive `PartialEq/Eq`, but if any field type changes in the future to f64 or similar, this breaks.
**How to avoid:** Add `PartialEq, Eq` to `SyncSettings`, `ContentTypes`. Both are safe (only bools, enums, and u32 fields). Also verify `SyncFrequency` already derives it (confirmed: it does).

### Pitfall 4: Frontend Stale State After Settings Update

**What goes wrong:** User changes per-device settings, but the UI doesn't reflect the change because `DeviceSettingsPanel` uses `defaultChecked` (uncontrolled).
**How to avoid:** Convert to controlled components with React state. After `updateDeviceSyncSettings` succeeds, update local state or re-fetch.

### Pitfall 5: Sync Engine Performance

**What goes wrong:** Loading per-device settings from DB for every peer on every clipboard change adds latency.
**How to avoid:** The user explicitly chose "load from storage, not cached." Accept the DB query cost. SQLite with WAL mode and connection pooling is fast enough for the small number of paired devices (typically 2-5). The `PairedDeviceRepositoryPort::get_by_peer_id` already does a single-row query.

## Code Examples

### Diesel Migration

```sql
-- up.sql
ALTER TABLE paired_device ADD COLUMN sync_settings TEXT DEFAULT NULL;

-- down.sql
-- SQLite doesn't support DROP COLUMN before 3.35.0
-- Use a table recreation approach if needed, or:
ALTER TABLE paired_device DROP COLUMN sync_settings;
```

### Mapper JSON Handling

```rust
// Source: project pattern from existing mappers
impl InsertMapper<PairedDevice, NewPairedDeviceRow> for PairedDeviceRowMapper {
    fn to_row(&self, domain: &PairedDevice) -> Result<NewPairedDeviceRow> {
        let sync_settings_json = domain.sync_settings.as_ref()
            .map(|s| serde_json::to_string(s))
            .transpose()
            .map_err(|e| anyhow!("serialize sync_settings: {}", e))?;

        Ok(NewPairedDeviceRow {
            // ... existing fields ...
            sync_settings: sync_settings_json,
        })
    }
}
```

### Settings Resolution in Sync Engine

```rust
// In SyncOutboundClipboardUseCase::execute_async, inside the peer loop:
let device = self.paired_device_repo.get_by_peer_id(&PeerId::from(peer.peer_id.as_str())).await?;
let global_settings = self.settings.load().await?;
let effective_settings = device
    .and_then(|d| d.sync_settings.clone())
    .unwrap_or(global_settings.sync.clone());

if !effective_settings.auto_sync {
    debug!(peer_id = %peer.peer_id, "Skipping sync for peer: auto_sync disabled");
    continue;
}
```

### Frontend API Call Pattern

```typescript
// Source: project pattern from existing api/p2p.ts
export async function getDeviceSyncSettings(peerId: string): Promise<SyncSettings> {
  return await invokeWithTrace<SyncSettings>('get_device_sync_settings', { peerId })
}

export async function updateDeviceSyncSettings(
  peerId: string,
  settings: SyncSettings | null
): Promise<void> {
  await invokeWithTrace('update_device_sync_settings', { peerId, settings })
}
```

## State of the Art

| Old Approach                     | Current Approach                          | Impact                                                |
| -------------------------------- | ----------------------------------------- | ----------------------------------------------------- |
| Global sync settings only        | Per-device overrides with global fallback | Each paired device can have independent sync behavior |
| Hardcoded DeviceSettingsPanel UI | Data-driven from backend                  | Settings actually persist and affect sync behavior    |

## Open Questions

1. **Content type filtering in sync engine**
   - What we know: `ContentTypes` has 6 boolean fields (text, image, link, file, code_snippet, rich_text). The outbound sync currently sends `SystemClipboardSnapshot` representations without type classification.
   - What's unclear: How to map clipboard representation MIME types to ContentTypes categories. E.g., is `text/html` classified as `text`, `rich_text`, or both?
   - Recommendation: For this phase, implement `auto_sync` toggle only as the primary per-device filter. Content type filtering requires a MIME-to-ContentType classification system that can be added as a follow-up. The UI should render content type toggles but mark them as "coming soon" if not wired.

2. **Sync frequency per device**
   - What we know: `SyncFrequency` has `Realtime` and `Interval` variants. Current sync is event-driven (realtime).
   - What's unclear: What "Interval" mode means for per-device sync. Is it a polling interval? A debounce window?
   - Recommendation: Store the value but only honor `auto_sync` toggle for now. Interval-based sync is a separate feature.

## Sources

### Primary (HIGH confidence)

- Project codebase: `uc-core/src/settings/model.rs` -- SyncSettings, ContentTypes, SyncFrequency structs
- Project codebase: `uc-core/src/network/paired_device.rs` -- PairedDevice domain model
- Project codebase: `uc-infra/src/db/` -- Diesel schema, models, mappers, repositories
- Project codebase: `uc-app/src/usecases/clipboard/sync_outbound.rs` -- outbound sync logic
- Project codebase: `uc-tauri/src/commands/pairing.rs` -- existing Tauri pairing commands
- Project codebase: `src/components/device/DeviceSettingsPanel.tsx` -- existing UI placeholder
- Project codebase: `uc-infra/migrations/` -- existing migration structure (8 migrations)

### Secondary (MEDIUM confidence)

- Diesel documentation for `embed_migrations!` and ALTER TABLE handling with SQLite

## Metadata

**Confidence breakdown:**

- Standard stack: HIGH -- all components already exist in the project, no new dependencies
- Architecture: HIGH -- follows established hexagonal architecture patterns exactly
- Pitfalls: HIGH -- derived from direct codebase analysis (e.g., Diesel column ordering, upsert behavior)
- Sync engine integration: MEDIUM -- content type filtering deferred, auto_sync toggle is straightforward

**Research date:** 2026-03-11
**Valid until:** 2026-04-11 (stable -- internal architecture, no external dependency risk)
