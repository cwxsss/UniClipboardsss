# 进度记录：建立 uc-desktop 桌面宿主

## 2026-04-27

- 检查当前仓库结构，确认宿主相关能力分散在 `uc-daemon`、`uc-webserver`、`uc-tauri`、`uc-bootstrap`。
- 运行 `cargo check -p uc-desktop`，确认当前 workspace 还没有 `uc-desktop` 包。
- 从现有 `uc-daemon` 创建 `src-tauri/crates/uc-desktop`。
- 将 daemon 宿主实现文件迁入 `uc-desktop`。
- 删除 `uc-desktop` 中复制出来的 daemon 二进制入口，避免新增第二个 daemon 命令。
- 更新 `src-tauri/Cargo.toml`，将 `uc-desktop` 加入 workspace。
- 更新 `uc-desktop/Cargo.toml`，将包名和库名改为 `uc-desktop` / `uc_desktop`。
- 将 `uc-daemon` 缩成兼容壳，只依赖 `uc-desktop`，继续保留 `uniclipboard-daemon` 二进制。
- 将 `uc-daemon/src/lib.rs` 改为重新导出 `uc-desktop`，保持旧 `uc_daemon::*` 调用路径可用。
- 新增 `src-tauri/crates/uc-desktop/AGENTS.md`，说明 `uc-desktop` 只做桌面宿主，不写业务规则。
- 运行 `cargo check -p uc-desktop -p uc-daemon -p uc-tauri`，通过。
- 运行 `cargo check -p uc-cli`，通过。
- 运行 `cargo check -p uniclipboard`，通过，并成功准备 daemon 二进制。
- 开始第二阶段：收拢启动和应用入口装配。
- 在 `uc-bootstrap/src/non_gui_runtime.rs` 新增 `build_app_facade_from_deps`、`AppFacadeAssemblyOptions`、`ClipboardRestoreAssembly`。
- 将 CLI 查询 runtime、完整 CLI runtime、Tauri runtime、desktop daemon 入口改为共用 `build_app_facade_from_deps`。
- 保留运行模式差异：daemon 仍传入 space setup、roster、clipboard sync、blob transfer、restore、search coordinator；Tauri 仍只传入 restore；CLI 按命令类型传入需要的能力。
- 运行 `cargo fmt --all`，通过。
- 运行 `cargo check -p uc-bootstrap -p uc-desktop -p uc-tauri`，通过。
- 运行 `cargo check -p uc-cli -p uniclipboard`，通过，并成功准备 daemon 二进制。
- 运行 `cargo tree -p uc-tauri | rg "uc-desktop|uc-daemon v" || true`，无输出，说明 `uc-tauri` 没有重新依赖 `uc-desktop` 或 `uc-daemon`。
- 运行 `git diff --check`，通过。
- 提交 `70d14e52 arch: introduce uc-desktop host crate`。
- 开始第三阶段：收拢 daemon 启动里的后台服务创建和启动编排。
- 新增 `src-tauri/crates/uc-desktop/src/daemon/mod.rs`，作为 daemon 运行模式模块入口。
- 新增 `src-tauri/crates/uc-desktop/src/daemon/service_plan.rs`，集中维护立即启动服务、延迟启动服务和初始服务状态。
- 将 `entrypoint.rs` 中内联的服务分组逻辑替换为 `DaemonServicePlan`。
- 为服务清单补充 2 个单元测试，覆盖解锁独立模式和 GUI/锁定延迟模式。
- 运行 `cargo test -p uc-desktop daemon::service_plan -- --nocapture`，通过。
- 运行 `cargo fmt --all`，通过。
- 运行 `cargo check -p uc-desktop -p uc-daemon -p uc-cli -p uniclipboard`，通过，并成功准备 daemon 二进制。
- 运行 `git diff --check`，通过。

## 工作区备注

- 本轮没有修改与 `uc-desktop` 宿主迁移无关的用户手写内容。
- 当前仍有这三个计划文件本身为未跟踪文件，属于本次记录用途。
