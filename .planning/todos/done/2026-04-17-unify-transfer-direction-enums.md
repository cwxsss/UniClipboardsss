---
created: 2026-04-17T13:03:08.519Z
title: 统一 TransferDirection 与 FileTransferDirection 枚举
area: core
files:
  - src-tauri/crates/uc-app/src/shared/host_event_publisher.rs:145-150
  - src-tauri/crates/uc-core/src/ports/transfer_progress.rs:10-15
  - src-tauri/crates/uc-core/src/file_transfer/event.rs:11-22
  - src-tauri/crates/uc-core/src/ports/mod.rs:97
  - src-tauri/crates/uc-core/src/lib.rs:30
  - src-tauri/crates/uc-daemon-contract/src/api/types.rs:5
  - src-tauri/crates/uc-platform/src/adapters/file_transfer/service.rs:25
  - src-tauri/crates/uc-platform/src/adapters/libp2p_network/stream_handler.rs:11
  - src-tauri/crates/uc-platform/src/adapters/libp2p_network/business_stream.rs:15
  - src-tauri/crates/uc-app/src/usecases/file_sync/file_transfer_orchestrator.rs:20
  - src-tauri/crates/uc-app/src/shared/host_event.rs:16
---

## Problem

`uc-core` 同时存在两个语义完全一致的枚举：

- `uc_core::ports::transfer_progress::TransferDirection`（ports 层，服务于
  `TransferProgressPort` 与底层 chunk 进度上报）
- `uc_core::file_transfer::FileTransferDirection`（file_transfer 领域事件）

两者都只有 `Sending` / `Receiving` 两个变体，且 `FileTransferDirection` 已经为
`TransportTransferDirection` 提供了 `From` 实现
（`uc-core/src/file_transfer/event.rs:16-22`）。

结果是：`uc-app/src/shared/host_event_publisher.rs:145-150` 不得不写一个纯样板
`direction_to_transport`，只是把 `FileTransferDirection` 转回 ports 层的
`TransferDirection`。这是明显的重复定义造成的摩擦：领域层的同一个概念，被迫维护
两份类型 + 一次映射函数。

影响的调用方（均只关心方向本身，不关心类型归属）：

- `uc-daemon-contract/src/api/types.rs` — API DTO 直接使用 `TransferDirection`
- `uc-platform/src/adapters/file_transfer/service.rs`、`libp2p_network/*` —
  传输适配器上报进度时使用 `TransferDirection`
- `uc-app/src/usecases/file_sync/file_transfer_orchestrator.rs` — 根据
  `TransferDirection::Receiving` 做分支
- `uc-app/src/shared/host_event.rs` — UI 广播契约

## Solution

在 `uc-core` 里把 `TransferDirection` 合并为 `FileTransferDirection` 的别名，
或者反过来，只保留 `FileTransferDirection` 并删掉 `ports::TransferDirection`：

1. 决定保留哪一个作为规范名称（倾向 `FileTransferDirection`，因为它已经在
   `file_transfer` 领域模块中成为事件字段的类型，且有完整的 transport 转换）。
2. 在 `uc-core/src/ports/transfer_progress.rs` 中把 `TransferDirection` 改为
   `pub use crate::file_transfer::FileTransferDirection as TransferDirection;`
   或直接替换 `TransferProgress::direction` 字段类型。
3. 级联更新所有调用方 import（见 files 列表），让 ports / app / platform /
   daemon-contract 全部直接使用统一类型。
4. 删除 `uc-app/src/shared/host_event_publisher.rs:145-150` 的
   `direction_to_transport` 样板函数。
5. 同步审视 `TransferHostEvent` 是否继续使用同一枚举作为 UI 契约字段，或需要新
   的 view-model 层映射（若 UI 契约需要稳定字符串，应在 publisher 里显式序列化
   而不是复制一份枚举）。

注意：按 `uc-core/AGENTS.md` 的边界约束，这次合并只涉及领域内部两个等价类型的
去重，不引入新依赖，属于合规重构。
