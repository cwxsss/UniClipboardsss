---
estimated_steps: 4
estimated_files: 1
skills_used: []
---

# T05: Settings API module

Create `src/api/daemon/settings.ts`:

`getSettings()` → GET /settings via DaemonClient.request()
`updateSettings(settings)` → PUT /settings via DaemonClient.request()

Types from uc-core DTOs (SettingsResponse, SettingsUpdateRequest).

## Inputs

- `src-tauri/crates/uc-app/src/dtos/settings.rs (response shapes)`

## Expected Output

- `src/api/daemon/settings.ts`

## Verification

TypeScript compiles with correct response types. Integration test against running daemon.
