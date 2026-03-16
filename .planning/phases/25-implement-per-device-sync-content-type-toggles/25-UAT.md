---
status: complete
phase: 25-implement-per-device-sync-content-type-toggles
source: 25-01-SUMMARY.md, 25-02-SUMMARY.md
started: 2026-03-12T05:00:00Z
updated: 2026-03-12T05:10:00Z
---

## Current Test

[testing complete]

## Tests

### 1. Text/Image Toggles Are Interactive

expected: Open Device Settings panel for a paired device. Under content type toggles, clicking the Text or Image toggle switches it on/off and the change persists (dispatches to backend).
result: pass

### 2. Coming Soon Badge on Unimplemented Types

expected: In Device Settings, the File, Link, Code Snippet, and Rich Text content type toggles display a "Coming Soon" badge and are non-interactive (cannot be toggled).
result: pass

### 3. All Content Types Disabled Warning

expected: With auto_sync ON for a device, disable both Text and Image toggles. An amber warning message appears indicating all content types are disabled.
result: skipped
reason: Feature was removed, no longer needed

### 4. Auto-Sync Off Grays Out Content Type Toggles

expected: Turn auto_sync OFF for a device. All content type toggles become visually grayed out / disabled, but their on/off values are preserved (not reset).
result: pass

### 5. New Device Defaults All Content Types Enabled

expected: When a new device is added/paired, all content type toggles (Text, Image) default to ON (enabled), not OFF.
result: pass

## Summary

total: 5
passed: 4
issues: 0
pending: 0
skipped: 1

## Gaps

[none yet]
