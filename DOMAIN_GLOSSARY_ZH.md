# UC-CORE 领域词汇表（M-B 交付物）

> **状态**：**首版完成（§0–§8 全部填充）**
> **前置**：`DOMAIN_REARCH_ZH.md`（M-A 交付物）
> **后续**：`USECASE_CATALOG_ZH.md`（M-C）、`PORT_CATALOG_ZH.md`（M-D）
> **交付日期**：2026-04-18

---

## §0. 文档性质

### 0.1 本文档是什么

**一份 uc-core 现有领域的词汇整合表**，兼作 `network/` 残留概念的归宿决策清单。

### 0.2 本文档不是什么

- ❌ 不是"从零重新设计 uc-core"
- ❌ 不是 port 目录（port 属于 M-D）
- ❌ 不是 usecase 清单（属于 M-C）
- ❌ 不重新发明已有权威域（`trusted_peer` / `membership` 有独立文档，本文只**引用+摘要**）

### 0.3 本文档的火力集中在

1. 统一口径：哪些域已存在、权威在哪
2. 决策：`network/` 目录里剩下的概念各自归向哪里
3. 修正 `DOMAIN_REARCH_ZH.md` 里与项目实际决策冲突的部分（如"独立 pairing 业务域"提案）
4. 给 M-C 产出 usecase 清单提供稳定的词汇基础

### 0.4 方法论原则（同 M-A §3.1）

**domain → usecase → port** 的顺序。本文档只处理 domain。port 派生是 M-D 的工作，本文档不讨论任何 port 签名。

---

## §1. uc-core 域清单（顶层视图）

### 1.1 域分类

| 域 | 当前位置 | 性质 | 权威来源 | 本文档处置 |
|---|---|---|---|---|
| `trusted_peer` | `uc-core/src/trusted_peer/` | 信任关系事实 | `TRUSTED_PEER_DOMAIN_ZH.md` | §2.1 引用摘要 |
| `membership` | `uc-core/src/membership/` | 空间成员与同步偏好 | `MEMBERSHIP_MIGRATION_PLAN_ZH.md` Phase 0 | §2.2 引用摘要 |
| `space_access` | `uc-core/src/space_access/` | 空间接入流程（含状态机） | 现有代码（标准范式） | §2.3 代码考古摘要 |
| `setup` | `uc-core/src/setup/` | 仅 `SetupStatus`；流程已迁 uc-application | 现有代码 | §2.4 摘要 |
| `pairing` | `uc-core/src/pairing/` + `uc-application/src/pairing/` | **薄 core 域 + 应用层主体**（已决策 B） | 现有代码 | §2.5 摘要 |
| `clipboard` | `uc-core/src/clipboard/` | 剪切板领域 | 现有代码 | §3.1 盘点 |
| `crypto` | `uc-core/src/crypto/` | 加密模型 | 现有代码 | §3.2 盘点 |
| `blob` | `uc-core/src/blob/` | 二进制大对象 | 现有代码 | §3.3 盘点 |
| `file_transfer` | `uc-core/src/file_transfer/` | 文件传输 | 现有代码 | §3.4 盘点 |
| `search` | `uc-core/src/search/` | 全文搜索 | 现有代码 | §3.5 盘点 |
| `settings` | `uc-core/src/settings/` | 用户设置模型 | 现有代码 | §3.6 盘点 |
| `ids` | `uc-core/src/ids/` | 标识符值对象集 | 现有代码 | §3.7 盘点 |
| `config` | `uc-core/src/config/` | 应用配置 | 现有代码 | §3.8 盘点 |
| `app_dirs` | `uc-core/src/app_dirs/` | 应用目录（可疑） | 现有代码 | §3.9 盘点 + 可能越界 |
| **`network`** | `uc-core/src/network/` | **混乱，本次重构目标** | 无 | §4 残留概念归宿决策 |

### 1.2 uc-core 根层逸出物（非域）

`lib.rs:42-62` 有两个"不知道如何分类，临时定义在这里"的类型：

- `EncryptionMeta` — 加密元数据（算法名 / key_id / nonce / aad）
- `MaterializedPayload` — 物化后的 payload（`Inline` / `Blob`）

归宿待定，见 §4.8。

### 1.3 域之间的关系（初步）

```
    ┌─────────────────────────────────────────┐
    │  ids（DeviceId / SessionId / SpaceId 等） │  ← 标识符，几乎所有域依赖
    └─────────────────────────────────────────┘
              ▲              ▲          ▲
              │              │          │
    ┌─────────┴──┐     ┌─────┴────┐  ┌─┴──────────┐
    │trusted_peer│     │membership│  │space_access │
    │（信任关系）│     │（成员+偏好）│  │（接入流程）│
    └─────────▲──┘     └─────▲────┘  └─┬──────────┘
              │              │         │
              │  （流程完成后）│  （流程完成后）
              │              │         │
      由 space_access 流程触发 trusted_peer + membership 的写入
```

- **`space_access`** 是流程（状态机 + action + event）
- **`trusted_peer`** 是流程完成后对"认证关系"的持久化
- **`membership`** 是流程完成后对"业务成员身份"的持久化
- 三者语义互相独立，边界清晰（由 MIGRATION Phase 0 + TRUSTED_PEER Phase 0 厘清）

---

## §2. 已有权威域（引用+摘要）

### 2.1 `trusted_peer` 域

**权威文档**：`TRUSTED_PEER_DOMAIN_ZH.md`

**一句话定义**：**TrustedPeer 是"本机已认可可通信的对端设备"这一事实的持久化表达。**

**核心原则**（P1–P6 详见权威文档）：
- 关系而非动作（P1）
- 身份而非显示（P2）
- 单一职责（P3）
- 硬删模型（P4）
- 传输无关（P5）
- Smoke test：换 UI 或协议版本不改 domain（P6）

**aggregate + 值对象 + 事件**：

```rust
pub struct TrustedPeer {
    pub local_device_id: DeviceId,
    pub peer_device_id: DeviceId,
    pub peer_fingerprint: PeerFingerprint,
    pub trusted_at: DateTime<Utc>,
}

pub struct PeerFingerprint(String);  // 对端公钥的规范指纹

pub enum TrustedPeerEvent {
    PeerVerificationRequired { peer_fingerprint: PeerFingerprint },
    PeerTrusted { trusted_peer: TrustedPeer },
    PeerDistrusted { peer_device_id: DeviceId },
    PeerTrustAborted { reason: TrustAbortReason },
}

pub enum TrustAbortReason { UserCancelled, Timeout, ProtocolError }

pub enum TrustedPeerError {
    AlreadyTrusted(DeviceId),
    NotFound(DeviceId),
    Repository(String),
}
```

**显式排除**（与旧 `PairedDevice` 切割）：`device_name` / `sync_settings` / `state` / `last_seen_at` / `peer_id` 作为网络地址 — 均不进本域。

**本文档后续使用**：本文档视 `trusted_peer` 为**终态 / 冻结**，不再重新设计；如未来有调整，修改权威文档而非本文。

---

### 2.2 `membership` 域

**权威文档**：`MEMBERSHIP_MIGRATION_PLAN_ZH.md`（Phase 0 建模；迁移已完成）

**一句话定义**：**SpaceMember 是"本地对某个远端设备的成员身份 + 同步偏好"的表达。**

**决策要点**（详见权威文档 §1）：
- D1 单空间模型（`SpaceMember` 不带 `SpaceId`）
- D2 本地自治（成员关系不跨设备同步，revoke 是本地动作）
- D3 `MemberSyncPreferences` 语义：本机对某远端成员的**发送/接收**独立偏好
- D4 revoke = 硬删（无 `Revoked` 中间态）
- D5 `DeviceId == peer_id` 字符串直接复用
- D6 `last_seen_at` 不进 membership
- D8 `PairingState` 枚举整体删除（已完成）

**aggregate + 值对象**：

```rust
pub struct SpaceMember {
    pub device_id: DeviceId,
    pub device_name: String,
    pub identity_fingerprint: String,
    pub joined_at: DateTime<Utc>,
    pub sync_preferences: MemberSyncPreferences,
}

pub struct MemberSyncPreferences {
    pub send_enabled: bool,
    pub receive_enabled: bool,
    pub send_content_types: ContentTypes,     // 来自 settings
    pub receive_content_types: ContentTypes,  // 来自 settings
}
impl Default for MemberSyncPreferences { /* 双向全开 + ContentTypes::default() */ }

pub enum MembershipError {
    AlreadyAdmitted(DeviceId),
    NotFound(DeviceId),
    Repository(String),
}
```

**显式放弃的设计**（权威文档 §2.3）：
- ❌ `MemberState` 枚举（硬删模型无用武之地）
- ❌ `IdentityFingerprint` 值对象（暂用 `String`）
- ❌ `DomainEvent` / `MemberEventPort`（本地自治，不广播）
- ❌ `ensure_active_member` use case（调用方不清楚，暂不引入）

**本文档后续使用**：冻结 / 引用，不再设计。

---

### 2.3 `space_access` 域（代码考古摘要）

**权威来源**：`uc-core/src/space_access/` 现有代码（无独立文档，但结构完整）

**本域是项目里最接近 DOMAIN_REARCH 主张"标准领域建模"的范本**，其结构（state / action / event / state_machine / error）应作为后续 `pairing`、`transfer` 等域重建时的模板。

**一句话定义**：**SpaceAccess 是"对端设备向本 space 申请接入 / 本 space 授权对端接入"这一**双向流程**的状态机表达。**

**状态**（`SpaceAccessState`）：

```
Idle
├─ Joiner 分支：
│   WaitingOffer      → WaitingUserPassphrase → WaitingDecision → Granted / Denied
├─ Sponsor 分支：
│   WaitingJoinerProof → Granted / Denied
└─ Cancelled（任意分支可中断）
```

**领域命令**（`SpaceAccessAction`，intent）：
- Sponsor 侧：`RequestOfferPreparation` / `SendOffer`
- Joiner 侧：`RequestSpaceKeyDerivation` / `SendProof`
- 通用：`SendResult` / `PersistJoinerAccess` / `PersistSponsorAccess` / `StartTimer` / `StopTimer`

**领域事件**（`SpaceAccessEvent`，过去式）：
- 启动：`JoinRequested` / `SponsorAuthorizationRequested`
- Offer 阶段：`OfferAccepted`
- 用户输入：`PassphraseSubmitted`
- Proof 阶段：`ProofVerified` / `ProofRejected`
- 裁决：`AccessGranted` / `AccessDenied`
- 控制流：`CancelledByUser` / `Timeout` / `SessionClosed`

**业务级失败原因**（在 core，不是传输错误）：

```rust
pub enum DenyReason { Expired, InvalidProof, SpaceMismatch, SessionMismatch, InternalError }
pub enum CancelReason { UserCancelled, Timeout, SessionClosed }
```

**值对象**：

```rust
pub struct SpaceAccessProofArtifact {
    pub pairing_session_id: SessionId,
    pub space_id: SpaceId,
    pub challenge_nonce: [u8; 32],
    pub proof_bytes: Vec<u8>,
}
```

**辅助**（`reason_codec.rs`）：`DenyReason` ⟷ 字符串 code 的互转（线上协议兼容）

**本域对其他域的期望**：流程完成（`Granted`）时调用 `membership::admit_member` + `trusted_peer::save` 的 usecase（编排由 uc-application 层做，本域只表达意图 `PersistJoinerAccess` / `PersistSponsorAccess`）

**MIGRATION D15 影响**：
> "uc-app/pairing 整体拆除：协议状态机并入 space_access；设备列表归 membership；协议技术层下沉到 uc-core/uc-infra"

D15 说"协议状态机并入 space_access" — 指的是 **uc-app 层 pairing orchestrator** 的状态机并入 **uc-application::space_access** 的 facade，而 uc-core 这一层的 `space_access` 状态机本身已经独立完整，无需扩容。

**本文档后续使用**：冻结 / 引用。§4 在决策 `pairing_state_machine` 归宿时要参考本域。

---

### 2.4 `setup` 域（残留摘要）

**现状**：`uc-core/src/setup/` 只剩 `status.rs`，导出 `SetupStatus`。

**完整 setup 流程**已根据 MIGRATION 阶段 B 全部迁到 `uc-application::setup`（状态机 / 事件 / 动作 / orchestrator / state-machine / action-executor / mark-complete），按 uc-core AGENTS §9.1 "setup 流程编排不在 core" 的规定。

**SetupStatus** 的语义：本机是否已完成首次初始化（持久化的布尔事实），是 `SetupStatusPort`（在 ports 里）的数据契约。

**本文档后续处置**：setup 域已收口，§3 / §4 不做额外处理。

---

### 2.5 `pairing` 域（已决策：薄 core + 应用层主体）

**决策（用户 2026-04-18 确认）**：pairing 是一个独立业务域，但 **uc-core 只保留薄层 domain 语言**，主体业务（状态机 + orchestration + facade + use cases）在 `uc-application::pairing`。

此决策与以下事实一致：

1. pairing 状态机已于 MIGRATION 期间从 `uc-app/pairing` 搬到 `uc-application/pairing`，定位为 "application-layer orchestration state"（见 `uc-application/src/pairing/state_machine.rs` 文件顶部注释）
2. 状态机本身强编排特征（构造线上 `PairingMessage`、调 crypto port、触发持久化），**不是纯 domain FSM**，按 `uc-application/AGENTS.md §5.3` 明确归应用层
3. 对比 `space_access`（纯 domain FSM，留在 core）— pairing 选 B 是基于实际编排性质，不是方法论退让

**2.5.1 `uc-core::pairing` 的薄域内容**

当前：

```rust
// uc-core/src/pairing/role.rs
pub enum PairingRole {
    Initiator,  // 发起方
    Responder,  // 响应方
}
```

**建议后续纳入 `uc-core::pairing`**（等 M-C usecase 清单落定后回填；不是本 M-B 当前动作）：
- 跨层共用的 pairing **纯值对象**（若有明确业务语义而非编排细节）
- pairing **失败原因**分类（若能独立于 `trusted_peer::TrustAbortReason` 存在；目前 `TrustAbortReason` 已覆盖三类 — `UserCancelled` / `Timeout` / `ProtocolError`，按 MIGRATION D24 决策已收口）

**2.5.2 `uc-application::pairing` 的主体模块**

对外暴露（见 `uc-application/src/pairing/mod.rs`）：

