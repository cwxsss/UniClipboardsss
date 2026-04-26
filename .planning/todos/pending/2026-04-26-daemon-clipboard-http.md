---
created: 2026-04-26T11:07:09.705Z
title: 收口 daemon clipboard HTTP 入口
area: api
files:
  - src-tauri/crates/uc-daemon/src/api/clipboard.rs
  - src-tauri/crates/uc-daemon/src/api/conversion.rs
  - src-tauri/crates/uc-application/src/facade/app_facade.rs
---

## Problem

daemon 的 clipboard HTTP handler 仍直接构造 `CoreUseCases`,并直接使用 core entry id、链接解析和 uc-app 返回模型。它还依赖 `api/conversion.rs` 中对 uc-app clipboard DTO 的投影,不符合外部只调用 `AppFacade` 的边界目标。

## Solution

在 `uc-application` 下新增或扩展 clipboard-facing application facade,覆盖列表、详情、资源、清历史等 HTTP 入口需要的应用模型。daemon 统一从 `AppFacade` 调用,只保留 HTTP DTO 映射和状态码处理。
