---
estimated_steps: 10
estimated_files: 1
skills_used: []
---

# T03: Migration verification and grep audit

Before completing this slice, run verification:

```bash
# Verify no invoke() calls for clipboard commands remain
rg 'invoke.*get_clipboard' src/
rg 'invoke.*delete_clipboard' src/
rg 'invoke.*restore_clipboard' src/
rg 'invoke.*toggle_favorite' src/
rg 'invoke.*get_clipboard_stats' src/
```

All should return no matches. Then do browser smoke test: list entries, delete one, restore one, toggle favorite, check stats.

## Inputs

- None specified.

## Expected Output

- `Verification report`

## Verification

All grep commands return zero matches. Browser smoke test passes for all clipboard operations.
