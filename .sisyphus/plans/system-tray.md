# System Tray Support (Tauri v2)

## TL;DR

> **Summary**: Add a Rust-based system tray with menu actions, close-to-tray behavior, silent-start support, and tray menu labels synced to the UI language.
> **Deliverables**:
>
> - Tray icon created in Rust (`TrayIconBuilder`) with menu: Open, Settings, Quit
> - Left click shows main window (Windows/macOS; Linux tray click events are unsupported)
> - Right click shows tray context menu (macOS requires disabling left-click menu)
> - CloseRequested hides window (prevents exit); Quit is only from tray menu
> - Startup respects `Settings.general.silent_start` (silent=true => start hidden)
> - Tray menu text follows UI language immediately (`set_tray_language` + frontend sync)
>   **Effort**: Medium
>   **Parallel**: YES - 2 waves
>   **Critical Path**: enable `tray-icon` → implement `TrayState` → wire `main.rs` + `tauri.conf.json` → frontend navigation + language sync → tests → verification

## Context

### Original Request

- 为当前项目支持 tray（系统托盘）。

### Interview Summary

- 实现侧：Rust（Tauri v2 `tauri::tray::TrayIconBuilder`）。
- 行为：关闭窗口隐藏到托盘；退出只通过托盘菜单 Quit。
- 启动：跟随 `Settings.general.silent_start`（静默启动=启动隐藏）。
- 交互：左键托盘图标显示主窗口；右键显示菜单。
- 菜单：包含“打开设置”。
- i18n：托盘菜单语言跟随 UI 语言，且切换“立即生效”。
- 测试策略：tests-after（补最小必要测试 + 跑现有测试）。

### Metis Review (gaps addressed)

- Linux 限制：托盘 click 等事件不触发（只保证右键菜单）。计划中在菜单里保留 Open/Settings，作为 Linux 主要入口。
- API 细节：使用 `TrayIconBuilder::show_menu_on_left_click(false)`（`menu_on_left_click` 在新版本已 deprecated）。
- 生命周期：`TrayIcon` 最后一个实例 drop 会移除托盘图标；必须持有句柄（托盘状态放入 Tauri state）。
- 避免竞态：启动期用同步 `block_on(settings.load())` 得到 `silent_start`，并在 `setup`/后台初始化中一致使用。

## Work Objectives

### Core Objective

Provide production-ready system tray support aligned with existing Tauri v2 patterns and project guardrails.

### Deliverables

- Rust tray icon + menu + event handling
- Close-to-tray behavior (`CloseRequested` prevented)
- Silent start (window starts hidden when enabled)
- “Open Settings” action navigates to `/settings`
- Tray menu text updates when UI language changes
- Tests: Rust unit tests for language normalization/labels; Vitest unit tests for UI navigate listener

### Definition of Done (verifiable)

- From `src-tauri/`:
  - `cargo check --workspace`
  - `cargo test --workspace`
- From repo root:
  - `bun run test`
- Manual smoke (agent-executed tooling): tray menu shows; Open/Settings/Quit behave as required (see QA scenarios).

### Must Have

- No `unwrap()` / `expect()` added in production Rust code paths.
- Use `tracing` for logs (no `println!`/`eprintln!` in newly touched code).
- Tray menu must always include Open and Settings (Linux tray click events unsupported).
- macOS: left click must NOT automatically show menu (so left click can be used for show-window intent).

### Must NOT Have

- Do NOT implement tray via JS `@tauri-apps/api/tray` (explicit decision: Rust tray).
- Do NOT introduce new domain/app-layer business logic for tray behaviors.
- Do NOT add new Rust dependencies for locale detection (language is synced from frontend).
- Do NOT refactor unrelated wiring in `crates/uc-tauri/src/bootstrap/wiring.rs`.

## Verification Strategy

> ZERO HUMAN INTERVENTION — verification steps are agent-executed.

- Test decision: **tests-after**
- Rust: add focused unit tests under `src-tauri/crates/uc-tauri/`.
- Frontend: add Vitest test under `src/**/__tests__/`.
- Evidence outputs:
  - `.sisyphus/evidence/task-*-*.txt` (command outputs)

## Execution Strategy

### Parallel Execution Waves

Wave 1 (foundation + core wiring)

- Enable Rust feature flags + config adjustments
- Implement backend tray state + init logic
- Wire tray + silent start + close-to-tray into `main.rs`

Wave 2 (UI integration + tests)

- Backend command for tray language sync
- Frontend navigation listener + language sync calls
- Add tests and run full verification

