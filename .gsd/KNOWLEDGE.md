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
