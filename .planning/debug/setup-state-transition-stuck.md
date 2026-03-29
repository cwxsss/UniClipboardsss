---
status: diagnosed
trigger: 'Phase 37-wiring-decomposition diagnose-only: setup-state-transition-stuck'
created: 2026-03-17T16:12:22Z
updated: 2026-03-17T16:12:22Z
---

## Current Focus

hypothesis: join-space 在 `select_device` 之后对 pairing 领域事件的订阅存在竞态，导致 `PairingVerificationRequired` / `PairingFailed` 在 setup 监听器挂上之前已经发出并丢失
test: 对照 `ensure_pairing_session -> initiate_pairing -> subscribe` 顺序、`PairingEventPort::subscribe` 实现、以及 frontend 实际挂载的 pairing/setup UI 消费链路
expecting: 若 subscribe 非回放且晚于 initiate，则同机快速 accept/reject 会让 setup 一直停在 `ProcessingJoinSpace`，并同时解释 Test 2/3/4/5
next_action: return diagnosis

## Symptoms

expected: From a fresh or unpaired state, moving through setup should visibly advance step state; processing screens should transition to next step or confirmation state instead of staying stuck silently.
actual: 用户结论为“无法加入空间, 整体测试失败”。已知 create-space 正常，但 join-space 在处理中/等待链路出现卡住。
errors: 无显式前端报错；join-space 在处理中/等待确认时卡住，未出现预期确认弹窗
reproduction: Test 2 in .planning/phases/37-wiring-decomposition/37-UAT.md
started: Discovered during UAT on 2026-03-18 Asia/Shanghai.

## Eliminated

## Evidence

- timestamp: 2026-03-17T16:12:22Z
  checked: required context files listed in prompt
  found: SetupPage only advances join-space UI from `setup-state-changed` events / command returns; PairingConfirmStep renders only when state becomes `JoinSpaceConfirmPeer`
  implication: if backend emits malformed setup-state payload or frontend rejects it, join flow will stay on processing/waiting UI without explicit error

- timestamp: 2026-03-17T16:12:22Z
  checked: `src-tauri/crates/uc-app/src/usecases/setup/action_executor.rs`
  found: `ensure_pairing_session()` calls `pairing_orchestrator.initiate_pairing(peer_id)` first, stores the session id, and only then calls `start_pairing_verification_listener()` to subscribe to pairing domain events
  implication: remote accept/reject can happen before the setup listener exists

- timestamp: 2026-03-17T16:12:22Z
  checked: `src-tauri/crates/uc-app/src/usecases/pairing/orchestrator.rs` and `protocol_handler.rs`
  found: `PairingEventPort::subscribe()` just appends a new `mpsc::Sender` to `event_senders`; `emit_event_to_senders()` only iterates current senders and has no replay/sticky state
  implication: any `PairingVerificationRequired` or `PairingFailed` emitted before subscription is permanently lost

- timestamp: 2026-03-17T16:12:22Z
  checked: frontend pairing/setup consumers in `src/App.tsx`, `src/pages/SetupPage.tsx`, `src/components/PairingNotificationProvider.tsx`, `src/components/PairingDialog.tsx`
  found: app mounts `PairingNotificationProvider` only; `PairingDialog` is not mounted anywhere outside tests, while join-space confirmation in setup depends on `SetupPage` receiving `setup-state-changed -> JoinSpaceConfirmPeer`
  implication: join-space initiator path has no second mounted UI path to recover if setup misses the pairing domain event; Test 2 and Test 3 are the same failure domain

- timestamp: 2026-03-17T16:12:22Z
  checked: backend event contract in `src-tauri/crates/uc-tauri/src/adapters/host_event_emitter.rs`, `src-tauri/crates/uc-core/src/setup/state.rs`, and setup command tests in `src-tauri/crates/uc-tauri/src/commands/setup.rs`
  found: current code serializes `SetupState` and top-level `sessionId` consistently enough for `JoinSpaceConfirmPeer` payloads; no contradictory evidence of a distinct frontend state-mapping bug in the IPC shape
  implication: observed UAT failures are better explained by missed async events before subscription than by a separate JSON/state mapping defect

## Resolution

root_cause: `select_device`/join-space starts pairing before attaching the setup-side pairing event subscriber. Because pairing domain subscriptions are plain `mpsc` listeners with no replay, fast `PairingVerificationRequired` and `PairingFailed` events from peerA are dropped on peerB. The setup context never leaves `ProcessingJoinSpace`, so the joiner does not see `JoinSpaceConfirmPeer` or rejection feedback. This is the same root cause as the missing pairing verification prompt, not a separate frontend state-mapping gap.
fix:
verification: diagnosis only; root cause established by code-path inspection across setup action ordering, pairing event subscription semantics, and mounted frontend consumers
files_changed: []
