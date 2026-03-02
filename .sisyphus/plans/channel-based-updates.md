# Channel-Based Auto-Updates

## TL;DR

> **Summary**: Implement channel-based auto-updates (alpha/beta/rc/stable) so users on any release channel discover updates for their channel. Channel auto-detected from version semver prerelease tag; manual override available in Settings UI. Update manifests hosted on GitHub Pages — no external server needed. Also fixes the existing broken `latest.json` publication in CI.
> **Deliverables**:
>
> - `UpdateChannel` enum in Rust settings model + TypeScript mirror
> - Channel detection utility (semver → channel)
> - Rust Tauri commands for channel-aware update check/install
> - Frontend API wrapper, refactored UpdateContext, channel selector UI
> - CI: `latest.json` assembly + per-channel GitHub Pages deployment
> - Fix existing `latest.json` publication (currently broken)
>   **Effort**: Medium
>   **Parallel**: YES - 3 waves + final verification
>   **Critical Path**: Settings Model → Tauri Commands → Frontend Context → Channel Selector UI

## Context

### Original Request

User installed an alpha version package. The auto-updater should check for the latest alpha channel release, not the latest stable. Currently the updater only checks `/releases/latest/download/latest.json` which excludes prereleases entirely.

### Interview Summary

- **Plan A chosen**: GitHub Pages hosting for per-channel update manifests, no external server
- **Channel strategy**: Auto-detect from version semver prerelease tag + manual override in Settings UI
- **Technical approach**: Tauri v2 `updater_builder().endpoints()` for runtime endpoint override from Rust side (JS `check()` API does NOT support endpoint override)
- **Current state**: CI already supports alpha/beta/rc releases via `release.yml` + `bump-version.js`. But `latest.json` is never published as a release asset (broken).

### Metis Review (gaps addressed)

- **CRITICAL**: `latest.json` never published — `release.yml` `find` command doesn't include `*.json`. Existing stable auto-update is completely broken. Must fix first.
- **Schema migration**: Use `#[serde(default)]` for `update_channel` field, NOT schema version bump
- **Architecture**: No UseCase layer needed for updater — it's a Tauri plugin concern, direct command → plugin interaction is acceptable
- **Channel downgrade**: alpha→stable may require `version_comparator` override since `0.2.0-alpha.1` is semantically before `0.2.0` but after `0.1.0`
- **`PendingUpdate` state**: Store in Tauri managed state (`app.manage()`), NOT inside `AppRuntime`
- **AI-slop guardrails**: No download progress bar, no retry/backoff, no migration framework, no multi-endpoint fallback

## Work Objectives

### Core Objective

Users on any release channel (stable/alpha/beta/rc) receive auto-updates for their channel. Users can manually switch channels in Settings.

### Deliverables

1. `UpdateChannel` enum in `uc-core/src/settings/model.rs` + defaults
2. Channel detection utility (parse semver prerelease → channel)
3. TypeScript mirror of `UpdateChannel` in `src/types/setting.ts`
4. Rust Tauri commands: `check_for_update`, `install_update` in `uc-tauri/src/commands/updater.rs`
5. Frontend API wrapper `src/api/updater.ts`
6. Refactored `UpdateContext.tsx` using Tauri commands
7. Channel selector in `AboutSection.tsx`
8. CI: `latest.json` assembly script + per-channel GitHub Pages deployment
9. Updated `tauri.conf.json` endpoint

### Definition of Done (verifiable conditions with commands)

```bash
# Rust workspace compiles
cd src-tauri && cargo check --workspace

# Rust tests pass (including new settings deserialization tests)
cd src-tauri && cargo test -p uc-core

# Frontend builds
bun run build

# Frontend lint passes
bun run lint

# CI manifest assembly script produces valid JSON (dry-run)
node scripts/assemble-update-manifest.js --test
```

### Must Have

- Auto-detect channel from installed version's semver prerelease tag (e.g., `0.1.0-alpha.1` → Alpha)
- Manual channel override in Settings UI (dropdown)
- Per-channel update manifests on GitHub Pages (`stable.json`, `alpha.json`, `beta.json`, `rc.json`)
- Backward-compatible settings deserialization (`#[serde(default)]`)
- Proper `tracing` spans + `_trace: Option<TraceMetadata>` in new Tauri commands
- `#[serde(rename_all = "camelCase")]` on all payloads emitted to frontend
- `invokeWithTrace` for all frontend → Rust command calls

### Must NOT Have (guardrails)

- ❌ Download progress bar (keep current simple "installing…" state)
- ❌ Retry/backoff logic for update checks
- ❌ Settings migration framework or schema version bump
- ❌ UseCase layer in `uc-app` for updater (not a domain concern)
- ❌ Multi-endpoint fallback logic
- ❌ Update notification system beyond existing dialog
- ❌ `unwrap()`/`expect()` in production Rust code
- ❌ Direct `invoke()` calls — use `invokeWithTrace` wrapper
- ❌ `tauri-plugin-updater` dependency in `uc-core` or `uc-app`