| 符号 | 种类 | 备注 |
|---|---|---|
| `PairingFacade` | Application Facade | 唯一对外入口（按 `uc-application/AGENTS §11.4` 强制） |
| `PairingDomainEvent` | 应用层事件 | 变体：`KeyslotReceived` / `PairingVerificationRequired` / `PairingVerifying` / `PairingSucceeded` / `PairingFailed { reason: TrustAbortReason }` |
| `PairingEventPort` | 订阅 trait | `subscribe() -> mpsc::Receiver<PairingDomainEvent>` |
| `PairingCryptoPorts` | Crypto port bundle | 聚合 pairing 所需 crypto 能力 |
| `PairingConfig` | 配置值对象 | |
| `PairingStateMachine` | 应用层状态机 | 1604 行，`(state, event) -> (new_state, actions[])` 的应用层编排 FSM |
| `PairingState` | 状态枚举 | `Idle` / `RequestSent` / `AwaitingUserConfirm` / `ResponseSent` / `AwaitingUserApproval` / `ChallengeSent` / `Finalizing` / `Paired` 等 |
| `PairingAction` | 应用层 intent | |
| `PairingEvent` | 应用层事件 | |
| `CancellationBy` / `TimeoutKind` / `PairingPolicy` | 附属枚举 / 策略 | |

**内部**（`pub(crate)`）：`orchestrator::PairingOrchestrator` / `protocol_handler` / `session_manager::PairingSessionManager` / `usecases::*`。

**辅助值对象**（在 state_machine.rs 内定义，跨层 domain 语义）：

```rust
pub struct PairingHandshakeOutcome {
    pub peer_id: PeerId,
    pub identity_fingerprint: String,
}
```

> 文件注释明示它 "replaces the retired `uc_core::pairing::PairedDevice` domain type (phase 4b PR-5)"—— 即补位 MIGRATION D8 删除 `PairedDevice` 留下的空档。

**2.5.3 与其他域的关系**

- **pairing → trusted_peer**：配对成功（`PairingState::Paired`）时，由应用层持久化 `TrustedPeer`（靠 `TrustedPeerRepositoryPort::save`）
- **pairing → membership**：配对成功后，可能**间接**触发 `AdmitMemberUseCase`（通过 space_access 路径；pairing 本身不直接写 `SpaceMember`）
- **pairing ↔ space_access**：两者都有状态机，但关切不同 — pairing 管"设备对设备的身份互认"，space_access 管"设备加入 space 的授权"；二者可以串联（pairing 建立信任 → space_access 接纳成员），也可以独立演进
- **pairing 依赖 trusted_peer**：复用 `TrustAbortReason` 三类（D24）作为失败原因分类
- **pairing 依赖 `uc-core::pairing::PairingRole`**

**2.5.4 本文档后续处置**

- §2.5 认定 pairing 域当前形态 — **冻结**
- §4 归宿决策不再讨论 `PairingStateMachine` 的归属（已在 uc-application）
- §4 仍需处理 `PairingMessage` 等**线上消息**的归宿（按 §4.1 处理）
- §5.2 "新建独立 pairing core 域"的必要性审视 — **已决策：否**（core 维持薄形态）

---

## §3. 存量域盘点

**盘点原则**：
- 只记录"本域包含什么"（实体 / 值对象 / 策略 / 事件 / 错误）
- 标注可疑越界（技术参数混进业务语义、跨域耦合、位置不合理）
- 不做重新设计
- 编号 §3.1–§3.9 固定，**分批填充**：
  - **批 1（本轮）**：§3.6 `settings` / §3.7 `ids` / §3.8 `config` / §3.9 `app_dirs`
  - 批 2：§3.1 `clipboard`（独立一轮，子模块多）
  - 批 3：§3.2 `crypto` / §3.3 `blob` / §3.4 `file_transfer` / §3.5 `search`

### 3.1 `clipboard` 域

**定位**：UniClipboard 最核心的业务域 — 描述"被观察、被选择、被持久化、被传递"的剪切板内容，以及相关策略、事件、状态机。

**目录**：`uc-core/src/clipboard/` — 17 个文件 + `policy/` 子目录（4 个文件），共 21 个 .rs 文件。

**mod.rs 公共导出清单**：见 `uc-core/src/clipboard/mod.rs:19-38`，`pub use` 约 20 个顶层类型 / 函数。

**3.1.1 实体（Entity）**

| 实体 | 字段 | 语义 |
|---|---|---|
| `ClipboardEntry` | `entry_id: EntryId`, `event_id: EventId`, `created_at_ms: i64`, `active_time_ms: i64`, `title: Option<String>`, `total_size: i64` | 剪切板历史中的**条目**（一个被保留的条目由多个 representation 组成） |
| `ClipboardEvent` | `event_id: EventId`, `captured_at_ms: i64`, `source_device: DeviceId`, `snapshot_hash: SnapshotHash` | 一次**捕获事件**（来自某设备的某次剪切板变化） |

**3.1.2 值对象（Value Objects）**

| 类型 | 语义 |
|---|---|
| `SystemClipboardSnapshot { ts_ms, representations }` | 从系统剪切板瞬间观察到的快照 |
| `ObservedClipboardRepresentation { id, format_id, mime, bytes, cached_hash }` | 快照中一种 representation（含原始字节 + lazy hash 缓存） |
| `PersistedClipboardRepresentation` | 已落库的 representation（inline 或 blob 引用） |
| `ClipboardChange { snapshot, origin: ClipboardChangeOrigin }` | 剪切板变化（含触发源） |
| `ClipboardSelection { primary_rep_id, secondary_rep_ids, preview_rep_id, paste_rep_id, policy_version }` | 从多 representation 中选出的 UI 预览 + 默认粘贴的 representation 组合 |
| `ClipboardSelectionDecision { entry_id, selection }` | 针对某条目的选择决策 |
| `MimeType(pub String)` | MIME 类型（提供 `text_plain` / `text_html` / `uri_list` 等构造器） |
| `ContentHash { alg: HashAlgorithm, bytes: [u8; 32] }` | 内容哈希 |
| `SnapshotHash(pub ContentHash)` | 快照哈希 newtype |
| `RepresentationHash(pub ContentHash)` | representation 哈希 newtype |
| `TimestampMs(i64)` | 毫秒时间戳 |
| `ThumbnailMetadata { representation_id, thumbnail_blob_id, thumbnail_mime_type, original_width, original_height, original_size_bytes, created_at_ms }` | 缩略图元数据 |

**3.1.3 枚举 / 状态**

| 枚举 | 变体 | 语义 |
|---|---|---|
| `ClipboardChangeOrigin` | `LocalCapture / LocalRestore / RemotePush` | **变化**来源（事件级） |
| `ClipboardOrigin` | `Local / Remote` | **条目**来源（条目级，更粗粒度） |
| `HashAlgorithm` | `Blake3V1` | 哈希算法 |
| `PayloadAvailability` | `Inline / BlobReady / Staged / Processing / Failed{last_error} / Lost` | payload 可用性显式 FSM（带 invariant 校验） |
| `ClipboardIntegrationMode` | `Full / Passive` | OS 剪切板集成模式（全观察 / 仅粘贴） |
| `ClipboardContentAction` | `CopyToSystemClipboard / Delete / Pin / Unpin` | 用户可对条目执行的动作 |
| `ClipboardContentActionDecision` | `Allow / Reject{reason}` | 动作决策 |
| `RejectReason` | `NotFound / Expired / Sensitive / PolicyDenied / InternalError` | 拒绝原因 |
| `DuplicationHint` | `New / Repeated` | 去重判定提示 |

**3.1.4 事件（Domain Event）**

| 事件 | 载荷 |
|---|---|
| `ClipboardContentActionEvent::UserRequested` | `content_hash: ContentHash`, `action: ClipboardContentAction` |

**3.1.5 策略（Policy）**

位于 `uc-core/src/clipboard/policy/`：

| 类型 | 语义 |
|---|---|
| `SelectionPolicyVersion::V1` | 策略版本枚举（为未来演进预留） |
| `SelectionTarget` | `UiPreview / DefaultPaste`（同一选择有两个目标视角） |
| `SelectRepresentationPolicyV1` | v1 策略对象：UI Preview 优先 `files > plain > image > rich > uri > unknown`；Default Paste 优先 `files > rich > plain > image > uri > unknown`；stable sort 用 `(score desc, size asc, format_id asc, id asc)` |
| `PolicyError::NoUsableRepresentation` | 策略错误（仅在无可用 rep 时失败） |

`SelectRepresentationPolicyV1` 实现了 `uc-core::ports::SelectRepresentationPolicyPort` — port 在 core、实现也在 core，属于 domain service 模式。

**3.1.6 工具 / 辅助函数**

- `is_file_mime_or_format(mime, format_id) -> bool` — **规范**的文件识别函数（其它 representation wrapper 委托调用它）
- `link_utils::is_single_url(text) -> bool` / `is_all_urls(text) -> bool` — URL 识别（依赖 `url` crate）
- `SystemClipboardSnapshot::snapshot_hash()` — 顺序无关的快照哈希（representation hashes sorted + blake3）
- `SystemClipboardSnapshot::meaningful_origin_key()` / `origin_guard_key()` — 去重 key 生成

**3.1.7 可疑 / 观察点**

| # | 项 | 说明 | 处置建议 |
|---|---|---|---|
| 1 | `ContentHash::From<String>` 使用 `panic!` | 无效输入 panic 而非 Result，违反 uc-core AGENTS §23"unwrap/expect in production" | 改为 `TryFrom`（独立微 issue） |
| 2 | `ClipboardEntry.total_size: i64` | i64 来自 SQLite integer 类型，infra 妥协偷渡到 domain | 可改 `u64` 或 `usize`（独立微 issue） |
| 3 | clipboard 直接用 `blake3` crate | `snapshot.rs` / `system.rs` 直接调 `blake3::hash()`；同样的 hash 能力 `crypto` 域可能也有 | 等批 3 crypto 盘点后再决策是否统一走 `crypto::` 抽象 |
| 4 | `link_utils` 依赖 `url` crate | 纯函数 URL 解析，相对中立但引入第三方依赖 | 接受；登记 |
| 5 | `ClipboardChangeOrigin` vs `ClipboardOrigin` 命名易混 | 一个事件级 / 一个条目级，语义有差异但命名靠得太近 | 考虑重命名（如 `CaptureSource` / `EntryProvenance`）；独立微 issue |
| 6 | `ObservedClipboardRepresentation.bytes: Vec<u8>` 是 `pub` 字段 | 文件注释承认 "Alternative designs if this trade-off changes: clear cache in Clone / make bytes non-public" | 观察，暂不改 |
| 7 | `payload_availability::PayloadAvailability` 是 explicit FSM | 良好的领域建模 | 无问题，作为其它域建模的参考 |
| 8 | `settings::content_type_filter` 依赖 `clipboard::link_utils` + `SystemClipboardSnapshot` | §3.6 已登记，`content_type_filter` 应属于本域 | 独立微 issue 搬家 |
| 9 | 模块 `selection.rs` + `policy/model.rs::ClipboardSelection` 的命名重复 | `ClipboardSelection`（在 policy/model.rs）+ `ClipboardSelectionDecision`（在 selection.rs）共存，命名上有歧义 | 观察；或重命名其中之一 |

**3.1.8 本域与其他域的关系**

- `clipboard` **依赖** `ids`（使用 `EntryId` / `EventId` / `FormatId` / `RepresentationId` / `BlobId`）
- `clipboard` **依赖** `DeviceId`（`ClipboardEvent.source_device`）
- `clipboard` → `blob`（`PersistedClipboardRepresentation.blob_id: Option<BlobId>`，大 payload 流入 blob）
- `settings` **反向依赖** `clipboard`（`content_type_filter` 调 `clipboard::SystemClipboardSnapshot`；§3.6 已登记）
- `membership::MemberSyncPreferences.send_content_types` 使用 `settings::ContentTypes`（间接与 clipboard 类别相关）

**处置**：盘点记录；共 9 条可疑点，其中 5 条对应独立微 issue（附录 §8 回填）。

### 3.2 `crypto` 域

**定位**：加密相关的纯领域模型（密钥、密文容器、KDF 参数、错误、状态机）。不含算法实现。

**目录**：`uc-core/src/crypto/{mod, model, aad, secret, state}.rs`

**3.2.1 密钥值对象**

| 类型 | 语义 |
|---|---|
| `MasterKey(pub [u8; 32])` | DEK（数据加密密钥），加/解 clipboard blob；`Debug` 显示 `[REDACTED]` |
| `Kek(pub [u8; 32])` | KEK（密钥加密密钥），从 passphrase 派生，仅用于 wrap/unwrap `MasterKey` |
| `Passphrase(pub String)` | 用户输入的口令，`Debug` 显示 `[REDACTED]` |
| `SecretString` | 通用敏感字符串：不可 `Clone`/`Serialize`，`Drop` 时 zeroize（依赖 `zeroize` crate） |
| `KeyScope { profile_id: String }` | 密钥作用域（当前仅 profile 维度），`to_identifier() -> "profile:<id>"` |
| `SearchKey([u8; 32])` | 见 §3.5（独立存在于 `search` 域，但性质与 crypto key 一致） |

**3.2.2 持久化容器**

| 类型 | 语义 |
|---|---|
| `KeySlot { version, scope, kdf, salt, wrapped_master_key: Option<WrappedMasterKey> }` | 解锁所需参数 + 已 wrap 的 MasterKey（持久层视角） |
| `KeySlotFile { version, scope, kdf, salt, wrapped_master_key: EncryptedBlob, created_at, updated_at }` | 落盘文件格式（`wrapped_master_key` 必填；带时间戳）；与 `KeySlot` 双向 `TryFrom`/`From` |
| `WrappedMasterKey { blob: EncryptedBlob }` | `MasterKey` 被 `Kek` 加密后的封装 |
| `EncryptedBlob { version, aead, nonce, ciphertext, aad_fingerprint }` | **通用 AEAD 容器**（既 wrap key，也 encrypt clipboard blob）；带 `validate_basic()` 自检 |

**3.2.3 算法 / 参数枚举**

| 枚举 | 变体 | 语义 |
|---|---|---|
| `KeySlotVersion` | `V1` | KeySlot 格式版本 |
| `EncryptionFormatVersion` | `V1` | EncryptedBlob 格式版本 |
| `KdfAlgorithm` | `Argon2id` | KDF 算法 |
| `EncryptionAlgo` | `XChaCha20Poly1305` | AEAD 算法（`Display` 输出 `"xchacha20-poly1305"`） |
| `KdfParams { alg, params }` | 聚合参数 | |
| `KdfParamsV1 { mem_kib, iters, parallelism }` | Argon2id 参数：`Default` = `(128 MB, 3, 4)` |

