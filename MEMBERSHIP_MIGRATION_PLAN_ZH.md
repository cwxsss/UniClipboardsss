# Membership 迁移计划（临时工作文档）

> 范围：用 `uc-core::membership` + `uc-application::membership` 彻底替换现有的 `paired_device` 体系。
> 状态：**Phase 1 / Phase 2 已落地；阶段 0.1 / 0.2 / 0.3 已提交，下一步进入阶段 0.4 消费者切换。**
> 本文档只服务于迁移本身，不作为产品文档，迁移完成后可直接删除。

---

## 0. 当前状态（2026-04-17 更新）

迁移走到 Phase 2 完成后，发现**整件事无法和 `space_access` / `pairing` 解耦**，因此插入"里程碑 §5"处理 `space_access` + `setup` 搬家。

进一步讨论后发现：搬 space_access 会遭遇 `SpaceAccessPersistenceAdapter → StagedPairedDeviceStore` 的反向依赖绊脚石，其根源是 `PairedDevice` 上帝对象（把"配对动作"、"信任关系"、"同步偏好"、"显示名"捏在一起）。根治需先重构 pairing domain。

**2026-04-17 决策**：

1. 将 `pairing` 域重命名并重构为 **`trusted_peer`**（只管"两台设备可通信信任关系"），拆解 `PairedDevice`
2. 在里程碑 §5 前插入 **阶段 0 — trusted_peer domain 重构**，作为 §5.4 的第一阶段
3. `TRUSTED_PEER_DOMAIN_ZH.md` 已作为 §5.4 阶段 0 的权威定义固化
4. Phase 3/4/5 仍保留，等阶段 0→A→B→C 完成后再启动

**当前状态**（2026-04-17 更新）：
- Phase 1/2 已提交（`space_member` repo + 双写 `space_member` 影子）
- **阶段 0.0 / 0.1 / 0.2 / 0.3 已提交**（详见 §5.4 阶段 0 执行记录与决策）
- Phase 2 的双写（`paired_device` 权威 + `space_member` 影子）继续运行 — 无回归风险
- `uc-application::trusted_peer` 已就位但尚未 wire（bootstrap 未构造 orchestrator，pairing 协议未切过去）

**下一步**：阶段 0.4（方案 B2），按 §5.4 "阶段 0.4 commit 拆分" 的 0.4.1 → 0.4.4 顺序推进。**0.4.1 / 0.4.2 / 0.4.2.b / 0.4.3 / 0.4.4 均已完成**（写入路径从 PairedDevice 切到 TrustedPeer；`FailureReason` 收口至 `TrustAbortReason`；反向依赖清除；daemon 入口统一走 `PairingFacade`）。当前进入 **0.5**（删除 `PairedDevice` 族 Rust 类型），再切入阶段 A（space_access 搬家 + `SpaceAccessContext` 扩字段 + admit 挂接）。

---

## 1. 背景与核心决策（拍板一次、沿用到底）

| # | 决策 | 备注 |
|---|---|---|
| D1 | **单空间模型**，`SpaceMember` 不带 `SpaceId` | 当前 UniClipboard 是单空间 |
| D2 | **本地自治**，成员关系不跨设备同步，revoke 是本地动作 | 对端若继续发数据，由接收路径兜底 |
| D3 | **`MemberSyncPreferences` 语义**：本机对某远端成员的"发送/接收"独立偏好 | 与旧 `SyncSettings` 不同层次 |
| D4 | **revoke = 从 repo 硬删**（无 `Revoked` 中间态） | 因此 `MemberState` 枚举不存在 |
| D5 | **`DeviceId == peer_id` 字符串直接复用**，不引入独立映射表 | 妥协换零迁移成本；副作用：libp2p key 轮换 = 新身份，已接受 |
| D6 | `last_seen_at` **不进 membership**，搬到网络层 | 不属于成员关系属性 |
| D7 | 老数据 **一次性 migration**，`paired_device` 里 `Trusted` 的行自动搬到 `space_member` | Phase 1 已经包进 up.sql |
| D8 | `PairingState` 枚举 **整体删除**（Phase 5 或合并到 pairing 搬家时） | 代码里 `Pending` 从未写进 DB，`Revoked` 从未被读过 |
| D9 | Daemon DTO **一次性改名**（`PairedDeviceDto` → `SpaceMemberDto`），前端同步改 | Phase 3.6 |
| D10 | admit 的默认 `sync_preferences` = `MemberSyncPreferences::default()`（双向全开） | 不读全局 `SyncSettings`，避免层次混淆 |
| D11 | admit 幂等冲突在双写期降级为 WARN | 不让 membership 侧失败影响配对流程 |
| D12 | `sync_settings → MemberSyncPreferences` 映射：`None → default`；`auto_sync` 镜像到 `send/receive`；`content_types` 镜像到发/收两侧；`sync_frequency` 丢弃 | 已固化在 `2026-04-18-000001_create_space_member/up.sql` |
| D13 | **`uc-application` 是新 use case 所在地**，`uc-app` 在退役。daemon 已依赖 uc-application；uc-app 不依赖 uc-application | 与最近几个 commit 方向一致（file_transfer 已搬家、search 改 port 等） |
| D14 | `admit_member` use case **保留**，等 `space_access` 搬家后作为其完成时的调用点 | 目前无调用方是暂时态 |
| D15 | **`uc-app/pairing` 整体拆除**（方案 b）：协议状态机并入 `space_access`；设备列表归 `membership`；协议技术层下沉到 `uc-core`/`uc-infra` | 在 space_access 搬家那一步同时发生 |
| D16 | 迁移**暂停**在 Phase 2 完成态，先做 `space_access` 到 `uc-application` 的搬家 | 见第 0 节 |

---

## 2. Phase 0 — 领域建模（已完成 · 提交 `1af58f34`）

### 2.1 `uc-core/src/membership/`

- `member.rs` — `SpaceMember { device_id, device_name, identity_fingerprint, joined_at: DateTime<Utc>, sync_preferences }`
- `preferences.rs` — `MemberSyncPreferences { send_enabled, receive_enabled, send_content_types, receive_content_types }`；`Default` 双向全开
- `error.rs` — `MembershipError { AlreadyAdmitted(DeviceId), NotFound(DeviceId), Repository(String) }`
- `ports.rs` — `MemberRepositoryPort { get, list, save(upsert), remove }`
- `mod.rs` — 导出四者，顶层 `lib.rs` re-export

### 2.2 `uc-application/src/membership/`

- `errors.rs` — `MembershipApplicationError`，实现 `From<MembershipError>`
- `usecases/admit_member.rs` — 幂等检查（get→if Some→AlreadyAdmitted），然后 save
- `usecases/get_member.rs` — 查不到返回 NotFound
- `usecases/list_members.rs` — 无参，直接透传 repo.list
- `usecases/update_member_settings.rs` — 全量覆盖 sync_preferences
- `usecases/reset_member_preferences_to_default.rs` — 重置为默认值
- `usecases/revoke_member.rs` — 调 repo.remove，未存在返回 NotFound
- `usecases/mod.rs` — 统一导出

### 2.3 刻意放弃的设计

