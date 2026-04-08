---
estimated_steps: 3
estimated_files: 4
skills_used:
  - debug-like-expert
---

# T03: Add a live daemon WS proof harness and UAT evidence

**Slice:** S07 — Direct Daemon WS & Integration Proof Remediation
**Milestone:** M003-fbgash

## Description

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
2. Add or update a focused consumer-facing check in `src/hooks/__tests__/useClipboardEventStream.test.tsx` so corrected websocket envelopes still drive a real frontend consumer after the bootstrap/auth fixes.
3. Update `docs/security-audit.md` and add `docs/uat/direct-daemon-ws.md` with exact runtime commands, expected outputs, and inspection guidance for auth/reconnect failures.

## Must-Haves

- [ ] Repo-local proof script can be run in self-test mode and live-daemon mode without printing raw bearer/session tokens.
- [ ] Consumer-level coverage proves the corrected websocket envelope still reaches a real frontend subscriber.
- [ ] UAT/security docs describe how to verify websocket auth, reconnect recovery, and failure diagnostics after shipping.

## Verification

- `node scripts/verify-direct-daemon-ws.mjs --self-test`
- `npx vitest run src/hooks/__tests__/useClipboardEventStream.test.tsx`
- `test -f docs/uat/direct-daemon-ws.md && rg -n "invalid_session_token|reconnect|query parameter|self-test" docs/uat/direct-daemon-ws.md docs/security-audit.md`

## Observability Impact

- Signals added/changed: runtime proof stages (auth exchange, websocket open, subscribe, snapshot/event receipt, reconnect verdict) become explicit script output
- How a future agent inspects this: run `node scripts/verify-direct-daemon-ws.mjs --self-test` or the live-daemon mode documented in `docs/uat/direct-daemon-ws.md`
- Failure state exposed: auth failure vs websocket handshake vs snapshot timeout is emitted as a distinct stage and exit code

## Inputs

- `package.json` — available runtime/test tooling for the proof harness
- `src/api/daemon/client.ts` — HTTP session exchange contract the live proof must mirror
- `src/hooks/__tests__/useClipboardEventStream.test.tsx` — existing consumer-facing websocket coverage to extend
- `docs/security-audit.md` — current security claims to update after remediation

## Expected Output

- `scripts/verify-direct-daemon-ws.mjs` — redacted runtime proof harness for self-test and live-daemon execution
- `src/hooks/__tests__/useClipboardEventStream.test.tsx` — consumer-level coverage updated for corrected websocket envelopes
- `docs/security-audit.md` — security audit updated with browser-compatible websocket auth and proof guidance
- `docs/uat/direct-daemon-ws.md` — operator/UAT runbook for live websocket verification
