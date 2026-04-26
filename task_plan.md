# Task Plan: 并行引入 iroh-native domain + infra,废弃 libp2p

> **策略**:平行路径。libp2p 相关代码**完全冻结**(不改、不迁移),core 中新开独立 domain,infra 中新增 iroh adapter,双栈并存验证后一次性删除 libp2p。

## 🎯 目标(Goal Statement)

在不触碰现有 libp2p 任何一行代码的前提下,构建一套**从零遵循六边形架构**的网络 domain:
- `uc-core`:纯领域(无 wire、无 PeerId 字符串泄漏、无 iroh 类型泄漏)
- `uc-infra`:iroh 实现(iroh-net + iroh-blobs,流式原生)
- 最终替换策略:**用户重新配对**,无静默迁移
- 最终清理:验证通过后一次性删除整个 libp2p adapter + core 旧 network 模块

## 🧭 用户已锁定的决策

| # | 决策 | 影响 |
|---|---|---|
| D1 | libp2p 代码冻结,不做任何迁移/变更 | 新旧隔离,便于回滚 |
| D2 | 传输语义从"帧"改为"流"(iroh bi-directional stream 原生) | port 形状变:open/read/write/close 而非 send_frame |
| D3 | 文件传输改用 iroh-blobs(废弃手写 chunked 协议) | core 需要 `BlobDigest`/`BlobTicket` 概念 |
| D4 | 用户重新配对,**无静默迁移** | 可以随便改 peer 身份格式、会话协议,不受历史兼容约束 |

## 🏛 Domain 划分决策(方案 C)

**已锁定**:两层结构
1. **底层纯 ports**(无领域色彩,与 `ClockPort`/`HashPort` 同级)→ 放在 `uc-core/src/ports/`
2. **中层 `trust` 子域** → 新建 `uc-core/src/trust/`,承载"跨业务的节点关系语义"
3. **上层业务子域**(`pairing` / `clipboard` / `file_transfer` / `space`)各自扩展,不互相依赖

### 底层 ports 清单(候选)

| Port | 职责 | 映射到 iroh |
|---|---|---|
| `EndpointPort` | 节点运行时生命周期(start/stop) | `iroh::Endpoint` |
| `DiscoveryPort` | 订阅"可见对端"事件流 | iroh mDNS + DNS discovery |
| `SessionOpenerPort` | 按 capability 开启双向字节流 | ALPN + `Endpoint::open_bi` |
| `BlobTransferPort` | 基于 digest 的 blob 发送/拉取 | `iroh-blobs` |
| `PresencePort` | 查询某节点当前可达性 | adapter 内部状态 |

### `trust` 子域内容(候选)

| 领域对象 | 职责 |
|---|---|
| `TrustedPeer`(聚合根) | "我信任的一个对端"——身份 + 业务元数据(DeviceName、DeviceId) |
| `PeerIdentity`(值对象) | 稳定身份(Ed25519 公钥的业务包装) |
| `TrustPolicy` | 原 `ConnectionPolicy` 的演化——**业务语义**替代 ProtocolKind 字符串 |
| `Capability` | 业务能力枚举(Pairing/ClipboardSync/FileTransfer) |
| 领域事件 | `TrustEstablished` / `TrustRevoked` / `PeerReachabilityChanged` |

### 🔴 需要先裁决的边界问题

`trust::TrustedPeer` 与既有 `space::SpaceMember` 是否会重叠?两个候选路径:

- **C.1(独立)** 新建 `trust/`,`TrustedPeer` 从连接视角描述对端;`SpaceMember` 从加密空间视角描述成员;两者通过 ID 互引
- **C.2(并入)** 不新建 `trust/`,把 `TrustPolicy` / `Capability` / `PeerReachabilityChanged` 合进 `space/`,由 `SpaceMember` 承载
- **C.3(命名替换)** 新建子域但叫 `peerage`(更贴近"对端群体"业务语义,少和安全策略词 `trust` 撞)

## ✅ 已敲定决策(2026-04-18)

| # | 结论 |
|---|---|
| **Q1** | **C.2** — 并入 `space/`,`TrustPolicy` / `Capability` / `PeerReachabilityChanged` 合入 `space/`;`SpaceMember` 同时承载"可达性/能力"视角 |
| **Q2** | Cargo feature `iroh` 切换,bootstrap 二选一,无运行时热切 |
| **Q4** | Clipboard 每次同步开新双向流,不做长连复用 |
| **Q5** | 默认使用 iroh 官方 relay,可通过 `SyncSettings` 覆盖(包括关闭/自建) |

## ✅ Q3 已有结论(基于 milestone/1.0.0 只读调研)

**Q3 = 独立 iroh 密钥文件**,具体方案:
- 密钥位置:`uc-infra/src/network/iroh/identity_store.rs`(新建,不复用 `platform::SystemIdentityStore`,后者还是 libp2p 专用)
- 存储后端:复用 `uc_core::ports::SecureStoragePort`,key = `"iroh-identity:v1"`
- 指纹展示:复用 `uc-infra/src/security/identity_fingerprint.rs`(公钥→Base32,对 Ed25519 通用)

## ✅ 已敲定决策(2026-04-19 · Slice 1 outside-in 细化)

> **方法论转向**:从原"先列 port → 后写业务"改为 **业务故事 → domain → usecase → 让 port 从依赖里被发现**(outside-in)。
> 见 `progress.md` 2026-04-19 session 的过程记录。

### 边界决策(Q-α ~ Q-ε)

| # | 决策 | 落地 |
|---|---|---|
| Q-α | `EndpointTicket` / `NodeTicket` / `NodeHandle` **不进 core** | 不暴露 iroh wire 类型,opaque handle 也不出现在 core |
| Q-β | `ReachVia { Direct/Relay }` **不进 core** | `Reachability` 简化:`Connected/Reachable/LastSeen{ms_ago}/Unknown` 无 via 字段 |
| Q-γ | "成员=身份"语义 | 实际复用 `SpaceMember.identity_fingerprint`,不新建 `NodeIdentity` 域对象 |
| Q-δ | 单例约束(同时 1 个 pending invitation)放 application 编排 | core 的 `PairingInvitation` 不强制不变量 |
| Q-ε | `InvitationCode` 格式/校验放 infra | core 只有 `InvitationCode(String)` newtype,不验证格式 |

### Domain 边界裁决(Q-1 ~ Q-3)

| # | 决策 | 说明 |
|---|---|---|
| Q-1 | `PairingInvitation` 是 **core 聚合** | 业务规则(TTL / 状态转换 / consume)集中在 core |
| Q-2 | `PairingInvitation` **不持久化**(in-memory) | 进程崩溃 → pending 丢失,用户重发码;5 分钟 TTL 反正快 |
| Q-3 | 不需要"区分本机"新概念 | 复用现有 `DeviceIdentityPort::current_device_id()`(已在 milestone) |

### 实现命名/对称设计

| # | 决策 | 说明 |
|---|---|---|
| 命名-1 | port 命名 `PairingInvitationPort`(不叫 `RendezvousClientPort`) | "rendezvous" 是实现机制,业务概念是"邀请" |
| 命名-2 | port 命名 `LocalIdentityPort`(不叫 `NodeIdentityStorePort`) | "Node" 暗示 iroh,"Local Identity" 业务中性 |
| 命名-3 | 删除 `LocalEndpointTicketPort` | ticket 是 adapter 内部细节,core 不应见 |
| 设计-1 | `LocalIdentityPort` 显式 `create()` + `current_fingerprint()` | 跟 `SpaceAccessPort::initialize/unlock` 对称;identity 在 A1 时生成 |
| 设计-2 | `LocalDeviceNamePort::current()` 总能返回 | 系统 hostname/计算机名必有;无业务文案兜底 |
| 设计-3 | joiner 侧 redeem+dial 合并到 `PairingTransportPort::dial_by_invitation` | 避免 opaque handle 穿过 core |
| 设计-4 | 业务层只面对 `DeviceId` | infra 内部完成 `DeviceId → fingerprint(查 SpaceMember)+ NodeAddr(查 discovery)→ connection` |

### Slice 0.5 预备小重构(独立 PR,Slice 1 启动前完成)

把 `IdentityFingerprint` 上提到 core 并统一类型:
- 新建 `uc-core/src/security/identity_fingerprint.rs`
- `SpaceMember.identity_fingerprint: String` → `IdentityFingerprint`
- `TrustedPeer.peer_fingerprint: PeerFingerprint` → `IdentityFingerprint`(`PeerFingerprint` 这个名字本身是冗余的)

### 概念三分(business invariant)

| 概念 | 类型 | 用途 | 出现位置 |
|---|---|---|---|
| `DeviceId` | UUID v4 | 业务标识(主键 / 引用) | core 业务层主要 ID |
| `IdentityFingerprint` | 公钥 SHA-256 截断 + Base32 | **身份验证**("是不是同一台设备") | core 的 SpaceMember/TrustedPeer 字段 |
| iroh `NodeAddr`(infra 内部) | relay url + direct addrs | **网络寻址**("怎么连到这台设备") | infra 内部不上浮 |

## ⚠ 新增外部依赖 — milestone/1.0.0 分支

**观察**:该分支正在做 Slice 1 migration,改动范围:
- 新 `SpaceAccessPort` / `BlobCipherPort`(领域中性)
- 数据层:`paired_device` → `space_member` / `trusted_peer`(表 + repo)
- `identity_fingerprint` 从 platform 下沉到 infra

**结论**:关注点不同(空间加密 vs 设备身份/网络),冲突面小;但:
- Phase 1(C.2 扩展 `space/`)与其 `space_access` / `trusted_peer` 模型**高度重合**
- 我们需要**借用**它们的 `trusted_peer` 模型作为 C.2 承载体
- **时序约束**:Phase 1 必须在 milestone/1.0.0 合入 dev(或我们从该分支起飞)之后启动;Phase 0 无依赖,可先走

## 🏛 架构规则:Facade 是唯一对外入口

```
┌────────────────────────────────────────────────────┐
│ 外部调用方                                           │
│  · Tauri command(uc-tauri)                        │
│  · Daemon IPC(uc-daemon / uc-daemon-client)      │
│  · CLI(uc-cli)                                    │
│  · 可能的 web / mobile host                        │
└────────────────────┬───────────────────────────────┘
                     │  只允许
                     ▼
             ┌───────────────┐
             │   Facade      │  ← uc-application/src/<domain>/facade.rs
             │ (稳定契约)     │
             └───────┬───────┘
                     │
                     ▼
          UseCase / Orchestrator  ← uc-application/src/<domain>/
                     │
                     ▼
                   Port          ← uc-core/src/ports/
                     │
                     ▼
                 Adapter         ← uc-infra/ 或 uc-platform/
```

### 硬规则
1. **Tauri command / Daemon IPC / CLI 只允许调用 Facade**。禁止直接 `use uc_application::<domain>::<UseCase>`
2. Facade 聚合同一业务域的多个 UseCase,提供稳定的"对外契约";UseCase 可随意重构,Facade 签名不动
3. UseCase / Orchestrator 之间**同域内可互相依赖**,**跨域通过 Port 协作**(不直接 import 另一个 UseCase)
4. **应用内调用**(bootstrap / daemon 主循环触发 F1/F2)**允许**直接调 UseCase,不算违规
5. **CI 守卫**(建议):`rg '::(pairing|space_access|clipboard_sync|member_roster|setup)::[A-Z][a-zA-Z]*UseCase'` 在 `uc-tauri / uc-daemon / uc-cli` 下为空

### 现有 Facade(milestone/1.0.0 已建)

| Facade | commit | 扩展/沿用 |
|---|---|---|
| `PairingFacade` | `dd3978f5` | Slice 1 扩展(rendezvous / shortcode 方法) |
| `SpaceAccessFacade` | `cb171f37` | Slice 1 沿用 |
| `SetupFacade`(+ 14 UseCase/Query) | `b1285605` | Slice 1 沿用 |

### 新建 Facade(本次重构)

| Facade | Slice | 聚合 UseCase |
|---|---|---|
| `ClipboardSyncFacade` | Slice 2 | C1 outbound / C2 inbound / 订阅事件 |
| `MemberRosterFacade` | Slice 2 | E1 roster / E2 presence events / A3 revoke / A5 rename |
| `BlobProgressFacade`(可选) | Slice 3 / 技术债 T-01 | 暴露 blob 进度流 |

---

## 📦 Phases

### Phase 0 — 技术侦察 ✅(2026-04-18 完成)
**产出**:`findings.md` F-010 ~ F-019 章节
**完成**:
- [x] iroh 0.95.1 + iroh-blobs(latest)API 速览写入 F-011 / F-012
- [x] Q1 已定(C.2),不新建 domain,无需选名
- [x] iroh ↔ domain 概念映射表(F-013)
- [x] iroh-blobs store 与 `uc-infra/blob` 分工(F-017)— 两层加密独立,目录分开
- [x] ALPN 规划(F-014):`/uniclipboard/pairing/1` + `/uniclipboard/clipboard/1` + `iroh_blobs::ALPN`
- [x] Discovery 三层(mDNS + n0 DNS/Pkarr + OOB ticket,F-015)
- [x] Relay 策略(默认官方,可覆盖,F-016)
- [x] 五个底层 port 签名草稿(F-018)
**遗留到 Phase 1**:F-019 列出的 4 个细节

> **方向修正**(2026-04-18):原 Phase 1-8 线性设计(先 port → 再业务)违反六边形。已改为 **Slice 切片**:每个 slice 从零到端到端交付一个业务能力,port 按需引入。

### Slice 0.5 · IdentityFingerprint 统一(预备) ✅ 2026-04-19 完成

> **背景**:Slice 1 outside-in 设计发现 `SpaceMember.identity_fingerprint: String`、`TrustedPeer.peer_fingerprint: PeerFingerprint`、`uc-infra/security/IdentityFingerprint` 三处类型分裂,但底层是同一个值(Ed25519 公钥 SHA-256 截断 Base32)。

**任务**:
- [x] 新建 `uc-core/src/security/identity_fingerprint.rs`(算法无关的值对象 + verify 行为;SHA-256 派生仍留 infra)
- [x] `SpaceMember.identity_fingerprint: String` → `IdentityFingerprint`
- [x] `TrustedPeer.peer_fingerprint: PeerFingerprint` → `IdentityFingerprint`(`PeerFingerprint` 已删除)
- [x] mapper / repo / 调用方跟随类型升级
- [x] codec/schema 层无变化(SQLite 列仍存 Base32 字符串)
- [x] `IdentityFingerprintFactoryPort::from_public_key` 返回类型 `String` → `IdentityFingerprint`
- [x] `SpaceAccessContext.peer_fingerprint` / `AdmitMember.identity_fingerprint` / `PairingHandshakeOutcome.identity_fingerprint` / `TrustVerificationChallenge.peer_fingerprint` / `TrustPeer.peer_fingerprint` / `TrustedPeerEvent::PeerVerificationRequired.peer_fingerprint` 全部升级为 `IdentityFingerprint`(Q-0.5 决策 (a))
- [x] infra `Sha256IdentityFingerprintFactory` 返回 core 的 `IdentityFingerprint`;`FingerprintError` 拆分为 core(`InvalidFormat` / `Mismatch`) + infra `FingerprintDerivationError::InvalidKeyLength`

**验收**:
- [x] core 内只有一个 `IdentityFingerprint` 类型表达"公钥指纹"
- [x] `cargo check --workspace` 通过

## Session 2026-04-24 · Slice 3 Phase 1 T0 开工

**完成标准**:
- 写出 `uc-infra/tests/iroh_blobs_probe.rs`,覆盖本地 store、tag、ticket 字节编解码、router/downloader 单节点自回环。
- 用真实 `cargo test -p uc-infra iroh_blobs_probe --tests` 验证通过。
- 若探针发现计划中的 iroh-blobs API / 版本假设不成立,同步修订 `slice3-phase1-plan.md` 与本计划记录。

**T0 当前状态**:`complete`

**T0 初始发现**:
- `uc-infra` 直接依赖 `iroh 0.95.1`。
- 当前 `iroh-blobs 0.95.0` 依赖 `iroh 0.93.2`,和共享 `IrohNodeBuilder` 的 endpoint 类型不一致。
- 已确认 `iroh-blobs 0.97.0` 依赖 `iroh 0.95`,与当前共享 endpoint 路线匹配;T0 将先用测试跑实,再升级依赖。
- 已升级到 `iroh-blobs 0.97.0` + `iroh-tickets 0.2`,T0 探针 4 项通过。
- `downloader().download` 只吃 provider id;adapter `fetch` 必须先用 ticket 内 `EndpointAddr` 做一次 `endpoint.connect(..., iroh_blobs::ALPN)` 预热。

**Errors Encountered**:
| Error | Attempt | Resolution |
|---|---|---|
| `cargo tree` 被 `sccache` 拦截:`Operation not permitted` | 直接运行 cargo / escalate 后仍失败 | 使用 `RUSTC_WRAPPER=` + `CARGO_BUILD_RUSTC_WRAPPER=` 覆盖本机 cargo config |
| 沙箱网络无法访问 crates.io | `cargo test` 触发依赖解析 | escalation 后下载并完成测试 |
| 沙箱禁止绑定本地 UDP socket | loopback downloader 探针 | escalation 后正常运行 |
- [x] 现有 pairing/membership/trusted_peer 单测通过(uc-core 22 + uc-infra 24 + uc-application 58 + uc-app 7)

**设计决策 Q-0.5**:`SpaceAccessContext.peer_fingerprint` 采用 **(a) 升级为 `Option<IdentityFingerprint>`**,在 WS/daemon-contract 投影边界(query.rs、host.rs snapshot 构造点)用 `.to_string()` 降维回 JSON。DTO 契约(`daemon-contract::types`、`daemon-client::realtime`、`setup::state::JoinSpaceConfirmPeer.peer_fingerprint`、`pairing::events::PairingDomainEvent`)**保持 `Option<String>`**——属序列化/UI 边界。

**UI/wire 契约保留 String 的点**:
- `PairingState::AwaitingUserConfirm.peer_fingerprint: String`(state_machine 序列化态)
- `PairingAction::ShowVerification.{local_fingerprint, peer_fingerprint}`(给 UI 展示层)
- `PairingContext.{local_fingerprint, peer_fingerprint}: Option<String>`(state_machine 内部缓存,与 UI 形态对齐)
- `PairingDomainEvent::PairingVerificationRequired.{local_fingerprint, peer_fingerprint}`(UI event)
- `P2pPeerSnapshot.identity_fingerprint: String`(DTO)

这些在调用 `crypto.fingerprint.from_public_key(...)` 成功点立即 `.to_string()`,避免 IdentityFingerprint 跨 UI 边界。

**为什么独立做**:
- 不与 Slice 1 业务变更纠缠
- 影响面小但跨多个 crate,独立 PR review 友好
- Slice 1 启动时直接享用统一类型

---

### Slice 1 · Pairing E2E(MVP 原点) ✅ 2026-04-20 完成

> **重大修订(2026-04-19)**:本 slice 经过 outside-in 重新规划,port 数量从原计划 6 新 + 2 iroh impl + 1 扩展 → **3 真新 port + 待 B1/B2/F1/F2 反推确认 2-3 个**。
> 阻塞已解除(milestone/1.0.0 已合入 `slender-soybean` 分支)。

> **执行进度(2026-04-20 更新)**:P1-P9 全部完成,sponsor+joiner E2E 和单机 CLI e2e 双路径打通。Slice 1 核心功能闭合。
>
> | 子 phase | 状态 | 产出 |
> |---|---|---|
> | P1 bootstrap iroh/rendezvous deps | ✅ | `d06db536` |
> | P2 PairingInvitation aggregate | ✅ | `a05aa86c` |
> | P3 A1 InitializeSpace + A2 UnlockSpace | ✅ | `52550b7b` |
> | P4 AppFacade + SpaceSetupFacade | ✅ | `1fc10e43` / `b0541110` |
> | P5 IrohIdentityStore impl LocalIdentityPort | ✅ | `55ef4877` |
> | P6 F1 自动 start_network on A1/A2 | ✅ | `59c24870` |
> | P7a RendezvousPairingInvitationAdapter(sponsor) | ✅ | `d264059d` |
> | P7b PairingSessionPort + PairingEventPort + session_message | ✅ | `61d5e7c7` |
> | P7c.1 wire codec(postcard) | ✅ | `1d02d348` |
> | P7c.2 IrohPairingSessionAdapter joiner side(dial) | ✅ | `6675ab00` |
> | P7c.3 sponsor ALPN handler + PairingEventPort impl | ✅ | `9460a71b` |
> | P7d B1 IssuePairingInvitationUseCase + holder | ✅ | `5259d52d` |
> | P7e sponsor inbound subscriber + rendezvous consume | ✅ | `35a20e37` |
> | P7f sponsor 握手(v1 · FSM 复用 + 直写 repo) | ✅ | `1049eaae` |
> | P7f cleanup(sponsor_handshake 独立 + admit/trust 走 use case) | ✅ | `bdff9588` |
> | P7g sponsor handshake TTL watchdog(内部 spawn,不走 TimerPort) | ✅ | `5befc370` |
> | P7h joiner 侧 RedeemPairingInvitationUseCase(线性,F-053 推翻 FSM 计划) | ✅ | `697e182b` / `d788dcc5` |
> | P8a IrohNode(共享 endpoint + ALPN 扩展点) | ✅ | `e160f2fd` |
> | P8b uc-bootstrap 装配 SpaceSetupFacade | ✅ | `e160f2fd` |
> | P8c E2E sponsor+joiner 握手(wiremock + 真 iroh loopback + 真 crypto) | ✅ | `e160f2fd` |
> | P9a PairingOutcome broadcast + try_resume_session + SetupStatus.space_id | ✅ | `4fe4f16b` |
> | P9b uniclipboard-cli init/invite/join + 单机 e2e 脚本 | ✅ | `f43ff8c4` |
> | P9 infra 支撑(rendezvous URL 契约 + noop clipboard + webpki-roots) | ✅ | `2890c43b` |
>
> **关键决策沉淀**:
> - F-049 rendezvous ticket 编码约定(P7a)
> - F-050 Slice 5 清理签到名单(P7b)
> - F-051 `prepare_join_offer` Branch A 忽略 passphrase(P7f)
> - F-052 Sponsor 侧不走 `SpaceAccessStateMachine` — persist 先于 Confirm 的排序与 FSM action order 冲突(P7f cleanup)
> - F-053 Joiner 侧也不走 FSM — 推翻 F-052 末尾"joiner 保留 FSM"的预判(P7h 实施时)
> - F-054 A1 identity 生命周期归 bootstrap(P8 实施时发现原 A1 `create()` 和 iroh endpoint 预绑定冲突)
> - F-055 iroh sponsor adapter 必须 spawn per-session recv pump(P8 E2E 暴露 `MessageReceived` 事件缺失)
> - F-056 `PairingOutcome` broadcast:sponsor 侧握手终态作为应用事件(P9a,facade 订阅接口)
> - F-057 CLI session resume:`try_resume_session` 是已有基础设施,不需要新 cache 层(P9a / CLI invite 撬出)
> - F-058 `SpaceId` 必须在 `SetupStatus` 里持久化,否则 sponsor/joiner 漂移(P9a,用户在单机 e2e 发现双方 id 对不上)
> - F-059 rendezvous 客户端 URL 形态与服务端不匹配(P9 infra,subagent 读 uc-rendezvous 源码确认)
> - F-060 reqwest 0.12 `rustls-tls` 不自带 root CA,5 crate 统一加 `rustls-tls-webpki-roots`(P9 infra)
> - F-061 非-bundled macOS CLI 的 NSPasteboard 空返回 → `UC_DISABLE_SYSTEM_CLIPBOARD=1` + `NoopSystemClipboard` 兜底(P9 infra)

**目标**:两台全新设备通过**配对邀请凭据**(短码等)+ passphrase 完成配对,持久化互成 SpaceMember;进程重启后能 unlock 读到记录。

**覆盖 usecase**:A1 initialize / A2 unlock / B1 sponsor / B2 joiner / F1 最小启动 / F2 关闭

#### 真新 port 清单(已确认 3 个,待反推 2-3 个)

| Port | 状态 | 服务 usecase |
|---|---|---|
| `LocalIdentityPort`(uc-core) | ✅ 已确认 | A1(create)/ B1 / B2 / 任何需要本机指纹的地方 |
| `LocalDeviceNamePort`(uc-core,实现在 uc-platform) | ✅ 已确认 | A1(默认 device_name) |
| `PairingInvitationPort`(uc-core) | ✅ 已确认 | B1 / B2 |
| `PeerAddressRepositoryPort` | 🟡 待 F1 反推确认 | F1 last-known NodeAddr 缓存 |
| `PairingTransportPort::dial_by_invitation` 扩展 | 🟡 待 B2 反推确认 | B2 redeem+dial 合并 |
| `NetworkControlPort::stop_network` 扩展 | 🟡 待 F2 反推确认 | F2 |
| `PeerDirectoryPort` reachability 方法扩展 | 🟡 待 F1 反推确认 | F1 |

#### 删除/不再新建的 port(原计划)

- ❌ `NodeIdentityStorePort` — 改为 `LocalIdentityPort`(显式 create + current_fingerprint)
- ❌ `LocalEndpointTicketPort` — adapter 内部细节,core 不见
- ❌ `RendezvousClientPort` — 改为 `PairingInvitationPort`(业务语义)
- 🟡 `PeerIdentityResolverPort` / `PresencePort` — 待评估是否扩展 `PeerDirectoryPort` 而非新建

#### 复用(milestone/1.0.0)

`SpaceAccessPort` / `DeviceIdentityPort` / `MemberRepositoryPort` / `TrustedPeerRepositoryPort` / `ProofPort` / `SetupStatusPort` / `SecureStoragePort` / `PairingTransportPort`(trait,iroh 加新 impl) / `PairingFacade` / `PairingOrchestrator` / `PairingProtocolHandler` / `SpaceMemberRepo`

#### 真新 domain(只 1 个聚合)

```
uc-core/src/pairing/invitation/
  ├── invitation.rs    PairingInvitation, InvitationState
  ├── code.rs          InvitationCode (newtype String)
  ├── events.rs        InvitationEvent::{Issued, Consumed, Revoked, Expired}
  └── error.rs         ConsumeError, RevokeError
```

#### A1 · InitializeSpaceUseCase 草图

**业务故事**:用户首次装机 → Setup 向导填 passphrase + (可选) device_name → 创建本地加密空间 + 生成本机 identity + 落本机 owner SpaceMember。

| # | 业务步骤 | port |
|---|---|---|
| 1 | 校验 passphrase 与 confirm 一致 | application |
| 1.5 | 解析 / 持久化 `device_name`(UI 传入或沿用 `SettingsPort.load().general.device_name`) | `SettingsPort::load/save` 🔁 复用(F-046 取消 `LocalDeviceNamePort`) |
| 2 | 创建加密空间 | `SpaceAccessPort::initialize(space_id, passphrase)` |
| 3 | **生成本机 identity** | `LocalIdentityPort::create()` 🆕 |
| 4 | 拿本机 DeviceId | `DeviceIdentityPort::current_device_id()` |
| 5 | 构造 owner SpaceMember(用步骤 3 的指纹) | core domain |
| 6 | 持久化本机 SpaceMember | `MemberRepositoryPort::save` |
| 7 | 标记 setup 完成 | `SetupStatusPort::mark_completed` |

