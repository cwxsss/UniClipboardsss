# S01: Frontend Daemon HTTP Client & Auth Module — UAT

**Milestone:** M003-fbgash
**Written:** 2026-03-30T03:25:25.873Z

# S01 UAT — Frontend Daemon HTTP Client & Auth Module

## Test Case 1 — DaemonClient Bootstrap
1. Mock `daemon://connection-info` event with `{ baseUrl, wsUrl, token }`.
2. Call `loadDaemonAuth()`.
3. Assert returned session has non-empty token, future expiresAt, boolean encryptionReady.
4. Assert wsUrl matches emitted event.
5. Assert `daemonClient.isSessionExpired(session)` returns false.

## Test Case 2 — Session Keep-Alive (240s)
1. Initialize DaemonClient, record original session token.
2. Advance time by 240s (mock timer).
3. Assert POST /auth/connect was called again.
4. Assert new token differs or is valid.

## Test Case 3 — Auto-Retry on 401
1. Mock first request to return 401.
2. Mock refreshSession to return new token.
3. Mock retry to return 200 with data.
4. Assert final return value is the data (not an error).

## Test Case 4 — verifyAuthState Two-Phase Check
1. With daemon running: assert `{ daemonReady: true, encryptionInitialized: true|false, encryptionSessionReady: true|false }`.
2. Unlock encryption, call again: assert sessionReady becomes true.
3. Stop daemon: assert daemonReady becomes false; encryption fields stay at last known values (early return on L1 failure).

## Test Case 5 — waitForEncryptionReady Timeout
1. Mock /encryption/state to always return sessionReady=false.
2. Call `waitForEncryptionReady(1000)` — assert it rejects false after ~1s.
3. Mock sessionReady=true on first call.
4. Call `waitForEncryptionReady(5000)` — assert it resolves true immediately.

## Test Case 6 — Settings API Type Correctness
1. Call `getSettings()` — assert full Settings object with all sections.
2. Call `updateSettings({ sync: { enabled: false } })` — assert PUT /settings with partial payload, full merged response returned.

## Test Case 7 — Encryption API Endpoints
1. `getEncryptionState()` — GET /encryption/state, returns { initialized, sessionReady } camelCase.
2. `unlockEncryption("passphrase")` — POST /encryption/unlock with { passphrase }.
3. `lockEncryption()` — POST /encryption/lock, no body.
4. All three produce correct types with no TS errors.

## Test Case 8 — Error Code Mapping
1. For each HTTP status (401, 403, 404, 429, 503), mock failing request.
2. Assert thrown DaemonApiError.code matches expected enum value.
3. For unknown status (e.g. 418), assert code === INTERNAL_ERROR.