- ❌ `ensure_active_member` use case — 调用方不清楚，暂不引入
- ❌ `MemberState` 枚举 — 硬删模型下没有用武之地
- ❌ `IdentityFingerprint` 值对象 — 暂保留 `String`，未来再说
- ❌ `DomainEvent` / `MemberEventPort` — 本地自治不需要广播

---

## 3. Phase 1 — Infra 落地 + 数据迁移（已完成 · 提交 `5f5c6f4c`）

### 3.1 Migration

`uc-infra/migrations/2026-04-18-000001_create_space_member/`

- `up.sql` — `CREATE TABLE space_member(...)`，然后 `INSERT ... SELECT` 从 `paired_device WHERE pairing_state = 'Trusted'`，期间把 JSON `sync_settings` 映射成 `MemberSyncPreferences` JSON。每个布尔字段走 `CASE json_extract(...) WHEN 0 THEN json('false') ELSE json('true') END` 规避 SQLite bool-as-int 怪癖
- `down.sql` — `DROP TABLE space_member`

### 3.2 新增文件

- `src/db/models/space_member_row.rs` — `SpaceMemberRow` / `NewSpaceMemberRow`
- `src/db/mappers/space_member_mapper.rs` — `SpaceMemberRowMapper`（`to_row` 序列化 preferences 为 JSON，`to_domain` 反序列化）
- `src/db/repositories/space_member_repo.rs` — `DieselSpaceMemberRepository`，实现 `MemberRepositoryPort::{get, list, save(UPSERT on conflict), remove(bool)}`
- `src/db/schema.rs` — 新增 `space_member` table 条目

### 3.3 测试

`space_member_repo.rs` 内置 6 个集成测试（用 `init_db_pool` + `tempfile`）：

- `save_then_get_roundtrip`
- `get_missing_returns_none`
- `save_is_upsert`
- `list_returns_all_saved`
- `remove_returns_true_when_present_false_when_absent`
- `migration_copies_trusted_paired_devices_with_default_preferences` — 验证 Pending 被跳过 + `auto_sync=false`/混合 content_types 的映射

---

## 4. Phase 2 — 双写：pairing 完成后同时写 `space_member`（已完成 · 提交 `befbbdfe`）

### 4.1 装配

- `uc-app/src/deps.rs` — `DevicePorts::member_repo: Arc<dyn MemberRepositoryPort>`
- `uc-bootstrap/src/assembly.rs` — `InfraLayer::member_repo`、构造 `DieselSpaceMemberRepository`、填入 `DevicePorts`
- `uc-bootstrap/src/builders.rs` 两处 — 从 `deps.device.member_repo.clone()` 取出，传给 `PairingOrchestrator::new`

### 4.2 Pairing 改造

- `PairingOrchestrator::new` 签名加 `member_repo: Arc<dyn MemberRepositoryPort + Send + Sync + 'static>`（第 3 参数）
- `PairingProtocolHandler::new` 同样加一个字段
- `execute_action_inner` 和 `handle_timeout` 签名都新增该参数

### 4.3 双写逻辑（`PairingAction::PersistPairedDevice` 分支）

```rust
let member_snapshot = space_member_from_paired_device(&device); // snapshot before move
let persist_result = device_repo.upsert(device).await;          // authoritative
if persist_result.is_ok() {
    dual_write_member(member_repo.as_ref(), &session_id, member_snapshot).await;
}
```

两个 helper 在文件底部：

- `space_member_from_paired_device(&PairedDevice) -> SpaceMember`：字段映射，`device_id = DeviceId::new(peer_id.as_str())`，`joined_at = paired_at`，`sync_preferences = default()`
- `dual_write_member(...)`：只做 `member_repo.save`，任何错误都 WARN，不 propagate

### 4.4 测试

`protocol_handler.rs` 的 `#[cfg(test)] mod tests` 新增 4 个 case：

- `space_member_from_paired_device_maps_core_fields`
- `dual_write_persists_member_on_success`
- `dual_write_swallows_repository_errors`
- `dual_write_swallows_already_admitted_errors`

都用 `FakeMemberRepo`（sync `StdMutex<Vec>` + 可选错误注入），不依赖真实 DB。

---

## 5. 插入里程碑 — `space_access` + `setup` 搬到 `uc-application`（进行中，优先级最高）

> 2026-04-17 细化：把本里程碑固化为 **阶段 A / 阶段 B / 阶段 C**，顺序不可颠倒。每阶段完成是下阶段的前置。

### 5.1 大致形状（背景）

- `uc-core/src/space_access/` 已有领域模型（`domain.rs` / `state.rs` / `state_machine.rs` / `action.rs` / `event.rs` / `error.rs` / `reason_codec.rs`）
- `uc-core/src/setup/` 里除 `status.rs` 之外的所有状态机 / 事件 / 动作 / 错误**应拉回 `uc-application`**（当前位置违反 `uc-core/AGENTS.md` §9.1，顺带纠偏）
- `uc-application/src/space_access/` 和 `uc-application/src/setup/` **未建立**，本里程碑同时建立
- 选项 (b) 下，`uc-app/usecases/pairing/` 的协议状态机和 orchestrator **合并或整合进 space_access**；纯技术协调层（如 `StagedPairedDeviceStore`）下沉到 `uc-core` / `uc-infra`

### 5.2 与 membership 的对接

- `space_access` 的 **`SpaceAccessState::Granted`** 是 `AdmitMemberUseCase::execute(AdmitMember { ... })` 的**唯一正式入场点**（D14）
- `setup` **不重复**挂 admit（`SetupEvent::JoinSpaceSucceeded` 只推 UI 状态）
- `device_name` / `identity_fingerprint` 由 **pairing 在 `KeyslotReceived` / `PairingSucceeded` 事件**写入 `SpaceAccessContext`，由 space_access 取出构造 `AdmitMember` 输入
- 调用来源从"uc-app 的 pairing protocol_handler"切到"uc-application 的 space_access orchestrator"
- `uc-app/src/usecases/pairing/` 里的 `list_paired_devices` / `unpair_device` / `get_device_sync_settings` / `update_device_sync_settings` 等 use case 在 pairing 搬家后应不再有消费方

### 5.3 对 membership 迁移的影响

- 搬家完成后再启动修正版 Phase 3；且 Phase 3 的一部分工作（删除 uc-app 重叠 use case）会被 space_access 搬家顺手完成
- Phase 4 的"删双写"简化为"space_access 成功后只写 `space_member`，不再写 `paired_device`"

---

### 5.4 阶段 0 / A / B / C 固化（2026-04-17 决策）

**执行顺序不可颠倒**：0 → A → B → C。每阶段的"出口条件"是下阶段启动的前置。

> 2026-04-17 增补：原计划直接启动阶段 A，但发现 `SpaceAccessPersistenceAdapter` 反向依赖 `StagedPairedDeviceStore`，其病根是 `PairedDevice` 上帝对象。根治需先重构 pairing domain，因此插入 **阶段 0 — `trusted_peer` domain 建立 + `PairedDevice` 拆解清退**。详细 domain 定义见 `TRUSTED_PEER_DOMAIN_ZH.md`。

