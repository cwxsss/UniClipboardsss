---
estimated_steps: 11
estimated_files: 2
skills_used: []
---

# T01: Remove clipboard Tauri commands

In `src-tauri/crates/uc-tauri/src/commands/clipboard.rs`:

Remove these functions (or delete entire file if empty after):
- get_clipboard_entries
- get_clipboard_entry
- delete_clipboard_entry
- restore_clipboard_entry (was proxying to daemon already)
- toggle_favorite_clipboard_item
- get_clipboard_stats
- get_clipboard_entry_resource

Remove from `src-tauri/src/main.rs` or wherever invoke_handler![] macro registers clipboard commands.

Remove use statements and imports that become unused.

## Inputs

- `src-tauri/crates/uc-tauri/src/commands/clipboard.rs (full file)`
- `src-tauri/src/main.rs (invoke_handler registrations)`

## Expected Output

- `src-tauri/crates/uc-tauri/src/commands/clipboard.rs (modified/removed)`

## Verification

cargo build in src-tauri/ succeeds. Frontend builds and clipboard operations work via daemon HTTP.