**3.2.4 状态机**

```rust
pub enum EncryptionState { Uninitialized, Initializing, Initialized }
```

**3.2.5 AAD 工具函数**（`aad.rs`）

统一 AAD 格式 `uc:<type>:v<ver>|<ids>`：
- `for_inline(event_id, rep_id) -> Vec<u8>` — `uc:inline:v1|{event_id}|{rep_id}`
- `for_blob(blob_id) -> Vec<u8>` — `uc:blob:v1|{blob_id}`
- `for_blob_v2(blob_id) -> Vec<u8>` — `uc:blob:v2|{blob_id}`（zstd 压缩格式用）
- `for_network_clipboard(message_id) -> Vec<u8>` — `uc:net_clipboard:v1|{message_id}`
- `for_chunk_transfer(transfer_id: &[u8; 16], chunk_index: u32) -> Vec<u8>` — **二进制** `transfer_id || chunk_index.to_le_bytes()`，用于 chunked 传输 AEAD

**3.2.6 错误**

- `EncryptionError` — **15 个变体**：`NotInitialized` / `Locked` / `WrongPassphrase` / `UnsupportedKeySlotVersion` / `UnsupportedBlobVersion` / `CorruptedKeySlot` / `CorruptedBlob` / `CryptoFailure` / `InvalidKey` / `InvalidParameter(String)` / `KdfFailed` / `UnsupportedKdfAlgorithm` / `EncryptFailed` / `KeyNotFound` / `KeyMaterialCorrupt` / `KeyringError(String)` / `PermissionDenied` / `IoFailure` / `UnsupportedVersion`
- `EncryptionStateError` — 加载/持久化状态错误
- `KeySlotConvertError::MissingWrappedMasterKey`

**3.2.7 可疑 / 观察点**

| # | 项 | 说明 | 处置建议 |
|---|---|---|---|
| 1 | `crypto::model` 直接调 `rand::rngs::OsRng` | `MasterKey::generate` / `KeySlot::draft_v1` 生成随机数 — **严重违反 uc-core AGENTS.md §7.1/7.2**（禁止随机数实现） | 抽象成 `RandomSourcePort`（独立微 issue，纳入 DOMAIN_REARCH Q2 审视） |
| 2 | `KdfParamsV1::default` 硬编码 Argon2 参数 | 这些是"安全策略"还是"技术参数"？`mem_kib: 128MB` / `iters: 3` 是算法选择后果 | 灰色。建议留下但登记"策略参数属于 domain，但默认值的选择接近实现细节" |
| 3 | `EncryptedBlob::validate_basic` 硬编码 nonce 长度 24 | `match (aead, nonce.len()) { (XChaCha20Poly1305, 24) => {} }` — 算法参数紧耦合验证 | 合理（algorithm trait 一致性）；无需改 |
| 4 | `EncryptionAlgo::From<String>` panic | 同 §3.1 clipboard hash 的 panic 问题 | 改 `TryFrom`（独立微 issue） |
| 5 | `MasterKey` / `Kek` 未 zeroize | 文件注释 "TODO: consider adding zeroize"；目前仅 `SecretString` 有 `Drop + zeroize` | 后续补 zeroize（独立微 issue，安全增强） |
| 6 | `MasterKey` 有 `Clone` trait | 注释 "TODO: Remove Clone trait" | 后续移除（独立微 issue） |
| 7 | `EncryptionError` 15 个变体 | 粒度很细；许多变体语义类似（`Locked` / `WrongPassphrase`；`CorruptedKeySlot` / `CorruptedBlob` / `KeyMaterialCorrupt`） | 观察；可能有合并空间，但影响面大，不在 M-B 范围 |
| 8 | AAD 格式跨层共识字符串（`uc:inline:v1|...`） | 这是领域契约，变更会破坏存储/传输兼容 | 合理放 domain；无问题 |
| 9 | `KeyScope` 只有 `profile_id` 一个字段 | 为未来多 scope 预留（space、device），但当前过度抽象 | 观察 |

**3.2.8 本域与其他域的关系**

- `ids` → `crypto`（`KeyScope.profile_id: String` 用裸字符串而非 `ProfileId` newtype — 缺一个 ID）
- `crypto` → `clipboard`（AAD `for_inline` 用 `EventId` / `RepresentationId`；`for_blob` 用 `BlobId`）
- `crypto::KeySlotFile` 被 `pairing::PairingKeyslotOffer`（线上消息）引用 — 即 KeySlot 在配对时会被传输
- `search::SearchKey` 复用 `MasterKey` 同款模式（opaque 32-byte newtype）

**处置**：盘点记录；9 条可疑点（2 条与 DOMAIN_REARCH 原有诊断呼应：Q2 crypto 真身 + AGENTS §7.1 违规）。

---

### 3.3 `blob` 域

**定位**：极简 port 定义，**domain 类型（`Blob`、`BlobStorageLocator`）在 uc-infra**。

**目录**：`uc-core/src/blob/{mod.rs, ports/{mod, reader, writer}.rs}`

**mod.rs 自述**（原文）：
> Holds the read/write port abstractions for blob storage. The blob value object itself (`Blob`, `BlobStorageLocator`) and storage-format details live in `uc-infra` — only the cross-layer contracts are exposed here.

**port**：
```rust
pub trait BlobReaderPort: Send + Sync {
    async fn get(&self, blob_id: &BlobId) -> Result<Vec<u8>>;
}
pub trait BlobWriterPort: ...
```
（签名纯净：`BlobId` 入，`Vec<u8>` 出；domain-semantic only）

**3.3.1 可疑 / 观察点**

| # | 项 | 说明 | 处置建议 |
|---|---|---|---|
| 1 | **域里只有 port，没有 domain 类型** | `Blob` / `BlobStorageLocator` 在 infra — 这是"port 作为跨层契约"的边界情形；如果按"domain 先于 port"原则，`Blob` 值对象本身应该在 core 定义 | 登记：M-D port 派生阶段审视，可能需要把 `Blob` 值对象搬回 core；保留现状到 M-D |
| 2 | `get()` 返回 `Vec<u8>` 原始字节 | 没有封装成 `BlobContent` 之类的值对象 | 配合 #1 一起考虑 |
| 3 | 该域无 error 枚举，直接用 `anyhow::Result` | 违反 uc-core AGENTS 边界错误收敛原则 | 独立微 issue |

**处置**：盘点记录；本域属于"domain 被偷偷下沉到 infra"的情形，M-D 审视。

---

### 3.4 `file_transfer` 域

**定位**：文件传输的**业务事实**（事件 / 进度 / 方向 / 失败/取消原因 + 相关 ports）。**模范建模**之一。

**目录**：`uc-core/src/file_transfer/{mod, event, ports}.rs`

**自述注释**（原文节选）：
> This event model captures business facts only. Transport details such as chunk counters, local file paths, and raw infrastructure errors stay out of this boundary.
> 这些事件只表达业务事实，不表达底层传输实现细节。

**3.4.1 领域事件**

```rust
pub enum FileTransferEvent {
    Started { transfer_id, peer_id, filename, file_size },
    Progress { transfer_id, peer_id, progress: FileTransferProgress },
    Completed { transfer_id, peer_id },
    Failed { transfer_id, peer_id, reason: FileTransferFailureReason, detail: Option<String> },
    Cancelled { transfer_id, peer_id, reason: FileTransferCancellationReason },
}
```

**3.4.2 值对象 / 枚举**

| 类型 | 语义 |
|---|---|
| `FileTransferProgress { direction, bytes_transferred, total_bytes }` | 业务级字节进度，**刻意不含 chunk 级字段** |
| `FileTransferDirection` | `Sending / Receiving` |
| `FileTransferFailureReason` | `NetworkUnavailable / TimedOut / AccessDenied / StorageUnavailable / IntegrityCheckFailed / Unknown` |
| `FileTransferCancellationReason` | `LocalUser / RemotePeer / Replaced / Unknown` |

**3.4.3 Ports**

- `FileTransferEventStorePort::{load(transfer_id), append(event)}`
- `FileTransferEventPublisherPort::publish(event)`
- `FileTransferEventInboundPort::subscribe() -> mpsc::Receiver<FileTransferEvent>`

三个 port 语义：
- `Inbound` — 适配器（libp2p 等）**产生**事件流供应用层消费
- `Publisher` — 应用层**推送**事件给 host（UI / daemon WS 等）
- `Store` — 持久化事件时间线

**3.4.4 可疑 / 观察点**

| # | 项 | 说明 | 处置建议 |
|---|---|---|---|
| 1 | 事件里 `transfer_id: String` / `peer_id: String` 用裸字符串 | 与 `pairing::PairingDomainEvent`、`TrustedPeerEvent` 里的裸 `String` 一致；应统一为 `TransferId` / `DeviceId` newtype | 与 M-D 的 ID 清理联动（批 1 §3.7 已登记 `SessionId` 重复问题，此处同类） |
| 2 | `filename: String` 在 `Started` 事件里裸传 | 文件名是外部数据，可能含 path separator / non-UTF8；无 `FileName` 值对象做约束 | 观察，暂不改 |
| 3 | `detail: Option<String>` 在 `Failed` 里作为"可选自由文本" | domain 故意对格式不透明，这里允许 infra 自由填充 — 合理的"业务 + 上下文"组合 | 模范设计 |
| 4 | 3 个 event ports 语义相近 | 命名 `Inbound` / `Publisher` / `Store` 职责明确，但 port 总数多 | 观察，M-D 审视是否可合并 |

**3.4.5 本域与其他域的关系**

- `file_transfer` 与 clipboard 业务编排一起使用（文件传输与剪切板同步的映射在 `clipboard::FileTransferMapping`）
- `peer_id: String` 裸字符串 — 应是 `DeviceId` 或 `PeerId`（见 §3.7 的 ID 体系）

**处置**：盘点记录；整体是**模范建模**（核心评价：domain 干净地表达业务事实；技术细节显式排除）。

---

### 3.5 `search` 域

**定位**：全文搜索的纯契约定义（文档/查询/结果/索引元信息/密钥/错误）。**实现在 uc-infra Phase 90+，daemon 路由在 uc-daemon Phase 92**。

**目录**：`uc-core/src/search/{mod, document, key, query, result, pipeline_input, error}.rs`

**mod.rs 自述**：
> This module is pure contract definition: no implementations, no database access, no HTTP routes.

**3.5.1 核心数据类型**

| 类型 | 语义 |
|---|---|
| `SearchDocument` | 一条可索引 entry 的元信息（`entry_id, event_id, active_time_ms, captured_at_ms, content_type, file_extensions, mime_type, indexed_at_ms, index_version, text_preview`） |
| `SearchPosting` | 倒排索引一行（`term_tag: Vec<u8>` 是 `HMAC-SHA256(search_key, normalized_token)` 32 字节, `entry_id, field_mask: u8, term_freq: u32`） |
| `SearchIndexMeta` | 索引元信息只读投影（`index_version, blocked, ...`） |
| `SearchResult` | 单条结果 row（`entry_id, content_type, active_time_ms, text_preview, mime_type, file_extensions`） |
| `SearchResultsPage { items, total, has_more }` | 分页结果 |

**3.5.2 查询模型**

| 类型 | 语义 |
|---|---|
| `SearchQuery { query_string, operator, time_range, content_types, extensions, limit, offset }` | 结构化查询（镜像 daemon HTTP 请求 body） |
| `QueryOperator` | `And / Or`（混用为非法） |
| `TimeRangeFilter` | 7 变体：`Today / Yesterday / Last24h / Last7d / Last30d / ThisWeek / ThisMonth / Absolute{from_ms, to_ms}` |
| `ContentType` | `Text / Html / Link / File / Image / Other` |

**3.5.3 状态 / 进度**

| 类型 | 语义 |
|---|---|
| `RebuildStage` | `Started / Indexing / Complete / Failed` |
| `RebuildProgress { stage, indexed, total }` | 重建进度（mpsc 上报） |

**3.5.4 密钥值对象**

`SearchKey([u8; 32])` — opaque HMAC key，从 MasterKey 派生；**禁 Serialize**，`Debug` 显示 `[REDACTED]`。模式对齐 `crypto::MasterKey`。

**3.5.5 错误**

```rust
pub enum SearchError {
    InvalidQuery(String),  // → HTTP 400
    SessionLocked,         // → HTTP 423
    IndexNotReady,         // → HTTP 503
    IndexUnavailable,
    Internal(String),
}
```

**3.5.6 可疑 / 观察点**

| # | 项 | 说明 | 处置建议 |
|---|---|---|---|
| 1 | `SearchError` 注释直接提 HTTP 状态码（400/423/503） | domain 注释**泄漏了表示层细节**（即使不影响代码） | 改写注释避免提及 HTTP；独立微 issue |
| 2 | 三个 `ContentType` 类型并存 | `search::ContentType` / `settings::model::ContentTypes`（位集） / `settings::content_type_filter::ContentTypeCategory`（分类结果） | 非重复，语义不同（类型枚举 / 开关位集 / 分类结果）；但命名混乱；建议跨域命名统一（M-D 阶段）|
| 3 | `SearchDocument.mime_type: String` | clipboard 域有 `MimeType` newtype 为什么不复用？ | 改用 `MimeType`（独立微 issue） |
| 4 | `SearchPosting.term_tag: Vec<u8>` 无 newtype 封装 | 32 字节二进制，可封装为 `TermTag` 值对象 | 观察 |
| 5 | `indexed_at_ms: i64` / `captured_at_ms: i64` / `active_time_ms: i64` 用裸 i64 | clipboard 域有 `TimestampMs` newtype 为什么不复用？ | 改用 `TimestampMs`（独立微 issue） |

**3.5.7 本域与其他域的关系**

- `search` → `ids`（`EntryId`, `EventId`）
- `search` → `clipboard`（逻辑上索引 `ClipboardEntry`；但字段用 `String` 而非 `MimeType`，耦合不完整）
- `search::SearchKey` 与 `crypto::MasterKey` 形成密钥家族

**处置**：盘点记录；3 条 "ID/值对象复用" 类清理（MimeType / TimestampMs / TermTag），1 条注释越界清理。

### 3.6 `settings` 域

**定位**：用户可调的应用设置数据模型 + 若干纯函数辅助。

**目录**：`uc-core/src/settings/{model, defaults, channel, content_type_filter, version}.rs`

