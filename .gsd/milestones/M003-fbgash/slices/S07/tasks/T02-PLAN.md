---
estimated_steps: 3
estimated_files: 10
skills_used:
  - debug-like-expert
---

# T02: Unify frontend bootstrap around a live session before realtime connect

**Slice:** S07 — Direct Daemon WS & Integration Proof Remediation
**Milestone:** M003-fbgash

## Description

The frontend currently has two separate bootstrap paths: `daemon-auth` knows how to refresh the session token, while `daemon-ws-bootstrap` immediately opens the websocket after only `daemonClient.initialize()`. That race leaves `daemonWs` unauthenticated at startup and compounds the known `invalid_session_token` noise in setup realtime consumers.

## Failure Modes

| Dependency | On error | On timeout | On malformed response |
|------------|----------|-----------|----------------------|
| `daemon://connection-info` bootstrap event | Surface one high-signal bootstrap failure and keep retry/rehydration behavior inspectable | Leave startup in a bounded waiting state; do not spin uncontrolled retries | Reject malformed payloads before using them to initialize clients |
| `daemonClient.refreshSession()` | Bubble auth failure to bootstrap diagnostics without leaking token values | Keep websocket connect blocked until a valid session exists | Treat missing session/token fields as bootstrap failure |
| `daemonWs` + realtime bridge | Keep reconnect state visible and subscriptions recoverable | Retry via existing backoff instead of creating duplicate sockets | Parse the actual daemon event keys (`type`, `sessionId`) and fail tests if stale keys are used |

## Load Profile

- **Shared resources**: one daemon HTTP session, one websocket singleton, setup realtime retry timer
- **Per-operation cost**: one `/auth/connect` at bootstrap plus websocket reconnect backoff on disconnect
- **10x breakpoint**: duplicate bootstrap calls or reconnect loops would create auth churn first, so bootstrap must stay idempotent

## Negative Tests

- **Malformed inputs**: missing bootstrap payload fields, websocket events using stale key shapes, empty session state
- **Error paths**: auth refresh failure, websocket close-before-open, reconnect after disconnect, setup store startup failure
- **Boundary conditions**: repeated bootstrap calls stay idempotent, late realtime events hydrate once, reconnect preserves subscriptions

## Steps

1. Refactor `src/main.tsx`, `src/lib/daemon-auth.ts`, and `src/lib/daemon-ws-bootstrap.ts` so one authenticated bootstrap path waits for `daemon://connection-info`, refreshes the daemon session, then opens `daemonWs`.
2. Update `src/lib/daemon-ws.ts` and `src/api/realtime.ts` to consume the daemon’s real websocket envelope shape while preserving the legacy frontend callback contract.
3. Tighten frontend tests around bootstrap ordering, reconnect behavior, and setup realtime hydration/retry handling in `src/__tests__/lib/daemon-auth.test.ts`, `src/__tests__/lib/daemon-ws.test.ts`, `src/api/__tests__/p2p-realtime-contract.test.ts`, and `src/store/__tests__/setupRealtimeStore.test.ts`.

## Must-Haves

- [ ] Websocket connect never starts before `daemonClient` has a live session token.
- [ ] Frontend realtime parsing matches daemon `type` / `sessionId` fields without regressing existing callers.
- [ ] Setup realtime startup/retry tests cover the auth/bootstrap timing that previously produced `invalid_session_token` churn.

## Verification

- `npx vitest run src/__tests__/lib/daemon-auth.test.ts src/__tests__/lib/daemon-ws.test.ts src/api/__tests__/p2p-realtime-contract.test.ts src/store/__tests__/setupRealtimeStore.test.ts`
- Confirm reconnect and startup-order failures are reported by targeted test names rather than generic timeout noise.

## Observability Impact

- Signals added/changed: bootstrap/auth ordering, parsed event keys, and reconnect state become explicit in targeted frontend tests and startup logs
- How a future agent inspects this: run the Vitest command and inspect `src/store/setupRealtimeStore.ts` retry/rehydration state plus bootstrap logs
- Failure state exposed: auth refresh failure vs websocket timing vs stale envelope parsing is isolated by test scope

## Inputs

- `src/main.tsx` — current startup bootstrap entrypoint
- `src/lib/daemon-auth.ts` — session refresh/bootstrap helper already capable of authenticated setup
- `src/lib/daemon-ws.ts` — websocket client currently parsing stale event keys
- `src/lib/daemon-ws-bootstrap.ts` — websocket bootstrap path that currently races session refresh
- `src/api/realtime.ts` — legacy envelope bridge that must preserve caller compatibility
- `src/store/setupRealtimeStore.ts` — startup consumer currently noisy on auth timing failures
- `src/__tests__/lib/daemon-auth.test.ts` — auth/bootstrap regression coverage
- `src/__tests__/lib/daemon-ws.test.ts` — websocket reconnect/envelope parsing coverage
- `src/api/__tests__/p2p-realtime-contract.test.ts` — bridge-level realtime contract checks
- `src/store/__tests__/setupRealtimeStore.test.ts` — setup store hydration/retry coverage

## Expected Output

- `src/main.tsx` — startup path uses one authenticated websocket bootstrap flow
- `src/lib/daemon-auth.ts` — source of truth for session-bearing daemon bootstrap
- `src/lib/daemon-ws.ts` — websocket client aligned with daemon envelope shape and reconnect expectations
- `src/lib/daemon-ws-bootstrap.ts` — bootstrap helper updated or reduced to a thin wrapper around authenticated startup
- `src/api/realtime.ts` — legacy callback bridge fed from the real websocket envelope
- `src/store/setupRealtimeStore.ts` — setup realtime startup resilient to auth/bootstrap timing
- `src/__tests__/lib/daemon-auth.test.ts` — auth/bootstrap ordering coverage updated
- `src/__tests__/lib/daemon-ws.test.ts` — websocket parsing/reconnect coverage updated
- `src/api/__tests__/p2p-realtime-contract.test.ts` — realtime bridge contract reflects daemon envelope
- `src/store/__tests__/setupRealtimeStore.test.ts` — startup retry/hydration regression coverage updated