## Verification Strategy

> ZERO HUMAN INTERVENTION — all verification is agent-executed.

- **Test decision**: Tests-after, included in each task
- **Rust**: `cargo test -p uc-core` for settings model + channel detection, `cargo check -p uc-tauri` for commands
- **Frontend**: `bun run build && bun run lint`
- **CI**: Dry-run manifest assembly script with mock artifacts
- **QA policy**: Every task has agent-executed scenarios
- **Evidence**: `.sisyphus/evidence/task-{N}-{slug}.{ext}`

## Execution Strategy

### Parallel Execution Waves

**Wave 1 — Foundation (4 parallel tasks)**

- Task 1: Fix CI `latest.json` publication [quick]
- Task 2: Add `UpdateChannel` to settings model + channel detection utility [quick]
- Task 3: Mirror TypeScript types [quick]
- Task 4: Update `tauri.conf.json` endpoint [quick]

**Wave 2 — Core Implementation (3 parallel tasks)**

- Task 5: Create Tauri update commands [unspecified-high]
- Task 6: CI: Assemble + deploy per-channel update manifests [unspecified-high]
- Task 7: Frontend API wrapper [quick]

**Wave 3 — Frontend Integration (2 tasks)**

- Task 8: Refactor UpdateContext + useUpdate hook [unspecified-high]
- Task 9: Add channel selector to AboutSection [visual-engineering]

### Dependency Matrix

| Task | Blocks | Blocked By |
| ---- | ------ | ---------- |
| 1    | 6      | —          |
| 2    | 5      | —          |
| 3    | 7, 9   | —          |
| 4    | —      | —          |
| 5    | 8      | 2          |
| 6    | —      | 1          |
| 7    | 8, 9   | 3          |
| 8    | 9      | 5, 7       |
| 9    | —      | 3, 7, 8    |

### Agent Dispatch Summary

| Wave  | Tasks | Categories                                 |
| ----- | ----- | ------------------------------------------ |
| 1     | 4     | quick ×4                                   |
| 2     | 3     | unspecified-high ×2, quick ×1              |
| 3     | 2     | unspecified-high ×1, visual-engineering ×1 |
| Final | 4     | oracle, unspecified-high ×2, deep          |

## TODOs

> Implementation + Test = ONE task. Never separate.
> EVERY task MUST have: Agent Profile + Parallelization + QA Scenarios.

<!-- TASKS_INSERT_POINT -->

### Wave 1 — Foundation

- [x] 1. Fix CI `latest.json` Publication

  **What to do**:
  The `create-release` job in `.github/workflows/release.yml` (lines 175-183) uses a `find` command that only collects `.dmg`, `.app.tar.gz`, `.deb`, `.AppImage`, `.msi`, `.exe`, `.sig` files. It does NOT include `*.json`, so the `latest.json` generated by `tauri build` is never uploaded to the GitHub Release. Fix this:
  1. Add `-name "*.json"` to the `find` command in the `prepare-assets` step
  2. Verify the `upload-artifact` path in `build.yml` (line 135: `src-tauri/target/**/release/bundle/**`) captures `latest.json` from the bundle output
  3. If `latest.json` is not in `release/bundle/`, add a separate artifact upload step for it

  **Must NOT do**: Do not modify the `latest.json` content or format. Do not change artifact naming conventions.

  **Recommended Agent Profile**:
  - Category: `quick` — Single-file CI config edit
  - Skills: []

  **Parallelization**: Can Parallel: YES | Wave 1 | Blocks: [6] | Blocked By: []

  **References**:
  - File: `.github/workflows/release.yml:175-183` — the `find` command missing `*.json`
  - File: `.github/workflows/build.yml:131-135` — artifact upload path
  - Tauri docs: `createUpdaterArtifacts: true` generates update bundles and signatures

  **Acceptance Criteria**:
  - [ ] `find` command in `release.yml` includes `-name "*.json"` alongside existing patterns
  - [ ] `grep -c 'json' .github/workflows/release.yml` returns at least 1 match in the prepare-assets step

  **QA Scenarios**:

  ```text
  Scenario: latest.json included in release assets
    Tool: Bash
    Steps: grep -A 20 'prepare-assets' .github/workflows/release.yml | grep -q 'json'
    Expected: Exit code 0
    Evidence: .sisyphus/evidence/task-1-ci-fix.txt
  ```

  **Commit**: YES | Message: `chore: include latest.json in release assets` | Files: [`.github/workflows/release.yml`]

---

