# Trusted Peer Domain 设计文档（临时固化文档）

> 范围：定义 `uc-core::trusted_peer` / `uc-application::trusted_peer` 的领域语义，作为 `MEMBERSHIP_MIGRATION_PLAN_ZH.md` §5.4 **阶段 0** 实施的唯一权威来源。
> 状态：**规范已固化，阶段 0 实施未开始**。
> 迁移完成后（阶段 0.5 结束 + MIGRATION_PLAN Phase 5 完成），本文可删。

---

## 1. 文档目的

固化 **"两台设备建立可通信信任关系"** 这一领域的建模决策，避免阶段 0 实施过程中出现命名漂移、边界污染、设计返工。

本规范同时约束：
- `uc-core::trusted_peer` 的纯领域模型
- `uc-application::trusted_peer` 的流程编排
- 迁移期间的双写策略和消费者切换顺序

---

## 2. 一句话定义

> **TrustedPeer 是"本机已认可可通信的对端设备"这一事实的持久化表达。**
> 建立与撤销由应用层流程驱动；底层采用何种传输协议（libp2p / TCP / WebSocket / NFC）由 `network` 层决定，对本 domain 透明。

---

## 3. 核心原则

| # | 原则 | 含义 | 负例 |
|---|---|---|---|
| P1 | **关系而非动作** | Aggregate 是"已存在的信任关系"，不是"配对动作" | 旧 `PairedDevice` 把动作状态（Pending）塞进持久实体 |
| P2 | **身份而非显示** | core 只存"身份本体"（`PeerFingerprint`），不存表示形式（short code / QR / emoji） | `ShortCode` 不进 core —— 它是表示层关切 |
| P3 | **单一职责** | trusted_peer 只管"可通信关系"，不管同步偏好 / 设备显示名 / 连接策略 | 旧 `PairedDevice.sync_settings` / `device_name` 属于其他 domain |
| P4 | **硬删模型** | 不信任 = 从 repository 中删除，没有 `Distrusted` 中间态 | 沿用 membership 的 D4 决策 |
| P5 | **传输无关** | 不依赖 libp2p / TCP / 任何具体传输协议 | 握手消息协议归 `uc-core::network::pairing` |
| P6 | **smoke test** | 换一种 UI 方案（NFC / QR / passkey）或换一种协议版本，domain 都不用改 | `ShortCode` 在 core 会被 NFC 打脸；`PeerFingerprint` 不会 |

---

## 4. Domain 模型（`uc-core::trusted_peer`）

### 4.1 Aggregate Root

```rust
pub struct TrustedPeer {
    pub local_device_id: DeviceId,
    pub peer_device_id: DeviceId,
    pub peer_fingerprint: PeerFingerprint,
    pub trusted_at: DateTime<Utc>,
}
```

**字段**：
- `local_device_id` / `peer_device_id`：两个端点标识，沿用现有 `DeviceId`（D5 延续）
- `peer_fingerprint`：对端公钥的规范指纹，**重连时验证"还是原来那台"的唯一凭据**
- `trusted_at`：信任建立的事实时间点（非 `established_at`，后者动作色彩过重）

**显式排除**（与 `PairedDevice` 上帝对象切割）：

| 被排除字段 | 归属 |
|---|---|
| ❌ `device_name` | `uc-core::device` 或 `SpaceMember.device_name` |
| ❌ `sync_settings` | `uc-core::membership::MemberSyncPreferences` |
| ❌ `state` / `pairing_state` | 不信任即删除（硬删模型），无中间态 |
| ❌ `last_seen_at` | 网络层（MIGRATION_PLAN D6） |
| ❌ 作为网络地址的 `peer_id` | D5 `DeviceId == peer_id` 字符串直接复用 |

### 4.2 值对象

```rust
pub struct PeerFingerprint(String);
```

- 对端公钥的规范指纹（具体派生算法在 `network::pairing` 协议层，不进本 domain）
- 跨传输、跨会话、跨重启稳定
- 命名用 `Peer` 前缀强调"对端视角"

### 4.3 领域事件（过去式）

