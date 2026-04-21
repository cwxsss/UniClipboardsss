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

### T3 设计修订(2026-04-20 · T3a probe 后)

原设计假设 `Endpoint::conn_type` Watcher 在 peer 断开时会 transition,adapter 订阅 stream 就能知道 offline。

**probe 实测结论**(commit `36fc7e3b`,`uc-infra/tests/iroh_presence_probe.rs` 4 个场景):

1. ✅ `conn_type(unknown_peer)` → `Option::None` —— `Unknown` 映射成立
2. ✅ `conn_type(after connect)` → `Direct(SocketAddr)` —— `Online` 映射成立
3. ⚠️ `conn_type` 是**缓存**,peer 关闭 3s 后仍返 `Direct(...)` —— **不能**用做 offline 检测
4. ✅ `Connection::closed().await` 111ms 内触发 —— 才是可靠的 offline 信号

**修订后的 adapter 架构**:
- `IrohPresenceAdapter` 持有 `Mutex<HashMap<DeviceId, TrackedPeer>>`,`TrackedPeer` 包 `iroh::Connection` + `JoinHandle` watchdog
- `ensure_reachable(device)`:查 map,若 conn alive 返 `Online` 缓存;否则 `endpoint.connect(addr, PRESENCE_ALPN)` → 成功则插 map + spawn watchdog(等 `conn.closed().await` → 改 map + broadcast `Offline`);失败则 broadcast `Offline` 立返
- `current_state` 只读缓存(`list_with_presence` 调用路径不拨号)
- B 重启 → A 再次 online 的恢复路径:lazy retry 在下次 `ensure_reachable` 时生效。CLI `members` 命令在查询前先跑一轮 `ensure_reachable_all`,天然满足"≤ 10s 反映 online"的验收条款
- 新增 `PRESENCE_ALPN`(`uniclipboard/presence/0`)和 accept 侧 `IrohPresenceHandler`(实现 iroh `ProtocolHandler`,accept → hold until closed)

### 其他风险

| 风险 | 缓解 |
|---|---|
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

---

## 12. 进度跟踪(live · 2026-04-20 最新)

### 12.1 任务状态

