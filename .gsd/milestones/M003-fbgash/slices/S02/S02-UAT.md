# S02: Frontend Clipboard API Migration — UAT

**Milestone:** M003-fbgash
**Written:** 2026-03-30T03:57:01.126Z

## Smoke Test (Unit/Artifact)

All checks run via `npx vitest run src/api/ src/store/` (80 tests, 11 files) and `npx tsc --noEmit` (0 errors). Five grep patterns confirm zero invoke() clipboard calls in src/store/slices/ and src/api/daemon/.

## Live Runtime Tests (Blocked)

Blocked by pre-existing daemon 401 auth issue on /setup/state (S01 scope). Tests documented in UAT for when auth issue is resolved:
- TC01: Clipboard list page loads entries
- TC02: Delete a clipboard entry
- TC03: Restore (copy to clipboard)
- TC04: Toggle favorite
- TC05: Stats page shows correct totals

## Edge Cases

- EC01: Daemon returns not_ready status → UI shows not-ready state
- EC02: API call fails with network error → error displayed, Redux error state set
- EC03: clearAllItems uses Tauri fallback (expected gap, tracked as R-NEW-1)

## Failure Signals

- TypeScript compilation error
- Any test fails
- Grep finds invoke() in src/store/slices/ or src/api/daemon/
- Browser console shows invoke() channel calls for clipboard

## Not Proven

- Live browser clipboard CRUD (blocked by S01 auth issue)
- Real daemon API endpoint path correctness for toggleFavorite/getClipboardEntryResource
- WebSocket real-time updates (S03 scope)
- UC-tauri command cleanup (S04 scope)
