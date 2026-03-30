# DECISIONS.md — Project Decision Register

> Append-only. Decisions are immutable once recorded.

---

## D001 — Inline compute_dir_size in uc-daemon instead of importing from uc-app

**Scope:** architecture  
**Decision:** Inline `compute_dir_size()` in `uc-daemon/src/api/storage.rs` using `tokio::fs` rather than importing `uc-app/usecases/storage::dir_size`  
**Choice:** Inline implementation in uc-daemon  
**Rationale:** `uc-app/usecases/storage::dir_size` is `pub(crate)` visibility, which is not accessible from `uc-daemon` (different crate boundary). Rust visibility applies at crate level, not module level.  
**Revisable:** Yes — if `dir_size` is promoted to `pub` in uc-app, the inline impl can be replaced with the import.  
**When:** M002-zldd9y / S03 (2026-03-30)  
**Made by:** agent

---

## D004 — WebSocket authentication via URL query param, not headers

**Scope:** pattern  
**Decision:** Pass session token to daemon WebSocket as `?auth=Session%20TOKEN` URL query parameter, not a custom header.  
**Choice:** `ws://host/ws?auth=Session%20TOKEN`  
**Rationale:** Browsers do not allow arbitrary headers on WebSocket upgrade requests — only safe CORS-allowed headers are permitted. The standard `Authorization` header is blocked. The daemon reads the token from the query parameter instead of a header. Frontend constructs the WS URL by appending the token to the `wsUrl` base from the Tauri bootstrap event.  
**Revisable:** Yes — if the daemon adds CORS-allowed header support or a browser-native auth mechanism becomes available.  
**When:** M003-fbgash / S03 / T01 (2026-03-30)  
**Made by:** agent

---

## D005 — DaemonWsEvent field name: eventType (not type) on incoming WebSocket messages

**Scope:** pattern  
**Decision:** The Rust `DaemonWsEvent` struct serializes `event_type` as `eventType` (camelCase). The legacy frontend event API uses `type`. The `realtime.ts` bridge does a one-line field rename (`eventType` → `type`) so all existing callers continue working.  
**Choice:** Frontend uses `eventType` in new code; `type` only in the `onDaemonRealtimeEvent()` bridge for backward compatibility.  
**Rationale:** Existing callers (useDeviceDiscovery, setup, p2p) consume `type` via the legacy `DaemonRealtimeEnvelope`. Rather than migrating all callers, the bridge normalizes the field name. New hook code (`usePairingEvents`, `useEncryptionState`) uses `eventType` directly.  
**Revisable:** Yes — once all legacy callers are migrated to the hooks, the field rename in realtime.ts can be removed.  
**When:** M003-fbgash / S03 / T03 (2026-03-30)  
**Made by:** agent

---

## D002 — L4 confirmation pattern for destructive HTTP endpoints

**Scope:** pattern  
**Decision:** Destructive operations (e.g., clear cache) require an explicit `confirmed: bool` field in the request body; HTTP 400 returned if absent (JsonRejection) or false.  
**Choice:** Confirmation field in request body, not query param or separate step  
**Rationale:** Matches the established pattern used elsewhere in the codebase (e.g., reset endpoints). JSON body is more structured and self-documenting than query params. JsonRejection handling catches both "missing body" and "malformed body" cases cleanly.  
**Revisable:** Yes — could be replaced with a two-phase confirm endpoint if UX requires a separate confirmation step.  
**When:** M002-zldd9y / S03 (2026-03-30)  
**Made by:** agent

---

## Decisions Table