#### 阶段 0 — `trusted_peer` domain 重构（`PairedDevice` 上帝对象拆解）

**产物**：`uc-core::trusted_peer` + `uc-application::trusted_peer` 就位；`PairedDevice` 上帝对象拆解为 `TrustedPeer`（信任关系）+ `SpaceMember`（成员登记）+ 未来的 `DeviceDisplayInfo`（显示属性），`StagedPairedDeviceStore` 消失。

| # | 状态 | 动作 | 说明 |
|---|---|---|---|
| 0.0 | ✅ | 写 `TRUSTED_PEER_DOMAIN_ZH.md` 固化 domain 定义 | 作为 0.1~0.5 的唯一权威来源，防止实施漂移 |
| 0.1 | ✅ `47861357` | 新建 `uc-core::trusted_peer`：`TrustedPeer` / `PeerFingerprint` / `TrustedPeerEvent` / `TrustedPeerRepositoryPort` / `TrustedPeerError` | 严格按 DOMAIN.md §4 形状；`ShortCode` **不进** core |
| 0.2 | ✅ `d7aa22a1` | `uc-infra`: 新建 `trusted_peer` 表 + `DieselTrustedPeerRepository`（无数据迁移，用户升级后重新配对） | DOMAIN.md §8；**不与 `paired_device` 做兼容搬迁**（2026-04-17 决策，详见执行记录 D17） |
| 0.3 | ✅ `ef5ba23b` | 新建 `uc-application::trusted_peer`：orchestrator + 状态机 + `TrustPeerUseCase` / `ConfirmPeerVerificationUseCase` / `CancelTrustingUseCase` / `DistrustPeerUseCase` / `ListTrustedPeersQuery` / `GetTrustedPeerQuery` | DOMAIN.md §5；状态机终态仅 `Trusted` / `Aborted` |
| 0.4 | ⏳ 进行中 | **pairing 协议层整体从 `uc-app/pairing/` 平移到 `uc-application/pairing/`**（B2 方案，2026-04-17 决策 D26），同时切换写入目标到 `TrustedPeer`；`staged_paired_device_store` / `uc-app/pairing/facade.rs` 删除；daemon 直切 `uc-application::pairing`；旧错误类型 `FailureReason` / `PairingBusy` 翻译到 `TrustStateEvent` 三档 | 涉及 6 个协议文件搬家 + bootstrap / daemon import 改名 + 写入路径切换；非协议类 use case（`list_paired_devices` / `unpair_device` / 等 8 个）留在 uc-app，到 Phase 3 再切换。0.4.1 已提交 `b8de5aa2`；0.4.2 已落地（写入路径切 TrustPeerOrchestrator） |
| 0.5 | ⏳ | 删除 `uc-core::pairing::PairedDevice` / `PairingState` / `PairedDeviceRepositoryPort`；`paired_device` 表进入只读 | Rust 类型删除；`DROP TABLE paired_device` 统一留给 MIGRATION_PLAN Phase 5 |

**阶段 0 出口条件**（2026-04-17 修订 — 配合 B2 方案）：
1. `uc-core::trusted_peer` + `uc-application::trusted_peer` + `uc-application::pairing` 编译通过，测试全绿
2. `StagedPairedDeviceStore` 从代码库彻底消失
3. `SpaceAccessPersistenceAdapter` 不再引用 `crate::usecases::pairing::*`，反向依赖绊脚石清除
4. ~~`SpaceAccessContext` 已承载 `device_name` / `peer_fingerprint`~~ **推到阶段 A**（space_access 搬家时一并扩字段 + 挂写入最自然；0.4 pairing 搬家后已具备跨模块直接调用条件）
5. ~~`PairingState` 枚举彻底删除~~ **推到 0.5**（和 `PairedDevice` 一起删；0.4 搬家期间 `PairingState` 仍是 PairedDevice 字段，但写入路径已不再生产该值）
6. `paired_device` 表只读，新写入路径已切到 `trusted_peer`（`space_member` 写入由阶段 A 的 admit 挂接补齐）
7. `uc-app/pairing/` 协议层（6 个文件）消失；daemon 不再 import `uc_app::usecases::pairing::{PairingOrchestrator, PairingAction, PairingDomainEvent, FailureReason}`
8. CI 全绿；无回归

**阶段 0 拆 commit**：
- commit 1：0.0 DOMAIN.md
- commit 2：0.1 core domain 定义 — 已提交 `47861357`
- commit 3：0.2 infra 表 + migration — 已提交 `d7aa22a1`
- commit 4：0.3 application 层 orchestrator + UseCases — 已提交 `ef5ba23b`
- commit 5：0.4 消费者切换 + staged store 删除（进行中）
- commit 6：0.5 `PairedDevice` 及相关 Rust 类型清除

#### 阶段 0 执行记录与决策（2026-04-17）

##### 已完成工作

**0.1 `uc-core::trusted_peer`**（commit `47861357`）
- 新建 `src-tauri/crates/uc-core/src/trusted_peer/` 下 6 个文件：`mod.rs` / `peer.rs` / `fingerprint.rs` / `events.rs` / `ports.rs` / `error.rs`
- 产出类型：`TrustedPeer` aggregate（§4.1）、`PeerFingerprint` 值对象（§4.2）、`TrustedPeerEvent` + `TrustAbortReason` 三档（§4.3）、`TrustedPeerRepositoryPort`（§4.4）、`TrustedPeerError` 三变体（§4.5）
- 在 `uc-core/src/lib.rs` 顶层 re-export 全部 6 个类型
- 6 个单元测试覆盖 `PeerFingerprint` 等值/Display、3 条错误翻译、`TrustAbortReason` 变体互异
- 审查清单 §11 七条全答"否"；不引入 `tokio` / `diesel` / `libp2p`

**0.2 `uc-infra` 落地**（commit `d7aa22a1`）
- migration `2026-04-19-000001_create_trusted_peer/{up,down}.sql`：`CREATE TABLE trusted_peer(peer_device_id PK, local_device_id, peer_fingerprint, trusted_at)` + `idx_trusted_peer_local`
- 新增 `TrustedPeerRow` / `NewTrustedPeerRow` / `TrustedPeerRowMapper` / `DieselTrustedPeerRepository<E, M>`
- `trusted_peer` 表加入 `schema.rs` 的 `allow_tables_to_appear_in_same_query!`
- `save` 采用 `ON CONFLICT(peer_device_id) DO UPDATE` 上 upsert
- 5 个 repo 集成测试：save/get 回环、missing→None、UPSERT、list-all、remove 布尔返回