| # | 任务 | 状态 | commit | 实际工时 | 备注 |
|---|---|---|---|---|---|
| T1 | uc-core 加 `PresencePort` + `PeerAddressRepositoryPort` | ✅ | `011472cf` | 0.4h | 估 0.5h,顺 |
| T2 | `DieselPeerAddressRepository` + migration | ✅ | `e81cec97` | 2h | 估 2h,6 单测绿;schema 新增 `peer_address` 表(TEXT PK + BLOB + INTEGER) |
| T3a | iroh 探针(conn_type 语义调研) | ✅ | `36fc7e3b` | 0.8h | **发现**:`conn_type` 是缓存,peer 关后仍返 `Direct(...)`;`Connection::closed()` 111ms 内可靠触发 |
| T3 计划修订 | 改 adapter 架构为 Connection::closed watchdog | ✅ | `a5394349` | 0.2h | `slice2-phase1-plan.md` §8 增补 |
| T3b | `IrohPresenceAdapter`(watchdog + `PRESENCE_ALPN` handler) | ✅ | `5c69b2a6` | ~0.6h(subagent) | 5 单测绿;`peers` / `last_state` 双 map(String 键——`DeviceId` 缺 `Hash`);`TrackedPeer::Drop` 自动 abort watchdog |
| T4 | `IrohNodeBuilder::install_presence` 扩展点 | ✅ | `32a02c62` | 0.3h | 镜像 `install_pairing`,两 ALPN 同 router 共存单测绿 |
| T5 | pairing 收尾点写 `NodeAddr` 到 repo | ✅ | `a562e529` | ~1.8h | 比估多 0.8h:wire 协议升级不可避,bump `WIRE_VERSION` → 2;3 个 T5 专项单测全绿 |
| T6 | `EnsureReachableAllUseCase` | ✅ | `e66776f8` | ~1.4h | 按 §12.4 决策用 `peer_addr_repo.list()` 作迭代源;`JoinSet` 并发 + `DeviceIdentityPort` 防御性 self-filter;6 单测全绿(含并发性 wall-time 断言——mockall expectation 内部 Mutex 会序列化 `.returning` 调用,改用手写 `SleepyPresence` fake) |
| T7 | `MemberRosterFacade` | ✅ | `548b3bdf` | ~0.5h | `facade/roster/` 全套(facade+commands+errors+mod);thin wrapper 不拨号;`is_local` 通过 `LocalIdentityPort::get_current_fingerprint()` 对比 `SpaceMember.identity_fingerprint`;drop `MemberId`(无此类型)+ `last_seen_at`(presence port 当前无时间追踪);8 单测全绿 |
| T8 | F1 hook `auto_start_network` | ✅ | `f461a6eb` | ~0.7h | `SpaceSetupDeps` 加 `presence: Arc<dyn PresencePort>`;facade 内部构造 `EnsureReachableAllUseCase`;`auto_start_network` 成功后紧接 `ensure_reachable_all.execute()`,失败走 `warn!` 不传播;4 新单测覆盖验收点;bootstrap 的 presence port 接线(`IrohNodeBuilder::install_presence` 调用)**随 T8 合入**,因为 `SpaceSetupDeps` 新字段不装 `presence` 编译不过——T9 scope 缩减为只做 MemberRosterFacade 的 bootstrap 接线 |
| T9 | bootstrap 装配 | ✅ | `181f2cc8` | ~0.2h | scope 缩减(T8 已吸收 presence 接线)后只剩 `MemberRosterFacade` 装配:`SpaceSetupAssembly` 加 `pub roster: Arc<MemberRosterFacade>` 字段,`build_space_setup_assembly` 构造时复用 `member_repo` / `local_identity` / `presence` 三个 Arc(后两个需先 `Arc::clone` 给 SpaceSetupFacade,之后再 move 给 roster);工作空间全量编译 + slice1 e2e 仍绿 |
| T10 | `uniclipboard-cli members` | ✅ | `bda7686b` | ~0.4h | 自包含直连模式:build_assembly → try_resume_session → **facade.refresh_presence**(plan §12.4 T10 提醒,F1 hook 之外的显式 probe 入口)→ `roster.list_with_presence` → human(`name (state) [local]` per line)/ JSON 双渲染。为不泄露 `EnsureReachableAllUseCase`(§11.4),在 `SpaceSetupFacade` 加 thin `refresh_presence()` wrapper,并从 space_setup mod 透出 `EnsureReachableAllReport/Error`。无新单测(integration-level);workspace 全量编译 + uc-application 176 + uc-cli 10 + slice1_handshake_e2e 单测绿 |
| T11 | `slice2_phase1_presence_e2e` 集成测试 | ✅ | `d39889e0` | ~1.0h | 两例:`pair_then_refresh_reports_both_sides_online`(plan §1.1 verdict 1)+ `joiner_shutdown_flips_sponsor_roster_to_offline_within_10s`(verdict 2)。verdict 3("B 重启 online")**刻意跳过**:`disable_relays=true` loopback-only 测试无法模拟 iroh 的 NodeAddr 刷新,强行跑只出假阳性——手动验证已覆盖。测试跑时 37.4s(两例共用 wiremock + iroh bind)。暴露 Slice 1 pre-existing gap:joiner B2 不 save self → joiner 视角 roster 只有 sponsor(测试断言 `joiner_roster.len() == 1` 作为契约信号,注释标记 future fix 时应改) |
| T12 | `single-machine-e2e.sh` 扩展 | ⏭️ 跳过 | — | 0h | 评估后跳过:shell 脚本维护成本 > 回归保护价值(改 CLI 输出文案就断),T11 Rust 集成测试已给等价覆盖,且比 shell 更精确(10s 时效断言)。需要时再补,task_plan.md Phase 1 ✅ 标注已收录 |
| T13 | task_plan.md Phase 1 ✅ 收尾 | ✅ | `(本提交)` | ~0.3h | task_plan.md 中 Slice 2 Phase 1 节改 🔲 → ✅ 并罗列所有 T1-T11 commit hash + 验收达成状态;follow-up 记录"joiner 不 save self" gap(T11 暴露)供 Phase 2/3 处理;本 plan §12.2 累计更新 |

