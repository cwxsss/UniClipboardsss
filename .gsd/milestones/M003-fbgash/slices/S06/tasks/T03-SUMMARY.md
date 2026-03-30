---
id: T03
parent: S06
milestone: M003-fbgash
provides: []
requires: []
affects: []
key_files: ["src/api/storage.ts", "src/api/security.ts", "src/hooks/useTransferProgress.ts", "src/hooks/__tests__/useTransferProgress.test.tsx", "src/api/__tests__/storage.test.ts", "src/api/__tests__/security.test.ts"]
key_decisions: ["Storage stats and cache/history operations now use daemon HTTP API instead of Tauri invoke", "Encryption session status uses daemon GET /encryption/state instead of Tauri command", "openDataDirectory remains on Tauri (requires native OS file explorer integration)", "File transfer progress/status events come from Tauri; durable entry status persisted to Redux", "Daemon client re-export pattern for backward compatibility (clearAllClipboardHistory)"]
patterns_established: []
drill_down_paths: []
observability_surfaces: []
duration: ""
verification_result: "All 21 tests pass (useTransferProgress: 10, storage: 5, security: 6). Grep audit passes with zero matches for the targeted Tauri invoke patterns."
completed_at: 2026-03-30T10:10:39.217Z
blocker_discovered: false
---

# T03: Migrated storage stats/cache/history and encryption session status to daemon HTTP API; added test coverage

> Migrated storage stats/cache/history and encryption session status to daemon HTTP API; added test coverage

## What Happened
---
id: T03
parent: S06
milestone: M003-fbgash
key_files:
  - src/api/storage.ts
  - src/api/security.ts
  - src/hooks/useTransferProgress.ts
  - src/hooks/__tests__/useTransferProgress.test.tsx
  - src/api/__tests__/storage.test.ts
  - src/api/__tests__/security.test.ts
key_decisions:
  - Storage stats and cache/history operations now use daemon HTTP API instead of Tauri invoke
  - Encryption session status uses daemon GET /encryption/state instead of Tauri command
  - openDataDirectory remains on Tauri (requires native OS file explorer integration)
  - File transfer progress/status events come from Tauri; durable entry status persisted to Redux
  - Daemon client re-export pattern for backward compatibility (clearAllClipboardHistory)
duration: ""
verification_result: passed
completed_at: 2026-03-30T10:10:39.219Z
blocker_discovered: false
---

# T03: Migrated storage stats/cache/history and encryption session status to daemon HTTP API; added test coverage

**Migrated storage stats/cache/history and encryption session status to daemon HTTP API; added test coverage**

## What Happened

This task completed the transport boundary closure for storage and file-transfer operations:

1. Fixed `src/api/storage.ts`: Rewrote cleanly to use daemon HTTP client (`/storage/stats`, `/storage/clear-cache`) with only `openDataDirectory()` remaining on Tauri for OS integration.

2. Fixed `src/api/security.ts`: Changed `getEncryptionSessionStatus()` to use daemon `GET /encryption/state` instead of Tauri invoke. Added re-export of daemon encryption client functions. Kept Keychain operations on Tauri.

3. Updated `src/api/storage.ts`: Added re-export of `clearClipboardHistory` from daemon clipboard API as `clearAllClipboardHistory` for backward compatibility with `StorageSection.tsx`.

4. Created comprehensive tests:
   - `src/api/__tests__/storage.test.ts`: 5 tests for daemon API calls and Tauri fallback
   - `src/api/__tests__/security.test.ts`: 6 tests (updated to mock daemon client)
   - `src/hooks/__tests__/useTransferProgress.test.tsx`: 10 tests including observability requirement for failed reasons remaining inspectable

All 21 tests pass. Grep audit confirms zero remaining Tauri invokes for storage/encryption operations in non-test code.

## Verification

All 21 tests pass (useTransferProgress: 10, storage: 5, security: 6). Grep audit passes with zero matches for the targeted Tauri invoke patterns.

## Verification Evidence

| # | Command | Exit Code | Verdict | Duration |
|---|---------|-----------|---------|----------|
| 1 | `npx vitest run src/hooks/__tests__/useTransferProgress.test.tsx` | 0 | ✅ pass | 255ms |
| 2 | `npx vitest run src/api/__tests__/storage.test.ts` | 0 | ✅ pass | 4ms |
| 3 | `npx vitest run src/api/__tests__/security.test.ts` | 0 | ✅ pass | 3ms |
| 4 | `rg -n invokeWithTrace for get_storage_stats, clear_cache, clear_all_clipboard_history, get_encryption_session_status` | 1 | ✅ pass (no matches) | 100ms |


## Deviations

None

## Known Issues

None

## Files Created/Modified

- `src/api/storage.ts`
- `src/api/security.ts`
- `src/hooks/useTransferProgress.ts`
- `src/hooks/__tests__/useTransferProgress.test.tsx`
- `src/api/__tests__/storage.test.ts`
- `src/api/__tests__/security.test.ts`


## Deviations
None

## Known Issues
None