- [x] 2. Add `UpdateChannel` to Settings Model + Channel Detection Utility

  **What to do**:
  Add `UpdateChannel` enum and channel detection to `uc-core`. Four sub-parts:

  **Part A — Settings Model** (`src-tauri/crates/uc-core/src/settings/model.rs`):
  1. Add `UpdateChannel` enum with variants: `Stable`, `Alpha`, `Beta`, `Rc`
  2. Use `#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]` and `#[serde(rename_all = "snake_case")]`
  3. Follow the `Theme` enum pattern at `model.rs:19-25`
  4. Add `update_channel: Option<UpdateChannel>` field to `GeneralSettings` with `#[serde(default)]`
  5. `None` = auto-detect from version; `Some(channel)` = user override

  **Part B — Defaults** (`src-tauri/crates/uc-core/src/settings/defaults.rs`):
  1. Impl `Default for UpdateChannel` returning `UpdateChannel::Stable`
  2. Add `update_channel: None` to `GeneralSettings::default()`

  **Part C — Channel Detection** (new file `src-tauri/crates/uc-core/src/settings/channel.rs`):
  1. Create `pub fn detect_channel(version: &str) -> UpdateChannel`
  2. Parse semver prerelease tag: `"0.1.0-alpha.1"` -> `Alpha`, `"0.1.0"` -> `Stable`
  3. Simple string matching: split on `-`, match first segment after `-`
  4. Unknown prerelease tags -> `Stable`, empty string -> `Stable`
  5. Re-export from `src-tauri/crates/uc-core/src/settings/mod.rs`

  **Part D — Tests** (in `model.rs` `#[cfg(test)]` and `channel.rs` `#[cfg(test)]`):
  1. Test `UpdateChannel` serialization roundtrip
  2. Test `GeneralSettings` deserialization with missing `update_channel` defaults to `None`
  3. Test `detect_channel` for: `"0.1.0"` -> Stable, `"0.1.0-alpha.1"` -> Alpha, `"0.1.0-beta.2"` -> Beta, `"0.1.0-rc.1"` -> Rc, `"0.1.0-unknown.1"` -> Stable

  **Must NOT do**: No `semver` crate. No business logic beyond parsing. Do not touch `uc-app` or `uc-tauri`.

  **Recommended Agent Profile**:
  - Category: `quick` — Pure data model + utility function
  - Skills: []

  **Parallelization**: Can Parallel: YES | Wave 1 | Blocks: [5] | Blocked By: []

  **References**:
  - Pattern: `src-tauri/crates/uc-core/src/settings/model.rs:19-25` — `Theme` enum
  - Pattern: `src-tauri/crates/uc-core/src/settings/model.rs:9-17` — `GeneralSettings`
  - Pattern: `src-tauri/crates/uc-core/src/settings/defaults.rs:5-42` — `GeneralSettings::default()`
  - Pattern: `src-tauri/crates/uc-core/src/settings/model.rs:176-219` — existing tests

  **Acceptance Criteria**:
  - [ ] `cargo check -p uc-core` passes (from `src-tauri/`)
  - [ ] `cargo test -p uc-core` passes with all new tests green
  - [ ] `UpdateChannel` enum has 4 variants: `Stable`, `Alpha`, `Beta`, `Rc`
  - [ ] Old `GeneralSettings` JSON without `update_channel` deserializes to `None`

  **QA Scenarios**:

  ```text
  Scenario: Settings backward compatibility
    Tool: Bash
    Steps: cd src-tauri && cargo test -p uc-core -- test_general_settings_defaults_update_channel
    Expected: Test passes
    Evidence: .sisyphus/evidence/task-2-settings-compat.txt

  Scenario: Channel detection correctness
    Tool: Bash
    Steps: cd src-tauri && cargo test -p uc-core -- test_detect_channel
    Expected: All channel detection test cases pass
    Evidence: .sisyphus/evidence/task-2-channel-detect.txt
  ```

  **Commit**: YES | Message: `arch: add UpdateChannel enum and channel detection to uc-core` | Files: [`src-tauri/crates/uc-core/src/settings/model.rs`, `src-tauri/crates/uc-core/src/settings/defaults.rs`, `src-tauri/crates/uc-core/src/settings/channel.rs`, `src-tauri/crates/uc-core/src/settings/mod.rs`]

---

- [x] 3. Mirror TypeScript Types

  **What to do**:
  Update `src/types/setting.ts` to mirror the new Rust types:
  1. Add `UpdateChannel` type: `export type UpdateChannel = 'stable' | 'alpha' | 'beta' | 'rc'`
  2. Add `update_channel?: UpdateChannel | null` to `GeneralSettings` interface (after line 22)
  3. Place the new type AFTER the existing `Theme` type (line 9) for consistency

  **Must NOT do**: Do not change existing type definitions. Do not add runtime validation. Do not create a new file.

  **Recommended Agent Profile**:
  - Category: `quick` — Single-file type addition
  - Skills: []

  **Parallelization**: Can Parallel: YES | Wave 1 | Blocks: [7, 9] | Blocked By: []

  **References**:
  - File: `src/types/setting.ts:9` — `Theme` type
  - File: `src/types/setting.ts:14-22` — `GeneralSettings` interface
  - File: `src-tauri/crates/uc-core/src/settings/model.rs` — Rust source of truth

  **Acceptance Criteria**:
  - [ ] `bun run build` succeeds
  - [ ] `bun run lint` passes
  - [ ] `grep 'UpdateChannel' src/types/setting.ts` returns the new type definition

  **QA Scenarios**:

  ```text
  Scenario: TypeScript types compile
    Tool: Bash
    Steps: bun run build
    Expected: Exit code 0
    Evidence: .sisyphus/evidence/task-3-ts-build.txt
  ```

  **Commit**: YES | Message: `chore: mirror UpdateChannel type in frontend TypeScript` | Files: [`src/types/setting.ts`]

