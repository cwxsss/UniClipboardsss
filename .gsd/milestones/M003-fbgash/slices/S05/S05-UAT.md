# S05: Frontend-Daemon Integration Testing & Security Audit — UAT

**Milestone:** M003-fbgash
**Written:** 2026-03-30T09:33:34.007Z

## UAT Test Cases

### HTTP API: Clipboard Endpoints (18 tests)
- GET /clipboard/entries: correct pagination shape, empty page
- GET /clipboard/entries/:id: not found (404 → error), success (correct shape)
- DELETE /clipboard/entries/:id: 404 → error, 204 success
- POST /clipboard/entries/:id/restore: 404 → error, 409 already-restored → error, 200 success
- POST /clipboard/entries/:id/favorite: correct toggle
- GET /clipboard/stats: correct shape
- Auto-refresh on 401: 3 fetch calls (401 → refresh → retry → success)
- Refresh failure: error propagated correctly

### HTTP API: Settings Endpoints (9 tests)
- GET /settings: correct shape (snake_case)
- PUT /settings: validation error (400), success

### HTTP API: Encryption Endpoints (11 tests)
- GET /encryption/state: correct shape (camelCase: sessionReady)
- POST /encryption/unlock: wrong passphrase → 401, success → { sessionReady: true }
- POST /encryption/lock: success

### HTTP API: Storage Endpoints (9 tests)
- GET /storage/stats: correct shape
- POST /storage/clear-cache: missing confirmed → 400 CONFIRMATION_REQUIRED, confirmed:true → 200

### WebSocket Event Delivery (28 tests)
- Subscribe receives events within 100ms, unsubscribe stops events
- Reconnect with exponential backoff, auto-resubscribe after reconnect
- Rapid sequential events delivered in order, JSON parse error resilience
- Callback exception resilience, singleton reset, multiple rapid events

### Session Token Lifecycle (33 tests)
- Token not in localStorage/sessionStorage (grep verification)
- Bearer token not in URL query params (HTTP uses header)
- Session refresh on 401 (3 fetch calls), refresh failure propagation
- PID included in requests, verifyAuthState polls until ready

### Security Audit (6 checks)
- SEC01: No token in localStorage/sessionStorage/cookies → 0 matches
- SEC02: Authorization header used for HTTP, not URL query param
- SEC03: Rate limit 100 req/min enforced (Rust tests)
- SEC04: PID whitelist enforced (Rust tests)
- SEC05: L2: missing session → 401 missing_session_token
- SEC06: L4: clear-cache without confirmed → 400 confirmation_required
- SEC07: JWT expiry enforced (Rust tests)
- SEC08: No wildcard CORS headers (0 matches in codebase)

### Edge Cases
- WS connects to daemon that immediately closes → backoff reconnect
- Session refresh returns 401 → error propagated, no infinite loop
- Multiple DaemonWsClient instantiations → singleton enforces single instance
