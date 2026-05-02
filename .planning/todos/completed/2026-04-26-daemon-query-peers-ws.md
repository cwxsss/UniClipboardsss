---
created: 2026-04-26T11:07:09.705Z
title: 收口 daemon query peers ws
area: api
files:
  - src-tauri/crates/uc-daemon/src/api/query.rs
  - src-tauri/crates/uc-daemon/src/api/ws.rs
  - src-tauri/crates/uc-daemon/src/peers/presence_monitor.rs
---

## Problem

daemon 的 query / peers / ws 状态读取仍直接持有 `CoreRuntime`、`PresencePort` 和 core space access / presence 状态。虽然部分成员列表已经经由 application facade,但整体查询通道还没有统一到 `AppFacade`。

## Solution

把 daemon 状态查询、peers snapshot、presence 事件投影收成 application query facade。daemon 只负责 HTTP/WS 传输和订阅管理,状态模型和投影规则放到 application。

## Progress

- 2026-04-26:已完成 query service 的 peers / paired devices 收口。
  - `AppFacade` 增加 `list_peer_snapshots()` / `list_members()` 统一入口。
  - daemon `DaemonQueryService` 不再持有 `CoreRuntime` / `PresencePort`,改为只持有 `AppFacade`。
  - `/peers` 查询使用 application 层 `PeerSnapshotView` 后再投影为 daemon DTO。
  - 已验证 `cargo check -p uc-application -p uc-daemon`、`cargo test -p uc-application facade::roster --lib`、`cargo test -p uc-daemon peers::presence_monitor --lib`、`cargo test -p uc-daemon api::ws --lib`、`cargo test -p uc-daemon --lib`。

- 2026-04-26:已完成 presence monitor / WS 推送收口。
  - `AppFacade` 增加 `subscribe_peer_presence_events()` 统一入口。
  - daemon `PresenceMonitor` 不再直接订阅 `PresencePort`,也不再从 `CoreRuntime` 派生 peer snapshot。
  - 删除 daemon 内部 `peers/snapshot.rs`,peer snapshot 规则统一由 application 侧提供。
  - 已验证 `cargo check -p uc-application -p uc-daemon`、`cargo test -p uc-daemon peers::presence_monitor --lib`、`cargo test -p uc-application facade::roster --lib`、`cargo test -p uc-daemon --lib`。

## Completed

本 todo 范围已完成。daemon 装配阶段仍直接接收 `MemberRosterFacade` 用于创建 `AppFacade`,归入已有 composition root 收口 todo。
