# Phase 25: Implement Per-Device Sync Content Type Toggles - Research

**Researched:** 2026-03-12
**Domain:** Outbound sync filtering + React toggle UI
**Confidence:** HIGH

## Summary

This phase makes per-device content type toggles functional end-to-end. The infrastructure is already in place from Phase 24: `ContentTypes` struct exists in Rust and TypeScript, `SyncSettings` includes `content_types`, `resolve_sync_settings()` handles per-device vs global fallback, and the frontend has toggle UI (currently disabled). The work is extending `filter_by_auto_sync` to also check content types, adding MIME-to-content-type mapping logic, and enabling the UI toggles for text/image while marking others as "coming soon".

The sync outbound flow currently: list sendable peers -> filter by auto_sync -> encrypt + send. Content type filtering fits naturally into the existing `filter_by_auto_sync` method, which already loads `SyncSettings` per peer. The key new piece is determining a snapshot's "primary content type" from its MIME types, then checking against the peer's `ContentTypes` toggles.

**Primary recommendation:** Extend `filter_by_auto_sync` (renamed to `apply_sync_policy`) to accept the snapshot, determine its primary content type from MIME data, and filter peers whose content type toggle is disabled for that type.

<user_constraints>

## User Constraints (from CONTEXT.md)

### Locked Decisions

- Use **primary type** matching based on MIME type mapping
- MIME mapping rules: `text/plain` -> text, `image/*` -> image, `text/html` -> rich_text, `text/uri-list` -> link, `application/octet-stream` -> file, unknown -> default to sync
- Only text and image types are filterable; unimplemented types always sync
- Content type filtering applies **outbound only**
- Merge content type check into existing `filter_by_auto_sync` method, **rename** to semantically meaningful name (e.g., `apply_sync_policy`)
- Single pass: check auto_sync first, then content type match
- **All content types disabled**: Allowed, show inline warning text below toggles
- **auto_sync off**: Content type toggles become visually disabled (grayed out) but preserve state
- **Unimplemented types**: Always sync
- **Editable toggles**: text and image become fully interactive (remove "not editable" badge)
- **Coming soon toggles**: file, link, code_snippet, rich_text keep toggle UI with "coming soon" badge, remain disabled
- **Save mechanism**: Immediate save on each toggle change via existing `updateDeviceSyncSettings` thunk
- **Restore defaults**: Keep current behavior (sets per-device settings to null)

### Claude's Discretion

- Exact inline warning text and styling for all-disabled state
- How to determine primary MIME type from clipboard snapshot (which representation to check first)
- Exact method rename for filter_by_auto_sync
- Toggle animation/transition details
- "Coming soon" badge styling

### Deferred Ideas (OUT OF SCOPE)

None
</user_constraints>

## Architecture Patterns

### Backend: MIME-to-ContentType Mapping

A new pure function in `uc-core` maps a snapshot's representations to a content type category:

```rust
// Location: uc-core/src/settings/content_type_filter.rs (new file)
// or uc-core/src/clipboard/content_type.rs (new file)

/// Content type category for sync filtering
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentTypeCategory {
    Text,
    Image,
    RichText,
    Link,
    File,
    CodeSnippet,
    Unknown, // always syncs
}

/// Determine the primary content type from a snapshot's representations.
/// Checks representations in order, returns first recognized category.
pub fn classify_snapshot(snapshot: &SystemClipboardSnapshot) -> ContentTypeCategory {
    for rep in &snapshot.representations {
        if let Some(ref mime) = rep.mime {
            let m = mime.as_str();
            if m == "text/plain" { return ContentTypeCategory::Text; }
            if m.starts_with("image/") { return ContentTypeCategory::Image; }
            if m == "text/html" { return ContentTypeCategory::RichText; }
            if m == "text/uri-list" { return ContentTypeCategory::Link; }
            if m == "application/octet-stream" { return ContentTypeCategory::File; }
        }
    }
    ContentTypeCategory::Unknown
}

/// Check if a content type category is allowed by the given ContentTypes settings.
/// Unknown and unimplemented types (rich_text, link, file, code_snippet) always return true.
pub fn is_content_type_allowed(category: ContentTypeCategory, ct: &ContentTypes) -> bool {
    match category {
        ContentTypeCategory::Text => ct.text,
        ContentTypeCategory::Image => ct.image,
        // Unimplemented types always sync regardless of toggle
        ContentTypeCategory::RichText
        | ContentTypeCategory::Link
        | ContentTypeCategory::File
        | ContentTypeCategory::CodeSnippet
        | ContentTypeCategory::Unknown => true,
    }
}
```

