# Progress Log

## Session: 2026-04-27

### Phase 1: AppFacade 统一入口检查记录

- **Status:** complete
- **Started:** 2026-04-27 16:59:47 CST
- Actions taken:
  - 使用 `planning-with-files` 技能。
  - 运行 session catchup,发现上一轮图标处理有未同步上下文,但与本次问题记录无直接关系。
  - 运行 `git diff --stat`,确认当前工作区已有 `src-tauri/icons/` 下图标二进制改动。
  - 创建 `task_plan.md`、`findings.md`、`progress.md`。
  - 将上一轮 AppFacade 检查发现的问题写入 `findings.md`。
- Files created/modified:
  - `task_plan.md`
  - `findings.md`
  - `progress.md`

## Test Results

| Test | Input | Expected | Actual | Status |
|------|-------|----------|--------|--------|
| session catchup | `python3 ~/.codex/skills/planning-with-files/scripts/session-catchup.py "$(pwd)"` | 检查是否有未同步上下文 | 检测到上一轮图标处理上下文,无规划文件更新 | Pass |
| 工作区概况 | `git diff --stat` | 确认当前已有改动 | 显示 `src-tauri/icons/` 下 17 个图标文件已变化 | Pass |
| 规划文件创建 | 新增三份 markdown 文件 | 根目录有持久化记录文件 | 已创建并写入本次问题记录 | Pass |

## Error Log

| Timestamp | Error | Attempt | Resolution |
|-----------|-------|---------|------------|
| 2026-04-27 16:59:47 CST | 根目录没有 `task_plan.md` / `findings.md` / `progress.md` | 1 | 按 `planning-with-files` 模板新建三份文件。 |

## Historical Carryover

- session catchup 提示上一轮有图标处理上下文未同步。
- 当前 `git diff --stat` 显示 `src-tauri/icons/` 下 17 个图标文件有二进制改动。
- 本次任务只记录 AppFacade 绕过问题,未处理图标改动。

## 5-Question Reboot Check

| Question | Answer |
|----------|--------|
| Where am I? | Phase 5 已完成 |
| Where am I going? | 等待后续是否要实际收口 AppFacade 绕过点 |
| What's the goal? | 记录当前仍未统一从 AppFacade 开始的外部业务调用问题 |
| What have I learned? | 详见 `findings.md` |
| What have I done? | 创建三份规划文件并记录问题清单 |

### Phase 2: CLI 业务入口收口实现

- **Status:** complete
- **Started:** 2026-04-27
- Actions taken:
  - 确认用户目标:CLI 业务命令独立运行,直接与 application 层交互。
  - 确认 daemon 只作为 `start` / `stop` / 隐藏 `daemon` 的进程管理目标保留。
  - 读取 CLI 当前命令入口、daemon 侧载 helper、HTTP client 使用点。
  - 将 CLI 会话 helper 从临时命名收口为 `app_session::CliAppSession`。
  - 在 `AppFacade` 增加 CLI 需要的顶层业务方法。
  - 迁移 `init` / `invite` / `join` / `members` / `devices` / `status` / `send` / `watch` / `blob` / `search`。
  - 删除旧 `setup` 命令和 autostop 侧载流程。
  - 移除 `uc-cli` 对 `uc-daemon-client`、`uc-cli-macros`、`dialoguer` 的依赖。
- Files created/modified:
  - `task_plan.md`
  - `findings.md`
  - `progress.md`
  - `src-tauri/crates/uc-application/src/facade/app_facade.rs`
  - `src-tauri/crates/uc-application/src/facade/search/coordinator.rs`
  - `src-tauri/crates/uc-application/src/facade/search/mod.rs`
  - `src-tauri/crates/uc-application/src/usecases/search/search_clipboard_entries.rs`
  - `src-tauri/crates/uc-bootstrap/src/lib.rs`
  - `src-tauri/crates/uc-bootstrap/src/non_gui_runtime.rs`
  - `src-tauri/crates/uc-cli/src/main.rs`
  - `src-tauri/crates/uc-cli/src/commands/`
  - `src-tauri/crates/uc-cli/Cargo.toml`
  - `src-tauri/crates/uc-daemon/src/entrypoint.rs`
  - `src-tauri/crates/uc-tauri/src/bootstrap/runtime.rs`

