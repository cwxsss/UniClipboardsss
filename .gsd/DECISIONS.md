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
