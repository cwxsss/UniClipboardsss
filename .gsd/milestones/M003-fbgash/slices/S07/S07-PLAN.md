# S07: Direct Daemon WS & Integration Proof Remediation

**Goal:** Remediate the remaining direct-daemon WebSocket gaps so the frontend/browser path can authenticate and stay connected with a live daemon session, then capture executable proof that realtime delivery, reconnect recovery, token lifecycle, and security claims all hold end-to-end.
**Demo:** After this: Live daemon WS auth works from browser client; end-to-end tests/UAT prove WS delivery, reconnect recovery, token lifecycle, and security/integration claims

## Tasks
- [x] **T01: Daemon /ws now accepts browser-safe ?auth=Session%20<jwt> query param auth with comprehensive Rust test coverage** — ## Why

The current daemon WebSocket handler still authenticates from `Authorization` headers, but the browser/WebView client can only send the session token via URL query parameter. Until the daemon accepts the browser-safe transport and the Rust tests prove the same JWT/PID/rate-limit rules still hold, direct frontend WebSocket auth cannot work live.

## Failure Modes

| Dependency | On error | On timeout | On malformed response |
|------------|----------|-----------|----------------------|
| `src-tauri/crates/uc-daemon/src/api/ws.rs` handshake/auth path | Return explicit 401/403/429 and keep logs redacted instead of silently upgrading | Fail the upgrade early; do not leave half-open sockets | Reject the request and cover the branch with Rust tests |
| `SecurityState` / JWT verification | Preserve existing verification path; do not add an unauthenticated query-param bypass | Keep upgrade path bounded by existing request lifecycle | Treat malformed `auth` values as unauthorized |

## Load Profile

- **Shared resources**: per-PID rate limiter, broadcast receiver fanout, websocket connection slots
- **Per-operation cost**: one JWT verification + whitelist lookup + rate-limit check per upgrade
- **10x breakpoint**: reconnect/auth storms would hit rate limiting or socket churn first, so tests must keep those verdicts observable

## Negative Tests

- **Malformed inputs**: missing `auth`, wrong prefix, empty token, invalid JWT, malformed query string
- **Error paths**: unauthorized upgrade, non-whitelisted PID, rate-limited reconnect path
- **Boundary conditions**: header auth still works, query auth works, event JSON still serializes `type`/`sessionId` only

## Steps

1. Update `src-tauri/crates/uc-daemon/src/api/ws.rs` to normalize `Authorization: Session <jwt>` and `?auth=Session%20<jwt>` into the same validation path without weakening JWT, PID whitelist, or rate limiting.
2. Expand `src-tauri/crates/uc-daemon/tests/websocket_api.rs` to cover browser-style query-param auth success plus missing/invalid auth rejection and the actual camelCase websocket envelope keys.
3. Update `src-tauri/crates/uc-daemon/tests/pairing_ws.rs` to use the browser-compatible auth path and re-assert pairing snapshot/event redaction guarantees.

## Must-Haves

- [ ] Browser-style `?auth=Session%20<jwt>` websocket upgrades succeed through the real daemon handler.
- [ ] Missing/invalid/malformed websocket auth still yields the expected 401/403/429 outcomes.
- [ ] Rust websocket tests assert `type` / `sessionId` JSON keys and keep pairing secrets out of snapshot payloads.

## Done when

A browser-compatible websocket handshake authenticates successfully, the old negative paths still fail safely, and the Rust websocket contract tests prove both auth transport variants and JSON envelope shape.
  - Estimate: 90m
  - Files: src-tauri/crates/uc-daemon/src/api/ws.rs, src-tauri/crates/uc-daemon/src/api/types.rs, src-tauri/crates/uc-daemon/tests/websocket_api.rs, src-tauri/crates/uc-daemon/tests/pairing_ws.rs
  - Verify: env -C src-tauri cargo test -p uc-daemon --test websocket_api --test pairing_ws
- [x] **T02: Added await daemonClient.refreshSession() before daemonWs.connect() in connectDaemonWs(), eliminating the startup race that caused invalid_session_token errors; added validatePayload() guard and rewritten p2p-realtime-contract.test.ts** — ## Why

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

## Done when