## Test Results: CLI 业务入口收口

| Test | Expected | Actual | Status |
|------|----------|--------|--------|
| `cargo test -p uc-cli removed -- --nocapture` | 旧 `setup` 与 `--no-wait` 被拒绝 | 2 passed | Pass |
| `cargo test -p uc-cli` | CLI 单测通过 | 2 passed | Pass |
| `cargo check -p uc-cli` | CLI 编译通过 | 通过,无 warning | Pass |
| `cargo check -p uc-tauri` | Tauri runtime 构造仍编译 | 通过 | Pass |
| `cargo check -p uc-daemon` | daemon 构造仍编译 | 通过 | Pass |
| `cargo run -p uc-cli -- --help` | help 中不再出现 `setup` | 不再出现 `setup` | Pass |
| `cargo run -p uc-cli -- search rebuild --help` | 不再出现 `--no-wait` | 不再出现 `--no-wait` | Pass |
| `cargo run -p uc-cli -- --profile codex-check status --json` | `status` 可直连输出应用状态 | 输出 setup/encryption/search 状态 | Pass |

## Remaining Scope

- daemon API 和 daemon worker 仍有直接调用子 facade 的路径,不在本次“先从 CLI 开始”的范围内。

### Phase 3: CLI bin 名称与英文输出

- **Status:** complete
- **Started:** 2026-04-27
- Actions taken:
  - 将 `uc-cli` 的 binary 名称从 `uniclipboard-cli` 改为 `uniclip`。
  - 将 clap 命令名改为 `uniclip`。
  - 将 CLI command 改为可选,无子命令时打印 help 并以成功退出。
  - 将 help 中残留的中文 `blob` / `search` 说明改为英文。
  - 将 CLI 提示中的示例命令从 `uniclipboard-cli` 改为 `uniclip`。
- Files created/modified:
  - `src-tauri/crates/uc-cli/Cargo.toml`
  - `src-tauri/crates/uc-cli/src/main.rs`
  - `src-tauri/crates/uc-cli/src/commands/search.rs`
  - `src-tauri/crates/uc-cli/src/commands/blob.rs`
  - `src-tauri/crates/uc-cli/src/commands/app_session.rs`
  - `src-tauri/crates/uc-cli/src/commands/invite.rs`
  - `src-tauri/crates/uc-cli/src/commands/members.rs`
  - `src-tauri/crates/uc-cli/src/commands/start.rs`
  - `src-tauri/crates/uc-cli/src/commands/send.rs`
  - `src-tauri/crates/uc-cli/src/commands/init.rs`
  - `src-tauri/crates/uc-cli/src/commands/join.rs`
  - `src-tauri/crates/uc-cli/src/commands/watch.rs`

## Test Results: CLI bin 名称与默认 help 行为

| Test | Expected | Actual | Status |
|------|----------|--------|--------|
| `cargo test -p uc-cli -- --nocapture` | 新增 bin 名称和无子命令测试通过 | 4 passed | Pass |
| `cargo check -p uc-cli` | 编译通过 | 通过 | Pass |
| `cargo run -p uc-cli -- --dev --profile abc` | 输出 help,不报错 | exit 0,显示 `Usage: uniclip [OPTIONS] [COMMAND]` | Pass |
| `target/debug/uniclip --dev --profile abc` | 直接运行 binary 输出 help | exit 0,显示 help | Pass |
| `cargo run -p uc-cli -- blob --help` | help 使用英文 | 输出英文 help | Pass |
| `cargo run -p uc-cli -- search query --help` | help 使用英文 | 输出英文 help | Pass |

## Error Log: CLI bin 名称与默认 help 行为

| Timestamp | Error | Attempt | Resolution |
|-----------|-------|---------|------------|
| 2026-04-27 | `cargo test` 一次传了两个 filter,命令用法错误 | 1 | 改跑 `cargo test -p uc-cli -- --nocapture`。 |
| 2026-04-27 | 初版测试使用 `expect_err`,但 `Cli` 未实现 `Debug` 导致测试编译失败 | 1 | 改为显式 `match`。 |
| 2026-04-27 | 新增测试确认旧行为仍存在:命令名是 `uniclipboard-cli`,无子命令是错误 | 1 | 修改 bin/clap 名称并改为无子命令时打印 help。 |
