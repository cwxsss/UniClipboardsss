# Phase 24: Implement Per-Device Sync Settings for Paired Devices - Context

**Gathered:** 2026-03-11
**Status:** Ready for planning

<domain>
## Phase Boundary

Enable users to configure sync settings on a per-device basis for each paired device. Each paired device can have its own auto_sync toggle, content type filters, and sync frequency — overriding the global sync settings when customized. This phase covers the full stack: domain model, database storage, backend use cases/commands, and frontend UI wiring.

</domain>

<decisions>
## Implementation Decisions

### Per-device settings scope
- Full per-device control: each paired device gets its own complete set of sync settings (auto_sync, content_types, sync_frequency)
- New devices inherit global settings as default when first paired
- Users can customize any device independently after pairing

### Storage approach
- Add a JSON column (`sync_settings`) to the existing `paired_device` database table
- JSON stores a serialized `DeviceSyncSettings` struct (or null when using global defaults)
- Diesel migration required to add the column

### Settings override behavior
- Device settings override global: if a device has custom settings, use those; otherwise fall back to global settings
- Clear mental model: global = default, per-device = override
- Users can "reset to global" by clearing per-device settings (setting JSON to null)

### Sync engine integration
- Runtime check: sync logic queries per-device settings before each sync operation
- Falls back to global settings when no per-device override exists
- Settings changes take effect immediately — no restart or reconnect required
- Settings are loaded from storage (not cached in memory with stale risk)

### Claude's Discretion
- Exact JSON schema for the sync_settings column
- Whether to use Option<DeviceSyncSettings> or a separate "use_global" flag
- Diesel migration strategy details
- How to structure the settings resolution logic (trait vs function)
- Frontend state management approach (extend devicesSlice vs new slice)

</decisions>

<specifics>
## Specific Ideas

No specific requirements — open to standard approaches

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `DeviceSettingsPanel.tsx`: Already has placeholder UI with hardcoded sync rules (autoSync, syncText, syncImage, syncFile) — needs wiring to real data
- `PairedDevicesPanel.tsx`: Lists paired devices with expandable settings panel — integration point for per-device settings
- `SyncSettings` struct in `uc-core/src/settings/model.rs`: Global sync settings model with auto_sync, sync_frequency, content_types, max_file_size_mb
- `ContentTypes` struct: Boolean fields for text, image, link, file, code_snippet, rich_text
- `PairedDeviceRepositoryPort`: Existing trait with get_by_peer_id, list_all, upsert, set_state, update_last_seen, delete
- `SettingsPort`: Load/save global settings
- Redux `devicesSlice`: Manages paired devices state with fetch, update status, update name thunks
- `SettingContext`: Provides global settings with update functions

### Established Patterns
- Hexagonal architecture: ports in uc-core, implementations in uc-infra, commands in uc-tauri
- UseCases accessor pattern: `runtime.usecases().xxx()` for Tauri commands
- Diesel ORM for database operations with schema.rs and mapper patterns
- Redux Toolkit thunks for async frontend state management
- JSON serialization via serde for Rust ↔ frontend data exchange

### Integration Points
- `paired_device` table in SQLite: Add sync_settings JSON column via Diesel migration
- `PairedDevice` domain model: Extend with optional sync settings field
- `DieselPairedDeviceRepository`: Update mapper to handle new JSON column
- `pairing.rs` commands: Add get/update device sync settings commands
- `DeviceSettingsPanel.tsx`: Wire to real Tauri commands instead of hardcoded data
- Sync engine: Check per-device settings before sending clipboard data to each peer

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 24-implement-per-device-sync-settings-for-paired-devices*
*Context gathered: 2026-03-11*