**0.3 `uc-application::trusted_peer`**（commit `ef5ba23b`）
- 新建 `src-tauri/crates/uc-application/src/trusted_peer/` 下 14 个文件：`mod.rs` / `errors.rs` / `challenge.rs` / `state.rs` / `state_machine.rs` / `orchestrator.rs` / `testing.rs` + `usecases/` 下 8 个文件
- 产出：`TrustState` 五态 + `TrustStateEvent` 六事件；纯函数 `transition()`；`TrustPeerOrchestrator<R>` 持 `Mutex<TrustState>`
- 六个 UseCase / Query：`TrustPeerUseCase`、`DistrustPeerUseCase`、`ListTrustedPeersQuery`、`GetTrustedPeerQuery` 直接操作 repo；`ConfirmPeerVerificationUseCase`、`CancelTrustingUseCase` 是 orchestrator 的 thin wrapper
- crate-internal `InMemoryTrustedPeerRepository`（`#[cfg(test)]`）供所有单元测试共享
- 26 个单元测试：`errors` × 1、`state_machine` × 9、`orchestrator` × 7、`trust_peer` × 2、`distrust_peer` × 2、`list_trusted_peers` × 2、`get_trusted_peer` × 2
- bootstrap 尚未构造 orchestrator；pairing 协议尚未驱动；0.3 不发 `TrustedPeerEvent`（无订阅者，留到 0.4）

##### 执行中新增的决策

承接 §1 的 D1-D16，阶段 0 执行期间新固化了下列决策，供后续阶段引用：

| # | 决策 | 生效阶段 | 备注 |
|---|---|---|---|
| D17 | **阶段 0.2 不做 `paired_device → trusted_peer` 数据搬迁**（2026-04-17） | 0.2 | 用户升级后重新配对。DOMAIN.md §8.2 的一次性 migration 不执行，迁移 SQL 只 `CREATE TABLE` |
| D18 | **`trusted_at` 在 infra 落为 `BigInt` seconds**（偏离 DOMAIN.md §8.1 的 `TEXT ISO-8601`） | 0.2 | 对齐项目全局约定（`space_member.joined_at` / `paired_device.paired_at` / `*_at_ms`）；core 契约仍是 `DateTime<Utc>`，纯 infra 决策 |
| D19 | **`TrustPeerOrchestrator` 在 bootstrap 中以全局单例方式装配**（2026-04-17） | 0.4 | 单空间模型下同一时刻只允许一个 trust flow；不做 per-peer 实例化；`Mutex<TrustState>` 天然串行化所有推进路径 |
| D20 | **阶段 0.4 直接删除旧协议错误类型（`PairingMessage` / `PairingBusy` 等）**（2026-04-17） | 0.4 | 无向后兼容；翻译到 `TrustStateEvent::{TimedOut, ProtocolError, UserCancelled}` 三档后，原有错误类型消失，不保留 sentinel |
| D21 | **`TrustPeerUseCase` 对重复 peer 返回 `AlreadyTrusted`，不静默覆盖 fingerprint**（DOMAIN.md §4.5 对称执行） | 0.3 | fingerprint 轮换属于合法场景，但走 "先 `DistrustPeerUseCase` 再 `TrustPeerUseCase`" 的显式路径；repo 层 `save` 仍是 upsert（支持状态机内部重入），业务侧由 UseCase 拒绝 |
| D22 | **`ConfirmPeerVerificationUseCase` / `CancelTrustingUseCase` 是 orchestrator 的 thin wrapper，不是独立 repo 操作** | 0.3 | 让 UI 调用方对 orchestrator 无感，但状态真相仍单点收口在 orchestrator（AGENTS.md §10.2 对齐） |
| D23 | **`TrustedPeerEvent` publisher 留到阶段 A 再加**（2026-04-17） | A（非 0.4） | 0.4 只做同步入口调用，不引入异步事件通道；阶段 A 的 `SpaceAccessOrchestrator` 是唯一订阅者，届时一次性加 `tokio::sync::broadcast` 通道 + 订阅，避免 0.4 先加通道再返工订阅端 |
| D24 | **D20 缩范围：只删 `FailureReason` / `PairingBusy` 等错误类型，保留 `PairingMessage` 报文结构**（2026-04-17 修订） | 0.4 | `PairingMessage` 是网络线上协议报文，99 处引用横跨 `uc-platform/adapters` / `uc-daemon`；0.4 翻译到 `TrustStateEvent::{TimedOut, ProtocolError, UserCancelled}` 三档；报文本身的重命名或结构重写留到阶段 A |
| D25 | **0.4 daemon 直切 `uc-application::trusted_peer`，删除 `uc-app/pairing/facade.rs`**（2026-04-17） | 0.4 | `uc-app/pairing/facade.rs` 当前未被 daemon 使用（daemon 直 import `PairingOrchestrator` / `PairingAction` / `PairingDomainEvent` / `FailureReason`），保留 facade 只是虚债；0.4 完成 daemon → uc-application 切换一次到位，不走"uc-app facade 内部委托"的过渡态（与 D13 对齐） |
| D26 | **B2 方案：pairing 协议层作为 `uc-application::pairing` 永久独立模块存在，不合并进 space_access**（2026-04-17 修订 D15/§5.1） | 0.4 及以后 | D15 原文"协议状态机并入或整合进 space_access"改读为**语义整合**（模块协作 + 共享事件/context），而非**物理合并**到同一文件夹。理由：pairing 协议（两台设备握手 + 建立信任关系）和 space_access（把远端身份升级为空间成员）是两个业务边界；强制物理合并会过度耦合，且阶段 A 会再搬一次 pairing（中间态）。B2 下 0.4 把 pairing 直接落到 `uc-application/pairing/` 独立模块，阶段 A 只做 space_access 搬家 + 消费 pairing 事件 |
| D27 | **迁移期允许 `uc-app` 依赖 `uc-application`（修订 §9）**（2026-04-17） | 0.4 及以后 | §9 原文"uc-app 不新增对 uc-application 的 crate 依赖"改读为：uc-app **不新增业务逻辑**也不反向污染 uc-application，但**允许作为过渡期消费方**引用 uc-application 已搬走的类型。理由：B2 方案下 pairing 搬离后，uc-app 残余 setup / space_access 有 4 处 import（`FailureReason` / `PairingDomainEvent` / `PairingEventPort` / `PairingOrchestrator` / `StagedPairedDeviceStore`）必须从 uc-application 引入；否则被迫把 setup/space_access 也一起搬家（扩大为方案 C）。依赖方向仍是 uc-app → uc-application（退出方向），与 D13 精神一致；阶段 C 清退 uc-app 后依赖自然消失 |
| D28 | **`TrustPeerOrchestrator` 增加 `reset()` 方法以支持单例多次流程复用**（2026-04-17） | 0.4.2 | `state_machine.rs` 的 `transition()` 纯函数契约要求 `Trusted` / `Aborted` 终态拒绝所有后续事件（有 `terminal_{trusted,aborted}_rejects_further_events` 两条测试写死），但 D19 规定 orchestrator 以全局单例装配，第二次 pairing flow 就会被终态 `IllegalTransition` 拒绝。三条候选路径：(1) 加 `reset()` 方法从任意状态回 `Idle`；(2) 放宽 `transition()` 允许 `Trusted | Aborted` + `Initiate` 回到 `EstablishingSession`（改纯函数契约，要改 9 条单元测试）；(3) 0.4.2 绕过 orchestrator 直接调 `TrustPeerUseCase::execute()`。选 (1) — 契约最清晰：终态 = 一次流程结束，`reset()` 表达"开启新流程"，纯函数转移表不变。protocol_handler 在 PersistPairedDevice 分支先 `reset` 再 `initiate→record_session_opened→confirm_verification`。`TrustPeerOrchestrator<R>` / `TrustPeerUseCase<R>` / `ConfirmPeerVerificationUseCase<R>` / `CancelTrustingUseCase<R>` 同步放宽为 `R: ?Sized`，允许 `R = dyn TrustedPeerRepositoryPort` 作为 bootstrap 注入类型 |
| D29 | **0.4.2 不再把 `FailureReason` / `PairingBusy` 错误类型从代码树彻底删除，只切换写入路径**（2026-04-17） | 0.4.2 → 0.4.2.b | D24 原文要求 0.4.2 删 `FailureReason`，但实测 `state_machine.rs` 内部 100+ 处引用（`PairingState::Failed { reason: FailureReason }` + 大量 `FailureReason::*` 构造路径），且 `PairingState` 留到 0.5 再删（§5.4 出口条件 5 推迟）——真正删除必须等 `state_machine.rs` 整体重写。0.4.2 现实可完成的是：写入路径切到 `TrustPeerOrchestrator`，公共面 `PairingDomainEvent::PairingFailed::reason: FailureReason` 暂保留，daemon / action_executor 对 `FailureReason` 的 match 不破坏；FailureReason 类型完全移除拆到新子步 **0.4.2.b**（改 `PairingDomainEvent::PairingFailed::reason` → `String` 或新枚举，daemon / setup 同步收敛）。保障 0.4.2 commit 体量可控（~500 行），且风险最集中的写入路径切换不被 FailureReason 大规模重命名混淆 |
| D30 | **0.4.4 用 `PairingFacade` 替代原计划的"UseCase thin wrapper"作为 daemon 入口**（2026-04-17） | 0.4.4 及以后 | 原计划要求 daemon 改调 `ConfirmPeerVerificationUseCase` / `CancelTrustingUseCase`；实测这两个 UseCase 位于 trust_peer 层级、由 pairing protocol handler 在 `PersistPairedDevice` 内部驱动（0.4.2 落地），**daemon 并不直接触达 trust_peer 层**。daemon 用户动作实际是 pairing-level 的 short-code 确认/拒绝/取消。若只在 daemon 层"包一层 UseCase"但仍暴露 `Arc<PairingOrchestrator>`，封装无意义。最终采用 **External → Facade → Orchestrator → Ports** 边界：`PairingFacade` 对外公开，`PairingOrchestrator` 变 `pub(crate)`；Facade 内部组合三个 `pub(crate)` 级 thin-wrapper UseCase（`AcceptPairingUseCase` / `RejectPairingUseCase` / `CancelPairingUseCase`）处理 user-intent 入口，网络事件分派/会话查询/`PairingEventPort` 订阅由 Facade 直接委托给内部 orchestrator。`bootstrap` / `daemon` / `setup pairing_facade` 全部改用 `PairingFacade`。符合 AGENTS.md §11 "应用 Facade" 定位 |