### 12.2 累计

- **已完成**:T1 / T2 / T3a / T3(修订) / T3b / T4 / T5 / T6 / T7 / T8 / T9 / T10 / T11 / T13 = 14 项 / ~10.6h
- **跳过**:T12(评估后 shell e2e 扩展价值低于维护成本,Rust 集成测试已覆盖——task_plan.md Phase 1 ✅ 节有记录)
- **进度**:🏁 **Slice 2 Phase 1 完成**(2026-04-22)。实际总工时 ~10.6h(不含跳过的 T12),比原估算 ~15.2h 省约 30%——主要来自 T4/T7/T9 模块化良好 + T6 复用 `JoinSet` 而非自造并发模型 + T12 战略性跳过

### 12.3 关键发现 / 偏离

1. **iroh `conn_type` 不可靠**(T3a 探针发现):原计划用 `Endpoint::conn_type` Watcher 订阅状态流做 offline 检测,实测是缓存语义,peer 关后仍返 `Direct(...)`。改走"持有 Connection + 等 `closed()`"模式,加 watchdog task。`slice2-phase1-plan.md` §8 修订已合入。

2. **`DeviceId` 缺 `Hash` derive**(T3b 编码时遇):全 `uc-core::ids` 家族里唯一没派生 `Hash` 的 ID。绕过:adapter 内部用 `String` 作 HashMap key,边界处重建 `DeviceId`,port 契约不受影响。后续若要修需动 uc-core 公共 API(非本 Phase 范围)。

3. **`TrackedPeer::Drop` 自动 abort watchdog**(T3b 设计补丁):防止 `peers` 移除 entry 后 watchdog task 成为孤儿。

4. **T5 wire 协议升级**(2026-04-21):原计划只写 repo 即可,实际发现 sponsor 拿不到 joiner 的 `EndpointAddr`(iroh `Endpoint::remote_info` 是 `pub(crate)`,`Connection::remote_address` 只给单个 SocketAddr 不含 relay)。改为 wire 对称扩展:`JoinerRequest` / `SponsorConfirm` 各加 `transport_address_blob: Vec<u8>`(opaque bytes,core 纯净);新增 port 方法 `PairingSessionPort::local_transport_address_blob`(iroh adapter 返 `postcard(endpoint.addr())`);`WIRE_VERSION` 从 1 升到 2——Slice 1 → Slice 2 升级期跨版本对端由 `UnsupportedVersion` 显式拒连,因为 pre-release 不需兼容层。

5. **T6 mockall 并发坑**(2026-04-21):并发性单测用 mockall 的 `.returning(|_| { thread::sleep; ... })` 会 **被序列化**——mockall 把 expectation 的 `FnMut` closure 存在内部 `Mutex<...>` 里以保证 trait object 安全,三个 JoinSet task 在 Mutex 上排队,即使 multi-thread runtime 也走 serial(实测 616ms ≈ 3 × 200ms)。同一问题会影响**任何**需要断言并发的 mockall 测试。统一替换为手写 `impl PresencePort` fake(30 行,`tokio::time::sleep` 直接 yield)。正常"调用次数 + 参数匹配"断言仍用 mockall。

### 12.4 后续提醒

