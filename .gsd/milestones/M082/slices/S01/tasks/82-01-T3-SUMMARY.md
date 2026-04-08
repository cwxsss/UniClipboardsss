---
id: 82-01-T3
parent: S01
milestone: M082
provides: []
requires: []
affects: []
key_files:
  [
    'src-tauri/crates/uc-app/src/usecases/clipboard/get_entry_resource.rs',
    'src-tauri/crates/uc-app/src/usecases/clipboard/list_entry_projections/list_entry_projections.rs',
  ]
key_decisions: []
patterns_established: []
drill_down_paths: []
observability_surfaces: []
duration: ''
verification_result: 'cargo test -p uc-app -- get_entry_resource: 2 passed; cargo test -p uc-app -- list_entry_projections: 15 passed'
completed_at: 2026-04-01T15:28:16.598Z
blocker_discovered: false
---

# 82-01-T3: 将 use case URL 格式从 uc:// 迁移到 daemon 相对路径

> 将 use case URL 格式从 uc:// 迁移到 daemon 相对路径

## What Happened

---

id: 82-01-T3
parent: S01
milestone: M082
key_files:

- src-tauri/crates/uc-app/src/usecases/clipboard/get_entry_resource.rs
- src-tauri/crates/uc-app/src/usecases/clipboard/list_entry_projections/list_entry_projections.rs
  key_decisions:
- (none)
  duration: ""
  verification_result: passed
  completed_at: 2026-04-01T15:28:16.598Z
  blocker_discovered: false

---

# 82-01-T3: 将 use case URL 格式从 uc:// 迁移到 daemon 相对路径

**将 use case URL 格式从 uc:// 迁移到 daemon 相对路径**

## What Happened

`get_entry_resource.rs` 中将 `uc://blob/{id}` 改为 `/clipboard/blobs/{id}`。`list_entry_projections.rs` 中两处（`execute` 和 `execute_single`）将 `uc://thumbnail/{id}` 改为 `/clipboard/thumbnails/{id}`。对应的测试断言同步更新。所有测试通过。

## Verification

cargo test -p uc-app -- get_entry_resource: 2 passed; cargo test -p uc-app -- list_entry_projections: 15 passed

## Verification Evidence

| #   | Command                                                               | Exit Code | Verdict | Duration |
| --- | --------------------------------------------------------------------- | --------- | ------- | -------- |
| 1   | `cd src-tauri && cargo test -p uc-app -- get_entry_resource 2>&1`     | 0         | ✅ pass | 66200ms  |
| 2   | `cd src-tauri && cargo test -p uc-app -- list_entry_projections 2>&1` | 0         | ✅ pass | 2900ms   |

## Deviations

None

## Known Issues

None

## Files Created/Modified

- `src-tauri/crates/uc-app/src/usecases/clipboard/get_entry_resource.rs`
- `src-tauri/crates/uc-app/src/usecases/clipboard/list_entry_projections/list_entry_projections.rs`

## Deviations

None

## Known Issues

None
