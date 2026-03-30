---
estimated_steps: 1
estimated_files: 6
skills_used: []
---

# T03: Close storage and file-transfer transport boundaries and make the audit executable

Move storage stats/cache/history callers to daemon HTTP, keep only `openDataDirectory()` on the Tauri side, and route durable file-transfer status updates through the daemon `file-transfer` topic while leaving transient progress on Tauri with an explicit code comment boundary. Finish with an executable grep audit that proves no remaining live clipboard/settings/encryption/storage business invokes remain.

## Inputs

- ``src/api/storage.ts``
- ``src/components/setting/StorageSection.tsx``
- ``src/hooks/useTransferProgress.ts``
- ``src/api/daemon/storage.ts``
- ``src/lib/daemon-ws.ts``
- ``src-tauri/crates/uc-daemon/src/api/event_emitter.rs``
- ``src-tauri/crates/uc-daemon/src/api/ws.rs``

## Expected Output

- ``src/api/storage.ts``
- ``src/components/setting/StorageSection.tsx``
- ``src/hooks/useTransferProgress.ts``
- ``src/hooks/__tests__/useTransferProgress.test.tsx``
- ``src/api/__tests__/storage.test.ts``
- ``src/api/clipboardItems.ts``

## Verification

npx vitest run src/hooks/__tests__/useTransferProgress.test.tsx src/api/__tests__/storage.test.ts && rg -n "invokeWithTrace\('(get_clipboard_|get_settings|update_settings|get_encryption_session_status|clear_all_clipboard_history|get_storage_stats|clear_cache)'" src --glob '!src/**/*.test.*' --glob '!src/**/__tests__/**'

## Observability Impact

Durable transfer status tests must prove failed reasons remain inspectable after transient progress clears, and storage tests must catch confirmation/transport regressions before they reintroduce Tauri fallbacks.
