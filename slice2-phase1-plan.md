# Slice 2 Phase 1 · roster + presence 基础设施 —— 细化计划

> **状态**:计划稿(2026-04-20),待用户过目后开工
> **父文档**:`task_plan.md` 的 Slice 2 章节 + Slice 2 Phase 1 子段
> **前置**:Slice 1 ✅ + T-15 ✅(commit `255fd2fe`)

---

## 1. 目标复述

让两台已配对设备**互相知道对方在不在线**。不做剪贴板同步,不接 rename / revoke UI 按钮,不写新 wire 协议。

**验收(已在 task_plan.md 锁定)**:
1. 两设备都 unlock 后,任一跑 `uniclipboard-cli members` 能列出所有 SpaceMember + online/offline
2. 关掉 B → ≤ 10s 内 A 的 `members` 反映 B = offline
3. 重启 B + unlock → ≤ 10s 内 A 的 `members` 反映 B = online
4. 单元测试覆盖 facade + `ensure_reachable_all` 并发安全

> **命名说明**:CLI 子命令改用 `members` 而非 task_plan.md 初稿的 `status`——后者在 `uc-cli/src/commands/status.rs` 已被 legacy 的 daemon HTTP 状态查询占用。Slice 5 删 libp2p 后再统一。

---

## 2. 架构分层(新建 / 扩展对照)

```
uc-cli
  └── commands/members.rs          🆕 自包含的 members 子命令(无 daemon)
      └→ build_slice1_cli_context  ♻️ 复用
      └→ MemberRosterFacade        🆕 uc-application

uc-application
  ├── facade/roster/               🆕 目录
  │     ├── facade.rs              🆕 MemberRosterFacade
  │     ├── commands.rs            🆕 ListWithPresenceQuery / RosterEntry / PresenceEvent
  │     ├── errors.rs              🆕 RosterError
  │     └── mod.rs                 🆕
  └── usecases/presence/           🆕 目录
        ├── ensure_reachable_all.rs 🆕 EnsureReachableAllUseCase
        └── mod.rs                 🆕

uc-core
  └── ports/
      ├── presence.rs              🆕 PresencePort(非 legacy 的新 trait)
      └── peer_address.rs          🆕 PeerAddressRepositoryPort
      └── mod.rs                   ✏️ 挂接两个新 mod

uc-infra
  ├── network/iroh/
  │     ├── node.rs                ✏️ 新增 install_presence 扩展点
  │     └── presence_adapter.rs    🆕 IrohPresenceAdapter(PresencePort 实现)
  └── storage/
        └── peer_address_repo.rs   🆕 SqlitePeerAddressRepository

uc-bootstrap
  ├── assembly.rs                  ✏️ WiredDependencies 加 roster_facade / peer_address_repo
  └── space_setup.rs               ✏️ 装配 MemberRosterFacade;F1 unlock 后触发 ensure_reachable_all
```

**Legacy 保留**:`uc-core/src/ports/{discovery,peer_directory}.rs`(libp2p 时代)**不碰**,Slice 5 统一删。

---

## 3. 新 port 契约草图

### 3.1 `PresencePort`(uc-core)

```rust
// uc-core/src/ports/presence.rs

use crate::membership::MemberId;
use crate::network::PeerAddressRecord;
use async_trait::async_trait;
use tokio::sync::broadcast;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReachabilityState {
    Online,         // 有活跃 iroh 连接或最近探测成功
    Offline,        // 无连接 / 探测失败
    Unknown,        // 尚未探测过
}

#[derive(Debug, Clone)]
pub struct PresenceEvent {
    pub member_id: MemberId,
    pub state: ReachabilityState,
    pub at: crate::time::Timestamp,
}

#[async_trait]
pub trait PresencePort: Send + Sync {
    /// 主动探测某成员,可能触发拨号。
    /// 成功返回当前状态;失败(无地址、被拒)返回 Offline。
    async fn ensure_reachable(&self, member: &MemberId) -> Result<ReachabilityState, PresenceError>;

    /// 批量探测。内部并发执行(见 EnsureReachableAllUseCase)。
    /// adapter 只暴露单点 API,"全员"由 usecase 聚合——保持 port 最小。

    /// 不拨号,读当前快照。
    async fn current_state(&self, member: &MemberId) -> ReachabilityState;

    /// 订阅 presence 变化事件(上线 / 下线 / 从 Unknown 首次确认)。
    fn subscribe(&self) -> broadcast::Receiver<PresenceEvent>;
}

#[derive(Debug, thiserror::Error)]
pub enum PresenceError {
    #[error("no known address for member {0:?}")]
    NoAddress(MemberId),
    #[error("internal: {0}")]
    Internal(String),
}
```