**Key design choice for primary MIME detection order**: Check representations in array order (first match wins). The platform clipboard watcher typically puts the "primary" representation first. This is consistent with how `SelectRepresentationPolicy` already works.

### Backend: Renamed Filter Method

The current `filter_by_auto_sync` method signature:

```rust
async fn filter_by_auto_sync(
    &self,
    peers: &[DiscoveredPeer],
) -> Vec<DiscoveredPeer>
```

Becomes:

```rust
async fn apply_sync_policy(
    &self,
    peers: &[DiscoveredPeer],
    snapshot: &SystemClipboardSnapshot,
) -> Vec<DiscoveredPeer>
```

The snapshot parameter is needed to classify content type. The method:

1. Loads global settings (once, as currently done)
2. Classifies the snapshot's content type (once)
3. For each peer: checks auto_sync, then checks content type toggle

### Backend: Integration Point in execute_async

In `sync_outbound.rs`, the call site changes from:

```rust
let sendable_peers = self.filter_by_auto_sync(&all_sendable_peers).await;
```

To:

```rust
let sendable_peers = self.apply_sync_policy(&all_sendable_peers, &snapshot).await;
```

This must happen **before** `snapshot.representations.into_iter()` consumes the snapshot (line ~193). Current code accesses `snapshot.snapshot_hash()` and `snapshot.ts_ms` before consumption, so passing `&snapshot` to the policy filter fits naturally in the existing flow.

### Frontend: Toggle State Categories

The `contentTypeEntries` array needs metadata to distinguish editable vs coming-soon:

```typescript
const contentTypeEntries: {
  field: keyof ContentTypes
  i18nKey: string
  status: 'editable' | 'coming_soon'
}[] = [
  { field: 'text', i18nKey: 'syncText', status: 'editable' },
  { field: 'image', i18nKey: 'syncImage', status: 'editable' },
  { field: 'file', i18nKey: 'syncFile', status: 'coming_soon' },
  { field: 'link', i18nKey: 'syncLink', status: 'coming_soon' },
  { field: 'code_snippet', i18nKey: 'syncCodeSnippet', status: 'coming_soon' },
  { field: 'rich_text', i18nKey: 'syncRichText', status: 'coming_soon' },
]
```

### Frontend: Toggle Handler

Content type toggle changes follow the same pattern as `handleAutoSyncToggle`:

```typescript
const handleContentTypeToggle = useCallback(
  (field: keyof ContentTypes) => {
    if (!settings) return
    dispatch(
      updateDeviceSyncSettings({
        peerId: deviceId,
        settings: {
          ...settings,
          content_types: {
            ...settings.content_types,
            [field]: !settings.content_types[field],
          },
        },
      })
    )
  },
  [dispatch, deviceId, settings]
)
```

### Frontend: Visual States

| State                | auto_sync | Toggle behavior                                              |
| -------------------- | --------- | ------------------------------------------------------------ |
| Normal (editable)    | ON        | Interactive, checked/unchecked based on content_types[field] |
| auto_sync off        | OFF       | Visually grayed out, non-interactive, preserves value        |
| Coming soon          | any       | Disabled with "Coming Soon" badge, non-interactive           |
| All content disabled | ON        | Show inline warning below toggles                            |

### Anti-Patterns to Avoid

- **Consuming snapshot before policy check**: The `snapshot.representations.into_iter()` call consumes the snapshot. The policy check MUST happen before this line.
- **Separate DB queries per content type**: Only one `resolve_sync_settings` call per peer is needed; the ContentTypes struct has all fields.
- **Filtering on both inbound and outbound**: Decision is outbound-only. Do NOT add content type filtering to inbound sync.

