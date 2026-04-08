---
id: T01
parent: S01
milestone: M003-fbgash
provides: []
requires: []
affects: []
key_files: ["src/api/daemon/client.ts", "src/api/daemon/types.ts", "src/api/daemon/errors.ts", "src/api/daemon/index.ts", "src/api/daemon/__tests__/client.test.ts"]
key_decisions: ["Daemon config bootstrapped via initialize(config) instead of Tauri invoke — config comes from daemon://connection-info event", "Concurrent refreshSession calls coalesced via shared promise", "Created types.ts and errors.ts in T01 since client.ts requires them to compile"]
patterns_established: []
drill_down_paths: []
observability_surfaces: []
duration: ""
verification_result: "TypeScript compiles cleanly (npx tsc --noEmit). All 14 unit tests pass covering: initialize, refreshSession (success, coalescing, auth failure), request<T> (auto-refresh, 401 retry, typed response, error mapping), destroy (clears state/timer), and keep-alive timer behavior."
completed_at: 2026-03-30T02:57:18.219Z
blocker_discovered: false
---

# T01: Created DaemonClient singleton with session token lifecycle, auto-refresh every 4min, typed request/error handling, and 401 auto-retry

> Created DaemonClient singleton with session token lifecycle, auto-refresh every 4min, typed request/error handling, and 401 auto-retry

## What Happened
---
id: T01
parent: S01
milestone: M003-fbgash
key_files:
  - src/api/daemon/client.ts
  - src/api/daemon/types.ts
  - src/api/daemon/errors.ts
  - src/api/daemon/index.ts
  - src/api/daemon/__tests__/client.test.ts
key_decisions:
  - Daemon config bootstrapped via initialize(config) instead of Tauri invoke — config comes from daemon://connection-info event
  - Concurrent refreshSession calls coalesced via shared promise
  - Created types.ts and errors.ts in T01 since client.ts requires them to compile
duration: ""
verification_result: passed
completed_at: 2026-03-30T02:57:18.220Z
blocker_discovered: false
---

# T01: Created DaemonClient singleton with session token lifecycle, auto-refresh every 4min, typed request/error handling, and 401 auto-retry

**Created DaemonClient singleton with session token lifecycle, auto-refresh every 4min, typed request/error handling, and 401 auto-retry**

## What Happened

Created src/api/daemon/ with four modules: client.ts (DaemonClient singleton with initialize/refreshSession/request/destroy, 240s keep-alive, concurrent refresh coalescing, 401 auto-retry), types.ts (DaemonConfig, SessionToken, isSessionExpired), errors.ts (DaemonApiError, DaemonErrorCode, mapStatusToErrorCode), and index.ts (barrel exports). Adapted from plan: used initialize(config) instead of invoke('daemon_connect_info') since no such Tauri command exists — config arrives via daemon://connection-info event.

## Verification

TypeScript compiles cleanly (npx tsc --noEmit). All 14 unit tests pass covering: initialize, refreshSession (success, coalescing, auth failure), request<T> (auto-refresh, 401 retry, typed response, error mapping), destroy (clears state/timer), and keep-alive timer behavior.

## Verification Evidence

| # | Command | Exit Code | Verdict | Duration |
|---|---------|-----------|---------|----------|
| 1 | `npx tsc --noEmit` | 0 | ✅ pass | 46400ms |
| 2 | `npx vitest run src/api/daemon/__tests__/client.test.ts` | 0 | ✅ pass | 5800ms |


## Deviations

Created types.ts and errors.ts alongside client.ts (planned as T02/T03) because they are required imports. Used initialize(config) instead of invoke('daemon_connect_info') because the Tauri command doesn't exist.

## Known Issues

None.

## Files Created/Modified

- `src/api/daemon/client.ts`
- `src/api/daemon/types.ts`
- `src/api/daemon/errors.ts`
- `src/api/daemon/index.ts`
- `src/api/daemon/__tests__/client.test.ts`


## Deviations
Created types.ts and errors.ts alongside client.ts (planned as T02/T03) because they are required imports. Used initialize(config) instead of invoke('daemon_connect_info') because the Tauri command doesn't exist.

## Known Issues
None.
