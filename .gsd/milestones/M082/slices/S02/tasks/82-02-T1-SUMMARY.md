---
id: 82-02-T1
parent: S02
milestone: M082
provides: []
requires: []
affects: []
key_files: ['src/api/daemon/client.ts']
key_decisions: []
patterns_established: []
drill_down_paths: []
observability_surfaces: []
duration: ''
verification_result: 'bun run lint --quiet: 无相关 lint 错误'
completed_at: 2026-04-01T15:35:43.362Z
blocker_discovered: false
---

# 82-02-T1: 为 DaemonClient 添加 blobUrl() 方法，支持在 <img src> 中使用带 auth 的完整 URL

> 为 DaemonClient 添加 blobUrl() 方法，支持在 <img src> 中使用带 auth 的完整 URL

## What Happened

---

id: 82-02-T1
parent: S02
milestone: M082
key_files:

- src/api/daemon/client.ts
  key_decisions:
- (none)
  duration: ""
  verification_result: passed
  completed_at: 2026-04-01T15:35:43.363Z
  blocker_discovered: false

---

# 82-02-T1: 为 DaemonClient 添加 blobUrl() 方法，支持在 <img src> 中使用带 auth 的完整 URL

**为 DaemonClient 添加 blobUrl() 方法，支持在 <img src> 中使用带 auth 的完整 URL**

## What Happened

在 `DaemonClient` 类中添加了 `blobUrl(path: string): string | null` 方法。方法使用与 `sendRequest()` 相同的 URL 构建逻辑（`${config.baseUrl}${path}` + `?auth=Session ${token}`），但同步返回 URL 字符串而非发起 fetch。配置未初始化或 session token 不可用时返回 `null`。修复了 import 顺序问题以通过 lint 检查。

## Verification

bun run lint --quiet: 无相关 lint 错误

## Verification Evidence

| #   | Command                                                                               | Exit Code               | Verdict        | Duration         |
| --- | ------------------------------------------------------------------------------------- | ----------------------- | -------------- | ---------------- | ------------ | --------------- | --------- | --- | ------- | ------ |
| 1   | `cd /Volumes/ExternalSSD/myprojects/uniclipboard-desktop && bun run lint --quiet 2>&1 | grep -E "(ClipboardItem | clipboardItems | ClipboardPreview | PreviewPanel | daemon/client)" | head -10` | 0   | ✅ pass | 4500ms |

## Deviations

None

## Known Issues

None

## Files Created/Modified

- `src/api/daemon/client.ts`

## Deviations

None

## Known Issues

None
