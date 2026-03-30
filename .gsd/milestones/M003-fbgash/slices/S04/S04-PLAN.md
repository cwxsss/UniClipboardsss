# S04: uc-tauri Command Cleanup

**Goal:** Remove all Tauri commands migrated to daemon HTTP API. Retain only Tauri-native commands (daemon lifecycle, tray, updater, autostart, protocol handler).
**Demo:** After this: uc-tauri/src/commands/clipboard.rs, encryption.rs, settings.rs, storage.rs deleted; invoke_handler![] cleaned up

## Tasks
- [x] **T01: Removed all 11 clipboard Tauri commands; deleted clipboard.rs, clipboard DTOs from models/mod.rs, and clipboard test file** — In `src-tauri/crates/uc-tauri/src/commands/clipboard.rs`:

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
  - Estimate: medium
  - Files: src-tauri/crates/uc-tauri/src/commands/clipboard.rs, src-tauri/src/main.rs
  - Verify: cargo build in src-tauri/ succeeds. Frontend builds and clipboard operations work via daemon HTTP.
- [x] **T02: Removed six migrated Tauri commands (get_encryption_session_status, unlock_encryption_session, get_settings, update_settings, get_storage_stats, clear_cache); deleted settings.rs** — In `src-tauri/crates/uc-tauri/src/commands/encryption.rs`:

Remove: get_encryption_state, unlock_encryption_session, lock_encryption_session.

In `settings.rs`:
Remove: get_settings, update_settings.

In `storage.rs`:
Remove: get_storage_stats, clear_storage_cache, get_dir_size.

Remove respective invoke_handler![] registrations. Clean up imports.

Note: Phase 72 already moved restore_clipboard_entry proxying to daemon, so verify it's not in clipboard.rs before deletion.
  - Estimate: medium
  - Files: src-tauri/crates/uc-tauri/src/commands/encryption.rs, src-tauri/crates/uc-tauri/src/commands/settings.rs, src-tauri/crates/uc-tauri/src/commands/storage.rs, src-tauri/src/main.rs
  - Verify: cargo build in src-tauri/ succeeds. Settings, encryption, storage operations work via daemon HTTP.
- [ ] **T03: Code audit and build verification** — Final verification before completing:

```bash
# Frontend: no invoke() calls to removed commands
rg 'invoke.*get_clipboard' src/
rg 'invoke.*delete_clipboard' src/
rg 'invoke.*restore_clipboard' src/
rg 'invoke.*toggle_favorite' src/
rg 'invoke.*get_clipboard_stats' src/
rg 'invoke.*get_settings' src/
rg 'invoke.*update_settings' src/
rg 'invoke.*get_encryption_state' src/
rg 'invoke.*unlock_encryption' src/
rg 'invoke.*lock_encryption' src/
rg 'invoke.*get_storage_stats' src/
rg 'invoke.*clear_storage' src/

# Backend: cargo build succeeds
cd src-tauri && cargo build
```

All grep should return zero matches. Cargo build should succeed. Manual smoke test of clipboard, settings, encryption in running app.
  - Estimate: small
  - Verify: All grep returns zero matches. cargo build succeeds. Manual smoke test passes.