**Command 改动**:`SubmitNewSpacePassphraseCommand` 扩展 `device_name: Option<DeviceName>`(breaking,milestone 用户少接受)。

**Facade 表面**(B2 决策 Q-B2-7 = A 后定):A1 入口由 `AppFacade::initialize_space` 提供;`SetupFacade::submit_new_space_passphrase` 仍保持 `pub`(本 slice 不破坏旧接入),Tauri/daemon/CLI 后续 slice 切到 AppFacade。

**业务不变量**:A1 是原子动作,要么全成 + `has_completed = true`,要么全失败 → 下次走 A1 重做(由 SpaceAccess.initialize 幂等性 / setup_status 兜底)。

#### A2 · UnlockSpaceUseCase 草图

**业务故事**:用户重启 app → UI 弹 unlock → 输 passphrase → 从 OS keychain 取 KDK 解锁 ActiveSpace。

| # | 业务步骤 | port |
|---|---|---|
| 1 | 校验 setup 已完成 | `SetupStatusPort::has_completed` |
| 2 | 解锁加密空间 | `SpaceAccessPort::unlock(space_id, passphrase)` |
| 3 | 返回 result | — |

**A2 没有 self-member 自愈**:走到 A2 = A1 已成功完成,identity / Space / SpaceMember 三者必定齐备。

**A2 端口依赖**:仅 2 个 port(纯复用 milestone)。

#### B1 · IssuePairingInvitationUseCase 草图

**业务故事**:Sponsor(已 unlock)用户点"添加新设备" → 系统生成邀请凭据 → UI 展示(短码/QR/文本 ticket)→ sponsor 进入"等待 joiner 加入"状态 → 5 分钟内有人用这个凭据连进来 → 配对协议自动启动(orchestrator handler 处理,不在 B1 UseCase 内)。

**前置**:Space 已 unlock(A1+A2)+ Network 已启动(F1)+ 当前无其他 pending invitation。

**Command / Result**:
```rust
pub struct IssuePairingInvitationCommand {
    // v1 无字段;TTL 由 server 决定
}

pub struct IssuePairingInvitationResult {
    pub code: InvitationCode,
    pub expires_at: DateTime<Utc>,
}
```

**业务步骤**:

| # | 动作 | 实现 |
|---|---|---|
| 1 | 校验 Space 已 unlocked | `SpaceAccessPort::is_unlocked` |
| 2 | 懒清理过期的 in-memory invitation | application |
| 2a | **若仍有 pending(未过期)→ 本地清空 + 发 `InvitationEvent::Revoked`**(server 端 5min 自然过期,不调 server) | application |
| 3 | 拿本机 DeviceId | `DeviceIdentityPort::current_device_id` |
| 4 | 调 `PairingInvitationPort::issue_invitation()` | 🆕 port |
| 5 | 构造 `PairingInvitation`(用 server 给的 expires_at) | core domain |
| 6 | 写入 in-memory `Arc<RwLock<Option<PairingInvitation>>>` | application |
| 7 | 状态机 `Idle → AwaitingInvitationRedeem { code, expires_at }` | application |
| 8 | 发 `InvitationEvent::Issued` | `PairingEventPort` |
| 9 | 返回 `IssuePairingInvitationResult` | — |

**入站事件处理**(B1 触发后,但属于 PairingOrchestrator handler,不是 B1 UseCase):

```
PairingTransport 入站事件(对端 device_id, incoming_code)
   ↓
application 检查:in_memory.code == incoming_code?
   ├─ 匹配 → 进入 RecvRequest,走现有 PairingProtocolHandler
   └─ 不匹配(或 in_memory = None) → 关闭入站流,拒绝
```

→ ✅ **B2 已定稿**:code 不放 stream metadata,**放在 `PairingRequest` 协议消息字段里**(wire 改动);`PairingTransportPort` 入站事件接口**不需扩展**;application 层在收到 `PairingRequest` 时匹配 code(详见 B2 章节)。

**`PairingInvitationPort` 终版**(只 1 个方法):
```rust
#[async_trait]
pub trait PairingInvitationPort: Send + Sync {
    /// Sponsor 发出邀请。TTL 由 server authoritative 决定(防 client 时钟漂移)。
    async fn issue_invitation(&self) -> Result<IssuedInvitation, InvitationError>;
}

pub struct IssuedInvitation {
    pub code: InvitationCode,
    pub expires_at: DateTime<Utc>,
}

pub enum InvitationError {
    NetworkNotStarted,
    ServiceUnavailable,
    Internal(String),
}
```

**B1 决策(Q-B1-1 ~ Q-B1-5)**:
| # | 决策 |
|---|---|
| Q-B1-1 | TTL 由 adapter/server authoritative 决定;application 不持有 ttl 常量 |
| Q-B1-2 | 过期清理走懒清理(下次 issue 时检查) |
| Q-B1-3 | 每次 issue 都生成新 code,本地清空旧的 + 发 Revoked 事件;**不调 server**(server 不支持 revoke,旧码靠 5min 自然过期 + sponsor 入站时 code 匹配检查保安全) |
| Q-B1-4 | Network 未启动由 adapter 内部 issue 失败上抛,无需 NetworkStatusPort |
| Q-B1-5 | 配对协议失败也清空 invitation,UI 提示用户重新发起 |

**安全模型**(server 不 revoke 也安全的原因):
- 旧 code 即便被攻击者用 → server 给攻击者 sponsor NodeAddr → 攻击者拨号 sponsor → sponsor 入站事件 metadata 带的 code 跟 in_memory 不一致 → **拒绝连接**
- → 安全性靠 sponsor 侧本地状态匹配,不靠 server revoke

**业务不变量**:
- 同一时间至多 1 个 pending invitation(application 编排守门)
- invitation issuer 必须是本机 `current_device_id`
- application 层 in-memory 是 single source of truth(server 端 stale 不影响)

**边界情况**:
| 场景 | 处理 |
|---|---|
| Space 未 unlock | `PairingError::SpaceNotUnlocked` |
| 已有 pending(未过期) | 本地清空 + 发 Revoked + 创建新(无错误) |
| Network 未启动 | adapter 内部报 `InvitationError::NetworkNotStarted` |
| Server 不可达 | `InvitationError::ServiceUnavailable` |
| 配对协议中途失败 | invitation 清空(无论 in-memory 还是状态机回 Idle) |

**Facade 表面**:
```rust
impl PairingFacade {
    pub async fn issue_pairing_invitation(&self)
        -> Result<IssuePairingInvitationResult, PairingError>;
}
```

**状态机改动**(`PairingStateMachine`):
- 加状态 `AwaitingInvitationRedeem { code: InvitationCode, expires_at: DateTime<Utc> }`
- 转移:`Idle --IssueInvitation--> AwaitingInvitationRedeem`
- 转移:`AwaitingInvitationRedeem --IssueInvitation(再次)--> AwaitingInvitationRedeem(新 code)`(隐式清空旧的)
- 转移:`AwaitingInvitationRedeem --IncomingPairingRequest(code 匹配)--> RecvRequest`
- 转移:`AwaitingInvitationRedeem --Expired/Revoked--> Idle`

**B1 真新 port 增量**:1 (`PairingInvitationPort`,1 个方法)。

---

#### B2 · RedeemPairingInvitationUseCase 草图

**业务故事**:Joiner(全新设备,无 Space)用户输入 invitation code + passphrase + (可选) device_name → 系统拨号 sponsor → 配对协议握手 → 拿到 sponsor 的 keyslot_offer → 用 passphrase 解出 master_key → 发挑战响应 → 收 confirm → 持久化本地 Space + self SpaceMember + sponsor SpaceMember + sponsor TrustedPeer。

**前置**:Joiner 是全新设备(无 Space、无 SpaceMember、`setup_status.has_completed = false`);Identity 可能已 create(上次 B2 重试残留),也可能未 create(首次);Network 已启动(F1)。

**Command / Result**:
```rust
pub struct RedeemPairingInvitationCommand {
    pub code: InvitationCode,
    pub passphrase: Passphrase,
    pub device_name: Option<DeviceName>,   // 跟 A1 一致
}

pub struct RedeemPairingInvitationResult {
    pub space_id: SpaceId,
    pub self_device_id: DeviceId,
    pub sponsor_device_id: DeviceId,
    pub sponsor_fingerprint: IdentityFingerprint,
}
```

**业务步骤**:

| # | 动作 | 实现 |
|---|---|---|
| 1 | 解析 device_name | `LocalDeviceNamePort::current()` |
| 2 | **ensure 本机 identity**(失败重试友好) | `LocalIdentityPort::ensure()` 🆕 新方法 |
| 3 | 拿 DeviceId + fingerprint | `DeviceIdentityPort` + `LocalIdentityPort::current_fingerprint` |
| 4 | **拨号 sponsor**(adapter 内部:rendezvous resolve(code) + iroh dial + 开 bi-stream) | `PairingTransportPort::dial_by_invitation(code)` 🆕 扩展 |
| 5 | 发 `PairingRequest { invitation_code, identity_fingerprint, device_name, nonce, ... }` | adapter wire(infra) |
| 6 | 收 `PairingKeyslotOffer { keyslot_blob, challenge_nonce }` | adapter wire |
| 7 | `derive_master_key_for_proof(offer, passphrase)` → MasterKey | `SpaceAccessPort::derive_master_key_for_proof` ✅ milestone |
| 8 | 构造并发 `PairingChallengeResponse` | adapter wire |
| 9 | 收 `PairingConfirm.success` 或 `PairingReject` | adapter wire |
| **10** | **PairingConfirm.success → 提交点(persist 全部本地状态)** | AppFacade 编排 |
| 10a | 持久化本地 Space + KeySlot(用 joiner passphrase 包装 master_key) | 复用 milestone `CompleteJoinSpaceUseCase` 内部逻辑(待 Read 确认接口) |
| 10b | 持久化 self SpaceMember | `MemberRepositoryPort::save` |
| 10c | 持久化 sponsor SpaceMember + TrustedPeer | `MemberRepositoryPort::save` + `TrustedPeerRepositoryPort::save` |
| 10d | 标记 setup 完成 | `SetupStatusPort::mark_completed` |
| 11 | 返回 result | — |

**入站 code 匹配定稿**(B1 F-041 悬念):
- code 不在 stream metadata,**放在 `PairingRequest` 协议消息字段里**
- sponsor 收 `PairingRequest` 后,application 层匹配 `in_memory.code == request.invitation_code`
- 匹配 → 进入 RecvRequest;不匹配 → 发 `PairingReject` + 关流
- → `PairingTransportPort` 入站事件接口**不需要扩展**;wire 层 `PairingRequest` 加 `invitation_code: String` 字段

**B2 决策(Q-B2-1 ~ Q-B2-8)**:

| # | 决策 |
|---|---|
| Q-B2-1 | `LocalIdentityPort` 加 `ensure()` 方法(B2 用幂等,A1 仍用严格 `create()`) |
| Q-B2-2 | 复用 milestone `CompleteJoinSpaceUseCase` 内部 persist 逻辑;**新增 AppFacade 集中编排**(见下文) |
| Q-B2-3 | joiner 失败:identity 保留(下次复用),本地 Space/SpaceMember 不持久化(以 PairingConfirm 为提交点) |
| Q-B2-4 | UseCase 同步 await 整个流程(5-30s),UI 显示 spinner |
| Q-B2-5 | **不**做指纹核对(passphrase 验证已是身份证明) |
| Q-B2-6 | **单一 AppFacade**(Slice 1 先建单一,后续 Slice 大了再按业务拆) |
| Q-B2-7 | A1/A2 也搬到 AppFacade(完全集中,外部调方一个接入点) |
| Q-B2-8 | Tauri/daemon/CLI 切换到 AppFacade **推迟到后续 slice**;Slice 1 内 sub-facade 保持 `pub` 不破坏 |

**业务不变量**:
- B2 是原子事务:PairingConfirm.success 之前任何失败都不持久化(除 identity 外)
- B2 完成后:`MemberRepositoryPort::list()` 含 2 条记录(self + sponsor);`TrustedPeerRepositoryPort::list()` 含 1 条(sponsor)
- B2 完成后:`SetupStatusPort::has_completed = true`,后续 A2 unlock 即可恢复

**边界情况**:
| 场景 | 处理 |
|---|---|
| Code 格式无效 | adapter 校验报错 |
| Code 在 server 端不存在 / 已过期 | `PairingError::InvitationNotFound / Expired` |
| Sponsor 不可达(NodeAddr 拨号失败) | `PairingError::SponsorUnreachable` |
| Passphrase 错(derive_master_key_for_proof 失败) | `PairingError::WrongPassphrase`,无副作用 |
| Sponsor 拒绝(PairingReject) | `PairingError::SponsorRejected { reason }`,无副作用 |
| 协议中途网络断 | `PairingError::ProtocolInterrupted`,无副作用 |
| 提交点失败(10a-10d 中任一) | ⚠ 已经发了 PairingConfirm 给 sponsor,sponsor 那边可能已落库;joiner 这边写不下去 → 状态分裂(技术债标记,Slice 1 暂接受 + 错误日志清晰) |

**Facade 表面**(走 AppFacade):
```rust
impl AppFacade {
    pub async fn redeem_pairing_invitation(&self, cmd: RedeemPairingInvitationCommand)
        -> Result<RedeemPairingInvitationResult, PairingError>;
}
```

**B2 真新 port 增量**:0(只有 2 个方法扩展)。

---

#### Slice 1 架构补充:AppFacade 集中化(Q-B2-2 / Q-B2-6/7/8 推出)

**现状**(milestone):外部调方(Tauri / daemon / CLI)需注入多个 sub-facade(`PairingFacade` / `SetupFacade` / `SpaceAccessFacade`),跨域动作要自己组合。

**目标**:新增 `AppFacade` 作为统一对外入口,内部持有 sub-facade 并编排跨域动作。

```
外部调方 (Tauri / daemon / CLI)
   ↓
                AppFacade            ← 唯一对外入口(本 slice 新建)
                   ↓
   ┌──────────────┼──────────────┐
PairingFacade   SetupFacade   SpaceAccessFacade   ...    ← sub-facade 仍存在(内部协调)
   ↓                ↓                ↓
UseCase / Orchestrator / Port
```

**位置**:`uc-application/src/facade/`(新目录)
```
uc-application/src/facade/
  ├── mod.rs            pub use AppFacade
  └── app_facade.rs     AppFacade 类型 + 跨域方法
```

**AppFacade Slice 1 终态接口**:
```rust
pub struct AppFacade {
    pairing: Arc<PairingFacade>,
    setup: Arc<SetupFacade>,
    space_access: Arc<SpaceAccessFacade>,
    // + 必要的 ports / repos
}

impl AppFacade {
    // A1(成功后内部串 StartNetworkUseCase)
    pub async fn initialize_space(&self, cmd: InitializeSpaceCommand)
        -> Result<InitializeSpaceResult, SetupError>;

    // A2(成功后内部串 StartNetworkUseCase)
    pub async fn unlock_space(&self, cmd: UnlockSpaceCommand)
        -> Result<UnlockSpaceResult, SetupError>;

    // B1
    pub async fn issue_pairing_invitation(&self)
        -> Result<IssuePairingInvitationResult, PairingError>;

    // B2
    pub async fn redeem_pairing_invitation(&self, cmd: RedeemPairingInvitationCommand)
        -> Result<RedeemPairingInvitationResult, PairingError>;

    // F1 入口(进程启动时调用一次):内部委托 BootstrapOnStartupUseCase
    pub async fn on_startup(&self)
        -> Result<BootstrapOutcome, AppError>;

    // F2 入口(进程退出前调用):内部委托 StopNetworkUseCase,幂等
    pub async fn on_shutdown(&self);
}
```

**职责划分**:
- 跨域动作走 AppFacade(本 slice 全部 A1/A2/B1/B2/F1/F2 都走)
- 单域查询(已有的 status/list 等)仍可由 sub-facade 提供;AppFacade 视情况 thin 转发
- AppFacade 在 uc-application 同一 crate 内,可直接调 sub-facade 的 `pub(crate)` UseCase
- 跨业务模块的 UseCase(如 `CompleteJoinSpaceUseCase`)保持 `pub(crate)` 但允许跨模块同 crate 调用

**Slice 1 内的工作量**:
- 新建 `facade/app_facade.rs`
- 实现 6 个跨域方法(A1/A2/B1/B2/F1/F2)
- sub-facade 保持 `pub`(本 slice 内不破坏外部调方)
- Tauri/daemon/CLI 切换到 AppFacade **推到 Slice 1.5 或后续 slice**

**与 §11.4 对外封装规则的关系**:
- 不破坏:外部 crate 仍只见 facade(只是多了一层 AppFacade)
- AppFacade 的存在让"跨业务流程的复杂动作"有自然容身之处,符合"应用动作面向使用方"的精神

#### F1 · Bootstrap + StartNetworkUseCase 草图

**设计**:按 outside-in,F1 拆为**两个 UseCase**(Q-F1-1):
- `BootstrapOnStartupUseCase`(上层分支派发,前置 = 进程启动)
- `StartNetworkUseCase`(纯 "Endpoint 起来",前置 = **已 unlock**)

**业务故事**:F1 = "进程启动 → 把网络层带到能响应业务的状态"。Slice 1 **不做预连**(Q-F1-4),理由:预连的业务动机(roster 在线状态 / C1 首字节延迟)全部属 Slice 2/3 的 usecase;Slice 1 只交付 pairing,只需 sponsor accept 入站 + joiner 靠 `dial_by_invitation` 出站即可。

##### `BootstrapOnStartupUseCase`

**触发**:`AppFacade::on_startup()` 开机调一次(Q-F1-2)。

**Command / Result**:
```rust
pub struct BootstrapOnStartupCommand; // 无字段

pub enum BootstrapOutcome {
    NotInitialized,                              // UI: 进 Setup 向导
    Locked,                                      // UI: 进 Unlock 页
    Started { fingerprint: IdentityFingerprint },
}
```

**业务流程**:
| # | 步骤 | Port / Facade |
|---|---|---|
| 1 | `SpaceAccessPort::is_initialized()` | 已有 |
| 1a | ❌ 未 init → `Ok(BootstrapOutcome::NotInitialized)`;返回 | — |
| 2 | `SpaceAccessPort::is_unlocked()` | 已有 |
| 2a | ❌ 已 init 未 unlock → `Ok(BootstrapOutcome::Locked)`;返回(UI 指导 A2;A2 成功路径由 `AppFacade::unlock_space` 内部串 StartNetwork) | — |
| 3 | ✅ 已 unlock → 委托 `StartNetworkUseCase::execute()` | UseCase |
| 4 | 返回 `Ok(BootstrapOutcome::Started { fingerprint })` | — |

##### `StartNetworkUseCase`

**前置**:Space 已 unlock(调用方保证 / 内部断言)。

**Command / Result**:
```rust
pub struct StartNetworkCommand; // 无字段

pub struct StartNetworkResult {
    pub fingerprint: IdentityFingerprint,
}

pub enum StartNetworkError {
    NotUnlocked,                      // 断言失败(bug)
    LocalIdentityMissing,             // A1/B2 未跑过(bug)
    EndpointBindFailed(String),       // adapter 透传 iroh bind err 文本
    AlreadyStarted,                   // 进程已调过 start(Q-F1-7 单例保护)
}
```

**业务流程**:
| # | 步骤 | Port |
|---|---|---|
| 1 | 断言 `SpaceAccessPort::is_unlocked()`;false 则 `Err(NotUnlocked)` | `SpaceAccessPort` |
| 2 | `LocalIdentityPort::get_current_fingerprint()` → `Some(fp)`;None 返 `LocalIdentityMissing`(Q-F1-3) | `LocalIdentityPort` |
| 3 | `NetworkControlPort::start_network()` → adapter 内部:bind iroh Endpoint + 注册 `/uniclipboard/pairing/1` ALPN handler;**bind 成功即返回**(Q-F1-6,< 100ms,非长 await) | `NetworkControlPort`(**新**) |
| 4 | 返回 `Ok(StartNetworkResult { fingerprint: fp })` | — |

**决策汇总**:
| # | 决策 |
|---|---|
| Q-F1-1 | 拆为 `BootstrapOnStartupUseCase` + `StartNetworkUseCase` |
| Q-F1-2 | `AppFacade::on_startup()` 开机调一次 |
| Q-F1-3 | `get_current_fingerprint()`,None = bug;不用 `ensure()`(`ensure()` 仅 B2 joiner 路径用) |
| Q-F1-4 | **不预连**,Slice 1 零成员枚举、零拨号;预连随 Slice 2 加入 |
| Q-F1-5 | N/A(预连没了,成员并发重连议题自动消解) |
| Q-F1-6 | `start_network` bind 成功即返回(< 100ms) |
| Q-F1-7 | Endpoint 进程级单例;不支持 re-start;省掉 `NetworkStatusPort` |
| Q-F1-8 | bind 失败上抛 `EndpointBindFailed`,UI 进错误态 |

**F1 真新 port 增量**:1 个 — `NetworkControlPort`(签名见 F-044)。

#### F2 · StopNetworkUseCase 草图

**触发**:`AppFacade::on_shutdown()` 在进程退出前调一次(Q-F2-2),对称 F1。

**Slice 1 特殊性**:无后台长 session(pairing 握手同步,B1/B2 同步 await)→ 原草图 "drain in-flight 5s" 属 Slice 2/3 剪贴板/blob 需求。Slice 1 **不做 graceful drain**(Q-F2-3),直接 close Endpoint。

**Command / Result**:
```rust
pub struct StopNetworkCommand; // 无字段

// Result:Ok(()) — 幂等且不返错(close 失败 swallow + log)
```

**业务流程**:
| # | 步骤 | Port |
|---|---|---|
| 1 | `NetworkControlPort::stop_network()` → 幂等关闭 Endpoint;未启动也 ok(Q-F2-4 / Q-F2-6);close 失败 swallow + log(Q-F2-5) | `NetworkControlPort` |
| 2 | 返回 `()` | — |

**决策汇总**:
| # | 决策 |
|---|---|
| Q-F2-1 | 独立 `StopNetworkUseCase`(对称 F1,为 Slice 2 drain 预留位置) |
| Q-F2-2 | `AppFacade::on_shutdown()` 对称 `on_startup` |
| Q-F2-3 | Slice 1 不 graceful drain,直接 close;drain 逻辑推 Slice 2/3 |
| Q-F2-4 | `stop_network` 幂等(重复调不抛) |
| Q-F2-5 | close 失败 swallow + log(进程反正要退) |
| Q-F2-6 | 不要求 "已 start" 前置;未启动调也 ok(幂等自然覆盖) |

**F2 真新 port 增量**:0(复用 `NetworkControlPort::stop_network`)。

##### Slice 1 真新 port 累计(编码前 Read 修正,2026-04-19)

**3 个** — 见 F-046 最终清单:

| Port | 说明 |
|---|---|
| `LocalIdentityPort` | 新建;iroh Ed25519 秘钥对 + fingerprint 暴露 |
| `PairingInvitationPort` | 新建;rendezvous 客户端抽象(实现:`uc-infra/src/rendezvous/client.rs`,调 `https://rendezvous.uniclipboard.app`) |
| Slice 1 新 pairing session port | 新建(名字待编码时定);替代旧 `PairingTransportPort`(打 `#[deprecated]`) |

**扩展**:`NetworkControlPort` 加 `stop_network()` 默认 no-op impl(libp2p 零侵入)
**取消**:`LocalDeviceNamePort` → 改复用 `SettingsPort`(F-046)

#### Slice 1 实施方案决策(N / I 系列,2026-04-19)

**N 系列**(由编码前 Read 引出):

| # | 决策 | 详见 |
|---|---|---|
| N-1 | `NetworkControlPort` **扩展** + `stop_network` 默认 no-op | F-045 / F-044 |
| N-2 | `PairingTransportPort` 旧 port 打 `#[deprecated]`,**新建独立 Slice 1 pairing session port** | F-045 / F-035 |
| N-3 | Rendezvous 客户端放 `uc-infra/src/rendezvous/client.rs`(不新建 crate) | F-045 |

**I 系列**(基础设施):

| # | 决策 | 说明 |
|---|---|---|
| I-1 | **Slice 0.5 先行** → Slice 1 | 独立 PR;A1 起步用统一 `IdentityFingerprint`。工作量:只上提值对象 + 改 `SpaceMember.identity_fingerprint: String` → `IdentityFingerprint`(`IdentityFingerprintFactoryPort` 已在 core) |
| I-2 | 工作分支 = `slender-soybean`(继续) | 不切新分支 |
| I-3 | Cargo:`iroh` / `iroh-blobs` / `reqwest`(rendezvous client) 一次加齐;**无 `#[cfg(feature = "iroh")]` 门控** | iroh 代码默认编译。依赖 N-1/N-2 的 libp2p 零侵入设计 |
| I-4 | Rendezvous server = `https://rendezvous.uniclipboard.app`(现有,不建 `uc-rendezvous` crate) | — |

#### 状态机改动(最小)

- 去 `AwaitingUserConfirm`(PIN 比对分支)
- 去 `AwaitingUserApproval`(sponsor 弹窗,前期决议去掉)
- Sponsor 侧加 `AwaitingInvitationRedeem { code, expires_at }`(原 `AwaitingShortcodeRedeem` 改名)
- `PairingChallenge{pin}` / `PairingResponse{pin_hash}` **保留不用**(下 slice 删除)

#### 对外表面(Facade + UI/IPC/CLI)

| 层 | 动作 |
|---|---|
| **`PairingFacade`**(扩展) | `issue_pairing_invitation() → (InvitationCode, expires_at_ms)` / `redeem_pairing_invitation(code, passphrase) → MemberHandle` / `revoke_pairing_invitation(code)` / `subscribe_pairing_events() → Stream<PairingEvent>` |
| **`SpaceAccessFacade`**(沿用,可能加 unlock) | `initialize(passphrase) → SpaceId` / `unlock(passphrase)` / `is_unlocked() → bool` |
| **`SetupFacade`**(扩展) | `submit_new_space_passphrase(passphrase, confirm, device_name?)` 命令扩展 |
| **Tauri commands** | `pairing_issue` / `pairing_redeem` / `pairing_revoke` / `pairing_events` / `space_initialize` / `space_unlock` / `space_is_unlocked` |
| **前端页面** | 首启 Setup 向导 / 配对页 / unlock 页 |
| **Daemon IPC**(`uc-daemon-contract`) | pairing issue/redeem/revoke + space init/unlock |
| **CLI**(`uc-cli`) | `uc space init` / `uc space unlock` / `uc pair issue` / `uc pair redeem --code XXXX-XXXX --passphrase ...` |
| **Bootstrap**(`uc-bootstrap`) | `#[cfg(feature = "iroh")]` 装配 iroh/invitation adapter |

#### 验收

- [ ] Slice 0.5 完成(IdentityFingerprint 统一)
- [ ] 两台机器端到端配对成功,持久化 SpaceMember + TrustedPeer
- [ ] 进程重启后 A2 unlock 成功,能读到持久化列表
- [ ] sponsor 侧 invitation 唯一性(同时只允许 1 个 pending,application 编排守门)
- [ ] `cargo build --features iroh` 和默认 build 都通过

