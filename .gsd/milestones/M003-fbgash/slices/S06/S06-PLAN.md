# S06: Transport Boundary Closure Remediation

**Goal:** Close the remaining frontend transport gaps so clipboard/history/settings-storage business operations use daemon HTTP/WS contracts instead of legacy Tauri invoke paths, while leaving only explicitly shell-native commands on the Tauri side.
**Demo:** After this: Clipboard clear uses daemon HTTP endpoint only; grep audit shows zero clipboard/settings/encryption/storage invoke paths remain; file-transfer event scope explicitly closed or migrated

## Tasks
- [x] **T01: Added POST /clipboard/entries/clear daemon route with typed TypeScript wrappers for clear/detail/resource/favorite** — Close the backend/client contract gaps that still force clipboard business flows through Tauri. Add a confirmed daemon clear-history route, expose typed daemon wrappers for clear/detail/resource, and align the favorite toggle method with the real daemon route so frontend code can migrate without transport fallbacks.
  - Estimate: 1h
  - Files: src-tauri/crates/uc-daemon/src/api/clipboard.rs, src-tauri/crates/uc-app/src/usecases/clipboard/clear_history.rs, src-tauri/crates/uc-daemon/tests/clipboard_api.rs, src/api/daemon/clipboard.ts, src/api/daemon/index.ts, src/api/daemon/__tests__/clipboard.test.ts
  - Verify: cd src-tauri && cargo test -p uc-daemon clipboard_api -- --nocapture && npx vitest run src/api/daemon/__tests__/clipboard.test.ts
- [x] **T02: Migrated clipboard hooks, components, and Redux slice to use daemon HTTP client** — Replace the live clipboard business callers that still import runtime functions from `@/api/clipboardItems`. Move list hydration, single-entry reloads, preview/detail/resource loading, and clear/stats consumers onto daemon clients/store thunks, while leaving `clipboardItems.ts` as a types-and-native-utility module only.
  - Estimate: 1.5h
  - Files: src/hooks/useClipboardCollection.ts, src/hooks/useClipboardEventStream.ts, src/components/clipboard/ClipboardPreview.tsx, src/components/clipboard/ClipboardItem.tsx, src/preview-panel/PreviewPanel.tsx, src/components/layout/ActionBar.tsx, src/hooks/__tests__/useClipboardEventStream.test.tsx, src/preview-panel/__tests__/PreviewPanel.test.tsx
  - Verify: npx vitest run src/hooks/__tests__/useClipboardEventStream.test.tsx src/preview-panel/__tests__/PreviewPanel.test.tsx src/api/daemon/__tests__/clipboard.test.ts
- [x] **T03: Migrated storage stats/cache/history and encryption session status to daemon HTTP API; added test coverage** — Move storage stats/cache/history callers to daemon HTTP, keep only `openDataDirectory()` on the Tauri side, and route durable file-transfer status updates through the daemon `file-transfer` topic while leaving transient progress on Tauri with an explicit code comment boundary. Finish with an executable grep audit that proves no remaining live clipboard/settings/encryption/storage business invokes remain.
  - Estimate: 1.5h
  - Files: src/api/storage.ts, src/components/setting/StorageSection.tsx, src/hooks/useTransferProgress.ts, src/hooks/__tests__/useTransferProgress.test.tsx, src/api/__tests__/storage.test.ts, src/api/clipboardItems.ts
  - Verify: npx vitest run src/hooks/__tests__/useTransferProgress.test.tsx src/api/__tests__/storage.test.ts && rg -n "invokeWithTrace\('(get_clipboard_|get_settings|update_settings|get_encryption_session_status|clear_all_clipboard_history|get_storage_stats|clear_cache)'" src --glob '!src/**/*.test.*' --glob '!src/**/__tests__/**'