---

- [x] 4. Update `tauri.conf.json` Endpoint

  **What to do**:
  Update the updater plugin endpoint in `src-tauri/tauri.conf.json` (line 53) from the current GitHub Releases latest URL to the future GitHub Pages URL:
  - Old: `https://github.com/UniClipboard/UniClipboard/releases/latest/download/latest.json`
  - New: `https://uniclipboard.github.io/UniClipboard/stable.json`
    This serves as the fallback for stable channel. Rust runtime code will override for other channels.

  **Must NOT do**: Do not remove the `pubkey` field. Do not change `createUpdaterArtifacts`. Do not modify any other config sections.

  **Recommended Agent Profile**:
  - Category: `quick` — Single JSON field edit
  - Skills: []

  **Parallelization**: Can Parallel: YES | Wave 1 | Blocks: [] | Blocked By: []

  **References**:
  - File: `src-tauri/tauri.conf.json:51-57` — current updater plugin config
  - Tauri docs: endpoints are fallback when `updater_builder().endpoints()` is used at runtime

  **Acceptance Criteria**:
  - [ ] `tauri.conf.json` has new GitHub Pages URL in endpoints array
  - [ ] `pubkey` field is unchanged

  **QA Scenarios**:

  ```text
  Scenario: Endpoint URL updated
    Tool: Bash
    Steps: grep 'uniclipboard.github.io' src-tauri/tauri.conf.json
    Expected: Returns the new endpoint URL
    Evidence: .sisyphus/evidence/task-4-endpoint.txt
  ```

  **Commit**: YES | Message: `chore: update tauri.conf.json updater endpoint for GitHub Pages` | Files: [`src-tauri/tauri.conf.json`]

---

### Wave 2 — Core Implementation

- [ ] 5. Create Tauri Update Commands

  **What to do**:
  Create new Tauri commands for channel-aware update checking and installation. This replaces the current JS-side `check()` call with Rust-side `updater_builder().endpoints()` for dynamic channel support.

  **Part A — New command file** (`src-tauri/crates/uc-tauri/src/commands/updater.rs`):
  1. Create `PendingUpdate` struct: `pub struct PendingUpdate(pub Mutex<Option<tauri_plugin_updater::Update>>);`
  2. Create `UpdateMetadata` response struct with `#[serde(rename_all = "camelCase")]`:
     - `version: String`
     - `current_version: String`
     - `body: Option<String>`
     - `date: Option<String>`
  3. Create `check_for_update` command:
     - Signature: `pub async fn check_for_update(app: AppHandle, channel: Option<String>, pending: State<'_, PendingUpdate>, _trace: Option<TraceMetadata>) -> Result<Option<UpdateMetadata>, CmdError>`
     - Read `UpdateChannel` from `channel` param, or detect from `app.package_info().version`
     - Build GitHub Pages endpoint URL: `https://uniclipboard.github.io/UniClipboard/{channel}.json`
     - Use `app.updater_builder().endpoints(vec![url])?.build()?.check().await?`
     - If channel is alpha/beta/rc, use `version_comparator` to allow cross-channel version transitions
     - Store `Update` in `PendingUpdate` state
     - Return `UpdateMetadata` or `None`
  4. Create `install_update` command:
     - Signature: `pub async fn install_update(pending: State<'_, PendingUpdate>, _trace: Option<TraceMetadata>) -> Result<(), CmdError>`
     - Take `Update` from `PendingUpdate` via `.lock().unwrap().take()`
     - Call `update.download_and_install(...)` with simple progress logging
     - No progress callback to frontend (keep simple)

  **Part B — Registration** (`src-tauri/src/main.rs`):
  1. Add `use uc_tauri::commands::updater::PendingUpdate;` import
  2. Add `app.manage(PendingUpdate(Mutex::new(None)));` in setup block (near line 685, after updater plugin init)
  3. Add `uc_tauri::commands::updater::check_for_update` and `uc_tauri::commands::updater::install_update` to `tauri::generate_handler![]`
  4. Add `pub mod updater;` to `src-tauri/crates/uc-tauri/src/commands/mod.rs`

  **Part C — Tracing** (follow project conventions):
  1. Both commands must accept `_trace: Option<TraceMetadata>`
  2. Create `info_span!` with `trace_id` and `trace_ts` fields
  3. Call `record_trace_fields(&span, &_trace)` and `.instrument(span)`
  4. Use structured fields: `info!(channel = %channel, "checking for update")`

  **Must NOT do**: No download progress callback to frontend. No retry logic. No UseCase layer in uc-app. No `unwrap()/expect()` in production paths — use `?` and proper error types. Do not put `PendingUpdate` inside `AppRuntime`.

  **Recommended Agent Profile**:
  - Category: `unspecified-high` — Multi-file Rust implementation with Tauri plugin integration
  - Skills: [] — Standard Rust patterns, no special skills

  **Parallelization**: Can Parallel: YES | Wave 2 | Blocks: [8] | Blocked By: [2]

  **References**:
  - Pattern: `src-tauri/crates/uc-tauri/src/commands/settings.rs` — command structure, State usage, tracing
  - Pattern: `src-tauri/src/main.rs:685` — updater plugin initialization location
  - Pattern: `src-tauri/src/main.rs` — `generate_handler![]` macro for command registration
  - API: `tauri_plugin_updater::UpdaterExt` — `app.updater_builder().endpoints().build().check()`
  - API: `tauri_plugin_updater::Update` — `download_and_install()` method
  - Docs: https://v2.tauri.app/plugin/updater/#runtime-configuration — official channel example
  - Type: `uc_core::settings::model::UpdateChannel` — channel enum (from Task 2)
  - Type: `uc_core::settings::channel::detect_channel` — version parser (from Task 2)

  **Acceptance Criteria**:
  - [ ] `cargo check -p uc-tauri` passes (from `src-tauri/`)
  - [ ] `cargo check --workspace` passes (from `src-tauri/`)
  - [ ] `check_for_update` command compiles with proper signature
  - [ ] `install_update` command compiles with proper signature
  - [ ] Both commands registered in `generate_handler![]`
  - [ ] `PendingUpdate` managed via `app.manage()` in `main.rs`

  **QA Scenarios**:

  ```text
  Scenario: Commands compile and are registered
    Tool: Bash
    Steps: cd src-tauri && cargo check --workspace
    Expected: Exit code 0
    Evidence: .sisyphus/evidence/task-5-rust-check.txt

  Scenario: Tracing pattern followed
    Tool: Bash
    Steps: grep -c 'info_span!' src-tauri/crates/uc-tauri/src/commands/updater.rs
    Expected: At least 2 (one per command)
    Evidence: .sisyphus/evidence/task-5-tracing.txt
  ```

  **Commit**: YES | Message: `impl: add channel-aware update check/install Tauri commands` | Files: [`src-tauri/crates/uc-tauri/src/commands/updater.rs`, `src-tauri/crates/uc-tauri/src/commands/mod.rs`, `src-tauri/src/main.rs`]