#### 阻塞

- ✅ milestone/1.0.0 合入 dev → **已解除**(2026-04-19 当前分支已合入)
- 🟡 Slice 0.5 完成(独立小 PR)

---

### Slice 1 → Slice 2 交接(2026-04-20)

**Slice 1 已交付(commits)**:
- 架构层:`IrohNode` 共享 endpoint + ALPN router 扩展点(`e160f2fd`)
- 应用层:`SpaceSetupFacade` A1/A2/B1/B2/F2 + `try_resume_session` + `subscribe_pairing_completion`(`4fe4f16b`)
- 基础设施:rendezvous 客户端、iroh pairing session、headless clipboard、reqwest TLS 根(`2890c43b`)
- 装配:`uc-bootstrap::build_slice1_cli_context` + `build_space_setup_assembly`(`e160f2fd` / `4fe4f16b`)
- CLI:`uniclipboard-cli init/invite/join` + 单机双进程 e2e 脚本(`f43ff8c4`)
- 测试:`uc-application` 152 单测 + `uc-bootstrap` slice1_handshake_e2e(真 iroh loopback + wiremock rendezvous + 真 crypto)

**Slice 2 启动前应消化的 Slice 1 遗留**:
- T-15:A2 unlock 返回的 space_id 对齐 SetupStatus(不先做会污染 Slice 2 的 roster 查询)—— **建议在 Slice 2 phase 0 顺手修**
- T-16:`uniclipboard-cli lock` / `unlock` 命令—— **❌ 2026-04-20 决策不做**(CLI 进程短命,keyslot + keychain 本就长期持有,`lock` 无业务价值)
- T-17:legacy profile 迁移—— **❌ 2026-04-20 决策不做**(项目尚无真实用户,不需要向后兼容)
- `SpaceSetupFacade` 保留的 `A1/A2/B1/B2/F2` 对外表面稳定,Slice 2 只新增 `ClipboardSyncFacade`,不动 `SpaceSetupFacade`

**Slice 2 可复用的 Slice 1 基础设施**:
- `IrohNodeBuilder::install_pairing` 的扩展点模式 → Slice 2 加 `install_clipboard` 走同一个 endpoint
- `PairingOutcome` broadcast 模式 → Slice 2 的 clipboard 同步事件、presence 事件可以同款广播
- `build_slice1_cli_context` → Slice 2 的 CLI 命令(`status` / `rename` / `revoke`)同样基于它
- `MemberRepositoryPort` + `TrustedPeerRepositoryPort` 已填充真实数据 → Slice 2 的 `MemberRosterFacade::list_with_presence()` 读它们即可

**Slice 2 预研决策(2026-04-20 定稿)**:

- **D1 · facade 订阅通道**:`ClipboardSyncFacade::subscribe_inbound` / `subscribe_outbound_result` 走 `broadcast::Sender<InboundClipboardNotice>`(沿用 Slice 1 `PairingOutcome` pattern)
  - Notice 是小 struct(kind / size / sender / entry_id / at),**不**承载 raw payload。UI / CLI 订阅 Notice,再按需从 DB 拉全文
  - raw payload bytes 走内部单路管线:iroh stream → 解码 → 写系统剪贴板 + 写 DB → 发 Notice。不经 broadcast,没有 10MB fanout 问题
  - lagging drop 可接受(掉 Notice 不致命,UI 下次打开面板从 DB 全量拉)

- **D2 · F1 预连触发时机**:`A2 unlock` 成功 + `try_resume_session` 成功两条路径,都在 `StartNetworkUseCase` 完成后紧跟一步 `ensure_reachable_all(roster)`
  - Slice 2 验收条款"F1 启动后自动 ensure_reachable 全员,UI 即时反馈"已否决懒连
  - `on_startup` 在 locked 状态没密钥,连了白连 → 必须等解密上下文可用
  - 单次 dispatch 若目标尚未连上,由 dispatch 内部 `ensure_reachable(target)` 兜底(单 target,不扫全员)
  - N > 10 资源放大属 T-05 阈值懒连(P3),Slice 2 假设典型 N ≤ 10

- **D3 · `ClipboardFrame` wire 格式**:分层——`FrameHeader` 走 postcard,payload 走 raw iroh stream,**不**做 app 层分片
  - Header 结构:`{ version: u8, kind: PayloadKind, size: u32, sender: DeviceId, entry_id: EntryId, at: Timestamp }` ~50 bytes,postcard 序列化
  - Payload:reader 读完 header 拿到 `size`,`read_exact(size)` 流式读 N bytes(10MB 不整块 allocate 再 serialize)
  - 协议演进用 header 里的 `version: u8` + `#[non_exhaustive]` enum 兜着(Slice 1 `PairingSessionMessage` 已验证跑得通)
  - 工具链复用 `uc-infra/src/pairing/wire.rs` 的 postcard helper,新增 `uc-infra/src/clipboard/wire.rs` 对称放

---

### Slice 2 · 剪贴板同步 + 预连式 F1 🔲

**目标**:已配对设备之间**文本 / 小 payload 剪贴板**端到端同步;F1 启动自动预连全员;UI roster 实时反映在线状态。

**覆盖 usecase**:C1 outbound / C2 inbound(不含 C3 files)/ F1 完整版 / A3 revoke / A5 rename(被动传播)/ E1 roster / E2 presence events

**新建 port**(2)+ 扩展(2):
| Port | 类型 |
|---|---|
| `ClipboardDispatchPort` | 🆕 |
| `ClipboardReceiverPort` | 🆕 |
| `PresencePort::ensure_reachable` | 扩展 |
| `PeerAddressRepositoryPort` | 🆕 完整实现(Slice 1 骨架激活) |

**废弃的既有代码**(不删,Slice 5 统一删):
- `ClipboardOutboundTransportPort` / `ClipboardInboundTransportPort`(帧模型)标 `#[deprecated]`
- 旧 `ClipboardMessage` JSON 外壳(iroh 新 wire 用 header + V3 binary payload)

**对外表面(Facade + UI/IPC/CLI)**:

| 层 | 动作 |
|---|---|
| **`ClipboardSyncFacade`** 🆕 | `dispatch_current_entry() → DispatchResult` / `subscribe_inbound() → Stream<InboundClipboardNotice>` / `subscribe_outbound_result() → Stream<DispatchResult>` |
| **`MemberRosterFacade`** 🆕 | `list_with_presence() → Vec<RosterEntry>` / `subscribe_presence_events() → Stream<PresenceEvent>` / `rename_local_device(new_name)` / `revoke_member(id)` |
| **Tauri commands** | `clipboard_sync_events`(事件订阅)/ `roster_list` / `roster_presence_events` / `device_rename` / `member_revoke` |
| **前端页面** | 设备列表页(roster + 实时 presence)/ 设置页"改本机名"/ 设备详情"移除" |
| **Daemon IPC** | 订阅事件:clipboard 同步事件 / presence 事件 / 设备列表 |
| **CLI** | `uc status`(显示成员 + presence)/ `uc rename <name>` / `uc revoke <id>` |
| **Bootstrap** | F1 启动链接上 `NetworkControlPort::start_network`(在 `space_unlock` 成功后触发) |

**验收**:
- [ ] 两设备端到端文本同步(< 10KB payload)< 1s
- [ ] 图片 / HTML 富文本 同步(< 10MB)
- [ ] F1 启动后自动 ensure_reachable 全员,UI 即时反馈
- [ ] A3 revoke 后被撤销设备尝试连入被拒(connection deny)
- [ ] A5 rename 后下次同步,对端 SpaceMember 名字更新

#### Slice 2 Phase 1 · roster + presence 基础设施 ✅(2026-04-22)

**范围**:只做"谁在线"这件事变活——roster 查询 + presence 事件 + F1 预连 hook。**不做**剪贴板同步,**不接** rename / revoke UI 按钮,**不写**新 wire 协议。

**交付**(全部合入 `slender-soybean` 分支):
- `PresencePort` 新 port + `IrohPresenceAdapter`(T3a/T3b `36fc7e3b` / `5c69b2a6`):`Connection::closed()` watchdog 替代原计划的 `conn_type` Watcher —— T3a 探针发现 `conn_type` 是缓存不可靠
- `PeerAddressRepositoryPort` 完整实现(T1 `2ec1cabd` / T2 `e81cec97`):core port + Diesel adapter + migration `2026-04-20-000002_create_peer_address`
- wire 对称扩展(T5 `a562e529`):`JoinerRequest` / `SponsorConfirm` 加 `transport_address_blob: Vec<u8>`,`WIRE_VERSION` → 2
- `EnsureReachableAllUseCase`(T6 `e66776f8`):pub(crate),`JoinSet` 并发 + `peer_addr_repo.list()` 迭代源 + 本机防御性 filter
- `MemberRosterFacade`(T7 `548b3bdf`):`list_with_presence()` + `subscribe_presence_events()`;thin wrapper 不拨号
- F1 预连 hook(T8 `f461a6eb`):`SpaceSetupFacade::auto_start_network` 成功后 unconditional 跑 `ensure_reachable_all`;失败 warn 不传播
- `IrohNodeBuilder::install_presence` 扩展点(T4 `32a02c62`)+ bootstrap `MemberRosterFacade` 装配(T9 `181f2cc8`)
- `uniclipboard-cli members` 子命令(T10 `bda7686b`):自包含直连模式,`facade.refresh_presence()` + `roster.list_with_presence()` + human / JSON 双渲染
- 集成测试 `slice2_phase1_presence_e2e` 两例(T11 `d39889e0`):pair → 双向 online;关 joiner → sponsor ≤ 10s 观察到非 Online

**验收达成**:
- [x] 两台设备 unlock 后 `uniclipboard-cli members` 列出 SpaceMember + online / offline(手动验证 2026-04-22)
- [x] 关闭 B → A 的 `members` 命令 ≤ 10s 反映 offline(`slice2_phase1_presence_e2e::joiner_shutdown_flips_sponsor_roster_to_offline_within_10s` 自动覆盖)
- [x] B 重启 + unlock → A 的 `members` ≤ 10s 反映 online(手动覆盖;loopback-only 自动化受 `disable_relays=true` 限制无法模拟,rationale 见 T11 in-file comment)
- [x] 单测覆盖 `MemberRosterFacade`(T7:8 tests with fake `FakePresence`)+ `ensure_reachable_all` 并发(T6:6 tests 含 `SleepyPresence` 手写 fake 绕过 mockall Mutex 序列化)
- [x] 子命令改名 `members`(原计划 `status` 被 Slice 1 legacy daemon HTTP 状态命令占用)

**后续 follow-up(非 Phase 1 scope,记录供 Phase 2/3)**:
- **B2 不 save self 为 SpaceMember**(T11 暴露):`RedeemPairingInvitationUseCase::persist` 只 admit sponsor,joiner 视角下 `members` 看不到自己。修复需在 persist 收尾加一步 save self;补完后 T11 的 `joiner_roster.len() == 1` 断言需改为 `== 2`(作为契约变更信号,已在测试注释里标记)。
- **T12 e2e shell 扩展**(plan §1.1 验收点 3 的第二条覆盖):故意跳过,shell e2e 本质是演示脚本,Rust 集成测试已覆盖回归面。需要时再补。

**跳过的任务**:
- T12 `single-machine-e2e.sh` 扩展:shell 脚本维护成本 > 回归保护价值;Rust 集成测试已给等价覆盖

---

#### Slice 2 Phase 2 · 剪贴板同步(text-only,CLI-only)✅(2026-04-22)

**范围**:把"A 复制 → B 收到"这件事在 iroh 栈跑通;CLI 提供 `send` / `watch` 两条命令完成端到端验收。**不含**系统剪贴板写入(daemon 侧)、A3 revoke / A5 rename UI、blob / 文件传输。

**交付**(全部合入 `slender-soybean` 分支):
- **uc-core port**(T1 `0edb7479`):`ClipboardDispatchPort` / `ClipboardReceiverPort` / `ClipboardHeader` / `SyncPayload` / `DispatchAck` / `ClipboardDispatchError` / `InboundClipboard`;legacy `ClipboardOutboundTransportPort` / `ClipboardInboundTransportPort` 加 `#[deprecated(since="Slice2-Phase2")]`(Phase 2 双栈并行,Slice 5 删除)
- **iroh identity probe**(T2 `5a9ea34f`):3 verdict 探针确认 `iroh::EndpointId == iroh_base::PublicKey`(32-byte Ed25519),`Connection::remote_id().as_bytes()` 与 `SecretKey::public().as_bytes()` 字节等价 → 复用现有 `IdentityFingerprintFactoryPort::from_public_key(&[u8])`,**无需扩 port**
- **wire codec**(T3 `b2206e33`):`clipboard_wire.rs` 7 单测;frame format `[magic=0xC1 \| header_len_be(4) \| header(postcard) \| payload_len_be(4) \| ciphertext]` + 1-byte ack 反向流;`MAX_HEADER_SIZE=4KiB` / `MAX_PAYLOAD_SIZE=2MiB` / `AckCode { Accepted=0x01, DuplicateIgnored=0x02, Rejected=0xFF }`
- **dispatch adapter**(T4 `ae5b8202`):`IrohClipboardDispatchAdapter` impls `ClipboardDispatchPort`;`CLIPBOARD_ALPN = b"uniclipboard/clipboard/0"`;按 `peer_addr_repo.get → postcard-decode EndpointAddr → endpoint.connect(CLIPBOARD_ALPN) → open_bi → write_frame → read 1-byte ack` 链路走;`Offline` / `Io` / `PeerRejected` 错误折叠
- **receiver adapter**(T5 `63330895`):`IrohClipboardReceiverAdapter` 持广播 Sender + `IrohClipboardReceiverHandler` ProtocolHandler 装在 router 上;identity 反查靠 `Connection::remote_id().as_bytes()` → `IdentityFingerprintFactoryPort` → `member_repo.list().scan` → `DeviceId`;**关键 bug 修**:handler 返回时 `Connection` 被 drop 导致 ack byte 来不及 flush,加 `connection.closed().await` 保活
- **install_clipboard 扩展点**(T6 `c500ae62`):`IrohNodeBuilder::install_clipboard(peer_addr_repo, member_repo, fingerprint_factory) -> ClipboardHandlers`;3 ALPN(pairing + presence + clipboard)同 router 共存测试覆盖
- **DispatchClipboardEntryUseCase**(T7 `896e371b` + `e134247c` mockall 重写):`pub(crate)`;输入 `(plaintext: Bytes, content_hash: String, payload_version: u8)`;流程 `cipher.encrypt → peer_addr_repo.list → filter self + Online → JoinSet 并发 fan-out`;5 单测全 mockall(`.with(eq(DeviceId))` per-target 路由)
- **IngestInboundClipboardUseCase**(T8 `57ab9e65` + `e134247c` mockall 重写):`pub(crate)`;subscribe receiver broadcast → decrypt → 重 broadcast `InboundClipboardNotice { from_device, content_hash, plaintext, action, at_ms }`;`IngestSpawnHandle` Drop 自动 abort;Phase 2 不持久化 + 不 dedup;4 单测(mockall + `FakeReceiver` 手写)
- **ClipboardSyncFacade**(T9 `5b49d0ca` + `e134247c` mockall 重写):公开入口,3 方法(`dispatch_entry` / `subscribe_inbound_notices` / `spawn_ingest_loop`);完整 public ↔ internal 类型映射(7 对 + `From<DispatchSyncError>`),保证 §11.4 内部类型不外泄;3 单测
- **bootstrap 装配**(T10 `d4849971`):`SpaceSetupAssembly` 加 `pub clipboard_sync: Arc<ClipboardSyncFacade>` + 私有 `ingest_handle: IngestHandle`(parallel to `roster`);`build_space_setup_assembly` 在 `install_presence` 之后调 `install_clipboard`、构造 facade、起 ingest loop;`shutdown` 显式 abort ingest 走在 router 关之前;`uc_infra::network::iroh` 顶层导出 `ClipboardHandlers`
- **uc-cli 命令**(T11 `5d7622ed`):
  - `uniclipboard-cli send [TEXT]` — positional 或 stdin → resume → refresh_presence → `dispatch_entry`,human + JSON 双输出,non-zero exit when nothing landed
  - `uniclipboard-cli watch` — `subscribe_inbound_notices` 循环 + Ctrl-C 退出,JSON 模式 line-delimited
  - **决策**:不读系统剪贴板(`UC_DISABLE_SYSTEM_CLIPBOARD=1` 让 clipboard-rs 在 non-bundled CLI 上不会 panic),plaintext 改 CLI arg / stdin;daemon 改装到 iroh 时再开 OS clipboard 路径
- **集成测试**(T12 `734d52fe`):`slice2_phase2_clipboard_e2e` 两 verdict —— `sponsor_dispatch_lands_on_joiner_within_2s`(plaintext + content_hash 字节级 round-trip 通过 V3 chunked AEAD)+ `repeat_dispatch_lands_twice_phase2_no_dedup`(pin Phase 2 不 dedup 的当前事实,Phase 3 持久化时 flip)
- **文档**(T14 `<本提交>`):本节标 ✅;`slice2-phase2-plan.md §15` tracker 全部封版

**验收达成**:
- [x] A 复制文字 → B 在 ≤ 2s 内收到相同内容 + 匹配 content_hash(自动 e2e `sponsor_dispatch_lands_on_joiner_within_2s`,5s ceiling 含 CI 抖动)
- [x] CLI `uniclipboard-cli send <TEXT>` / `watch` 走通(单元 + e2e + smoke `--help` / resume guard)
- [x] 重复内容第二次 dispatch 仍 Accepted(Phase 2 receiver 不 dedup,wire 有编码无生产者;`repeat_dispatch_lands_twice_phase2_no_dedup` pin)
- [x] 离线对端不 panic(`DispatchPerTarget.outcome = Err("Offline")`,Phase 2 dispatch 错误折叠覆盖)
- [x] 单测覆盖 3 usecase + 1 facade + 4 adapter(总 ~29 单测 + 5 e2e + 3 probe)

**跳过的任务**:
- **T13 手动双 profile 验收 ⏭️ 战略跳过**:沿用 Phase 1 T12 战略跳过决策。Rust 集成测试已等价覆盖 pair → dispatch → receive 全路径(real iroh loopback transport,3 ALPN 同 router,V3 chunked AEAD round-trip,接收时序 ≤ 5s),CLI plaintext pipeline 不读系统剪贴板没有 OS-side variance 需要手动验证。手动 recipe 留在 `slice2-phase2-plan.md §9.3` 供需要时使用

**后续 follow-up(非 Phase 2 scope,记录供 Phase 3+)**:
- **daemon clipboard watcher 改装到 iroh**(Phase 3):`uc-app::sync_outbound` / `uc-daemon::workers::inbound_clipboard_sync` 改 wire 到 `ClipboardSyncFacade`,完成后 Slice 5 才能删 deprecated transport ports
- **receiver-side dedup + 持久化**(Phase 3):ingest 接 `ClipboardEventWriter.insert_event` + content_hash 去重 → emit wire `DuplicateIgnored`,同时 flip `repeat_dispatch_lands_twice_phase2_no_dedup` 验收
- **B2 不 save self 为 SpaceMember**(继承自 Phase 1):修复后 phase2 e2e 可加 B→A 双向断言
- **e2e harness 抽 `tests/common`**:slice1 + slice2-phase1 + slice2-phase2 三份重复,Phase 3 出第四份前可统一抽取

---

#### Slice 2 Phase 3 · daemon 接管 iroh 剪贴板同步 ✅(2026-04-23)

**范围**:把 Phase 2 的 `ClipboardSyncFacade` 接到 daemon `ClipboardWatcherWorker` / `InboundClipboardSyncWorker`,完成"系统剪贴板复制 → 自动 dispatch → 对端落库 + 写系统剪贴板"闭环;wire payload V3 envelope 全链路落地;CLI `send` / `watch` 降为可选验收工具。**不含**per-member sync preferences(D3 推 Phase 3.5)、wire `DuplicateIgnored` ack(D4 推 Phase 3.5)、A3 revoke / A5 rename UI、大 payload(Slice 3 blob)、daemon clipboard 路径之外的 libp2p 退役(Slice 5)。

**交付**(全部合入 `slender-soybean` 分支):
- **usecase crate 迁移**(T0a `cb4ac588` / T0b `ad5ac7ac`):`CaptureClipboardUseCase` + `ClipboardWriteCoordinator` 从 `uc-app` 迁到 `uc-application/src/usecases/clipboard_capture/` 与 `clipboard_write/`;老路径留 `#[deprecated]` re-export shim,Slice 5 删
- **dedup port 方法**(T1 `9ce27893`):`ClipboardEntryRepositoryPort::find_entry_id_by_snapshot_hash(&str) -> Option<EntryId>`,Diesel 两步查询(event → entry)避开 JoinDsl 冲突;2 单测
- **payload V3 envelope codec**(T2 `68f89b31`):`uc-application/src/usecases/clipboard_sync/payload_codec.rs` — `encode_snapshot_to_v3_bytes(snapshot) -> (Bytes, content_hash)` + `decode_v3_bytes_to_snapshot(bytes) -> SystemClipboardSnapshot`;content_hash 走 `snapshot_hash()` 与本地 `clipboard_event.snapshot_hash` 列对齐;4 单测
- **facade 扩展**(T3 `de2c8da6`):`ClipboardSyncFacade::dispatch_snapshot(snapshot, origin)` — 内部 encode V3 envelope + 调 `dispatch_entry(payload_version=3)`;保留 `dispatch_entry` 兼容 CLI Phase 2 路径;1 新 mockall verdict
- **`ApplyInboundClipboardUseCase`**(T4 `84129746`):6 mockall 单测(dedup miss/hit / decode failure / capture failure / write failure / dedup-query failure),`InboundCapture` + `InboundWrite` 两 internal traits 把 7+2 port 依赖外包给 blanket impl,测试用 mockall 桩
- **daemon 装配**(T5 `19595e06`):`DaemonBootstrapContext` 加 `clipboard_sync_facade: Arc<ClipboardSyncFacade>` + `space_setup_assembly: SpaceSetupAssembly` 两 pub 字段;`build_daemon_app` 内 `block_on` 块新增第三个 future 跑 `build_space_setup_assembly`
- **daemon workers 改装**(T6+T7+T8 `8e007150`):`entrypoint.rs` 注入 `clipboard_sync_facade` + 新构造的 `ApplyInboundClipboardUseCase`(shared `ClipboardWriteCoordinator` Arc 维 origin guard 单例);`DaemonClipboardChangeHandler` 删 `build_sync_outbound_clipboard_use_case`,dispatch arm 改走 `clipboard_sync.dispatch_snapshot`;`InboundClipboardSyncWorker` 订阅源切 `subscribe_inbound_notices`,处理改走 `ApplyInboundClipboardUseCase.execute`,`parse_clipboard_frame` 整段删;shutdown 路径加 `assembly.shutdown().await`
- **CLI envelope 升级**(T9+T10 `8e075213`):`send` 把 text wrap 成 single-rep `SystemClipboardSnapshot` 走 `dispatch_snapshot`(删 sha2 / bytes dep);`watch` 用 `decode_v3_bytes_to_snapshot` 展开 envelope,first `text/*` rep 优先渲染,JSON schema `plaintext_utf8` → `text` + 新 `rep_summary`
- **Phase 2 e2e 迁移**(T11 `f8f2079c`):两 verdict 切 `dispatch_snapshot` + decode 校验,content_hash 改断 `snapshot_hash()` canonical
- **shell e2e schema 更新**(T13 `416346f2`):`scripts/test_clipboard_e2e.sh` `"plaintext_utf8"` → `"text"` 全局替换 9 处
- **§11.4 合规性修复**(`dec6f5fb` / `5eb1bb2c` — T14 真机验收 byproduct):`SetupStatus` 访问从 daemon / CLI 多处散落 → 全收敛到 `SpaceSetupFacade::is_setup_complete()` / `facade/` 子模块,迁 `IsSetupCompleteUseCase` pub(crate)
- **iroh / pairing 地址持久化修**(真机验收期间发现,见 `slice2-phase3-plan.md §15.5`):
  - `90048909` 共享 daemon long-lived runtime,iroh stack 活到 `daemon.run()` 返回
  - `dbaa5cbd` outbound dispatch 删 `Online` pre-filter,全量 iterate `peer_addr_repo`;失败折叠为 `Offline`
  - `67e6cb3a` `ApplyInboundClipboardUseCase` 在 `ClipboardWriteCoordinator.write` 前调 `narrow_to_primary` 收敛 paste-priority rep
  - `21716a02` iroh connect 前过滤 stored addr 中的 stale `DirectIp(...)` 条目
  - `9e65ab73` pairing 序列化 NodeAddr 时丢弃 ephemeral `Ip(...)`,只 persist `NodeId + relay`
- **文档收尾**(T15 `<本提交>`):本节标 ✅;`slice2-phase3-plan.md §15` tracker 全部封版(T12 战略跳过 + T14 真机验收追记);progress.md 续 29 记录

**验收达成**:
- [x] A daemon + B daemon 跑着,A 用户复制文字 → ≤ 2s 内 B 系统剪贴板被覆盖,内容字节级相等(真机验收 T14)
- [x] 双端 daemon 都把 entry 落 `ClipboardEntry` 库,B 侧 `clipboard.new_content` WS 事件 `origin: "remote"` fire 一次(真机验收)
- [x] B 端 daemon 不因自己刚写系统剪贴板就反向 dispatch(`ClipboardChangeOriginPort` `RemotePush` guard 在 `ClipboardWriteCoordinator.write` 前注册,watcher 消费时跳过)
- [x] A 复制同一文字两次 → B 系统剪贴板仍只被写一次,DB 也只多 1 条 entry(`ApplyInboundClipboardUseCase` 走 `find_entry_id_by_snapshot_hash` → `DuplicateSkipped`,不写 OS / 不 emit WS / 不 broadcast)
- [x] B daemon 离线时 A 复制 → A daemon log `0 accepted, 1 offline`,不 panic;B 重启后下一次 A 复制 ≤ 10s 收到(T14 手动覆盖,dispatch 失败折叠 `Err("Offline")`)
- [x] CLI `send` / `watch` 仍可工作(T9/T10 升级后同 daemon 共享 envelope codec)
- [x] 单测 / 集成测试:T1–T4 application 层累计 13 单测 + T11 迁移后 phase 2 e2e 2 verdict + shell e2e `bash -n` 通过

**跳过的任务**:
- **T12 `slice2_phase3_daemon_e2e` 战略跳过**:沿用 Phase 1 T12 / Phase 2 T13 先例。`slice2_phase2_clipboard_e2e`(T11 envelope 迁移后)已覆盖 real-iroh transport + V3 envelope encode/decode + broadcast + cipher AEAD round-trip 完整 application 层;新写 daemon-process-level e2e 需要拉两个真实 daemon process + mock 系统剪贴板 + 绕 `UC_DISABLE_SYSTEM_CLIPBOARD`,工程成本 ~2.5h 的增量覆盖(process lifecycle + OS clipboard IO)由 T14 人工验收 ~1h 覆盖

