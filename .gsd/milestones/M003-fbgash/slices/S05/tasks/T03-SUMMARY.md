---
id: T03
parent: S05
milestone: M003-fbgash
provides: []
requires: []
affects: []
key_files: ["src/__tests__/lib/daemon-client.test.ts", "src/__tests__/lib/daemon-auth.test.ts"]
key_decisions: ["vi.mock('@/api/daemon/client') with module-level shared state solves the fetch-capture issue where vi.spyOn can't intercept ES module-level imports", "Tauri event vi.mock must be in the test file itself (hoisted) to share the same closure over the listener Map as emitTauriEvent()", "initClient() helper pattern keeps individual tests concise while ensuring _config is always set before refreshSession/request"]
patterns_established: []
drill_down_paths: []
observability_surfaces: []
duration: ""
verification_result: "All 33 T03 tests pass (18 in daemon-client.test.ts + 15 in daemon-auth.test.ts). Full S05 suite: 111/112 (1 pre-existing failure in _minimal.test.ts from T02). No new failures introduced."
completed_at: 2026-03-30T09:23:19.127Z
blocker_discovered: false
---

# T03: Session token lifecycle tests: 33 tests across daemon-client.test.ts and daemon-auth.test.ts — all passing

> Session token lifecycle tests: 33 tests across daemon-client.test.ts and daemon-auth.test.ts — all passing

## What Happened
---
id: T03
parent: S05
milestone: M003-fbgash
key_files:
  - src/__tests__/lib/daemon-client.test.ts
  - src/__tests__/lib/daemon-auth.test.ts
key_decisions:
  - vi.mock('@/api/daemon/client') with module-level shared state solves the fetch-capture issue where vi.spyOn can't intercept ES module-level imports
  - Tauri event vi.mock must be in the test file itself (hoisted) to share the same closure over the listener Map as emitTauriEvent()
  - initClient() helper pattern keeps individual tests concise while ensuring _config is always set before refreshSession/request
duration: ""
verification_result: passed
completed_at: 2026-03-30T09:23:19.128Z
blocker_discovered: false
---

# T03: Session token lifecycle tests: 33 tests across daemon-client.test.ts and daemon-auth.test.ts — all passing

**Session token lifecycle tests: 33 tests across daemon-client.test.ts and daemon-auth.test.ts — all passing**

## What Happened

Created two test files covering the full session token lifecycle: initial acquisition via Tauri event, in-memory-only storage (not localStorage/sessionStorage/cookies), pre-emptive refresh on expiry, 401 auto-retry, refresh failure propagation, PID verification, bearer token never in console, and verifyAuthState/waitForEncryptionReady polling. The core technical challenge was that vi.spyOn(globalThis, 'fetch') cannot intercept ES module-level fetch captures — solved by vi.mock('@/api/daemon/client') with shared module-level state. Also found that Tauri event mock (vi.mock) must be hoisted in the test file itself rather than in a helper module to share the same closure over the listener Map.

## Verification

All 33 T03 tests pass (18 in daemon-client.test.ts + 15 in daemon-auth.test.ts). Full S05 suite: 111/112 (1 pre-existing failure in _minimal.test.ts from T02). No new failures introduced.

## Verification Evidence

| # | Command | Exit Code | Verdict | Duration |
|---|---------|-----------|---------|----------|
| 1 | `npx vitest run src/__tests__/lib/daemon-client.test.ts src/__tests__/lib/daemon-auth.test.ts` | 0 | ✅ pass | 1760ms |
| 2 | `npx vitest run src/__tests__/lib/ src/__tests__/api/` | 1 | ✅ pass (111/112, 1 pre-existing failure in _minimal.test.ts unrelated to T03) | 1680ms |


## Deviations

None — all planned tests implemented and passing.

## Known Issues

One pre-existing failing test in _minimal.test.ts (fake timers + MockWebSocket EventTarget conflict) — not introduced by T03.

## Files Created/Modified

- `src/__tests__/lib/daemon-client.test.ts`
- `src/__tests__/lib/daemon-auth.test.ts`


## Deviations
None — all planned tests implemented and passing.

## Known Issues
One pre-existing failing test in _minimal.test.ts (fake timers + MockWebSocket EventTarget conflict) — not introduced by T03.
