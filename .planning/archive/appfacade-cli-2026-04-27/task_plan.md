# Task Plan: AppFacade 统一入口检查记录

## Goal

记录当前仓库中仍未统一从 `src-tauri/crates/uc-application/src/facade/app_facade.rs` 发起外部业务调用的问题,供后续收口改造使用。

## Current Phase

Phase 5

## Phases

### Phase 1: 需求确认

- [x] 明确用户要求:记录上一轮检查中发现的 AppFacade 绕过点。
- [x] 确认本次只写计划记录,不改业务代码。
- **Status:** complete

### Phase 2: 已知问题整理

- [x] 汇总 `AppFacade` 公开字段导致的统一入口不完整问题。
- [x] 汇总 daemon 入口和后台 worker 中直接调用子 facade / usecase 的问题。
- [x] 汇总 CLI 直连模式中直接调用 `SpaceSetupAssembly` 子入口的问题。
- **Status:** complete

### Phase 3: 写入规划文件

- [x] 新建 `task_plan.md`。
- [x] 新建 `findings.md`。
- [x] 新建 `progress.md`。
- **Status:** complete

### Phase 4: 验证

- [x] 运行 `git diff --stat` 确认工作区已有图标改动,本次新增规划文件。
- [x] 后续运行文件检查确认记录文件存在且内容可读。
- **Status:** complete

### Phase 5: 交付

- [x] 向用户说明记录已完成。
- **Status:** complete

## Key Questions

1. 当前是否已满足“所有外部业务调用都从 AppFacade 开始”?
   答:没有。daemon 和 CLI 仍有多处直接调用子 facade / usecase / assembly 的路径。
2. 这次是否需要修代码?
   答:不需要。用户要求是记录问题。

## Decisions Made

| Decision | Rationale |
|----------|-----------|
| 本次只记录,不修复 | 用户明确要求“记录下上方看到的问题”。 |
| 将问题写入项目根目录三份规划文件 | 符合 `planning-with-files` 的持久化工作记忆要求。 |

## Errors Encountered

| Error | Attempt | Resolution |
|-------|---------|------------|
| 项目根目录不存在 `task_plan.md` / `findings.md` / `progress.md` | 1 | 按技能模板新建三份文件。 |

## Notes

- 上一轮检查结论:统一入口方向已经开始做,但 daemon 和 CLI 还没有完全收口。
- 这份记录不代表修复完成,只是为后续改造保留问题清单。

## Implementation Session: CLI 业务入口收口

### Goal

将 CLI 业务命令改为独立直连 application 层,并通过 `AppFacade` 顶层方法进入;daemon 只保留为后台进程管理能力。

### Scope

- 业务命令: `init` / `invite` / `join` / `members` / `devices` / `space-status` / `status` / `send` / `watch` / `blob` / `search`。
- 进程命令: `start` / `stop` / 隐藏 `daemon` 仍允许使用 daemon。
- 删除旧 `setup` 命令。

### Status

- [x] 已确认当前 CLI 是混合状态,不是所有命令都会侧载 daemon。
- [x] 补测试。
- [x] 新增 AppFacade 顶层 CLI 入口。
- [x] 迁移 CLI 命令。
- [x] 删除旧 setup/daemon HTTP 业务路径。
- [x] cargo check/test 验证。

### Result

- `Slice1Cli` 临时命名已收口为 `CliAppSession`,模块名改为 `app_session`。
- CLI 业务命令已不再依赖 daemon HTTP API。
- `setup` 命令已删除,`search rebuild --no-wait` 已删除。
- `status` 现在显示应用状态,不再显示 daemon worker 状态。
- 保留 daemon 相关命令:`start` / `stop` / 隐藏 `daemon`。

### Verification

- `cargo test -p uc-cli removed -- --nocapture`
- `cargo test -p uc-cli`
- `cargo check -p uc-cli`
- `cargo check -p uc-tauri`
- `cargo check -p uc-daemon`
- `cargo run -p uc-cli -- --help`
- `cargo run -p uc-cli -- search rebuild --help`
- `cargo run -p uc-cli -- --profile codex-check status --json`

## Follow-up: CLI bin 名称与默认 help 行为

### Goal

- 编译出的 CLI binary 名称为 `uniclip`。
- 直接运行 `uniclip` 或 `cargo run -p uc-cli -- --dev --profile abc` 时输出 help,不报缺少子命令错误。
- CLI 终端可见输出统一使用英文。

### Status

- [x] 将 `uc-cli` bin 名称改为 `uniclip`。
- [x] 无子命令时主动打印 help 并返回成功。
- [x] 将 CLI help 中残留的中文说明改为英文。
- [x] 将 CLI 提示中的 `uniclipboard-cli` 示例改为 `uniclip`。
- [x] 测试和手动命令验证。

### Verification

- `cargo test -p uc-cli -- --nocapture`
- `cargo check -p uc-cli`
- `cargo run -p uc-cli -- --dev --profile abc`
- `target/debug/uniclip --dev --profile abc`
- `cargo run -p uc-cli -- blob --help`
- `cargo run -p uc-cli -- search query --help`