### Dependency Matrix (full)

- Task 1 blocks Task 2–5 (tray feature must compile).
- Task 2 blocks Task 3–4 (tray state/init used by wiring + command).
- Task 4 blocks Task 5 (frontend calls command).
- Task 5 blocks its test assertions.

### Agent Dispatch Summary

- Wave 1: 3 tasks (mixed Rust + config)
- Wave 2: 2 tasks (Rust command + frontend integration/tests)

## TODOs

- [ ] 1. Enable `tray-icon` + hide window by default

  **What to do**:
  - Update `src-tauri/Cargo.toml` to enable `tauri` feature `tray-icon`.
    - Current: `tauri = { version = "2", features = ["macos-private-api"] }` at `src-tauri/Cargo.toml:29`.
    - Target: add `"tray-icon"` to that feature list.
  - Update `src-tauri/tauri.conf.json` main window to start hidden:
    - Change `app.windows[0].visible` from `true` to `false` (`src-tauri/tauri.conf.json:25`).
    - Rationale: ensures `silent_start=true` never flashes a window.

  **Must NOT do**:
  - Do not change other window geometry/titlebar flags.

  **Recommended Agent Profile**:
  - Category: `quick` — Reason: small config-only edits
  - Skills: `[]`
  - Omitted: `['test-driven-development']` — Reason: config enabling is straightforward

  **Parallelization**: Can Parallel: YES | Wave 1 | Blocks: 2,3,4,5 | Blocked By: none

  **References**:
  - Config: `src-tauri/Cargo.toml:29`
  - Window visibility: `src-tauri/tauri.conf.json:25`
  - External: https://v2.tauri.app/learn/system-tray/

  **Acceptance Criteria**:
  - [ ] From `src-tauri/`: `cargo check --workspace` succeeds.

  **QA Scenarios**:

  ```text
  Scenario: Rust workspace compiles with tray feature
    Tool: Bash
    Steps:
      1) Run: cd src-tauri && cargo check --workspace
      2) Save output to: .sisyphus/evidence/task-1-cargo-check.txt
    Expected: exit code 0
    Evidence: .sisyphus/evidence/task-1-cargo-check.txt

  Scenario: App window starts hidden
    Tool: Bash
    Steps:
      1) Run: bun run tauri:dev
      2) Observe that no main window flashes before backend shows it (Task 3 will control show)
      3) Save logs to: .sisyphus/evidence/task-1-tauri-dev.log
    Expected: No visible window until explicitly shown by logic
    Evidence: .sisyphus/evidence/task-1-tauri-dev.log
  ```

  **Commit**: NO