```rust
pub enum TrustedPeerEvent {
    /// 需要用户对对端身份做视觉验证。
    /// 只携带 fingerprint，不携带 short_code（后者是 application 层从 fingerprint 派生的视图）
    PeerVerificationRequired { peer_fingerprint: PeerFingerprint },

    /// 信任关系已建立（application 层在此触发 `AdmitMemberUseCase`）
    PeerTrusted { trusted_peer: TrustedPeer },

    /// 信任关系已撤销
    PeerDistrusted { peer_device_id: DeviceId },

    /// 建立流程被放弃（用户取消 / 超时 / 协议错误）
    PeerTrustAborted { reason: TrustAbortReason },
}
```

### 4.4 Port

```rust
#[async_trait]
pub trait TrustedPeerRepositoryPort: Send + Sync {
    async fn get(&self, peer_device_id: &DeviceId) -> Result<Option<TrustedPeer>, TrustedPeerError>;
    async fn list(&self) -> Result<Vec<TrustedPeer>, TrustedPeerError>;
    async fn save(&self, trusted_peer: &TrustedPeer) -> Result<(), TrustedPeerError>;  // upsert
    async fn remove(&self, peer_device_id: &DeviceId) -> Result<bool, TrustedPeerError>;
}
```

### 4.5 错误

```rust
pub enum TrustedPeerError {
    AlreadyTrusted(DeviceId),
    NotFound(DeviceId),
    Repository(String),
}
```

---

## 5. Application 层（`uc-application::trusted_peer`）

### 5.1 UseCases

| UseCase | 对应动作 | 典型触发 |
|---|---|---|
| `TrustPeerUseCase` | 落盘新的 `TrustedPeer`（状态机内部调用） | 用户完成视觉验证后 |
| `ConfirmPeerVerificationUseCase` | 用户确认对端身份 | UI 点击"确认是这台" |
| `CancelTrustingUseCase` | 用户中途取消信任建立 | UI 点击"取消" |
| `DistrustPeerUseCase` | 撤销已建立的信任（问题 #4 方案 A） | 用户在设备列表"解除信任" |

### 5.2 Queries

| Query | 输出 |
|---|---|
| `ListTrustedPeersQuery` | `Vec<TrustedPeer>` |
| `GetTrustedPeerQuery { peer_device_id }` | `Option<TrustedPeer>` |

### 5.3 TrustVerificationChallenge（应用层视图，不进 core）

```rust
pub struct TrustVerificationChallenge {
    pub peer_fingerprint: PeerFingerprint,
    pub short_code: String,  // 派生算法在 network::pairing
    // 未来扩展: pub qr_payload: Option<String>, pub nfc_token: Option<Vec<u8>>
}
```

由 orchestrator 在收到 `PeerVerificationRequired` 事件时构造，传递给 UI。

### 5.4 状态机（application 层）

```text
Idle
  ↓ (initiate)
EstablishingSession
  ↓ (session opened, fingerprint exchanged)
AwaitingUserVerification(challenge)
  ↓ (user confirms)
Trusted(trusted_peer)   ← 终态

任意态 ↓ (user cancels / timeout / protocol error)
Aborted(reason)         ← 终态
```

**终态仅两个**：`Trusted` / `Aborted`。中间态不持久化。

---

## 6. 与相邻 domain 的交互契约

### 6.1 `trusted_peer` → `space_access`

- `PeerTrusted` 事件是 space_access `SponsorAuthorizationRequested` / `JoinRequested` 的前置
- `space_access` 通过 `TrustedPeerRepositoryPort::get(...)` 或订阅 `PeerTrusted` 事件确认对端信任已建立
- `space_access` **不触发**信任建立，也不触发撤销

### 6.2 `trusted_peer` → `membership`

- `space_access::Granted` 时调用 `AdmitMemberUseCase` —— 此时 `TrustedPeer` 必然已存在
- `AdmitMember` 的输入 `identity_fingerprint` 等价于对应 `TrustedPeer.peer_fingerprint`（同一加密指纹）
- 两个 domain **不共享类型**，各自演化独立的值对象（`PeerFingerprint` vs `SpaceMember.identity_fingerprint: String`）

### 6.3 `trusted_peer` ↔ `network::pairing`

`network::pairing`（`uc-core::network`）负责：
- 握手消息协议（`PairingMessage`, `PairingBusy`）
- short code 的派生算法（两端必须一致 —— 协议绑定）
- fingerprint 的加密生成