##### 阶段 0.4 已决事项（2026-04-17 拍板）

上一版列出的 7 条待决疑问已全部有答案：

| # | 疑问 | 结论 | 证据 |
|---|---|---|---|
| Q1 | `protocol_handler` 当前如何驱动 `PairedDevice` 写入？ | `PairingAction::PersistPairedDevice` 分支：`device_repo.upsert` @ `protocol_handler.rs:284` → `dual_write_member` @ `:287`；事件反馈 @ `:295-302`；超时 @ `:351-361`；领域事件广播 @ `:145-154, :194-201, :462-464`。0.4 把这些挂点改为调 `TrustPeerOrchestrator::{initiate, record_session_opened, confirm_verification, cancel, record_timeout, record_protocol_error}`，原 `device_repo.upsert` / `dual_write_member` 同步删除 | `uc-app/pairing/protocol_handler.rs` |
| Q2 | `TrustedPeerEvent` 是否在 0.4 发出？ | **否**（D23）。`orchestrator.rs` 无事件通道，阶段 A 再加 | `uc-application/trusted_peer/orchestrator.rs` |
| Q3 | `SpaceAccessContext` 改造面？ | 现有 7 字段无 `device_name` / `peer_fingerprint`；0.4 新增 `pub peer_device_name: Option<String>` + `pub peer_fingerprint: Option<PeerFingerprint>`，写入时机是 pairing 协议原本触发 `PersistPairedDevice` 的事件路径（`KeyslotReceived` / `PairingSucceeded`） | `uc-app/space_access/context.rs:22-30` |
| Q4 | `PairingMessage` / `PairingBusy` 删除范围？ | D24 缩范围：0.4 只删 `FailureReason` / `PairingBusy` 等错误类型并翻译到 `TrustStateEvent` 三档；`PairingMessage` 报文保留（99 处引用，跨 `uc-platform` / `uc-daemon`），留到阶段 A | `uc-core/network/protocol/pairing.rs:7, :163` |
| Q5 | `StagedPairedDeviceStore` 调用方清单？ | 22 处引用，关键挂点：`uc-app/pairing/protocol_handler.rs:41, :278`；`uc-app/pairing/orchestrator.rs:36`；`uc-bootstrap/builders.rs:56, :76, :151, :166`、`assembly.rs:828`；`uc-app/space_access/persistence_adapter.rs:24, :31, :39`；`uc-app/pairing/mod.rs:30`。切换顺序：先改 `persistence_adapter` 用 `TrustedPeerRepositoryPort::get(...).is_some()` 替代 → 再改 `protocol_handler` / `orchestrator` → 最后删 staged store 文件 + bootstrap 字段 | — |
| Q6 | `local_device_id` 从哪取？ | `DeviceIdentityPort`（uc-core port，实现 `uc-infra::device::LocalDeviceIdentity::load_or_create()` @ `assembly.rs:453-456`）；取用方式 `deps.device.device_identity.current_device_id().to_string()`（`builders.rs:134, :150`） | `uc-core::DeviceIdentityPort` |
| Q7 | uc-app 侧保留薄 facade？ | 否（D25）。`uc-app/pairing/facade.rs:1-17` 当前未被 daemon 使用（daemon 直 import orchestrator 与 `FailureReason`），0.4 删除 uc-app 的 pairing facade，daemon 直切 `uc-application::trusted_peer` | `uc-daemon/pairing/host.rs:11-17` |

##### 阶段 0.4 commit 拆分（2026-04-17 · 方案 B2）

按下列顺序提交，每个 commit 单独编译 + 测试通过；核心思路：**先搬家后切写入**，搬家阶段保持行为不变降低风险：