**聚合根**：`Settings`（见 `model.rs`）

```rust
pub struct Settings {
    pub schema_version: u32,           // 持久化 schema 版本
    pub general: GeneralSettings,      // 自启动 / 主题 / 语言 / 遥测 / 更新 channel
    pub sync: SyncSettings,            // 自动同步 + 频率 + content_types
    pub retention_policy: RetentionPolicy,
    pub security: SecuritySettings,
    pub pairing: PairingSettings,
    pub keyboard_shortcuts: HashMap<String, ShortcutKey>,
    pub file_sync: FileSyncSettings,
}
pub const CURRENT_SCHEMA_VERSION: u32 = 1;
```

**值对象 / 枚举**（来自 `model.rs`）：

| 类型 | 内容 |
|---|---|
| `GeneralSettings` | `auto_start`, `silent_start`, `auto_check_update`, `theme: Theme`, `theme_color`, `language`, `device_name`, `update_channel: Option<UpdateChannel>`, `telemetry_enabled` |
| `Theme` | `Light / Dark / System` |
| `UpdateChannel` | `Stable / Alpha / Beta / Rc` |
| `ShortcutKey` | `Single(String)` \| `Multiple(Vec<String>)` — `#[serde(untagged)]` 适配前端 `string \| string[]` |
| `ContentTypes` | 位集：`text / image / link / file / code_snippet / rich_text` |
| `SyncSettings` | `auto_sync`, `sync_frequency: SyncFrequency`, `content_types` |
| `SyncFrequency` | `Realtime / Interval` |
| `RetentionPolicy` | `enabled`, `rules: Vec<RetentionRule>`, `skip_pinned`, `evaluation: RuleEvaluation` |
| `RetentionRule` | 6 变体：`ByAge{max_age}` / `ByCount{max_items}` / `ByContentType{content_type, max_age}` / `ByTotalSize{max_bytes}` / `Sensitive{max_age}` |
| `RuleEvaluation` | `AnyMatch / AllMatch` |
| `SecuritySettings` | `encryption_enabled`, `passphrase_configured`, `auto_unlock_enabled` |
| `PairingSettings` | `step_timeout`, `user_verification_timeout`, `session_timeout`, `max_retries`, `protocol_version: String` |
| `FileSyncSettings` | `file_sync_enabled`, `small_file_threshold`, `max_file_size`, `file_cache_quota_per_device`, `file_retention_hours: u32`, `file_auto_cleanup` |

**纯函数**：
- `detect_channel(version: &str) -> UpdateChannel`（`channel.rs`）：从 semver prerelease 标签识别 update channel
- `classify_snapshot(snapshot: &SystemClipboardSnapshot) -> ContentTypeCategory`（`content_type_filter.rs`）：剪切板内容分类

**跨域依赖**：
- `MemberSyncPreferences` 使用 `ContentTypes`（membership 领域复用 settings 的值对象）
- `content_type_filter.rs` 依赖 `clipboard::SystemClipboardSnapshot` / `clipboard::link_utils`（**settings → clipboard 反向依赖**）

**可疑 / 观察点**：

| 项 | 说明 | 建议 |
|---|---|---|
| `content_type_filter.rs` 归宿 | 它是"把剪切板 snapshot 归类成 ContentTypeCategory"的策略，业务语义属于 clipboard 域，只因为 `ContentTypes` 定义在 settings 所以放这里 | **建议搬到 `clipboard/` 域**（或 `clipboard/policy/`）— 独立微 issue，不纳入本次 M-B 处置 |
| `FileSyncSettings::file_retention_hours: u32` | 和 `RetentionRule::ByAge { max_age: Duration }` 的时间单位不一致 | 后续统一为 `Duration`（独立微 issue） |
| `PairingSettings::protocol_version: String` | 协议版本本质是网络协议契约而非用户设置 | 观察，不改动 |
| `ContentTypes` 在 settings 但被 membership + clipboard 都依赖 | 算公共值对象 | 保留现状 |

**处置**：盘点记录，不改动。

---

### 3.7 `ids` 域

**定位**：强类型 newtype wrapper 集合，提供跨域 ID 安全。

**目录**：`uc-core/src/ids/{mod, id_macro, blob_id, clipboard, device_id, peer_id, rep_id, session_id, space_id}.rs`

**值对象清单**（全部是 `Struct<String>` newtype）：

| ID | 用途 | 来源语义 |
|---|---|---|
| `BlobId` | blob 唯一标识 | blob 域内 |
| `DeviceId` | 设备的 6 位稳定 ID | 跨域核心标识（MIGRATION D5：`DeviceId == peer_id` 字符串） |
| `PeerId` | libp2p PeerId 的业务层 wrapper | 网络层标识，**注释声明是 "Business-layer wrapper for libp2p PeerId"** |
| `RepresentationId` | 剪切板 representation ID | clipboard 域 |
| `SessionId` | 配对会话 ID，格式 `{timestamp}-{random}` | pairing + space_access 通用 |
| `SpaceId` | 空间 ID | membership + space_access |
| `EntryId` | 剪切板条目 ID（定义在 `ids/clipboard.rs`） | clipboard |
| `EventId` | 剪切板事件 ID | clipboard |
| `FormatId` | 剪切板格式 ID | clipboard |
| `SnapshotId` | 剪切板快照 ID | clipboard |

**技术辅助**：`id_macro::impl_id!` — 批量生成 `new` / `as_str` / `Display` / `From` 等样板

**可疑 / 观察点**：

| 项 | 说明 | 建议 |
|---|---|---|
| **`SessionId` 重复定义** | `ids::session_id::SessionId` 是类型安全的 newtype；`network::session::SessionId` 是 `pub type SessionId = String;` **类型别名**。lib.rs:37 `pub use ids::SessionId` 导出的是 newtype 版本 | `network::session` 是死代码或冗余，**建议删除**（一个微 issue；纳入 §4.4 标识符统一） |
| **`PeerId` 的正当性** | 按 TRUSTED_PEER P5"传输无关"，peer_id 语义与 libp2p 耦合；但作为"和 libp2p 的接口适配标识"，保留 newtype 能防止和 `DeviceId` 混用 | 保留；注释已经很清楚 |
| **`ids/clipboard.rs` 放在 ids 还是 clipboard** | 4 个 clipboard 相关 ID 集中放在 `ids/clipboard.rs` — 和其他按类型分文件的风格不一致 | 可考虑搬到 `clipboard/ids.rs`，但也有整合价值，保留现状 |

**处置**：盘点记录，`SessionId` 重复问题进 §4.4。

---

### 3.8 `config` 域

**定位**：应用配置的**纯数据 DTO**（没有业务逻辑、没有验证、没有默认值计算）。

**目录**：`uc-core/src/config/mod.rs`（单文件）

**核心类型**：

```rust
pub struct AppConfig {
    pub device_name: String,
    pub vault_key_path: PathBuf,
    pub vault_snapshot_path: PathBuf,
    pub webserver_port: u16,
    pub database_path: PathBuf,
    pub silent_start: bool,
}
```

**构造器**：
- `AppConfig::from_toml(toml_value: &toml::Value) -> anyhow::Result<Self>`
- `AppConfig::empty() -> Self`
- `AppConfig::with_system_defaults(data_dir: PathBuf) -> Self`

**常量**：`RECEIVE_PLAINTEXT_CAP: usize = 128 * 1024 * 1024`（128 MiB，clipboard 传输明文上限）

**铁律**（mod.rs 顶部自述）：
> 只包含数据结构定义；禁止：任何业务逻辑或策略、验证逻辑、默认值计算。
> 空字符串是合法的"事实"，不是错误。

**可疑 / 观察点**：

| 项 | 说明 | 建议 |
|---|---|---|
| `RECEIVE_PLAINTEXT_CAP` 的归宿 | 它是 clipboard 传输策略参数，定义在 config 里 | 可搬到 `clipboard/` 或 `settings::defaults`；独立微 issue |
| `with_system_defaults` 里硬编码路径（`vault/key` / `uniclipboard.db`） | 虽然调用方传入 `data_dir`，但子路径是 core 硬编码的 | 观察，不改动 — 这些路径是跨平台的稳定常量 |
| 与 `settings` 域的区分 | `config` = 启动期读一次的 TOML 配置；`settings` = 用户运行时可改的偏好 | 边界清晰，保留 |

**处置**：盘点记录，无明显越界。

---

### 3.9 `app_dirs` 域

**定位**：应用目录的纯值对象。

**目录**：`uc-core/src/app_dirs/mod.rs`（单文件）

**核心类型**：

```rust
pub struct AppDirs {
    pub app_data_root: PathBuf,
    pub app_cache_root: PathBuf,
}
```

**无函数 / 无常量 / 无 impl**。仅是值对象定义。

**可疑 / 观察点**：

| 项 | 说明 | 建议 |
|---|---|---|
| **归 core 还是 platform** | `uc-core/AGENTS §23` 明确 "Importing ... system APIs in uc-core" 是反模式，但"app data / cache 路径"是典型平台差异概念 | **作为跨层值对象可接受**（它只是两个 `PathBuf`，不含目录推断逻辑；推断在 `uc-platform::app_dirs/`）。**保留现状，但登记为灰色**；后续如果 uc-core 出现类似"纯值对象承载平台概念"的新需求，应统一归宿 |
| 名字 `app_dirs` 的主权感 | 模块名暗示"应用目录"的主权；但实际实现分散在 `uc-platform::app_dirs` 和 `uc-infra::*` | 可重命名 `AppPathLayout` 或 `AppDataRoots` 表达得更谦逊；独立微 issue |

**处置**：盘点记录 + 登记为"灰色但可接受"。

---

## §4. `network/` 残留概念的归宿决策

**本文档核心火力**。当前工作区 `uc-core/src/network/` 实际包含：`connection_policy.rs` / `events.rs` / `mod.rs` / `protocol/` / `session.rs`（经过 MIGRATION Phase 4b，`paired_device.rs` / `pairing_state_machine.rs` / `address_registry.rs` / `daemon_api_strings.rs` / `protocol_ids.rs` 这些历史上出现过的文件已不在该目录）。

**本章方法论**：
- **只给目标归宿**，不给实施路径细节（实施路径是后续 milestone 的工作）
- **区分目标和现状**：某项的目标归宿是 X，不代表今天就能搬到 X（可能有前置依赖）
- **每条决策**必须给出：目标归宿 / 理由 / 前置依赖 / 风险等级

**已关闭的议题**（无需 §4 再处理）：
- ~~`PairingStateMachine` 归属~~ — 已在 `uc-application::pairing`（§2.5 确认）
- ~~`PairedDevice` 归属~~ — 已删除（MIGRATION D8，Phase 4b PR-5 完成）
- ~~`PairingState` 归属~~ — 已下线（MIGRATION Phase 4b PR-5 完成），被 `PeerTrustStatus` 替换
- ~~`address_registry` / `daemon_api_strings` / `protocol_ids` 归属~~ — 这些文件当前不在 `uc-core/src/network/`（见 §4.6 附录核查）

---

### 4.1 线上消息类

**本小节覆盖**：`uc-core/src/network/protocol/` 下 7 个文件里的所有类型 + `network/mod.rs` 导出的 MIME 常量。

**目标归宿**：全部搬到 **`uc-infra::network::wire/`**（infra 私有的线上消息）。

**大原则**：线上协议消息是**传输层编码**的产物，包含序列化逻辑、framing、二进制 layout、版本字节、MIME 常量。按 uc-core AGENTS §23（禁 `libp2p/tauri/system APIs`）和 §6.3（禁"序列化结构"），它们本就不该在 core。按 §2.5.4（跨层流通单位的基本方向），这些类型不得跨层流通。

**4.1.1 逐项归宿决策**

| # | 类型 / 文件 | 当前位置 | 目标归宿 | 理由 | 前置依赖 | 风险 |
|---|---|---|---|---|---|---|
| 1 | `ProtocolMessage` enum<br>（`Pairing/Clipboard/Heartbeat/DeviceAnnounce` 变体 + `to_bytes`/`from_bytes`/`frame_to_bytes` 序列化方法） | `network/protocol/protocol_message.rs` | `uc-infra::network::wire::protocol_message` | 顶层 envelope + serde 逻辑 | ① uc-application::pairing 不再构造 `ProtocolMessage::Pairing(...)`<br>② uc-daemon / uc-app clipboard/file_sync 不再构造 `ProtocolMessage::Clipboard(...)` | 高（搬动面最大） |
| 2 | `PairingMessage` enum + 9 个子类型<br>`PairingRequest` / `PairingChallenge` / `PairingKeyslotOffer` / `PairingChallengeResponse` / `PairingResponse` / `PairingConfirm` / `PairingReject` / `PairingCancel` / `PairingBusy` | `network/protocol/pairing.rs` | `uc-infra::network::wire::pairing` | 纯线上消息 + 自定义 Debug（REDACT pin_hash） | ① `uc-application::pairing::state_machine` 改为发 domain command 而非构造 PairingMessage（重构 state_machine.rs 1604 行中涉及消息构造的部分）<br>② 引入 mapper `PairingAction ⟷ PairingMessage` 在 infra | **最高**（pairing 是关键业务）|
| 3 | `ClipboardMessage` struct<br>（带 `payload_version` / `traceparent` / `file_transfers` 等字段） | `network/protocol/clipboard.rs` | `uc-infra::network::wire::clipboard` | 线上剪切板消息 | ① `uc-app::clipboard::sync_outbound/inbound` 不再构造 `ClipboardMessage`<br>② 引入 mapper `ClipboardSyncIntent ⟷ ClipboardMessage` | 中 |
| 4 | `ClipboardPayloadVersion` enum（`V3`）<br>`FileTransferMapping` struct（`transfer_id` + `filename`） | `network/protocol/clipboard.rs` | `uc-infra::network::wire::clipboard`（随 `ClipboardMessage` 一起） | 线上 payload 版本标识 + 路径重写辅助 | 同 #3 | 中 |
| 5 | `ClipboardBinaryPayload` struct<br>`BinaryRepresentation` struct<br>（UC3 header + chunked AEAD 二进制 layout） | `network/protocol/clipboard_payload_v3.rs` | `uc-infra::network::wire::clipboard_payload_v3` | 纯二进制 wire format | 同 #3 | 中 |
| 6 | `HeartbeatMessage` struct<br>（`device_id` + `timestamp`） | `network/protocol/heartbeat.rs` | `uc-infra::network::wire::heartbeat` | 纯线上消息 | 引入 heartbeat 对应的 mapper / port（当前似乎没 domain 层热身机制，heartbeat 可能仅 infra 内部需要，无需 domain 映射） | 低 |
| 7 | `DeviceAnnounceMessage` struct<br>（`peer_id` + `device_name` + `timestamp`） | `network/protocol/device_announce.rs` | `uc-infra::network::wire::device_announce` | 纯线上消息 | 引入 domain 级 `AnnounceDeviceNameCommand`（port 派生 M-D 决策） | 中 |
| 8 | `FileTransferMessage` enum + `Read/Write` 二进制编解码<br>（6 变体：Announce / Data / Complete / Cancel / Error / Nack 等） | `network/protocol/file_transfer.rs` | `uc-infra::network::wire::file_transfer` | 线上 + 二进制 codec | ① `uc-core::ports::file_transport::FileTransportPort` 签名改为 domain-level `TransferIntent` 而非 `FileTransferMessage`（DOMAIN_REARCH §1.4 已经 flag 这个 port）<br>② 引入 mapper | 高（M-D port 重设计的重要决策点） |
| 9 | MIME 常量（`MIME_IMAGE_PREFIX`, `MIME_TEXT_HTML`, `MIME_TEXT_RTF`, `MIME_TEXT_PLAIN`） | `network/protocol/mod.rs` 顶层 | `uc-infra::network::wire`（和 `ClipboardMessage` 同 crate） | 线上协议用的 MIME 字符串；不同于 clipboard 域的 `MimeType` 值对象（后者是业务语义的 MIME） | 审视是否真的需要在 infra 放；可能和 clipboard 域的 `MimeType::text_plain()` 等构造器重复 | 低 |

