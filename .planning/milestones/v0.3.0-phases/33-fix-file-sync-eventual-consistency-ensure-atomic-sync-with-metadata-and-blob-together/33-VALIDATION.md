---
phase: 33
slug: fix-file-sync-eventual-consistency-ensure-atomic-sync-with-metadata-and-blob-together
status: ready
nyquist_compliant: true
wave_0_complete: true
created: 2026-03-15
---

# Phase 33 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property               | Value                                                                                                                                    |
| ---------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| **Framework**          | cargo test (Rust), vitest (frontend)                                                                                                     |
| **Config file**        | `src-tauri/Cargo.toml`, `package.json`                                                                                                   |
| **Quick run command**  | `cd src-tauri && cargo test -p uc-app --lib track_inbound_transfers -- --nocapture`                                                      |
| **Full suite command** | `cd src-tauri && cargo test -p uc-app -p uc-infra -p uc-tauri --lib -- --nocapture && cd .. && bun run test -- --run -t "file transfer"` |
| **Estimated runtime**  | ~30 seconds                                                                                                                              |

---

## Sampling Rate

- **After every task commit:** Run the quick run command for the touched crate or targeted Vitest filter
- **After every plan wave:** Run the full suite command
- **Before `$gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID  | Plan | Wave | Requirement       | Test Type   | Automated Command                                                                                | Wave 0           | Status     |
| -------- | ---- | ---- | ----------------- | ----------- | ------------------------------------------------------------------------------------------------ | ---------------- | ---------- |
| 33-01-01 | 01   | 1    | FSYNC-CONSISTENCY | unit        | `cd src-tauri && cargo test -p uc-app --lib -- sync_inbound track_inbound_transfers --nocapture` | inline TDD       | ⬜ pending |
| 33-01-02 | 01   | 1    | FSYNC-CONSISTENCY | unit        | `cd src-tauri && cargo test -p uc-app --lib list_entry_projections -- --nocapture`               | inline TDD       | ⬜ pending |
| 33-02-01 | 02   | 2    | FSYNC-CONSISTENCY | unit        | `cd src-tauri && cargo test -p uc-infra --lib file_transfer_repo -- --nocapture`                 | repo tests       | ⬜ pending |
| 33-02-02 | 02   | 2    | FSYNC-CONSISTENCY | unit        | `cd src-tauri && cargo test -p uc-infra --lib migrations -- --nocapture`                         | repo tests       | ⬜ pending |
| 33-03-01 | 03   | 3    | FSYNC-CONSISTENCY | unit        | `cd src-tauri && cargo test -p uc-tauri --lib wiring -- --nocapture`                             | event-loop tests | ⬜ pending |
| 33-03-02 | 03   | 3    | FSYNC-CONSISTENCY | integration | `cd src-tauri && cargo test -p uc-tauri --test models_serialization_test -- --nocapture`         | test-add         | ⬜ pending |
| 33-04-01 | 04   | 4    | FSYNC-CONSISTENCY | unit        | `bun run test -- --run -t "file transfer status"`                                                | test-add         | ⬜ pending |
| 33-05-01 | 05   | 5    | FSYNC-CONSISTENCY | unit        | `bun run test -- --run -t "Clipboard file state"`                                                | test-add         | ⬜ pending |
| 33-05-02 | 05   | 5    | FSYNC-CONSISTENCY | unit        | `bun run test -- --run -t "Clipboard file actions"`                                              | test-add         | ⬜ pending |

_Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky_

---

## Wave 0 Justification

Wave 0 is satisfied inline by the phase plans:

- **Plan 01** adds app-layer tests for metadata seed, timeout selection, and projection aggregation.
- **Plan 02** adds repository and migration tests in `uc-infra`.
- **Plan 03** adds wiring and serialization tests for `file-transfer://status-changed`, timeout sweeps, and command mapping.
- **Plans 04-05** add frontend tests for durable state hydration, live status updates, and state-aware file actions.

No separate Wave 0 plan is required because every execution plan owns its own automated verification.

---

## Manual-Only Verifications

| Behavior                                                   | Requirement       | Why Manual                       | Test Instructions                                                                                             |
| ---------------------------------------------------------- | ----------------- | -------------------------------- | ------------------------------------------------------------------------------------------------------------- |
| Entry moves through `pending -> transferring -> completed` | FSYNC-CONSISTENCY | Requires real transfer timing    | Send a file from peer A to peer B and observe Dashboard state changes                                         |
| Timeout converts stalled transfer to failed                | FSYNC-CONSISTENCY | Requires wall-clock timing       | Start a transfer, block further chunks, wait past 60-second/5-minute timeout, verify failed state and cleanup |
| Restart marks orphaned in-flight entry as failed           | FSYNC-CONSISTENCY | Requires process restart timing  | Kill receiver during transfer, relaunch, verify failed state appears on same entry                            |
| Delete works for pending/failed entries                    | FSYNC-CONSISTENCY | Requires file-system side effect | Delete the entry from Dashboard and confirm cache/temp files are removed                                      |
| Completed transfer still writes to OS clipboard            | FSYNC-CONSISTENCY | OS integration                   | After completion, paste in Finder/Explorer and verify the received file is available                          |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or inline test ownership
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covered inline by plan-owned tests
- [x] No watch-mode flags
- [x] Feedback latency < 30s
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