- [ ] 2. Implement backend `TrayState` (init + labels + unit tests)

  **What to do**:
  - Add a new module `src-tauri/crates/uc-tauri/src/tray.rs` and export it from `src-tauri/crates/uc-tauri/src/lib.rs`.
  - Implement a `TrayState` managed via Tauri state that:
    - Holds `TrayIcon` handle (to keep it alive)
    - Holds `MenuItem<tauri::Wry>` handles for: Open, Settings, Quit
    - Provides:
      - `init(&self, app: &tauri::AppHandle, initial_language: &str) -> tauri::Result<()>`
      - `set_language(&self, language: &str) -> tauri::Result<()>` (uses `MenuItem::set_text`)
    - Internal structure (decision-complete; avoid judgment calls):
      - Implement `TrayState` as:
        - `#[derive(Default)] pub struct TrayState { inner: std::sync::Mutex<Option<TrayHandles>> }`
        - `struct TrayHandles { tray: tauri::tray::TrayIcon, open: tauri::menu::MenuItem<tauri::Wry>, settings: tauri::menu::MenuItem<tauri::Wry>, quit: tauri::menu::MenuItem<tauri::Wry>, language: String }`
      - `init(...)` is idempotent:
        - If `inner` is already `Some(_)`, return `Ok(())` (do not rebuild tray).
        - Otherwise build tray + menu, then set `inner = Some(TrayHandles { ... })`.
      - `set_language(...)` behavior:
        - If `inner` is `None`, return `Ok(())` and log at `debug`.
        - If `inner` is `Some(handles)`, update `handles.language` and call `handles.open/settings/quit.set_text(...)`.
    - Uses menu item IDs:
      - `tray.open`, `tray.settings`, `tray.quit`
    - Uses tray id `uc-tray`
  - Tray icon appearance (decision-complete):
    - In `init`, attempt to use `app.default_window_icon()` for the tray icon.
    - If `default_window_icon` is `None`, log a `warn!` and continue building the tray icon without setting an explicit icon (do not crash the app).
    - Set tooltip to `"UniClipboard"` (Linux may ignore tooltip; safe).
  - Label mapping (decision-complete):
    - Normalize language exactly like frontend: `zh*` => `zh-CN`, else `en-US`.
    - For `zh-CN`:
      - Open: `打开 UniClipboard`
      - Settings: `设置`
      - Quit: `退出`
    - For `en-US`:
      - Open: `Open UniClipboard`
      - Settings: `Settings`
      - Quit: `Quit`
  - Add Rust unit tests in the same module (`#[cfg(test)]`) verifying:
    - language normalization matches `src/i18n/index.ts:11`
    - labels for both `zh-CN` and `en-US`
    - calling `set_language` before `init` is a safe no-op (returns Ok, logs at debug)

  **Must NOT do**:
  - Do not call `TrayIcon::set_menu` repeatedly on Linux (menu replacement is restricted); update via `MenuItem::set_text` only.
  - Do not add new dependencies for locale detection.

  **Recommended Agent Profile**:
  - Category: `unspecified-high` — Reason: cross-platform tray API + state design
  - Skills: `['systematic-debugging']` — Reason: avoid subtle platform/runtime issues
  - Omitted: `['ui-ux-pro-max']` — Reason: no UI layout work

  **Parallelization**: Can Parallel: YES | Wave 1 | Blocks: 3,4 | Blocked By: 1

  **References**:
  - Tauri APIs:
    - https://docs.rs/tauri/latest/tauri/tray/struct.TrayIconBuilder.html
    - https://docs.rs/tauri/latest/tauri/tray/struct.TrayIcon.html
    - https://docs.rs/tauri/latest/tauri/menu/struct.MenuItem.html
  - Frontend language normalization: `src/i18n/index.ts:11`

  **Acceptance Criteria**:
  - [ ] From `src-tauri/`: `cargo test -p uc-tauri` succeeds.

  **QA Scenarios**:

  ```text
  Scenario: Tray label normalization tests pass
    Tool: Bash
    Steps:
      1) Run: cd src-tauri && cargo test -p uc-tauri tray
      2) Save output to: .sisyphus/evidence/task-2-uc-tauri-tests.txt
    Expected: exit code 0
    Evidence: .sisyphus/evidence/task-2-uc-tauri-tests.txt

  Scenario: Linux limitation documented by design
    Tool: Bash
    Steps:
      1) Confirm code does NOT depend on tray click events for Open/Settings actions
      2) Save notes to: .sisyphus/evidence/task-2-linux-note.txt
    Expected: Menu contains Open + Settings items
    Evidence: .sisyphus/evidence/task-2-linux-note.txt
  ```

  **Commit**: NO

