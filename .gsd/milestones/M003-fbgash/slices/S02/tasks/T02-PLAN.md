---
estimated_steps: 9
estimated_files: 1
skills_used: []
---

# T02: Redux clipboard thunks migration

Identify all clipboard-related Redux thunks in src/store/ that use Tauri invoke(). Replace each invoke() call with the corresponding daemon API function from src/api/daemon/clipboard.ts.

Priority order:
1. clipboardSlice entries list thunk
2. clipboardSlice delete thunk
3. clipboardSlice restore thunk
4. clipboardSlice favorite toggle thunk
5. clipboardSlice stats thunk
6. Any RTK Query endpoints for clipboard

Keep the old Tauri invoke() calls as commented-out references for rollback during transition period.

## Inputs

- `src/api/clipboard.ts (current implementation)`
- `src/api/daemon/clipboard.ts (new implementation)`

## Expected Output

- `Updated src/store/ clipboard slices`

## Verification

TypeScript compiles. Browser test: all clipboard operations work via daemon HTTP. Redux DevTools shows correct state transitions.
