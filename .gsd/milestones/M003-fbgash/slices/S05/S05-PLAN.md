# S05: Frontend-Daemon Integration Testing & Security Audit

**Goal:** End-to-end integration tests for full frontend-daemon stack. Security audit covering token leakage, rate limiting, permission enforcement.
**Demo:** After this: Test suite runs: HTTP API correctness, WS event delivery, session token lifecycle, reconnection recovery, security properties

## Tasks
- [x] **T01: HTTP API integration tests: 47 tests across clipboard, settings, encryption, storage endpoints — all passing** — Create integration test suite for all daemon HTTP endpoints. Use Vitest + msw (mock service worker) or real HTTP client against running daemon.

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
  - Estimate: medium
  - Files: src/__tests__/api/daemon/clipboard.test.ts, src/__tests__/api/daemon/settings.test.ts, src/__tests__/api/daemon/encryption.test.ts, src/__tests__/api/daemon/storage.test.ts
  - Verify: All integration tests pass. `npm test` or `bun test` returns 0 failures.
- [x] **T02: WebSocket event delivery and reconnect tests: 28 tests all passing** — Test DaemonWsClient event delivery:

- Subscribe to 'clipboard' topic, copy on device B → event received within 100ms
- Subscribe to 'encryption' topic, lock/unlock → events received
- Kill daemon process → daemonWs reconnects with exponential backoff
- Restart daemon → frontend auto-resubscribes, data refreshes
- Multiple rapid events → all delivered in order
- Unsubscribe → no further events received

Use a test daemon instance or mock WebSocket server.
  - Estimate: medium
  - Files: src/__tests__/lib/daemon-ws.test.ts
  - Verify: All WS tests pass. Event latency measured and within 100ms threshold.
- [x] **T03: Session token lifecycle tests: 33 tests across daemon-client.test.ts and daemon-auth.test.ts — all passing** — Test session token lifecycle:

- Initial loadDaemonAuth() → session token obtained, stored in memory
- Session expiry (mock time or wait 5min in test): next request auto-refreshes
- Refresh failure (daemon down during refresh): error propagated correctly
- PID verification: request from unknown PID → 403 or appropriate rejection
- Bearer token never appears in console.log or network URL (grep tests)

Session token must not be stored in localStorage, sessionStorage, or cookies — only in-memory JS variable.
  - Estimate: medium
  - Files: src/__tests__/lib/daemon-auth.test.ts, src/__tests__/lib/daemon-client.test.ts
  - Verify: All session lifecycle tests pass. Grep verifies tokens not persisted to storage.
- [ ] **T04: Security audit and token leakage check** — Security audit checklist:

1. **Token leakage**: grep source for localStorage.setItem('token'), sessionStorage.setItem('token'), document.cookie with token → should find zero
2. **Bearer token placement**: verify Authorization header set only, never in URL query params
3. **Rate limiting**: send 101 requests in <1 minute → 429 on 101st
4. **Permission enforcement**:
   - L2 (no auth): health check works without session
   - L3 without encryption session: call encryption-modifying endpoint → ENCRYPTION_NOT_READY error
   - L4 without confirmation: call clear-cache with confirmed:false → 400 CONFIRMATION_REQUIRED
5. **PID verification**: make request from process with wrong PID → rejection
6. **CORS**: daemon HTTP responses should not have Access-Control-Allow-Origin: * (localhost is fine, but verify no wildcard)

Document findings in a security audit report.
  - Estimate: medium
  - Files: docs/security-audit.md
  - Verify: All security checks pass. Audit report documents each check and result. No critical issues found.
