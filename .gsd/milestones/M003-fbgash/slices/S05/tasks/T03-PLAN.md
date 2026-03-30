---
estimated_steps: 7
estimated_files: 2
skills_used: []
---

# T03: Session token lifecycle tests

Test session token lifecycle:

- Initial loadDaemonAuth() → session token obtained, stored in memory
- Session expiry (mock time or wait 5min in test): next request auto-refreshes
- Refresh failure (daemon down during refresh): error propagated correctly
- PID verification: request from unknown PID → 403 or appropriate rejection
- Bearer token never appears in console.log or network URL (grep tests)

Session token must not be stored in localStorage, sessionStorage, or cookies — only in-memory JS variable.

## Inputs

- `src/lib/daemon-auth.ts`
- `src/api/daemon/client.ts`

## Expected Output

- `src/__tests__/lib/daemon-auth.test.ts`
- `src/__tests__/lib/daemon-client.test.ts`

## Verification

All session lifecycle tests pass. Grep verifies tokens not persisted to storage.
