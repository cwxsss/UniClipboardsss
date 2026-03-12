---
phase: 25
slug: implement-per-device-sync-content-type-toggles
status: draft
nyquist_compliant: true
wave_0_complete: true
created: 2026-03-12
---

# Phase 25 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property               | Value                                                   |
| ---------------------- | ------------------------------------------------------- |
| **Framework**          | Rust: cargo test; Frontend: Vitest                      |
| **Config file**        | src-tauri/Cargo.toml; vitest.config.ts                  |
| **Quick run command**  | `cd src-tauri && cargo test -p uc-core -p uc-app --lib` |
| **Full suite command** | `cd src-tauri && cargo test && cd .. && bun test`       |
| **Estimated runtime**  | ~30 seconds                                             |

---

## Sampling Rate

- **After every task commit:** Run `cd src-tauri && cargo test -p uc-core -p uc-app --lib`
- **After every plan wave:** Run `cd src-tauri && cargo test && cd .. && bun test`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID  | Plan | Wave | Requirement         | Test Type | Automated Command                                                                    | Wave 0     | Status     |
| -------- | ---- | ---- | ------------------- | --------- | ------------------------------------------------------------------------------------ | ---------- | ---------- |
| 25-01-01 | 01   | 1    | CT-01, CT-02        | unit      | `cd src-tauri && cargo test -p uc-core settings::content_type_filter -- --nocapture` | inline TDD | ⬜ pending |
| 25-01-02 | 01   | 1    | CT-03, CT-04        | unit      | `cd src-tauri && cargo test -p uc-app --lib usecases::clipboard::sync_outbound`      | inline TDD | ⬜ pending |
| 25-02-01 | 02   | 1    | CT-05, CT-06        | build     | `cd /home/wuy6/myprojects/UniClipboard && bun run build`                             | n/a (UI)   | ⬜ pending |
| 25-02-02 | 02   | 1    | CT-05, CT-06, CT-07 | unit      | `cd /home/wuy6/myprojects/UniClipboard && bun test DeviceSettingsPanel`              | test-fix   | ⬜ pending |

_Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky_

---

## Wave 0 Justification

Wave 0 is satisfied inline within the 2 existing plans:

- **Plan 25-01** (Tasks 1 and 2): Both tasks have `tdd="true"` — tests are written RED before implementation (GREEN). This satisfies the Nyquist requirement for backend logic.
- **Plan 25-02** (Task 2): Fixes stale test expectations and adds new behavioral tests for the UI changes made in Task 1. This provides test coverage for frontend changes.

No separate Wave 0 plan is needed.

---

## Manual-Only Verifications

| Behavior                                        | Requirement | Why Manual       | Test Instructions                                                    |
| ----------------------------------------------- | ----------- | ---------------- | -------------------------------------------------------------------- |
| Toggle visual disabled state when auto_sync off | CT-05       | CSS visual state | Toggle auto_sync off, verify content type toggles appear grayed out  |
| "Coming Soon" badge styling                     | CT-06       | CSS visual       | Verify badge renders correctly in both light/dark themes             |
| All-disabled warning text                       | CT-07       | Visual layout    | Disable all content types, verify warning text appears below toggles |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or inline TDD
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covered by inline TDD (25-01) and test-fix task (25-02)
- [x] No watch-mode flags
- [x] Feedback latency < 30s
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