---

- [ ] 6. CI: Assemble + Deploy Per-Channel Update Manifests

  **What to do**:
  Create the CI infrastructure to assemble platform-specific `latest.json` files into a combined manifest, and deploy per-channel to GitHub Pages.

  **Part A — Assembly script** (new file `scripts/assemble-update-manifest.js`):
  1. Accept args: `--version <ver> --artifacts-dir <path> --output <path> --base-url <github-release-url>`
  2. Scan artifacts directory for `*.sig` files
  3. For each `.sig` file, determine platform (`darwin-aarch64`, `darwin-x86_64`, `linux-x86_64`, `windows-x86_64`) from the filename
  4. Read `.sig` file content as the `signature` value
  5. Construct the download `url` as `{base-url}/{artifact-filename}` (without `.sig`)
  6. Output combined JSON:

  ```json
  {
    "version": "0.1.0-alpha.1",
    "notes": "",
    "pub_date": "2026-03-01T00:00:00Z",
    "platforms": {
      "darwin-aarch64": {
        "signature": "...",
        "url": "https://github.com/.../download/v0.1.0-alpha.1/app_aarch64.app.tar.gz"
      }
    }
  }
  ```

  7. Support `--test` flag for dry-run validation (output to stdout, use mock data)

  **Part B — Release workflow update** (`.github/workflows/release.yml`):
  1. In `create-release` job, after uploading release assets:
     - Run assembly script with release artifacts
     - Determine channel from `needs.validate.outputs.channel`
     - Write output to `updates/{channel}.json`
  2. Add a deploy step using `peaceiris/actions-gh-pages@v4`:
     - Deploy `updates/` directory to `gh-pages` branch
     - Keep existing files on `gh-pages` (use `keep_files: true`)
     - This overwrites only the current channel's JSON
  3. Ensure `gh-pages` branch is created if it doesn't exist

  **Part C — GitHub Pages setup**:
  1. Document in the plan that repo settings need GitHub Pages enabled on `gh-pages` branch (manual one-time setup)
  2. Add a note in release workflow summary about the Pages URL

  **Must NOT do**: Do not self-host a server. Do not use Cloudflare Workers. Do not change the build workflow. Do not modify release asset naming.

  **Recommended Agent Profile**:
  - Category: `unspecified-high` — New script + CI workflow changes
  - Skills: [] — Standard JS + GitHub Actions

  **Parallelization**: Can Parallel: YES | Wave 2 | Blocks: [] | Blocked By: [1]

  **References**:
  - File: `.github/workflows/release.yml:149-302` — `create-release` job to extend
  - File: `scripts/bump-version.js` — existing script pattern for Node.js scripts
  - Tauri docs: Static JSON file format for updater
  - Action: `peaceiris/actions-gh-pages@v4` — for GitHub Pages deployment

  **Acceptance Criteria**:
  - [ ] `scripts/assemble-update-manifest.js` exists and runs with `--test` flag
  - [ ] `node scripts/assemble-update-manifest.js --test` outputs valid JSON with correct structure
  - [ ] `release.yml` has new steps for manifest assembly and GitHub Pages deployment
  - [ ] Deployment uses `keep_files: true` to preserve other channel JSONs

  **QA Scenarios**:

  ```text
  Scenario: Assembly script produces valid JSON
    Tool: Bash
    Steps: node scripts/assemble-update-manifest.js --test 2>&1 | python3 -m json.tool
    Expected: Valid JSON with version, platforms keys
    Evidence: .sisyphus/evidence/task-6-assembly.txt

  Scenario: Workflow has Pages deployment step
    Tool: Bash
    Steps: grep -c 'gh-pages' .github/workflows/release.yml
    Expected: At least 1 match
    Evidence: .sisyphus/evidence/task-6-workflow.txt
  ```

  **Commit**: YES | Message: `chore: add per-channel update manifest assembly and GitHub Pages deployment` | Files: [`scripts/assemble-update-manifest.js`, `.github/workflows/release.yml`]

