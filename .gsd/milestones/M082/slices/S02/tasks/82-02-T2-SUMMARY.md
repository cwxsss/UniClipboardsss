---
id: 82-02-T2
parent: S02
milestone: M082
provides: []
requires: []
affects: []
key_files:
  [
    'src/api/clipboardItems.ts',
    'src/components/clipboard/ClipboardItem.tsx',
    'src/components/clipboard/ClipboardPreview.tsx',
    'src/preview-panel/PreviewPanel.tsx',
  ]
key_decisions: []
patterns_established: []
drill_down_paths: []
observability_surfaces: []
duration: ''
verification_result: 'daemonClient.blobUrl 在 4 个文件中各出现 1 次；resolveUcUrl 在 4 个文件中均为 0 次'
completed_at: 2026-04-01T15:35:43.363Z
blocker_discovered: false
---

# 82-02-T2: 将 4 个前端消费者从 resolveUcUrl 迁移到 daemonClient.blobUrl

> 将 4 个前端消费者从 resolveUcUrl 迁移到 daemonClient.blobUrl

## What Happened

---

id: 82-02-T2
parent: S02
milestone: M082
key_files:

- src/api/clipboardItems.ts
- src/components/clipboard/ClipboardItem.tsx
- src/components/clipboard/ClipboardPreview.tsx
- src/preview-panel/PreviewPanel.tsx
  key_decisions:
- (none)
  duration: ""
  verification_result: passed
  completed_at: 2026-04-01T15:35:43.364Z
  blocker_discovered: false

---

# 82-02-T2: 将 4 个前端消费者从 resolveUcUrl 迁移到 daemonClient.blobUrl

**将 4 个前端消费者从 resolveUcUrl 迁移到 daemonClient.blobUrl**

## What Happened

将 4 个前端文件中的 `resolveUcUrl()` 调用替换为 `daemonClient.blobUrl()`。每个文件：移除 `@/lib/protocol` import，添加 `@/api/daemon/client` import（按字母序排列），更新 URL 解析逻辑。修复了所有文件的 import 顺序 lint 错误。

## Verification

daemonClient.blobUrl 在 4 个文件中各出现 1 次；resolveUcUrl 在 4 个文件中均为 0 次

## Verification Evidence

| #   | Command                                                                                                                                                                                                                                                                                                                                  | Exit Code      | Verdict | Duration |
| --- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------- | ------- | -------- | --- |
| 1   | `cd /Volumes/ExternalSSD/myprojects/uniclipboard-desktop && for f in src/api/clipboardItems.ts src/components/clipboard/ClipboardItem.tsx src/components/clipboard/ClipboardPreview.tsx src/preview-panel/PreviewPanel.tsx; do echo -n "$f daemonClient.blobUrl: $(rg -c 'daemonClient.blobUrl' $f) resolveUcUrl: $(rg 'resolveUcUrl' $f | wc -l)"; done` | 0       | ✅ pass  | 0ms |

## Deviations

None

## Known Issues

None

## Files Created/Modified

- `src/api/clipboardItems.ts`
- `src/components/clipboard/ClipboardItem.tsx`
- `src/components/clipboard/ClipboardPreview.tsx`
- `src/preview-panel/PreviewPanel.tsx`

## Deviations

None

## Known Issues

None
