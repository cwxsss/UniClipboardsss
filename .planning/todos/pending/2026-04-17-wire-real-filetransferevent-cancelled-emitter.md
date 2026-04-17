---
created: 2026-04-17T15:34:15.629Z
title: 给 FileTransferEvent::Cancelled 接入真实发射方
area: file-transfer
files:
  - src-tauri/crates/uc-daemon/src/workers/file_sync_orchestrator.rs:184-210
  - src-tauri/crates/uc-platform/src/adapters/file_transfer/service.rs
  - src-tauri/crates/uc-bootstrap/src/file_transfer_lifecycle.rs
  - src-tauri/crates/uc-application/src/file_transfer/usecases/cancel_transfer.rs
---

## Problem

`FileTransferEvent::Cancelled` 在领域模型里已经建好、`CancelTransferUseCase` 也已经存在并接入 `FileTransferLifecycle`（`file_transfer_lifecycle.rs` 的 `cancel` 字段），
但**没有任何代码路径真正产出 `Cancelled` 事件**：

- `uc-platform` 的 `FileTransferService` 从未构造 `FileTransferEvent::Cancelled`
- 也没有 daemon / Tauri API 让 UI 主动触发取消
- 当前 `FileSyncOrchestratorWorker::handle_event` 对 `Cancelled` 事件的处理方式是**兜底走 Failed**（见 `file_sync_orchestrator.rs` 行 184-210 的注释：
  "No adapter currently emits Cancelled ... Route through the fail path"），把 `reason` 写进 `detail` 字符串里当文本保留

影响：
- 用户发起"取消传输"这一业务动作，目前没有办法流到领域时间线上
- Projection 把取消折叠成 failed，UI 无法在展示上区分"失败"与"取消"（只能靠 `failure_reason` 文本前缀区分）
- `CancelTransferUseCase` 是死代码

## Solution

TBD — 需要成体系地接通 UI → daemon → use case → 平台层关停流程，大致方向：

1. 给 daemon API 增加 cancel 入口（Tauri command / WS 消息）
2. 入口内部调用 `lifecycle.cancel.execute(CancelTransfer { ... })`
3. `FileTransferService` 的 send/receive 任务需要支持外部中断：携带 `CancellationToken`，被取消时清理 stream + tmp 文件，然后由发起侧发 `FileTransferEvent::Cancelled`
4. 去掉 `file_sync_orchestrator.rs::handle_event` 对 `Cancelled` 走 `fail_transfer` 兜底的逻辑，改为真正经由 `CancelTransferUseCase`
5. 同步评估 projection：目前 `Cancelled` 折叠为 `failed`（见 checklist Phase 2 决议），如果前端要区分两者，可以引入 `failure_reason` 前缀约定或新增状态枚举

参考 `FILE_TRANSFER_USECASE_INTEGRATION_CHECKLIST_ZH.md` 关于 `Cancelled` 的既有决议（Phase 1 第 8 条、Phase 2 第 13 条）。