---

- [ ] 7. Frontend API Wrapper

  **What to do**:
  Create `src/api/updater.ts` with typed wrappers for the new Tauri update commands.
  1. Import `invokeWithTrace` from `@/lib/tauri-command`
  2. Define `UpdateMetadata` interface matching Rust `UpdateMetadata` (camelCase):
     - `version: string`
     - `currentVersion: string`
     - `body?: string`
     - `date?: string`
  3. Create `checkForUpdate(channel?: UpdateChannel | null): Promise<UpdateMetadata | null>`
     - Calls `invokeWithTrace('check_for_update', { channel })`
  4. Create `installUpdate(): Promise<void>`
     - Calls `invokeWithTrace('install_update', {})`
  5. Export all functions and types

  **Must NOT do**: Do not call raw `invoke()`. Do not import from `@tauri-apps/plugin-updater`. Do not add retry logic. Do not add caching.

  **Recommended Agent Profile**:
  - Category: `quick` — Single new file with typed wrappers
  - Skills: []

  **Parallelization**: Can Parallel: YES | Wave 2 | Blocks: [8, 9] | Blocked By: [3]

  **References**:
  - Pattern: `src/api/security.ts` — existing API wrapper pattern with `invokeWithTrace`
  - Pattern: `src/lib/tauri-command.ts` — `invokeWithTrace` helper
  - Type: `src/types/setting.ts:UpdateChannel` — channel type (from Task 3)

  **Acceptance Criteria**:
  - [ ] `src/api/updater.ts` exists with `checkForUpdate` and `installUpdate` functions
  - [ ] Uses `invokeWithTrace`, NOT raw `invoke()`
  - [ ] `bun run build` succeeds
  - [ ] `bun run lint` passes

  **QA Scenarios**:

  ```text
  Scenario: API wrapper compiles
    Tool: Bash
    Steps: bun run build
    Expected: Exit code 0
    Evidence: .sisyphus/evidence/task-7-build.txt

  Scenario: Uses invokeWithTrace
    Tool: Bash
    Steps: grep -c 'invokeWithTrace' src/api/updater.ts
    Expected: At least 2 (one per function)
    Evidence: .sisyphus/evidence/task-7-invoke.txt
  ```

  **Commit**: YES | Message: `impl: add frontend API wrapper for update commands` | Files: [`src/api/updater.ts`]

---

### Wave 3 — Frontend Integration

