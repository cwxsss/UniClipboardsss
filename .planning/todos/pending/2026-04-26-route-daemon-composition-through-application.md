---
created: 2026-04-26T11:07:09.705Z
title: 收口 daemon 装配根
area: architecture
files:
  - src-tauri/crates/uc-daemon/src/app.rs
  - src-tauri/crates/uc-daemon/src/entrypoint.rs
  - src-tauri/crates/uc-application/src/facade/app_facade.rs
---

## Problem

daemon handler 调用面已经开始统一到 `AppFacade`,但 `app.rs` 和 `entrypoint.rs` 仍直接认识 `CoreRuntime`、`wiring_deps`、core ports、platform clipboard 等底层细节。最终目标是外部入口不知道 infra/platform/core,装配根也需要继续瘦身。

## Solution

在前面 HTTP / worker / query 入口逐步收口后,把 `AppFacade` 的构造和 daemon 所需服务组装迁到 application/bootstrap 内部可控边界。daemon 保留进程生命周期、HTTP server 启停和 OS 信号处理。

## Progress

- 2026-04-26:已把宿主事件模型 / 发送端口 / 文件传输 host-event publisher 从 `uc-app::shared` 迁到 `uc_application::facade::host_event`。
  - `uc-app::shared::*` 只保留兼容转发。
  - daemon / bootstrap / Tauri 不再 import `uc_app::shared::*`。
  - daemon `entrypoint.rs` 不再通过 `uc-app` 的 capture 兼容 shim 获取 `CaptureClipboardUseCase`。
  - 已验证 `cargo test -p uc-application facade::host_event --lib`、`cargo test -p uc-daemon --lib`、`cargo check -p uc-application -p uc-app -p uc-bootstrap -p uc-daemon -p uc-tauri`。
- 2026-04-26:已把 lifecycle 状态端口和内存实现从 `uc-app::usecases::app_lifecycle` 迁到 `uc_application::facade::lifecycle`。
  - `uc-app` 旧 lifecycle 路径只保留兼容转发。
  - daemon / bootstrap / Tauri 不再从 `uc_app::usecases` 导入 lifecycle 状态端口。
  - 已验证 `cargo test -p uc-application facade::lifecycle --lib`、`cargo test -p uc-daemon --lib`、`cargo check -p uc-application -p uc-app -p uc-bootstrap -p uc-daemon -p uc-tauri`。
