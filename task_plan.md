# 任务计划：建立 uc-desktop 桌面宿主

## 目标

将现有 daemon 宿主实现收拢到新的 `uc-desktop` crate，让 `uc-desktop` 成为桌面宿主层的起点；原有 `uc-daemon` 先保留为兼容入口，避免破坏现有命令、CLI 调用和 Tauri sidecar 启动链。

## 完成标准

- 新增 `uc-desktop` crate，并纳入 `src-tauri/Cargo.toml` workspace。
- 将原 `uc-daemon` 中的宿主实现迁入 `uc-desktop`。
- `uc-daemon` 保留 `uniclipboard-daemon` 二进制和 `uc_daemon::*` 兼容出口。
- 不改变 daemon 对外启动方式。
- 不修改核心业务逻辑。
- 实际运行 Rust 检查，确认 `uc-desktop`、`uc-daemon`、`uc-tauri`、`uc-cli` 和根桌面应用可编译。

## 阶段

| 阶段 | 状态 | 内容 |
|---|---|---|
| 1 | 完成 | 对照当前 `uc-daemon`、`uc-webserver`、`uc-tauri`、`uc-bootstrap` 职责分布 |
| 2 | 完成 | 创建 `uc-desktop` crate，并迁入 daemon 宿主实现 |
| 3 | 完成 | 将 `uc-daemon` 缩成兼容入口，继续导出旧路径 |
| 4 | 完成 | 增加 `uc-desktop/AGENTS.md`，明确桌面宿主边界 |
| 5 | 完成 | 运行编译检查，确认现有入口链未断 |
| 6 | 完成 | 收拢 `AppFacade` 公共装配，减少 daemon/Tauri/CLI 重复拼装 |
| 7 | 完成 | 抽出 daemon 后台服务启动清单，明确立即启动与 ready 后启动的分组 |
| 8 | 完成 | 抽出 daemon runtime worker 装配，进一步缩短入口编排 |
| 9 | 完成 | 抽出 daemon 启动恢复任务，隔离解锁、会话恢复和 presence 预热 |
| 10 | 完成 | 抽出 GUI 管理模式关闭信号，隔离父进程退出监听 |
| 11 | 完成 | 引入 `DaemonRunMode`，用明确运行模式替代 daemon 内部布尔判断 |
| 12 | 完成 | 收口 daemon 运行模式参数解析，记录 hybrid 连接信息待办 |
| 13 | 完成 | 抽出 daemon 搜索服务装配，继续缩短入口编排 |
| 14 | 完成 | 抽出 daemon AppFacade 装配，明确桌面宿主传入哪些能力 |
| 15 | 完成 | 抽出 daemon 后台 blob 任务启动 |
| 16 | 完成 | 抽出 daemon 应用实例装配 |
| 17 | 完成 | 抽出 daemon 运行循环和退出收尾 |
| 18 | 完成 | 抽出 daemon Tokio runtime 创建 |
| 19 | 完成 | 抽出 daemon 运行控制量创建 |
| 20 | 完成 | 抽出 daemon 服务清单装配 |
| 21 | 完成 | 抽出 daemon bootstrap 拆包装配 |
| 22 | 完成 | 将 daemon API facade 句柄收回 AppFacade 装配模块 |
| 23 | 完成 | 将 daemon host 实现迁入 `daemon/host.rs`，保留旧入口转发 |
| 24 | 完成 | 收窄 `uc-daemon` 兼容导出面 |
| 25 | 完成 | 将 `uc-cli` 从 `uc-daemon` 兼容壳迁出 |
| 26 | 完成 | 删除 `uc-desktop` 内部旧 `entrypoint` 转发 |

## 决策记录

- 第一阶段只收拢宿主入口，不搬业务规则。
- `uc-desktop` 是桌面宿主层，不是业务层。
- 旧的 `uniclipboard-daemon` 命令保持不变。
- 旧的 `uc_daemon::*` 路径暂时保留，后续再逐步收窄。
- HTTP/WS 和 Tauri 先保持原有 crate 关系，不在本阶段继续搬迁。
- 第二阶段将公共 `AppFacade` 装配放在 `uc-bootstrap`，由 `uc-desktop`、`uc-tauri`、CLI 共用；运行模式差异通过显式选项传入。
- 第三阶段只抽服务启动清单，不改 worker 行为、不改 HTTP/WS、不改业务逻辑。
- 第四阶段只抽 worker 构造，不改服务分组和启动策略。
- 第五阶段只抽启动恢复后台任务，不改解锁、空间会话恢复、presence 预热和 ready 后服务触发行为。
- 第六阶段只抽 GUI 管理模式下的关闭信号接入，不改 daemon 主循环和关闭策略。
- 第七阶段只替换运行模式表达，不改变旧的 `--gui-managed` 默认行为；`Hybrid` 先作为显式模式接入 daemon 入口。
- 第八阶段只统一参数解析规则，不切换 GUI 默认启动模式，不实现 hybrid 连接信息。
- 第九阶段只抽搜索协调器和搜索服务构造，不改搜索行为和 HTTP/WS 事件语义。
- 第十阶段只抽 daemon 专属 `AppFacade` 参数装配，不改公共 facade 构造函数和业务入口。
- 第十一阶段只抽后台任务启动包装，不改任务内容、启动时机和取消行为。
- 第十二阶段只抽 `DaemonApp` 实例创建，不改服务清单、keepalive 和 deferred ready 语义。
- 第十三阶段只抽最终运行循环，不改启动恢复、daemon 退出和 space setup shutdown 顺序。
- 第十四阶段只抽 Tokio runtime 创建，不改 runtime 类型、线程模型和生命周期。
- 第十五阶段只抽事件通道、ready notify、剪贴板 gate 和初始解锁状态创建，不改默认值。
- 第十六阶段只抽 worker/search service 到服务清单的装配，不改服务分组规则。
- 第十七阶段只抽 daemon bootstrap context 拆包，不改依赖构造和资源持有顺序。
- 第十八阶段只移动 AppFacade 相关句柄提取，不改本机设备 ID 来源和 facade 能力集合。
- 第十九阶段只移动 daemon host 实现位置，不改 `uc_daemon::entrypoint::run` 兼容路径。
- 第二十阶段只收窄 `uc-daemon` 对外重导出，不改旧命令和当前 CLI 调用路径。
- 第二十一阶段只迁移 `uc-cli` 的依赖路径，不改 CLI 命令语义和 daemon 启动方式。
- 第二十二阶段只删除 `uc-desktop::entrypoint` 旧路径；`uc_daemon::entrypoint::run` 继续保留。

## 错误记录

| 错误 | 尝试 | 处理 |
|---|---|---|
| 无 | - | 目标包、CLI、根桌面应用编译检查和新增服务清单测试均通过 |
