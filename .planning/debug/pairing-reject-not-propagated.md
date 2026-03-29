---
status: diagnosed
trigger: 'pairing-reject-not-propagated'
created: 2026-03-18T00:00:00+08:00
updated: 2026-03-18T00:24:00+08:00
---

## Current Focus

hypothesis: 初始请求阶段的 remote reject 没有可靠的失败回流兜底；一旦 `Reject` 终止帧未被上层消费，pairing stream 的 clean close 不会发出 `NetworkEvent::PairingFailed`，导致 setup 永远收不到 `JoinSpaceFailed`。
test: 汇总 setup listener、pairing domain event、pairing stream service 的关闭语义，确认只有 error-close 才会上抛 `PairingFailed`，而 clean close/EOF 不会上抛。
expecting: 这能解释为什么 B 端 spinner 只在初始 reject 场景卡住，而不是所有 pairing 失败都卡住。
next_action: return diagnosis

## Symptoms

expected: If pairing or discovery subscription temporarily fails and then recovers, the UI should show failure or retry feedback and later return to a healthy pairing/discovery state without requiring restart.
actual: peerB 向 peerA 发起配对请求, peerA 弹出确认弹窗。在 A 点击 reject 后, B 没有报错, 仍停留在等待步骤转圈。对照分支: 如果 A 点击接受后 B 点击取消, A 会正确显示对方拒绝, 该分支符合预期。
errors: none surfaced to B UI; asymmetry between A reject -> B no feedback and B cancel -> A feedback correct
reproduction: Test 4 in 37-UAT.md
started: Discovered during UAT on 2026-03-18 Asia/Shanghai.

## Eliminated

- hypothesis: A 侧全局 pairing 弹窗与 B 侧 setup 取消走了不同 backend reject 入口
  evidence: `reject_p2p_pairing` 与 setup 的 `cancel_setup -> AbortPairing` 最终都调用 `PairingOrchestrator::user_reject_pairing`
  timestamp: 2026-03-18T00:24:00+08:00

## Evidence

- timestamp: 2026-03-18T00:05:00+08:00
  checked: knowledge base
  found: no existing knowledge base entry
  implication: no prior resolved pattern to reuse; must trace current code path directly
- timestamp: 2026-03-18T00:11:00+08:00
  checked: src-tauri/crates/uc-app/src/usecases/setup/action_executor.rs
  found: setup join flow leaves ProcessingJoinSpace only when pairing listener receives `PairingDomainEvent::PairingFailed` and maps it to `SetupEvent::JoinSpaceFailed`
  implication: if pairing failure event is not delivered, frontend will remain in spinner state exactly as reported
- timestamp: 2026-03-18T00:11:00+08:00
  checked: src-tauri/crates/uc-app/src/usecases/pairing/protocol_handler.rs and src-tauri/crates/uc-core/src/network/pairing_state_machine.rs
  found: both `RecvReject` and `RecvCancel` transitions call `cancel_with_reason`, which always emits `PairingAction::EmitResult { success: false }`; protocol handler then converts that into `PairingDomainEvent::PairingFailed`
  implication: reject/cancel asymmetry is not introduced in setup state machine or pairing domain event mapping
- timestamp: 2026-03-18T00:11:00+08:00
  checked: src/pages/SetupPage.tsx and src/pages/setup/PairingConfirmStep.tsx
  found: confirm page cancel invokes `cancelSetup()`, which backend maps to `AbortPairing -> user_reject_pairing`, not a separate `user_cancel_pairing`
  implication: the observed “B cancel 后 A 正常提示拒绝” is actually another reject path, but from a later pairing state than “A 初始拒绝 B”
- timestamp: 2026-03-18T00:18:00+08:00
  checked: cargo test -p uc-app pairing_verification_listener_emits_join_space_failed_event_on_pairing_failure
  found: the only existing setup-side failure test covers synthetic transport error (`handle_transport_error("stream closed")`) and passes
  implication: current automated coverage proves setup reacts to generic pairing failure, but does not exercise real remote `PairingMessage::Reject` propagation from the initial request phase
- timestamp: 2026-03-18T00:24:00+08:00
  checked: src-tauri/crates/uc-tauri/src/commands/pairing.rs and src/components/PairingNotificationProvider.tsx
  found: A 侧全局 pairing 拒绝按钮调用 `reject_p2p_pairing -> user_reject_pairing`；setup 页取消也调用 `user_reject_pairing`
  implication: user-visible asymmetry is not caused by different reject command implementations
- timestamp: 2026-03-18T00:24:00+08:00
  checked: src-tauri/crates/uc-platform/src/adapters/pairing_stream/service.rs and service_test.rs
  found: pairing stream only emits `NetworkEvent::PairingFailed` on error termination; `StreamClosedByPeer`/early EOF are treated as clean shutdown and emit no failure event
  implication: if the responder-side reject terminal frame is not observed before the stream closes, the initiator side receives neither `PairingMessage::Reject` nor fallback `PairingFailed`, so setup remains stuck in `ProcessingJoinSpace`

## Resolution

root_cause: Inferred from code: the setup join flow has a hard dependency on receiving `PairingDomainEvent::PairingFailed`, but the transport layer only surfaces failure on error-close. In the “A rejects B’s initial request” path, the initiator is still in `RequestSent` and depends on a single remote `Reject` frame. If that terminal frame is not consumed before the responder closes the pairing stream, `PairingStreamService` treats the close as clean (`StreamClosedByPeer` / early EOF) and emits no `NetworkEvent::PairingFailed`, so `SetupActionExecutor` never emits `JoinSpaceFailed` back to the UI.
fix:
verification: static trace + targeted test review; existing automated coverage only validates synthetic transport-error propagation, not real remote reject propagation from initial request phase
files_changed: []
