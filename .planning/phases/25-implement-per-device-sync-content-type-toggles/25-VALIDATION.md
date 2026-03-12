---
phase: 25
slug: implement-per-device-sync-content-type-toggles
status: draft
nyquist_compliant: false
wave_0_complete: false
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

| Task ID  | Plan | Wave | Requirement | Test Type | Automated Command                                               | File Exists | Status     |
| -------- | ---- | ---- | ----------- | --------- | --------------------------------------------------------------- | ----------- | ---------- |
| 25-01-01 | 01   | 0    | P25-01      | unit      | `cd src-tauri && cargo test -p uc-core classify_snapshot`       | ❌ W0       | ⬜ pending |
| 25-01-02 | 01   | 0    | P25-02      | unit      | `cd src-tauri && cargo test -p uc-core is_content_type_allowed` | ❌ W0       | ⬜ pending |
| 25-01-03 | 01   | 0    | P25-03      | unit      | `cd src-tauri && cargo test -p uc-app apply_sync_policy`        | ❌ W0       | ⬜ pending |
| 25-01-04 | 01   | 0    | P25-05      | unit      | `bun test DeviceSettingsPanel`                                  | ❌ W0       | ⬜ pending |
| 25-02-01 | 02   | 1    | P25-01      | unit      | `cd src-tauri && cargo test -p uc-core classify_snapshot`       | ❌ W0       | ⬜ pending |
| 25-02-02 | 02   | 1    | P25-02      | unit      | `cd src-tauri && cargo test -p uc-core is_content_type_allowed` | ❌ W0       | ⬜ pending |
| 25-03-01 | 03   | 1    | P25-03      | unit      | `cd src-tauri && cargo test -p uc-app apply_sync_policy`        | ❌ W0       | ⬜ pending |
| 25-03-02 | 03   | 1    | P25-04      | unit      | `cd src-tauri && cargo test -p uc-app sync_outbound`            | ❌ W0       | ⬜ pending |
| 25-04-01 | 04   | 2    | P25-05      | unit      | `bun test DeviceSettingsPanel`                                  | ❌ W0       | ⬜ pending |
| 25-04-02 | 04   | 2    | P25-06      | unit      | `bun test DeviceSettingsPanel`                                  | ❌ W0       | ⬜ pending |
| 25-04-03 | 04   | 2    | P25-07      | unit      | `bun test DeviceSettingsPanel`                                  | ❌ W0       | ⬜ pending |

_Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky_

---

## Wave 0 Requirements

- [ ] Content type classification unit tests in `uc-core` (classify_snapshot, is_content_type_allowed)
- [ ] Sync policy filter tests with content type scenarios in `uc-app` (apply_sync_policy)
- [ ] Updated `DeviceSettingsPanel.test.tsx` — fix stale references to removed permissions section

_Wave 0 creates test stubs that subsequent waves fill with implementation._

---

## Manual-Only Verifications

| Behavior                                        | Requirement | Why Manual       | Test Instructions                                                    |
| ----------------------------------------------- | ----------- | ---------------- | -------------------------------------------------------------------- |
| Toggle visual disabled state when auto_sync off | P25-05      | CSS visual state | Toggle auto_sync off, verify content type toggles appear grayed out  |
| "Coming Soon" badge styling                     | P25-06      | CSS visual       | Verify badge renders correctly in both light/dark themes             |
| All-disabled warning text                       | P25-07      | Visual layout    | Disable all content types, verify warning text appears below toggles |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 30s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
