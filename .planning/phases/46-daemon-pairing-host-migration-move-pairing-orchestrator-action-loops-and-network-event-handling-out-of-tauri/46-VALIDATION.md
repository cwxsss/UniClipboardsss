---
phase: 46
slug: daemon-pairing-host-migration-move-pairing-orchestrator-action-loops-and-network-event-handling-out-of-tauri
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-19
---

# Phase 46 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property               | Value                                                                                                                  |
| ---------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| **Framework**          | Rust `cargo test`                                                                                                      |
| **Config file**        | `src-tauri/Cargo.toml`                                                                                                 |
| **Quick run command**  | `cd src-tauri && cargo test -p uc-daemon --test pairing_host --test pairing_api --test pairing_ws -- --test-threads=1` |
| **Full suite command** | `cd src-tauri && cargo test -p uc-daemon -p uc-tauri -- --test-threads=1`                                              |
| **Estimated runtime**  | ~120 seconds                                                                                                           |

---

## Sampling Rate

- **After every task commit:** Run the task-local command from the verification map below
- **After every plan wave:** Run `cd src-tauri && cargo test -p uc-daemon -p uc-tauri -- --test-threads=1`
- **Before `$gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** 120 seconds

---

## Per-Task Verification Map

| Task ID  | Plan | Wave | Requirement | Test Type            | Automated Command                                                                                                                           | File Exists | Status     |
| -------- | ---- | ---- | ----------- | -------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- | ----------- | ---------- |
| 46-01-01 | 01   | 1    | PH46-01     | integration          | `cd src-tauri && cargo test -p uc-daemon --test pairing_host -- daemon_pairing_host_enforces_single_active_session --test-threads=1`        | ❌ W0       | ⬜ pending |
| 46-01-02 | 01   | 1    | PH46-02     | integration          | `cd src-tauri && cargo test -p uc-daemon --test pairing_host -- daemon_pairing_host_survives_client_disconnect --test-threads=1`            | ❌ W0       | ⬜ pending |
| 46-02-01 | 02   | 2    | PH46-03     | integration          | `cd src-tauri && cargo test -p uc-daemon --test pairing_api -- pairing_mutations_ack_immediately_and_require_session_id --test-threads=1`   | ❌ W0       | ⬜ pending |
| 46-02-02 | 02   | 2    | PH46-04     | integration          | `cd src-tauri && cargo test -p uc-daemon --test pairing_ws -- pairing_ws_hides_verification_secrets_from_summary_payloads --test-threads=1` | ❌ W0       | ⬜ pending |
| 46-03-01 | 03   | 3    | PH46-05     | integration          | `cd src-tauri && cargo test -p uc-tauri --test pairing_bridge -- bridge_preserves_existing_pairing_event_contract --test-threads=1`         | ❌ W0       | ⬜ pending |
| 46-03-02 | 03   | 3    | PH46-06     | integration + manual | `cd src-tauri && cargo test -p uc-tauri --test pairing_bridge -- bridge_keeps_setup_flow_semantics --test-threads=1`                        | ❌ W0       | ⬜ pending |

_Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky_

---

## Wave 0 Requirements

- [ ] `src-tauri/crates/uc-daemon/tests/pairing_host.rs` — host ownership, single-session gate, and disconnect continuity coverage
- [ ] `src-tauri/crates/uc-daemon/tests/pairing_api.rs` — daemon pairing mutation/control surface coverage
- [ ] `src-tauri/crates/uc-daemon/tests/pairing_ws.rs` — realtime contract and secret-boundary coverage
- [ ] `src-tauri/crates/uc-tauri/tests/pairing_bridge.rs` — Tauri compatibility bridge contract coverage

---

## Manual-Only Verifications

| Behavior                                                                                               | Requirement | Why Manual                                                         | Test Instructions                                                                                                                                                                                                                                                     |
| ------------------------------------------------------------------------------------------------------ | ----------- | ------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Daemon-kept session outlives Tauri/webview disconnect but UI does not auto-resume it                   | PH46-02     | Requires real desktop shell lifecycle and reconnect timing         | Start pairing from desktop app, reach verification or waiting state, restart or detach the Tauri/webview host, confirm daemon session remains active until timeout or terminal result, then reopen UI and confirm there is no automatic session-resume UX in Phase 46 |
| Verification codes and fingerprints remain absent from normal session reads                            | PH46-04     | Requires cross-checking realtime path against HTTP session summary | Start a pairing flow, call `/pairing/sessions/{sessionId}` through an authenticated local client, confirm response omits short code and fingerprints, then inspect authenticated realtime pairing updates to confirm verification data is delivered only there        |
| Setup flow still reaches the expected pairing-confirm/join-space transitions through the daemon bridge | PH46-06     | Requires end-to-end setup navigation in desktop shell              | Launch desktop app in setup flow, join a peer through the existing UI, confirm stage transitions still follow `request -> verification -> verifying -> complete/failed` semantics and join-space path is not skipped or duplicated                                    |

---

## Validation Sign-Off

- [ ] All tasks have `<acceptance_criteria>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 120s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
