# S02: Frontend Clipboard API Migration

**Goal:** Migrate all clipboard API calls from Tauri invoke() to daemon HTTP client. Preserve data contracts. Update Redux thunks/RTK Query.
**Demo:** After this: Clipboard list page loads entries via GET /clipboard/entries; restore sends POST; entries update in real-time via WS events

## Tasks
- [x] **T01: Created daemon clipboard API module with typed HTTP client functions for entries, stats, restore, delete, favorite toggle, and resource metadata** — Create `src/api/daemon/clipboard.ts` with all clipboard API functions:

```typescript
export async function getClipboardEntries(
  limit: number = 50,
  offset: number = 0
): Promise<ClipboardEntriesResponse>

export async function getClipboardEntry(id: string): Promise<ClipboardEntryDto>

export async function deleteClipboardEntry(id: string): Promise<void>

export async function restoreClipboardEntry(id: string): Promise<RestoreResult>

export async function toggleFavorite(id: string, favorited: boolean): Promise<void>

export async function getClipboardStats(): Promise<ClipboardStats>

export async function getClipboardEntryResource(id: string): Promise<Blob>
```

All use DaemonClient.request(). Response types must match existing Tauri command return types (EntryProjectionDto, ClipboardStats, etc.) — use same field names.
  - Estimate: medium
  - Files: src/api/daemon/clipboard.ts
  - Verify: TypeScript compiles. All functions return correct types. Integration test against running daemon.
- [x] **T02: All clipboard Redux thunks migrated from Tauri invoke() to daemon HTTP client; TypeScript compiles clean; all 80 store+API tests pass** — Identify all clipboard-related Redux thunks in src/store/ that use Tauri invoke(). Replace each invoke() call with the corresponding daemon API function from src/api/daemon/clipboard.ts.

Priority order:
1. clipboardSlice entries list thunk
2. clipboardSlice delete thunk
3. clipboardSlice restore thunk
4. clipboardSlice favorite toggle thunk
5. clipboardSlice stats thunk
6. Any RTK Query endpoints for clipboard

Keep the old Tauri invoke() calls as commented-out references for rollback during transition period.
  - Estimate: medium
  - Files: src/store/clipboardSlice.ts (or equivalent)
  - Verify: TypeScript compiles. Browser test: all clipboard operations work via daemon HTTP. Redux DevTools shows correct state transitions.
- [ ] **T03: Migration verification and grep audit** — Before completing this slice, run verification:

```bash
# Verify no invoke() calls for clipboard commands remain
rg 'invoke.*get_clipboard' src/
rg 'invoke.*delete_clipboard' src/
rg 'invoke.*restore_clipboard' src/
rg 'invoke.*toggle_favorite' src/
rg 'invoke.*get_clipboard_stats' src/
```

All should return no matches. Then do browser smoke test: list entries, delete one, restore one, toggle favorite, check stats.
  - Estimate: small
  - Verify: All grep commands return zero matches. Browser smoke test passes for all clipboard operations.