`trusted_peer` 不感知协议细节。两者通过 application 层 orchestrator 连接：
- orchestrator 订阅网络协议事件，翻译为 `trusted_peer` 领域事件
- orchestrator 消费 `trusted_peer` UseCase 结果，调用网络层发送响应

### 6.4 `trusted_peer` ↔ `setup`

- `setup.JoinSpaceConfirmPeer` 状态消费 `TrustVerificationChallenge`（application 层视图），展示 short_code
- `setup.ConfirmPeerTrust` 事件触发 `ConfirmPeerVerificationUseCase`
- setup 不直接操作 `TrustedPeerRepositoryPort`

---

## 7. 不属于本 domain（明确排除清单）

| 现有内容 | 归属 | 原因 |
|---|---|---|
| `PairedDevice.device_name` | `uc-core::device` / `SpaceMember.device_name` | 显示属性 |
| `PairedDevice.sync_settings` | `uc-core::membership::MemberSyncPreferences` | 成员偏好（D3） |
| `PairedDevice.pairing_state::Pending` | 状态机内存中间态 | 不持久化 |
| `PairedDevice.pairing_state::Trusted` | 用 `TrustedPeer` 存在性表达 | 硬删模型 |
| `ShortCode` / short_code 派生算法 | `uc-core::network::pairing`（算法）+ application（视图） | 表示形式 + 协议派生 |
| `staged_paired_device_store` | **不存在** | 职责由 `SpaceAccessContext` + `AdmitMemberUseCase` 取代 |
| `PairingState` 枚举 | **删除** | 替换为 `TrustedPeer` 存在性 |
| `list_sendable_peers` / `resolve_connection_policy` | `membership` / `network` | 可达性 / 连接策略不是信任 |
| `unpair_device` 作为单一动作 | **拆为两个 UseCase**（#4 方案 A） | `DistrustPeerUseCase` + `RevokeMemberUseCase`；UI 的"解除配对"按钮由 Facade 级联调度 |

---

## 8. 持久化

### 8.1 新表 `trusted_peer`

```sql
CREATE TABLE trusted_peer (
    peer_device_id     TEXT PRIMARY KEY NOT NULL,
    local_device_id    TEXT NOT NULL,
    peer_fingerprint   TEXT NOT NULL,
    trusted_at         TEXT NOT NULL  -- ISO 8601
);
CREATE INDEX idx_trusted_peer_local ON trusted_peer(local_device_id);
```

### 8.2 数据迁移（阶段 0.2）

从 `paired_device WHERE pairing_state = 'Trusted'` 导入 `trusted_peer`：
- `peer_device_id ← paired_device.peer_id`
- `local_device_id ← 设备初始化时的本机 id`
- `peer_fingerprint ← paired_device.fingerprint`（为空则标记需一次性协议升级）
- `trusted_at ← paired_device.paired_at`

与 `space_member` 的 migration（MIGRATION_PLAN Phase 1 D12）**可在同一 migration 文件**执行（降低迁移风险），但不是硬约束。

### 8.3 `paired_device` 表的命运

- **阶段 0.4 结束**：`paired_device` 进入只读，不再被写入
- **阶段 0.5**：删除 `uc-core::pairing::PairedDevice` 等 Rust 类型
- **MIGRATION_PLAN Phase 5**：统一 `DROP TABLE paired_device`

---

## 9. 命名决策记录

### 9.1 为什么是 `trusted_peer` 不是 `pairing`

`pairing` 是动作名词（"配对"这个过程），用它命名 aggregate 会混淆动作和事实 —— 这正是上帝对象 `PairedDevice` 的病根（`PairingOrchestrator` 过程 + `PairedDevice` 结果 + `PairingState` 状态生命周期用同一词）。

### 9.2 为什么是 `trusted_peer` 不是 `device_trust`

`device_trust` 的 `trust` 是抽象概念名词，不够实例化。`TrustedPeer` 对应一个具体对象（"一个已信任的对端"），与 `SpaceMember` / `ClipboardEntry` 的命名模式对称。

### 9.3 为什么用 `peer_` 前缀而不是 `device_`

