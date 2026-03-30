# KNOWLEDGE.md — Project Patterns and Lessons Learned

> Append-only. Lessons that save future agents from repeating investigation.

---

## Rust pub(crate) visibility does not cross crate boundaries

**Pattern:** When a function is declared `pub(crate)` in module `foo` of crate `A`, it is NOT accessible from crate `B` that depends on `A`. `pub(crate)` means "public within crate A" — crate B is a different crate.

**Lesson:** If uc-daemon needs a utility from uc-app, and it's only `pub(crate)`, either:

1. Make it `pub` in uc-app (if appropriate), or
2. Re-implement inline in uc-daemon (chosen in D001)

**Seen in:** M002-zldd9y / S03 — `uc-app/usecases/storage::dir_size` was `pub(crate)`, so `compute_dir_size()` was re-implemented in `storage.rs`.

---

## L4 destructive operation pattern: JsonRejection + explicit false check

**Pattern:** When an HTTP endpoint requires explicit user confirmation for a destructive action:

```rust
// 1. JsonRejection catches missing body
let Json(req) = body_result else {
    return (StatusCode::BAD_REQUEST, Json(confirmation_error())).into_response();
};

// 2. Explicit false check
if !req.confirmed {
    return (StatusCode::BAD_REQUEST, Json(confirmation_error())).into_response();
}
```

**Lesson:** Both missing body (JsonRejection) and explicit `confirmed: false` must return 400. JsonRejection alone only catches missing/malformed JSON — a body of `{}` would parse as valid JSON but have no `confirmed` field.

**Seen in:** M002-zldd9y / S03 — `POST /storage/clear-cache`.

---

## Tauri one-shot event bootstrap over invoke for one-time config delivery

**Pattern:** When the frontend needs a one-time config payload from the Rust side (e.g., daemon connection info), use a Tauri `emit()` + frontend `listen()` event pair instead of `invoke()`.

```typescript
// Frontend: listen for one-shot event
function waitForConnectionEvent(): Promise<Payload> {
  return new Promise((resolve) => {
    listen<Payload>('daemon://connection-info', (event) => {
      resolve(event.payload)
    })
  })
}
```

**Lesson:** `invoke()` requires a matching Tauri command in Rust (registered with `#[tauri::command]`). If no such command exists, an event-driven approach works without requiring a Rust change. Events are one-shot (auto-unsubscribe after first receipt) and are the correct primitive for "here is your initial config" messages.

**Seen in:** M003-fbgash / S01 / T01 — `daemon://connection-info` event carries `{ baseUrl, wsUrl, token }`. The plan assumed `invoke('daemon_connect_info')` but that command doesn't exist.

---

## Rust serde field naming is inconsistent between endpoints — always match Rust source

**Pattern:** Different Rust endpoints use different `#[serde(rename_all = "...")]` conventions. TypeScript must match the actual serialization, not assume a consistent convention.

| Module | Rust convention | TS field names |
|---|---|---|
| `/settings/*` | Default (snake_case) | `sync_settings.enabled` |
| `/encryption/*` | `rename_all = "camelCase"` | `sessionReady` |

**Lesson:** Always read the actual Rust `serde` attributes in the handler before writing TypeScript types. Never assume snake_case or camelCase without checking. A mismatch causes silent `undefined` values at runtime.

**Seen in:** M003-fbgash / S01 / T05 (settings — snake_case) and T06 (encryption — camelCase).

---

## globalThis.process is not accessible in all JS environments — use type assertion

**Pattern:** Accessing `globalThis.process` in a webview (non-Node context) causes TypeScript errors (`globalThis` has no index signature).

```typescript
// ❌ Type error: 'process' doesn't exist on type 'typeof globalThis'
pid: globalThis.process?.pid ?? 0

// ✅ Type assertion
pid: (globalThis as unknown as { process?: { pid?: number } }).process?.pid ?? 0
```

**Lesson:** In Tauri webviews, `globalThis` is the browser's `Window` object, not Node's `global`. The `process` object may be shimmed but TypeScript's `lib.es5.d.ts` doesn't know about it. Use a type assertion to access it safely.

**Seen in:** M003-fbgash / S01 / T04 — `daemon-auth.ts` PID field.

---

## Known issue: daemon 401 invalid_session_token on dev startup

**Symptom:** After `tauri:dev` launches, the frontend console floods with:

```text
Failed to initialize setup realtime store:
daemon setup request /setup/state failed with status 401 Unauthorized: {"error":"invalid_session_token"}
```

The `setupRealtimeStore.ts` retry loop (line ~106) retries indefinitely, generating noise.

**Root cause:** Not yet diagnosed. The session token the frontend sends to the daemon does not match what the daemon expects. Likely a race condition or stale token during dev startup — the frontend may attempt `/setup/state` before the daemon has registered the token emitted via `daemon://connection-info`.

**Impact:** The app cannot proceed past setup initialization. All daemon-dependent features (clipboard list, stats, etc.) are unreachable until this is resolved.

**Where to investigate:**
- `src/store/setupRealtimeStore.ts` — retry loop and token source
- `src/api/daemon/client.ts` — how the session token is attached to requests
- `src/lib/daemon-auth.ts` — token acquisition from Tauri event
- Rust side: daemon session token generation and validation logic

**Seen in:** M003-fbgash / S02 / T03 — observed during browser smoke test attempt.

---

## Daemon clipboard endpoints return preview projections only — full content still via Tauri

**Pattern:** GET /clipboard/entries returns EntryProjectionDto (preview data: text preview, size, timestamps, thumbnails, link metadata). The full clipboard entry detail (decrypted text, full content, resource metadata) is still served by the Tauri command `get_clipboard_entry_detail`.

**Lesson:** ClipboardSlice transforms daemon DTOs into ClipboardItemResponse using a local `transformDtoToItemResponse()` helper. This keeps the slice independent of clipboardItems.ts. The old clipboardItems.ts is retained only for type/enum imports — no function calls. When the daemon gains a full content endpoint, the Tauri path in clipboardItems.ts can be replaced without touching clipboardSlice.

**Seen in:** M003-fbgash / S02 / T01 — `src/api/daemon/clipboard.ts` and `src/store/slices/clipboardSlice.ts`.

---

## Grep audit for invoke() must be scoped to migrated files only

**Pattern:** When migrating from Tauri invoke() to daemon HTTP, the old Tauri module (clipboardItems.ts) intentionally retains invoke() calls for backward compatibility — types/enums are still imported from it. Broader `rg 'invoke' src/` will always find matches in the old module.

**Lesson:** The migration audit grep must be scoped to `src/store/slices/ src/api/daemon/` (the migrated layer) and exclude `src/api/clipboardItems.ts`. The T03 task plan correctly scoped it this way.

**Seen in:** M003-fbgash / S02 / T03 — audit confirmed zero invoke() clipboard calls in migrated files.

---

## clearClipboardItems has no daemon endpoint — falls back to Tauri invoke

**Pattern:** The `POST /clipboard/entries/clear` or similar endpoint does not exist in the daemon. The `clearAllItems` thunk in clipboardSlice falls back to the Tauri invoke path.

**Lesson:** When adding clipboard endpoints to the daemon, `POST /clipboard/entries/clear` should be included. The fallback in clipboardSlice.ts is marked with a `// TODO: Replace with daemon API once /clipboard/entries clear endpoint is available` comment.

**Seen in:** M003-fbgash / S02 / T02 — `src/store/slices/clipboardSlice.ts`, `clearAllItems` thunk.
