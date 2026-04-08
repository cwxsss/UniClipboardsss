---
id: 82-03-T1
parent: S03
milestone: M082
provides: []
requires: []
affects: []
key_files: ['src-tauri/src/main.rs']
key_decisions: []
patterns_established: []
drill_down_paths: []
observability_surfaces: []
duration: ''
verification_result: 'cargo check: 0 errors; 无 uc:// 相关代码残留'
completed_at: 2026-04-01T15:41:27.991Z
blocker_discovered: false
---

# 82-03-T1: 删除 main.rs 中的 uc:// 协议处理器和 7 个辅助函数

> 删除 main.rs 中的 uc:// 协议处理器和 7 个辅助函数

## What Happened

---

id: 82-03-T1
parent: S03
milestone: M082
key_files:

- src-tauri/src/main.rs
  key_decisions:
- (none)
  duration: ""
  verification_result: passed
  completed_at: 2026-04-01T15:41:27.992Z
  blocker_discovered: false

---

# 82-03-T1: 删除 main.rs 中的 uc:// 协议处理器和 7 个辅助函数

**删除 main.rs 中的 uc:// 协议处理器和 7 个辅助函数**

## What Happened

从 `main.rs` 中删除了 uc:// 协议处理器的所有代码：删除了 `http` 模块相关的 import（`tauri::http::header` 和 `tauri::http::{Request, Response, StatusCode}`）、`uc_tauri::protocol` import，以及 7 个 helper 函数（`is_allowed_cors_origin`、`set_cors_headers`、`build_response`、`text_response`、`resolve_uc_request`、`resolve_uc_blob_request`、`resolve_uc_thumbnail_request`）和对应的测试模块。从 builder chain 中删除了 `.register_asynchronous_uri_scheme_protocol("uc", ...)` 调用及其注释。

## Verification

cargo check: 0 errors; 无 uc:// 相关代码残留

## Verification Evidence

| #   | Command                                 | Exit Code | Verdict | Duration |
| --- | --------------------------------------- | --------- | ------- | -------- | --- |
| 1   | `cd src-tauri && cargo check 2>&1`      | 0         | ✅ pass | 6000ms   |
| 2   | `cd src-tauri && rg 'uc://' src/main.rs | wc -l`    | 0       | ✅ pass  | 0ms |

## Deviations

None

## Known Issues

None

## Files Created/Modified

- `src-tauri/src/main.rs`

## Deviations

None

## Known Issues

None
