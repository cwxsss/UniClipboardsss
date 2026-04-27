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

## 错误记录

| 错误 | 尝试 | 处理 |
|---|---|---|
| 无 | - | 目标包、CLI、根桌面应用编译检查和新增服务清单测试均通过 |
