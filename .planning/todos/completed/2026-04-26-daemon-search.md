---
created: 2026-04-26T11:07:09.705Z
title: 收口 daemon search 入口
area: api
files:
  - src-tauri/crates/uc-daemon/src/api/search.rs
  - src-tauri/crates/uc-daemon/src/search/coordinator.rs
  - src-tauri/crates/uc-application/src/facade/app_facade.rs
---

## Problem

daemon search 入口仍直接构造 core `SearchQuery`、`ContentType`、`TimeRangeFilter`,并直接映射 core `SearchError`。search coordinator / projection 还直接知道 core 和 infra 搜索类型。

## Solution

先把 `GET /search/query` 的查询输入、结果和错误收成 application 模型,再处理 status/rebuild/coordinator。最终 daemon search handler 只通过 `AppFacade` 调用 search application 入口,不再 import core/infra 搜索类型。

## Progress

- 2026-04-26:已完成 `GET /search/query` 收口。daemon handler 不再构造 core `SearchQuery` / `ContentType` / `TimeRangeFilter`,查询参数解析与错误分类迁入 `uc-application::facade::SearchFacade`。
- 2026-04-26:已完成 `/search/status`、`/search/rebuild` 和 SEARCH websocket snapshot 收口。daemon HTTP/WS handler 不再直接读 search coordinator 或 search index meta,统一走 `AppFacade.search`。
- 2026-04-26:已完成 search projection 规则迁移。`SearchProjectionBuilder` 移到 `uc-application`,daemon search coordinator / clipboard watcher 改为调用 application 导出的 projection builder。
- 2026-04-26:已完成 search coordinator 本体迁移。重建判断、状态、串行化和进度事件归 `uc-application`;daemon search 模块只保留服务生命周期和 WS 转发包装。

## Completed

本 todo 范围已完成。daemon `entrypoint.rs` 仍负责从 runtime 取 ports 组装 application coordinator deps,归入已有 composition root 收口 todo。