**4.1.2 实施层级汇总**

按前置依赖严格程度，搬迁顺序应为：

```
Wave 1 (最简单):  HeartbeatMessage  →  MIME 常量
Wave 2 (中等):    DeviceAnnounceMessage  →  需要 AnnounceDeviceNameCommand (domain)
Wave 3 (clipboard): ClipboardMessage + ClipboardPayloadVersion + FileTransferMapping + ClipboardBinaryPayload + BinaryRepresentation
                    → 需要 ClipboardSyncIntent (domain) + mapper
Wave 4 (file_transfer): FileTransferMessage  →  需要 TransferIntent (domain) + 重设计 FileTransportPort
Wave 5 (pairing):  PairingMessage 全家  →  需要 pairing state_machine 剥离 wire 构造（最大动作）
Wave 6 (topmost):  ProtocolMessage  →  前 5 波都完成后才能搬
```

**4.1.3 本小节结论**

- **所有 9 项的目标归宿统一**：`uc-infra::network::wire/`
- **实施顺序有强依赖**：底层消息先搬，顶层 `ProtocolMessage` 最后搬
- **最大的卡点**：`PairingMessage`（Wave 5）—— 需要先把 `uc-application::pairing::state_machine` 里直接构造 wire message 的代码重构为发 domain-level command，这一部分在 M-C usecase 目录盘点时要详细列出
- **相关 port 重设计**：`FileTransportPort` 签名已在 DOMAIN_REARCH §1.4 flag 为待改，本次确认与 Wave 4 同步执行

---

### 4.2 事件类

**本小节覆盖**：`uc-core/src/network/events.rs` 的全部内容 — 3 个枚举（`NetworkStatus`、`ProtocolDirection`、`ProtocolDenyReason`）、2 个结构体（`DiscoveredPeer`、`ConnectedPeer`）、1 个聚合事件枚举（`NetworkEvent` 含 **17 个变体**）。

**核心诊断**：`NetworkEvent` 是一个**聚合垃圾袋**（grab bag） — 17 个变体横跨 6 种不同业务域，被"都是网络事件"这个技术标签硬捆在一起。这种 single-enum-for-all-events 模式正是 DOMAIN_REARCH §3 根因分析里"port 签名用单一事件枚举绕过领域抽象"的直接症状。

**拆解原则**：**按业务域拆 `NetworkEvent`，不是搬 `NetworkEvent`**。每个变体找到它真正的业务域，重新表达为该域的 domain event。某些变体（带 wire message 的）需要先重构掉 wire 字段。

**4.2.1 `NetworkEvent` 17 个变体的归宿**

| # | 变体 | 载荷 | 真实业务域 | 目标归宿 | 备注 |
|---|---|---|---|---|---|
| 1 | `PeerDiscovered(DiscoveredPeer)` | `DiscoveredPeer` | presence | `uc-core::presence::PresenceEvent::PeerAppeared` 或类似 | 需重建 `DomainPeer`（见 §4.2.2） |
| 2 | `PeerLost(String)` | `peer_id: String` | presence | `uc-core::presence::PresenceEvent::PeerDisappeared(DeviceId)` | 改用 `DeviceId` |
| 3 | `PeerNameUpdated { peer_id, device_name }` | 裸字符串 | presence / membership | `uc-core::presence::PresenceEvent::PeerRenamed{ device_id, new_name }` | `device_name` 本质属于 `SpaceMember`，但此事件是 presence 层面的通告 |
| 4 | `PeerConnected(ConnectedPeer)` | `ConnectedPeer` | 传输层 | **infra 私有** | 业务层只关心 "能否通信"，具体的 libp2p 连接建立事件不跨层 |
| 5 | `PeerDisconnected(String)` | `peer_id: String` | 传输层 | **infra 私有** | 同上；真正需要被应用层感知的，是"某设备从可达变为不可达"（映射到 `PeerDisappeared`） |
| 6 | `PeerReady { peer_id }` | 裸字符串 | 传输就绪 | infra 私有 或 `uc-core::presence::PresenceEvent::PeerReadyForBroadcast(DeviceId)` | 审视：应用层是否真的需要此事件？若需要，建议和 Pairing-Complete 融合（配对 + 就绪 = 真正可用） |
| 7 | `PeerNotReady { peer_id }` | 裸字符串 | 传输就绪 | 同上 | |
| 8 | `PairingMessageReceived { peer_id, message: PairingMessage }` | **含 wire message** | pairing | **删除**（不对外暴露） | 裸 wire message 不应出现在跨层事件；由 infra mapper 转成 `PairingAction::HandleIncoming(...)` 驱动 state_machine |
| 9 | `PairingRequestReceived { session_id, peer_id, request: PairingRequest }` | **含 wire message** | pairing | 同上：**删除**（不对外暴露） | pairing 内部通过 state_machine 接收；事件不对外 |
| 10 | `PairingPinReady { session_id, pin, peer_device_name, peer_device_id }` | 业务字段 | pairing | `uc-application::pairing::PairingDomainEvent::PairingVerificationRequired`（已存在，结构相近） | **已有对应**；可能需要字段调整（pin 是否该传给应用层？应该由应用层从 fingerprint 派生） |
| 11 | `PairingResponseReceived { session_id, peer_id, response: PairingResponse }` | **含 wire message** | pairing | **删除**（不对外暴露） | 同 #8 |
| 12 | `PairingComplete { session_id, peer_id, peer_device_id, peer_device_name }` | 业务字段 | pairing | `uc-application::pairing::PairingDomainEvent::PairingSucceeded`（已存在，结构相近） | **已有对应**；改用 `DeviceId` |
| 13 | `PairingFailed { session_id, peer_id, error: String }` | 业务字段 + 错误字符串 | pairing | `uc-application::pairing::PairingDomainEvent::PairingFailed { reason: TrustAbortReason }`（已存在） | **已有对应**；错误字符串改用 `TrustAbortReason` 三类枚举（已按 MIGRATION D24 收口） |
| 14 | `ClipboardReceived(ClipboardMessage)` | **含 wire message** | clipboard sync | **删除**（不对外暴露） | infra 收到 `ClipboardMessage` 后 mapper 转成 domain `IncomingClipboardContent`，经由 clipboard sync port 上送应用层；domain 侧事件为 `uc-core::clipboard::sync_event::ReceivedRemoteClipboardEntry` 或类似 |
| 15 | `ClipboardSent { id, peer_count }` | 业务字段 | clipboard sync | `uc-core::clipboard::sync_event::LocalClipboardDispatched { entry_id, peer_count }` 或类似 | M-C usecase 盘点时确认是否真有消费者 |
| 16 | `StatusChanged(NetworkStatus)` | `NetworkStatus` | connectivity | `uc-core::connectivity::ConnectivityEvent::StatusChanged(ConnectivityStatus)` 或并入 presence | 见 §4.2.3 + §5.4 |
| 17 | `ProtocolDenied { peer_id, protocol_id, trust, direction, reason }` | 含多类字段 | 策略评估结果 | **infra 内部日志 + 按需上抛业务事件** | 参见 §4.2.4 |
| 18 | `Error(String)` | 裸字符串 | 兜底错误 | **infra 私有**（带 `#[allow(dead_code)]` 表明无消费者） | 直接删除（见 §4.2.5） |

**4.2.2 `DiscoveredPeer` / `ConnectedPeer` 的重建**

当前结构混合了"业务身份"和"传输细节"：

| 字段 | 类型 | 性质 | 新归宿 |
|---|---|---|---|
| `peer_id: String` | libp2p PeerId | 传输身份 | 仅 infra；domain 用 `DeviceId` |
| `device_id: Option<String>` | 6-位 ID | **业务身份** | domain（必填） |
| `device_name: Option<String>` / `String` | 显示名 | **业务** | domain |
| `addresses: Vec<String>` | multiaddr 字符串 | 传输细节 | **infra 私有** |
| `discovered_at` / `last_seen` / `connected_at` | 时间戳 | 业务（presence 时序） | domain（可能复用 `TimestampMs`） |
| `is_paired: bool` | 业务判定 | **业务** | domain（但需要思考是放 presence 还是从 membership 派生） |

**目标重建**（暂拟；最终定型在 §5.1 presence 域审视）：

```rust
// uc-core::presence::peer_presence (拟案)
pub struct DomainPeer {
    pub device_id: DeviceId,
    pub device_name: Option<String>,
    pub first_seen_at: TimestampMs,
    pub last_seen_at: TimestampMs,
    pub is_member: bool,       // 从 membership 查询合成
    pub connection_phase: ConnectionPhase,  // Visible / Reachable / Connected（按业务分层）
}
```

`DiscoveredPeer` / `ConnectedPeer` **都删除**（合并为 `DomainPeer`）；infra 内部可保留 libp2p 层的对应结构（如 `InfraPeerRecord { peer_id, addresses, ... }`）。

**4.2.3 `NetworkStatus` 的归宿**

当前：`NetworkStatus { Disconnected, Connecting, Connected, Error(String) }`

- `Connecting` / `Error(String)` 是传输过程态，属于 infra
- `Connected` / `Disconnected` 是业务视角的"是否有任意可达对端"

**决策**：
- `NetworkStatus` enum 本体**删除**
- 派生 domain 级 `ConnectivityStatus { Online, Offline }`（放 `uc-core::connectivity` 域，若保留该域；否则并入 `presence`）— 最终形态由 §5.4 决策
- 传输过程细节（`Connecting` / `Error`）**infra 内部用**，不跨层

**4.2.4 `ProtocolDirection` / `ProtocolDenyReason` / `ProtocolDenied` 的归宿**

| 类型 | 当前 | 归宿 |
|---|---|---|
| `ProtocolDirection { Inbound, Outbound }` | 事件字段 | **infra 私有**（传输方向） |
| `ProtocolDenyReason { NotTrusted, Blocked, RepoError, NotSupported }` | 事件字段 | **拆分**：`NotTrusted` → domain 语义（属于 `connection_policy` 评估结果）；`Blocked` / `RepoError` / `NotSupported` → infra 私有错误分类 |
| `NetworkEvent::ProtocolDenied` | 事件变体 | **infra 私有 tracing 日志**；不跨层。若 UI 真的需要显示"对端被拒绝连接"，改用基于 `PeerTrustStatus` 查询派生的 UI 状态（见 §4.3） |

**4.2.5 `NetworkEvent::Error(String)` 的归宿**

**⚠️ 勘误（M-B 自检发现）**：`#[allow(dead_code)]` 仅表示 **uc-core 内部**无消费者（uc-app 不 match 此变体），但 **uc-platform libp2p adapter 实际在构造并广播**：

- `uc-platform/src/adapters/libp2p_network/swarm_event_loop.rs:1016`
- `uc-platform/src/adapters/libp2p_network/swarm_event_loop.rs:1038`

两处均是 libp2p 网络连接失败时的兜底错误上报。因此本变体**不是死代码**。

**修正归宿**：属于"传输层兜底错误广播"，随 `NetworkEvent` 整体拆解时归 infra 私有（或重构为更具体的领域事件如 `PresenceEvent::TransportError` / `ConnectivityEvent::Degraded`）。不直接删除。

**4.2.6 实施依赖与批次**

本小节结论与 §4.1 Wave 互相依赖：

- 变体 #8/9/11 删除 → 配合 **§4.1 Wave 5 Pairing**（PairingMessage 搬走时，这些包装它的事件自然消失）
- 变体 #14 删除 → 配合 **§4.1 Wave 3 Clipboard**（ClipboardMessage 搬走时）
- 变体 #10/12/13（Pin/Complete/Failed）→ 映射到已有 `PairingDomainEvent`，仅需字段调整（改用 DeviceId、去掉 pin 字段等）
- 变体 #1/2/3/6/7 → 依赖 **§5.1 `presence` 域是否新建**
- 变体 #16 → 依赖 **§5.4 `connectivity` 域是否新建**

**4.2.7 对后续章节的输入**

- **§5.1 presence 域审视**：本节将 NetworkEvent 的 5 个 presence 性变体 + `DiscoveredPeer` / `ConnectedPeer` 都归到了 presence 域。**结论：presence 域有必要新建**。形态建议：`uc-core::presence::{peer_presence::DomainPeer, presence_event::PresenceEvent, ports}`
- **§5.4 connectivity 域审视**：`ConnectivityStatus { Online, Offline }` 是否独立为 `connectivity` 域？**初步倾向：并入 `presence`**（避免过度细分；"网络整体是否在线"本质是 "presence 是否为空" 的派生）
- **§4.3 策略类**：`ProtocolDenied.trust: PeerTrustStatus` 字段说明"信任评估结果"在 network 事件里渗出；§4.3 讨论 `PeerTrustStatus` 时要处理

**4.2.8 本小节结论**