| commit | 范围 | 关键动作 |
|---|---|---|
| **0.4.1** | pairing 协议层平移（行为不变） | 新建 `uc-application/src/pairing/`（`mod.rs` + 6 个协议层文件：`protocol_handler.rs` / `orchestrator.rs` / `session_manager.rs` / `state_machine.rs` / `crypto.rs` / `events.rs`）；删除 `uc-app/src/usecases/pairing/` 中对应 6 个文件；bootstrap（`assembly.rs` / `builders.rs`）和 daemon（`uc-daemon/pairing/host.rs`）的 import 路径从 `uc_app::usecases::pairing::*` 改为 `uc_application::pairing::*`；搬家过程中 **`dual_write_member` / `device_repo.upsert` / `staged_paired_device_store` 全部保留**，写入目标仍是 `paired_device` + `space_member`（Phase 2 现状）；所有现有单元测试 / 集成测试跟随搬家并跑绿 |
| **0.4.2** | ✅ 写入路径切换到 `TrustPeerOrchestrator` | `uc-application/pairing/protocol_handler.rs`：`PairingAction::PersistPairedDevice` 分支替换为 `trust_peer_orchestrator.reset().await` + `initiate` + `record_session_opened` + `confirm_verification`（D28 reset）；删除 `device_repo.upsert` / `dual_write_member` / `space_member_from_paired_device` helper；`PairingOrchestrator::new` / `PairingProtocolHandler::new` 不再需要 `PairedDeviceRepositoryPort` / `MemberRepositoryPort`；bootstrap（`assembly.rs` + `builders.rs`）构造 `TrustPeerOrchestrator<dyn TrustedPeerRepositoryPort>` 进程内单例（D19），通过 `WiredDependencies.trusted_peer_repo` 暴露给 GUI / daemon 两个入口；`TrustPeerOrchestrator` 新增 `reset()` 方法（D28）以支持多次 pairing 流程；`TrustPeerUseCase` / `TrustPeerOrchestrator` / `ConfirmPeerVerificationUseCase` / `CancelTrustingUseCase` 放宽 `R: ?Sized` 支持 dyn 注入；**`FailureReason` 类型保留不删**（D29 — 推到 0.4.2.b）；重写本文件 4 个 Phase 2 dual_write 单元测试为 trusted_peer 断言（`trust_flow_{persists_trusted_peer_on_success, rejects_second_pairing_for_same_peer, reset_allows_pairing_another_peer, peer_device_id_matches_peer_id_string}`） |
| **0.4.2.b** | ✅ 收口 `FailureReason` → `TrustAbortReason` | `PairingDomainEvent::PairingFailed::reason` 类型从 `FailureReason` 改为 `uc_core::TrustAbortReason`（D24 三档：UserCancelled / Timeout / ProtocolError）；`PairingAction::EmitResult` 新增 `abort_reason: Option<TrustAbortReason>` 字段，state machine 在 `fail_with_reason` / `cancel_with_reason` / PersistOk / PersistErr 四个发射点显式填充；新增 `abort_reason_from_failure(&FailureReason) -> TrustAbortReason` 翻译器；`uc-application/pairing/mod.rs` 移除 `FailureReason` re-export；`uc-daemon/pairing/host.rs` `pairing_failure_message` 改为 `&TrustAbortReason`（3-variant string mapping）；`uc-app/setup/action_executor.rs` `map_pairing_failure_reason` 改为 `&TrustAbortReason`（旧的字符串子串匹配路径删除）；`FailureReason` 类型本体保留在 `state_machine.rs` 作为内部实现细节（0.5 随 PairingState 一起删除） |
| **0.4.3** | ✅ 切断 space_access 的反向依赖 + 删 staged store / facade | `uc-app/src/usecases/space_access/persistence_adapter.rs` 改用 `TrustedPeerRepositoryPort::get(...).is_some()` 替代 `staged_paired_device_store` 查询（`TrustPromotionSource::Staged` → `TrustPromotionSource::TrustedPeer`）；删除 `uc-application/src/pairing/staged_paired_device_store.rs`；删除 `uc-application/src/pairing/facade.rs`（D25 — 当前无调用方，`PairingOrchestrator` 的 impl block 同步删除）；`uc-application/src/pairing/mod.rs` 移除对 staged store / facade 的 re-export；`PairingOrchestrator::new` / `PairingProtocolHandler::new` / `execute_action_inner` / `handle_timeout` 移除 `staged_store: Arc<StagedPairedDeviceStore>` 参数和字段；protocol_handler 的 `PersistPairedDevice` 分支删除 `staged_store.stage(...)` 调用；bootstrap（`assembly.rs` / `builders.rs`）摘掉 `StagedPairedDeviceStore::new()` 构造与 `GuiBootstrapContext.staged_store` / `DaemonBootstrapContext.staged_store` 字段；`SetupAssemblyPorts` 新增 `trusted_peer_repo: Arc<dyn TrustedPeerRepositoryPort>` 字段（`from_network` 新增参数；`placeholder` 用 `NoopTrustedPeerRepository`）由 `build_setup_orchestrator` 喂给 `SpaceAccessPersistenceAdapter`；daemon entrypoint 与 main.rs 同步收口 |
| **0.4.4** | ✅ daemon UI 触发路径切到 `uc-application` Facade | 新建 `uc-application/pairing/facade.rs` 的 `PairingFacade`，作为 External 唯一入口；`PairingOrchestrator` 降级为 `pub(crate)`（不再对外暴露）；`PairingFacade` 内部组合三个 `pub(crate)` 级 UseCase（`AcceptPairingUseCase` / `RejectPairingUseCase` / `CancelPairingUseCase`，D22 thin wrapper）处理 user-intent 入口，网络事件分派 / 会话查询 / event 订阅直接委托给内部 orchestrator；`impl PairingEventPort` 从 orchestrator 移到 Facade；`SetupPairingFacadePort` 的 blanket impl 从 `PairingOrchestrator` 改挂 `PairingFacade`（method 调用从 `user_accept_pairing` / `user_reject_pairing` / `user_cancel_pairing` 切到 `accept_pairing` / `reject_pairing` / `cancel_pairing`）；`uc-bootstrap`（`builders.rs` / `assembly.rs`）构造 `PairingFacade::new` 并把 `pairing_orchestrator` 字段改名为 `pairing_facade`（`GuiBootstrapContext` / `DaemonBootstrapContext` / `SetupAssemblyPorts::from_network`）；`uc-daemon/pairing/host.rs` 所有 `pairing_orchestrator` 引用切到 `pairing_facade`；`src-tauri/src/main.rs` 同步改名；`FailureReason` 模式匹配收口工作在 0.4.2.b 已经落地，0.4.4 只确认 daemon 侧无残留。边界上保证 External → Facade → Orchestrator → Ports 单向依赖。<br/>**计划偏离说明**：原计划写"调 `ConfirmPeerVerificationUseCase` / `CancelTrustingUseCase`"，但这两个 UseCase 在 trusted_peer 层级（state 为 `AwaitingUserVerification`），由 pairing protocol handler 在 `PersistPairedDevice` 分支内部驱动（0.4.2 已落地），daemon 不直接触达；daemon 的用户动作实际是 pairing-level（short-code 确认 / 拒绝 / 取消），因此 0.4.4 在 pairing 层级新增三个 thin-wrapper UseCase，并以 Facade 统一收口，语义更贴合 AGENTS.md §11 的"应用 Facade" 规范 |

**方案 B2 下 `uc-app/pairing/` 的最终状态**：