**后续 follow-up(非 Phase 3 scope)**:
- **图像同步 known-issue**(不开 follow-up,仅记录):`narrow_to_primary` 在 snapshot 含 `FileList + Image` 双 rep 时优先输出 `FileList`,图片语义退化为文件列表。Phase 3 scope 外,**当前不处理**(2026-04-23 决策);见 `slice2-phase3-plan.md §15.4` 第一条与 `9ebb03be` chore commit 日志
- **per-member sync preferences**(D3 deferred):`MemberSyncPreferences.send_enabled` / `send_content_types` 整链失效,只保 global master toggle;Phase 3.5 做(需新增 `dispatch_entry_to_targets` 或 `target_filter`)
- **wire `DuplicateIgnored` ack**(D4 deferred):Phase 3 dedup 只在 application 层;wire 层 `AckCode::DuplicateIgnored=0x02` 仍无生产者。Phase 3.5 flip `repeat_dispatch_lands_twice_phase2_no_dedup` 验收
- **deprecated transport ports 清理**:Phase 3 完工 = daemon 内 0 消费者;`uc-app::sync_outbound` / `uc-app::sync_inbound` 仍 impl(deprecated 活着)。Slice 5 删
- **A3 revoke / A5 rename UI**:推 Phase 4 或 Slice 6
- **大 payload(图片 / 富文本 / 文件)**:Slice 3 blob ticket 路径;`MAX_PAYLOAD_SIZE=2MiB` 上限不变

---

### Slice 3 · 文件 / Blob 🔲

**目标**:含文件的剪贴板端到端;明文 hash 去重生效(重复复制同一文件不产生重复密文)。

**覆盖 usecase**:C3 with-files / D1 publish / D2 fetch

**新建 port**(2):
| Port | 类型 |
|---|---|
| `BlobTransferPort` | 🆕(publish / fetch / has / tag / untag / issue_ticket) |
| `BlobReferenceRepositoryPort` | 🆕(明文 hash → digest 去重缓存) |

**对外表面(Facade + UI/IPC/CLI)**:

| 层 | 动作 |
|---|---|
| **Facade** | **通常无需新增**——Blob 由 `ClipboardSyncFacade` 内部使用,不直接暴露给 UI |
| **`BlobProgressFacade`**(可选,技术债 T-01) | 若要显示进度条:`subscribe_blob_progress() → Stream<BlobProgressEvent>` |
| **Tauri commands** | 扩展 `clipboard_sync_events` 事件 payload 带文件下载状态;不新增命令 |
| **前端页面** | 剪贴板历史项展示"文件传输中 / 已完成",若做 T-01 则加进度条 |
| **Daemon IPC** | 无新增(blob 仅作为 clipboard 内容的一部分) |
| **CLI** | `uniclipboard-cli blob publish <file>` / `uniclipboard-cli blob fetch <ticket> --entry-id <id> --out <file>` 作为长期诊断命令;剪贴板级 `copy/paste` 留到 Phase 3 |
| **Bootstrap** | 装配 `BlobTransferPort` / `BlobReferenceRepositoryPort` 的 adapter;FsStore 目录创建 |

**验收**:
- [ ] 复制文件 → 粘贴到另一台 → 文件内容一致(BLAKE3 校验)
- [ ] 重复复制同一文件 10 次 → 本地密文只存 1 份(去重生效)
- [ ] 大文件(1GB)断点续传(iroh-blobs 原生能力)
- [ ] 一对多 fanout:同一 ticket 被多接收方并发拉取

**阻塞**:Slice 2 完成(已于 2026-04-23 完成)

**Phase 拆分**(参照 Slice 2 成熟节奏,2026-04-24 敲定):

| Phase | 范围 | 验收重心 |
|---|---|---|
| Phase 1 · Blob 基础设施 ✅ | 2 个新 port + iroh-blobs FsStore adapter + `blob_reference` Diesel 表 + bootstrap 装配 | adapter 单元测试(含自回环 publish/fetch);**不接** usecase/CLI/剪贴板 |
| Phase 2 · D1/D2 usecase + CLI-only e2e ✅ | `PublishBlobUseCase` / `FetchBlobUseCase` + `uniclipboard-cli blob publish/fetch`(长期命令) | application test:重复 publish 10 次只存 1 份密文;fetch 后登记去重缓存;CLI local round-trip 字节一致 |
| Phase 3 · C3 剪贴板含文件端到端 🟡 | V3 envelope 兼容扩展 + daemon dispatch / apply 分支 + blob cache 写本机缓存目录 | 已完成代码接线与单元/编译验证;剩余真机两台 `cli start` 后复制文件 → 另一台粘贴字节一致 |

**跨 Phase 决策锁定**(2026-04-24):

| # | 决策 | 理由 |
|---|---|---|
| S3-D1 | Phase 1/2/3 三段拆分(不合并 Phase 2 到 Phase 3) | 沿用 Slice 2 成熟节奏;先 CLI 闭环再接剪贴板,降低调试半径 |
| S3-D2 | V3 envelope 走**兼容扩展**(新字段 `Option<Vec<BlobTicket>>`),不 bump V4 | `postcard` 结尾追加 `Option` 字段对旧 decoder 透明;避免 wire version 矩阵 |
| S3-D3 | Blob cache 落盘走**临时目录**,路径由调用方决定 | 不引入"blob cache 生命周期"新 domain;C3 落地时由 usecase 返回路径,调用方自行处置 |
| S3-D4 | `uniclipboard-cli blob publish` / `blob fetch` 为**长期命令**,Slice 5 不删 | 用户侧长期验收工具;daemon 路径之外的 blob 直用场景(脚本 / 自动化) |

**阻塞**:无(可开工)

**Phase 1 完成记录(2026-04-24)**:
- `BlobTransferPort` / `BlobReferenceRepositoryPort` 已进入 core;infra 已实现 iroh-blobs adapter 与 sqlite 去重缓存。
- `IrohBlobTransferAdapter` 支持 publish / has / issue_ticket / digest_of / fetch / tag / untag,并覆盖 9 个单测(self-fetch + 双节点 fetch + tag 幂等)。
- `IrohNodeBuilder::install_blobs` 已把 iroh-blobs 挂入共享 router;pairing / presence / clipboard / blobs 四个 ALPN 共存测试通过。
- `SpaceSetupAssembly` 已暴露 `blob_transfer` / `blob_reference`,Phase 2 usecase 可直接接入。
- 验证:`cargo test -p uc-infra` 通过;`cargo check --workspace` 通过。

**Phase 2 完成记录(2026-04-24)**:
- `uc-application` 新增 `PublishBlobUseCase` / `FetchBlobUseCase` 与 `BlobTransferFacade`。
- `SpaceSetupAssembly` 新增 `blob` 门面,CLI 不直接调用 use case。
- `uniclipboard-cli blob publish <file>` 输出 `ticket` + `entry_id`;`blob fetch <ticket> --entry-id <id> --out <file>` 拉取内容并按这个 `entry_id` 登记归属。
- 重要约束:`ticket` 定位内容,`EntryId` 用于 fetch 后登记本次剪贴板归属;CLI publish 仍输出二者,fetch 仍同时输入二者。
- 验证:`cargo test -p uc-application blob_transfer --lib` 通过;`cargo check -p uc-cli` 通过;临时 `--dev --profile` 下真实执行 init → publish → fetch → `cmp` 字节一致;`cargo check --workspace` 通过。
- Phase 2 未承诺跨进程远端供给:CLI publish 退出后 provider 不再常驻,远端/并发 fanout 继续由 Phase 3 daemon/剪贴板路径或专门长驻测试覆盖。

**Phase 3 接线记录(2026-04-24)**:
- V3 payload 已能附带 `V3BlobRef`,旧 decoder 仍能读取普通 snapshot,新 decoder 能读出 blob 引用。
- 发送侧 daemon 剪贴板监听器遇到文件列表时,先把本机文件发布成 blob,再随剪贴板消息发出;不再走旧的文件传输分支。
- 接收侧 `ApplyInboundClipboardUseCase` 会先拉取 blob 到本机 `file_cache_dir/iroh-blobs/...`,再把剪贴板里的文件路径改成本机 `file://` 路径,最后落库并写系统剪贴板。
- `cli start` 启动的 daemon 已装配同一套发送/接收路径:出站使用 `BlobTransferFacade`,入站使用 `FileCacheBlobMaterializer`。
- 验证: `cargo test -p uc-application apply_inbound --lib` 通过;`cargo test -p uc-daemon --lib` 通过;`cargo check -p uc-application` / `cargo check -p uc-daemon` / `cargo check -p uc-cli` 通过。
- 未完成:尚未跑两台真机或双 profile 的真实 OS 剪贴板文件复制粘贴验收;Phase 3 最终验收仍保留该项。

---

### Slice 4 · 删除 libp2p 业务代码 🔲(2026-04-24 重写;2026-04-24 二次扩张:含 setup 流程 setupFacade 迁移 + 前端 join space UI 重设计)

**目标**:把 libp2p 业务代码、旧 ports、旧 wire 协议、死代码 usecase 一次性清干净;daemon peer worker 切到 `PresencePort`;**daemon HTTP `/setup/*` 从老 stateful FSM 迁移到新 stateless `SpaceSetupFacade`**;**前端 join space 流程从"扫描+点选+确认"重设计为"输入邀请码+口令"**;`iroh` Cargo feature 一并取消,iroh 成为唯一实现。

**硬验收**:**通过 GUI 完成两台真机配对**(sponsor 创建空间 + 出码,joiner 输入码 + 口令,两端均落库 SpaceMember + TrustedPeer)。

**为什么取消原"双栈并行验证"**(决策记录,2026-04-24):
- 证据:`findings.md` F-100 显示 GUI 进程的 `sync_outbound_clipboard()` 工厂**零调用方**,uc-app 内部 0 处引用旧 sync usecase——v0.4.0 daemon-first 完成后,libp2p 业务路径在 GUI 进程已是空跑死代码
- daemon 路径 Slice 1/2/3 已端到端跑通三个切片(pairing / 剪贴板 / blob);双栈并行 1-2 周拿不到额外可验证场景
- 双栈意味着维护 `#[cfg(feature = "iroh")]` 条件装配 + CI 矩阵,工程包袱大于收益
- 用户偏好"先删后优化",降低架构腰带的过渡时长

**前置准备**(已坐实,见 `findings.md`):
- F-100:GUI sync 路径已是空跑死代码,删除零功能损失
- F-102:7 个旧 port 中 4 个完全有 iroh 替代(`ClipboardOutboundTransportPort` / `ClipboardInboundTransportPort` / `FileTransportPort` / `ConnectionPolicyResolverPort`),3 个需要切换 consumer(`PairingTransportPort` / `NetworkEventPort` / `DiscoveryPort`),`NetworkControlPort` 保留不动
- F-113:DB schema 已无 `peer_id` 列,**无需新 migration**
- F-114:`PresencePort::subscribe()` 接口已就绪,daemon peer worker 替换是新代码但工程量小

**Phase 拆分**:

| Phase | 范围 | 验收重心 |
|---|---|---|
| Phase 1 · daemon peer worker 切换 ✅(2026-04-24) | 新增 `uc-daemon/src/peers/presence_monitor.rs`(基于 `PresencePort::subscribe()`),删除旧 `monitor.rs` | daemon 在每次 `PresenceEvent::Online/Offline` 后向 ws `peers` topic 推一条 `peers.changed` 全量快照(已用单测覆盖,真机两台验收挪到 Slice 4 整体收尾) |
| Phase 2 · 应用层 consumer 切换(部分)✅(2026-04-24) | `uc-application/setup/{orchestrator,action_executor,facade,testing}.rs` 拿掉 `Arc<dyn DiscoveryPort>` 参数;`uc-bootstrap/src/assembly.rs` 同步删除 discovery 装配。**B 部分(network_adapter envelope 迁移)经 F-116/F-117 重评后并入 Phase 3** | `cargo check --workspace` 通过;Slice 1/2/3 e2e 全绿 |
| **Phase 3 · daemon HTTP setup 迁移到新 facade**(新增,~2 天) | daemon `/setup/*` 11 个 HTTP endpoint 从老 `SetupFacade`(stateful FSM)迁移到新 `SpaceSetupFacade`(stateless commands);新增 daemon ws 事件投影 setup 进度;daemon api/server 装配链同步切换;**老 `SetupFacade` + `SpaceAccessOrchestrator` FSM + `SpaceAccessNetworkAdapter` + `SpaceAccessTransportPort` 删除** | daemon 进程不再依赖 `PairingTransportPort`;`PairingRuntimeOwner::CurrentProcess` 装配点能直接换成无 libp2p 模式;daemon 单元/集成测试全绿 |
| **Phase 4 · 前端 join space UI 重新设计**(新增,~2-3 天) | `src/api/daemon/setup.ts` 重写为新 endpoint 调用;`src/pages/SetupPage.tsx` + `src/hooks/useSetupFlow.ts` 适配新状态投影;**join 流程重设计**:删 `JoinPickDeviceStep` + `useDeviceDiscovery`(libp2p mDNS 残留),改为单一 "输入邀请码 + 口令" 步;sponsor 流程加 **`SponsorInviteStep`**(展示邀请码 + 倒计时,订阅 ws `pairing.completed`) | 前端单测 / 组件测试全绿;两台真机本地局域网 GUI 配对成功 |
| **Phase 5 · 整体删除 libp2p**(原 Phase 3) | 按 F-111 删除清单 14 个目录/文件级 `rm`;`mod.rs` / `lib.rs` / `deps.rs` / `Cargo.toml` 同步清理 | `rg -w libp2p src-tauri/crates/uc-{core,app,application,platform,infra,bootstrap,daemon,tauri,cli}/src/` 0 命中(允许 logging filter 字面量) |
| **Phase 6 · 收尾 + 真机端到端验收**(原 Phase 4 + 验收上挪) | 移除 Cargo feature `iroh` 门控;清理 frozen 注释 / `#![allow(deprecated)]`;**两台真机 GUI 跑完整 setup 流程并完成剪贴板同步**(双 mac、mac+win 之一,本地局域网 + 跨网络 relay 各一遍) | `grep -r "frozen libp2p\|allow(deprecated).*libp2p" src-tauri/` 0 命中;`Cargo.lock` 不再含 `libp2p*` 任何 crate;两台真机 GUI 走完 setup → 剪贴板互通 |

---

#### 决策记录:Phase 2 B → Phase 3 P3-pre(2026-04-24)

**原计划 B**:`uc-application/src/space_access/network_adapter.rs` 把 `PairingTransportPort` 调用替换为 `PairingSessionPort` + `PairingEventPort`。

**重新评估后推迟,理由**:
1. **`PairingSessionMessage` 没有 envelope 变体**——只有 5 个 pairing 握手专用变体(Request/KeyslotOffer/ChallengeResponse/Confirm/Reject),没有 Busy 或通用 envelope。`SpaceAccessNetworkAdapter` 当前依赖 `PairingMessage::Busy.reason` 作为 JSON envelope 承载 space_access_offer/proof/result——直接换 port **不存在 1:1 替换路径**。
2. **iroh 栈下当前没有 `PairingTransportPort` 的活 impl**——唯一非占位 impl 是 `Libp2pNetworkAdapter`(`uc-platform/src/adapters/libp2p_network/mod.rs:825`),即所有 space-access 协议消息**目前还在走 libp2p stream**。Phase 2 范围内单独切 network_adapter 没有 iroh 落点。
3. **F-050 明文预言**:"`uc-application/src/space_access/network_adapter.rs` —— 桥接旧 pairing transport 到 space_access(Slice 5 换 iroh pairing session;**文件可能保留但内部重写**)"——这是 Slice 5 / Phase 3 工作,不是 Phase 2"应用层 consumer 切换"。
4. **Phase 2 验收条件已满足**——`cargo check --workspace` 通过 + Slice 1/2/3 e2e 全绿是 A 完成时的状态。
5. **强联动**:envelope 迁移和 libp2p adapter 删除必须在同一阶段做——adapter 没删之前 envelope 切走没收益,adapter 删之前 envelope 没切走 setup 整段断。所以挪进 Phase 3 作为**前置子任务 P3-pre**(必须先做完再删 libp2p adapter)。

**P3-pre 决策结果(2026-04-24)**:**方案 D · 删除整条 `SpaceAccessTransportPort` 套件**(替代之前列的 A/B)。

**依据**:研究表明 iroh path 已经完整覆盖 space-access 协议三段,字节级语义等价(详见 `findings.md` F-116):
- 旧 `space_access_offer` envelope ↔ `PairingSessionMessage::KeyslotOffer`(由 `sponsor_handshake.rs:222` 直发)
- 旧 `space_access_proof` envelope ↔ `PairingSessionMessage::ChallengeResponse { encrypted_challenge }`(`joiner_handshake.rs:247` 把 `proof.proof_bytes` 直接放入)
- 旧 `space_access_result` envelope ↔ `PairingSessionMessage::Confirm` / `Reject`(sponsor_handshake.rs:385/436)

`SpaceAccessNetworkAdapter` + `SpaceAccessTransportPort` + FSM `SendOffer/SendProof/SendResult` 三个 action 在 iroh-only 配置下都是 dead leg(只有 libp2p stack 的 PairingTransportPort impl 会真正发送),删除而非替换。

**P3-pre 实施清单**(详见 F-116):
1. 删 `uc-application/src/space_access/network_adapter.rs`(整文件)
2. 删 `uc-core/src/ports/space/transport.rs`(整文件)
3. 删 `SpaceAccessAction::SendOffer/SendProof/SendResult` 三变体(`uc-core/src/space_access/action.rs`)
4. 改 `state_machine.rs` 转移序列:去掉这三个 action(L56/L108/L126/L144/L352)——FSM 状态机本身保留
5. 改 `SpaceAccessExecutor`:删 `transport` 字段(executor.rs:7)
6. 改 `SpaceAccessOrchestrator::execute_actions`:删 SendXxx 分支(orchestrator.rs:442–520)
7. 改 setup `{orchestrator,action_executor,facade,testing}.rs`:移除 `transport_port: Arc<TokioMutex<dyn SpaceAccessTransportPort>>` 参数链
8. 改 `bootstrap/assembly.rs:1189–1194`:删 transport_port 装配 + `SpaceAccessNetworkAdapter::new` 调用
9. 删 daemon `pairing/host.rs:1436+` 的 `PairingMessage::Busy` envelope 解析段(随 libp2p adapter 整体删除一并清理)
10. 删 `parse_space_access_busy_payload` / `SpaceAccessBusyPayload` 等 helper

**入口风险**(进入 Phase 3 时必先验证):
- A1 路径(`initialize_new_space`)走 FSM 触发 `SendOffer`,但 A1 是单机动作没有 joiner。需先核实 iroh-only 配置下 A1 当前到底跑通了没——是否被 libp2p stack 兜底,或被某种 fake/short-circuit 跳过

**`task_plan.md:1180` 修正**:方案 D 落地后,`PairingMessage` / `PairingBusy` wire 类型一并删除,1180 行原文"保 `PairingMessage` / `PairingBusy`"自动失效——这两个 wire 类型不再有消费者。

---

#### A1 路径行为核实结论(2026-04-24,F-116 后续 + F-117)

**进程级路由表**(`uc-bootstrap/src/builders.rs:130–140`):

| 进程 | `PairingRuntimeOwner` | `PairingTransportPort` 实际 impl |
|---|---|---|
| GUI(Tauri) | `ExternalDaemon` | `DisabledPairingTransport`(报错) |
| CLI | `ExternalDaemon` | `DisabledPairingTransport`(报错) |
| **daemon** | **`CurrentProcess`** | **`Libp2pNetworkAdapter`(真发包)** |

**关键事实**:
1. e2e 测试 `slice1_handshake_e2e.rs` 用的是**新 `SpaceSetupFacade`**(`uc-application/src/facade/space_setup/facade.rs`)→ iroh-only 路径,不经过 `SpaceAccessNetworkAdapter` / FSM。这条路径**已验证通过**。
2. **老 `SetupFacade`**(`uc-application/src/setup/facade.rs`)仍被 daemon HTTP `/setup/*` 11 个 endpoint 使用(`uc-daemon/src/api/setup.rs`),走 FSM → `SpaceAccessNetworkAdapter` → `PairingTransportPort`。daemon 进程下 `PairingTransportPort` = `Libp2pNetworkAdapter`,所以这条路径**当前在 libp2p 上跑通**。
3. 前端(`src/api/daemon/setup.ts`)消费 daemon HTTP `/setup/*`——所以 GUI 用户的真实 setup 流程**仍在走老 facade + libp2p**。

**结论**:F-116 提出的"方案 D · 删除 SpaceAccessTransportPort 套件"**前提不成立**,因为老 `SetupFacade` + libp2p stack 仍是**生产路径**(daemon HTTP setup endpoints 的实现)。

**P3-pre 工作量重估(F-117)**:

要让 libp2p adapter 真正可删,必须**先迁移 daemon setup 流程从老 facade 到新 facade**:
- 老 `SetupFacade`:stateful FSM 风格(`new_space` / `submit_passphrase` / `cancel_setup` / `reset` / `confirm_peer_trust`)
- 新 `SpaceSetupFacade`:stateless commands 风格(`initialize_space` / `issue_pairing_invitation` / `redeem_pairing_invitation`)

迁移工作:
1. 核对 daemon HTTP `/setup/*` 11 个 endpoint 在新 facade 里如何表达——契约模型不同
2. 重写 daemon HTTP setup endpoints 为新 facade 调用,可能需要 stateful 状态投影层
3. 调整前端 `src/api/daemon/setup.ts` + React 组件契约
4. 删 `SetupFacade` + `SpaceAccessOrchestrator` FSM transport leg + `SpaceAccessNetworkAdapter` + `SpaceAccessTransportPort`

**修正预算**:1-3 天(从 F-116 估的 1-2h 大幅上修)

**Slice 4 范围决策(2026-04-24 拍板)**:**方向 1 · 扩大 Phase 3**——把 daemon setup HTTP 迁移并入,**额外把前端 join space UI 重设计纳入**(原决策表里是 daemon 后端工作,扩张后明确包含前端)。Slice 4 验收升级为**两台真机 GUI 完成配对**。

下面详细拆 Phase 3(daemon HTTP 迁移)+ Phase 4(前端重设计)的任务。

---

#### Phase 3 详细任务 · daemon HTTP setup 迁移到新 facade(~2 天)

**T3.1 · daemon ws 事件投影层**(新增 0.5 天)
- 新增 `uc-daemon-contract` ws event 类型:
  - `setup.invitation_issued { code: String, expires_at_ms: i64 }`
  - `setup.pairing_completed { sponsor_device_id: String, joiner_device_id: String, success: bool, reason: Option<String> }`(双侧均接收)
  - `setup.invitation_revoked { reason: String }`(超时 / cancel)
- daemon 侧 ws topic 名 `setup`(类比现有 `peers` / `space_access`),subscribe 协议复用
- 取代老 `SetupStateChangedEvent` ws 推送

##### T3.1 执行细分(2026-04-25 起,session 39)

**现状盘点**(2026-04-25)
- ✅ `ws_topic::SETUP = "setup"` 已存在(`uc-daemon-contract/src/constants.rs:11`),`is_supported_topic` 已收(`uc-daemon/src/api/ws.rs:447`),snapshot 返 `None`(`ws.rs:549`),subscribe 协议复用 OK
- ❌ 待新增三类事件常量 + payload DTO + daemon 侧广播接口
- 📌 老事件 `SETUP_STATE_CHANGED` / `SETUP_SPACE_ACCESS_COMPLETED` 仍被 daemon `event_emitter.rs:78,103` 发出、`uc-daemon-client/src/ws_bridge.rs:1119,1171` 消费、前端 `useDaemonEvents.ts` / `setup.ts` / `daemon/setup.ts` 订阅 —— **T3.1 不删**(留给 T3.4 + Phase 4 收尾),新事件并存

**决策记录**(2026-04-25 拍板)
- **D1 · camelCase**(已查前端 `src/hooks/useDaemonEvents.ts:60-95,378` 全用 camelCase):事件类型串 `setup.invitationIssued` / `setup.pairingCompleted` / `setup.invitationRevoked`;payload 字段 Rust 写 snake_case + `#[serde(rename_all = "camelCase")]` → 前端拿到 `code` / `expiresAtMs` / `sponsorDeviceId` / `joinerDeviceId` / `success` / `reason`
- **D2 · A 选项**:T3.1 含 daemon broadcaster helper(用户拍板 A,避免 T3.3 各调用方手写 wire 编码;且与"topic 接通"标题对齐)
- **D3 · 新建 `dto/setup_events.rs`**:3 个新事件 payload struct 单独成文件,不与 `dto/setup.rs` 内的老 `SetupStateResponse` / `SetupSelectPeerRequest` 等混(老的随 T3.4 整体删)
- **D4 · 老事件常量加 `#[deprecated]`**:`SETUP_STATE_CHANGED` / `SETUP_SPACE_ACCESS_COMPLETED` 标 deprecated;5 处现存调用点(`uc-daemon/src/api/event_emitter.rs:78,103`、`uc-daemon-client/src/ws_bridge.rs:1119,1171`、`uc-daemon/src/api/setup.rs:398`)各自加 `#[allow(deprecated)]` 压噪音;workspace 未配 `deny(warnings)` 已确认 → 不会编译失败

**子步骤**
1. **S1 · 常量注册**(uc-daemon-contract) — 在 `constants.rs::ws_event` 新增三条 const:`SETUP_INVITATION_ISSUED = "setup.invitationIssued"` / `SETUP_PAIRING_COMPLETED = "setup.pairingCompleted"` / `SETUP_INVITATION_REVOKED = "setup.invitationRevoked"`;同步给老 `SETUP_STATE_CHANGED` / `SETUP_SPACE_ACCESS_COMPLETED` 加 `#[deprecated(note = "removed in T3.4 — switch to setup.invitationIssued/pairingCompleted/invitationRevoked")]`
2. **S2 · DTO payload**(uc-daemon-contract) — **新建** `api/dto/setup_events.rs`:3 个 struct(`#[serde(rename_all = "camelCase")]`,`Debug + Clone + Serialize + Deserialize + ToSchema`):`SetupInvitationIssuedEvent { code, expires_at_ms }` / `SetupPairingCompletedEvent { sponsor_device_id, joiner_device_id, success, reason }` / `SetupInvitationRevokedEvent { reason }`;在 `api/dto/mod.rs` 注册新模块
3. **S2.5 · 老调用点压 deprecated 噪音**(必做,否则 5 处 warning 飘) — `event_emitter.rs:78,103` / `ws_bridge.rs:1119,1171` 函数级或语句级 `#[allow(deprecated)]`;`api/setup.rs:398` 注释里的字面量不算 deprecated 调用,无需处理
4. **S3 · daemon broadcaster helper**(uc-daemon) — 新建 `uc-daemon/src/api/setup_events.rs`(或并入 `event_emitter.rs`,看现有 emitter 是不是同形结构后定):`pub struct SetupEventBroadcaster { ws_bus: Arc<WsEventBus> }`,3 个方法 `emit_invitation_issued(code, expires_at_ms)` / `emit_pairing_completed(sponsor, joiner, success, reason)` / `emit_invitation_revoked(reason)`,内部组装 `DaemonWsEvent { topic: ws_topic::SETUP, event_type: ws_event::SETUP_*, payload: serde_json::to_value(...) }` 推 ws bus
5. **S4 · 单元测试**(uc-daemon) — 新增测试:订阅 `setup` topic → broadcaster 发各事件 → 断言收到的 `DaemonWsEvent.topic == "setup"` + `event_type` + payload 字段名(必须是 camelCase)与契约一致;3 个事件各 1 测;复用 ws bus / event emitter 既有测试 helper(若有)
6. **S5 · 编译 + 既有契约测试** — `cargo check -p uc-daemon-contract -p uc-daemon`、`cargo test -p uc-daemon-contract`、`cargo test -p uc-daemon --lib` 全绿;前端契约测 `src/api/__tests__/p2p-realtime-contract.test.ts` 不应受影响(老事件路径完全不动,只是常量加 deprecated 标注);**不**跑 e2e(留 T3.5)

