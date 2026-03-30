---
estimated_steps: 16
estimated_files: 4
skills_used: []
---

# T01: HTTP API integration tests

Create integration test suite for all daemon HTTP endpoints. Use Vitest + msw (mock service worker) or real HTTP client against running daemon.

Test coverage:
- GET /clipboard/entries — correct pagination, entry shapes
- GET /clipboard/entries/:id — not found case, correct shape
- DELETE /clipboard/entries/:id — 404, success cases
- POST /clipboard/entries/:id/restore — 404, success, already-restored cases
- POST /clipboard/entries/:id/favorite — correct toggle
- GET /clipboard/stats — correct shape and values
- GET /settings — correct shape
- PUT /settings — validation errors, success
- GET /encryption/state — correct state shapes
- POST /encryption/unlock — wrong passphrase (401), success
- POST /encryption/lock — success
- GET /storage/stats — correct shape
- POST /storage/clear-cache — missing confirmed (400), confirmed:true (success)

Error response shapes: DaemonApiError fields populated correctly.

## Inputs

- `src/api/daemon/clipboard.ts`
- `src/api/daemon/settings.ts`
- `src/api/daemon/encryption.ts`
- `src/api/daemon/errors.ts`

## Expected Output

- `src/__tests__/api/daemon/clipboard.test.ts`
- `src/__tests__/api/daemon/settings.test.ts`
- `src/__tests__/api/daemon/encryption.test.ts`
- `src/__tests__/api/daemon/storage.test.ts`

## Verification

All integration tests pass. `npm test` or `bun test` returns 0 failures.