| 文件 | 0.4 后状态 |
|---|---|
| `protocol_handler.rs` / `orchestrator.rs` / `session_manager.rs` / `state_machine.rs` / `crypto.rs` / `events.rs` | 搬到 `uc-application/pairing/`（0.4.1） |
| `staged_paired_device_store.rs` / `facade.rs` | 删除（0.4.3） |
| `list_paired_devices.rs` / `unpair_device.rs` / `update_device_sync_settings.rs` / `get_device_sync_settings.rs` / `resolve_connection_policy.rs` / `list_sendable_peers.rs` / `get_local_device_info.rs` / `get_p2p_peers_snapshot.rs` / `dto.rs` | **留在 uc-app**，Phase 3 再按 §6.1 切换表逐个处理 |

**阶段 0.4 不做的事**（留给阶段 A 或 0.5）：
- 不发 `TrustedPeerEvent`（D23）
- 不删 `PairingMessage` 报文结构（D24）
- 不扩 `SpaceAccessContext` 的 `peer_device_name` / `peer_fingerprint` 字段（推到阶段 A —— 届时 space_access 搬进 uc-application 后，可以和 `uc-application::pairing` 在同 crate 内直接共享）
- 不删 `PairedDevice` / `PairingState` / `PairedDeviceRepositoryPort` Rust 类型（留给 0.5）
- 不碰 `uc-app/pairing/` 余下的 9 个非协议类文件（Phase 3）
- 不在 `SpaceAccessState::Granted` 调 `AdmitMemberUseCase`（阶段 A）

#### 阶段 A — `space_access` 搬家 + admit 挂点

**产物**：`uc-application::space_access` 成为唯一应用层入口；`AdmitMemberUseCase` 在 `SpaceAccessState::Granted`（**仅 joiner 侧**）被调用。

| # | 动作 | 说明 |
|---|---|---|
| A.1 | 新建 `uc-application/src/space_access/{mod.rs, errors.rs, context.rs, events.rs, orchestrator.rs, executor.rs, crypto_adapter.rs, network_adapter.rs, persistence_adapter.rs, proof_adapter.rs, usecases/}` | 文件骨架从 `uc-app/usecases/space_access/` 平移，公开 API 面保持不变；`persistence_adapter` 改为通过 `TrustedPeerRepositoryPort` 落盘（阶段 0.4 已清 staged store） |
| A.2 | `SpaceAccessOrchestrator` 注入泛型 `Option<Arc<AdmitMemberUseCase<R>>>`；在 `Granted` 转移点调用 `admit_member.execute(...)` | 仅 joiner 角色触发；失败只 WARN，不影响 `Granted` 返回（与 `dual_write_member` 同源语义） |
| A.3 | `uc-bootstrap` 装配切换：构造 `AdmitMemberUseCase::new(member_repo.clone())` 注入 orchestrator；`uc-app` 旧 space_access `#[deprecated]` 不删 | `trusted_peer` + `space_member` 双写在阶段 0.4 已就位，admit 只是多出一条路径 |
| A.4 | `uc-application/tests/space_access_admit_member.rs`：joiner `Granted` → repo 能 `get`；`Denied` 不写入；sponsor 侧不触发 admit | 同时保留 `uc-app` 原测试跑绿 |
| ~~A.5~~ | ~~pairing 侧改造：事件写 `device_name` / `fingerprint` 到 `SpaceAccessContext`~~ | **已在阶段 0.4 完成**，阶段 A 不再需要 |

**阶段 A 出口条件**：
1. `uc-application/src/space_access/` 编译通过 + 新增测试全绿
2. `uc-app/usecases/space_access/` 标 `#[deprecated]` 但仍可编译
3. daemon 仍从 `uc-app` 路径消费 space_access（阶段 B 再切换）
4. CI 全绿；双写行为无回归

**阶段 A 拆 commit**：
- commit 1：A.1 骨架搬家（纯搬运，零行为变更，反向依赖已由阶段 0 清除）
- commit 2：A.2 + A.3 admit 注入 + 装配切换
- commit 3：A.4 测试

#### 阶段 B — `setup` 语义化搬家（状态机拉回 + 拆 UseCase）

**产物**：setup 完全脱离 `uc-app`；状态机回归 application 层；`SetupOrchestrator` 变内部实现（决策细则 #2），daemon 只看见 `SetupFacade`。

| # | 动作 | 说明 |
|---|---|---|
| B.1 | `uc-core/src/setup/` 删除 `state.rs` / `event.rs` / `action.rs` / `state_machine.rs` / `error.rs`（仅保留 `status.rs`）；顶层 `lib.rs` re-export 清理 | 纠偏 `uc-core/AGENTS.md` §9.1 |
| B.2 | 新建 `uc-application/src/setup/{mod.rs, state.rs, events.rs, actions.rs, state_machine.rs, errors.rs, context.rs, orchestrator.rs, action_executor.rs, pairing_facade.rs, facade.rs, commands.rs, queries.rs, usecases/}` | 状态机直接从 core 平移；orchestrator 薄化为"dispatch 循环 + context + cancel/reset" |
| B.3 | 把 `SetupOrchestrator` 的 13 个公开方法**拆为独立 UseCase** — 命名见 §5.4.1 清单 | 所有 UseCase 共享同一个 `Arc<SetupOrchestrator>`（避免状态分散）；`SetupOrchestrator` **不 re-export**，只 `pub(crate)` |
| B.4 | `SetupFacade` 作为 daemon 的唯一入口，内部路由到具体 UseCase | AGENTS §11 薄 Facade |
| B.5 | `uc-bootstrap` 切换：daemon 改依赖 `uc_application::setup::SetupFacade`；`uc-app/usecases/setup/` 标 `#[deprecated]` 不删 | 保留 `uc-app` 旧 setup 到阶段 C |
| B.6 | 测试按 UseCase 粒度重写到 `uc-application/tests/setup/`（`mockall` + port mock） | AGENTS §17.4 |

**B.3 的 UseCase 拆分清单**：

| 原 `SetupOrchestrator` 方法 | 目标 UseCase / Query |
|---|---|
| `new_space()` | `StartNewSpaceUseCase` |
| `join_space()` | `StartJoinSpaceUseCase` |
| `select_device(peer_id)` | `SelectJoinPeerUseCase` |
| `confirm_peer_trust()` | `ConfirmPeerTrustUseCase` |
| `submit_passphrase(p1, p2)` | `SubmitNewSpacePassphraseUseCase` |
| `verify_passphrase(p)` | `VerifyJoinPassphraseUseCase` |
| `complete_join_space()` | `CompleteJoinSpaceUseCase` |
| `cancel_setup()` | `CancelSetupUseCase` |
| `reset()` | `ResetSetupUseCase` |
| `clear_transient_state()` | `ClearSetupTransientStateUseCase` |
| `get_state()` | `GetSetupStateQuery` |
| `start_completed_host_sponsor_authorization(...)` | `StartSponsorAuthorizationForJoinerUseCase` |
| `resolve_host_space_access_proof(...)` | `ResolveHostSpaceAccessProofUseCase` |
| `apply_joiner_space_access_result(...)` | `ApplyJoinerSpaceAccessResultUseCase` |

**阶段 B 出口条件**：
1. `uc-application/src/setup/` 编译通过 + 新增 UseCase 级测试全绿
2. `uc-core/src/setup/` 只剩 `status.rs`
3. daemon 不再 import `uc_app::usecases::setup::*`
4. `uc-app::usecases::setup` 标 `#[deprecated]` 但仍可编译（阶段 C 再删）
5. admit 仍只挂在 space_access `Granted`（`CompleteJoinSpaceUseCase` 不调 admit）