**验收门**
- `cargo check -p uc-daemon-contract -p uc-daemon` 通过、无新增 warning
- `cargo test -p uc-daemon` 全绿,新单测覆盖 3 类事件的 wire 编码
- 老的 `SETUP_STATE_CHANGED` / `SETUP_SPACE_ACCESS_COMPLETED` 路径**完全不动**,行为不变(grep 验证 emitter / ws_bridge 老路径无修改)
- 新事件**未在任何业务路径被调用**(只被测试调用),为 T3.3 装配预留空挂钩

**显式不在范围**(T3.1 不做,留给后续)
- ⛔ `SpaceSetupFacade::subscribe_pairing_completion()` 真实订阅 → T3.3
- ⛔ daemon HTTP `/setup/issue-invitation` 等新路由 → T3.2
- ⛔ 删除老 ws 事件常量 / 老 event_emitter 路径 → T3.4
- ⛔ 前端订阅切换 → Phase 4
- ⛔ daemon-client `ws_bridge.rs` 解析新事件 → 与前端订阅一同处理(若 Phase 4 用 daemon-client 路径)

**风险 / 已知坑**
- 命名风格不统一:既有 `peers.changed` 是 `.` + camelCase,但 `peers_snapshot` 这类还掺了下划线一面 —— 选 camelCase 后端到端检查所有新增点,避免又掺一种命名
- broadcaster helper 切忌用 `Arc<dyn Trait>` 抽象过早,先用具体类型,T3.3 真实接 facade 时再视情况抽

**T3.2 · 新 daemon HTTP endpoints**(0.5 天)

替换路由:`uc-daemon/src/api/setup.rs` 整体重写

| 老 endpoint | 新 endpoint | 调用 |
|---|---|---|
| `POST /setup/new` | — | 取消(并入 `POST /setup/initialize`) |
| `POST /setup/submit-passphrase` | `POST /setup/initialize` | `SpaceSetupFacade::initialize_space(InitializeSpaceCommand)` |
| `POST /setup/join` | — | 取消(joiner 不再走"启动加入流程"动作,直接在 UI 输入 code 即可) |
| `POST /setup/select-peer` | — | **删除**(libp2p mDNS 残留) |
| `POST /setup/confirm-peer` | — | **删除**(invitation code 自身就是身份凭证) |
| `POST /setup/verify-passphrase` | `POST /setup/redeem` | `SpaceSetupFacade::redeem_pairing_invitation(RedeemPairingInvitationCommand)` |
| `POST /setup/complete-space-access` | — | **删除**(由 ws `setup.pairing_completed` 替代) |
| — | `POST /setup/issue-invitation`(新增) | `SpaceSetupFacade::issue_pairing_invitation()` → 返回 `{code, expires_at_ms}` |
| `POST /setup/cancel` | `POST /setup/cancel`(语义改) | 撤销 in-flight invitation;若 already completed,返回 409 |
| `POST /setup/reset` | `POST /setup/reset`(保留) | 调用现有重置流程(清 keyslot + setup_status) |
| `POST /setup/clear-transient` | — | **删除**(stateless 模型不再有 transient state) |
| `GET /setup/state` | `GET /setup/state`(语义瘦身) | 仅返回 `{ has_completed: bool, current_invitation: Option<{code, expires_at_ms}>, device_name: Option<String> }`——不再返回 stateful FSM |

##### T3.2 执行细分(2026-04-25 起,session 40)

**现状盘点**(2026-04-25)
- 老 `uc-daemon/src/api/setup.rs`:11 个 handler / 647 行(`get_setup_state` / `start_new` / `start_join` / `select_peer` / `confirm_peer` / `submit_passphrase` / `verify_passphrase` / `complete_space_access` / `cancel` / `clear_transient` / `reset`)
- `SpaceSetupFacade`(`uc-application/src/facade/space_setup/facade.rs`)已有 9 个公开方法,T3.2 直接可用 4 个:`initialize_space` / `unlock_space` / `issue_pairing_invitation` / `redeem_pairing_invitation`
- `SpaceSetupFacade` **缺** 3 个 thin 方法,T3.2 必须补:`cancel_invitation()` / `reset()` / `query_setup_state() -> SetupStateView`
- 老 `SetupFacade` 装配点:`daemon/src/app.rs:272`、`server.rs:22,48,77,101`、`query.rs:13,184,225`、`pairing/host.rs` 多处 — **T3.2 一概不动**(T3.3 切装配)
- 命令/结果型已具备:`InitializeSpaceCommand` / `InitializeSpaceResult` / `IssuePairingInvitationResult` / `RedeemPairingInvitationCommand` / `RedeemPairingInvitationResult`(`uc-application/src/facade/space_setup/commands.rs`)
- AGENTS.md §11.4 铁律:外部 crate 只能 `use uc_application::facade::*`;T3.2 内**所有新代码**严格遵循,不引入任何 `uc_application::setup::*` 老路径

**决策记录**(2026-04-25 拍板)
- **D1 · server.rs 加新字段**(渐进):`DaemonApiState.space_setup_facade: Option<Arc<SpaceSetupFacade>>`,老 `setup_facade: Option<Arc<SetupFacade>>` 字段保留;新 handler 从新字段拿,老 handler 从老字段拿;T3.4 删老字段 + 老 handler
- **D2 · 含**:T3.2 内补 3 个缺失 facade 方法(`cancel_invitation` / `reset` / `query_setup_state`)+ 必要的 `SetupStateView` 型
- **D3 · B 方案 + `/v2/setup/*` 命名空间**(用户拍板):新 6 个 endpoint 全部走 `/v2/setup/*` 子路径(`/v2/setup/initialize` / `/v2/setup/issue-invitation` / `/v2/setup/redeem` / `/v2/setup/cancel` / `/v2/setup/reset` / `/v2/setup/state`);老 `/setup/*` 全留,T3.4 一刀切删
- **D4 · 不删老 handler**:本步只在 routes 注册侧不暴露(老 handler 加 `#[allow(dead_code)]` 静默 unused 警告);T3.4 整体清理时一并删
- **D5 · facade 真实现(b 选项)**(用户拍板):S1 不写桩,3 个 facade 方法连同底层 use case 一并实现;`cancel_invitation` / `reset` / `query_setup_state` 完工后立刻可被 endpoint 调用并返真实结果(非 503/桩)

**子步骤**
1. **S1 · uc-application 加 facade 方法**(`facade/space_setup/facade.rs` + `commands.rs` + `errors.rs` + `mod.rs`)
   - `commands.rs` 新增 `pub struct SetupStateView { has_completed: bool, current_invitation: Option<CurrentInvitation>, device_name: Option<String> }`、`pub struct CurrentInvitation { code: InvitationCode, expires_at: DateTime<Utc> }`
   - `errors.rs` 新增 `pub enum CancelInvitationError { NotIssued, AlreadyRedeemed, /* infra */ }`、`pub enum ResetSpaceError { /* ... */ }`、`pub enum QuerySetupStateError { /* ... */ }`(具体 variant 看现有 facade 风格)
   - `facade.rs` 新增 3 个 `pub async fn`:`cancel_invitation()`、`reset()`、`query_setup_state()`;实现可先调底层 use case(若已有)或返 `Err(NotImplemented)` 桩,但**接口签名稳定**,T3.3 装配后再补完
   - `mod.rs` `pub use` 3 个新型(`SetupStateView` / `CurrentInvitation` / 3 个 Error)+ `facade/mod.rs` re-export
   - 单测(uc-application 内):至少 3 个 thin facade test 验签名走通
2. **S2 · uc-daemon-contract 加新 DTO**(用户拍板**新建 `dto/v2/` 目录**,不混 setup_v2 单文件)
   - 新建 `api/dto/v2/mod.rs`(`pub mod setup;`)+ `api/dto/v2/setup.rs`(类型不带 V2 后缀,模块路径 `api::dto::v2::setup` 已显式)
   - 7 个 struct(全 `#[serde(rename_all = "camelCase")]` + `ToSchema`):`InitializeSpaceRequest` / `InitializeSpaceResponse` / `IssueInvitationResponse` / `RedeemRequest` / `RedeemResponse` / `SetupStateResponse` / `CurrentInvitation`
   - cancel + reset 不需要 response DTO(返 HTTP 204 No Content)
   - `constants.rs` 新增**兄弟模块** `http_route_v2 { SETUP_INITIALIZE, SETUP_ISSUE_INVITATION, SETUP_REDEEM, SETUP_CANCEL, SETUP_RESET, SETUP_STATE }`(与老 `http_route` 平级,T3.4 删老的不影响)
   - `lib.rs` `DAEMON_API_REVISION` 升 `setup-pairing-http-routes-v1` → `setup-pairing-http-routes-v2`
3. **S3 · server.rs 加新字段**(`uc-daemon/src/api/server.rs`)
   - `DaemonApiState` 加字段 `space_setup_facade: Option<Arc<SpaceSetupFacade>>`
   - `new()` 默认 `None`
   - `with_space_setup(self, facade: Arc<SpaceSetupFacade>) -> Self` builder + `space_setup_facade()` getter
   - **import 用 `uc_application::facade::SpaceSetupFacade`(AGENTS.md §11.4 合规)**,老 `uc_application::setup::SetupFacade` import 保留(老 11 handler 还在用)
4. **S4 · 新建 `api/v2/` 目录**(用户拍板**整体一个 v2 目录**,所有 v2 代码都进去)
   - `api/v2/mod.rs`:注册 `pub mod setup;` + 提供 `pub fn router()` 聚合所有 v2 子模块 router
   - `api/v2/setup.rs`:6 个 `pub(crate) async fn` handler(`initialize` / `issue_invitation` / `redeem` / `cancel` / `reset` / `get_state`),命名不带 v2 后缀(模块路径已显式),用 `http_route_v2::SETUP_*` 常量挂路由;`require_facade(state)` helper 统一返 503;每个 handler 把 facade error map 成 `ApiError`(40x/50x);DTO 转换函数(`initialize_to_dto` / `issue_to_dto` / `redeem_to_dto` / `state_to_dto`)放同文件
   - 老 `api/setup.rs` **完全不动**,T3.4 整体删
5. **S5 · routes.rs 注册新路由**(`uc-daemon/src/api/routes.rs`)
   - **+1 行** `.merge(crate::api::v2::router())`(放在 `crate::api::setup::router()` 后面)
   - 老 `/setup/*` 11 条路由**保留**(D4 决定 T3.4 才删 → R1 破坏性变更不发生)
6. **S6 · OpenAPI 注册**(`uc-daemon/src/api/openapi.rs`)
   - 新 6 个 handler 的 utoipa::path 引用注册到 paths
   - 7 个 v2 DTO 加到 schemas(用 `as V2Xxx` alias 避免与老 DTO 同名)
   - 新增 tag `setup-v2`
7. **S7 · 单测**(`uc-daemon/src/api/v2/setup.rs` 末尾 `mod tests`)
   - 7 个纯函数测试覆盖:
     - DTO 转换:initialize/issue/redeem/state(2 个) → 4 个测试
     - error mapping:`map_init_err` 5 variant / `map_redeem_err` 6 variant → 2 个测试
   - 放弃 oneshot router 集成测试:构造真 `DaemonApiState` 需要完整 bootstrap,代价远大于收益(facade-未装配 503 路径在装配测试里覆盖更自然)
8. **S8 · 编译 + 测试**:
   - `cargo check -p uc-application -p uc-daemon-contract -p uc-daemon -p uc-daemon-client`:✅ 全过
   - `cargo test -p uc-daemon --lib`:✅ 19/19 全过(12 既有 + 7 新 v2)
   - `cargo test -p uc-daemon-contract --lib`:✅ 12/12(5 setup_events + 7 v2 setup)
   - 既存 daemon ws + setup 老路径测试**完全不受影响**(老路径 handler + router 都不动)
   - **R1 风险消失**:实际方案下不下线老路由,前端 Phase 4 切换前没有 404 窗口

**验收门**
- 6 个 `/v2/setup/*` endpoint 全部能被 router 路由到,handler 调到对应 facade 方法(单测覆盖)
- `cargo build -p uc-daemon` 通过(老 handler 留着但加 `#[allow(dead_code)]`,无新增 warning)
- `cargo test -p uc-application -p uc-daemon-contract -p uc-daemon` 全绿
- `DaemonApiState.space_setup_facade` 字段就位但**未被装配**(T3.3 装),所有 handler 在 facade 为 `None` 时返 503 — 单测覆盖一个"facade 未装配 → 503"路径
- AGENTS.md §11.4 合规:新 daemon 代码中无 `use uc_application::setup::*`,只有 `use uc_application::facade::space_setup::*`
- DAEMON_API_REVISION 升版(`uc-daemon-contract/src/lib.rs:3` 当前 `"setup-pairing-http-routes-v1"` → 改 `"setup-pairing-http-routes-v2"` 或类似)
- daemon-client 暂不更新(等 Phase 4 前端切换)

**显式不在范围**(T3.2 不做)
- ⛔ daemon 装配 `SpaceSetupFacade` 实例 → T3.3
- ⛔ daemon-client `ws_bridge.rs` 加新事件解析 → 与前端订阅一同在 Phase 4 处理(如果 daemon-client 路径还要保留)
- ⛔ 删除老 `setup.rs` 11 handler / 老 `setup_facade` 字段 / 老路由 → T3.4
- ⛔ 前端切到新 `/v2/setup/*` API → Phase 4 T4.1
- ⛔ daemon 启动时订阅 `SpaceSetupFacade::subscribe_pairing_completion()` → T3.3

**风险 / 已知坑**
- **R1 · 破坏性变更**:S5 把老 `/setup/*` 路由从 router 中移除后,前端老调用立即 404。这是 D3=B 决策的预期后果。回滚预案:revert routes.rs 单文件即可恢复
- **R2 · facade 桩实现**:S1 新增 3 个 facade 方法若内部 use case 未实现,T3.2 内可返 `Err(NotImplemented)` 桩;T3.3 真实接装配时同步补;但 endpoint 单测要 mock facade 而非真调 use case
- **R3 · DAEMON_API_REVISION 升版触发 daemon-client mismatch**:若 daemon-client 在 connect 时校验 revision 字符串,升版会让所有现有 client 拒连。需 grep 确认 client 是否做严格校验(否则 fail-soft)
- **R4 · OpenAPI 老 schema 残留**:openapi.rs 移除老 endpoints 后,若有持续集成的 OpenAPI snapshot test,会跑红 — 需同步更新 snapshot

**T3.3 · daemon api 装配链切换**(✅ 已完成)

**实际范围**(比原描述大幅收窄):T3.3 只做"加",老 `SetupFacade` 一概不动 → T3.4 整体删

**决策**(D1/D2/D3/D4/D5 全部按推荐方案):
- **D1 · a**:`DaemonApp` 加 `space_setup_facade: Option<Arc<SpaceSetupFacade>>` field,与 `space_access_facade` 等套路一致
- **D2 · a**:forwarder task 在 `app.rs::run()` 内 spawn,`tokio::select!` 配合 `cancel.child_token()` 干净退出
- **D3 · b**:entrypoint 一次性取 sponsor device_id 字符串(`runtime.wiring_deps().device.device_identity.current_device_id().to_string()`)传进 DaemonApp,避免 forwarder 持有新 port 依赖
- **D4 · a**(★ T3.1 留下的 wire schema 欠账修复):`SetupPairingCompletedEvent.joiner_device_id: String` → `Option<String>`,Failure 路径填 `None`;bump `DAEMON_API_REVISION = "setup-pairing-http-routes-v2-event-wired"`
- **D5 · a**:T3.3 只接 `pairingCompleted`,`invitation_issued` / `invitation_revoked` 两条留 T4 前端要用时再决定

**实际改动**(7 个文件):
- `uc-daemon-contract/src/api/dto/setup_events.rs`:`SetupPairingCompletedEvent.joiner_device_id` 改 `Option<String>`,新增 `pairing_completed_failure_carries_null_joiner_id` 测试
- `uc-daemon-contract/src/lib.rs`:`DAEMON_API_REVISION` bump
- `uc-daemon/src/api/setup_events.rs`:`emit_pairing_completed` 第二参数改 `Option<String>`,新增 `pairing_completed_failure_without_joiner_id_carries_null_field` 测试
- `uc-application/src/facade/mod.rs`:`pub use space_setup::PairingOutcome` 重导出(daemon 现在需要消费)
- `uc-daemon/src/app.rs`:
  - 新增 imports `SpaceSetupFacade`、`PairingOutcome`、`SetupEventBroadcaster`
  - `DaemonApp` 加两个 field:`space_setup_facade` + `local_device_id`
  - `new_with_deferred(...)` 多两个参数 + `debug_assert!` 不变量(两者必须同时 Some)
  - `run()` 内 `with_space_setup(...)` 注入 api_state(在已有 `with_setup` / `with_space_access` 等之后)
  - `run()` 内 spawn forwarder task:`subscribe_pairing_completion()` → 翻译为 `setup.pairingCompleted` ws,`Lagged` warn、`Closed` debug 退出
- `uc-daemon/src/entrypoint.rs`:在调 `DaemonApp::new_with_deferred(...)` 之前 clone `ctx.space_setup_assembly.facade.clone()` + 取 `current_device_id().to_string()`,多传两个参数

**测试结果**:
- `uc-daemon-contract` lib:13/13 ✅(含新增 `pairing_completed_failure_carries_null_joiner_id`)
- `uc-daemon` lib:20/20 ✅(含新增 `pairing_completed_failure_without_joiner_id_carries_null_field`)
- `uc-application::facade::space_setup`:24/24 ✅
- `cargo check -p uc-daemon-contract -p uc-daemon -p uc-application -p uc-bootstrap`:全绿(只剩预存 deprecation warning,与 T3.3 无关)
- 预存在的 `facade::clipboard::facade::tests::dispatch_*` 2 失败:T3.2 已确认与本任务无关

**T3.3 完成后的现状**:
- `/v2/setup/*` endpoint 真正可用(facade 已注入,503 路径只在测试 / facade 缺失场景触发)
- sponsor 侧成功配对完成时,daemon ws bus 上的 `setup` topic 会广播一条 `setup.pairingCompleted { sponsorDeviceId, joinerDeviceId, success: true, reason: null }`
- sponsor 侧 pairing 失败时(proof_mismatch / 持久化失败 / Confirm 发送失败),广播 `{ success: false, reason: "...", joinerDeviceId: null/Some }`
- 老 `setup.stateChanged` / `setup.spaceAccessCompleted` 路径仍并存(`event_emitter.rs` 走老 `SetupFacade`),前端 Phase 4 切换时双发期内可忽略其一
- forwarder task 与 daemon 同生命周期,daemon shutdown(cancel)时干净退出

**本步未做(留 T3.4)**:
- ⛔ 删 `uc-application/src/setup/` 整个老目录
- ⛔ 删 daemon 老 `setup_facade` 字段 / `with_setup` builder / 老 `api/setup.rs` 11 个 handler
- ⛔ 删 `event_emitter.rs` 老 ws 事件路径(`SETUP_STATE_CHANGED` / `SETUP_SPACE_ACCESS_COMPLETED`)
- ⛔ 删 `assembly.rs:1209` 老 `SetupFacade::new(...)` 装配
- ⛔ 删 `pairing/host.rs` / `query.rs` 内 `Arc<SetupFacade>` 用法

**本步未做(留 T4/前端)**:
- ⛔ `setup.invitationIssued` / `setup.invitationRevoked` ws 事件接通(D5 决定)— facade 没有 invitation broadcast channel,要等前端真消费再加

**T3.4 · 删除老 setup 模块**(0.5 天)
- 删 `uc-application/src/setup/`(整个目录 12 个文件,见 inventory):
  - `facade.rs` / `orchestrator.rs` / `action_executor.rs` / `state.rs` / `errors.rs` / `events.rs` / `actions.rs` / `event_port.rs` / `pairing_facade.rs` / `ports.rs` / `testing.rs` / `mod.rs`
  - 11 个 usecase 子文件(`usecases/*.rs`)
- 删 `uc-application/src/space_access/network_adapter.rs`
- 删 `uc-application/src/space_access/orchestrator.rs` 中 `SendOffer/SendProof/SendResult` action 分支(`execute_actions` 内)
- 删 `uc-application/src/space_access/executor.rs` 的 `transport` 字段
- 删 `uc-core/src/ports/space/transport.rs`(`SpaceAccessTransportPort` trait)
- 删 `uc-core/src/space_access/action.rs` 的 `SendOffer/SendProof/SendResult` 三 variant
- 改 `uc-core/src/space_access/state_machine.rs` 转移序列移除这三个 action

**T3.5 · daemon 单元测试 + handshake e2e 守住**
- 跑 `cargo test -p uc-daemon` 全绿
- 跑 `slice1_handshake_e2e.rs`(已用新 SpaceSetupFacade)全绿
- 新增 daemon HTTP 集成测试:initialize → issue-invitation → 模拟 joiner redeem(用 daemon-client) → ws 收到 `setup.pairing_completed`

---

#### Phase 4 详细任务 · 前端 join space UI 重新设计(~2-3 天)

**T4.1 · 前端 API 层重写**(0.5 天)
- `src/api/daemon/setup.ts` 整体重写:
  - 删:`startNewSpace` / `startJoinSpace` / `selectJoinPeer` / `confirmPeerTrust` / `submitPassphrase` / `verifyPassphrase` / `completeSpaceAccess` / `clearTransient`
  - 新增:
    - `initializeSpace(passphrase: string, deviceName?: string): Promise<InitializeSpaceResult>`
    - `issuePairingInvitation(): Promise<{code: string, expiresAtMs: number}>`
    - `redeemPairingInvitation(code: string, passphrase: string): Promise<RedeemResult>`
    - `cancelInvitation(): Promise<void>`
    - `resetSetup(): Promise<void>`
    - `getSetupState(): Promise<{hasCompleted: boolean, currentInvitation: {...} | null, deviceName: string | null}>`
- 类型定义:`SetupState` 简化为上述瘦身版,删 `SetupError` 老 variants 中 mDNS 相关项
- `src/store/setupRealtimeStore.ts` 重写:订阅 `setup.invitation_issued` / `pairing_completed` ws events

**T4.2 · UI 流程重设计 · sponsor 路径**(1 天)
- **`WelcomeStep`**(保留)不变
- **`CreateSpaceStep`**(替代 `CreatePassphraseStep`,新名):
  - 输入 passphrase + passphrase confirm + device name
  - 提交调 `initializeSpace(passphrase, deviceName)`
  - 成功后自动进入 `SponsorInviteStep`(不需要用户额外点击)
- **`SponsorInviteStep`**(全新组件):
  - 进入时自动调 `issuePairingInvitation()` 获取 code + expires_at
  - 醒目显示 code(大字体,monospace,易抄写;含 copy 按钮)
  - 提示用户:"在另一台设备上输入此邀请码 + 创建空间时使用的口令"
  - 倒计时 UI(从 expires_at 实时倒数;到期自动调 `issuePairingInvitation` 续发)
  - 订阅 ws `setup.pairing_completed` → 跳 `SetupDoneStep`
  - 取消按钮调 `cancelInvitation()` + 退回 `WelcomeStep`
- **`SetupDoneStep`**(保留)显示 sponsor / joiner 设备名;复用现有 UI

**T4.3 · UI 流程重设计 · joiner 路径**(0.5 天)
- **`WelcomeStep`**(保留):"加入空间" 按钮直接跳 `JoinInputCodeStep`(不再调 `startJoinSpace`,因为后端无该 endpoint)
- **`JoinInputCodeStep`**(全新组件,替代 `JoinPickDeviceStep` + `JoinVerifyPassphraseStep` + `PairingConfirmStep` 三步):
  - 单一表单:邀请码输入框(支持粘贴) + passphrase 输入框
  - 提交调 `redeemPairingInvitation(code, passphrase)`
  - 成功跳 `SetupDoneStep`
  - 失败错误码处理:
    - `InvitationNotFound` → "邀请码无效或已过期"
    - `InvitationExpired` → 同上
    - `SponsorUnreachable` → "找不到对端设备,请确认 sponsor 在线"
    - `PassphraseMismatch` → "口令错误"
    - `Internal(_)` → 通用错误
- 进入 `JoinInputCodeStep` 期间显示 inline `ProcessingJoinStep` 风格的等待态(submit 按钮 loading)
- **`ProcessingJoinStep`**(保留)仅在 `redeemPairingInvitation` 调用过程中显示(spinner)

**T4.4 · 删除 libp2p mDNS 残留**(0.5 天)
- 删 `src/pages/setup/JoinPickDeviceStep.tsx`(整文件)
- 删 `src/pages/setup/JoinVerifyPassphraseStep.tsx`(整文件)
- 删 `src/pages/setup/PairingConfirmStep.tsx`(整文件)
- 删 `src/hooks/useDeviceDiscovery.ts`(整文件,libp2p mDNS hook)
- 删 `src/store/slices/devicesSlice.ts` 中 `discoveredPeers` 相关 state(若 store 仅此一处使用,整 slice 删)
- 删 daemon ws event 老 type:`peer_discovered` / `peer_lost` / `peer_name_updated`(`uc-daemon-contract` 同步,task_plan L1316 已列入但需确认前端订阅点已切)

**T4.5 · 路由 + 主流程整合**(0.5 天)
- `src/pages/SetupPage.tsx` 重写:
  - 不再依赖 daemon `SetupState` 投影,改为前端本地 React state 驱动 step 切换
  - step 序列(sponsor):`Welcome → CreateSpace → SponsorInvite → SetupDone`
  - step 序列(joiner):`Welcome → JoinInputCode →(processing)→ SetupDone`
  - `useSetupFlow` hook 简化:仅管理 step + loading + error,不再轮询 `getSetupState`
  - 启动时调一次 `getSetupState()` 检查 `hasCompleted`——为 true 直接跳过 setup
- `src/App.tsx` 启动逻辑同步(`hasCompleted` 路径不变)

**T4.6 · 测试更新**(0.5 天)
- 删除 `src/pages/setup/__tests__/{joinPickDeviceErrorMessage,joinPickPeerIdDisplay,peerIdDisplay,ProcessingJoinStep}.test.tsx` 中已删组件测试
- 新增:
  - `__tests__/SponsorInviteStep.test.tsx`:countdown / copy code / cancel 行为
  - `__tests__/JoinInputCodeStep.test.tsx`:错误态映射
  - `__tests__/SetupPage.flow.test.tsx`:sponsor + joiner 两条 step 序列 navigation
