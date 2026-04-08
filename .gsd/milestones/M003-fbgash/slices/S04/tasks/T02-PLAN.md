---
estimated_steps: 8
estimated_files: 4
skills_used: []
---

# T02: Remove encryption/settings/storage Tauri commands

In `src-tauri/crates/uc-tauri/src/commands/encryption.rs`:

Remove: get_encryption_state, unlock_encryption_session, lock_encryption_session.

In `settings.rs`:
Remove: get_settings, update_settings.

In `storage.rs`:
Remove: get_storage_stats, clear_storage_cache, get_dir_size.

Remove respective invoke_handler![] registrations. Clean up imports.

Note: Phase 72 already moved restore_clipboard_entry proxying to daemon, so verify it's not in clipboard.rs before deletion.

## Inputs

- `src-tauri/crates/uc-tauri/src/commands/encryption.rs`
- `src-tauri/crates/uc-tauri/src/commands/settings.rs`
- `src-tauri/crates/uc-tauri/src/commands/storage.rs`

## Expected Output

- `src-tauri/crates/uc-tauri/src/commands/encryption.rs (modified)`
- `src-tauri/crates/uc-tauri/src/commands/settings.rs (modified)`
- `src-tauri/crates/uc-tauri/src/commands/storage.rs (modified)`

## Verification

cargo build in src-tauri/ succeeds. Settings, encryption, storage operations work via daemon HTTP.
