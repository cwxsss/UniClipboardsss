---
status: diagnosed
trigger: 'pairing-verification-prompt-missing'
created: 2026-03-18T00:00:00+08:00
updated: 2026-03-18T00:00:01+08:00
---

## Current Focus

hypothesis: peerA 在点击 Accept 后，前端 PairingNotificationProvider 因 React 状态提交竞态丢弃了紧随其后的 verification 事件，导致 PIN 弹窗不出现
test: 对照 responder 侧 UserAccept -> ShowVerification -> wiring emit -> PairingNotificationProvider listener 守卫条件
expecting: 后端路径完整，但前端用 activeSessionIdRef 过滤 verification，且该 ref 在 accept 点击后不是同步更新
next_action: 汇总证据并返回根因诊断

## Symptoms

expected: When pairing is initiated between two devices, the verification UI should appear with the correct prompt and state updates, and it should let the user continue or fail explicitly instead of missing the event.
actual: peerB 准备加入 peerA 的空间，peerA 收到配对请求并点击同意后，peerB 一直等待 peerA 的确认结果，没有出现预期的 pin 码确认弹窗。
errors: none reported; symptom is missing verification prompt / missing state transition after peerA accept
reproduction: Test 3 in .planning/phases/37-wiring-decomposition/37-UAT.md
started: Discovered during UAT on 2026-03-18 Asia/Shanghai.

## Eliminated

- hypothesis: HostEvent/Tauri 适配器没有发出 verification 事件
  evidence: responder 侧 `UserAccept` 会生成 `PairingAction::ShowVerification`，`wiring.rs` 会把它映射成 `p2p-pairing-verification`
  timestamp: 2026-03-18T00:00:01+08:00
- hypothesis: peerB setup-state-changed 链路本身缺失，导致 JoinSpaceConfirmPeer 无法出现
  evidence: `SetupActionExecutor::start_pairing_verification_listener` 会把 `PairingVerificationRequired` 转成 `JoinSpaceConfirmPeer` 并发出 `setup-state-changed`，相关 orchestrator/adapter 已有测试覆盖
  timestamp: 2026-03-18T00:00:01+08:00

## Evidence

- timestamp: 2026-03-18T00:00:00+08:00
  checked: initial debug context + Phase 37 UAT
  found: 问题稳定复现于 Test 3，链路聚焦为 peerA 点击 accept 之后 peerB 未进入 PIN 确认弹窗
  implication: 需要优先检查 pairing accept 后的后端事件发射与前端状态推进，而不是设备发现前置流程
- timestamp: 2026-03-18T00:00:01+08:00
  checked: uc-core pairing state machine + uc-app protocol handler
  found: responder 侧 `AwaitingUserApproval + UserAccept` 会立即生成 `PairingAction::ShowVerification`，protocol handler 同时广播 `PairingDomainEvent::PairingVerificationRequired` 并转发 UI action
  implication: peerA 点击 Accept 后 verification 事件应立即出现，后端没有等待额外步骤
- timestamp: 2026-03-18T00:00:01+08:00
  checked: uc-tauri wiring.rs
  found: `run_pairing_action_loop` 直接把 `PairingAction::ShowVerification` 映射为 `HostEvent::Pairing(... kind: Verification ...)` 并发出 `p2p-pairing-verification`
  implication: Tauri 事件发射点存在，事件名与前端监听一致
- timestamp: 2026-03-18T00:00:01+08:00
  checked: frontend PairingNotificationProvider
  found: request toast 的 Accept 只调用 `setActiveSessionId(event.sessionId)`；`activeSessionIdRef.current` 直到后续 `useEffect` 才同步，而 verification 分支要求 `currentSessionId && event.sessionId === currentSessionId`
  implication: verification 若在 React 提交 state 之前到达，会被当前 session 过滤器静默丢弃
- timestamp: 2026-03-18T00:00:01+08:00
  checked: frontend fallback behavior
  found: provider 对丢失的 verification 没有同步写 ref、重放事件或补拉当前配对状态的兜底
  implication: 一旦首个 verification 事件被竞态丢弃，PIN 弹窗不会出现，peerB 只能继续等待 peerA 的后续确认

## Resolution

root_cause: peerA 侧 `PairingNotificationProvider` 在点击 Accept 后用异步 `setActiveSessionId()` 建立会话过滤，但 `verification` 事件可能在该状态提交前立即从后端到达；listener 依赖尚未更新的 `activeSessionIdRef.current` 做守卫，结果把本该打开 PIN 弹窗的事件静默丢弃。
fix: 在 Accept 点击路径中同步建立当前 session（例如先写 ref 再发命令，或放宽 verification 首事件的过滤），并补一个覆盖“request -> accept -> immediate verification”的前端回归测试。
verification: 诊断模式，仅代码路径取证，未实施修复。
files_changed: []
