# Triage Report: Frontend WS Event Name Mismatches

## Root Cause

The frontend subscribes to WebSocket event names using **hyphen/camelCase**, but the daemon
emits **snake_case** event names (defined in `uc-core/src/network/daemon_api_strings.rs`).
The daemon is the authoritative source — frontend must match daemon output exactly.

## Affected Events

### HIGH Impact

| Frontend (wrong)           | Daemon (correct)            | Files affected                          |
|---------------------------|-----------------------------|-----------------------------------------|
| `clipboard.new-content`    | `clipboard.new_content`     | `useClipboardEventStream.ts` (×2)      |
|                           |                             | `useTransferProgress.ts` (×1)           |
|                           |                             | `useDaemonEvents.ts` (×1)               |
|                           |                             | `scripts/verify-direct-daemon-ws.mjs`   |
|                           |                             | Test files (5 files)                    |

**Impact**: All clipboard change events are silently ignored by the frontend.

### MEDIUM Impact

| Frontend (wrong)               | Daemon (correct)                  | Files affected          |
|-------------------------------|-----------------------------------|------------------------|
| `encryption.sessionReady`     | `encryption.session_ready`         | `useEncryptionSessionState.ts` |
|                               |                                   | `useDaemonEvents.ts`   |
| `pairing.verificationRequired`| `pairing.verification_required`   | `useDaemonEvents.ts`   |

### LOW Impact (dead code / non-existent events)

| Frontend             | Daemon status | Files affected          |
|---------------------|---------------|------------------------|
| `encryption.sessionFailed` | No such event emitted | `useDaemonEvents.ts` |
| `clipboard.deleted` | No such event emitted | `useClipboardEventStream.ts` |

## Files to Fix

### Source files (4)
- `src/hooks/useClipboardEventStream.ts` — 2 occurrences
- `src/hooks/useTransferProgress.ts` — 1 occurrence
- `src/hooks/useDaemonEvents.ts` — 4 occurrences (`clipboard.new-content`, `pairing.verificationRequired`, `encryption.sessionReady`, `encryption.sessionFailed`)
- `src/hooks/useEncryptionSessionState.ts` — 1 occurrence

### Test files (5)
- `src/hooks/__tests__/useClipboardEventStream.test.tsx`
- `src/hooks/__tests__/useClipboardEvents.test.ts`
- `src/hooks/__tests__/useDaemonEvents.test.ts`
- `src/hooks/__tests__/useTransferProgress.test.tsx`
- `scripts/verify-direct-daemon-ws.mjs`

## Fix Approach

1. Rename all hyphen/camelCase event names to snake_case to match daemon output.
2. Remove `encryption.sessionFailed` handler (daemon never emits it) — either delete the branch
   or note it as unreachable.
3. `clipboard.deleted` branch can be removed or kept as defensive code (it will never fire).

## Blast Radius

- Clipboard sync is completely broken for remote entries (but may appear to work for local due to
  Tauri-side clipboard capture).
- Pairing verification code screen won't appear for remote pairing requests.
- Encryption ready state relies on polling fallback; WS event is dead code.

## Proposed Fix Order

Fix source files first, then update all test files in the same commit to maintain consistency.
