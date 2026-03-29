---
phase: 28
slug: implement-file-sync-via-libp2p-for-cross-platform-file-copy-paste
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-13
---

# Phase 28 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property               | Value                                               |
| ---------------------- | --------------------------------------------------- |
| **Framework**          | Rust `cargo test` (inline `#[cfg(test)]` modules)   |
| **Config file**        | `src-tauri/Cargo.toml` (existing)                   |
| **Quick run command**  | `cd src-tauri && cargo test -p uc-core`             |
| **Full suite command** | `cd src-tauri && cargo test -p uc-core -p uc-infra` |
| **Estimated runtime**  | ~15 seconds                                         |

---

## Sampling Rate

- **After every task commit:** Run `cd src-tauri && cargo test -p uc-core`
- **After every plan wave:** Run `cd src-tauri && cargo test -p uc-core -p uc-infra`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 15 seconds

---

## Per-Task Verification Map

| Task ID  | Plan | Wave | Requirement      | Test Type | Automated Command                                           | File Exists | Status     |
| -------- | ---- | ---- | ---------------- | --------- | ----------------------------------------------------------- | ----------- | ---------- |
| 28-01-01 | 01   | 1    | FSYNC-FOUNDATION | unit      | `cd src-tauri && cargo test -p uc-core file_transfer`       | ❌ W0       | ⬜ pending |
| 28-01-02 | 01   | 1    | FSYNC-FOUNDATION | unit      | `cd src-tauri && cargo test -p uc-core filename_validation` | ❌ W0       | ⬜ pending |
| 28-02-01 | 02   | 1    | FSYNC-FOUNDATION | unit      | `cd src-tauri && cargo test -p uc-core content_type_filter` | ✅          | ⬜ pending |
| 28-02-02 | 02   | 1    | FSYNC-FOUNDATION | unit      | `cd src-tauri && cargo test -p uc-core settings`            | ✅          | ⬜ pending |
| 28-03-01 | 03   | 2    | FSYNC-FOUNDATION | unit      | `cd src-tauri && cargo test -p uc-core protocol_ids`        | ✅          | ⬜ pending |
| 28-03-02 | 03   | 2    | FSYNC-FOUNDATION | compile   | `cd src-tauri && cargo check -p uc-app`                     | ✅          | ⬜ pending |

_Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky_

---

## Wave 0 Requirements

- Existing infrastructure covers all phase requirements. Tests are inline `#[cfg(test)]` modules in each new file.

---

## Manual-Only Verifications

| Behavior                           | Requirement      | Why Manual              | Test Instructions                                         |
| ---------------------------------- | ---------------- | ----------------------- | --------------------------------------------------------- |
| Database migration applies cleanly | FSYNC-FOUNDATION | Requires SQLite runtime | Run `diesel migration run` in `src-tauri/crates/uc-infra` |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 15s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
