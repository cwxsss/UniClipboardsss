---
status: diagnosed
trigger: 'Issue slug: join-space-cannot-complete; Expected: After completing create-space or join-space, the app should leave the processing state and reach the completion or ready screen with relevant peer/device context visible. Actual: create-space 正常; join-space 因前面的配对确认问题无法到最后一步, 测试失败。 Severity: major. Reproduction: Test 5 in 37-UAT.md. Timeline: Discovered during UAT on 2026-03-18 Asia/Shanghai.'
created: 2026-03-17T16:11:26Z
updated: 2026-03-17T16:13:12Z
---

## Current Focus

hypothesis: 已确认 Test 5 失败是前序 join 配对确认事件未到达/未被消费导致的下游结果，不是独立的 join 完成态缺陷
test: 已完成 setup 状态机、pairing verification listener、space access completion listener、前端 setup state 消费与 join 成功测试的交叉核对
expecting: join-space 若无法进入 JoinSpaceConfirmPeer，就无法进入 JoinSpaceInputPassphrase，更无法启动 space access，因此不会到达 Completed
next_action: 输出诊断结论

## Symptoms

expected: After completing create-space or join-space, the app should leave the processing state and reach the completion or ready screen with relevant peer/device context visible.
actual: create-space 正常; join-space 因前面的配对确认问题无法到最后一步, 测试失败。
errors: 无新增错误文本；上游相关失败为“peerB 一直在等待 peerA 的确认结果,没有如预期那样出现 pin 码确认弹窗”
reproduction: Test 5 in .planning/phases/37-wiring-decomposition/37-UAT.md
started: Discovered during UAT on 2026-03-18 Asia/Shanghai.

## Eliminated

## Evidence

- timestamp: 2026-03-17T16:11:26Z
  checked: .planning/phases/37-wiring-decomposition/37-UAT.md
  found: Test 5 明确写明 “join-space 因为上方反馈的问题,无法到最后一步”；上方对应 Test 3/4 的配对确认/失败反馈问题
  implication: UAT 现场证据已将 join-space 无法完成描述为前序问题阻断后的后果

- timestamp: 2026-03-17T16:11:26Z
  checked: src-tauri/crates/uc-core/src/setup/state_machine.rs
  found: join 流程必须按 JoinSpaceSelectDevice -> ProcessingJoinSpace(连接) -> JoinSpaceConfirmPeer -> JoinSpaceInputPassphrase -> ProcessingJoinSpace(验证口令) -> Completed 推进；Completed 仅在 ProcessingJoinSpace 收到 JoinSpaceSucceeded 后产生
  implication: 如果流程在配对确认前卡住，join 完成态在状态机上根本不可达

- timestamp: 2026-03-17T16:11:26Z
  checked: src-tauri/crates/uc-app/src/usecases/setup/action_executor.rs
  found: 只有收到 PairingDomainEvent::PairingVerificationRequired 时，listener 才会把状态改成 JoinSpaceConfirmPeer；只有用户 confirm_peer_trust 后才会进入 JoinSpaceInputPassphrase；只有提交口令后才会启动 StartJoinSpaceAccess 和后续 JoinSpaceSucceeded/Failed 映射
  implication: 前序配对确认事件缺失会阻断整个 join 完成链，不存在绕过该步骤直接完成 join 的路径

- timestamp: 2026-03-17T16:11:26Z
  checked: src-tauri/crates/uc-app/src/usecases/setup/orchestrator.rs
  found: 集成测试 setup_completes_after_access_granted_result_arrives 覆盖了 join 流程在 access granted 后转换到 Completed 的行为
  implication: join 完成链路在应用层已有成功证据；当前失败更像前置事件没到，而不是完成态自身有独立断裂

- timestamp: 2026-03-17T16:11:26Z
  checked: src/pages/SetupPage.tsx, src/api/setup.ts, src/pages/setup/SetupDoneStep.tsx
  found: 前端会渲染 JoinSpaceConfirmPeer、JoinSpaceInputPassphrase、ProcessingJoinSpace 和 Completed；setup-state-changed 事件 payload 使用 sessionId camelCase，Completed 会直接渲染 SetupDoneStep
  implication: 在收到正确 setup 状态事件的前提下，前端具备展示 join 完成态的路径；本次失败点发生在更早的配对确认阶段

## Resolution

root_cause:
Test 5 “join-space cannot complete” 不是独立根因，而是前序配对确认事件失败的下游结果。join 只有在收到 PairingVerificationRequired 并推进到 JoinSpaceConfirmPeer、随后确认信任并提交口令后，才会启动 space access 并最终进入 Completed。当前 UAT 中 peerB 一直等待 peerA 确认且没有出现 pin 确认弹窗，导致流程停在配对前半段，后续 JoinSpaceSucceeded 根本无从发生。
fix:
verification:
files_changed: []
