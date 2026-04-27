# 发现记录：建立 uc-desktop 桌面宿主

## 已知上下文

- 目标是把 daemon 打造成 `uc-desktop` 的一个运行模式，而不是继续让 daemon 作为桌面总宿主。
- 当前仓库中的应用层名称是 `uc-application`，不是草稿中的 `uc-app`。
- 第一阶段只做入口和归属收拢，不改变业务行为。

## 代码结构发现

- Rust 检查必须从 `src-tauri/` 执行。
- `uc-bootstrap` 当前负责大量组装逻辑。
- `uc-daemon` 原本同时承载 daemon 生命周期、服务管理、事件源和入口编排。
- `uc-webserver` 已经独立承载 HTTP/WS 接口。
- `uc-tauri` 当前负责桌面壳、sidecar 启动和 GUI 与 daemon 的连接管理。
- `uc-cli` 仍通过 `uc_daemon::*` 读取部分兼容路径，因此 `uc-daemon` 不能一次性删除。

## 本轮结构变化

- 新增 `src-tauri/crates/uc-desktop`，承接原 daemon 宿主实现。
- `uc-desktop` 暂时包含 daemon 模式、服务、worker、状态和进程元数据等宿主代码。
- `uc-daemon` 现在只保留兼容入口，避免破坏外部命令和旧调用路径。
- `uniclipboard-daemon` 二进制名称未变。
- 新增 `uc-desktop/AGENTS.md` 记录边界：桌面宿主不写业务规则，业务调用走 `uc-application` facade。
- 第二阶段新增 `uc-bootstrap::build_app_facade_from_deps`，将共同的 `AppFacade` 子 facade 拼装收拢到单一函数。
- `uc-desktop`、`uc-tauri`、CLI runtime 现在共用同一个 `AppFacade` 装配函数。
- daemon/Tauri/CLI 的差异没有硬编码在公共函数里，而是通过 `AppFacadeAssemblyOptions` 传入。
- 第三阶段新增 `daemon::service_plan`，把 daemon 服务启动分组从 `entrypoint.rs` 抽出。
- `DaemonServicePlan` 现在统一决定哪些服务立即启动、哪些服务等待 ready 信号后启动。
- `entrypoint.rs` 仍负责构造具体 worker 和 facade，但不再内联维护服务状态列表和分组规则。
- 第四阶段新增 `daemon::runtime_assembly`，把 daemon worker 的依赖拼装从 `entrypoint.rs` 抽出。
- clipboard watcher、inbound clipboard sync、file sync orchestrator 的构造现在集中在 `build_daemon_runtime_workers`。
- `entrypoint.rs` 继续负责启动顺序和生命周期，不再直接知道 clipboard worker 内部依赖怎么拼。
- 第五阶段新增 `daemon::startup_recovery`，把启动后的后台恢复任务从 `entrypoint.rs` 抽出。
- 后台恢复仍按原顺序执行：读取自动解锁设置、恢复加密会话、恢复空间会话、刷新 presence，并在 CLI 模式成功解锁后触发延迟服务。
- `entrypoint.rs` 现在只把启动恢复所需依赖交给 `spawn_startup_recovery`，不再直接写恢复任务细节。
- 第六阶段新增 `daemon::shutdown`，把 GUI 管理模式下的 stdin EOF 监听从 `entrypoint.rs` 抽出。
- daemon 主循环仍接收同一个关闭信号，关闭行为没有变化。
- 第七阶段新增 `daemon::run_mode`，把 daemon 运行模式收敛成 `Standalone`、`GuiSidecar`、`Hybrid`。
- `GuiSidecar` 保留旧行为：跟随 GUI 父进程退出，等待 GUI ready 后再启动剪贴板相关服务。
- `Hybrid` 现在作为显式模式存在：不跟随 GUI 父进程，不等待 GUI ready，并使用桌面设置里的自动解锁开关。
- 旧的 `--gui-managed` 参数只保留在入口解析层，`uc-desktop` 内部不再用裸布尔值表达运行模式。
- hybrid 本地连接信息已作为待办记录，当前阶段不实现 GUI 连接、token、单实例锁或默认 hybrid 切换。
- daemon 运行模式参数组合规则现在由 `DaemonRunMode::from_flags` 统一维护，避免 `uc-cli` 和 `uc-daemon` 各自判断。
- 第九阶段新增 `daemon::search_assembly`，把搜索协调器和搜索服务构造从 `entrypoint.rs` 抽出。
- 搜索业务仍由 `uc-application` facade 和既有搜索服务处理，`uc-desktop` 这里只做宿主装配。
- 第十阶段新增 `daemon::app_facade_assembly`，把 daemon 模式需要传入 `AppFacade` 的能力清单从 `entrypoint.rs` 抽出。
- 公共 `uc-bootstrap::build_app_facade_from_deps` 仍是 facade 构造的单一入口，`uc-desktop` 只提供 daemon 运行模式的参数装配。
- 第十一阶段新增 `daemon::background_tasks`，把 blob 后台任务的 runtime spawn 包装从 `entrypoint.rs` 抽出。
- blob 后台任务本身仍由 `uc-bootstrap::spawn_blob_processing_tasks` 统一实现，`uc-desktop` 只负责在 daemon runtime 上启动它。
- 第十二阶段新增 `daemon::app_assembly`，把 `DaemonApp` 实例创建和 peer keepalive 接入从 `entrypoint.rs` 抽出。
- peer keepalive 仍通过 `AppFacade` 访问 space setup，服务分组规则仍由 `DaemonServicePlan` 决定。
- 第十三阶段新增 `daemon::run_loop`，把启动恢复任务、daemon 运行和 space setup 关闭顺序从 `entrypoint.rs` 抽出。
- space setup 仍在 `daemon.run()` 返回后、Tokio runtime 释放前关闭，原有收尾顺序保持不变。

