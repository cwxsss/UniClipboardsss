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

## 后续 gap

- 需要决定 `uc-bootstrap` 是保留为通用组装库，还是逐步并入 `uc-desktop` 的 bootstrap 模块。
- 需要后续再收拢 HTTP/WS 和 Tauri bridge 的宿主归属。
- 需要逐步缩小 `uc-daemon` 兼容层，最终只保留必要的外部入口。
- 需要继续把 worker 构造本身拆成更清晰的 daemon runtime assembly，进一步缩短 `entrypoint.rs`。
