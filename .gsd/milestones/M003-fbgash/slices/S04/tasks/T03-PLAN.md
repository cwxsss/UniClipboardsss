---
estimated_steps: 19
estimated_files: 1
skills_used: []
---

# T03: Code audit and build verification

Final verification before completing:

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

## Inputs

- None specified.

## Expected Output

- `Verification report with grep results and build output`

## Verification

All grep returns zero matches. cargo build succeeds. Manual smoke test passes.
