# Phase 31: File Sync Settings and Polish — Research

**Researched:** 2026-03-13
**Status:** RESEARCH COMPLETE

## 1. Settings UI Architecture

### Existing Pattern Analysis

The Settings page uses a sidebar-category pattern defined in `settings-config.ts`:
- `SETTINGS_CATEGORIES` array maps category IDs to `LucideIcon` + `Component`
- Each section is a standalone React component (e.g., `SyncSection.tsx`)
- Components use `SettingGroup` (titled card with border) and `SettingRow` (label + description + control)
- State: `useSetting()` hook from `SettingContext` provides `setting`, `updateSyncSetting()`, etc.
- Local state mirrors settings to avoid UI flash, synced via `useEffect`

### SyncSection Current State

`SyncSection.tsx` already contains:
- Auto sync toggle (`Switch`)
- Sync frequency select (`Select`)
- Max file size input (`Input` with validation)

File sync settings should be added as a **new SettingGroup within SyncSection** (not a separate category) since they are logically part of sync configuration. This keeps the sidebar clean and groups related settings together.

### TypeScript Settings Interface

`src/types/setting.ts` defines `SyncSettings`:
```typescript
interface SyncSettings {
  auto_sync: boolean
  sync_frequency: SyncFrequency
  content_types: ContentTypes
  max_file_size_mb: number
}
```

Phase 28 will extend `SyncSettings` with file sync fields. Phase 31 wires these to UI.

### Rust Settings Model

`uc-core/src/settings/model.rs` defines `SyncSettings`:
```rust
pub struct SyncSettings {
    pub auto_sync: bool,
    pub sync_frequency: SyncFrequency,
    pub content_types: ContentTypes,
    pub max_file_size_mb: u32,
}
```

Phase 28 extends this with: `file_sync_enabled`, `small_file_threshold`, `max_file_size`, `file_cache_quota_per_device`, `file_retention_hours`, `file_auto_cleanup`.

### Size Input Component Design

For size inputs (threshold, max size, quota), options:
1. **Simple numeric input with fixed unit suffix** (current pattern: `Input` + "MB" text)
2. **Numeric input with unit dropdown** (KB/MB/GB selector)

Recommendation: Use pattern #1 (consistent with existing `max_file_size_mb` input) but with MB as the unit for threshold/quota and GB for max file size. Each input shows the unit as a suffix label.

## 2. Quota Enforcement Architecture

### Where Quota Check Happens

Per Phase 29 context, `SyncInboundFileUseCase` handles incoming files:
- Disk space pre-check before accepting transfer
- Per-device file-cache quota enforcement (default 500MB)

Phase 31 wires the settings value to this enforcement point:
1. Read `file_cache_quota_per_device` from settings
2. Calculate current cache usage for the source device
3. Compare against quota; reject if exceeded
4. Emit notification to both sender and receiver on rejection

### Cache Usage Calculation

Need a function/query to sum file sizes in `file-cache/` per source device:
- Query database for file entries by device_id where status != Expired
- Sum `file_size` column
- Compare against quota setting

### Rejection Flow

When quota exceeded:
1. Receiver sends `Error` message with quota-exceeded reason
2. Receiver emits `NetworkEvent::FileTransferFailed` with quota reason
3. Sender receives error, emits notification "Transfer rejected: quota exceeded on [device]"
4. Both devices get system notification

## 3. Auto-Cleanup Architecture

### Cleanup Trigger Points

Per context: "Auto-cleanup of expired files on app startup". Options:
1. **App startup only** — simplest, runs once per launch
2. **Periodic timer** — more thorough, catches long-running sessions
3. **Both** — startup + periodic (e.g., every hour)

Recommendation: Startup + optional periodic (if app runs for extended periods). Startup cleanup is mandatory; periodic is Claude's discretion.

### Cleanup Implementation

1. Query database for file entries where:
   - Status is `Completed` AND created_at older than `file_retention_hours`
   - OR Status is `Failed` (cleanup immediately)
2. For each expired entry:
   - Delete file from `file-cache/` directory
   - Update database status to `Expired`
3. Log cleanup results (count deleted, space reclaimed)

### Background Task Registration

`start_background_tasks()` in `wiring.rs` is where background tasks are spawned via `TaskRegistry`. The cleanup task should be registered here:
- Use `tokio::spawn` with the task registered in TaskRegistry
- Non-blocking: does not delay app startup
- Graceful shutdown: respects cancellation token

### File Retention Period

Default 24 hours. Configurable via `file_retention_hours` in settings.

## 4. Error Handling Polish

### Current Error Surface Area

Based on the pipeline design (Phases 28-30), error paths to review:
1. **Transfer initiation**: file not found, file too large, file_sync_enabled=false
2. **Transfer in progress**: network disconnect, hash mismatch, disk full
3. **Quota**: exceeded on receiver side
4. **Cleanup**: file already deleted, permission error on temp file

### Error Message Strategy

All errors should produce:
1. **System notification** — brief user-facing message
2. **Dashboard status** — error detail on file entry (if applicable)
3. **Log event** — full error with context for debugging

### file_sync_enabled Gate

When `file_sync_enabled` is false:
- Outbound: skip file sync entirely (no announce sent)
- Inbound: reject incoming file transfers with notification to sender
- UI: file sync settings section shows controls as cascade-disabled

## 5. Validation Architecture

### Settings Validation

File sync settings inputs need validation:
- `small_file_threshold`: must be > 0 and < `max_file_size`
- `max_file_size`: must be > 0 and <= reasonable limit (e.g., 10GB)
- `file_cache_quota_per_device`: must be > 0
- `file_retention_hours`: must be >= 1

### Cross-field Validation

- `small_file_threshold` < `max_file_size` (threshold must be smaller than limit)
- UI should show warning if quota is set very low (e.g., < 100MB)

## 6. Integration with Previous Phases

### Phase 28 Outputs Consumed
- Settings model fields (`file_sync_enabled`, `small_file_threshold`, etc.)
- Existing `SettingsPort` for reading/writing settings

### Phase 29 Outputs Consumed
- `SyncInboundFileUseCase`: add quota enforcement call
- File entry database records: used for quota calculation and cleanup queries
- `file-cache/` directory: cleanup target

### Phase 30 Outputs Consumed
- Dashboard file entries: settings changes affect display behavior
- Settings page already accessible with sync section

## 7. Key Implementation Files

### Backend (Rust)
- `uc-core/src/settings/model.rs` — already extended by Phase 28
- `uc-core/src/settings/defaults.rs` — default values already set by Phase 28
- New: `uc-app/src/usecases/file/cleanup.rs` — auto-cleanup use case
- New: quota enforcement logic in inbound file use case
- `uc-tauri/src/bootstrap/wiring.rs` — register cleanup background task

### Frontend (TypeScript/React)
- `src/types/setting.ts` — extend `SyncSettings` interface (already done by Phase 28)
- `src/components/setting/SyncSection.tsx` — add file sync settings group
- i18n keys: `settings.sections.sync.fileSync.*`

---

*Research completed: 2026-03-13*