## 验证发现

- `cargo check -p uc-desktop -p uc-daemon -p uc-tauri` 通过。
- `cargo check -p uc-cli` 通过，说明旧 `uc_daemon::*` 兼容路径没有断。
- `cargo check -p uniclipboard` 通过，说明根桌面应用仍可编译，并成功准备 daemon 二进制。
- `cargo check -p uc-bootstrap -p uc-desktop -p uc-tauri` 通过。
- `cargo check -p uc-cli -p uniclipboard` 通过。
- `cargo tree -p uc-tauri | rg "uc-desktop|uc-daemon v" || true` 无输出，说明 `uc-tauri` 没有重新依赖 `uc-desktop` 或 `uc-daemon`。
- `git diff --check` 通过。
- `cargo test -p uc-desktop daemon::service_plan -- --nocapture` 通过。
- `cargo check -p uc-desktop -p uc-daemon -p uc-cli -p uniclipboard` 通过。
- `cargo check -p uc-desktop -p uc-daemon -p uc-cli` 通过。
- `cargo check -p uniclipboard` 通过。
- `cargo test -p uc-desktop daemon::service_plan -- --nocapture` 通过。
- 抽出 GUI 管理模式关闭信号后，`cargo check -p uc-desktop -p uc-daemon -p uc-cli` 通过。
- 抽出 GUI 管理模式关闭信号后，`cargo check -p uniclipboard` 通过。
- 引入 `DaemonRunMode` 后，`cargo test -p uc-desktop daemon::run_mode -- --nocapture` 通过。
- 引入 `DaemonRunMode` 后，`cargo test -p uc-desktop daemon::service_plan -- --nocapture` 通过。
- 引入 `DaemonRunMode` 后，`cargo check -p uc-desktop -p uc-daemon -p uc-cli` 通过。
- 引入 `DaemonRunMode` 后，`cargo check -p uniclipboard` 通过。
- 收口 daemon 运行模式参数解析后，`cargo test -p uc-desktop daemon::run_mode -- --nocapture` 通过。
- 收口 daemon 运行模式参数解析后，`cargo check -p uc-desktop -p uc-daemon -p uc-cli` 通过。
- 收口 daemon 运行模式参数解析后，`cargo check -p uniclipboard` 通过。
- 抽出 daemon 搜索服务装配后，`cargo test -p uc-desktop daemon::run_mode -- --nocapture` 通过。
- 抽出 daemon 搜索服务装配后，`cargo test -p uc-desktop daemon::service_plan -- --nocapture` 通过。
- 抽出 daemon 搜索服务装配后，`cargo check -p uc-desktop -p uc-daemon -p uc-cli` 通过。
- 抽出 daemon 搜索服务装配后，`cargo check -p uniclipboard` 通过。
- 抽出 daemon AppFacade 装配后，`cargo test -p uc-desktop daemon::run_mode -- --nocapture` 通过。
- 抽出 daemon AppFacade 装配后，`cargo test -p uc-desktop daemon::service_plan -- --nocapture` 通过。
- 抽出 daemon AppFacade 装配后，`cargo check -p uc-desktop -p uc-daemon -p uc-cli` 通过。
- 抽出 daemon AppFacade 装配后，`cargo check -p uniclipboard` 通过。
- 抽出 daemon 后台 blob 任务启动后，`cargo test -p uc-desktop daemon::run_mode -- --nocapture` 通过。
- 抽出 daemon 后台 blob 任务启动后，`cargo test -p uc-desktop daemon::service_plan -- --nocapture` 通过。
- 抽出 daemon 后台 blob 任务启动后，`cargo check -p uc-desktop -p uc-daemon -p uc-cli` 通过。
- 抽出 daemon 后台 blob 任务启动后，`cargo check -p uniclipboard` 通过。
- 抽出 daemon 应用实例装配后，`cargo test -p uc-desktop daemon::run_mode -- --nocapture` 通过。
- 抽出 daemon 应用实例装配后，`cargo test -p uc-desktop daemon::service_plan -- --nocapture` 通过。
- 抽出 daemon 应用实例装配后，`cargo check -p uc-desktop -p uc-daemon -p uc-cli` 通过。
- 抽出 daemon 应用实例装配后，`cargo check -p uniclipboard` 通过。
- 抽出 daemon 运行循环后，`cargo test -p uc-desktop daemon::run_mode -- --nocapture` 通过。
- 抽出 daemon 运行循环后，`cargo test -p uc-desktop daemon::service_plan -- --nocapture` 通过。
- 抽出 daemon 运行循环后，`cargo check -p uc-desktop -p uc-daemon -p uc-cli` 通过。
- 抽出 daemon 运行循环后，`cargo check -p uniclipboard` 通过。

## 后续 gap

- 需要决定 `uc-bootstrap` 是保留为通用组装库，还是逐步并入 `uc-desktop` 的 bootstrap 模块。
- 需要后续再收拢 HTTP/WS 和 Tauri bridge 的宿主归属。
- 需要逐步缩小 `uc-daemon` 兼容层，最终只保留必要的外部入口。
- 需要继续收窄 `entrypoint.rs`，把剩余 daemon run context、shutdown 包装和宿主启动顺序拆成更清晰的桌面宿主步骤。