- [ ] 8. Refactor UpdateContext + useUpdate Hook

  **What to do**:
  Rewrite `UpdateContext.tsx` to use the new Tauri commands (via API wrapper from Task 7) instead of the JS `@tauri-apps/plugin-updater` `check()` function.

  **Changes to `src/contexts/UpdateContext.tsx`**:
  1. Replace `import { check, type Update } from '@tauri-apps/plugin-updater'` with imports from `@/api/updater`
  2. Replace `Update | null` state type with `UpdateMetadata | null` (from `@/api/updater`)
  3. In `checkForUpdates`:
     - Read current channel preference from settings (`setting.general.update_channel`)
     - If `null` (auto-detect), pass `null` to `checkForUpdate()` — Rust will auto-detect
     - If set, pass the channel string
     - Call `checkForUpdate(channel)` from API wrapper instead of JS `check()`
  4. The `updateInfo` state now holds `UpdateMetadata` instead of `Update`
  5. Remove `updateInfo.downloadAndInstall()` and `updateInfo.close()` calls from consumer code

  **Changes to `src/contexts/update-context.ts`**:
  1. Replace `Update` type import with `UpdateMetadata` from `@/api/updater`
  2. Update `UpdateContextType` interface accordingly
  3. Add `installUpdate: () => Promise<void>` to the context type

  **Changes to `src/components/setting/AboutSection.tsx`**:
  1. Replace `updateInfo.downloadAndInstall()` with `installUpdate()` from context
  2. Remove `updateInfo.close()` call (Rust handles resource cleanup)
  3. Display `updateInfo.version`, `updateInfo.currentVersion`, `updateInfo.body` (same field names, camelCase)

  **Changes to `src/components/layout/Sidebar.tsx`** (if it uses `updateInfo`):
  1. Update to use `UpdateMetadata` type instead of `Update`

  **Must NOT do**: Do not remove `@tauri-apps/plugin-updater` from `package.json` yet (may still be needed for other things). Do not add download progress UI. Do not change the update dialog layout beyond type adaptations.

  **Recommended Agent Profile**:
  - Category: `unspecified-high` — Multi-file frontend refactor
  - Skills: [] — Standard React patterns

  **Parallelization**: Can Parallel: YES | Wave 3 | Blocks: [9] | Blocked By: [5, 7]

  **References**:
  - File: `src/contexts/UpdateContext.tsx` — current implementation to refactor
  - File: `src/contexts/update-context.ts` — context type definition
  - File: `src/hooks/useUpdate.ts` — hook (may need type update)
  - File: `src/components/setting/AboutSection.tsx:63-77` — `handleInstallUpdate` to refactor
  - File: `src/components/layout/Sidebar.tsx` — may reference `Update` type
  - File: `src/api/updater.ts` — new API wrapper (from Task 7)
  - Test: `src/contexts/__tests__/UpdateContext.test.tsx` — existing tests to update
  - Test: `src/components/setting/__tests__/AboutSection.test.tsx` — existing tests to update
  - Test: `src/components/layout/__tests__/SidebarUpdateIndicator.test.tsx` — existing tests to update

  **Acceptance Criteria**:
  - [ ] `bun run build` succeeds
  - [ ] `bun run lint` passes
  - [ ] `bun run test` passes (existing tests updated)
  - [ ] No imports from `@tauri-apps/plugin-updater` in `UpdateContext.tsx`
  - [ ] `checkForUpdates` uses API wrapper, not JS `check()`
  - [ ] `installUpdate` function exposed via context

  **QA Scenarios**:

  ```text
  Scenario: No direct plugin-updater usage in context
    Tool: Bash
    Steps: grep -c '@tauri-apps/plugin-updater' src/contexts/UpdateContext.tsx
    Expected: 0 matches
    Evidence: .sisyphus/evidence/task-8-no-plugin.txt

  Scenario: Frontend tests pass
    Tool: Bash
    Steps: bun run test -- --reporter=verbose 2>&1 | tail -20
    Expected: All tests pass
    Evidence: .sisyphus/evidence/task-8-tests.txt
  ```

  **Commit**: YES | Message: `refactor: migrate UpdateContext from JS plugin to Tauri commands` | Files: [`src/contexts/UpdateContext.tsx`, `src/contexts/update-context.ts`, `src/hooks/useUpdate.ts`, `src/components/setting/AboutSection.tsx`, `src/components/layout/Sidebar.tsx`, `src/contexts/__tests__/UpdateContext.test.tsx`, `src/components/setting/__tests__/AboutSection.test.tsx`, `src/components/layout/__tests__/SidebarUpdateIndicator.test.tsx`]

---