- `src/__tests__/api/daemon/setup.test.ts` 改写:测试新 endpoints

**T4.7 · 真机两台 GUI 验收**(进入 Phase 6 一并跑,本阶段先在 dev mode 双开 daemon 跑通)
- 双开两个 daemon(不同端口 + 不同 config_dir)
- 双开两个 GUI 指向各自 daemon
- 走完 sponsor 创建 + 出码 → joiner 输入 → 双侧 SetupDone 全程
- 验证 ws `setup.pairing_completed` 双侧都收到

---



#### Phase 5 删除清单(整目录 / 整文件)

> 全部 0 个外部消费者,可直接 `rm`(详见 `findings.md` F-111)。**前置**:Phase 3(daemon setup 迁移)+ Phase 4(前端重设计)落地后才能跑——这两个 Phase 已把 setup 模块 + space_access/network_adapter 删除,本清单是剩余 mechanical 删除。

**uc-platform**:
- [ ] `uc-platform/src/adapters/libp2p_network/`(整目录,14 文件)
- [ ] `uc-platform/src/adapters/pairing_stream/`(整目录,3 文件)
- [ ] `uc-platform/src/adapters/file_transfer/`(整目录,6 文件)
- [ ] `uc-platform/src/identity_store.rs`(libp2p 专用)

**uc-app(死代码 usecase)**:
- [ ] `uc-app/src/usecases/clipboard/sync_outbound.rs`
- [ ] `uc-app/src/usecases/clipboard/sync_inbound.rs`
- [ ] `uc-app/src/usecases/file_sync/`(整目录:`sync_outbound.rs` / `sync_inbound.rs` / `sync_policy.rs` / `cleanup.rs` / `copy_file_to_clipboard.rs` / `mod.rs`,见 F-115)
- [ ] `uc-app/src/usecases/pairing/resolve_connection_policy.rs`

**uc-application**:
- [ ] `uc-application/src/pairing/state_machine.rs`(整文件,带掉 `AwaitingUserConfirm` / `PairingChallenge` / `PairingResponse`,见 F-105)

**uc-core ports**:
- [ ] `uc-core/src/ports/pairing_transport.rs`
- [ ] `uc-core/src/ports/network_events.rs`
- [ ] `uc-core/src/ports/file_transport.rs`
- [ ] `uc-core/src/ports/connection_policy.rs`
- [ ] `uc-core/src/ports/discovery.rs`

**uc-core 其他**:
- [ ] `uc-core/src/ids/peer_id.rs`(F-104:0 业务消费,`PeerId` 类型未被任何代码当类型参数使用)
- [ ] `uc-core/src/network/events.rs`(`NetworkEvent` / `ConnectedPeer` / `DiscoveredPeer` / `PeerTrustStatus`)
- [ ] `uc-core/src/network/connection_policy.rs`
- [ ] `uc-core/src/network/protocol/file_transfer.rs`
- [ ] `uc-core/src/network/protocol/heartbeat.rs`
- [ ] `uc-core/src/network/protocol/device_announce.rs`
- [ ] `uc-core/src/network/protocol/protocol_message.rs`

**uc-daemon**:
- [ ] `uc-daemon/src/peers/monitor.rs`(被 Phase 1 新 `presence_monitor.rs` 取代)
- [ ] `uc-daemon/src/workers/peer_discovery.rs`(F-114:整 worker 在 iroh 路径下无意义)

---

#### 文件内部清理(部分修改)

**Clipboard 帧模型 port**:
- [ ] `uc-core/src/ports/clipboard/transport.rs`:删 `ClipboardOutboundTransportPort` 和 `ClipboardInboundTransportPort` trait(见 F-102)
- [ ] `uc-core/src/ports/clipboard/mod.rs`:同步去掉 transport 子模块导出

**Wire 协议保留项瘦身**(见 F-103):
- [ ] `uc-core/src/network/protocol/pairing.rs`:整文件删除——`PairingChallenge` / `PairingResponse` 被 state_machine 唯一消费(随 state_machine 删除);`PairingMessage` / `PairingBusy` 在 P3-pre 方案 D 落地后无消费者(详见决策记录 + F-116)
- [ ] `uc-core/src/network/protocol/clipboard.rs`:删 `ClipboardMessage` / `ProtocolMessage` / `ProtocolDirection` / `ClipboardPayloadVersion`;保 `ClipboardBinaryPayload` / `BinaryRepresentation` / `MIME_IMAGE_PREFIX`(被 `payload_codec.rs` + `list_entry_projections.rs` 用)
- [ ] `uc-core/src/network/protocol/mod.rs`:同步导出
- [ ] `uc-core/src/network/mod.rs`:删 `events`/`connection_policy` 子模块声明
- [ ] `uc-core/src/ids/mod.rs:16`:删 `pub use peer_id::PeerId`
- [ ] `uc-core/src/lib.rs:38`:从 `pub use ids::{...}` 列表删 `PeerId`
- [ ] `uc-core/src/ports/mod.rs`:同步删除被删 port 的导出

**uc-app**:
- [ ] `uc-app/src/deps.rs`:删 `clipboard_outbound` / `clipboard_inbound` / `pairing` / `events` / `file_transfer` 字段 + 文件首行 `#![allow(deprecated)]`
- [ ] `uc-app/src/usecases/clipboard/mod.rs`:去掉 `sync_outbound` / `sync_inbound` mod 声明 + re-export
- [ ] `uc-app/src/usecases/pairing/mod.rs`:去掉 `resolve_connection_policy`
- [ ] `uc-app/src/usecases/mod.rs`:去掉 `file_sync` 模块
- [ ] `uc-app/src/runtime.rs` / `uc-app/src/lib.rs`:清掉旧路径残留(逐文件 grep 后处理)

**uc-application**(注:setup 模块整体随 Phase 3 T3.4 删除,以下条目此处仅作清单完整性,实际已合并到 Phase 3 计划):
- [x] `uc-application/src/setup/`(整目录,12 文件 + 11 个 usecase 子文件)→ Phase 3 T3.4
- [x] `uc-application/src/space_access/network_adapter.rs` → Phase 3 T3.4
- [ ] `uc-application/src/pairing/mod.rs`:去掉 `state_machine` 子模块导出(Phase 5 mass delete)

**uc-platform**:
- [ ] `uc-platform/src/adapters/network.rs`:删 `DisabledPairingTransport`(随 `PairingTransportPort` 删除一起,F-102);保 `PairingRuntimeOwner` 枚举(若仍有意义,需复查)
- [ ] `uc-platform/src/adapters/mod.rs`:删 `libp2p_network` / `pairing_stream` / `file_transfer` 子模块声明
- [ ] `uc-platform/src/lib.rs`:删 `identity_store` 等子模块导出

**uc-bootstrap**:
- [ ] `uc-bootstrap/src/builders.rs`:删 libp2p adapter 装配分支
- [ ] `uc-bootstrap/src/assembly.rs:1010-1157`:删 `DiscoveryPort` 装配 + `NetworkDiscoveryPort` / `EmptyDiscoveryPort` 内联占位

**uc-tauri**:
- [ ] `uc-tauri/src/bootstrap/runtime.rs:35`:从 `use uc_app::{...}` 移除已删字段相关 import
- [ ] `uc-tauri/src/bootstrap/runtime.rs:331-388`:整段删 `sync_outbound_clipboard()` 工厂(零调用方,F-100)
- [ ] `uc-tauri/src/bootstrap/logging.rs:31,49-52`:删 libp2p_mdns 相关 logging filter 注释
- [ ] `uc-tauri/src/test_utils.rs`:删除测试 fakes(`NoopPairingTransport` 等,F-102)

**uc-daemon**:
- [ ] `uc-daemon/src/pairing/host.rs:1`:去掉 `#![allow(deprecated)]`
- [ ] `uc-daemon/src/pairing/host.rs:20`:`use uc_core::network::{...}` 改为只 import 保留的 `PairingMessage` / `PairingBusy` / `SessionId`
- [ ] `uc-daemon/src/peers/mod.rs`:把 `monitor` 改为 `presence_monitor`
- [ ] `uc-daemon/src/workers/mod.rs`:删 `peer_discovery`
- [ ] `uc-daemon` service 装配点:把旧 worker 注册替换为 `PresenceMonitor`

**Cargo**:
- [ ] workspace `Cargo.toml`:移除 `libp2p` / `libp2p-stream` 工作区依赖项
- [ ] `uc-platform/Cargo.toml`:移除 libp2p deps
- [ ] `uc-app/Cargo.toml`:移除(若有)
- [ ] `uc-tauri/Cargo.toml`:保留 `uc-app = { path = "../uc-app" }`(uc-app 删旧 usecase 后仍是有用 crate)
- [ ] 全 workspace 的 `iroh` Cargo feature 门控移除(变成默认)

**daemon-contract**(WS 事件):
- [ ] `uc-daemon-contract/src/api/dto/...`:核实并删除 `PeerDiscoveredPayload` / `PeerLostPayload` / `PeerNameUpdatedPayload` 这类 ws event(若有);保 `PeerConnectionChangedPayload`

---

#### Phase 1 详细任务(daemon peer worker 切换)

**目标**:在删 `NetworkEventPort` 之前,把 daemon peer 事件流切到 `PresencePort`。

**T1**:`uc-daemon/src/peers/presence_monitor.rs` 新增
- 实现 `DaemonService`,持有 `Arc<dyn PresencePort>` + `broadcast::Sender<DaemonWsEvent>`
- 在 `start()` 里 `port.subscribe()` 拿 receiver,循环把 `PresenceEvent { device_id, state, at }` 转成 `peer_connection_changed` ws event
- 单测:`PresencePort` 用 mock 推 3 个事件 → ws 收到 3 条

**T2**:`uc-daemon/src/peers/mod.rs` + service registry 装配点
- 把 `PeerMonitor::new(...)` 替换为 `PresenceMonitor::new(...)`
- 删 `PeerDiscoveryWorker` 注册

**T3**:验证 daemon 启动 + 两机互连 + ws 仍翻 `Online/Offline`(用现有 e2e harness 或手动)

**T4**:删 `uc-daemon/src/peers/monitor.rs` + `uc-daemon/src/workers/peer_discovery.rs`(整文件),`mod.rs` 同步

**风险**:Phase 1 是 daemon-only 改动且已完成。Phase 3/4 的真正风险在 setup facade 迁移 + 前端 UI 重设计的契约一致性,详见各自任务段。

---

#### 验收(Slice 4 整体,Phase 6 收尾时检查)

**代码层面**:
- [ ] `rg -w libp2p src-tauri/crates/uc-{core,app,application,platform,infra,bootstrap,daemon,tauri,cli}/src/` 0 命中(允许:logging filter 字符串字面量、git history 中的注释)
- [ ] `cargo build --workspace` 通过且不再有 "libp2p frozen" 相关 deprecated warning
- [ ] `cargo test --workspace` 通过
- [ ] `pnpm test`(前端单测)全绿
- [ ] Slice 1/2/3 e2e 全绿(pairing 配对 / 剪贴板同步 / blob 传输)
- [ ] `Cargo.lock` 不再含 `libp2p` / `libp2p-stream` / `libp2p-*` 任何 crate

**daemon HTTP 契约**:
- [ ] `/setup/initialize` / `/setup/issue-invitation` / `/setup/redeem` / `/setup/cancel` / `/setup/reset` / `/setup/state` 6 个 endpoint 正常返回
- [ ] daemon ws topic `setup` 推送 `setup.invitation_issued` / `setup.pairing_completed` / `setup.invitation_revoked`
- [ ] daemon ws 不再推送 `peer_discovered` / `peer_lost` / `peer_name_updated`(libp2p mDNS 残留)
- [ ] daemon 启动后 ws `peers.changed` 事件正常翻转(Phase 1 已验)

**前端 UI**:
- [ ] sponsor 路径:`Welcome → CreateSpace → SponsorInvite → SetupDone` 4 步全程可走通
- [ ] joiner 路径:`Welcome → JoinInputCode → SetupDone` 3 步全程可走通
- [ ] 前端代码 grep `useDeviceDiscovery|JoinPickDeviceStep|JoinVerifyPassphraseStep|PairingConfirmStep|peer_discovered` 0 命中

**真机端到端(硬验收)**:
- [ ] 双开两台真机(同局域网,推荐 mac+mac 或 mac+win):
  - sponsor 端 GUI 走 `Welcome → CreateSpace`(输入口令 + 设备名)→ 自动进 `SponsorInvite` 显示邀请码
  - joiner 端 GUI 走 `Welcome → JoinInputCode`(输入码 + 同口令)→ ProcessingJoin → SetupDone
  - 双侧落库 SpaceMember + TrustedPeer
  - 双侧后续剪贴板同步可用(随手 copy 一段文本验证 round-trip)
- [ ] 跨网络 relay 路径(任一台开热点切到 4G):上述同流程仍然成功(可能耗时增加,但不超时)

**阻塞**:Slice 4 内部 Phase 3 / Phase 4 顺序进行(Phase 3 daemon 后端先落地,Phase 4 前端跟上;Phase 5 删 libp2p,Phase 6 收尾真机)。无外部阻塞。

---

### Slice 5 · 后续优化 🔲

**目标**:libp2p 删除后逐项处理还需要打磨的点。**不写死任务清单**,等 Slice 4 落地后再根据实际暴露的问题细化。

**当前已知候选**(按优先级排序):
1. ~~**GUI 路径接入 daemon WS**~~ —— **已并入 Slice 4 Phase 4**(2026-04-24 决策),完成判据下移到 Slice 4 验收。本 Slice 5 不再保留这一项。
2. **`uc-core/src/network/` 进一步整合**:Slice 4 后只剩 `session.rs` + 三个 protocol 文件,可考虑搬到 `uc-application` 或 `uc-infra`(协议帧不属于 core 业务概念)
3. **`uc-app` crate 评估**:`uc-app/usecases/` 保留项(write_coordinator / list_entry_projections / 其他 GUI usecase)可能可以下沉到 `uc-application`,把 `uc-app` 整 crate 退役
4. **技术债清单**(`task_plan.md` T-01 ~ T-17)按业务驱动逐项处理
5. **跨平台 / Relay / NAT 场景验证**(原 Slice 4 任务)纳入正常 QA 流程,不再作为单独 milestone

**阻塞**:Slice 4 完成

## 🧾 技术债清单

> 所有 v1 scope 外但值得记录的设计权衡。**任何处理**必须先检查这里看是否已有对应项。

### T-01 · Blob 传输进度事件流
- **来源**:D 组(2026-04-18)
- **业务背景**:大文件 / 大 payload 的 publish 和 fetch 需要给 UI 显示进度条
- **现状**:`BlobTransferPort::publish / fetch` 签名是阻塞式 `async fn → Result<Bytes>`
- **预案**:另起 `BlobProgressPort::subscribe() -> Stream<BlobProgressEvent>`;iroh-blobs 原生提供 `AddProgress` / `DownloadProgress`,adapter 适配即可
- **触发条件**:用户反馈"传大文件没进度显示" / UAT 提出 UX 需求
- **工作量**:小(1 port + 少量 adapter glue + UI 订阅)

### T-02 · Blob GC / 引用计数清理
- **来源**:D 组(2026-04-18)
- **业务背景**:`TagReason::ClipboardEntry(id)` 用作引用计数,但 v1 未实现 GC 扫描
- **现状**:iroh-blobs FsStore 会无限增长(永不清理),长期使用会占满磁盘
- **预案**:
  - 方案 a:`BlobTransferPort::gc()` 全量扫描,找出无 tag 的 blob 清理
  - 方案 b:独立 `BlobRetentionPolicyPort`(基于时间/大小阈值)
- **触发条件**:用户磁盘占用反馈 / 监控发现 blobs 目录超过阈值(例如 > 10GB)
- **工作量**:中(GC 实现 + ClipboardEntry 删除时 untag 流程)

### T-03 · Blob 跨设备转发去重(接收方作 sponsor)
- **来源**:D 组 F-033(2026-04-18)
- **业务背景**:A 发给 B 后,B 再发同一文件给 C,理论上 B 可以直接用自己持有的 digest 发 ticket,跳过重新加密和 publish
- **现状**:D2 完成后 `BlobReferenceRepositoryPort::save(plaintext_hash, digest)` 已埋下埋点,但未串到 D1 的"作 sponsor"路径
- **预案**:D1 内部查去重缓存时,不限定来源(本机生产 or 本机曾接收),都可复用
- **触发条件**:多设备网络流量优化,或大文件转发场景出现
- **工作量**:小(检查 D1 查询语义,确认跨来源可用)

### T-04 · LAN 附近未配对设备可观测性(原 E3)
- **来源**:E 组(2026-04-18,mDNS 移除之前已决 v1 不做)
- **业务背景**:用户可能想看"我家 LAN 里还有哪些设备在跑 UC 但没加入我的 Space"
- **现状**:mDNS 已从 discovery 完全移除(2026-04-18 决议),E3 搁置
- **预案**:若要做,需要恢复 mDNS discovery(但**仅用于 E3 查询**,不用于配对);另起独立 usecase E3' + 新 port `LocalNetworkProbePort`
- **触发条件**:UAT 反馈;或企业场景(零配置 IT 部署多设备)
- **工作量**:中(mDNS 恢复 + UI 页面 + 过滤逻辑)

### T-05 · F1 阈值懒连模式
- **来源**:F 组(2026-04-18)
- **业务背景**:SpaceMember 数量 > 10 时,预连全员占用 relay 带宽 / 本机资源
- **现状**:F1 启动后对每成员并发 `ensure_reachable`,无上限
- **预案**:`SyncSettings` 加开关 `eager_connect_threshold: usize = 10`;超过时进入懒连模式(仅按需连)
- **触发条件**:用户 SpaceMember > 10,或监控到 relay 出向流量异常
- **工作量**:小(SyncSettings 字段 + F1 分支逻辑)

### T-06 · 自建 DNS discovery 服务
- **来源**:F-015(2026-04-18)
- **业务背景**:企业内网/离线办公场景,用户不希望依赖 n0 公网 DNS 兜底
- **现状**:`SyncSettings` 留有 discovery 配置字段,但仅支持"关闭 / 默认";不支持指定自建 DNS
- **预案**:允许 SyncSettings 填写 `custom_dns_url`,iroh `Endpoint::builder().discovery(CustomDnsDiscovery::new(url))`
- **触发条件**:企业部署需求,或 n0 公网 DNS 不可用事故
- **工作量**:小(SyncSettings 字段 + adapter builder 分支)

### T-07 · Revoke 主动广播
- **来源**:A3(2026-04-18)
- **业务背景**:被撤销成员如果还在线,需要立即知道自己被踢(目前靠下次连接失败间接感知)
- **现状**:revoke 只在本机标记,其他在线成员不得知
- **预案**:新建 `/uniclipboard/control/1` ALPN,承载 `RevokeNotification / RenameAnnounce / ...` 等控制消息
- **触发条件**:安全事件响应场景(被盗设备需要立即断网)
- **工作量**:中(新 ALPN + 新 port `ControlChannelPort` + 状态机扩展)

### T-08 · Rename 主动广播
- **来源**:A5(2026-04-18)
- **业务背景**:用户改名后,其他成员不发剪贴板不会更新名称
- **现状**:v1 被动传播(C1 header 每次带 origin_device_name,C2 upsert)
- **预案**:复用 T-07 的 control ALPN(两者同时做性价比高)
- **触发条件**:UAT 反馈"改名对端看不到"
- **工作量**:小(如果 T-07 已做)

### T-09 · Change Passphrase
- **来源**:A4(2026-04-18,milestone/1.0.0 已删除 `change_passphrase`)
- **业务背景**:用户可能想定期换空间口令
- **现状**:`SpaceAccessPort::change_passphrase` 已从 milestone 移除(标记 unused)
- **预案**:
  1. `SpaceAccessPort` 加 `change_passphrase(space_id, old, new)` → 重新生成 KEK,重写所有 SpaceMember keyslot
  2. 广播"重新 unlock 提示"给其他在线成员(走 T-07 control ALPN)
  3. 其他成员下次启动需输新口令
- **触发条件**:产品决策要开此功能
- **工作量**:中-大(需涉及多方协调)

### T-10 · NodeAddr 快照增长控制
- **来源**:E 组 + F 组
- **业务背景**:`PeerAddressCache.direct_addresses: Vec<String>` 每次连接都可能 upsert,长期会累积失效地址
- **现状**:v1 没有大小限制 / 过期策略
- **预案**:
  - upsert 时限制 `direct_addresses.len() <= 8`(LRU 淘汰)
  - 连接失败多次的地址主动剔除
  - 超过 `observed_at_ms + 30 days` 的记录 GC
- **触发条件**:性能监控发现 repo 体积异常,或用户网络经常切换
- **工作量**:小

### T-11 · 剪贴板大 payload 进度(同 T-01 但覆盖剪贴板流)
- **来源**:C 组(隐含)
- **业务背景**:C1 outbound 写入大 payload(例如 50MB 图片 base64)时,UI 应显示"发送中"进度
- **现状**:`ClipboardDispatchPort::dispatch` 是阻塞式
- **预案**:`ClipboardDispatchPort` 加回调或返回 progress stream;或复用 T-01 的统一 progress bus
- **触发条件**:UAT 反馈 / 用户手动复制大图后无反馈
- **工作量**:小

### T-12 · 配对 Phase 1 细节(F-019 遗留)
- **来源**:findings F-019(Phase 0 调研)
- **具体**:
  - `NodeHandle` 是否暴露 `as_bytes()` / `fingerprint()`?(倾向只暴露 fingerprint)
  - `BlobTicket` 是否拆出 `node: NodeHandle` 字段?(便于路由 + UI 显示来源)
  - `Session::read/write_all` 是否支持 timeout?(iroh stream 原生支持)
  - `Capability` 枚举是否允许 `Custom(&'static str)`?(倾向不允许,强制加变体)
- **触发条件**:Slice 1 编码时遇到具体需求即敲定
- **工作量**:微(随 Slice 1 附带)

### T-13 · iroh-blobs FsStore 目录布局与迁移
- **来源**:F-017(Phase 0)
- **业务背景**:iroh-blobs FsStore 新目录 `blobs/iroh-store/` 与现有 `blobs/encrypted/` 共存;Slice 5 删 libp2p 后,两目录职责重新规划
- **现状**:Slice 3 实施 iroh-blobs 时就地建目录,不做迁移
- **预案**:Slice 5 后评估是否合并目录 + 废弃旧 `encrypted/` 的 file_transfer 残留
- **触发条件**:Slice 5 完成后
- **工作量**:小-中(迁移脚本)

### T-14 · Pairing 协议消息清理
- **来源**:Slice 5 清理任务(2026-04-18)
- **业务背景**:新流程不使用 `PairingChallenge{pin}` / `PairingResponse{pin_hash}` 两条消息
- **现状**:Slice 1-4 期间仍保留(有 libp2p 在用),Slice 5 一次删除
- **预案**:Slice 5 任务清单里已列
- **触发条件**:Slice 5 启动
- **工作量**:小

### T-15 · `UnlockSpaceUseCase` 返回的 `space_id` 应从 `SetupStatus` 读
- **来源**:Slice 1 P9a(F-058)留下的 API 不一致
- **业务背景**:A1 之后 `SetupStatus.space_id = Some(minted)`,A2 `unlock` 应该返回同一个值才能让上层代码(Tauri UI / CLI `status`)始终看到同一个空间
- **现状**:`UnlockSpaceUseCase::execute` line 53 `let space_id = SpaceId::new();` 现场铸,`UnlockSpaceResult.space_id` 跟 `SetupStatus.space_id` 对不上。本 slice 按注释"adapter 不看这个 id"留着没改,避免测试连锁修改
- **预案**:改成 `let space_id = status.space_id.unwrap_or_else(SpaceId::new);`,同步删除"adapter 不看"的注释——因为上层调用点会看
- **触发条件**:Slice 2 接入 Tauri / daemon 的 `status` 查询时必然撞上
- **工作量**:小(一处改动 + A2 测试 assertion 对齐)

### T-16 · `uniclipboard-cli lock` / `unlock` 命令 — ❌ 不做(2026-04-20)
- **来源**:Slice 1 P9b 留下的 CLI 空缺(F-057)
- **决策**:不做。设计上 `SpaceAccessPort` 的 keyslot(磁盘)+ KEK(OS keychain)本就是**长期共享存储**;CLI 进程短命,跑完命令即退出,`lock` 清内存无业务价值,`unlock` 也只是让 keyring 静默恢复路径多一个等价入口。若 keyring miss 真正发生,当前"引导用户重新 init"的错误已满足最小闭环,不值得为边角场景养两个 CLI 命令
- **若未来反悔**:参考本条历史讨论——`--forget` 需新增 `SpaceAccessPort::clear_keyring_cache`(只清 keyring,保留磁盘 keyslot),填在 `lock()` 与 `factory_reset()` 之间的粒度空档

### T-17 · Legacy profile 的 `SetupStatus.space_id == None` 迁移 — ❌ 不做(2026-04-20)
- **来源**:Slice 1 P9a F-058 fallback 路径
- **决策**:不做。项目尚无真实用户,不需要向后兼容;开发者自测老 profile 撞到时手动 `factory_reset` 即可。T-15 按原意保留 `unwrap_or_else(SpaceId::new)` fallback,不做"自愈写回"

---

## 技术债优先级建议

| 优先级 | 触发条件 | 项 |
|---|---|---|
| P0(Slice 1-5 必做) | 编码时附带 | T-12 F-019 细节 / T-14 消息清理 / T-13 目录布局 |
| P1(v1 后首个版本) | 用户体验回流 | T-01 blob 进度 / T-11 剪贴板进度 / T-10 NodeAddr 增长 |
| P2(产品决策驱动) | 功能回填 | T-07 Revoke 广播 / T-08 Rename 广播 / T-09 Change Passphrase |
| P3(规模或环境驱动) | 监控/反馈 | T-02 Blob GC / T-05 阈值懒连 / T-06 自建 DNS |
| P4(可能永不做) | 专项需求 | T-04 E3 / T-03 跨设备转发去重 |

---

## ✅ 完成判据

- [ ] `cargo build` 无 libp2p crate 依赖
- [ ] `grep -r libp2p src-tauri/crates/uc-core src-tauri/crates/uc-app` 为空
- [ ] core 中 network 相关代码 0 处出现 `peer_id: String` / `Multiaddr` / `iroh::NodeId`
- [ ] 所有原有用户场景(配对、剪贴板同步、文件传输)在 iroh 下通过

## 📌 风险 & 缓解

| 风险 | 缓解 |
|---|---|
| 双栈并存期 bootstrap 装配复杂 | Cargo feature + 单一装配入口二选一,不做运行时共存 |
| iroh-blobs 与现有 `uc-infra/blob`(加密 blob)角色重叠 | Phase 0 专项调研,必要时分层:iroh-blobs 传输层 / 现有 blob 加密层 |
| 新 domain 命名与旧冲突 | 新名(候选:`p2p` / `connectivity` / `net2`),不复用 `network` |
| Relay 可用性与隐私 | 可配置 relay URL,支持 self-hosted |
| 开发者上手 iroh API | Phase 0 输出 cheat sheet 写进 findings |

