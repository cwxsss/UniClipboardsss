---
created: 2026-04-27T00:00:00+08:00
title: Hybrid daemon 本地连接信息
area: desktop-host
files:
  - src-tauri/crates/uc-desktop/src/daemon/
  - src-tauri/crates/uc-tauri/src/bootstrap/run.rs
  - src-tauri/crates/uc-daemon-client/src/
---

## Problem

Hybrid 模式下 daemon 常驻，GUI 只是本机客户端。GUI 需要可靠知道当前 profile 的 daemon 是否已运行、如何连接、如何认证，不能依赖 GUI 直接拥有 daemon 子进程生命周期。

## Solution

为 hybrid 模式设计 profile 级本地连接信息文件，例如 `daemon-connection.json`。连接信息表达“本机客户端如何连接 daemon”，不要绑定死 HTTP 细节。

建议字段：

- `transport`: 当前可先为 `http`
- `address`: 本地监听地址，例如 `127.0.0.1`
- `port`: daemon 实际监听端口
- `token`: 本机访问 token
- `pid`: daemon 进程号
- `profile`: 当前 profile
- `created_at`: 写入时间

GUI 启动流程后续改为：

1. 读取当前 profile 的连接信息。
2. 调用 daemon health 接口并带本地 token。
3. 成功则复用已有 daemon。
4. 失败再启动 hybrid daemon。
5. GUI 关闭不停止 hybrid daemon。

## Notes

- 这个任务先记录，不在当前 `uc-daemon` / `uc-desktop` 结构收口阶段实现。
- 真正切默认 hybrid 前，还需要单实例锁、本地 token、连接信息过期清理和启动失败提示。
