# Findings & Decisions

## Requirements

- 用户要求记录上一轮检查中发现的问题:当前仓库是否还存在没有从 `src-tauri/crates/uc-application/src/facade/app_facade.rs` 统一开始的外部业务调用。
- 本次只做记录,不修改业务代码。

## Research Findings

### 总结

当前仓库还没有做到“所有外部业务调用都统一从 AppFacade 开始”。主要残留集中在 daemon 和 CLI 直连模式。

### 已确认的问题

1. `AppFacade` 仍公开子入口字段
   - 位置: `src-tauri/crates/uc-application/src/facade/app_facade.rs:44`
   - 现象: `space_setup`、`member_roster`、`lifecycle`、`encryption`、`resource`、`clipboard_history`、`clipboard_restore`、`search`、`settings`、`device`、`storage` 都是公开字段。
   - 影响:外部代码可以拿到 `AppFacade` 后直接调用子 facade,而不是通过 `app_facade.rs` 中定义的统一方法。

2. daemon 启动入口直接构造并传递多个子 facade / usecase
   - 位置: `src-tauri/crates/uc-daemon/src/entrypoint.rs:145`
   - 位置: `src-tauri/crates/uc-daemon/src/entrypoint.rs:166`
   - 位置: `src-tauri/crates/uc-daemon/src/entrypoint.rs:167`
   - 位置: `src-tauri/crates/uc-daemon/src/entrypoint.rs:168`
   - 位置: `src-tauri/crates/uc-daemon/src/entrypoint.rs:177`
   - 现象:daemon 入口直接构造 `CaptureClipboardUseCase`、`InboundClipboardFacade`、`ClipboardCaptureFacade`、`ClipboardLiveIndexFacade`、`ClipboardOutboundFacade`。
   - 影响:这些长期运行的业务路径没有从 `AppFacade` 方法开始。

3. daemon 剪贴板监听 worker 直接调用多个子 facade
   - 位置: `src-tauri/crates/uc-daemon/src/workers/clipboard_watcher.rs:67`
   - 位置: `src-tauri/crates/uc-daemon/src/workers/clipboard_watcher.rs:154`
   - 位置: `src-tauri/crates/uc-daemon/src/workers/clipboard_watcher.rs:191`
   - 位置: `src-tauri/crates/uc-daemon/src/workers/clipboard_watcher.rs:215`
   - 现象:worker 持有并调用 `ClipboardCaptureFacade`、`ClipboardLiveIndexFacade`、`ClipboardOutboundFacade`。
   - 影响:剪贴板捕获、索引、出站同步的业务编排分散在 daemon worker 内。

4. daemon 入站同步 worker 直接调用子 facade
   - 位置: `src-tauri/crates/uc-daemon/src/workers/inbound_clipboard_sync.rs:63`
   - 位置: `src-tauri/crates/uc-daemon/src/workers/inbound_clipboard_sync.rs:88`
   - 位置: `src-tauri/crates/uc-daemon/src/workers/inbound_clipboard_sync.rs:154`
   - 现象:worker 持有并调用 `ClipboardSyncFacade`、`InboundClipboardFacade`。
   - 影响:入站同步没有通过 `AppFacade` 方法统一进入。

5. daemon 后台解锁流程直接拿 `SpaceSetupFacade`
   - 位置: `src-tauri/crates/uc-daemon/src/entrypoint.rs:430`
   - 位置: `src-tauri/crates/uc-daemon/src/entrypoint.rs:472`
   - 现象:后台任务直接调用 `try_resume_session()` 和 `refresh_presence()`。
   - 影响:启动后的空间恢复和 presence 预热没有通过 `AppFacade` 方法统一进入。

6. CLI 直连模式直接使用 `SpaceSetupAssembly` 的子入口
   - `src-tauri/crates/uc-cli/src/commands/init.rs:70`
   - `src-tauri/crates/uc-cli/src/commands/invite.rs:46`
   - `src-tauri/crates/uc-cli/src/commands/invite.rs:87`
   - `src-tauri/crates/uc-cli/src/commands/join.rs:123`
   - `src-tauri/crates/uc-cli/src/commands/members.rs:46`
   - `src-tauri/crates/uc-cli/src/commands/members.rs:86`
   - `src-tauri/crates/uc-cli/src/commands/members.rs:109`
   - `src-tauri/crates/uc-cli/src/commands/send.rs:69`
   - `src-tauri/crates/uc-cli/src/commands/send.rs:106`
   - `src-tauri/crates/uc-cli/src/commands/send.rs:140`
   - `src-tauri/crates/uc-cli/src/commands/watch.rs:42`
   - `src-tauri/crates/uc-cli/src/commands/watch.rs:78`
   - `src-tauri/crates/uc-cli/src/commands/watch.rs:95`
   - `src-tauri/crates/uc-cli/src/commands/blob.rs:83`
   - `src-tauri/crates/uc-cli/src/commands/blob.rs:174`
   - 影响:CLI 的 init / invite / join / members / send / watch / blob 等直连路径还没有统一走 `AppFacade`。