## Don't Hand-Roll

| Problem                        | Don't Build                  | Use Instead                                      | Why                                                |
| ------------------------------ | ---------------------------- | ------------------------------------------------ | -------------------------------------------------- |
| Per-device settings resolution | Custom per-peer lookup logic | Existing `resolve_sync_settings()`               | Already handles device override vs global fallback |
| Settings persistence           | Custom save endpoint         | Existing `updateDeviceSyncSettings` thunk        | Already wired to Redux + Tauri command             |
| Toggle UI                      | Custom toggle component      | Existing checkbox pattern in DeviceSettingsPanel | Same visual style as auto_sync toggle              |

## Common Pitfalls

### Pitfall 1: ContentTypes Default is all-false

**What goes wrong:** `ContentTypes` derives `Default`, which sets all bools to `false`. New devices with `sync_settings: None` fall back to global settings, but if global `ContentTypes::default()` is all-false, nothing syncs.
**Why it happens:** Rust `#[derive(Default)]` for a struct of bools sets everything to false.
**How to avoid:** Verify the `SyncSettings::default()` in defaults.rs. Currently it uses `ContentTypes::default()` which is all-false. The global settings file likely overrides this, but verify that the TOML default has content types enabled. If not, the `Default` impl for `ContentTypes` should be changed to all-true, OR the defaults.rs `SyncSettings::default` should construct ContentTypes explicitly with all-true.
**Warning signs:** After implementing, test with a fresh install to verify defaults.

### Pitfall 2: Snapshot consumed before policy check

**What goes wrong:** `snapshot.representations.into_iter()` moves ownership. If the policy check isn't done before this line, the snapshot is no longer available.
**Why it happens:** Rust move semantics.
**How to avoid:** Call `apply_sync_policy(&all_sendable_peers, &snapshot)` before the `into_iter()` line.

### Pitfall 3: Badge i18n key missing

**What goes wrong:** The "Coming Soon" badge key exists at `devices.settings.badges.comingSoon` but code currently uses `devices.settings.badges.notEditable`. Need to switch to the correct key.
**Why it happens:** i18n keys for both badges already exist in en-US.json and zh-CN.json.
**How to avoid:** Use `t('devices.settings.badges.comingSoon')` for coming-soon types.

### Pitfall 4: Stale test expectations

**What goes wrong:** Existing test `DeviceSettingsPanel.test.tsx` checks for "Not Editable" badge text and permissions section (which was removed in 24-03). Tests will fail.
**Why it happens:** Test wasn't updated when permissions section was removed.
**How to avoid:** Update/fix the test to match current component behavior.

## Code Examples

### Determining all-content-types-disabled state (Frontend)

```typescript
const allContentTypesDisabled = settings
  ? Object.values(settings.content_types).every(v => !v)
  : false

// Only show warning when auto_sync is on but all content disabled
const showAllDisabledWarning = settings?.auto_sync && allContentTypesDisabled
```

### ContentTypes default fix (if needed)

```rust
// In uc-core/src/settings/model.rs, replace #[derive(Default)] with:
impl Default for ContentTypes {
    fn default() -> Self {
        Self {
            text: true,
            image: true,
            link: true,
            file: true,
            code_snippet: true,
            rich_text: true,
        }
    }
}
```

**Note:** This is CRITICAL. The current `#[derive(Default)]` produces all-false, meaning new users with no saved settings would have all content types disabled by default. Check whether existing TOML config files set these to true. If not, this default must be changed.

## State of the Art

| Aspect                     | Current State                          | After Phase 25                                                 |
| -------------------------- | -------------------------------------- | -------------------------------------------------------------- |
| Content type toggles       | All disabled with "Not Editable" badge | text/image interactive; others "Coming Soon"                   |
| Outbound filtering         | auto_sync only                         | auto_sync + content type check                                 |
| filter_by_auto_sync method | Checks auto_sync per peer              | Renamed to apply_sync_policy, checks auto_sync + content types |
| MIME classification        | Not implemented                        | New classify_snapshot() function in uc-core                    |

