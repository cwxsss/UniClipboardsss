# Phase 59: Secure daemon resource endpoints with scoped token auth - Context

**Gathered:** 2026-03-25
**Status:** Ready for planning

<domain>
## Phase Boundary

Migrate blob/thumbnail resource serving from Tauri's in-process `uc://` protocol handler to daemon HTTP endpoints with production-grade security designed for future `0.0.0.0` exposure. GUI retains `uc://` as a transparent proxy layer (via daemon-client), so frontend code requires zero changes.

**In scope:**

- Daemon exposes `GET /uc/blob/:id` and `GET /uc/thumbnail/:id` axum route handlers
- Scoped resource token system: dedicated scope for resource reads, not shared with management bearer token
- Token lifecycle: 15-second TTL, lazy refresh on expired token before request
- Rate limiting: thumbnail 100 req/s per token, blob 20 req/s per token
- Origin check on resource endpoints
- Encryption session binding: resource access requires unlocked encryption session
- Response security headers: `Cache-Control: no-store`, `Content-Disposition` as appropriate
- Audit logging: all plaintext resource reads recorded in audit log
- `uc-daemon-client` gains resource fetch methods (`fetch_blob`, `fetch_thumbnail`) with scoped token cache + refresh
- Tauri `uc://` protocol handler refactored: removes direct use-case calls, proxies through `uc-daemon-client` resource methods
- Update all import paths, no re-export stubs

**Explicitly NOT in scope:**

- Changing frontend resource loading code (`<img src="uc://...">` stays as-is)
- Removing `register_asynchronous_uri_scheme_protocol("uc", ...)` — it stays as the GUI proxy layer
- New content types or resource routes beyond blob/thumbnail
- 0.0.0.0 binding itself (security is designed for it, actual binding is a future phase)

</domain>

<decisions>
## Implementation Decisions

### Architecture approach

- **D-01:** Do NOT extract `parse_uc_request()` or `UcRoute` from uc-tauri to uc-core. Daemon uses axum native path extractors (`/uc/blob/:id`), no shared route enum needed. Tauri keeps its existing `protocol.rs` for the proxy layer's URI parsing.

### Daemon resource endpoints

- **D-02:** Daemon exposes `/uc/blob/:id` and `/uc/thumbnail/:id` as axum route handlers. Handlers call existing use cases (`resolve_blob_resource`, `resolve_thumbnail_resource`) from uc-app via CoreUseCases.

### Security — scoped token auth

- **D-03:** Resource access requires a dedicated scoped token, separate from the management bearer token. Read scope for resources must NOT be conflated with general API read permissions. Token has 15-second TTL.
- **D-04:** Lazy refresh — daemon-client checks token expiry before each resource request; if expired, refreshes first then proceeds. No background timer.
- **D-05:** Daemon issues scoped resource tokens via a token endpoint (e.g., `POST /uc/auth/token`). Management bearer token is required to obtain a resource token.

### Security — rate limiting

- **D-06:** Per-token rate limits: thumbnail 100 req/s, blob 20 req/s. Applied per scoped resource token.

### Security — additional layers

- **D-07:** Origin check on resource endpoints — validate request origin header.
- **D-08:** Encryption session binding — resource endpoints return 403 if encryption session is not unlocked.
- **D-09:** Response headers: `Cache-Control: no-store`, appropriate `Content-Disposition` to prevent unintended browser caching/rendering.

### Security — audit

- **D-10:** All plaintext resource reads (blob and thumbnail) are recorded in audit log with: timestamp, token identity, resource type, resource ID, request origin.

### GUI proxy layer

- **D-11:** Tauri `uc://` protocol handler stays but is refactored: instead of calling use cases directly, it proxies through `uc-daemon-client` resource methods. Frontend `<img src="uc://blob/xxx">` continues working unchanged.
- **D-12:** Tauri backend does NOT use reqwest directly — all daemon communication goes through `uc-daemon-client` crate.
- **D-13:** Tauri side holds a scoped resource token in memory with lazy refresh (15s TTL). Token obtained from daemon on first resource request.

### Migration strategy

- **D-14:** Direct delete + update imports, no re-export stubs (consistent with Phase 58 D-05).