## 🧩 Port 总表(12 个,含签名骨架)— ⚠ 已部分过时

> **⚠ 修订状态(2026-04-19)**:本章节是 outside-in 重新规划**之前**的初版 port 设计。
> Slice 1 部分以**新章节"✅ 已敲定决策(2026-04-19)"和 Slice 1 章节内的草图为准**:
> - `NodeIdentityStorePort` → 改名 `LocalIdentityPort`(显式 create + current_fingerprint)
> - `LocalEndpointTicketPort` → ❌ 删除(adapter 内部细节)
> - `RendezvousClientPort` → 改名 `PairingInvitationPort`(业务语义)
> - `PeerIdentityResolverPort` / `PresencePort` → 待评估扩展 `PeerDirectoryPort`
> - 新增共享值对象 `NodeHandle` / `NodeSecretBytes` / `NodeTicket` → ❌ 都不进 core(Q-α)
>
> Slice 2/3 的 port(C/D 组)**未重新评估**,仍按本章节描述参考;实际编码时按同样的 outside-in 方法反推。
>
> **用途**:Slice 1 编码起点参考;细节在实施中可微调(错误粒度、Bytes vs Vec<u8>、返回值形状等)。
> **约束**:全部位于 `uc-core/src/ports/`,仅依赖 `std + serde + thiserror + async_trait + bytes::Bytes`;禁止 tokio / iroh / libp2p / reqwest。
> **复用类型**(milestone/1.0.0 已存在):`MemberHandle` / `ActiveSpace` / `Plaintext` / `Ciphertext` / `Aad` / `PairingMessage` / `ClipboardEntryId`
> **新增共享值对象**(下方各 port 引用):
> ```rust
> pub struct NodeHandle(pub [u8; 32]);              // 不透明节点身份
> pub struct NodeSecretBytes(pub [u8; 32]);         // iroh 32B secret
> pub struct NodeTicket(pub Vec<u8>);               // opaque,iroh 序列化
> pub struct BlobDigest(pub [u8; 32]);              // BLAKE3 of ciphertext
> pub struct BlobTicket(pub Vec<u8>);               // opaque postcard from iroh-blobs
> pub struct PlaintextHash(pub [u8; 32]);           // BLAKE3 of plaintext(去重用)
> ```

### 🟩 Slice 1 引入(6 新 + 2 iroh 新 impl)

#### 1. `NodeIdentityStorePort` · 🆕
路径:`uc-core/src/ports/node_identity.rs`
```rust
#[derive(Debug, thiserror::Error)]
pub enum NodeIdentityError {
    #[error("secure storage failure: {0}")] Storage(String),
    #[error("corrupted identity blob")]     Corrupted,
}

#[async_trait]
pub trait NodeIdentityStorePort: Send + Sync {
    /// 加载本机 secret;不存在则生成并持久化
    async fn load_or_generate(&self) -> Result<NodeSecretBytes, NodeIdentityError>;
    /// 清除本机 identity(下次 load 会生成新的)
    async fn reset(&self) -> Result<(), NodeIdentityError>;
}
```
**iroh adapter**:`uc-infra/src/network/iroh/identity_store.rs`,key=`"iroh-identity:v1"` via `SecureStoragePort`
**调用方**:F1 / A1

---

#### 2. `LocalEndpointTicketPort` · 🆕
路径:`uc-core/src/ports/local_endpoint_ticket.rs`
```rust
#[derive(Debug, thiserror::Error)]
pub enum LocalEndpointError {
    #[error("endpoint not running")] NotRunning,
    #[error("internal: {0}")]        Internal(String),
}

#[async_trait]
pub trait LocalEndpointTicketPort: Send + Sync {
    /// 基于当前活跃 Endpoint 的 NodeAddr 生成可分享凭证
    async fn issue_node_ticket(&self) -> Result<NodeTicket, LocalEndpointError>;
}
```
**调用方**:B1(sponsor 生成 shortcode 前)

---

#### 3. `RendezvousClientPort` · 🆕
路径:`uc-core/src/ports/rendezvous.rs`
```rust
pub struct Shortcode(pub String);  // 格式 "XXXX-XXXX",8 字符 Crockford-Base32

pub struct RendezvousOffer {
    pub sponsor_device_id:    String,
    pub sponsor_device_name:  String,
    pub sponsor_endpoint_id:  String,        // 对应 iroh EndpointId 文本
    pub sponsor_ticket:       NodeTicket,
    pub ttl_secs:             u32,           // 默认 300
}

pub struct RendezvousResolution {
    pub sponsor_device_id:    String,
    pub sponsor_device_name:  String,
    pub sponsor_endpoint_id:  String,
    pub sponsor_ticket:       NodeTicket,
}

#[derive(Debug, thiserror::Error)]
pub enum RendezvousError {
    #[error("code not found")]         NotFound,
    #[error("code expired")]           Expired,
    #[error("code already consumed")]  AlreadyConsumed,
    #[error("code collision")]         Collision,
    #[error("server unreachable")]     Unreachable,
    #[error("invalid request")]        InvalidRequest,
    #[error("internal: {0}")]          Internal(String),
}

#[async_trait]
pub trait RendezvousClientPort: Send + Sync {
    /// Sponsor 登记 ticket,换取 shortcode + 过期时间
    async fn create(&self, offer: RendezvousOffer)
        -> Result<(Shortcode, u64 /* expires_at_ms */), RendezvousError>;
    /// Joiner 用 shortcode 拉回 sponsor 信息
    async fn resolve(&self, code: &Shortcode) -> Result<RendezvousResolution, RendezvousError>;
    /// Sponsor 配对成功后作废 shortcode
    async fn consume(&self, code: &Shortcode) -> Result<(), RendezvousError>;
}
```
**adapter**:`uc-infra/src/network/rendezvous/http_client.rs`(HTTP to F-030 三端点)
**调用方**:B1 / B2

---

#### 4. `PeerIdentityResolverPort` · 🆕
路径:`uc-core/src/ports/peer_identity.rs`
```rust
#[derive(Debug, thiserror::Error)]
pub enum PeerIdentityError {
    #[error("repository error: {0}")] Repository(String),
}

#[async_trait]
pub trait PeerIdentityResolverPort: Send + Sync {
    /// 节点 → 成员(iroh 对端身份映射回 Space 成员身份)
    async fn resolve(&self, node: &NodeHandle)
        -> Result<Option<MemberHandle>, PeerIdentityError>;
    /// 成员 → 节点(主动拨号时用)
    async fn node_handle_of(&self, member: &MemberHandle)
        -> Result<Option<NodeHandle>, PeerIdentityError>;
}
```
**adapter**:读 milestone 的 `trusted_peer_repo`(其记录了 NodeHandle ↔ MemberHandle 映射)
**调用方**:C2 鉴权、F1 重连、B1 配对后落库

---

#### 5. `PresencePort` · 🆕
路径:`uc-core/src/ports/presence.rs`
```rust
pub enum Reachability {
    Connected,
    Reachable { via: ReachVia },
    LastSeen  { ms_ago: u64 },
    Unknown,
}
pub enum ReachVia { Direct, Relay }

pub enum PresenceEvent {
    MemberReachable    { member: MemberHandle, via: ReachVia },
    MemberUnreachable  { member: MemberHandle, last_seen_ms: u64 },
    MemberConnected    { member: MemberHandle },
    MemberDisconnected { member: MemberHandle },
}

#[derive(Debug, thiserror::Error)]
pub enum PresenceError {
    #[error("network not running")] NotRunning,
    #[error("internal: {0}")]       Internal(String),
}

#[async_trait]
pub trait PresencePort: Send + Sync {
    async fn reachability(&self, m: &MemberHandle) -> Result<Reachability, PresenceError>;
    async fn snapshot(&self) -> Result<Vec<(MemberHandle, Reachability)>, PresenceError>;
    async fn subscribe(&self) -> Result<Box<dyn PresenceEventStream>, PresenceError>;

    // Slice 2 激活(Slice 1 可先 stub 为 Ok(()))
    async fn ensure_reachable(&self, m: &MemberHandle) -> Result<(), PresenceError>;
}

#[async_trait]
pub trait PresenceEventStream: Send {
    async fn recv(&mut self) -> Result<PresenceEvent, PresenceError>;
}
```
**调用方**:E1/E2 UI、C1 出站过滤、F1 重连

---

#### 6. `PeerAddressRepositoryPort` · 🆕
路径:`uc-core/src/ports/peer_address.rs`
```rust
pub struct PeerAddressCache {
    pub relay_url:        Option<String>,
    pub direct_addresses: Vec<String>,   // 上限 8 个(T-10 执行前可无限)
    pub observed_at_ms:   u64,
}

#[derive(Debug, thiserror::Error)]
pub enum PeerAddressError {
    #[error("repository error: {0}")] Repository(String),
}

#[async_trait]
pub trait PeerAddressRepositoryPort: Send + Sync {
    async fn get(&self, member: &MemberHandle)
        -> Result<Option<PeerAddressCache>, PeerAddressError>;
    async fn save(&self, member: &MemberHandle, cache: PeerAddressCache)
        -> Result<(), PeerAddressError>;
    async fn remove(&self, member: &MemberHandle) -> Result<(), PeerAddressError>;
}
```
**adapter**:sqlite repo(新表 `peer_address_cache`)
**调用方**:F1 重连、iroh adapter 连接成功时 upsert、A3 revoke 时清理

---

#### 7. `PairingTransportPort` · 既有 + iroh 新 impl
路径:`uc-core/src/ports/pairing_transport.rs`(milestone 已有)
**iroh 新 impl**:`uc-infra/src/network/iroh/pairing_transport.rs`
**Slice 5 重建**:清理 `peer_id: String` 参数 → `NodeHandle` / `MemberHandle`

---

#### 8. `NetworkControlPort` · 既有扩展 + iroh 新 impl
路径:`uc-core/src/ports/network_control.rs`(milestone 已有)
```rust
#[async_trait]
pub trait NetworkControlPort: Send + Sync {
    async fn start_network(&self) -> anyhow::Result<()>;     // 既有
    async fn stop_network(&self)  -> anyhow::Result<()>;     // Slice 1 新增方法
}
```
**iroh 新 impl**:`uc-infra/src/network/iroh/lifecycle.rs`(bind Endpoint + 注册 ALPN handlers + 启 discovery;停:drop connections + close endpoint)
**调用方**:F1 / F2

---

### 🟦 Slice 2 引入(2 新)

#### 9. `ClipboardDispatchPort` · 🆕
路径:`uc-core/src/ports/clipboard/dispatch.rs`
```rust
pub struct ClipboardHeader {
    pub content_hash:       String,
    pub timestamp_ms:       u64,
    pub origin_device_id:   String,
    pub origin_device_name: String,           // A5 rename 被动传播
    pub payload_version:    u8,               // V3 = 3
    pub blob_refs:          Vec<BlobTicket>,  // C3 的 ticket 列表,空 Vec = 无文件
}

#[derive(Debug, thiserror::Error)]
pub enum ClipboardDispatchError {
    #[error("member not reachable")]   NotReachable,
    #[error("stream rejected")]        StreamRejected,
    #[error("payload write timeout")]  Timeout,
    #[error("peer nack: {0}")]         PeerNack(String),
    #[error("internal: {0}")]          Internal(String),
}

#[async_trait]
pub trait ClipboardDispatchPort: Send + Sync {
    /// 向某 member 开新 iroh bi-stream,写 header + 加密 payload
    async fn dispatch(
        &self,
        target: &MemberHandle,
        header: &ClipboardHeader,
        payload_ciphertext: bytes::Bytes,
    ) -> Result<(), ClipboardDispatchError>;
}
```
**调用方**:C1

---

#### 10. `ClipboardReceiverPort` · 🆕
路径:`uc-core/src/ports/clipboard/receiver.rs`
```rust
pub struct InboundClipboard {
    pub peer:               MemberHandle,    // 已通过 PeerIdentityResolverPort 鉴权
    pub header:             ClipboardHeader,
    pub payload_ciphertext: bytes::Bytes,
}

#[async_trait]
pub trait ClipboardReceiverPort: Send + Sync {
    async fn subscribe(&self)
        -> Result<Box<dyn ClipboardInboundStream>, ClipboardDispatchError>;
}

#[async_trait]
pub trait ClipboardInboundStream: Send {
    async fn recv(&mut self) -> Result<InboundClipboard, ClipboardDispatchError>;
}
```
**调用方**:C2

---

### 🟨 Slice 3 引入(2 新)

#### 11. `BlobTransferPort` · 🆕
路径:`uc-core/src/ports/blob/transfer.rs`

> **2026-04-24 更新**:下方草案的 `BlobTicket::digest()` 值对象方法违反 `uc-core/AGENTS §19.1`(以实现反推领域)——已在 `slice3-phase1-plan.md §8 R2` 定稿改走方案 C:`BlobTicket` 真正 opaque,digest 提取走 `BlobTransferPort::digest_of(ticket) -> Result<BlobDigest, BlobError>`。下方 `impl BlobTicket { digest }` 已过时,仅保留作为重构轨迹参考。

```rust
// ⚠️ 过时初稿(2026-04-18),实际签名见 slice3-phase1-plan.md §3.1
impl BlobTicket {
    pub fn digest(&self) -> BlobDigest { /* parse postcard */ todo!() }
}

pub enum TagReason {
    ClipboardEntry(ClipboardEntryId),  // 预留扩展
}

#[derive(Debug, thiserror::Error)]
pub enum BlobError {
    #[error("digest not found")]   NotFound,
    #[error("download failed: {0}")] Download(String),
    #[error("internal: {0}")]      Internal(String),
}

#[async_trait]
pub trait BlobTransferPort: Send + Sync {
    // ── 发布 ──
    async fn publish(&self, ciphertext: bytes::Bytes) -> Result<BlobDigest, BlobError>;
    async fn issue_ticket(&self, digest: &BlobDigest) -> Result<BlobTicket, BlobError>;
    // ── 接收 ──
    async fn fetch(&self, ticket: &BlobTicket) -> Result<bytes::Bytes, BlobError>;
    // ── 生命周期 ──
    async fn has(&self, digest: &BlobDigest) -> Result<bool, BlobError>;
    async fn tag(&self, digest: &BlobDigest, reason: TagReason) -> Result<(), BlobError>;
    async fn untag(&self, digest: &BlobDigest, reason: TagReason) -> Result<(), BlobError>;
}
```
**adapter**:`uc-infra/src/network/iroh/blobs.rs`,内置 `iroh_blobs::store::fs::FsStore`
**调用方**:D1 / D2 / C3

---

#### 12. `BlobReferenceRepositoryPort` · 🆕
路径:`uc-core/src/ports/blob/reference.rs`
```rust
#[derive(Debug, thiserror::Error)]
pub enum BlobReferenceError {
    #[error("repository error: {0}")] Repository(String),
}

#[async_trait]
pub trait BlobReferenceRepositoryPort: Send + Sync {
    async fn find_by_plaintext_hash(&self, hash: &PlaintextHash)
        -> Result<Option<BlobDigest>, BlobReferenceError>;
    async fn save(&self, hash: PlaintextHash, digest: BlobDigest)
        -> Result<(), BlobReferenceError>;
    async fn forget(&self, hash: &PlaintextHash) -> Result<(), BlobReferenceError>;
}
```
**adapter**:sqlite repo(新表 `blob_reference`,`plaintext_hash PRIMARY KEY → digest`)
**调用方**:D1 去重判断、D2 完成后记录(跨设备转发准备,T-03)

---

## 📋 Usecases(业务动作清单)

> 六边形设计从这里开始。每个 usecase 确定后,能力缺口汇总 → 反推 port。

### 分类汇总

| 分类 | Usecase |
|---|---|
| A · 空间 & 身份 | A1 initialize / A2 join / A3 revoke / A4 change-passphrase / A5 rename-device |
| B · 配对 | B1 sponsor 发起 / B2 joiner 加入 |
| C · 剪贴板同步 | C1 outbound / C2 inbound / C3 with-files |
| D · Blob 传输 | D1 publish / D2 fetch |
| E · 在线 / 发现 | E1 roster / E2 presence-events / E3 unpaired-nearby |
| F · 生命周期 | F1 startup / F2 shutdown |
| G · v1 范围外 | G1 offline-catchup / G2 fingerprint-verification |

---

### B1 · Sponsor 发起配对(旧设备,Space 已解锁)

**触发**:Sponsor UI 点"添加新设备"

**约束**:
- 客户端**同时只允许 1 个 pending shortcode**(Q4 已定)
- Sponsor 侧**无用户弹窗确认**(已定);`AwaitingUserApproval` 状态去除

**业务步骤**:
1. 生成 iroh NodeTicket(含 NodeAddr + EndpointId)
2. `POST /v1/pairings` → 拿 shortcode(8 字符 `XXXX-XXXX`)
3. UI 显示 shortcode + 倒计时(5 分钟 TTL)
4. 本地状态 = `AwaitingShortcodeRedeem { shortcode, expires_at }`,跨 Space 成员广播"待入"事件(可选)
5. Joiner 通过 iroh 连入 pairing ALPN → 开 bi-stream
6. 收 `PairingRequest` → 走现有状态机 `RecvRequest`
7. `SpaceAccessPort::prepare_join_offer(space_id, own_passphrase)` → `JoinOffer{keyslot_blob, challenge_nonce}`
8. 发 `PairingKeyslotOffer { keyslot_file: Some(blob), challenge: Some(nonce) }`
9. 收 `PairingChallengeResponse { encrypted_challenge }` → 本地用 KEK 验证
10. 通过:发 `PairingConfirm{success}`,持久化 `TrustedPeer/SpaceMember`,`POST /v1/pairings/consume` 作废 shortcode
11. 失败:发 `PairingReject`,consume shortcode,回 `Idle`

**需要的领域能力**(→ port):
| 能力 | 已有? | 位置 / 缺口 |
|---|---|---|
| 查询本机 Space 是否已解锁 | ✅ | `SpaceAccessPort::is_unlocked` |
| 生成 join offer(keyslot + nonce) | ✅ | `SpaceAccessPort::prepare_join_offer`(milestone 已实现) |
| 生成 iroh NodeTicket | ❌ | 新 port,候选名:`LocalEndpointTicketPort` |
| 调用 rendezvous 3 接口(create/consume) | ❌ | 新 port,候选名:`RendezvousClientPort` |
| 接受 pairing 入站流 | ❌ | 新 port,候选名:`PairingAcceptorPort`(或沿用 `PairingTransportPort` 的 listen 端) |
| 持久化 SpaceMember | ✅ | milestone 的 `trusted_peer_repo` |
| 验证 joiner 的 challenge response | ✅ | 用 ActiveSpace 的 KEK 重算(已在 adapter 内) |
| 维持"同时 1 个 pending shortcode"的单例约束 | ❌ | uc-app 层编排,非 port |

---

### B2 · Joiner 加入(新设备,手持 passphrase 明文)

**触发**:Joiner UI 输入 shortcode + passphrase

**约束**:
- `Q1`:passphrase = Space passphrase 明文(复用,不引入一次性邀请口令)

**业务步骤**:
1. `POST /v1/pairings/resolve { code }` → 得 `sponsorTicket`(+ sponsorDeviceName / sponsorEndpointId)
2. 解析 NodeTicket → iroh NodeAddr
3. 本机 iroh endpoint 拨号,ALPN = `/uniclipboard/pairing/1` → `open_bi`
4. 发 `PairingRequest { session_id, identity_pubkey, nonce, device_id, device_name }`
5. 收 `PairingKeyslotOffer { keyslot_file, challenge }`
6. `SpaceAccessPort::derive_master_key_for_proof(offer, passphrase)` → MasterKey
7. 用 MasterKey 或派生 KEK 对 `challenge_nonce` 生成 `encrypted_challenge`
8. 发 `PairingChallengeResponse { encrypted_challenge }`
9. 收 `PairingConfirm { success }` 或 `PairingReject`
10. 通过:持久化本地 Space + MasterKey + 自己的 SpaceMember 记录 + 对端 TrustedPeer
11. 失败:清理,回 `Idle`,错误上抛 UI

**需要的领域能力**(→ port):
| 能力 | 已有? | 位置 / 缺口 |
|---|---|---|
| 调 rendezvous resolve | ❌ | `RendezvousClientPort`(与 B1 共用) |
| 解析 iroh NodeTicket | ❌ | adapter 内(不进 domain);domain 看到的是 opaque `SponsorHandle` |
| 主动开 pairing 出站流 | ❌ | 新 port,候选名:`PairingDialerPort`(或沿用 `PairingTransportPort` 的 dial 端) |
| 用 passphrase 解 keyslot 得 MasterKey | ✅ | `SpaceAccessPort::derive_master_key_for_proof`(milestone 已实现) |
| 构造 `encrypted_challenge` | ⚠️ | 部分已有,需检查 crypto_adapter 是否暴露此算法 |
| 持久化本地 Space + MasterKey | ✅ | milestone 的 `SpaceAccessPort` 内部 |
| 持久化对端 TrustedPeer | ✅ | `trusted_peer_repo` |

---

### B1 + B2 汇总:**新增的 3 个 port 雏形**

| Port | 服务对象 | 签名雏形(待 Phase 1 打磨) |
|---|---|---|
| `RendezvousClientPort` | B1 create/consume, B2 resolve | `create(offer) → Shortcode` / `resolve(code) → SponsorOffer` / `consume(code)` |
| `LocalEndpointTicketPort` | B1 | `issue_node_ticket() → NodeTicket` |
| `PairingTransportPort`(既有 trait,iroh 新增 impl) | B1 listen + B2 dial | 沿用,只加 iroh adapter |

所有 port **仅服务 B1/B2**。C/D/E/F 的 usecase 展开时会再汇总它们自己的能力缺口,与 B 的 port 共享/新增视情况而定。

---

### C1 · Outbound 剪贴板同步(本机 → 其他已配对在线设备)

**触发**:本机剪贴板变化 → 已完成去重 / 规范化 / 持久化 ClipboardEntry

**约束**:
- Q4:**每次同步开新 iroh bi-stream**,stream 一次性、用完关闭
- Q5 已定:payload 用 Space MasterKey 端到端加密(不信任 relay/网络)
- ALPN = `b"/uniclipboard/clipboard/1"`
- wire 不再需要 4 字节长度前缀(iroh stream 有原生消息边界)

**业务步骤**:
1. `SpaceAccessPort::is_unlocked(space_id)` —— 未解锁直接放弃
2. 枚举**当前在线且启用同步**的 SpaceMember(过滤条件:可达 + `MemberSyncPreferences.enabled`)
3. 若无在线成员 → 仅持久化本地条目,流程结束
4. 将 ClipboardEntry 规范化成 V3 chunked AEAD 加密字节流(**已有 payload_v3 逻辑**)
5. 构造 header(元数据,未加密):`content_hash / timestamp / origin_device_id / origin_device_name / payload_version / blob_refs:Vec<BlobTicket>` — 不含 payload 本身
6. 对每个目标**并发**执行:
   a. 开新 iroh bi-stream,ALPN 如上
   b. 写 header(一次性,长度由 QUIC 子帧决定)
   c. 流式写加密 payload chunks
   d. `finish` 发送端半流
   e. 等对端 ack(或读到 FIN)→ 关流
7. 收集每目标结果(成功/失败/对端 NACK)→ 记事件 `ClipboardDispatched`

**能力缺口**:
| 能力 | 已有? | 位置 / 缺口 |
|---|---|---|
| 查询 Space 解锁状态 | ✅ | `SpaceAccessPort::is_unlocked` |
| 枚举"在线 + 启用同步"成员 | 部分 | `GetMemberSyncPreferences`(milestone)+ 新增 PresencePort(E 组) |
| 加密 V3 payload | ✅ | `BlobCipherPort`(milestone)+ 已有 V3 codec |
| 对某 SpaceMember 开出站 stream | ❌ | 新 port:**`ClipboardDispatchPort`**(业务语义) |
| 记 outbound 事件 | ✅ | ClipboardEventRepo |

---

### C2 · Inbound 剪贴板同步(接受并落地)

**触发**:收到 `/uniclipboard/clipboard/1` ALPN 的入站 iroh bi-stream

**业务步骤**:
1. 从 iroh 连接拿到对端 EndpointId → 查 `TrustedPeer` → 获 SpaceMember 身份
2. 身份未识别(不是任何 Space 成员)→ 立即拒绝并关流
3. 读 header → 判定 payload_version(非 V3 拒绝)
4. 按 header 做**去重**:`content_hash` 已入库则丢弃,回 ack "duplicate",关流
5. 流式读加密 payload → 解密(`BlobCipherPort`)→ 校验 content_hash 一致
6. header 中 `blob_refs: Vec<BlobTicket>` 非空 → 触发 D2(按需异步拉 blob)
7. 持久化 ClipboardEntry + 事件 `ClipboardReceived`
8. 根据本地策略更新系统剪贴板(策略:自动写入 vs 仅存档,由 SyncPreferences 决定)
9. 回写 ack,关流

**能力缺口**:
| 能力 | 已有? | 位置 / 缺口 |
|---|---|---|
| 订阅 `/uniclipboard/clipboard/1` 入站流 | ❌ | 新 port:**`ClipboardReceiverPort`**(业务语义) |
| EndpointId → SpaceMember 解析 | ❌ | 新 port:**`PeerIdentityResolverPort`**(也被 C1/B1 复用) |
| 解密 V3 | ✅ | `BlobCipherPort` |
| 去重 / 持久化 ClipboardEntry | ✅ | ClipboardEntryRepo |
| 触发 blob 拉取 | ❌ | 依赖 D2 port(见 D 组) |
| 策略决定是否写入系统剪贴板 | ✅ | 已有 `MemberSyncPreferences` / select_representation_policy |

---

### C3 · 含文件的剪贴板同步

**触发**:C1 捕获到的剪贴板内含文件引用(image with file-path、URL 列表、文件拖放)

**扩展 C1**:
- 发送方在构造 header 前:对每个文件调 `BlobTransferPort::publish(encrypted_bytes) → digest`,再 `issue_ticket(digest) → BlobTicket`
- header.blob_refs 填 ticket 列表
- **payload 本身不含文件二进制**,只含路径占位符 + digest

**扩展 C2**:
- 接收方处理 header.blob_refs:
  - 按需 / 预取策略 触发 `BlobTransferPort::fetch(ticket)`
  - 写入本地文件系统 cache,回填路径(跨平台路径改写)

**能力缺口**(主要落在 D 组):
- `BlobTransferPort` → D 组展开时敲定

---

## C 组汇总:**新增 port 雏形**

| Port | 服务 usecase | 签名雏形(Phase 1 细化) |
|---|---|---|
| **`ClipboardDispatchPort`** | C1 | `dispatch(target: MemberHandle, header: ClipboardHeader, payload: Box<dyn AsyncBytesReader>) → Result<Ack>` |
| **`ClipboardReceiverPort`** | C2 | `subscribe() → Box<dyn ClipboardReceiverStream>`,每个 item 暴露 `(peer: MemberHandle, header, payload_reader)` |
| **`PeerIdentityResolverPort`** | C2(+B1/B2 共用) | `resolve(endpoint_like) → Option<MemberHandle>` |
| **(E 组预声明)`PresencePort`** | C1 枚举"当前在线成员" | 见 E 组 |