- [ ] 3. Wire tray + silent start + close-to-tray in `src-tauri/src/main.rs`

  **What to do**:
  - Manage `TrayState` on the Tauri builder (same pattern as runtime):
    - Add `.manage(TrayState::default())` near existing `.manage(...)` calls at `src-tauri/src/main.rs:572`.
  - Add global `Builder::on_window_event` handler to implement close-to-tray:
    - Match `tauri::WindowEvent::CloseRequested { api, .. }`.
    - Only apply to window label `"main"`.
    - Call `api.prevent_close()` and then `window.hide()`.
    - Use `tracing::{info, warn}` for observability.
    - Reference Tauri docs: `CloseRequestApi::prevent_close`.
  - In `setup` block (`src-tauri/src/main.rs:626`):
    - Load startup settings once (synchronously) using `tauri::async_runtime::block_on(runtime_for_handler.deps.settings.load())`.
      - If settings load fails, log a `warn!` and apply defaults:
        - `silent_start = false`
        - `initial_language = "en-US"`
    - Compute `silent_start` from `settings.general.silent_start`.
    - Compute `initial_language` as:
      - `settings.general.language.as_deref().unwrap_or("en-US")`
    - Initialize tray:
      - Get `TrayState` via `app.state::<TrayState>()` and call `init(app.handle(), initial_language)`.
      - If tray initialization fails, log `error!` and continue startup (tray is best-effort; must not crash app on edge cases).
      - Configure tray builder to show menu on right click only:
        - Use `TrayIconBuilder::show_menu_on_left_click(false)` (Linux unsupported; ok).
      - Implement menu events:
        - `tray.open` => show+focus+unminimize main window
        - `tray.settings` => show window + `app.emit("ui://navigate", "/settings")`
        - `tray.quit` => `app.exit(0)`
      - Implement tray icon events:
        - On left click up: show+focus+unminimize main window
        - On right click: no-op (menu will appear)
      - IMPORTANT: Linux tray click events are not emitted; Open/Settings must be reachable via menu.
    - Silent start window behavior:
      - If `silent_start` is `true`: do NOT show the main window in setup.
      - If `silent_start` is `false`: show+focus+unminimize the main window in setup.
    - Backend init completion (`src-tauri/src/main.rs:687`):
      - Only call `startup_barrier.try_finish(&app_handle_for_startup)` when `silent_start` is false.

  **Must NOT do**:
  - Do not add new logic to `crates/uc-tauri/src/bootstrap/wiring.rs`.
  - Do not use `unwrap()` when reading icons or windows.

  **Recommended Agent Profile**:
  - Category: `unspecified-high` — Reason: touches main entry wiring + cross-platform behavior
  - Skills: `['systematic-debugging']` — Reason: avoid startup races and close-event pitfalls
  - Omitted: `['test-driven-development']` — Reason: this is wiring-heavy; we’ll add targeted tests in other tasks

  **Parallelization**: Can Parallel: NO | Wave 1 | Blocks: 4,5 | Blocked By: 1,2

  **References**:
  - Builder setup site: `src-tauri/src/main.rs:626`
  - Existing show/focus/unminimize pattern: `src-tauri/crates/uc-tauri/src/commands/startup.rs:52`
  - Startup barrier callsite: `src-tauri/src/main.rs:687`
  - CloseRequested API: https://docs.rs/tauri/latest/tauri/struct.CloseRequestApi.html
  - Window label: `src-tauri/tauri.conf.json:16`

  **Acceptance Criteria**:
  - [ ] From `src-tauri/`: `cargo check --workspace` succeeds.

  **QA Scenarios**:

  ```text
  Scenario: Close button hides to tray
    Tool: Bash
    Steps:
      1) Run: bun run tauri:dev
      2) Trigger window close (TitleBar close on Windows or standard close)
      3) Verify process stays alive and tray icon remains
      4) Save notes/logs to: .sisyphus/evidence/task-3-close-to-tray.txt
    Expected: Window hides, app remains running
    Evidence: .sisyphus/evidence/task-3-close-to-tray.txt

  Scenario: Silent start keeps window hidden
    Tool: Bash
    Steps:
      1) Set `silent_start=true` via UI settings
      2) Restart app
      3) Verify no main window appears; tray icon is present
      4) Use tray menu Open to show window
      5) Save notes to: .sisyphus/evidence/task-3-silent-start.txt
    Expected: Startup hidden; Open shows main window
    Evidence: .sisyphus/evidence/task-3-silent-start.txt
  ```

  **Commit**: NO

- [ ] 4. Add backend command `set_tray_language` (sync tray menu with UI)

  **What to do**:
  - Add new command module `src-tauri/crates/uc-tauri/src/commands/tray.rs` and export from `src-tauri/crates/uc-tauri/src/commands/mod.rs`.
  - Implement Tauri command:
    - Name: `set_tray_language`
    - Signature: `set_tray_language(tray: State<'_, TrayState>, language: String, _trace: Option<TraceMetadata>) -> Result<(), String>`
    - Behavior:
      - Normalize `language` with the same rule as frontend (`zh*` => zh-CN else en-US).
      - Call `tray.set_language(normalized)`.
      - If tray is not initialized yet, return `Ok(())` and log at `debug`.
    - Must include trace span + `record_trace_fields` (follow `src-tauri/crates/uc-tauri/src/commands/settings.rs:24`).
  - Register the command in `src-tauri/src/main.rs` `invoke_handler![]` list near other settings commands (`src-tauri/src/main.rs:741`).

  **Must NOT do**:
  - Do not persist settings here; this is a UI sync command only.

  **Recommended Agent Profile**:
  - Category: `quick` — Reason: small command addition + registration
  - Skills: `[]`
  - Omitted: `['systematic-debugging']` — Reason: low risk

  **Parallelization**: Can Parallel: YES | Wave 2 | Blocks: 5 | Blocked By: 2,3

  **References**:
  - Trace pattern: `src-tauri/crates/uc-tauri/src/commands/settings.rs:74`
  - Frontend normalization: `src/i18n/index.ts:11`
  - Menu item update API: https://docs.rs/tauri/latest/tauri/menu/struct.MenuItem.html#method.set_text

  **Acceptance Criteria**:
  - [ ] From `src-tauri/`: `cargo check -p uc-tauri` succeeds.

  **QA Scenarios**:

  ```text
  Scenario: set_tray_language command does not crash
    Tool: Bash
    Steps:
      1) Run: cd src-tauri && cargo test -p uc-tauri
      2) Save output to: .sisyphus/evidence/task-4-uc-tauri-tests.txt
    Expected: exit code 0
    Evidence: .sisyphus/evidence/task-4-uc-tauri-tests.txt

  Scenario: Language switch updates tray menu labels
    Tool: Bash
    Steps:
      1) Run app in dev
      2) Toggle language in Settings (en-US <-> zh-CN)
      3) Open tray menu and verify labels update immediately
      4) Save notes to: .sisyphus/evidence/task-4-tray-labels.txt
    Expected: Tray menu reflects UI language
    Evidence: .sisyphus/evidence/task-4-tray-labels.txt
  ```

  **Commit**: NO

