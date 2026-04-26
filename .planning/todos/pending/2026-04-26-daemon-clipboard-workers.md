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

## Progress

- 2026-04-26:已完成 inbound worker 的入站应用调用收口。
  - 新增 `InboundClipboardFacade` 和 application 层输入/输出模型。
  - daemon `InboundClipboardSyncWorker` 不再直接持有 `ApplyInboundClipboardUseCase`,也不再直接处理 core `EntryId`。
  - 已验证 `cargo test -p uc-application facade::clipboard_inbound --lib`、`cargo check -p uc-application -p uc-daemon`、`cargo test -p uc-daemon --lib`。
- 2026-04-26:已完成 watcher capture 落库调用收口。
  - 新增 `ClipboardCaptureFacade` 和 application 层 `CapturedClipboardEntryView`。
  - daemon `DaemonClipboardChangeHandler` 不再构造 `CaptureClipboardUseCase`,改为调用 application facade。
  - 已验证 `cargo test -p uc-application facade::clipboard_capture --lib`、`cargo check -p uc-application -p uc-daemon`、`cargo test -p uc-daemon --lib`。

## Remaining

- outbound `DaemonClipboardChangeHandler` 仍直接持有 `CoreRuntime`,并在 worker 内执行 search live index、outbound planning、blob refs dispatch。
- `entrypoint.rs` 仍负责构造 inbound use case 并包成 facade,归入 composition root 收口。