- **`NetworkEvent` 整体拆解**：17 变体按业务域拆到 `presence` / `pairing`（已有 `PairingDomainEvent`）/ `clipboard sync`；6 个变体直接删除（带 wire message 或 dead code）
- **`DiscoveredPeer` / `ConnectedPeer` 合并为 `DomainPeer`**；原结构 infra 私有（可重命名 `InfraPeerRecord` 明示）
- **`NetworkStatus` 拆分**：业务部分成 `ConnectivityStatus { Online, Offline }`，过程态留 infra
- **`ProtocolDirection` / `ProtocolDenyReason.{Blocked,RepoError,NotSupported}`** → infra 私有
- **`ProtocolDenyReason::NotTrusted`** → 归 `connection_policy` 评估结果
- **新建 `presence` 域有必要**，`connectivity` 初步建议并入 `presence`

---

### 4.3 策略类

**本小节覆盖**：`uc-core/src/network/connection_policy.rs` 定义的 5 个类型 + 1 个 port（`ConnectionPolicyResolverPort` 在 `uc-core/src/ports/connection_policy.rs`）。

**4.3.1 类型清单**

当前（MIGRATION Phase 4b PR-5 后）：

```rust
pub enum PeerTrustStatus { Trusted, Untrusted }
// Trusted   = 对端已登记为本 space 的成员 → 允许 pairing + business
// Untrusted = 对端尚未登记或已被撤销 → 仅允许 pairing（用于再次建立信任）

pub enum ProtocolKind { Pairing, Business }

pub struct AllowedProtocols { pairing: bool, business: bool }
impl AllowedProtocols { pub fn allows(&self, kind: ProtocolKind) -> bool { ... } }

pub struct ConnectionPolicy;
impl ConnectionPolicy {
    pub fn allowed_protocols(status: PeerTrustStatus) -> AllowedProtocols { ... }
}

pub struct ResolvedConnectionPolicy {
    pub trust: PeerTrustStatus,
    pub allowed: AllowedProtocols,
}
```

伴随 port（`uc-core/src/ports/connection_policy.rs`）：

```rust
pub trait ConnectionPolicyResolverPort {
    async fn resolve(&self, ...) -> Result<ResolvedConnectionPolicy, _>;
}
```

**4.3.2 性质**

- 输入：`PeerTrustStatus`（业务事实，由 `MemberRepositoryPort` 命中/未命中合成）
- 输出：`AllowedProtocols`（业务授权）
- **纯领域策略**：无传输依赖，无 I/O，无外部库
- **是 port 契约的组成部分**：`ConnectionPolicyResolverPort::resolve` 的返回类型是 `ResolvedConnectionPolicy`

**前期结论**（DOMAIN_REARCH M-A 调查）：这组类型**必须留在 uc-core**（见 DOMAIN_REARCH §3.1 findings F2 "connection_policy 必须留 core 的连锁原因"）。

**4.3.3 命名问题**

类型名全部带着"Connection" / "Protocol" 等传输层色彩词，掩盖了真实的业务语义 — **"给定信任状态，对端能调用哪些能力家族"**：

| 当前名 | 真实语义 | 建议新名 |
|---|---|---|
| `ConnectionPolicy` | 能力授权策略 | `CapabilityPolicy` |
| `ProtocolKind::{Pairing, Business}` | 能力类别 | `CapabilityKind::{Pairing, Business}` |
| `AllowedProtocols` | 授权能力集合 | `AllowedCapabilities` |
| `ResolvedConnectionPolicy` | 授权策略解析结果 | `ResolvedCapability` 或 `CapabilityResolution` |
| `PeerTrustStatus` | （保留）对端信任状态 | `PeerTrustStatus`（名字已经准确） |

**DOMAIN_REARCH §5.1 原案**是放到 `uc-core::pairing/capability_policy.rs`；本文修正该提案 — 因 §2.5 已决定 pairing 为薄 core 域（不承载业务主体），capability 策略不应挂在 pairing 下。

**4.3.4 目标归宿**

**决策**：搬到**新独立小域 `uc-core::capability/`**。

理由：
- 不属于 `trusted_peer`（TRUSTED_PEER P3 "单一职责"，只管信任关系**事实**，不含策略）
- 不属于 `membership`（按 MIGRATION D15 是"设备成员身份 + 同步偏好"，无策略位）
- 不属于 `pairing`（§2.5 薄核心，且这组策略不限于 pairing 场景）
- 独立小域最符合 "基于信任状态做业务能力授权" 的单一职责

目标文件布局：

```
uc-core/src/capability/
  mod.rs
  trust_status.rs      PeerTrustStatus
  capability_kind.rs   CapabilityKind (原 ProtocolKind)
  allowed.rs           AllowedCapabilities (原 AllowedProtocols)
  policy.rs            CapabilityPolicy (原 ConnectionPolicy) + ResolvedCapability
```

Port 归宿同步：

```
uc-core/src/ports/capability/
  resolver.rs          CapabilityResolverPort (原 ConnectionPolicyResolverPort)
```

（port 具体签名与命名最终定型留 M-D。）

**4.3.5 实施依赖**

| 项 | 依赖 / 风险 |
|---|---|
| 类型迁移 | 只有位置变更 + 可选重命名，无业务逻辑变化 |
| 调用点更新 | DOMAIN_REARCH §3.1 findings F4 已盘点 `ConnectionPolicyResolverPort` 引用分布 — uc-app / uc-application / uc-platform / uc-daemon / uc-bootstrap 都有 |
| 命名修正 | 可**分阶段**：先搬位置保留旧名 → 独立 PR 改名；或一次性完成 |
| 风险等级 | 低（纯值对象 + 纯函数）；调用面中等 |
| 与其他 §4 决策的关系 | §4.2 的 `ProtocolDenied.trust: PeerTrustStatus` 字段说明 `PeerTrustStatus` 已被 network 事件消费；事件拆解后此引用消失 |

**4.3.6 本小节结论**

- **必须留 uc-core**（port 契约的组成部分）
- **搬到新独立小域 `uc-core::capability/`**（修正 DOMAIN_REARCH §5.1 "放 pairing/" 的原案）
- **重命名**：`ConnectionPolicy` → `CapabilityPolicy` / `ProtocolKind` → `CapabilityKind` / `AllowedProtocols` → `AllowedCapabilities` / `ResolvedConnectionPolicy` → `ResolvedCapability`
- **`PeerTrustStatus` 保留原名**（已经准确）
- **修正记入 §7 差异登记**

---

### 4.4 标识符

**本小节覆盖**：`uc-core/src/network/session.rs`。

**现状**：

```rust
// uc-core/src/network/session.rs
pub type SessionId = String;  // 类型别名
```

与：

```rust
// uc-core/src/ids/session_id.rs
pub struct SessionId(String);  // newtype
impl SessionId { pub fn new, as_str, into_inner ... }
```

`uc-core/src/lib.rs:37` 顶层 re-export 的是 `ids::SessionId`（newtype 版本）。

**问题**：两个同名类型共存，其中 `network::session::SessionId` 是 `String` 别名版本。`network/mod.rs:23` 里有 `pub use session::SessionId;` — 这个 re-export 在下游如何被消费需要 grep 验证，但正常使用中下游会通过 lib.rs 顶层 re-export 拿到 newtype 版本（§3.7 盘点已 flag）。

**4.4.1 归宿决策**

| 项 | 决策 |
|---|---|
| `uc-core/src/network/session.rs` | **整个文件删除**（死代码 / 冗余） |
| `uc-core/src/network/mod.rs` 中的 `pub mod session; pub use session::SessionId;` | 同步移除 |
| `ids::SessionId`（newtype） | 保留，作为唯一 `SessionId` |

**4.4.2 实施依赖**

**⚠️ 勘误（M-B 自检发现）**：`network::SessionId` **不是纯死代码**。grep 确认 `uc-application/src/pairing/{protocol_handler,session_manager}.rs` 实际使用，且作为 `HashMap<SessionId, _>` 的 key。当前 `SessionId = String` 别名，改为 `ids::SessionId` newtype 会触发：

- `HashMap<SessionId, _>::get(&str)` 不再成立（newtype 不能 `&str` lookup）
- `entry(session_id.to_string())` 构造需改
- 约 10+ 处代码同步修改

**修正定性**：这是一个**小型类型 refactor**（而非简单删除），变动量**中**而非最低。

**实施依赖**：
- `uc-application::pairing::session_manager` 中 `record_session_peer` / `get_session_peer` / `get_session_role` / `has_active_session` / `remove_session` 等方法签名从 `session_id: &str` 改为 `session_id: &SessionId`
- HashMap key 构造点改为 `SessionId::new(...)` 或 `SessionId::from(...)`
- 考虑使用 `ids::SessionId` 作为统一 SessionId（删掉 `network::SessionId` 后 uc-core 顶层 re-export 仍指向 `ids::SessionId`，调用方 import 改为 `use uc_core::SessionId` 或 `use uc_core::ids::SessionId`）

**风险等级**：中（非纯死代码删除，但不涉及业务语义变化）

**4.4.3 结论**

- `network::session.rs` 删除
- 唯一 `SessionId` = `uc-core::ids::SessionId` newtype
- 本决策在 §4 里优先执行（无前置依赖）

---

### 4.5 lib.rs 逸出物

**本小节覆盖**：`uc-core/src/lib.rs:42-62` 两个"不知道如何分类，临时定义在这里"的类型。

**当前代码**：

```rust
// lib.rs:42-62（有注释 "不知道如何分类，临时定义在这里"）

pub struct EncryptionMeta {
    pub algo: String,        // "xchacha20poly1305"
    pub key_id: String,      // keyslot id / key version
    pub nonce_b64: String,
    pub aad_b64: Option<String>,
}

#[derive(Debug, Clone)]
pub enum MaterializedPayload {
    Inline { mime: Option<String>, bytes: Vec<u8> },
    Blob { mime: Option<String>, blob_id: BlobId },
}
```

**4.5.1 消费验证**

`rg "EncryptionMeta|MaterializedPayload"` 结果：**仅 `uc-core/src/lib.rs` 一个文件命中**。

即：**在整个 workspace 内没有其他代码消费这两个类型**。它们是**死代码**（未完成的设计 / 历史遗留）。

**4.5.2 归宿决策**

两个类型各自的可能性：

**`EncryptionMeta`**：
- 字段（`algo` / `key_id` / `nonce_b64` / `aad_b64` 全是 `String`）与 `crypto::EncryptedBlob` 的字段（`aead: EncryptionAlgo` / `nonce: Vec<u8>` / `aad_fingerprint: Option<Vec<u8>>`）有重叠，**很可能是 `EncryptedBlob` 设计定型前的早期草稿**
- 没有 `key_id` 语义的承载者（`crypto::EncryptedBlob` 没有这个字段）；也许是面向持久化 DTO 的"落盘描述"
- **决策**：**删除**（若未来需要元信息落盘 DTO，可在 infra 层重新引入）

**`MaterializedPayload`**：
- 语义清晰：clipboard payload 的两种物化形态（inline 或 blob 引用）
- 但 `clipboard::PersistedClipboardRepresentation` 已经用 `inline_data: Option<Vec<u8>>` + `blob_id: Option<BlobId>` + `payload_state: PayloadAvailability` 表达了同样的信息
- **决策**：**删除**（`PersistedClipboardRepresentation` 已充分覆盖）

**4.5.3 实施依赖**

- 前置：workspace 全量搜索确认无消费（已确认 — 仅 `lib.rs` 本身）
- 风险等级：**最低**（死代码删除）

**4.5.4 结论**

- `EncryptionMeta` — **删除**
- `MaterializedPayload` — **删除**
- lib.rs 的"不知道如何分类"注释随之消失
- `ids` re-export（`BlobId` / `DeviceId` / `PeerId` / `SessionId`）保留

---

### 4.6 附录：原先以为在 network/ 但实际在其他位置的概念

**本小节覆盖**：此前 M-A 和早期工作区中间态里提到过、但当前**不在 `uc-core/src/network/`** 的几项概念的真实位置核查。

核查方式：`rg "pub (struct|enum|mod) (AddressRegistry|ProtocolId|DaemonApiString)"` + 文件名搜索。

**4.6.1 核查结果**

| 名字 | 实际位置 | 性质 | 归宿评价 |
|---|---|---|---|
| **`AddressRegistry` / `AddressRecord` / `AddressScope` / `AddressSource`** | `uc-platform/src/adapters/libp2p_network/address_registry.rs` | libp2p multiaddr 元数据管理（技术） | **位置合理**：是 libp2p adapter 的内部实现 — 按 §6.3 uc-platform AGENTS "libp2p adapter 归 infra" 的精神，未来 platform→infra 整体搬迁时一起走（本节不改变其位置） |
| **`ProtocolId`**（libp2p 协议 ID 字符串常量） | `uc-platform/src/adapters/protocol_ids.rs` | libp2p 协议 ID 字符串 | **位置合理**：libp2p 专属常量；同 AddressRegistry 一起随 platform→infra 迁移 |
| **`net_utils::get_physical_lan_ip`** | `uc-platform/src/net_utils.rs` | LAN IP 检测（仅 libp2p 监听用） | **位置合理**：同上 |
| **`daemon_api_strings`** | **不存在** | — | 当前整个 workspace 无 `daemon_api_strings` 文件或模块；此前 system-reminder 中显示的 `pub mod daemon_api_strings;` 属于已回滚的中间态。`uc-daemon/src/api/` 下各 route handler（`setup.rs` / `pairing.rs` / `clipboard.rs` 等）就是 daemon API 的真身，不需要一个单独的"strings"模块 |

**4.6.2 位置性质总结**

- `AddressRegistry` / `ProtocolId` / `net_utils` 三项本已在 `uc-platform`，不需要"从 network 搬出去"的动作
- 按 DOMAIN_REARCH §5.3 "消息类型的最终归宿"图示，未来 platform → infra 的整体搬迁（含 libp2p adapter）会把这三项也带走，归宿为 `uc-infra::network::libp2p/`
- 该搬迁不属于本 M-B 范围（domain 词汇表），由后续 M-H 实施阶段执行
- `daemon_api_strings` 是幽灵 — 不存在，不处理

**4.6.3 结论**

本小节无新归宿决策，仅做**核查备忘**。`uc-core::network/` 清理后不会牵涉这三项（它们早已不在 uc-core）。

---

## §4 整体小结

**4 批决策合并**：