**关键决策**:
- `ensure_reachable` 只做**单点**,`ensure_reachable_all` 是 **application 层 usecase** 的聚合(见 §3.3)——port 保持最小原则
- `subscribe` 返回 broadcast receiver,与 Slice 1 `PairingOutcome` pattern 一致
- 状态枚举只 3 态;**不**引入 `Connecting` / `Degraded` 之类的中间态(YAGNI,Slice 2 不需要)

### 3.2 `PeerAddressRepositoryPort`(uc-core)

```rust
// uc-core/src/ports/peer_address.rs

use crate::membership::MemberId;
use async_trait::async_trait;

/// Domain-neutral wrapper. Adapter 内部把 iroh::NodeAddr 序列化进 blob,
/// 上层只持有不透明字节——iroh 类型不上浮。
#[derive(Debug, Clone)]
pub struct PeerAddressRecord {
    pub member_id: MemberId,
    pub addr_blob: Vec<u8>,        // postcard-encoded NodeAddr
    pub observed_at: Timestamp,
}

#[async_trait]
pub trait PeerAddressRepositoryPort: Send + Sync {
    async fn get(&self, member: &MemberId) -> Result<Option<PeerAddressRecord>, PeerAddressError>;
    async fn upsert(&self, record: &PeerAddressRecord) -> Result<(), PeerAddressError>;
    async fn list(&self) -> Result<Vec<PeerAddressRecord>, PeerAddressError>;
    async fn remove(&self, member: &MemberId) -> Result<(), PeerAddressError>;
}
```

**地址来源**:Slice 1 pairing 完成那一刻,sponsor 和 joiner 都已知对方的 `iroh::NodeAddr`——需要在 `pairing_inbound` / `pairing_outbound` 的收尾点调 `peer_address_repo.upsert`。这一条同时属于 Phase 1 改动(不是"已有骨架")。

---

## 4. 新 facade + usecase

### 4.1 `MemberRosterFacade`(uc-application)

```rust
pub struct RosterEntry {
    pub member_id: MemberId,
    pub device_name: String,
    pub device_id: DeviceId,
    pub is_local: bool,                   // 本机
    pub state: ReachabilityState,
    pub last_seen_at: Option<Timestamp>,  // last state == Online 的时间
}

impl MemberRosterFacade {
    pub async fn list_with_presence(&self) -> Result<Vec<RosterEntry>, RosterError> {
        // 1. member_repo.list(space_id)
        // 2. 对每个 member,presence.current_state(member_id)(不拨号,只读快照)
        // 3. 组装 RosterEntry
    }

    pub fn subscribe_presence_events(&self) -> broadcast::Receiver<PresenceEvent> {
        self.presence.subscribe()
    }
}
```

**职责边界**:
- **不**主动拨号(那是 `EnsureReachableAllUseCase` 的事;用户要"刷新状态"时是 F1 触发,不是查询副作用)
- **不**管 rename / revoke(Phase 3)
- `is_local` 判断:用 `LocalIdentityPort::current_fingerprint()` 对比

### 4.2 `EnsureReachableAllUseCase`(uc-application)

```rust
pub(crate) struct EnsureReachableAllUseCase {
    member_repo: Arc<dyn MemberRepositoryPort>,
    presence: Arc<dyn PresencePort>,
    local_identity: Arc<dyn LocalIdentityPort>,
    clock: Arc<dyn ClockPort>,
}

impl EnsureReachableAllUseCase {
    /// F1 钩子:unlock / resume 成功后并发拨号全员(跳过本机)。
    ///
    /// 并发策略:`tokio::task::JoinSet`,每个成员独立 task;
    /// 单个 ensure_reachable 失败不影响其他(各自降级为 Offline)。
    pub async fn execute(&self, space_id: &SpaceId) -> Result<EnsureReachableAllReport, ...> {
        // N ≤ 10 假设(task_plan.md:842):全员并发,不做限流
        // N > 10 的资源放大:T-05(P3),Slice 2 不管
    }
}

pub struct EnsureReachableAllReport {
    pub total: usize,
    pub online: usize,
    pub offline: usize,
    pub errors: Vec<(MemberId, String)>,
}
```

---

## 5. F1 unlock hook 接入点

`SpaceSetupFacade::auto_start_network`(facade.rs:336)当前:

```rust
async fn auto_start_network(&self) {
    if let Err(err) = self.network_control.start_network().await {
        warn!(error = %err, "start_network failed after space-lifecycle action; ...");
    }
}
```

**改动**:

