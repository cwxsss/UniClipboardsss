# Captures

### CAP-8a3f1c2d

**Text:** UAT Workflow: Daemon HTTP API (M002-zldd9y/S02 — Settings & Encryption HTTP Handlers)
**Captured:** 2026-03-30T02:21:48.000Z
**Status:** resolved
**Classification:** note
**Resolution:** Informational reference to prior milestone UAT work. No action needed.
**Rationale:** This is a cross-reference to completed M002 work, not actionable in current slice.
**Resolved:** 2026-03-30T02:51:00.000Z

---

### CAP-1b4e9a7f

**Text:** Daemon vault is at `src-tauri/.app_data/vault/`, not `~/Library/Application Support/uniclipboard/`. Relative paths resolve from `config.toml` location (`src-tauri/`).
**Captured:** 2026-03-30T02:21:48.000Z
**Status:** resolved
**Classification:** note
**Resolution:** Dev environment context. Vault path knowledge useful for test setup and debugging.
**Rationale:** Frontend S01 doesn't interact with vault directly; this is backend/testing context.
**Resolved:** 2026-03-30T02:51:00.000Z

### CAP-2c5d8b3e

**Text:** Key files in vault: `.initialized_encryption` (determines initialized: true/false), `keyslot.json` (master key wrapping material)
**Captured:** 2026-03-30T02:21:48.000Z
**Status:** resolved
**Classification:** note
**Resolution:** Background knowledge about encryption internals. Useful context for T06 (Encryption API module) implementation.
**Rationale:** Informational — understanding what backend checks. No frontend action required.
**Resolved:** 2026-03-30T02:51:00.000Z

### CAP-3d6e9c4f

**Text:** Precondition for clean state: Encryption must be `Uninitialized` for daemon to start without keyring recovery. To reset: `rm -f src-tauri/.app_data/vault/.initialized_encryption src-tauri/.app_data/vault/keyslot.json`
**Captured:** 2026-03-30T02:21:48.000Z
**Status:** resolved
**Classification:** note
**Resolution:** Dev workflow knowledge for resetting encryption state during testing. No code action needed.
**Rationale:** Test setup procedure, not a code change. Useful reference for integration testing.
**Resolved:** 2026-03-30T02:51:00.000Z

### CAP-4e7f0d5a

**Text:** Bearer token location: `/tmp/uniclipboard-daemon.token`, format: 64-char hex (HS256 secret for JWT signing)
**Captured:** 2026-03-30T02:21:48.000Z
**Status:** resolved
**Classification:** note
**Resolution:** Token format/location context. Frontend obtains this via Tauri `invoke('daemon_connect_info')`, not direct file read.
**Rationale:** The DaemonClient (T01) abstracts this via Tauri IPC. No direct file access needed.
**Resolved:** 2026-03-30T02:51:00.000Z

### CAP-5f8a1e6b

**Text:** Auth header: `Authorization: Bearer <token>` for `/auth/connect` only. All other endpoints require `Authorization: Session <jwt>` (not Bearer).
**Captured:** 2026-03-30T02:21:48.000Z
**Status:** resolved
**Classification:** note
**Resolution:** Auth header distinction already reflected in T01 plan — `refreshSession()` uses Bearer, `request<T>()` uses Session.
**Rationale:** Critical implementation detail but already captured in the slice plan. Confirms correctness.
**Resolved:** 2026-03-30T02:51:00.000Z

### CAP-6a9b2f7c

**Text:** To initialize encryption (for unlock/lock tests): POST /setup/reset → POST /setup/host → POST /setup/submit-passphrase. After this, GET /encryption/state shows initialized: true, sessionReady: true.
**Captured:** 2026-03-30T02:21:48.000Z
**Status:** resolved
**Classification:** note
**Resolution:** Backend setup flow for encryption initialization. Useful context for T06 integration tests.
**Rationale:** This is a testing precondition, not a frontend code change. Referenced when writing integration tests.
**Resolved:** 2026-03-30T02:51:00.000Z

### CAP-7b0c3a8d

**Text:** Bug found: WS `encryption.session_ready` events silently dropped. Root cause in `src-tauri/crates/uc-daemon/src/api/ws.rs`: `build_snapshot_event()` had no match arm for `ws_topic::ENCRYPTION`. Fix: add `ws_topic::ENCRYPTION => Ok(None)` before `unsupported =>`.
**Captured:** 2026-03-30T02:21:48.000Z
**Status:** resolved
**Classification:** defer
**Resolution:** Deferred to S03 (Frontend WebSocket Direct Connection & Event Migration) where WS integration is actively built.
**Rationale:** One-line fix but irrelevant to S01 (HTTP-only). S03 is when WS events are consumed by the frontend, making it the right context for this fix.
**Resolved:** 2026-03-30T02:51:00.000Z

### CAP-8c1d4b9e

**Text:** Error response format: `{"error":{"code":"<snake_case_code>","message":"<human-readable>"}}`. Known codes: wrong_passphrase (401), not_initialized (400), bad_request (400), internal_error (500), invalid_session_token (401), rate_limit_exceeded (429)
**Captured:** 2026-03-30T02:21:48.000Z
**Status:** resolved
**Classification:** note
**Resolution:** Error response format details that enrich T03 (DaemonApiError) implementation. The snake_case `code` field in response body supplements HTTP status code mapping.
**Rationale:** Valuable implementation detail for T03 but doesn't change the task plan — it refines the existing error mapping approach.
**Resolved:** 2026-03-30T02:51:00.000Z

### CAP-9d2e5c0f

**Text:** Daemon HTTP base: `http://127.0.0.1:42715` (auto-assigned port, from logs). Health check: GET /health (no auth). WS: `ws://127.0.0.1:42715/ws`.
**Captured:** 2026-03-30T02:21:48.000Z
**Status:** resolved
**Classification:** note
**Resolution:** Snapshot of daemon port assignment. Port is auto-assigned so this is session-specific, not a fixed value.
**Rationale:** Confirms DaemonConfig shape (baseUrl, wsUrl) already in T02. No action needed.
**Resolved:** 2026-03-30T02:51:00.000Z

### CAP-a3f4d1e2

**Text:** Template for future UAT: bash script at end of CAPTURES.md documents daemon startup, auth, and HTTP API testing workflow. Reusable for regression testing.
**Captured:** 2026-03-30T02:21:48.000Z
**Status:** resolved
**Classification:** note
**Resolution:** Reference to UAT bash script template. Useful for S05 (Integration Testing & Security Audit).
**Rationale:** Informational pointer to testing resources. No immediate action.
**Resolved:** 2026-03-30T02:51:00.000Z