| 小节 | 对象 | 统一目标 |
|---|---|---|
| §4.1 | 线上消息（9 大类） | 全部搬到 `uc-infra::network::wire/`（分 6 个 Wave） |
| §4.2 | 事件类（17 变体 + 5 个值对象） | 按业务域拆解：`pairing` 已有 / 新 `presence` / infra 私有 / 删除 |
| §4.3 | 策略类（5 类型 + 1 port） | 留 core，搬到**新独立小域** `uc-core::capability/`，重命名 |
| §4.4 | `network::session::SessionId` 别名 | 删除 |
| §4.5 | lib.rs 两个逸出物 | 删除（死代码） |
| §4.6 | network/ 外的关联概念 | 核查备忘 — 无新动作 |

**`uc-core/src/network/` 清理后的最终状态**：**空目录，整个 `network` 模块从 uc-core 下线**。

**本 §4 激发的新决策**：
- 新建 `uc-core::capability/` 小域（§4.3）
- 建议新建 `uc-core::presence/` 域（§4.2 指向，§5.1 正式决策）
- 建议 `connectivity` 并入 `presence`（§5.4 初步决策）

---

## §5. 新建域的必要性审视

本章在 §3（存量盘点）和 §4（network/ 归宿决策）的基础上，对所有候选新建域做**正式决策**。

**5 项决策速览**：

| 域 | 决策 | 依据 |
|---|---|---|
| §5.1 `presence` | **新建** ✓ | §4.2 已盘出 5 类 presence 事件 + `DomainPeer` 无处安放 |
| §5.2 独立 `pairing` core 域 | **关闭** — uc-core 维持薄形态 | §2.5 用户决策 B |
| §5.3 `transfer` 通用域 | **不建** | 语义差异大于共性（本小节论证） |
| §5.4 `connectivity` | **不建** — 并入 `presence` | §4.2 分析 |
| §5.5 `capability` | **新建** ✓ | §4.3 策略类归宿 |

---

### 5.1 `presence` 域（新建 ✓）

**来源**：§4.2 把 `NetworkEvent` 的 5 个 presence 性变体 + `DiscoveredPeer` / `ConnectedPeer` 归到了一个新域。

**一句话定义**：**`presence` 是"对端设备当前是否在场 / 可达 / 可通信"的事实记录。**

**域边界**：

| 属于 presence | 不属于 presence |
|---|---|
| "某设备此刻是否被本机可见" | "某设备是否可信"（= `trusted_peer`） |
| "某设备此刻是否可达" | "某设备是否同步成员"（= `membership`） |
| "对端显示名当前是什么" | libp2p 连接的具体状态（= infra 私有） |
| 在场/离场事件 | 线上消息（= infra 的 wire） |
| 整体连通性 `Online`/`Offline` 派生状态 | 配对流程（= `uc-application::pairing`） |

**核心内容**（草案；具体字段定型留 M-C）：

```rust
// uc-core/src/presence/peer_presence.rs
pub struct DomainPeer {
    pub device_id: DeviceId,
    pub device_name: Option<String>,
    pub first_seen_at: TimestampMs,
    pub last_seen_at: TimestampMs,
    pub is_member: bool,
    pub connection_phase: ConnectionPhase,
}

pub enum ConnectionPhase {
    Visible,   // 本机可见对端（mDNS 发现层面）
    Reachable, // 本机可和对端建立通信通道
    Connected, // 已建立活跃通信会话
}

// uc-core/src/presence/presence_event.rs
pub enum PresenceEvent {
    PeerAppeared(DomainPeer),
    PeerDisappeared(DeviceId),
    PeerRenamed { device_id: DeviceId, new_name: String },
    PeerReachabilityChanged { device_id: DeviceId, phase: ConnectionPhase },
}

// 整体连通性（§5.4 并入）
pub enum ConnectivityStatus { Online, Offline }
```

**设计原则**：
- P1: **身份用 `DeviceId`**，不用 `peer_id`（后者留 infra）
- P2: **不含地址/多址**（multiaddr 是 libp2p 细节，留 infra）
- P3: **关系到 membership / trusted_peer 的字段（`is_member`）是派生只读视图**，不是 presence 域自己定义的事实；由 infra mapper 或应用层在合成 `DomainPeer` 时填充
- P4: presence 不广播到对端（本地自治；与 membership D2 原则一致）

**目录**：

```
uc-core/src/presence/
  mod.rs
  peer_presence.rs      DomainPeer / ConnectionPhase
  presence_event.rs     PresenceEvent
  connectivity_status.rs ConnectivityStatus（§5.4 并入）
  error.rs              PresenceError（若需要）
```

**Port 暂略**（M-D 派生）。

---

### 5.2 独立 `pairing` core 域 ~~关闭~~

已在 §2.5 决策 — uc-core 维持薄形态（仅 `PairingRole`），主体在 `uc-application::pairing`。

不再讨论。

---

### 5.3 `transfer` 通用域（不建）

**背景**：DOMAIN_REARCH §5.1 原案提议建 `uc-core/transfer/` 作为 "clipboard + file 的共同抽象"。本节审视其必要性。

**现状**：
- `uc-core::clipboard/` — 剪切板领域（21 个文件，§3.1 盘点）
- `uc-core::file_transfer/` — 文件传输领域（3 个文件，§3.4 盘点，**模范建模**）

**两者相似点**：
- 都是"设备 A → 设备 B"的内容传递
- 都有方向（sending / receiving）
- 都有失败/取消语义

**两者关键差异**：

| 维度 | clipboard | file_transfer |
|---|---|---|
| 触发模型 | 流式同步（任何剪切板变化都可能触发） | 按需启动（用户显式触发） |
| 数据结构 | 多 representation + MIME + selection | 不透明字节流 |
| 用户感知进度 | 不需要（消息小、秒级完成） | 需要（文件可大） |
| 去重机制 | 基于 hash（`ContentHash` / `SnapshotHash`） | 不需要 |
| 多路选择 | 有（UI 预览 / 默认粘贴两视角） | 无 |
| 典型大小 | KB 级 | MB–GB 级 |

**决策**：**不新建 `transfer` 域**。

**理由**：

1. **共性太浅**：两者只在"传输方向 + 失败/取消"上有相似，抽象后会被各自特有的"流式 vs 按需 / 多 representation vs 单 stream"大量特化方法淹没
2. **抽象成本高于收益**：domain 模型应该用最清晰的语言表达业务，不应为了代码复用而模糊业务语义
3. **file_transfer 已是模范**（§3.4 评价）：拿它和 clipboard 硬合并反而破坏既有的清晰建模
4. **现状不阻塞**：两个域保持独立，不会产生循环依赖或难以维护的点

**保留的微调空间**（独立微 issue 级别，不在 M-B 范围）：
- 两个域里 `peer_id: String` 都是裸字符串 → 统一改用 `DeviceId`（§3.4 + §4.2 已登记）
- 两个域都有"传输方向"概念，可抽 `TransferDirection` 共享值对象 — 但成本收益比不高，观察即可

**修正**：DOMAIN_REARCH §5.1 的 `transfer/` 子目录提案 → **撤销**，记入 §7。

---

### 5.4 `connectivity` 域（不建 — 并入 `presence`）

**背景**：§4.2 `NetworkStatus` 的业务部分（`Online` / `Offline`）需要归位。DOMAIN_REARCH §5.1 原案列为可选 `connectivity/`。

**决策**：**不独立建**，`ConnectivityStatus` 并入 `presence/`（见 §5.1 目录结构 `connectivity_status.rs`）。

**理由**：

1. 语义派生关系：`ConnectivityStatus::Online` **就是** "至少有一个 peer 处于 `Visible` 以上的 presence" — 完全可以作为 `presence` 的聚合视图
2. 避免过度细分：为一个 2-变体 enum 建独立域，边界收益低
3. 消费者场景：UI 层的"是否在线"指示器、应用层的"是否可开始同步"判断 — 两者都属于 presence 消费者

**修正**：DOMAIN_REARCH §5.1 的 `connectivity/` 子目录提案 → **撤销**（并入 presence），记入 §7。

---

### 5.5 `capability` 域（新建 ✓）

§4.3 已决策。本节仅登记确认。

**一句话定义**：**`capability` 是"基于对端信任状态，本机允许对端调用哪些业务能力家族"的策略**。

**目录**：

```
uc-core/src/capability/
  mod.rs
  trust_status.rs      PeerTrustStatus（原在 network/connection_policy.rs）
  capability_kind.rs   CapabilityKind (原 ProtocolKind) { Pairing, Business }
  allowed.rs           AllowedCapabilities (原 AllowedProtocols)
  policy.rs            CapabilityPolicy (原 ConnectionPolicy) + ResolvedCapability
```

Port 归宿同步：

```
uc-core/src/ports/capability/
  resolver.rs          CapabilityResolverPort (原 ConnectionPolicyResolverPort)
```

---

## §5 整体结论

**最终 uc-core 新增域清单（按业务视角组织）**：

```
uc-core/src/
  clipboard/              （存量，§3.1）
  crypto/                 （存量，§3.2）
  blob/                   （存量，§3.3）
  file_transfer/          （存量，§3.4）
  search/                 （存量，§3.5）
  settings/               （存量，§3.6）
  ids/                    （存量，§3.7）
  config/                 （存量，§3.8）
  app_dirs/               （存量，§3.9）
  membership/             （存量，§2.2 权威）
  trusted_peer/           （存量，§2.1 权威）
  space_access/           （存量，§2.3 权威范式）
  setup/                  （存量薄，§2.4）
  pairing/                （薄核心，§2.5）
  presence/         ← 新建（§5.1）
  capability/       ← 新建（§5.5）
  ports/                  （随业务域子组，M-D 派生）

  # network/ 整个目录删除（§4）
```

**§5 修正了 DOMAIN_REARCH M-A 的 3 条提案**（同步记入 §7）：
1. `uc-core/pairing/` 独立业务域 → 薄核心（§2.5）
2. `uc-core/transfer/` 通用传输域 → 不建（§5.3）
3. `uc-core/connectivity/` 域 → 并入 presence（§5.4）

**§5 新增确认了 2 个新域**：
1. `uc-core/presence/`（§5.1）
2. `uc-core/capability/`（§5.5）

---

## §6. 跨域关系图 & 命名字典

### 6.1 uc-core 域关系图

按"业务流"视角展示域间依赖（M-B 目标形态，含新建的 `presence` / `capability`，去掉 `network/`）：

```
                         ┌────────────┐
                         │    ids     │  ← 跨域身份/标识（所有域依赖）
                         └─────▲──────┘
                               │
        ┌──────────────┬───────┴───────┬──────────────┐
        │              │               │              │
┌───────┴────┐  ┌──────┴────┐  ┌───────┴────┐ ┌──────┴─────┐
│  pairing   │  │trusted_peer│  │ membership │ │space_access│
│  (薄核心)  │  │ (信任事实) │  │ (成员+偏好)│ │ (接入流程) │
└──────▲─────┘  └─────▲──────┘  └─────▲──────┘ └─────┬──────┘
       │              │               │              │
       │              │  配对成功触发 │   流程结束触发
       │              └───────────────┴──────────────┘
       │                          │
       │   (复用 TrustAbortReason)│
       └──────────────────────────┘

┌────────────┐   ┌────────────┐   ┌────────────┐
│  capability│   │  presence  │   │connectivity│ = presence::ConnectivityStatus（并入）
│  (授权策略)│   │ (在场性)   │   │    (派生)  │
└─────┬──────┘   └─────┬──────┘   └────────────┘
      │                │
      │ PeerTrustStatus │ 查询 membership → is_member
      │ 由 membership 合成│
      └─────────────────┘

┌────────────┐   ┌────────────┐   ┌────────────┐
│  clipboard │   │file_transfer│  │   search   │
│ (剪切板业务)│   │ (文件传输) │   │ (全文搜索) │
└─────┬──────┘   └─────┬──────┘   └─────┬──────┘
      │                │                │
      └───┬────────────┴────────────────┘
          │
          ▼
       ┌────────────┐
       │    blob    │（二进制大对象存储 port）
       │    crypto  │（加密模型 + AAD）
       └────────────┘

┌────────────┐   ┌────────────┐   ┌────────────┐
│  settings  │   │   config   │   │  app_dirs  │（基础支撑）
│  (用户设置)│   │ (启动配置) │   │ (路径值对象)│
└─────┬──────┘   └────────────┘   └────────────┘
      │
      └─ ContentTypes → membership::MemberSyncPreferences
      └─ content_type_filter 将搬到 clipboard（§3.6 微 issue）

┌────────────┐
│   setup    │（仅 SetupStatus；流程在 uc-application）
└────────────┘
```

**关键关系**：

| 关系 | 方向 | 说明 |
|---|---|---|
| `capability` ← `membership` | 只读消费 | `PeerTrustStatus` 由 `MemberRepositoryPort` 命中/未命中合成 |
| `presence::DomainPeer.is_member` ← `membership` | 只读消费 | 派生字段 |
| `pairing` 失败原因 ← `trusted_peer::TrustAbortReason` | 类型复用 | MIGRATION D24 已收口 |
| `space_access::Granted` → `membership::admit_member` + `trusted_peer::save` | 流程触发 | 由 uc-application 层编排 |
| `clipboard` / `file_transfer` / `search` ← `crypto` + `blob` | 能力依赖 | 加密 + 存储 |
| `settings::ContentTypes` → `membership` + `clipboard` | 值对象共享 | 跨域数据契约 |

### 6.2 命名字典

**本节汇总本次 M-B 产生的所有命名决策**，作为 M-C / M-D 的统一参考。

**6.2.1 `network/` 重命名（去"network"化）**

| 旧（当前代码） | 新（目标） | 归宿 |
|---|---|---|
| `ConnectionPolicy` | `CapabilityPolicy` | `uc-core::capability::policy` |
| `ProtocolKind` | `CapabilityKind` | `uc-core::capability::capability_kind` |
| `AllowedProtocols` | `AllowedCapabilities` | `uc-core::capability::allowed` |
| `ResolvedConnectionPolicy` | `ResolvedCapability` | `uc-core::capability::policy` |
| `PeerTrustStatus` | **保留**（已准确） | `uc-core::capability::trust_status` |
| `ConnectionPolicyResolverPort` | `CapabilityResolverPort` | `uc-core::ports::capability::resolver` |
| `DiscoveredPeer` + `ConnectedPeer`（合并） | `DomainPeer` | `uc-core::presence::peer_presence` |
| `NetworkStatus`（业务部分） | `ConnectivityStatus` | `uc-core::presence::connectivity_status` |
| `NetworkEvent`（整体） | 拆解 — 按业务域（见 §4.2.1） | 不存在单一继承者 |