```rust
async fn auto_start_network(&self) {
    if let Err(err) = self.network_control.start_network().await {
        warn!(error = %err, "start_network failed ...");
        return;
    }
    // F1 预连(Slice 2 Phase 1 D2 决策):unlock / resume / init 成功后
    // 并发探测全员,让 UI 的 roster presence 立刻准。
    //
    // space_id 从当前 SetupStatus 读(T-15 已修,A2 返回的就是这个 id)。
    if let Some(space_id) = self.current_space_id().await {
        match self.ensure_reachable_all.execute(&space_id).await {
            Ok(report) => info!(total = report.total, online = report.online, "F1 ensure_reachable_all done"),
            Err(err) => warn!(error = %err, "ensure_reachable_all failed; presence will recover lazily"),
        }
    }
}
```

**触发路径(自动接上,不用改调用方)**:
- `initialize_space`(A1)成功 → auto_start_network(facade.rs:267)
- `unlock_space`(A2)成功 → auto_start_network(facade.rs:279)
- `try_resume_session`(P9a)成功 → auto_start_network(facade.rs:311)

**空间 ID 从 `SetupStatus` 取**,T-15 已经保证这个一致。

**SpaceSetupDeps 加字段**:`ensure_reachable_all: Arc<EnsureReachableAllUseCase>`。

---

## 6. 任务拆解(执行顺序 + 依赖)

| # | 任务 | 依赖 | 工作量 |
|---|---|---|---|
| T1 | 在 `uc-core` 新建 `ports/presence.rs` + `ports/peer_address.rs` + 挂 mod | - | 0.5h |
| T2 | 在 `uc-infra/storage/` 新建 `SqlitePeerAddressRepository`(用现有 sqlite 基础设施)+ 单测 | T1 | 2h |
| T3 | 在 `uc-infra/network/iroh/` 新建 `IrohPresenceAdapter`(用 iroh 0.95 `Endpoint::remote_info` + `Watcher`)+ 单测(本地 loopback 两个 endpoint) | T1 | 3h |
| T4 | `IrohNodeBuilder::install_presence` 扩展点(返回 `Arc<dyn PresencePort>`),对称 `install_pairing` | T3 | 1h |
| T5 | pairing 完成收尾点把 `NodeAddr` 写入 `PeerAddressRepositoryPort`(sponsor + joiner 两边) | T2, Slice 1 代码 | 1h |
| T6 | `uc-application/usecases/presence/ensure_reachable_all.rs` + 单测(fake presence,并发安全 + 失败隔离) | T1 | 1.5h |
| T7 | `uc-application/facade/roster/` 全套(facade + commands + errors)+ 单测 | T1, T6 | 2h |
| T8 | `SpaceSetupFacade::auto_start_network` 接 `ensure_reachable_all` + 改 `SpaceSetupDeps` + 单测 3 个触发路径都会跑(initialize / unlock / resume) | T6 | 1h |
| T9 | `uc-bootstrap/assembly.rs` + `space_setup.rs` 装配新 port / facade | T2, T3, T7, T8 | 1h |
| T10 | `uc-cli/src/commands/members.rs` 新子命令 + mod.rs 挂接 + main.rs subcommand match | T9 | 2h |
| T11 | 集成测试 `uc-bootstrap/tests/slice2_phase1_presence_e2e.rs`(真 iroh + 两 endpoint + pairing + 断 B → A 探测 offline) | T9 | 3h |
| T12 | 扩展 `single-machine-e2e` 脚本跑 `members` 命令 | T10 | 1h |
| T13 | task_plan.md 标记 Phase 1 ✅ + 更新 Slice 2 路线图 | T11, T12 | 0.3h |

**总计**:~19.3h(≈ 2.5 个专注工作日)

**可并行组**:
- T2 / T3 互不依赖(都在 T1 之后),可并行
- T6 / T7 都在 T1 之后,可并行(T7 的测试会用 T6 的 usecase,但接口可先 mock)
- T11 / T12 最后并行

---

## 7. 测试策略

### 7.1 单元测试(随每个 T 交付)

| 组件 | 覆盖点 |
|---|---|
| `SqlitePeerAddressRepository` | upsert 后 get / 不存在返回 None / list 全量 / remove 幂等 / 并发 upsert 不 race |
| `IrohPresenceAdapter` | 两 loopback endpoint:`ensure_reachable` 初次 Online / peer drop 后 current_state = Offline / 订阅 receiver 拿到状态变化事件 |
| `EnsureReachableAllUseCase` | N=3 并发执行 / 单个失败不阻塞其他 / 跳过本机 / 空 roster 返回 total=0 |
| `MemberRosterFacade` | list_with_presence 聚合正确 / 本机标记 / subscribe receiver 实时收事件 |
| `SpaceSetupFacade::auto_start_network` | 3 个触发路径(A1 / A2 / resume)都跑一次 `ensure_reachable_all` / start_network 失败时不跑 |