**已有不动**:`BlobCipherPort`、`ClipboardEntryRepository`、`ClipboardEventRepository`、`MemberSyncPreferences` 查询 / `select_representation_policy`

**可废弃**(随 libp2p Phase 8 一起删):
- `ClipboardOutboundTransportPort` / `ClipboardInboundTransportPort`(帧模型)
- `ClipboardMessage` wire struct(JSON+base64,V3 已不再需要 JSON 外包装,直接二进制 header + payload)
- `frame_to_bytes` 4B 长度前缀

---

### E1 · Roster 查询(UI 加载 / C1 出站前 / F1 重连时)

**触发**:
- UI 拉"设备列表"页时
- C1 出站前枚举"当前在线 + 启用同步"目标
- F1 启动后决定哪些成员尝试连

**输入**:`SpaceId`
**输出**:`Vec<MemberRosterEntry>`,每项含:
- `MemberHandle` / 6 位 device_id / 设备名 / 设备类型
- `MemberSyncPreferences`(是否启用同步等)
- `Reachability`(见下)
- 最近同步事件时间戳(可选,UI 显示)

**业务步骤**:
1. `SpaceMemberRepo.list(space_id)` —— 全体成员
2. 对每成员 `PresencePort::reachability(member)` —— 合成状态
3. 可选合并最近同步事件(ClipboardEventRepo)
4. 返回

**能力缺口**:
| 能力 | 已有? | 位置 / 缺口 |
|---|---|---|
| SpaceMember 枚举 | ✅ | milestone 的 `space_member_repo` / `trusted_peer_repo` |
| 同步偏好 | ✅ | `GetMemberSyncPreferences`(milestone) |
| reachability 查询 | ❌ | 新 port:**`PresencePort::reachability`** |
| 最近同步事件 | ✅ | `ClipboardEventRepository` |

---

### E2 · Presence 事件订阅

**触发**:
- UI 订阅 roster 响应式更新
- C1 / C2 的编排层响应成员上下线(例如重发失败的条目)
- 日志 / 可观测性

**输出**:`PresenceEvent` 流

**建议事件类型**:
```rust
enum PresenceEvent {
    MemberReachable      { member: MemberHandle, via: ReachVia },
    MemberUnreachable    { member: MemberHandle, last_seen_ms: u64 },
    MemberConnected      { member: MemberHandle },
    MemberDisconnected   { member: MemberHandle },
}

enum ReachVia { Direct, Relay }
```

**业务步骤**(adapter 内部逻辑):
1. 监听 iroh `Endpoint` 的连接 accept / close 事件
2. 监听 iroh discovery(mDNS + n0 DNS)结果
3. 合并 EndpointId → 映射到 `MemberHandle`(通过 `PeerIdentityResolverPort`)
4. 去抖 + 去重 → 发 `PresenceEvent`
5. 广播给订阅者

**能力缺口**:
| 能力 | 已有? | 位置 / 缺口 |
|---|---|---|
| 订阅 presence 事件 | ❌ | 新 port:**`PresencePort::subscribe`** |
| iroh 连接 lifecycle 事件 | — | adapter 内,不出 port |
| iroh discovery 事件 | — | adapter 内,不出 port |
| EndpointId → Member 映射 | ❌ | 新 port:`PeerIdentityResolverPort`(C2 已声明,共用) |

---

### E3 · LAN 附近未配对设备(**v1 范围外,默认不做**)

**旧流程**中,E3 是 mDNS 驱动的配对入口("看到附近设备 → 发起请求")。

**新流程**下 rendezvous + shortcode 驱动配对,E3 不再是配对前置。

**结论**:
- E3 **v1 不做**
- mDNS 在 iroh adapter 中仍**保留**,但职责收窄为:**帮已配对的 SpaceMember 在同一 LAN 时优先走 direct(省 relay 流量)**。mDNS 结果不上 domain 事件,只在 adapter 内做路由选择。
- 若将来要做"附近未配对设备"的可观测性功能,作为独立 usecase E3' 另起。

---

## E 组汇总:**新增 port 雏形**

| Port | 服务 usecase | 签名雏形 |
|---|---|---|
| **`PresencePort`** | E1, E2, C1, F1 | `reachability(m) → Reachability` / `snapshot() → Vec<(Handle, Reachability)>` / `subscribe() → Box<dyn PresenceEventStream>` |
| **`PeerAddressRepositoryPort`** | F1, E2 内部 | `get(m) / save(m, cache) / remove(m)` — last-known NodeAddr 持久化 |
| **`PeerIdentityResolverPort`** | E2, C2, B 组 | 已在 C 组声明,此处只是复用 |

**`Reachability` 形状草案**(domain 值对象):
```rust
enum Reachability {
    Connected,                           // 活跃 iroh connection
    Reachable { via: ReachVia },         // 有地址线索但未建连
    LastSeen { ms_ago: u64 },            // 曾见,现失联
    Unknown,
}
```

**设计原则**:
- `PresencePort` 合并查询 + 订阅为一个 port(**一个业务概念 = 一个 port**,领域内聚)
- adapter 内:iroh Connection 事件 + n0 DNS discovery 事件 + 真实流量失败检测 → 合成 domain 事件
- **不主动心跳**:QUIC keep-alive + 真实流量失败检测足够
- **mDNS 不启用**(2026-04-18 决议):discovery 仅靠 n0 DNS + last-known NodeAddr(见 F1)

### E 组衍生:持久化 last-known NodeAddr

**由来**:mDNS 移除后,LAN 冷启动发现依赖**已缓存的 peer 地址**。这是 discovery 可靠性的兜底。

**新增能力**:
- 值对象 `PeerAddressCache { relay_url: Option<String>, direct_addresses: Vec<String>, observed_at_ms: u64 }`
- 新 port:**`PeerAddressRepositoryPort`** — `get(member)` / `save(member, cache)` / `remove(member)`
- **由谁写**:iroh adapter 在 connection 成功/closing 时触发,经 usecase 写入
- **由谁读**:F1 启动时尝试 last-known,E2 discovery 结果 merge 时 upsert

---

### 其他 usecase(outline,待展开)

- **A1** initialize 新 Space:创建 identity + 持久化 + 建 trust root;输入:passphrase 明文;输出:SpaceId
- **A2** unlock 已有 Space:passphrase → ActiveSpace
- **A3** revoke 成员
- **A4** change passphrase
- **A5** 重命名本机 + 广播
### D1 · Blob Publish(发送方把文件内容加入本地可分享存储)

**触发**:
- C1 捕获到的剪贴板含文件类型(C3 扩展路径)
- UI / 其他 usecase 直接要求共享一份内容

**输入**:`Plaintext`(已载入内存的字节,或通过 reader 流式产生)+ `ActiveSpace` + `ClipboardEntryId`(作为引用归属)

**业务步骤**:
1. 计算 `plaintext_hash = HashPort::blake3(plaintext)`(便宜,已有 `HashPort`)
2. `BlobReferenceRepositoryPort::find_by_plaintext_hash(plaintext_hash)`
   - 命中 → `BlobTransferPort::has(digest)?`
     - 本地仍存 → **跳过加密 + publish**,直接 `issue_ticket(digest)` 返回
     - 本地已 GC → 走 3
   - 未命中 → 走 3
3. `BlobCipherPort::encrypt(active_space, plaintext, aad)` → `Ciphertext`
4. `BlobTransferPort::publish(ciphertext_bytes)` → `BlobDigest`
5. `BlobReferenceRepositoryPort::save(plaintext_hash, digest)`(去重缓存)
6. `BlobTransferPort::tag(digest, ClipboardEntryId)`(防 GC)
7. `BlobTransferPort::issue_ticket(digest)` → `BlobTicket`
8. 返回 `BlobTicket` 给调用方(塞进 clipboard header)

**能力缺口**:
| 能力 | 已有? | 位置 / 缺口 |
|---|---|---|
| 明文 hash 计算 | ✅ | 已有 `HashPort`(blake3) |
| 明文 hash → digest 去重缓存 | ❌ | 新 port:**`BlobReferenceRepositoryPort`** |
| 加密 plaintext → ciphertext | ✅ | `BlobCipherPort`(milestone) |
| 发布密文到本地 content-addressed store | ❌ | 新 port:**`BlobTransferPort::publish / has / tag`** |
| 生成 BlobTicket | ❌ | 新 port:**`BlobTransferPort::issue_ticket`** |

---

### D2 · Blob Fetch(接收方按 ticket 拉取并解密)

**触发**:
- C2 收到 clipboard header 含 `blob_refs: Vec<BlobTicket>` → 按策略触发
  - 策略 1(默认):**小文件即拉**,大文件**懒拉**(阈值由 SyncPreferences 决定)
  - 策略 2:用户手动点"获取"按钮
- 用户直接请求某 ticket

**输入**:`BlobTicket` + `ActiveSpace`

**业务步骤**:
1. `digest = blob_transfer.digest_of(&ticket)?`(2026-04-24 R2 方案 C:`digest()` 从值对象方法移至 port,见 `slice3-phase1-plan.md §8`)
2. `BlobTransferPort::has(digest)?`
   - 本地已有(曾经发过或拉过)→ 走 4 解密
   - 没有 → 走 3
3. `BlobTransferPort::fetch(ticket)` —— iroh-blobs `Downloader.download`,把密文拉到本地 FsStore(自带断点续传 + BLAKE3 完整性校验)
4. 从 FsStore 读出 `ciphertext`
5. `BlobCipherPort::decrypt(active_space, ciphertext, aad)` → `plaintext`
6. 写入本地 cache(blob cache 目录),返回路径给 C2
7. `BlobReferenceRepositoryPort::save(plaintext_hash, digest)`(跨设备去重准备)
8. `BlobTransferPort::tag(digest, ClipboardEntryId)`(防 GC)

**能力缺口**:
| 能力 | 已有? | 位置 / 缺口 |
|---|---|---|
| 本地是否已有 digest | — | `BlobTransferPort::has` |
| 按 ticket 拉 | ❌ | `BlobTransferPort::fetch` |
| 解密 | ✅ | `BlobCipherPort::decrypt` |
| 写入本地 cache 路径 | ✅ | 已有 `BlobRepository`(uc-infra) |
| 登记跨设备去重 | — | 复用 `BlobReferenceRepositoryPort::save` |

---

## D 组汇总:**新增 port 雏形**

| Port | 服务 usecase | 签名雏形 |
|---|---|---|
| **`BlobTransferPort`** | D1, D2, C3 | `publish / has / tag / untag / issue_ticket / fetch` |
| **`BlobReferenceRepositoryPort`** | D1, D2 去重 | `find_by_plaintext_hash / save / forget` |

**`BlobTransferPort` 完整签名草案**:
```rust
#[async_trait]
pub trait BlobTransferPort: Send + Sync {
    // ---- 发布(D1)----
    async fn publish(&self, ciphertext: Bytes) -> Result<BlobDigest, BlobError>;
    async fn issue_ticket(&self, digest: &BlobDigest) -> Result<BlobTicket, BlobError>;

    // ---- 接收(D2)----
    async fn fetch(&self, ticket: &BlobTicket) -> Result<Bytes, BlobError>;

    // ---- 生命周期 ----
    async fn has(&self, digest: &BlobDigest) -> Result<bool, BlobError>;
    async fn tag(&self, digest: &BlobDigest, reason: TagReason) -> Result<(), BlobError>;
    async fn untag(&self, digest: &BlobDigest, reason: TagReason) -> Result<(), BlobError>;
}

/// Tag reason(引用计数的一部分),不预支业务,只定义稳定域:
pub enum TagReason {
    ClipboardEntry(ClipboardEntryId),
}
```

**domain 值对象**(2026-04-24 按 R2 方案 C 定稿,完整签名见 `slice3-phase1-plan.md §3.1`):
```rust
pub struct BlobDigest([u8; 32]);  // content-addressed identifier of ciphertext; adapter-computed
pub struct BlobTicket(Vec<u8>);   // opaque handoff token; uc-core never decodes
// 无值对象 digest() 方法 —— 走 BlobTransferPort::digest_of(ticket)
```

**进度事件(v1 先不做)**:iroh-blobs 返回 `AddProgress` / `DownloadProgress` stream,本 port 先用阻塞式 `publish/fetch`;真需要进度时另加 `BlobProgressPort::subscribe()`(技术债)。

**GC(v1 先不做)**:依赖 `TagReason` 引用计数,等清理策略成熟再加 `gc()` 方法或独立 `BlobRetentionPolicyPort`(技术债)。

---

### F1 · 启动(app 上电 / daemon 拉起)

> ⚠️ **已被 outside-in 取代**(2026-04-19 Slice 1 F1 定稿)。本节是 Port 总表时代的旧草图,用旧命名(`NodeIdentityStorePort` / `PresencePort::ensure_reachable`);Slice 1 最终草图见上面 **Slice 1 · Pairing E2E** 章节内 F1/F2 子节。本节保留作 Slice 2 反推参考(预连 / roster 相关 port 属 Slice 2 范围)。

**触发**:进程启动完成基础装配后

**前置分支**:
1. **未 initialize Space** → 不启网络,等 A1 完成后再触发 F1 核心部分
2. **已 initialize 但未 unlock** → **不启网络**(未解锁时无法解密入站,暴露在网上无业务价值)。等 A2 unlock 成功后再触发
3. **已 unlock** → 走下面核心流程

**核心业务步骤**(已 unlock 情境):
1. `NodeIdentityStorePort::load_or_generate()` → 32 字节 secret(`SecureStoragePort` 落盘)
2. `NetworkControlPort::start_network()`(iroh adapter 内部:bind Endpoint + 注册 ALPN handlers + 启 discovery)
3. `SpaceMemberRepo::list(space_id)` → 全体成员
4. 对每成员并发启动重连(不阻塞主流程):`PresencePort::ensure_reachable(member)`,adapter 内:
   a. 读 `PeerAddressRepositoryPort::get(member)` → `Option<PeerAddressCache>`
   b. 命中:用 last-known 拨号
   c. 未命中或失败:落 n0 DNS discovery
   d. 连通 → 发 `PresenceEvent::MemberConnected`,upsert `PeerAddressCache`
5. `PresencePort::subscribe()` 消费者(UI、C1 编排层)开始响应事件

**关键业务决策**:
- **预连式**:F1 启动后主动拨号每成员,维持活跃连接,UI 即时反映在线状态
- **阈值懒连**(SpaceMember 数 > 10)→ 技术债,v1 不做

**能力缺口**:
| 能力 | 已有? | 位置 / 缺口 |
|---|---|---|
| 本机 iroh identity 加载 / 生成 | ❌ | 新 port:**`NodeIdentityStorePort`** |
| 启/停网络运行时 | ✅ | `NetworkControlPort::start_network`(iroh 新 impl) |
| 主动拨号某 member | ❌ | `PresencePort::ensure_reachable`(E 组 port 扩展) |
| SpaceMember 列表 | ✅ | milestone |
| last-known NodeAddr | ✅ | `PeerAddressRepositoryPort`(E 组) |
| Presence 事件订阅 | ✅ | `PresencePort::subscribe`(E 组) |
| Unlock 状态查询 | ✅ | `SpaceAccessPort::is_unlocked` |

---

### F2 · 关闭(app 退出 / daemon SIGTERM)

**触发**:进程接到退出信号

**业务步骤**:
1. `NetworkControlPort::stop_network()`:
   a. 停止 accept 新入站
   b. 等 in-flight 会话完成(配置超时,默认 5s)
   c. 关闭 iroh Endpoint,触发所有 connection close
2. Flush 待持久化状态(PeerAddressRepository in-memory upsert)
3. 关闭 iroh-blobs FsStore 句柄
4. 发 `PresenceEvent::MemberDisconnected` 给订阅者

**能力缺口**:
| 能力 | 已有? | 位置 / 缺口 |
|---|---|---|
| 停止网络运行时 | ❌(方法) | `NetworkControlPort` 扩展 `stop_network()` |

---

## F 组汇总:**新增 port 雏形**

| Port | 服务 usecase | 签名雏形 |
|---|---|---|
| **`NodeIdentityStorePort`** | F1 | `load_or_generate() → NodeSecretBytes` / `reset() → ()` |
| **`NetworkControlPort`**(已有,扩展) | F1, F2 | 已有 `start_network` + 新增 `stop_network()` |
| **`PresencePort`**(E 组,扩展) | F1 | 加 `ensure_reachable(member)` |

**domain 值对象**:`NodeSecretBytes([u8; 32])` — 不暴露具体算法

---

### A1 · Initialize 新 Space(首次装机)

**触发**:用户首次启动 app,UI 引导填 passphrase + 设备名

**业务步骤**:
1. 用户输入 passphrase(明文)+ device_name
2. 生成新 SpaceId(uuid / 6 位 id)
3. `SpaceAccessPort::initialize(space_id, passphrase)` → `ActiveSpace`(milestone 已实现)
4. 生成本机 device_id(6 位)+ 持久化本机为 SpaceMember(owner)
5. `NodeIdentityStorePort::load_or_generate()` → 生成本机 iroh identity
6. 触发 F1 核心流程(已 unlock 分支)

**能力缺口**:无(F 组的 `NodeIdentityStorePort` 已覆盖)

---

### A2 · Unlock 已有 Space(app 启动 / 手动解锁)

**触发**:
- F1 启动时,检测到 `is_unlocked == false` + 已 initialize
- 用户在 UI 手动输入 passphrase

**业务步骤**:
1. 用户输入 passphrase
2. `SpaceAccessPort::unlock(space_id, passphrase)` → `ActiveSpace | WrongPassphrase`
3. 成功 → 触发 F1 核心(启网 + 重连成员)
4. 失败 → 上抛给 UI

**能力缺口**:无

---

### A3 · Revoke 成员

**触发**:用户 UI 选某设备"移除"

**业务步骤**:
1. `RevokeMemberUseCase(member_id)`(milestone 已有)
2. 从 `SpaceMemberRepo` 移除
3. 清理对该 member 的本机缓存:
   - `PeerAddressRepositoryPort::remove(member)`
4. 主动断开该 member 当前活跃的 iroh connection:
   - `PairingTransportPort::unpair_device(member_node_handle)`(milestone 保留,iroh 新 impl 里实现为"关闭所有对该 NodeId 的活跃 connection")
5. 发 `PresenceEvent::MemberDisconnected`

**广播策略**(被其他成员发现"我被踢了" / "他被踢了"):
- **v1 不做主动广播** —— 被撤销方重连会被拒(身份已不在 trusted_peer 表中,ClipboardReceiverPort 认证失败就关流)
- 其他成员从 `PairingTransportPort` 的 connection deny 事件间接感知
- 如果要做主动广播,需额外 control ALPN —— **技术债**

**能力缺口**:
| 能力 | 已有? | 备注 |
|---|---|---|
| Revoke usecase | ✅ | milestone `RevokeMemberUseCase` |
| 主动断该 member 活跃连接 | ✅(接口) | `PairingTransportPort::unpair_device`(iroh 新 impl 里实现) |
| 清理 PeerAddressCache | ✅ | E 组 port |

---

### A4 · Change Passphrase — **v1 不做**(与 milestone 对齐)

milestone 在 commit `450e0ee5`:*drop unused SpaceAccessPort::change_passphrase* —— 说明**当前产品没开改口令功能**。

**决议**:v1 **不实现** A4,与 milestone 保持一致。未来要做需:
- 重新在 `SpaceAccessPort` 加 `change_passphrase(old, new)`
- 重写所有 SpaceMember 的 keyslot(每个 keyslot 用新 KEK wrap)
- 广播"重新 unlock"指令给其他成员(他们下次启动要再输新口令)

**能力缺口**:无(v1 scope 外)

---

### A5 · 重命名本机 + 传播

**触发**:用户 UI 改本机设备名

**业务步骤**:
1. 更新本机 SpaceMember.device_name(本地 repo)
2. **传播策略**(重要业务决策):
   - **方案 1(选,v1)** · 被动传播:C1 outbound 的 header 每次带 `origin_device_name` → 对端 C2 inbound 见到就 upsert 对端 SpaceMember 表
     - 优点:不需新 ALPN,不需新 port
     - 缺点:改名后若不发剪贴板,对端显示旧名;但随下次同步会自动更正
   - 方案 2 · 主动广播:新建 `/uniclipboard/control/1` ALPN → 技术债 v1 不做

**能力缺口**:
| 能力 | 已有? | 备注 |
|---|---|---|
| 本机 SpaceMember 改名 | ✅ | milestone member_repo |
| C1 header 带 origin_device_name | 部分 | 旧 ClipboardMessage 已有,C 组新 header 需保留该字段 |
| 对端收到后 upsert | ✅ | C2 处理 header 时做 |

---

## A 组汇总:**无新 port**

| 变动 | 位置 |
|---|---|
| 复用 milestone `SpaceAccessPort`(initialize / unlock / is_unlocked / lock) | 0 改动 |
| 复用 milestone `RevokeMemberUseCase` | 0 改动 |
| `PairingTransportPort::unpair_device` | iroh adapter 新 impl(断连对端所有 connection) |
| C 组新 header schema 保留 `origin_device_name` | 纳入 C1 设计 |

**A 组 0 新 port 新增**——全部复用 B/C/D/E/F 已定义的能力或 milestone 现成 port。

---

## 🗺 依赖关系

```
Phase 0(已完成,2026-04-18)
        │
        ▼
[等 milestone/1.0.0 合入 dev] ◀── 外部阻塞
        │
        ▼
   Slice 1(Pairing E2E)
        │
        ▼
   Slice 2(Clipboard + F1 预连)
        │
        ▼
   Slice 3(Blob / Files)
        │
        ▼
   Slice 4(删除 libp2p 业务代码)
        │
        ▼
   Slice 5(后续优化)
```

- Phase 0 已完成
- Slice 1 阻塞于 milestone/1.0.0 合并
- **Slice 1→5 为严格线性**:每个 slice 是端到端业务交付,不适合并行(后续 slice 依赖前序建立的基础设施)
- 2026-04-24 调整:原 Slice 4"双栈并行验证 1-2 周"已取消,改为直接删除 libp2p 业务代码(GUI 进程旧路径已是空跑死代码,见 `findings.md` F-100)

---

## Daemon application 边界收口(2026-04-26 起)

**目标**:daemon / CLI / Tauri / HTTP / IPC 入口最终只面对 `uc-application` 暴露的应用模型和 facade,不直接认识 `uc-core`、`uc-infra`、`uc-platform`、`uc-app` 的领域模型、port、adapter、runtime。

**当前完成标准**:
- 外层入口不直接拿 core 领域模型当 API 输入输出。
- 外层入口不直接构造 usecase。
- 业务规则和 patch 合并规则收敛到 `uc-application`。
- daemon 每收一块都跑针对性测试和 `cargo check -p uc-daemon`。

### Phase D1 · daemon 成员/配对入口收口 — complete

**范围**:
- `/member/:device_id/sync-preferences`
- `/pairing/unpair`
- paired devices 查询路径

**结果**:
- `MemberRosterFacade` 现在提供字符串设备 ID + application 层成员偏好模型。
- daemon 成员接口不再直接调用 membership usecase。
- 成员偏好 patch 合并规则移入 `uc-application`。

**验证**:
- `cargo test -p uc-application facade::roster --lib`
- `cargo test -p uc-daemon api::member --lib`
- `cargo check -p uc-daemon`

### Phase D2 · daemon settings 入口收口 — complete

**范围**:
- `GET /settings`
- `PUT /settings`

**结果**:
- 新增 `uc-application::facade::settings::SettingsFacade`
- settings view / patch 应用层模型进入 `uc-application`
- settings patch 合并规则从 daemon 移入 `uc-application`
- daemon settings handler 不再直接构造 `CoreUseCases` 或持有 core `Settings`

**验证**:
- `cargo test -p uc-application facade::settings --lib`
- `cargo check -p uc-daemon`

**遗留**:
- `uc-daemon-contract` 仍依赖 `uc-core` 做 DTO 转换。daemon 当前不再使用这些转换,但外部 contract 彻底去 core 需要后续单独清理。

### Phase D3 · daemon device/me 入口收口 — complete

**范围**:
- `GET /device/me`

**结果**:
- 新增 `uc-application::facade::device::DeviceFacade`
- 本机设备名 trim / fallback 规则移入 `uc-application`
- daemon device handler 不再直接调用 `uc-app::CoreUseCases`
- daemon conversion 删除 `LocalDeviceInfo` 投影

**验证**:
- `cargo test -p uc-application facade::device --lib`
- `cargo check -p uc-daemon`
- `cargo test -p uc-daemon --lib`

### Phase D4 · daemon storage 入口收口 — complete

**范围**:
- `GET /storage/stats`
- `POST /storage/clear-cache`

**结果**:
- 新增 `uc-application::facade::storage::StorageFacade`
- 存储统计 view 和清缓存结果模型进入 `uc-application`
- 清缓存的目录遍历、删除和 freed bytes 计算规则移入 `uc-application`
- daemon storage handler 不再直接构造 `CoreUseCases`

**验证**:
- `cargo test -p uc-application facade::storage --lib`
- `cargo check -p uc-daemon`
- `cargo test -p uc-daemon --lib`

**注意**:
- `api::storage` 当前没有专门 daemon 单测,`cargo test -p uc-daemon api::storage --lib` 筛选到 0 个用例;本轮用 facade 单测 + daemon 全量 lib 测试覆盖。

### Phase D5 · daemon lifecycle 入口收口 — complete

**范围**:
- `GET /lifecycle/status`
- `POST /lifecycle/retry`

**结果**:
- 新增 `uc-application::facade::lifecycle::LifecycleFacade`
- lifecycle 状态 view 进入 `uc-application`
- retry 的状态推进规则移入 `uc-application`
- daemon lifecycle handler 不再直接构造 `CoreUseCases` 或引用 `uc-app` lifecycle state

**保留职责**:
- `/lifecycle/ready` 仍只打开 daemon 本地 clipboard gate 并通知 deferred services,没有应用状态读写。
- `/lifecycle/retry` 中打开 gate / notify deferred services 仍属于 daemon 进程控制,本轮只把 lifecycle 状态推进移入 application。

**验证**:
- `cargo test -p uc-application facade::lifecycle --lib`
- `cargo check -p uc-daemon`
- `cargo test -p uc-daemon --lib`

### Phase D6 · daemon 下一块收口 — in_progress

**候选优先级**:
1. `api/search.rs`:直接构造 core search query / error,范围较大。
2. clipboard workers:直接依赖 platform watcher / core snapshot,需要更完整的 application worker facade。
3. `entrypoint.rs` / `app.rs`:仍是装配根,需要后续把 daemon runtime 装配入口迁入 `uc-application` 或内部装配模块。
4. `api/encryption.rs` / `api/lifecycle.rs`:依赖 `uc-app` 旧用例,需要先判断是否搬迁用例还是新增 facade 包装。

**下一步选择**:先评估 `api/search.rs`,若改动范围过大,改收 `api/encryption.rs` / `api/lifecycle.rs` 这类较小入口。