- [ ] 5. Frontend: listen for `ui://navigate` + sync tray language (with tests)

  **What to do**:
  - Add a listener in `src/App.tsx` to handle backend navigation requests:
    - Listen to `ui://navigate` (payload string).
    - Whitelist allowed routes: at minimum `"/settings"`.
    - Call `navigate(route)` via `useNavigate`.
  - In `src/contexts/SettingContext.tsx` language effect (`src/contexts/SettingContext.tsx:186`):
    - After computing `next` language, call `invokeWithTrace('set_tray_language', { language: next })`.
    - Handle errors with `.catch` and observable `console.error` (follow existing patterns).
  - Tests (Vitest):
    - Create a small hook `src/hooks/useUINavigateListener.ts` used by `src/App.tsx` (to make testing easy).
    - Add `src/hooks/__tests__/useUINavigateListener.test.tsx`:
      - Mock `@tauri-apps/api/event` `listen`.
      - Verify it subscribes to `ui://navigate` and calls the provided `onNavigate` callback for whitelisted `/settings`.

  **Must NOT do**:
  - Do not introduce direct `invoke()` calls; use `invokeWithTrace`.
  - Do not add fixed-pixel layout changes.

  **Recommended Agent Profile**:
  - Category: `quick` — Reason: small React wiring + unit test
  - Skills: `['vercel-react-best-practices']` — Reason: clean effect wiring and dependency correctness
  - Omitted: `['ui-ux-pro-max']` — Reason: no design work

  **Parallelization**: Can Parallel: NO | Wave 2 | Blocks: Final verification | Blocked By: 4

  **References**:
  - Settings route exists: `src/App.tsx:127`
  - Language effect location: `src/contexts/SettingContext.tsx:186`
  - Event listen permission is enabled: `src-tauri/capabilities/default.json:15`

  **Acceptance Criteria**:
  - [ ] From repo root: `bun run test` succeeds.

  **QA Scenarios**:

  ```text
  Scenario: Open Settings from tray navigates to /settings
    Tool: Bash
    Steps:
      1) Run app in dev
      2) Use tray menu "Settings" item
      3) Verify Settings page opens (route /settings)
      4) Save notes/screens to: .sisyphus/evidence/task-5-open-settings.txt
    Expected: UI navigates to Settings page
    Evidence: .sisyphus/evidence/task-5-open-settings.txt

  Scenario: ui://navigate ignores non-whitelisted routes
    Tool: Bash
    Steps:
      1) Run bun test focusing the new hook test
      2) Save output to: .sisyphus/evidence/task-5-vitest.txt
    Expected: only /settings triggers navigate
    Evidence: .sisyphus/evidence/task-5-vitest.txt
  ```

  **Commit**: NO

## Final Verification Wave (4 parallel agents, ALL must APPROVE)

- [ ] F1. Plan Compliance Audit — oracle
- [ ] F2. Code Quality Review — unspecified-high
- [ ] F3. Real Manual QA — unspecified-high (tray + silent start + language switch)
- [ ] F4. Scope Fidelity Check — deep

## Commit Strategy

- Single atomic commit at the end:
  - Message: `feat: support system tray`
  - Includes: Rust tray + close-to-tray + silent start + settings navigation + language sync + tests

## Success Criteria

- Tray icon present on all desktop platforms.
- CloseRequested hides the main window; app remains running.
- Silent start starts without showing main window.
- Tray menu contains Open/Settings/Quit and works cross-platform.
- Tray menu labels match UI language and update immediately on language toggle.
- `cd src-tauri && cargo test --workspace` and `bun run test` both pass.
