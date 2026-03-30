---
estimated_steps: 3
estimated_files: 4
skills_used:
  - debug-like-expert
---

# T01: Make daemon `/ws` accept browser-safe session auth and lock the contract with Rust tests

**Slice:** S07 — Direct Daemon WS & Integration Proof Remediation
**Milestone:** M003-fbgash

## Description

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

## Verification

- `env -C src-tauri cargo test -p uc-daemon --test websocket_api --test pairing_ws`
- Confirm the updated tests cover both header and query-param auth variants plus negative auth verdicts.

## Observability Impact

- Signals added/changed: websocket handshake verdicts and serialization regressions become explicit Rust test failures
- How a future agent inspects this: read `src-tauri/crates/uc-daemon/tests/websocket_api.rs` and `src-tauri/crates/uc-daemon/tests/pairing_ws.rs`
- Failure state exposed: auth transport mismatch vs JWT/PID/rate-limit failure is localized by test name

## Inputs

- `src-tauri/crates/uc-daemon/src/api/ws.rs` — current websocket auth/upgrade logic
- `src-tauri/crates/uc-daemon/src/api/types.rs` — current websocket DTO serialization contract
- `src-tauri/crates/uc-daemon/tests/websocket_api.rs` — websocket contract tests to expand
- `src-tauri/crates/uc-daemon/tests/pairing_ws.rs` — pairing websocket coverage and redaction assertions

## Expected Output

- `src-tauri/crates/uc-daemon/src/api/ws.rs` — browser-safe websocket auth path normalized with existing security checks
- `src-tauri/crates/uc-daemon/tests/websocket_api.rs` — query/header auth coverage plus envelope-key assertions
- `src-tauri/crates/uc-daemon/tests/pairing_ws.rs` — browser-auth pairing websocket coverage with redaction assertions