### 7.2 集成测试(Phase 1 核心保障)

**`slice2_phase1_presence_e2e.rs`**(新建):
1. 起两个 `SpaceSetupAssembly` + `MemberRosterFacade`(A / B,用 loopback iroh)
2. 复用 Slice 1 测试夹具完成配对
3. A 和 B 分别 `list_with_presence`,断言对方在自己的 roster 里,state = Online(给 F1 触发器 ≤ 5s 时间)
4. 关掉 B 的 assembly(shutdown)
5. 等 ≤ 10s,A 再 `list_with_presence`,断言 B.state = Offline
6. B 重新起来 + unlock + pair-resume,等 ≤ 10s,A 断言 B.state = Online

### 7.3 CLI 冒烟(`single-machine-e2e.sh` 扩展)

在 Slice 1 已有脚本的 init / invite / join 之后,追加:
```bash
uniclipboard-cli members --profile=a  # 断言输出包含 b 的名字 + "online"
# 关掉 b 进程
sleep 12
uniclipboard-cli members --profile=a  # 断言 b "offline"
```

---

## 8. 风险 & 待确认

| 风险 | 缓解 |
|---|---|
| iroh 0.95 `remote_info` 语义跟预期不符(比如"尚未探测"也算 online) | T3 先写 adapter 探针测试验证 iroh 真实行为,再决定 `ReachabilityState` 映射规则 |
| `PeerAddressRepositoryPort` 的持久化层选择(新建 sqlite 表 vs 文件 vs 内存) | 建议**新建 sqlite 表**,理由:生命周期跟 `SpaceMember` 绑定,已有 sqlite 基础设施;内存不过进程,文件不好做并发原子写 |
| `pairing` 收尾点写 NodeAddr 会不会破坏 Slice 1 的原子性 | T5 写成"best-effort warn",失败不 fail 配对;presence 下次主动探测兜底 |
| `ensure_reachable_all` 并发拨号可能触发 iroh 限流 | N ≤ 10 假设下不会;若 T3 测试撞上限再降为 `JoinSet` + 并发度 5 |
| CLI `members` 需要 daemon 还是自包含 | **自包含**(同 Slice 1 init/invite/join),用 `build_slice1_cli_context`;daemon 模式留 Phase 2/3 |
| 测试时 iroh relay 不可用 | 复用 Slice 1 的 `IrohNodeConfig { disable_relays: true }` loopback 模式 |

---

## 9. Slice 1 Agent 规范合规性自查

| 规范项 | 确认 |
|---|---|
| uc-core 只含 port + 领域类型,不含 iroh 类型 | `PeerAddressRecord.addr_blob: Vec<u8>` 包住 NodeAddr |
| uc-application 只做编排,不侵入 core / infra | `EnsureReachableAllUseCase` 只调 port,不碰 iroh |
| uc-infra 实现面向 port,adapter 名清晰 | `IrohPresenceAdapter` / `SqlitePeerAddressRepository` |
| Orchestrator / StateMachine 不对外导出 | Phase 1 没有这俩;facade 直接驱动 usecase |
| Facade 只是入口,不重新编排业务 | `MemberRosterFacade` 是 2 个方法的 thin wrapper |
| 错误收敛,不外泄 iroh 类型 | `PresenceError` / `RosterError` / `PeerAddressError` 本地定义 |
| 敏感数据不打日志 | NodeAddr 可打(非敏感);`ensure_reachable` 日志只含 member_id + state |

---

## 10. 验收前检查清单

- [ ] 所有新 port + 实现 `cargo test -p uc-core -p uc-application -p uc-infra` 绿
- [ ] `slice2_phase1_presence_e2e.rs` 跑通
- [ ] `single-machine-e2e.sh` 扩展部分跑通
- [ ] CLI `uniclipboard-cli members --help` 输出合理
- [ ] 两台真实设备(或单机双进程)手动验证三条验收场景
- [ ] task_plan.md Phase 1 段打 ✅ + commit hash 记录

---

## 11. 推进节奏建议

- **Day 1**(~8h):T1 → T2/T3 并行 → T4 → T5
- **Day 2**(~8h):T6/T7 并行 → T8 → T9 → T10
- **Day 3**(~3h):T11/T12 并行 → 手动验收 → T13 + commit

每天结束或每完成一组相关 T 做一次 atomic commit,message 前缀 `feat(Slice2/P1): ...` / `test(Slice2/P1): ...` / `docs(Slice2/P1): ...`。

---

> **开工信号**:用户点头 → 从 T1 开始。