| # | When | Scope | Decision | Choice | Rationale | Revisable? | Made By |
|---|------|-------|----------|--------|-----------|------------|---------|
| D001 | M003-fbgash / S01 / T01 (2026-03-30) | architecture | DaemonClient bootstrapped via Tauri event daemon://connection-info, not Tauri invoke command | Tauri one-shot event daemon://connection-info carries { baseUrl, wsUrl, token, pid } | The Tauri command `daemon_connect_info` does not exist in the Rust backend. The correct bootstrap path is the `daemon://connection-info` Tauri event emitted once by the Rust side when the daemon is ready. Frontend listens for this one-shot event, extracts config, and calls DaemonClient.initialize(config). The invoke approach in the original plan cannot work without a Rust-side addition. | Yes — if daemon_connect_info Tauri command is added in future, daemon-auth.ts can be updated to use invoke as primary with event as fallback. | agent |
| D002 | M003-fbgash / S02 / T01 | architecture | Daemon clipboard endpoints return EntryProjectionDto only; full ClipboardItemResponse requires Tauri command | transformDtoToItemResponse maps daemon projection DTO to frontend ClipboardItemResponse shape | GET /clipboard/entries returns EntryProjectionDto (preview data only). The full clipboard entry detail (full content, decrypted text, etc.) is still served by the Tauri command get_clipboard_entry_detail. clipboardSlice mirrors the daemon projection into the existing ClipboardItemResponse shape used by UI components, preserving the same data contract. This keeps S02 scope clean — full content access via daemon HTTP will be addressed when the daemon gains a detail endpoint. | Yes | agent |
| D003 | M003-fbgash / S02 / T02 | pattern | transformDtoToItemResponse duplicated in clipboardSlice.ts to avoid importing from old Tauri module | Inline transformDtoToItemResponse in clipboardSlice.ts | The old clipboardItems.ts (which contains all the Tauri invoke calls) also has a transformProjectionToResponse function. clipboardSlice originally imported from it. To keep clipboardSlice independent of the Tauri layer after migration, the transform logic is duplicated inline. clipboardItems.ts is retained only for type/enum imports (no function calls). When clipboardItems.ts is deleted after S04 (uc-tauri cleanup), the duplicate can be consolidated into a shared utility. | Yes | agent |
| D004 | M003-fbgash / S03 / T03 (2026-03-30) | pattern | DaemonWsEvent field name mapping: eventType vs type | Frontend uses 'eventType' for incoming DaemonWsEvent, maps to 'type' in the legacy DaemonRealtimeEnvelope | The Rust DaemonWsEvent struct uses 'event_type' (serialized as 'eventType' in JSON). The existing frontend event API (used by useDeviceDiscovery, setup, p2p) uses 'type'. The realtime.ts bridge does a one-line field rename so all legacy callers continue working without changes. Future code should use 'eventType' directly. | Yes | agent |
| D005 | M003-fbgash / S06 | architecture | Which remaining frontend transport calls are allowed to stay on Tauri after transport-boundary closure | All clipboard/settings/storage business data fetch and mutation flows move to daemon HTTP/WS; only shell-native commands stay on Tauri (quick-panel/window control, openDataDirectory, and OS file utility commands such as copy/open/download file entry) | Keeping business transport on the daemon satisfies R003 and makes the boundary auditable, while forcing shell/window/OS integration through the daemon would either duplicate platform responsibilities or violate the thin-shell intent of uc-tauri. This also gives the grep audit a precise allowlist instead of an ambiguous zero-invoke claim. | Yes | agent |
| D006 | M003-fbgash / S06 / T01 | pattern | Route registration order: static paths before parameterized paths in Axum router | Register /entries/clear before /entries/:id to prevent path matching shadowing | Axum routes are evaluated in registration order. A route like /entries/:id matches any path starting with /entries/, including /entries/clear. Static routes must be registered before parameterized routes to take precedence. Calling next() continues to the next handler, so the registration order is the only mechanism to distinguish overlapping paths. Failure manifests as 404s for static routes. | Yes | agent |
| D007 | M003-fbgash / S06 / T01 | pattern | toggleFavorite HTTP method: POST, not PUT | POST /entries/:id/favorite with body {favorited: boolean} | The daemon route for favorite toggle uses POST with a request body ({favorited: true/false}), not PUT with a path parameter. PUT implies idempotent replacement of a resource state, which doesn't match the semantics of a toggle action. The TypeScript client was incorrectly using PUT — corrected to POST. Always read the actual Rust handler's HTTP method before writing the TypeScript client. | Yes | agent |
| D008 | M003-fbgash / S07 planning | architecture | Browser-compatible daemon WebSocket authentication and bootstrap order for direct frontend connections | Daemon `/ws` accepts `Session <jwt>` from either the `Authorization` header or the browser-safe `auth` query parameter, and the frontend must refresh `daemonClient` to a live session before opening `daemonWs` or consuming realtime events. | The current frontend opens `daemonWs` with a browser-style query parameter before any session refresh and parses a stale event shape, while the daemon WebSocket handler still expects header-based auth only. This makes live browser/WebView WebSocket auth and downstream realtime proofs impossible. Normalizing both auth transports through the same JWT/PID/rate-limit path preserves the security model while allowing real browser clients to connect. | Yes | agent |
