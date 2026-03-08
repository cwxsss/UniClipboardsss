---
status: resolved
trigger: 'text-entry-wrong-size: Text entries show absurdly large sizes (e.g., 15.62 MB for 2-char text)'
created: 2026-03-08T00:00:00Z
updated: 2026-03-08T00:02:00Z
---

## Current Focus

hypothesis: CONFIRMED - size_bytes in EntryProjectionDto uses entry.total_size (sum of ALL representations) instead of the preview representation's size
test: Fix applied, cargo check passes, all 10 list_entry_projections tests pass
expecting: User verification that text entries now show correct sizes
next_action: Await user verification

## Symptoms

expected: Text entries with 2 characters should show a size of a few bytes (e.g., "2 B" or similar)
actual: Some text entries show sizes like "15.62 MB" despite having only 2 characters and 1 word
errors: No error messages - just wrong data displayed
reproduction: Intermittent - happens with some text entries but not all (depends on whether source app puts rich representations like HTML/RTF on clipboard)
started: Unknown

## Eliminated

## Evidence

- timestamp: 2026-03-08T00:00:30Z
  checked: ClipboardPreview.tsx line 212-216
  found: textItem.size is used for display via formatFileSize(textItem.size)
  implication: Need to trace where ClipboardTextItem.size comes from

- timestamp: 2026-03-08T00:00:40Z
  checked: clipboardItems.ts line 146
  found: ClipboardTextItem.size = entry.size_bytes (from ClipboardEntryProjection)
  implication: size_bytes comes from backend projection

- timestamp: 2026-03-08T00:00:50Z
  checked: list_entry_projections.rs line 202 and 361
  found: EntryProjectionDto.size_bytes = entry.total_size
  implication: total_size is the sum of ALL representations, not just the text representation

- timestamp: 2026-03-08T00:01:00Z
  checked: capture_clipboard.rs line 218 and system.rs line 115-117
  found: total_size = snapshot.total_size_bytes() = sum of all representations' bytes.len()
  implication: When copying 2 chars from a rich app (browser, Word), there can be text/plain (2 bytes) + text/html (megabytes) + text/rtf (megabytes). The total_size sums all of them, causing the absurd display.

- timestamp: 2026-03-08T00:01:30Z
  checked: get_entry_detail.rs line 91
  found: Same bug - EntryDetailResult.size_bytes also uses entry.total_size instead of preview_rep.size_bytes
  implication: Detail view also shows wrong size

- timestamp: 2026-03-08T00:02:00Z
  checked: cargo check and cargo test
  found: Fix compiles cleanly, all 10 list_entry_projections tests pass
  implication: Fix is safe and backward-compatible

## Resolution

root_cause: EntryProjectionDto.size_bytes uses entry.total_size which sums ALL clipboard representations (text/plain + text/html + text/rtf + etc.) instead of just the preview representation's size. When copying from rich text sources (browsers, office apps), HTML/RTF representations can be megabytes while the plain text content is only a few bytes.
fix: Changed both list_entry_projections.rs (execute and execute_single) and get_entry_detail.rs to use representation.size_bytes (the preview representation's size) instead of entry.total_size.
verification: cargo check passes, 10/10 list_entry_projections tests pass
files_changed:

- src-tauri/crates/uc-app/src/usecases/clipboard/list_entry_projections/list_entry_projections.rs
- src-tauri/crates/uc-app/src/usecases/clipboard/get_entry_detail.rs
