# S01: Frontend Daemon HTTP Client & Auth Module

**Goal:** Create DaemonClient class with session token management, typed request/error handling. Bootstrap via Tauri daemon_connect_info command. Auto-refresh session before expiry.
**Demo:** After this: DaemonClient singleton in src/api/daemon/client.ts; loadDaemonAuth() and verifyAuthState() in src/lib/daemon-auth.ts; session refresh every 4min

## Tasks
- [x] **T01: Created DaemonClient singleton with session token lifecycle, auto-refresh every 4min, typed request/error handling, and 401 auto-retry** — Create `src/api/daemon/` directory and `client.ts` with singleton `daemonClient`.

`initialize()`: call Tauri `invoke('daemon_connect_info')` to get { baseUrl, wsUrl, pid, token }. Store in module state.

`refreshSession()`: POST to `/auth/connect` with bearer token + PID → get session token + expiresAt + encryptionReady. Returns `SessionToken`.

`request<T>(endpoint, options?)`: if session expired, auto-refresh once. Send HTTP request with `Authorization: Session <token>`. Return typed response.

`destroy()`: clear keep-alive timer.

Session keep-alive: setInterval every 4 minutes (240s) calling refreshSession() proactively.
  - Estimate: medium
  - Files: src/api/daemon/client.ts
  - Verify: TypeScript compiles. Unit tests: initialize succeeds when daemon is up, refreshSession returns valid session token, request<T> returns typed data, auto-retry on 401, destroy clears timer.
- [ ] **T02: DaemonConfig and SessionToken types** — Define TypeScript interfaces:

```typescript
interface DaemonConfig {
  baseUrl: string;   // e.g. "http://127.0.0.1:xxxxx"
  wsUrl: string;     // e.g. "ws://127.0.0.1:xxxxx/ws"
  pid: number;
  token: string;     // bearer token
}

interface SessionToken {
  token: string;     // JWT session token
  expiresAt: number; // unix timestamp ms
  encryptionReady: boolean;
}

function isSessionExpired(token: SessionToken | null): boolean
```
  - Estimate: small
  - Files: src/api/daemon/types.ts
  - Verify: Types used correctly in DaemonClient. No runtime type errors.
- [ ] **T03: DaemonApiError class with typed error codes** — Create `src/api/daemon/errors.ts`:

```typescript
export class DaemonApiError extends Error {
  code: DaemonErrorCode;
  message: string;
  details?: unknown;
  constructor(code: DaemonErrorCode, message: string, details?: unknown)
}

export enum DaemonErrorCode {
  UNAUTHORIZED = 'UNAUTHORIZED',
  FORBIDDEN = 'FORBIDDEN',
  NOT_FOUND = 'NOT_FOUND',
  RATE_LIMITED = 'RATE_LIMITED',
  ENCRYPTION_NOT_READY = 'ENCRYPTION_NOT_READY',
  CONFIRMATION_REQUIRED = 'CONFIRMATION_REQUIRED',
  INTERNAL_ERROR = 'INTERNAL_ERROR',
}
```

Map HTTP status codes to error codes: 401→UNAUTHORIZED, 403→FORBIDDEN, 404→NOT_FOUND, 429→RATE_LIMITED, 503→ENCRYPTION_NOT_READY (or parse from response body).
  - Estimate: small
  - Files: src/api/daemon/errors.ts
  - Verify: Unit tests: error thrown correctly for each HTTP status, code and message fields populated from response.
- [ ] **T04: Auth module bridging Tauri bootstrap and daemon HTTP** — Create `src/lib/daemon-auth.ts`:

`loadDaemonAuth()`: call Tauri invoke for connection config → call DaemonClient.refreshSession() → return session token. Also extract wsUrl for later WS connection.

`verifyAuthState()`: check daemon is reachable (GET /lifecycle/ready or similar health check) and encryption status.

`waitForEncryptionReady(timeout)`: poll GET /encryption/state every 500ms until encryptionReady===true or timeout. Return boolean.

This module bridges Tauri IPC (for bootstrap) and daemon HTTP (for all subsequent calls).
  - Estimate: medium
  - Files: src/lib/daemon-auth.ts
  - Verify: Unit tests: loadDaemonAuth calls both Tauri and daemon HTTP. verifyAuthState returns correct state. waitForEncryptionReady resolves on ready, rejects on timeout.
- [ ] **T05: Settings API module** — Create `src/api/daemon/settings.ts`:

`getSettings()` → GET /settings via DaemonClient.request()
`updateSettings(settings)` → PUT /settings via DaemonClient.request()

Types from uc-core DTOs (SettingsResponse, SettingsUpdateRequest).
  - Estimate: small
  - Files: src/api/daemon/settings.ts
  - Verify: TypeScript compiles with correct response types. Integration test against running daemon.
- [ ] **T06: Encryption API module** — Create `src/api/daemon/encryption.ts`:

`getEncryptionState()` → GET /encryption/state
`unlockEncryption(passphrase)` → POST /encryption/unlock with { passphrase }
`lockEncryption()` → POST /encryption/lock

All via DaemonClient.request(). Return typed EncryptionStateResponse.
  - Estimate: small
  - Files: src/api/daemon/encryption.ts
  - Verify: TypeScript compiles. Integration test against running daemon.
