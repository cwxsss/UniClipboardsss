---
phase: 36
slug: event-emitter-abstraction
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-17
---

# Phase 36 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property               | Value                                               |
| ---------------------- | --------------------------------------------------- |
| **Framework**          | cargo test (Rust)                                   |
| **Config file**        | `src-tauri/Cargo.toml`                              |
| **Quick run command**  | `cd src-tauri && cargo test -p uc-core -p uc-tauri` |
| **Full suite command** | `cd src-tauri && cargo test`                        |
| **Estimated runtime**  | ~30 seconds                                         |

---

## Sampling Rate

- **After every task commit:** Run `cd src-tauri && cargo test -p uc-core -p uc-tauri`
- **After every plan wave:** Run `cd src-tauri && cargo test`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID  | Plan | Wave | Requirement | Test Type    | Automated Command                                      | File Exists | Status     |
| -------- | ---- | ---- | ----------- | ------------ | ------------------------------------------------------ | ----------- | ---------- |
| 36-01-01 | 01   | 1    | EVNT-01     | compile      | `cd src-tauri && cargo check -p uc-core`               | ✅          | ⬜ pending |
| 36-01-02 | 01   | 1    | EVNT-01     | unit         | `cd src-tauri && cargo test -p uc-core host_event`     | ❌ W0       | ⬜ pending |
| 36-02-01 | 02   | 1    | EVNT-02     | compile+unit | `cd src-tauri && cargo test -p uc-tauri`               | ❌ W0       | ⬜ pending |
| 36-02-02 | 02   | 1    | EVNT-03     | unit         | `cd src-tauri && cargo test -p uc-tauri logging_event` | ❌ W0       | ⬜ pending |
| 36-03-01 | 03   | 2    | EVNT-04     | compile      | `cd src-tauri && cargo check`                          | ✅          | ⬜ pending |
| 36-03-02 | 03   | 2    | EVNT-04     | integration  | `cd src-tauri && cargo test`                           | ✅          | ⬜ pending |

_Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky_

---

## Wave 0 Requirements

- [ ] Unit tests for `HostEventEmitterPort` trait compilation without Tauri deps
- [ ] Unit tests for `TauriEventEmitter` adapter
- [ ] Unit tests for `LoggingEventEmitter` adapter

_Existing cargo test infrastructure covers compile-time verification._

---

## Manual-Only Verifications

| Behavior                                           | Requirement | Why Manual                  | Test Instructions                                                     |
| -------------------------------------------------- | ----------- | --------------------------- | --------------------------------------------------------------------- |
| Frontend receives clipboard events after migration | EVNT-02     | Requires running GUI app    | 1. `bun tauri dev` 2. Copy text 3. Verify frontend updates            |
| Frontend receives sync events after migration      | EVNT-04     | Requires multi-device setup | 1. Run dual peers 2. Copy on peer A 3. Verify peer B frontend updates |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 30s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
