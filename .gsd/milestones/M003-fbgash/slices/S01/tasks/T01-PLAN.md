---
estimated_steps: 6
estimated_files: 1
skills_used: []
---

# T01: DaemonClient class with session management

Create `src/api/daemon/` directory and `client.ts` with singleton `daemonClient`.

`initialize()`: call Tauri `invoke('daemon_connect_info')` to get { baseUrl, wsUrl, pid, token }. Store in module state.

`refreshSession()`: POST to `/auth/connect` with bearer token + PID → get session token + expiresAt + encryptionReady. Returns `SessionToken`.

`request<T>(endpoint, options?)`: if session expired, auto-refresh once. Send HTTP request with `Authorization: Session <token>`. Return typed response.

`destroy()`: clear keep-alive timer.

Session keep-alive: setInterval every 4 minutes (240s) calling refreshSession() proactively.

## Inputs

- `src/api/ (existing Tauri API patterns)`
- `src-tauri/crates/uc-tauri/src/commands/daemon.rs (daemon_connect_info)`

## Expected Output

- `src/api/daemon/client.ts`

## Verification

TypeScript compiles. Unit tests: initialize succeeds when daemon is up, refreshSession returns valid session token, request<T> returns typed data, auto-retry on 401, destroy clears timer.