- [ ] 9. Add Channel Selector to AboutSection

  **What to do**:
  Add a channel selector dropdown to the About section in Settings, allowing users to manually switch update channels.

  **Changes to `src/components/setting/AboutSection.tsx`**:
  1. Add new state: `const [updateChannel, setUpdateChannel] = useState<UpdateChannel | null>(null)`
  2. Initialize from `setting.general.update_channel` via `useEffect` (same pattern as `autoCheckUpdate` at lines 30-33)
  3. Add a new settings row between the "auto check update" toggle and the copyright section:
     - Label: `t('settings.sections.about.updateChannel.label')` ("更新渠道" / "Update Channel")
     - Description: `t('settings.sections.about.updateChannel.description')`
     - Control: `<select>` or shadcn `<Select>` with options:
       - `null` / auto: `t('settings.sections.about.updateChannel.auto')` — "Auto-detect (from version)"
       - `stable`: "Stable"
       - `beta`: "Beta"
       - `alpha`: "Alpha"
       - `rc`: "Release Candidate"
  4. On change handler:
     - Call `updateGeneralSetting({ update_channel: value })` (existing settings update flow)
     - After successful update, trigger `checkForUpdates()` to immediately check the new channel
     - Use same error handling pattern as `handleAutoCheckUpdateChange` (lines 35-47)
  5. Show the currently detected channel as hint text when set to "auto":
     - Parse `t('settings.sections.about.version')` or use a version constant to detect channel

  **Changes to i18n** (`src/i18n/locales/en-US.json` and `zh-CN.json` if exists):
  1. Add translation keys under `settings.sections.about.updateChannel`:
     - `label`, `description`, `auto`, `stable`, `beta`, `alpha`, `rc`

  **Must NOT do**: Do not use fixed pixel layouts — use Tailwind utilities. Do not create a separate settings page for channels. Do not add complex channel comparison logic in frontend.

  **Recommended Agent Profile**:
  - Category: `visual-engineering` — UI component with styling
  - Skills: [`ui-ux-pro-max`] — For consistent styling with existing settings UI

  **Parallelization**: Can Parallel: NO | Wave 3 | Blocks: [] | Blocked By: [3, 7, 8]

  **References**:
  - Pattern: `src/components/setting/AboutSection.tsx:119-133` — existing toggle row layout to follow
  - Pattern: `src/components/setting/AboutSection.tsx:35-47` — `handleAutoCheckUpdateChange` error handling pattern
  - File: `src/i18n/locales/en-US.json` — i18n key structure
  - Component: `src/components/ui/` — available UI primitives (Switch, Select)
  - Type: `src/types/setting.ts:UpdateChannel` — channel type (from Task 3)

  **Acceptance Criteria**:
  - [ ] `bun run build` succeeds
  - [ ] `bun run lint` passes
  - [ ] Channel selector renders in the About settings section
  - [ ] Selecting a channel calls `updateGeneralSetting` with correct payload
  - [ ] i18n keys exist for all channel labels

  **QA Scenarios**:

  ```text
  Scenario: Channel selector exists in AboutSection
    Tool: Bash
    Steps: grep -c 'updateChannel\|update_channel' src/components/setting/AboutSection.tsx
    Expected: At least 3 matches (state + handler + render)
    Evidence: .sisyphus/evidence/task-9-selector.txt

  Scenario: i18n keys added
    Tool: Bash
    Steps: grep -c 'updateChannel' src/i18n/locales/en-US.json
    Expected: At least 5 matches (label + description + channel options)
    Evidence: .sisyphus/evidence/task-9-i18n.txt
  ```

  **Commit**: YES | Message: `feat: add update channel selector to settings` | Files: [`src/components/setting/AboutSection.tsx`, `src/i18n/locales/en-US.json`]

## Final Verification Wave (4 parallel agents, ALL must APPROVE)

- [ ] F1. Plan Compliance Audit — oracle
      Verify all 9 tasks completed per spec. Check every Must Have delivered, every Must NOT Have respected. Verify settings backward compatibility, tracing patterns, serde rename conventions.

- [ ] F2. Code Quality Review — unspecified-high
      Run `cargo clippy --workspace` from `src-tauri/`. Run `bun run lint`. Check for `unwrap()`/`expect()` in production Rust code. Verify all new Tauri commands follow existing patterns (trace metadata, info_span, structured fields). Verify no direct `invoke()` in frontend — all use `invokeWithTrace`.

- [ ] F3. Real Manual QA — unspecified-high
      Build the app (`bun tauri build`). Verify update check works with mock endpoint. Verify channel selector UI renders correctly. Verify settings persist after app restart. Test version parsing for all channel variants (`stable`, `alpha.1`, `beta.2`, `rc.1`).

- [ ] F4. Scope Fidelity Check — deep
      Ensure no scope creep: no download progress bar, no retry logic, no migration framework. Verify `uc-core` has no infra dependencies. Verify `PendingUpdate` state is in Tauri managed state, not `AppRuntime`. Verify CI workflow generates valid `latest.json` format.

## Commit Strategy

Follow atomic commit rules from AGENTS.md. Each task is one commit (unless it crosses hex boundaries — split accordingly).

Recommended commit sequence:

```text
chore: fix latest.json publication in release CI
arch: add UpdateChannel enum to uc-core settings model
chore: mirror UpdateChannel type in frontend TypeScript
chore: update tauri.conf.json updater endpoint for GitHub Pages
impl: add channel-aware update check/install Tauri commands
chore: add per-channel update manifest assembly and GitHub Pages deployment
impl: add frontend API wrapper for update commands
refactor: migrate UpdateContext from JS plugin to Tauri commands
feat: add update channel selector to settings
```

## Success Criteria

1. User on `v0.2.0-alpha.1` runs update check → gets `v0.2.0-alpha.2` (not stable)
2. User on `v0.1.0` (stable) runs update check → gets `v0.1.1` (not alpha/beta)
3. User on alpha manually switches to stable in Settings → next check returns latest stable
4. User on stable manually switches to alpha → next check returns latest alpha
5. Old settings files without `update_channel` field load successfully with default (auto-detect)
6. `cargo check --workspace` passes, `bun run build` succeeds, `bun run lint` passes
7. CI release workflow generates valid per-channel `latest.json` and deploys to GitHub Pages
