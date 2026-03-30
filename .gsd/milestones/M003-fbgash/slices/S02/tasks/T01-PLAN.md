---
estimated_steps: 14
estimated_files: 1
skills_used: []
---

# T01: Daemon clipboard API module

Create `src/api/daemon/clipboard.ts` with all clipboard API functions:

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

## Inputs

- `src/api/clipboard.ts (current Tauri API patterns)`
- `src-tauri/crates/uc-app/src/dtos/clipboard.rs (response shapes)`

## Expected Output

- `src/api/daemon/clipboard.ts`

## Verification

TypeScript compiles. All functions return correct types. Integration test against running daemon.