- P2P 语境延续（和现有 `peer_id` 术语一致）
- 强调"相对本机的对端"视角，避免本机属性和对端属性字段混淆
- 值对象类型本身保持中性（`PeerFingerprint` 的 `Peer` 限定了视角，避免过度泛化为 `DeviceFingerprint`）

### 9.4 为什么 `ShortCode` / short_code 派生算法不进 core

- short code 是**表示形式**，不是身份本体
- 具体派生算法（hash 截断 / BIP39 / emoji）是加密实现细节（违反 `uc-core/AGENTS.md` §7.2）
- 媒介选择（数字 / 单词 / 表情 / QR）是表示层关切（违反 §6.3）
- **smoke test**：换成 NFC 验证 core 要不要改？不用改 = 可以放 core。`PeerFingerprint` 满足，`ShortCode` 不满足

### 9.5 为什么是 `trusted_at` 不是 `established_at`

- `trusted_at` 是过去分词事实（类似 `joined_at` / `created_at`），语义轻
- `established_at` 动作色彩重，容易误导为"建立过程的开始时间"

### 9.6 为什么 `Distrust` 和 `Revoke` 是两个 UseCase（问题 #4 方案 A）

- **撤销信任** ≠ **取消成员关系**：前者说"我不再信任这台设备与我通信"，后者说"这台设备不再是本空间成员"
- 理论上一台设备可以是多空间成员，撤销某一空间不等于撤销设备信任
- 当前只有单空间（D1），但 domain 建模按长期语义来，避免回头返工
- UI 的"解除配对"按钮由 `SettingsFacade::unpair_device` 作为**级联编排**调用两个 UseCase

---

## 10. 迁移路径

完整实施路径见 `MEMBERSHIP_MIGRATION_PLAN_ZH.md` §5.4 **阶段 0**（0.0 → 0.5）。

- **阶段 0 的前置**：无（就是最前置）
- **阶段 0 的后置**：完成后才启动阶段 A（`space_access` 搬家到 `uc-application`），因为阶段 A 需要 `TrustedPeerRepositoryPort` 替代 `StagedPairedDeviceStore`

---

## 11. 审查清单

修改 `uc-core::trusted_peer` 或 `uc-application::trusted_peer` 时必须自查：

- [ ] 改动是否引入了传输协议细节（libp2p / TCP / 消息格式）？
- [ ] 改动是否引入了表示形式（short code / QR / UI 枚举）？
- [ ] 改动是否引入了加密算法实现（hash / 派生函数调用）？
- [ ] 改动是否让 `TrustedPeer` 重新变成"大而全"的上帝对象？
- [ ] 改动是否破坏了硬删模型（引入了 `Distrusted` / `Suspended` 中间态）？
- [ ] 如果把 `TrustedPeer` 搬到另一种应用（非剪贴板）场景，改动是否仍合理？
- [ ] Port 是否只暴露业务能力，不暴露底层实现（SQL / HTTP / libp2p）？

任何一条答 "是" = 违反 domain 边界，应调整。

---

## 12. 已采纳决策清单

| # | 决策 | 日期 |
|---|---|---|
| T1 | Domain 命名 `trusted_peer`（非 `pairing` / `device_trust`） | 2026-04-17 |
| T2 | Aggregate 命名 `TrustedPeer` | 2026-04-17 |
| T3 | 字段前缀 `peer_`（不 `device_`） | 2026-04-17 |
| T4 | `PeerFingerprint` 作为唯一身份值对象，进 core | 2026-04-17 |
| T5 | `ShortCode` **不进** core（表示形式 + 加密实现） | 2026-04-17 |
| T6 | 硬删模型（无 Distrusted 中间态，沿用 D4） | 2026-04-17 |
| T7 | `peer_device_id` / `local_device_id` 复用 `DeviceId`（D5 延续） | 2026-04-17 |
| T8 | `DistrustPeerUseCase` 与 `RevokeMemberUseCase` 拆分，Facade 级联调度（#4 方案 A） | 2026-04-17 |
| T9 | 字段名 `trusted_at`（非 `established_at`） | 2026-04-17 |
| T10 | `TrustVerificationChallenge` 作为 application 层视图类型，承载 short_code 给 UI | 2026-04-17 |
| T11 | 协议派生算法（short_code / fingerprint 生成）归 `uc-core::network::pairing` | 2026-04-17 |
