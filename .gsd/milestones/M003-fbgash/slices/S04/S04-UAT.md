# S04: uc-tauri Command Cleanup — UAT

**Milestone:** M003-fbgash
**Written:** 2026-03-30T06:21:57.884Z

## Smoke Test

```bash
# 1. Backend compiles clean
cd src-tauri && cargo build 2>&1 | grep "^error" | head -5
# Expected: no errors

# 2. Frontend type-checks
npx tsc --noEmit 2>&1 | grep -v "PairingDialog\|useClipboardEvents"
# Expected: no errors in non-test files

# 3. No invoke() calls to removed commands in live code
rg 'invoke.*get_clipboard|invoke.*delete_clipboard|invoke.*restore_clipboard|invoke.*toggle_favorite|invoke.*get_settings|invoke.*update_settings|invoke.*get_encryption_state' src/ --no-heading | grep -v __tests__ | grep -v '.test.' | grep -v "src/api/clipboardItems\|src/api/security\|src/api/storage"
# Expected: zero matches
```

## Test Cases

### 1. Cargo build succeeds
- Run `cargo build` in `src-tauri/`
- **Expected:** Exit code 0

### 2. TypeScript compiles
- Run `npx tsc --noEmit`
- **Expected:** Zero errors in src/ (excluding pre-existing test failures)

### 3. Clipboard operations via daemon API
- Open app, navigate to clipboard history
- Select an item and trigger paste (quick panel)
- **Expected:** Item restored via `POST /clipboard/restore/:id`

### 4. Settings load/save via daemon API
- Open Settings page, toggle auto-start or change theme
- **Expected:** Setting persisted via `PUT /settings`

### 5. Encryption unlock via Tauri command
- Start app with encryption initialized (keychain has credentials)
- Click Unlock on UnlockPage
- **Expected:** `unlock_encryption_session` Tauri command succeeds

### 6. Storage stats display
- Open Settings → Storage section
- **Expected:** `get_storage_stats` returns breakdown (database, vault, cache, logs)

## Edge Cases

### A. PreviewPanel broken (known issue — S05 must fix)
- **Trigger:** Open any clipboard entry preview
- **Expected currently:** Crash — `get_clipboard_entry_detail` not found
- **Fix:** S05 must add daemon endpoint or restore Tauri command
