---
created: 2026-03-21T11:14:15.305Z
title: 修复 setup 配对确认提示缺失
area: ui
files:
  - src/pages/SetupPage.tsx
  - src/api/setup.ts
  - src/hooks/useDeviceDiscovery.ts
  - src/components/PairingNotificationProvider.tsx
  - src-tauri/crates/uc-app/src/usecases/setup/action_executor.rs
  - src-tauri/crates/uc-app/src/realtime/setup_consumer.rs
---

## Problem

在本轮 daemon-only discovery 修复后，设备发现已经恢复，但 setup 配对主流程仍存在行为缺口：

- 用户在 setup 中选择设备后，可以发起配对。
- 后端开始推进配对流程，但前端没有出现期望的确认 toast / 确认提示。
- 用户反馈这是“整个流程的配对仍然存在问题”，但本轮先不继续修。

这说明“发现设备”与“发起配对”虽然已经统一到 daemon 事实源，但 setup / pairing 的前端提示链路仍可能缺少事件映射、状态推进，或 toast 展示条件。

## Solution

TBD

建议下个会话从以下方向排查：

- 核对 `select_device` 之后 setup realtime 是否收到 `JoinSpaceConfirmPeer` 或等价确认事件。
- 核对前端 setup 页面是否只依赖状态页切换，而没有真正消费需要的 pairing 提示事件。
- 检查 `PairingNotificationProvider`、`src/api/setup.ts`、`setup_consumer.rs`、`action_executor.rs` 之间是否存在事件丢失或条件不匹配。
- 用真实日志区分“后端未发确认事件”和“前端收到事件但未展示 toast”。
