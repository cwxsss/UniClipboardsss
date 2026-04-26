---
created: 2026-04-26T11:07:09.705Z
title: 收口 daemon query peers ws
area: api
files:
  - src-tauri/crates/uc-daemon/src/api/query.rs
  - src-tauri/crates/uc-daemon/src/api/ws.rs
  - src-tauri/crates/uc-daemon/src/peers/snapshot.rs
  - src-tauri/crates/uc-daemon/src/peers/presence_monitor.rs
---

## Problem

daemon 的 query / peers / ws 状态读取仍直接持有 `CoreRuntime`、`PresencePort` 和 core space access / presence 状态。虽然部分成员列表已经经由 application facade,但整体查询通道还没有统一到 `AppFacade`。

## Solution

把 daemon 状态查询、peers snapshot、presence 事件投影收成 application query facade。daemon 只负责 HTTP/WS 传输和订阅管理,状态模型和投影规则放到 application。