### Claude's Discretion

- Exact axum middleware structure for rate limiting and auth
- Token signing mechanism (HMAC, JWT, or opaque)
- Audit log storage format and location
- Whether to batch-sign URLs or sign per-request in the Tauri proxy
- `Content-Disposition` strategy (attachment vs inline per resource type)
- Error response format for 401/403/429 on resource endpoints

</decisions>

<canonical_refs>

## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Current uc:// protocol implementation

- `src-tauri/crates/uc-tauri/src/protocol.rs` — UcRoute, UcRequestError, parse_uc_request() (stays for Tauri proxy layer)
- `src-tauri/src/main.rs` lines 74-262 — resolve_uc_request(), resolve_uc_blob_request(), resolve_uc_thumbnail_request(), CORS helpers, response builders (refactored to proxy)

### Daemon HTTP server

- `src-tauri/crates/uc-daemon/src/api/` — Existing daemon HTTP routes (pattern reference for new resource routes)
- `src-tauri/crates/uc-daemon/src/api/ws.rs` — WebSocket auth pattern (bearer token check reference)

### daemon-client

- `src-tauri/crates/uc-daemon-client/src/http/` — Existing HTTP client methods (pattern for new resource fetch methods)
- `src-tauri/crates/uc-daemon-client/src/lib.rs` — Crate exports

### Use cases (already exist, no changes needed)

- `src-tauri/crates/uc-app/src/usecases/clipboard/get_entry_resource.rs` — resolve_blob_resource use case
- `src-tauri/crates/uc-app/src/usecases/clipboard/list_entry_projections/list_entry_projections.rs` — EntryProjectionDto with uc:// URL construction

### Auth infrastructure

- `src-tauri/crates/uc-daemon/src/` — Daemon auth token file and bearer token verification (Phase 45)

### Prior phase decisions

- Phase 45 context — daemon auth token architecture
- Phase 58 context — D-05: no re-export stubs migration strategy

</canonical_refs>

<code_context>

## Existing Code Insights

### Reusable Assets

- `resolve_blob_resource` / `resolve_thumbnail_resource` use cases in uc-app — already exist, daemon route handlers just call them
- daemon HTTP auth middleware — bearer token check already implemented, extend for scoped tokens
- `uc-daemon-client` HTTP client infrastructure — reqwest client with base_url and token management

### Established Patterns

- daemon routes use axum with `DaemonApiState` extension for accessing runtime services
- daemon-client methods return typed results with error handling
- Tauri protocol handler uses `tauri::async_runtime::spawn` for async resolution

### Integration Points

- `src-tauri/src/main.rs` lines 423-429 — `register_asynchronous_uri_scheme_protocol("uc", ...)` — refactor resolve logic to daemon-client proxy
- `src-tauri/crates/uc-daemon/src/api/server.rs` — Add new resource routes to axum router
- `src-tauri/crates/uc-daemon-client/src/http/` — Add resource fetch methods with scoped token management
- `src-tauri/crates/uc-tauri/src/bootstrap/run.rs` — May need to initialize scoped token state

</code_context>

<specifics>
## Specific Ideas

- **Security-first design:** User explicitly requires all security measures designed for future `0.0.0.0` exposure, even though current binding is `127.0.0.1`.
- **Scoped token isolation:** Resource read scope must be strictly separated from management API scope. This is a non-negotiable security requirement.
- **Audit trail mandatory:** Every plaintext resource read must be logged. This is for compliance/security audit purposes.
- **15-second token TTL:** Short-lived tokens minimize exposure window. Lazy refresh keeps implementation simple.

</specifics>

<deferred>
## Deferred Ideas

### Reviewed Todos (not folded)

- "修复 setup 配对确认提示缺失" — UI bug, unrelated to resource endpoint security. Belongs in a separate UI fix phase.

### Future considerations

- Actual `0.0.0.0` binding — this phase designs the security for it, actual network exposure is a separate phase
- TLS for daemon HTTP server — needed when binding to `0.0.0.0`
- Cross-device resource access via daemon — depends on 0.0.0.0 + TLS

</deferred>

---

_Phase: 59-secure-daemon-resource-endpoints_
_Context gathered: 2026-03-25_
