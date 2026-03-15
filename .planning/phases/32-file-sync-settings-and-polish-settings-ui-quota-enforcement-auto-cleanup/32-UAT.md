---
status: complete
phase: 32-file-sync-settings-and-polish-settings-ui-quota-enforcement-auto-cleanup
source: 32-01-SUMMARY.md, 32-02-SUMMARY.md, 32-03-SUMMARY.md
started: 2026-03-14T14:20:00Z
updated: 2026-03-15T00:00:00Z
---

## Current Test

[testing complete]

## Tests

### 1. Cold Start Smoke Test

expected: Kill any running UniClipboard instance. Start the application from scratch with `bun tauri dev`. App boots without errors in terminal, main window appears, and the Settings page is accessible.
result: pass

### 2. File Sync Settings Group Visible

expected: Open Settings > Sync section. A "File Sync" group should be visible with 6 controls: enable toggle, small file threshold (MB), max file size (MB), cache quota per device (MB), retention hours, and auto-cleanup toggle.
result: pass

### 3. Controls Disabled When File Sync Off

expected: In the File Sync settings group, turn OFF the enable toggle. All 5 other controls (threshold, max size, quota, retention, auto-cleanup) should become visually disabled and non-interactive.
result: pass

### 4. Controls Enabled When File Sync On

expected: Turn ON the file sync enable toggle. All 5 other controls become interactive again - you can click, type, and change values.
result: pass

### 5. Input Validation - Threshold vs Max Size

expected: Set the small file threshold to a value LARGER than max file size (e.g., threshold=100, max size=50). An inline validation error should appear indicating the threshold must be less than the max file size.
result: pass

### 6. Settings Persistence

expected: Change a file sync setting (e.g., set max file size to 200 MB). Navigate away from Settings to another page, then navigate back to Settings > Sync. The changed value should still show 200 MB.
result: pass

## Summary

total: 6
passed: 6
issues: 0
pending: 0
skipped: 0

## Gaps

[none yet]