## Validation Architecture

### Test Framework

| Property           | Value                                                                           |
| ------------------ | ------------------------------------------------------------------------------- |
| Framework          | Rust: cargo test; Frontend: Vitest                                              |
| Config file        | src-tauri/Cargo.toml; vitest.config.ts                                          |
| Quick run command  | `cd src-tauri && cargo test -p uc-app --lib usecases::clipboard::sync_outbound` |
| Full suite command | `cd src-tauri && cargo test && cd .. && bun test`                               |

### Phase Requirements -> Test Map

| Req ID | Behavior                                                 | Test Type | Automated Command                                               | File Exists?           |
| ------ | -------------------------------------------------------- | --------- | --------------------------------------------------------------- | ---------------------- |
| P25-01 | classify_snapshot maps MIME to content type              | unit      | `cd src-tauri && cargo test -p uc-core classify_snapshot`       | No - Wave 0            |
| P25-02 | is_content_type_allowed filters correctly                | unit      | `cd src-tauri && cargo test -p uc-core is_content_type_allowed` | No - Wave 0            |
| P25-03 | apply_sync_policy skips peers with disabled content type | unit      | `cd src-tauri && cargo test -p uc-app apply_sync_policy`        | No - Wave 0            |
| P25-04 | apply_sync_policy allows sync for unknown types          | unit      | `cd src-tauri && cargo test -p uc-app sync_outbound`            | No - Wave 0            |
| P25-05 | Frontend toggles for text/image are interactive          | unit      | `bun test DeviceSettingsPanel`                                  | Partial - needs update |
| P25-06 | Frontend shows "Coming Soon" for non-implemented types   | unit      | `bun test DeviceSettingsPanel`                                  | No - Wave 0            |
| P25-07 | Frontend shows warning when all types disabled           | unit      | `bun test DeviceSettingsPanel`                                  | No - Wave 0            |

### Sampling Rate

- **Per task commit:** `cd src-tauri && cargo test -p uc-core -p uc-app --lib`
- **Per wave merge:** `cd src-tauri && cargo test && cd .. && bun test`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps

- [ ] Content type classification unit tests in uc-core
- [ ] Sync policy filter tests with content type scenarios in uc-app
- [ ] Updated DeviceSettingsPanel.test.tsx (current test references removed permissions section)

## Open Questions

1. **ContentTypes default: all-false or all-true?**
   - What we know: `#[derive(Default)]` produces all-false. SyncSettings::default() uses this.
   - What's unclear: Whether saved TOML config overrides this for existing users. New installs may get all-false defaults.
   - Recommendation: Change ContentTypes::default() to all-true. This is safer -- users expect content to sync by default. This is a prerequisite fix before enabling filtering.

2. **Multiple representations with different MIME types**
   - What we know: A snapshot can have multiple representations (e.g., text/plain + text/html for rich text copy).
   - What's unclear: Which representation determines the primary type when there are mixed types.
   - Recommendation: Use first-match in representation order. Platform clipboard puts primary format first. If first is text/html and second is text/plain, classify as rich_text (which always syncs for now anyway).

## Sources

### Primary (HIGH confidence)

- Direct code inspection of all referenced files (sync_outbound.rs, model.rs, paired_device.rs, DeviceSettingsPanel.tsx, devicesSlice.ts, p2p.ts, mime.rs, defaults.rs, system.rs)
- Existing test files (sync_outbound tests, DeviceSettingsPanel.test.tsx)
- i18n locale files (en-US.json, zh-CN.json)

### Secondary (MEDIUM confidence)

- Phase 24 decisions from STATE.md regarding architecture patterns

## Metadata

**Confidence breakdown:**

- Standard stack: HIGH - All code exists and was directly inspected
- Architecture: HIGH - Clear extension points identified in existing code
- Pitfalls: HIGH - ContentTypes default issue is verifiable from code

**Research date:** 2026-03-12
**Valid until:** 2026-04-12 (stable codebase, no external dependencies)