7. `uc-application` 仍公开导出一些 usecase / 内部模块
   - 位置: `src-tauri/crates/uc-application/src/lib.rs:16`
   - 位置: `src-tauri/crates/uc-application/src/lib.rs:22`
   - 位置: `src-tauri/crates/uc-application/src/lib.rs:32`
   - 现象:外部 crate 仍可引用部分 usecase 或内部业务模块。
   - 影响:这会继续允许绕过 facade。

### 相对干净的部分

- Tauri GUI command 层基本没有直接业务调用。
- `uc-tauri` 的运行时已经持有 `Arc<AppFacade>` 并提供 `runtime.app_facade()` 入口。
- GUI 的主要业务流目前更多通过 daemon API 间接完成,直接绕过点主要不在 Tauri command 层。

## Technical Decisions

| Decision | Rationale |
|----------|-----------|
| 后续收口应优先处理 daemon 长期运行路径 | daemon worker 是实际业务流最集中的地方,影响最大。 |
| CLI 直连模式需要单独计划 | CLI 依赖 `SpaceSetupAssembly` 的历史路径较多,一次性改动风险较高。 |
| `AppFacade` 公开字段需要收敛 | 只要公开字段存在,就无法真正保证统一入口。 |

## Issues Encountered

| Issue | Resolution |
|-------|------------|
| 根目录没有规划文件 | 新建 `task_plan.md`、`findings.md`、`progress.md`。 |
| session catchup 检测到上一轮图标处理上下文未同步 | 在 `progress.md` 中记录为历史备注,本次不处理该图标改动。 |

## Resources

- `src-tauri/crates/uc-application/src/facade/app_facade.rs`
- `src-tauri/crates/uc-application/src/lib.rs`
- `src-tauri/crates/uc-daemon/src/entrypoint.rs`
- `src-tauri/crates/uc-daemon/src/workers/clipboard_watcher.rs`
- `src-tauri/crates/uc-daemon/src/workers/inbound_clipboard_sync.rs`
- `src-tauri/crates/uc-cli/src/commands/`
- `src-tauri/crates/uc-tauri/src/bootstrap/runtime.rs`

## Visual/Browser Findings

- 本次没有浏览器或图像检查。

## 2026-04-27 CLI 重构决策

- CLI 业务命令应作为独立程序运行,不依赖 daemon HTTP API。
- `start` / `stop` / 隐藏 `daemon` 属于进程管理,可以继续使用 daemon。
- `status` 改为应用状态,不再表示 daemon worker 状态。
- 旧 `setup` 命令删除;使用 `init` / `invite` / `join` 作为唯一 setup/pairing 入口。
- `search rebuild` 在 CLI 进程内同步执行;不保留 daemon 后台 `--no-wait` 语义。
- 实施前实际状态:
  - 已直连但绕过 AppFacade 顶层: `init` / `invite` / `join` / `members` / `send` / `watch` / `blob`。
  - 仍走 daemon HTTP: `setup` / `devices` / `status` / `search`。
  - 只应保留 daemon 的命令: `start` / `stop` / 隐藏 `daemon`。

## 2026-04-27 CLI 收口结果

- `Slice1Cli` 临时命名已替换为 `CliAppSession`,模块名改为 `app_session`。
- CLI 业务命令已迁移为直连 application 层,并从 `AppFacade` 顶层方法开始:
  - `init`
  - `invite`
  - `join`
  - `members`
  - `devices`
  - `status`
  - `space-status`
  - `send`
  - `watch`
  - `blob`
  - `search`
- 旧 `setup` 命令已删除。
- `search rebuild --no-wait` 已删除;`search rebuild` 在当前 CLI 进程内同步执行。
- `status` 已改为应用状态,不再查询 daemon worker 状态。
- `start` / `stop` / 隐藏 `daemon` 仍保留 daemon 进程管理职责。
- `uc-cli` 已移除对 `uc-daemon-client`、`uc-cli-macros`、`dialoguer` 的依赖。

### 本次未处理的剩余问题

- daemon API handler 仍有直接调用 `app.search`、`app.encryption`、`app.member_roster`、`app.space_setup` 的路径。
- daemon worker 仍有直接持有/调用子 facade 的路径。
- `AppFacade` 公开字段仍允许外部绕过顶层方法,后续收口 daemon 时应一起处理。