**6.2.2 已有准确命名（不动）**

| 名字 | 所在域 |
|---|---|
| `TrustedPeer` / `PeerFingerprint` / `TrustAbortReason` | trusted_peer |
| `SpaceMember` / `MemberSyncPreferences` | membership |
| `PairingRole` | pairing |
| `SpaceAccessState` / `SpaceAccessAction` / `SpaceAccessEvent` / `DenyReason` / `CancelReason` | space_access |
| `PairingFacade` / `PairingDomainEvent` / `PairingEventPort` / `PairingStateMachine` / `PairingState` / `PairingAction` / `PairingEvent` / `PairingHandshakeOutcome` / `CancellationBy` / `TimeoutKind` / `PairingPolicy` | `uc-application::pairing` |
| `ClipboardEntry` / `ClipboardEvent` / `PayloadAvailability` / `ContentHash` / `MimeType` / `TimestampMs` | clipboard |
| `FileTransferEvent` / `FileTransferProgress` / `FileTransferDirection` / `FileTransferFailureReason` / `FileTransferCancellationReason` | file_transfer |
| `MasterKey` / `Kek` / `KeySlot` / `KeySlotFile` / `EncryptedBlob` / `EncryptionAlgo` / `KdfAlgorithm` | crypto |
| `DeviceId` / `PeerId` / `SessionId` / `SpaceId` / `BlobId` / `EntryId` / `EventId` / `FormatId` / `RepresentationId` / `SnapshotId` | ids |

**6.2.3 可疑命名（独立微 issue 跟踪）**

| 名字 | 问题 | 建议 |
|---|---|---|
| `ClipboardChangeOrigin` vs `ClipboardOrigin` | 两个相似概念命名过近 | 重命名其一（如 `CaptureSource` / `EntryProvenance`）— 微 issue |
| `ClipboardSelection` vs `ClipboardSelectionDecision` | 重叠 | 保留 `ClipboardSelection`；`ClipboardSelectionDecision` 改名为 `EntrySelection` 之类 — 微 issue |
| `app_dirs` | 模块名主权感过强 | 考虑重命名 `AppPathLayout` / `AppDataRoots` — 微 issue |
| `PairingHandshakeOutcome.peer_id: PeerId` | peer_id 裸名（虽然已是 newtype） | 语义准确，保留 |

**6.2.4 裸 `String` → newtype 化清单（M-D 联动）**

| 当前 | 目标 | 出处 |
|---|---|---|
| `FileTransferEvent::{Started,Progress,Completed,Failed,Cancelled}.peer_id: String` | `DeviceId` | §3.4 |
| `FileTransferEvent::*.transfer_id: String` | 新 `TransferId` newtype | §3.4 |
| `PairingDomainEvent::*.peer_id: String` / `session_id: String` | `DeviceId` / `SessionId` | §2.5 |
| `SearchDocument.mime_type: String` | `MimeType` | §3.5 |
| `SearchDocument.*_ms: i64` | `TimestampMs` | §3.5 |
| `SpaceMember.identity_fingerprint: String` | `PeerFingerprint`（或新值对象） | §2.2 |
| `KeyScope.profile_id: String` | 新 `ProfileId` newtype | §3.2 |

---

## §7. 本文档与 `DOMAIN_REARCH_ZH.md` 的差异登记

M-A 文档里与本文结论冲突的部分，集中登记并用于 M-A 修订。

### 7.1 M-A §5.1 "uc-core/pairing/ 独立业务域" → 修订为 "薄 core 域"

见 §2.5 决策（用户 B）。

### 7.2 M-A §5.1 "connection_policy 改名 capability_policy" 的位置修正

M-A 原案：把改名后的文件放在 `uc-core/pairing/capability_policy.rs`。
本文修订：放在独立 `uc-core::capability/` 小域（因 §2.5 pairing 为薄核心，不承载策略）。

### 7.3 M-A §5.1 `uc-core/connectivity/` 域 → 撤销

§5.4 决策：不建独立 connectivity 域，`ConnectivityStatus` 并入 `presence/`。

### 7.4 M-A §5.2 `uc-core/presence/` 域 → 确认新建

§5.1 决策：新建。与 M-A 方向一致，内容扩充（`DomainPeer` / `ConnectionPhase` / `PresenceEvent` / 并入 `ConnectivityStatus`）。

### 7.5 M-A §5.1 `uc-core/transfer/` 通用域 → 撤销

§5.3 决策：不建。`clipboard` 和 `file_transfer` 保持独立，共性太浅不足以抽象。

### 7.6 M-A §10 附录 B "当前工作区状态"

M-A 附录 B 记录的工作区污染（uc-infra 拷贝文件、uc-core 删除文件）在本 M-B 开始时已回滚干净（`git status` 只有 M-A / M-B 两份 untracked 文档）。M-A 附录 B 信息已过时，可忽略。

### 7.7 新增 `uc-core::capability/` 小域

M-A 未显式列出。§4.3 + §5.5 决策：新建 `uc-core::capability/`，承载 `CapabilityPolicy` / `CapabilityKind` / `AllowedCapabilities` / `ResolvedCapability` / `PeerTrustStatus`。

### 7.8 `uc-core/src/network/` 整个目录删除

M-A 虽然暗示了方向（§5.5 "uc-core 从此完全看不到线上消息"），但未明说 `network/` 目录本身的命运。本文 §4 整体结论：**该目录最终完全清空并删除**。

### 7.9 `NetworkEvent` 聚合 enum 的处置

M-A §8 Q7 列为开放问题。本文 §4.2.1 给出明确拆解方案：17 变体按业务域拆到 `presence` / 已有 `PairingDomainEvent` / infra 私有 / 删除，**不保留任何聚合顶层事件类型**。Q7 关闭。

### 7.10 `DiscoveredPeer` / `ConnectedPeer` 合并为 `DomainPeer`

M-A §5.1 暗示的重建，本文 §4.2.2 + §5.1 正式确认。

### 7.11 `PairingState` / `PairedDevice` 已下线事实确认

M-A 附录 B 把这两项列在"工作区中间态"。实际通过 MIGRATION Phase 4b PR-5 已彻底清理；本文 §4 前置备忘将它们从"待处理" 移到 "已关闭"。

### 7.12 `network::session::SessionId` 的处置

M-A 未特别处理。本文 §4.4 决策：删除（死代码）。

### 7.13 lib.rs 两个逸出物

M-A §8 Q... 未涉及。本文 §4.5 决策：`EncryptionMeta` / `MaterializedPayload` 都删除（workspace 零消费）。

---

## §8. 开放问题（移交 M-C / M-D）

### 8.1 M-A 原 Q1–Q10 的状态更新

| 原编号 | 主题 | 当前状态 |
|---|---|---|
| Q1 | uc-app vs uc-application 并存的意义 | **仍开放**。本 M-B 未触及；建议移交 M-C（usecase 归属时自然回答） |
| Q2 | crypto 真身 | **部分关闭**。§3.2 确认 `OsRng` 违规 + 9 条可疑点；"crypto 整体拆分"仍开放，移交微 issue |
| Q3 | clipboard 真身 | **部分关闭**。§3.1 盘点完成；域本身保留，内部 9 条微 issue |
| Q4 | connectivity 是否保留 | **关闭**。§5.4 并入 presence |
| Q5 | 命名字典 | **关闭**。§6.2 给出 |
| Q6 | port 粒度 | **仍开放**。§3.1.1 方法论原则要求延迟到 M-D |
| Q7 | NetworkEvent 聚合 enum | **关闭**。§4.2.1 拆解方案 |
| Q8 | peer_id: String 泄漏 | **方案确定**。§6.2.4 newtype 化清单。实施移交 M-D + 各实施 milestone |
| Q9 | uc-platform 清理后剩什么 | **部分关闭**。§4.6 确认 libp2p 相关随 platform→infra 一起搬；platform 剩余职责的定性移交后续 |
| Q10 | 架构测试 / fitness function | **仍开放**。移交 M-H 清理与守恒 milestone |

### 8.2 M-B 新开问题（需要 M-C / M-D 回答）

| 编号 | 问题 | 去向 |
|---|---|---|
| N1 | pairing state_machine 从 wire 构造剥离的重构路径 | M-C usecase 盘点 + 后续实施 |
| N2 | `DomainPeer.connection_phase` 三级枚举是否 overkill | M-C |
| N3 | `presence` 是否需要自己的 port（subscribe / query） | M-D |
| N4 | `capability` port 最终签名（是 `resolve(DeviceId)` 还是 `resolve_for_peer(DeviceId)`） | M-D |
| N5 | 是否需要一个跨域的 `RandomSourcePort`（回答 Q2 违规） | M-D |
| N6 | `Blob` 值对象是否搬回 core（§3.3 blob 域只有 port） | M-D |
| N7 | `uc-app::clipboard::sync_outbound/inbound` 如何换掉 `ClipboardMessage` 构造 | M-C |
| N8 | `FileTransportPort` 重设计（参数从 `FileTransferMessage` 变 `TransferIntent`） | M-D |
| N9 | heartbeat 是否真的需要 domain 级抽象（或仅 infra 内部） | M-C |
| N10 | `DeviceAnnounce` 是否需要 domain 级 `AnnounceDeviceNameCommand` | M-C |

### 8.3 M-B 发现的独立微 issue 清单（总 18 条）

**panic 清理（Result 化）**
1. `ContentHash::From<String>` panic → `TryFrom`
2. `EncryptionAlgo::From<String>` panic → `TryFrom`

**域归宿搬迁（不涉及领域重建）**
3. `settings::content_type_filter` 搬到 `clipboard/` 域
4. `RECEIVE_PLAINTEXT_CAP` 从 `config` 搬到 `clipboard/` 或 `settings::defaults`

**命名清理**
5. `ClipboardChangeOrigin` vs `ClipboardOrigin` 重命名
6. `ClipboardSelection` vs `ClipboardSelectionDecision` 重命名
7. `app_dirs` 模块名改谦逊

**死代码清理**（本 M-B 直接决策删除）
8. ~~`network::session::SessionId` 删除~~ — **勘误**：非纯死代码（`uc-application::pairing` 有消费者），改为小型类型 refactor（§4.4 已勘误）
9. `EncryptionMeta` 删除（§4.5，**warm-up 阶段已执行 2026-04-18**） ✅
10. `MaterializedPayload` 删除（§4.5，**warm-up 阶段已执行 2026-04-18**） ✅
11. ~~`NetworkEvent::Error(String)` 删除~~ — **勘误**：非死代码（libp2p adapter 有实际广播），应随 NetworkEvent 整体拆解处理（§4.2.5 已勘误）

**类型/字段修正**
12. `ClipboardEntry.total_size: i64` → 考虑 `u64` 或 `usize`
13. `FileSyncSettings::file_retention_hours: u32` → `Duration`
14. `SearchDocument.mime_type: String` → `MimeType`
15. `SearchDocument.{indexed_at_ms, captured_at_ms, active_time_ms}: i64` → `TimestampMs`
16. `SearchError` 注释移除 HTTP 状态码

**安全增强**
17. `MasterKey` / `Kek` 补 `zeroize`（含去 `Clone`）
18. `crypto::model` 用 `rand::OsRng` → 抽 `RandomSourcePort`（与 Q2 / AGENTS §7.1 呼应）

### 8.4 本节处置建议

- 上述 18 条微 issue 建议在 M-C 阶段末尾汇总为一份 `MICRO_ISSUES_ZH.md` 工作清单，由开发者按优先级分批处理（或作为 backlog）
- 10 个新开问题（§8.2 N1–N10）按"M-C / M-D"归属分拣，分别进入后续阶段的 agenda
- 5 个 M-A 原开放问题仍保留：Q1 / Q2（部分）/ Q6 / Q9（部分）/ Q10

---

## §7. 本文档与 `DOMAIN_REARCH_ZH.md` 的差异登记

M-A 文档里与本文结论冲突的部分 — 需在本章集中登记并修正 M-A：

### 7.1 M-A §5.1 "uc-core/pairing/ 独立业务域" → 修订为 "薄 core 域"

M-A 原文提案在 uc-core 新建完整的 pairing 业务域（含 state machine / Action / Event / capability_policy 等），与 `trusted_peer`、`space_access` 并列。

**本文档 §2.5 修订**：uc-core 维持薄形态（仅 `PairingRole` 等值对象），pairing 主体在 `uc-application::pairing`。理由：
- pairing 状态机已是 application-layer orchestration，不是纯 domain FSM
- MIGRATION 迁移结果是搬到 uc-application 而非 uc-core
- 用户 2026-04-18 决策选解读 B

**M-A 需要的修正**：§5.1 目录结构提案里的 `pairing/` 块应改为"薄域（PairingRole；后续若有跨层纯值对象可扩充）"，删除 `state_machine.rs` / `action.rs` / `event.rs` / `capability_policy.rs` / `failure.rs` 子文件提案。

### 7.2 M-A §5.1 提案的 `capability_policy.rs`（原 `connection_policy` 改名）

M-A §5.1 把 `connection_policy` 改名 `capability_policy` 并放入 `uc-core/pairing/`，说"它的真实语义是基于信任状态允许哪些业务能力"。

**现实已超越这个提案**：MIGRATION Phase 4b PR-5 已直接重构了 `connection_policy.rs` 本体，把 `PairingState` 依赖替换为 `PeerTrustStatus`，保留了原文件名。重命名一事项因此可以**不再执行**（或作为微调并入 §4.3 归宿决策）。

### 7.3 M-A §5.1 提案 `uc-core/connectivity/` 域 → 待 §5.4

### 7.4 M-A §5.1 / §5.2 提案 `uc-core/presence/` 域 → 待 §5.1

### 7.5 M-A §5.1 提案 `uc-core/transfer/` 域 → 待 §5.3

### 7.6 M-A §10 附录 B "当前工作区状态"

M-A 附录 B 记录的工作区污染（uc-infra 拷贝文件、uc-core 删除文件）在本 M-B 开始时已回滚干净（`git status` 只有 `DOMAIN_REARCH_ZH.md` 和本文档作为 untracked）。M-A 附录 B 信息已过时，可忽略。

---

## §8. 开放问题（移交 M-C / M-D，待填充）

---

**当前进度**：§0 / §1 / §2 完成（首轮）
**下一步**：用户审阅本轮内容 → 继续 §3 存量域盘点 → §4 `network/` 归宿决策 → §5 / §6 / §7 / §8
