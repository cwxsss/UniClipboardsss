---
phase: 31
slug: file-sync-ui-dashboard-file-entries-context-menu-progress-notifications
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-13
---

# Phase 31 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property               | Value                                                 |
| ---------------------- | ----------------------------------------------------- |
| **Framework**          | vitest (frontend), cargo test (backend)               |
| **Config file**        | `vitest.config.ts`, `src-tauri/Cargo.toml`            |
| **Quick run command**  | `bun run test -- --run --reporter=verbose`            |
| **Full suite command** | `bun run test -- --run && cd src-tauri && cargo test` |
| **Estimated runtime**  | ~30 seconds                                           |

---

## Sampling Rate

- **After every task commit:** Run `bun run test -- --run --reporter=verbose`
- **After every plan wave:** Run full suite command
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID  | Plan | Wave | Requirement | Test Type | Automated Command                              | File Exists | Status     |
| -------- | ---- | ---- | ----------- | --------- | ---------------------------------------------- | ----------- | ---------- |
| 31-01-01 | 01   | 1    | FSYNC-UI    | unit      | `bun run test -- --run -t "context menu"`      | ❌ W0       | ⬜ pending |
| 31-01-02 | 01   | 1    | FSYNC-UI    | unit      | `bun run test -- --run -t "file entry"`        | ❌ W0       | ⬜ pending |
| 31-02-01 | 02   | 1    | FSYNC-UI    | unit      | `bun run test -- --run -t "transfer progress"` | ❌ W0       | ⬜ pending |
| 31-03-01 | 03   | 2    | FSYNC-UI    | unit      | `bun run test -- --run -t "notification"`      | ❌ W0       | ⬜ pending |

_Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky_

---

## Wave 0 Requirements

- [ ] Test stubs for context menu logic (item state → menu items)
- [ ] Test stubs for transfer progress hook/slice
- [ ] Test stubs for notification batching logic

_Existing vitest infrastructure covers framework requirements._

---

## Manual-Only Verifications

| Behavior                                | Requirement | Why Manual            | Test Instructions                                      |
| --------------------------------------- | ----------- | --------------------- | ------------------------------------------------------ |
| Right-click context menu appears        | FSYNC-UI    | Browser interaction   | Right-click file entry in Dashboard, verify menu shows |
| System notification appears             | FSYNC-UI    | OS-level notification | Trigger file sync, verify system notification          |
| Progress bar animates                   | FSYNC-UI    | Visual rendering      | Start file transfer, verify progress indicator         |
| "Open file location" opens file manager | FSYNC-UI    | Platform-specific     | Click action, verify correct folder opens              |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 30s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
