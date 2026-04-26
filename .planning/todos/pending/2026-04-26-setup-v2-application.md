---
created: 2026-04-26T11:07:09.705Z
title: 收口 setup v2 application 输入模型
area: api
files:
  - src-tauri/crates/uc-daemon/src/api/v2/setup.rs
  - src-tauri/crates/uc-application/src/facade/app_facade.rs
---

## Problem

daemon 的 setup v2 handler 已经通过 `AppFacade` 调用 application,但 HTTP 层仍直接构造 core 的 `Passphrase`、`InvitationCode` 等输入模型。外部入口不应该知道 core 领域模型,应该只传 application 暴露的命令或普通输入。

## Solution

在 `uc-application` 的 facade 层补齐 setup v2 所需的 application 输入模型,让 daemon 只做 HTTP DTO 到 application DTO 的转换。确认 v2 setup handler 不再 import core 类型后,跑 application facade 测试、`cargo check -p uc-daemon` 和 daemon lib 测试。