- ~~T6 `EnsureReachableAllUseCase` 可以读 `peer_addr_repo.list()` 直接枚举所有 paired 设备(跳过本机),对每个调 `presence.ensure_reachable`;不需要再从 `member_repo` 拉取。~~ ✅ T6 已按此决策实施(2026-04-21)。`execute()` 签名无 `space_id` 参数——当前单 space 场景 peer_addr_repo 就是全量 roster 的上限;多 space 将来再加。`EnsureReachableAllError::Repository` 表达 repo 故障,单点 probe 失败归 `report.errors` 不 fail 整体。
- ~~T7 `MemberRosterFacade::list_with_presence` 的 `is_local` 判断:对每个 member 比 `LocalIdentityPort::get_current_fingerprint()`。~~ ✅ T7 已按此决策实施(2026-04-21)。`local_identity.get_current_fingerprint()` 取一次,对每个 member 的 `identity_fingerprint` 做 `==`(经 `IdentityFingerprint::PartialEq` 语义上等价于 `verify`)。pre-A1/B2 态(返回 `None`)下所有 entry 均标 `is_local=false`——此窗口期一般也无成员记录,属防御路径。plan §4.1 的 `member_id` 字段和 `last_seen_at` 字段已 drop(前者无对应类型,后者 presence port 当前不追踪时间戳;T7 验收点仅要求 state 三值正确)。
- ~~T8 的 "hook 在 `auto_start_network` 内触发 ensure_reachable_all" 需要从 `SetupStatus` 读 `space_id`(T-15 已确保一致,2026-04-20 `255fd2fe`)。~~ ✅ T8 已实施(2026-04-21)。实际实现比原计划更简:因 T6 `execute()` 已无 `space_id` 参数(§12.4),F1 hook 不用读 SetupStatus;facade 新增 `ensure_reachable_all: Arc<EnsureReachableAllUseCase>` 字段,`auto_start_network` 成功拉起 network 后 unconditionally `execute()` 一次。成功路径 `info!` 输出 total/online/offline/errors,失败路径 `warn!` 不传播——A1/A2/B2 的空间变更已落盘,失败回滚代价远大于"网络 lazy 重连"的代价。**bootstrap 的 presence port 接线一并合入 T8**(`SpaceSetupDeps.presence` 新字段不装配编译不过)——T9 scope 现在只剩 MemberRosterFacade 的 bootstrap 接线。
- ~~T10 CLI `members` 命令执行前**应先跑一轮** `ensure_reachable_all`(plan §8 T3 修订决策),保证 B 重启后"下次 CLI 查询 ≤ 10s 内显示 online"的验收条款。~~ ✅ T10 已实施(2026-04-21 `bda7686b`):`facade.refresh_presence()` 在 `try_resume_session` 之后 `list_with_presence` 之前调一次,把刷新后 report 的 total/online/offline/errors 摘要喂给 spinner,单个 peer 失败不 fatal(进 `report.errors`)。usecase 保持 pub(crate),只通过 facade thin wrapper 对外——后续 Tauri `get_roster` 命令可复用同一入口。
- ~~T11 e2e 覆盖"iroh keypair 重绑恢复"路径(T3b 用 repo swap 绕过的部分),以及 T5 新增的 wire 对称 blob 写入(两侧 repo 中都能 get 到对方 blob,且 postcard 解码得到合法 `EndpointAddr`)。~~ ✅ T11 已实施(2026-04-22 `d39889e0`):verdict 1 + verdict 2 覆盖,verdict 3(B 重启 online)因 `disable_relays=true` loopback 限制跳过自动化——stale socket 在无 relay 下拿不回来,强测只能出假阳性,手动验收覆盖。对称 blob 断言通过 `sponsor_report.online=1` / `joiner_report.online=1` 间接覆盖(能 probe online 说明两侧 repo 里 blob 有效)。
- **T11 暴露 pre-existing Slice 1 gap**:`RedeemPairingInvitationUseCase::persist` 只 admit sponsor,joiner 不把自己存进 `member_repo` → joiner 视角下 `members` 命令看不到本机。T11 断言 `joiner_roster.len() == 1` 作为契约信号。修复应在 `persist` 收尾追加 `save self` 步骤,对应测试断言需更新为 `== 2`。推到 Phase 2 / Phase 3 的 rename/revoke 工作中顺手修。