#### 阶段 C — `uc-app` 旧 setup / 旧 space_access 清退

**触发条件**：阶段 B 完成且 daemon 切换稳定运行一个迭代周期无回滚。

| # | 动作 |
|---|---|
| C.1 | 删除 `uc-app/src/usecases/setup/` |
| C.2 | 删除 `uc-app/src/usecases/space_access/` |
| C.3 | `uc-app/src/lib.rs` 和 `usecases/mod.rs` 移除相应导出 |
| C.4 | `grep` 验证 `uc-app` 不再被任何 crate import `setup::` / `space_access::` |
| C.5 | 更新本文档 §0 和 §5.4 状态标注 → 进入 Phase 3（消费者切换） |

**阶段 C 出口条件**：`uc-app` 里 setup / space_access 模块完全消失；`cargo tree` 验证只有 `uc-application` 作为应用层承载。

---

## 6. Phase 3 — 消费者切换（修正版；等 `space_access` 搬完后启动）

**原则**：
- UI 触发的读/写 → daemon 直接调 `uc-application::membership` use case，**不经 uc-app**
- 系统内部高频查询 → 直接用 `MemberRepositoryPort` port
- **修正版**：不在 uc-app 里做"改数据源"这种过渡操作；凡 UI 相关的 use case，直接从 uc-app 删除，daemon 调用点切到 uc-application

### 6.1 切换表

| 子阶段 | 消费者 | 做法 | 依赖 |
|---|---|---|---|
| **3.1** | `uc-app/usecases/pairing/resolve_connection_policy.rs` | 查询源从 `paired_device_repo.get_by_peer_id` 换成 `member_repo.get(DeviceId::new(peer_id.as_str()))` | 若 `pairing` 整体搬到 `space_access`，此步与搬家同时完成 |
| **3.2** | `uc-app/usecases/pairing/list_sendable_peers.rs` | "在 member 列表 ⇒ 可发" | 同上 |
| **3.3** | daemon 的 unpair 路径 | 改调 `uc-application::membership::RevokeMemberUseCase`；**删除** `uc-app/usecases/pairing/unpair_device.rs` | D13（daemon 已依赖 uc-application） |
| **3.4** | daemon 的 get/update device_sync_settings 路径 | 改调 `GetMemberUseCase` / `UpdateMemberSettingsUseCase` / `ResetMemberPreferencesToDefaultUseCase`；**删除** uc-app 对应 use case | 同 3.3 |
| **3.5** | daemon 的 list_paired_devices 路径 | 改调 `ListMembersUseCase`；**删除** `uc-app/usecases/pairing/list_paired_devices.rs` | 同 3.3 |
| **3.6** | daemon DTO 一次性改名 | `PairedDeviceDto → SpaceMemberDto`，`PairedDevicesChangedPayload` 相应改；前端 9 个 TS 文件（`devicesSlice.ts` / `PairedDevicesPanel.tsx` / `PairedPeer` type 等）联动改 | — |

### 6.2 一次性改名（D9 对应）

Phase 3.6 不再单独作为兼容过渡步骤——**DTO 改名 + 前端字段名切换一次完成**。期间前端有一次"破坏性 PR"，不留双写 DTO。

### 6.3 每一步必须回答

- 切换后这个消费者的**失败语义**和原来一致吗？
- **测试路径**：能否触发对应行为？
- **事务边界**：是否有半成功需要考虑？
- **DTO/字段**：前端还有没有别的地方依赖旧字段？

---

## 7. Phase 4 — 删双写（等 Phase 3 完成）

**触发条件**：Phase 3.1~3.6 跑稳一个迭代周期无回滚 + `space_access` 搬家已完成。

动作：

1. `space_access` 的 admit 调用链成为**唯一写入 `space_member` 的入口**
2. 删除 `uc-app/pairing/protocol_handler.rs` 里的 `dual_write_member` 调用（或整个 `protocol_handler.rs` 随 pairing 搬家一并消失）
3. `paired_device` 表只读，不再接收新写入
4. `DevicePorts::paired_device_repo` 视使用情况决定：
   - 若 `resolve_connection_policy` / `list_sendable_peers` 已切到 `member_repo`，可删除字段
   - 若还有残余读取，保留一段

---

## 8. Phase 5 — 彻底清理（等 Phase 4 完成）

**触发条件**：Phase 4 完成 + 一个稳定版本。

动作：

1. `uc-core/src/pairing/paired_device.rs` — 删除 `PairedDevice` / `PairingState`（D8）
2. `uc-core/src/ports/paired_device_repository.rs` — 删除 `PairedDeviceRepositoryPort`
3. `uc-core/src/ports/errors.rs` — 删除 `PairedDeviceRepositoryError`
4. `uc-infra/src/db/models/paired_device_row.rs` — 删除
5. `uc-infra/src/db/mappers/paired_device_mapper.rs` — 删除
6. `uc-infra/src/db/repositories/paired_device_repo.rs` — 删除
7. `uc-infra/src/db/schema.rs` — 删除 `paired_device` table! 条目
8. **新 migration** `drop_paired_device`（不删历史 migration 文件）：
   ```sql
   DROP TABLE paired_device;
   ```
9. grep 验证 `PairedDevice` / `PairingState` / `paired_device` 归零

---

## 9. 跨 Phase 不动的约束

- `uc-core` 对 `tokio` / `diesel` / `libp2p` 的依赖：永远 0
- `uc-app` 可以作为过渡期消费方依赖 `uc-application`（D27）；不新增业务逻辑、不反向污染 uc-application
- `MemberRepositoryPort` 接口不再加方法（`get_by_peer_id` 之类），因为 D5 约定让 `get(DeviceId::new(peer_id.as_str()))` 足够用
- 前端协议改名只在 **Phase 3.6** 做一次

---

## 10. 未解决 / 待决策

| 项 | 描述 | 触发时机 |
|---|---|---|
| U1 | `space_access` 搬家后 `uc-app/pairing` 的边界拆分：哪些进 space_access，哪些沉到 uc-core/uc-infra | space_access discuss-phase |
| U2 | 3.4 全量 vs patch 的 daemon 兼容策略 | 进入 3.4 前 |
| U3 | 3.3 双写 revoke 的原子性（member + paired_device 能不能放同一事务） — 若 3.3 和 pairing 搬家同时发生则可能不存在此问题 | 进入 3.3 前 |
| U4 | Phase 5 的 `DROP TABLE paired_device` migration 是否要保留一个 `.bak` 拷贝给 rollback | 进入 Phase 5 前 |
| U5 | `resolve_connection_policy` / `list_sendable_peers` 在 pairing 搬家后是否仍存在；若存在，归属到哪个模块 | space_access 搬家设计阶段 |

---

## 11. 已提交记录

```
befbbdfe  feat(membership): dual-write space_member during pairing (Phase 2)
5f5c6f4c  feat(membership): add Diesel SpaceMember repository and data migration
1af58f34  feat(membership): add SpaceMember domain model and use cases
```

分支：`milestone/0.6.0`。
