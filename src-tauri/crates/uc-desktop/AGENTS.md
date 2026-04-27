# uc-desktop 指南

## 定位

`uc-desktop` 是 UniClipboard 的桌面宿主层，负责把 `uc-application`
跑在桌面环境里。

它可以负责：

- 组装和启动桌面运行模式
- daemon 生命周期
- 本地 HTTP/WS/IPC 接入
- Tauri 桥接接入
- 桌面事件源
- 后台任务调度
- 桌面特有策略

它不负责：

- setup 状态迁移规则
- pairing 协议推进
- sync 决策
- transfer 会话决策
- 剪贴板内容分类规则

这些业务能力必须留在 `uc-application` 或 `uc-core`。

## 边界规则

- 外部业务调用只走 `uc_application::facade::AppFacade`。
- 不要在 HTTP handler、daemon worker、Tauri command 里重新拼业务流程。
- 事件源只负责监听桌面事件，并把事件交给应用层入口。
- 后台任务的运行时调度可以在这里，任务的业务定义应放在应用层。
- `uc-daemon` 现在只是兼容壳；新增 daemon 宿主能力应放在这里。

## 当前落地边界

- daemon 实现放在 `src/daemon/`，外部只应使用 `uc_desktop::daemon::run`
  和 `uc_desktop::daemon::run_mode`。
- `uc-webserver` 暂时保持独立 crate，由 `uc-desktop` 作为宿主调用；不要为了目录一致性直接把 HTTP/WS 物理迁入 `uc-desktop`。
- `uc-daemon` 只保留旧二进制和旧路径兼容；不要在其中新增宿主逻辑。