The frontend has one authenticated bootstrap flow, realtime consumers receive correctly parsed daemon events after reconnect, and the targeted Vitest suites catch bootstrap/session regressions.
  - Estimate: 90m
  - Files: src/main.tsx, src/lib/daemon-auth.ts, src/lib/daemon-ws.ts, src/lib/daemon-ws-bootstrap.ts, src/api/realtime.ts, src/store/setupRealtimeStore.ts, src/__tests__/lib/daemon-auth.test.ts, src/__tests__/lib/daemon-ws.test.ts, src/api/__tests__/p2p-realtime-contract.test.ts, src/store/__tests__/setupRealtimeStore.test.ts
  - Verify: npx vitest run src/__tests__/lib/daemon-auth.test.ts src/__tests__/lib/daemon-ws.test.ts src/api/__tests__/p2p-realtime-contract.test.ts src/store/__tests__/setupRealtimeStore.test.ts
- [ ] **T03: Add a live daemon WS proof harness and UAT evidence** — ## Why

S07 is the milestone’s final transport proof slice, so it needs something stronger than unit tests: a repeatable runtime probe and UAT notes that a future agent can run against a live daemon/browser session to confirm websocket auth, snapshot delivery, reconnect recovery, and security diagnostics.

## Failure Modes

| Dependency | On error | On timeout | On malformed response |
|------------|----------|-----------|----------------------|
| Live daemon HTTP `/auth/connect` | Exit non-zero with redacted diagnostics and point to auth/bootstrap failure | Time out with explicit stage name so operators know whether HTTP or WS stalled | Reject malformed JSON and print the response shape mismatch without echoing tokens |
| Live daemon websocket `/ws` | Exit non-zero with the handshake/status stage captured | Bound waits for open/message phases so reconnect hangs are inspectable | Treat unexpected envelope keys as proof failure |
| Proof docs / audit notes | Keep commands and expected outcomes in repo so future agents do not guess | N/A | N/A |

## Load Profile

- **Shared resources**: live daemon HTTP listener, live websocket connection, rate limiter entries for the proof client
- **Per-operation cost**: one bearer→session exchange, one websocket connection, one subscribe round-trip, bounded reconnect check
- **10x breakpoint**: repeated proof runs would trip rate limiting or reconnect churn first; the script should report that verdict clearly

## Negative Tests

- **Malformed inputs**: missing required env vars/CLI args, malformed URLs, missing session token in proof config
- **Error paths**: `/auth/connect` 401, websocket 401/403/429, no snapshot/event received before timeout
- **Boundary conditions**: self-test mode without live daemon, runtime mode against a live daemon, redacted logging only

## Steps

1. Add `scripts/verify-direct-daemon-ws.mjs` that exchanges bearer→session, opens a browser-compatible websocket URL, subscribes to one or more topics, and emits redacted pass/fail diagnostics suitable for CI or manual UAT.
2. Add or update a focused consumer-facing check (for example `src/hooks/__tests__/useClipboardEventStream.test.tsx`) so corrected websocket envelopes still drive a real frontend consumer after the bootstrap/auth fixes.
3. Update `docs/security-audit.md` and add `docs/uat/direct-daemon-ws.md` with exact runtime commands, expected outputs, and inspection guidance for auth/reconnect failures.

## Must-Haves

- [ ] Repo-local proof script can be run in self-test mode and live-daemon mode without printing raw bearer/session tokens.
- [ ] Consumer-level coverage proves the corrected websocket envelope still reaches a real frontend subscriber.
- [ ] UAT/security docs describe how to verify websocket auth, reconnect recovery, and failure diagnostics after shipping.

## Done when

There is a runnable live-proof harness, one consumer-facing test confirms the bridge still drives app code, and the repo documents how to reproduce and inspect the final transport proof.
  - Estimate: 60m
  - Files: scripts/verify-direct-daemon-ws.mjs, src/hooks/__tests__/useClipboardEventStream.test.tsx, docs/security-audit.md, docs/uat/direct-daemon-ws.md
  - Verify: node scripts/verify-direct-daemon-ws.mjs --self-test && npx vitest run src/hooks/__tests__/useClipboardEventStream.test.tsx && test -f docs/uat/direct-daemon-ws.md
