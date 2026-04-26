---
created: 2026-04-26T11:07:09.705Z
title: 收口 daemon search 入口
area: api
files:
  - src-tauri/crates/uc-daemon/src/api/search.rs
  - src-tauri/crates/uc-daemon/src/search/coordinator.rs
  - src-tauri/crates/uc-daemon/src/search/projection.rs
  - src-tauri/crates/uc-application/src/facade/app_facade.rs
---

## Problem

daemon search 入口仍直接构造 core `SearchQuery`、`ContentType`、`TimeRangeFilter`,并直接映射 core `SearchError`。search coordinator / projection 还直接知道 core 和 infra 搜索类型。

## Solution

先把 `GET /search/query` 的查询输入、结果和错误收成 application 模型,再处理 status/rebuild/coordinator。最终 daemon search handler 只通过 `AppFacade` 调用 search application 入口,不再 import core/infra 搜索类型。

## Progress

- 2026-04-26:已完成 `GET /search/query` 收口。daemon handler 不再构造 core `SearchQuery` / `ContentType` / `TimeRangeFilter`,查询参数解析与错误分类迁入 `uc-application::facade::SearchFacade`。
- 剩余:status/rebuild/coordinator/projection 仍需继续收口。
