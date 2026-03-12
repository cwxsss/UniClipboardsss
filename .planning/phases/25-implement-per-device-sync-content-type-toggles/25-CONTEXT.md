# Phase 25: Implement Per-Device Sync Content Type Toggles - Context

**Gathered:** 2026-03-12
**Status:** Ready for planning

<domain>
## Phase Boundary

Make the per-device content type toggles functional — both in the UI (currently disabled with "not editable" badge) and in the sync engine (currently only checks auto_sync, no content type filtering). This phase implements end-to-end content type filtering for text and image types, with other types marked as "coming soon".

</domain>

<decisions>
## Implementation Decisions

### Content type matching logic

- Use **primary type** matching based on MIME type mapping
- Each clipboard entry's main MIME type determines its content type category
- MIME mapping rules:
  - `text/plain` → text
  - `image/*` → image
  - `text/html` → rich_text
  - `text/uri-list` → link
  - `application/octet-stream` → file
  - Unknown/unmapped → default to sync (safety fallback)
- Only text and image types are filterable in this phase; unimplemented types (rich_text, link, file, code_snippet) always sync regardless of toggle state

### Filtering direction and timing

- Content type filtering applies **outbound only**, consistent with existing auto_sync behavior
- Merge content type check into the existing `filter_by_auto_sync` method, but **rename the method** to something more semantically meaningful (e.g., `apply_sync_policy` or `filter_by_sync_settings`)
- Single pass: check auto_sync first, then content type match — avoids extra DB queries

### Edge cases and auto_sync interaction

- **All content types disabled**: Allowed — equivalent to disabling sync for that device. Show inline warning text below the toggles (e.g., "All content types disabled, no content will sync to this device")
- **auto_sync off**: Content type toggles become visually disabled (grayed out) but preserve their on/off state. Re-enabling auto_sync restores previous content type configuration
- **Unimplemented types**: Always sync — only text and image are actually filtered by the engine

### UI interaction

- **Editable toggles**: text and image toggles become fully interactive (remove "not editable" badge, enable click)
- **Coming soon toggles**: file, link, code_snippet, rich_text keep toggle UI but marked with "coming soon" badge instead of "not editable". Toggles remain disabled/non-interactive
- **Save mechanism**: Immediate save on each toggle change, reusing existing `updateDeviceSyncSettings` thunk — consistent with auto_sync toggle behavior
- **Restore defaults**: Keeps current behavior — sets entire per-device settings to null, falling back to global defaults for all settings including content types

### Claude's Discretion

- Exact inline warning text and styling for all-disabled state
- How to determine primary MIME type from clipboard snapshot (which representation to check first)
- Exact method rename for filter_by_auto_sync
- Toggle animation/transition details
- "Coming soon" badge styling

</decisions>

<specifics>
## Specific Ideas

No specific requirements — open to standard approaches

</specifics>

<code_context>

## Existing Code Insights

### Reusable Assets

- `DeviceSettingsPanel.tsx`: Already has all 6 content type toggles rendered in a loop via `contentTypeEntries` array — needs toggle enable/disable logic per type
- `filter_by_auto_sync()` in `sync_outbound.rs`: Existing per-peer filtering logic that loads device settings and checks auto_sync — extend with content type check
- `resolve_sync_settings()` in `paired_device.rs`: Already resolves per-device vs global settings — content_types field is already part of SyncSettings
- `ContentTypes` struct in `model.rs`: Boolean fields for all 6 types already exist
- `updateDeviceSyncSettings` Redux thunk: Already handles immediate save of per-device settings

### Established Patterns

- Per-device settings stored as JSON column in paired_device table with null = use global defaults
- `resolve_sync_settings()` resolves device override vs global fallback
- Redux thunks for async state management with immediate API calls on toggle change
- Content type entries defined as array for loop rendering in DeviceSettingsPanel

### Integration Points

- `SyncOutboundClipboardUseCase::filter_by_auto_sync()` — rename and extend with content type filtering
- `DeviceSettingsPanel.tsx` — make text/image toggles interactive, add "coming soon" badge for others
- `contentTypeEntries` array — add metadata to distinguish implemented vs coming-soon types
- Clipboard snapshot/message — need to extract primary MIME type for content type determination

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

_Phase: 25-implement-per-device-sync-content-type-toggles_
_Context gathered: 2026-03-12_
