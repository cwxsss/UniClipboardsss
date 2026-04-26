---
created: 2026-04-26T11:07:09.705Z
title: 收口 daemon clipboard workers
area: clipboard-sync
files:
  - src-tauri/crates/uc-daemon/src/workers/clipboard_watcher.rs
  - src-tauri/crates/uc-daemon/src/workers/inbound_clipboard_sync.rs
  - src-tauri/crates/uc-daemon/src/entrypoint.rs
  - src-tauri/crates/uc-application/src/facade/app_facade.rs
---

## Problem

daemon clipboard worker 仍直接依赖 `uc-app` capture/usecase/planner、core snapshot/id/origin 以及 platform clipboard watcher。后台 worker 是外部入口的一部分,也应该通过 application 暴露的 worker-facing 入口驱动。

## Solution

在 application 层补出 clipboard capture / outbound planning / inbound apply 的 worker-facing facade 或 service。daemon worker 只保留进程生命周期、监听循环和日志,业务输入输出统一经过 `AppFacade`。
