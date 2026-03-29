---
phase: 30
slug: file-transfer-service-chunked-protocol-use-cases-retry-logic
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-13
---

# Phase 30 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property               | Value                                                                                                   |
| ---------------------- | ------------------------------------------------------------------------------------------------------- |
| **Framework**          | cargo test (Rust)                                                                                       |
| **Config file**        | `src-tauri/Cargo.toml`                                                                                  |
| **Quick run command**  | `cd src-tauri && cargo test -p uc-platform --lib file_transfer -- --nocapture 2>&1 \| head -50`         |
| **Full suite command** | `cd src-tauri && cargo test -p uc-platform -p uc-app -p uc-core --lib -- --nocapture 2>&1 \| head -100` |
| **Estimated runtime**  | ~30 seconds                                                                                             |

---

## Sampling Rate

- **After every task commit:** Run quick run command (file_transfer tests)
- **After every plan wave:** Run full suite command (all affected crates)
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID  | Plan | Wave | Requirement    | Test Type | Automated Command                                                         | File Exists | Status     |
| -------- | ---- | ---- | -------------- | --------- | ------------------------------------------------------------------------- | ----------- | ---------- |
| 30-01-01 | 01   | 1    | FSYNC-TRANSFER | unit      | `cd src-tauri && cargo test -p uc-platform --lib file_transfer`           | ❌ W0       | ⬜ pending |
| 30-01-02 | 01   | 1    | FSYNC-TRANSFER | unit      | `cd src-tauri && cargo test -p uc-platform --lib file_transfer::protocol` | ❌ W0       | ⬜ pending |
| 30-02-01 | 02   | 1    | FSYNC-TRANSFER | unit      | `cd src-tauri && cargo test -p uc-app --lib file_sync`                    | ❌ W0       | ⬜ pending |
| 30-02-02 | 02   | 1    | FSYNC-TRANSFER | unit      | `cd src-tauri && cargo test -p uc-app --lib file_sync::sync_outbound`     | ❌ W0       | ⬜ pending |
| 30-03-01 | 03   | 2    | FSYNC-TRANSFER | unit      | `cd src-tauri && cargo test -p uc-platform --lib file_transfer::queue`    | ❌ W0       | ⬜ pending |
| 30-03-02 | 03   | 2    | FSYNC-TRANSFER | unit      | `cd src-tauri && cargo test -p uc-platform --lib file_transfer::retry`    | ❌ W0       | ⬜ pending |

_Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky_

---

## Wave 0 Requirements

- [ ] `src-tauri/crates/uc-platform/src/adapters/file_transfer/` — module directory created
- [ ] `src-tauri/crates/uc-app/src/usecases/file_sync/` — module directory created
- [ ] blake3 dependency added to Cargo.toml

_Existing cargo test infrastructure covers all phase requirements. No new framework needed._

---

## Manual-Only Verifications

| Behavior                                   | Requirement    | Why Manual                           | Test Instructions                                                                                   |
| ------------------------------------------ | -------------- | ------------------------------------ | --------------------------------------------------------------------------------------------------- |
| File transfer between two physical devices | FSYNC-TRANSFER | Requires two devices on LAN          | 1. Copy file on device A, 2. Verify file appears on device B's file-cache, 3. Check Dashboard entry |
| Temp file cleanup on hash mismatch         | FSYNC-TRANSFER | Requires corrupted stream simulation | Unit test covers logic; manual confirms OS-level behavior                                           |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 30s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
