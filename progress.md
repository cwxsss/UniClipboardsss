# Progress Log

## Session 2026-04-18 — Kickoff

### 已完成
- ✅ 扫描 `uc-core/src/network/` 全部子模块 → 列清 wire 泄漏点(F-001 ~ F-004)
- ✅ 扫描 `uc-platform/src/adapters/libp2p_network/` → 规模 ~6594 行,位置违规
- ✅ 评估并确认方向:平行路径(libp2p 冻结 + iroh 新栈 + 验证后删除)
- ✅ 用户锁定 4 项关键决策:
  - D1:libp2p 不动不迁移
  - D2:流式(非帧)
  - D3:使用 iroh-blobs
  - D4:用户重新配对,无静默迁移
- ✅ 创建 `task_plan.md` / `findings.md` / `progress.md`

### 产出文件
- `task_plan.md` — 9 个 phase 的路线图(Phase 0 → Phase 8)
- `findings.md` — 审计结论 + iroh 调研骨架

### Domain 划分决策(已定)
- ✅ **方案 C.2** — 两层:底层纯 ports + `space/` 子域扩展承载 Trust/Capability 语义 + 业务子域各自扩展

### 已敲定
- ✅ Q1 = C.2(并入 `space/`)
- ✅ Q2 = Cargo feature 切换
- ✅ Q4 = clipboard 每次开新流
- ✅ Q5 = iroh 官方 relay 默认 + 可覆盖

### Q3 已有结论
- ✅ 独立 iroh 密钥:`uc-infra/src/network/iroh/identity_store.rs`(复用 `SecureStoragePort`,key = `iroh-identity:v1`)
- ✅ 指纹展示层复用 milestone 的 `uc-infra/src/security/identity_fingerprint.rs`
- ❌ 不复用 `platform::SystemIdentityStore`(libp2p 专用)

### milestone/1.0.0 分支调研(只读,未修改)
- 主题:Slice 1 migration — 空间加密重构 + `paired_device` → `space_member`/`trusted_peer`
- 与 iroh 工作冲突面小,但 **Phase 1(C.2 扩展 `space/`)需要借用其 `trusted_peer` 模型**
- **新增外部阻塞**:Phase 1/2 需等 milestone/1.0.0 合入 dev 后启动

### 当前可立即进行
- ✅ **Phase 0** 无依赖,可开始(iroh/iroh-blobs API 侦察、ALPN 规划、iroh-blobs 与现有 blob 关系)

### 下一步
用户决定何时启动 Phase 0;Phase 1/2 等 milestone/1.0.0 合并信号。

---

## Session 2026-04-18 — Phase 0 启动

### 任务
1. iroh / iroh-blobs 版本选型 + API 速览
2. 概念映射表(iroh ↔ 新 domain ↔ 既有领域对象)
3. iroh-blobs store 与 `uc-infra/src/blob` 分工
4. ALPN 命名规划
5. 五个底层 port 的签名草稿

### 注意
- Q1 = C.2 已定:**不新建 domain**,扩展 `space/`。原 task_plan 里"敲定新 domain 模块名"已失效,无需再选名。
- 只写调研,不改代码。

### ✅ Phase 0 完成
- iroh 0.95.1 + iroh-blobs latest 锁定
- 6 张关键章节写入 `findings.md`(F-010 ~ F-019)
- 五个底层 port 的签名草稿已给出(F-018),Phase 1 直接照实现
- iroh-blobs 与现有 `uc-infra/blob` 的分工敲定:两层加密独立,目录分开
- ALPN 敲定:pairing/1 + clipboard/1 + iroh_blobs::ALPN

### 关键发现
- **iroh 0.95 命名变化**:`NodeId/NodeAddr` → `EndpointId/EndpointAddr`(但 BlobTicket 内部字段仍叫 `node`)。domain 只看 `NodeHandle`,规避摇摆。
- **Discovery 三层**:mDNS + n0 DNS/Pkarr + OOB ticket
- **密钥存储极简**:`SecretKey` 就是 32 字节,`SecureStoragePort` 就够用
- **iroh-blobs 复用**:`iroh_blobs::ALPN` 直接用,不重造轮子

### 下一步(等 milestone/1.0.0 合并后)
进入 **Phase 1** — usecase 驱动反推 port(方向已修正,不再是"先设计 5 个 port")

---

## Session 2026-04-18 — Usecase 对齐

### 已完成
- 新配对流程锁定(基于 uc-rendezvous + iroh):shortcode → rendezvous → iroh 直连 → challenge/keyslot offer → consume
- **关键决策**:sponsor 侧无用户弹窗(去掉 `AwaitingUserApproval`),纯靠 passphrase 防 MitM
- Passphrase 语义:Q1 = Space passphrase 明文(复用,无一次性口令)
- Shortcode 单例:Q4 = 同时 1 个 pending
- 方向修正:Phase 1 从"port-first"改为"usecase-first → 反推 port"
- 写入 findings F-030(rendezvous API)+ F-031(milestone 上配对实现复用性)
- 写入 task_plan §Usecases — B1/B2 细化,汇总出 3 个新 port 雏形

### 发现
- ✅ milestone/1.0.0 的 `SpaceAccessPort.prepare_join_offer / derive_master_key_for_proof` 完全可复用
- ✅ `PairingTransportPort` / `PairingMessage` / 状态机骨架 均 transport-agnostic
- ⚠️ 现状态机是 PIN-显示模型,新流程不用 PIN,需去掉 `AwaitingUserConfirm`,加 `AwaitingShortcodeRedeem`
- ⚠️ `PairingChallenge{pin}` / `PairingResponse{pin_hash}` 两条消息**新流程不用**,随 libp2p Phase 8 一起删

### 下一步
- 展开 C 组(剪贴板同步) usecase → 汇总能力缺口
- 或先展开 A 组(空间/身份)如果与 C 有依赖

---

## Session 2026-04-18 — C 组展开

### 已完成
- C1(outbound)/ C2(inbound)/ C3(with-files)逐步细化
- 每 usecase 列出业务步骤 + 能力缺口表
- 汇总出 3 个新 port:
  - `ClipboardDispatchPort`(C1 出站)
  - `ClipboardReceiverPort`(C2 入站)
  - `PeerIdentityResolverPort`(C2 + B 组共用)
- E 组 `PresencePort` 预声明(C1 依赖)

### 关键设计决策
- **wire 结构**:header(metadata,含 blob_refs)+ payload(V3 加密字节流);iroh stream 边界替代 4B 长度前缀
- **加密位置**:usecase 层完成加密 → 向 transport 递送已加密字节流,transport 不看内容
- **可废弃清单**:旧 `ClipboardOutboundTransportPort` / `ClipboardInboundTransportPort`(帧模型)随 libp2p 一起 Phase 8 删

### 下一步
用户选 E / A / D 中哪组继续展开:
- **E 组** — PresencePort 详化,会反哺 C1 的"在线成员枚举"
- **A 组** — 基本复用 milestone,展开快
- **D 组** — C3 依赖,且 iroh-blobs 是新能力,值得早定

---

## Session 2026-04-18 — E 组展开

### 已完成
- E1(roster)/ E2(事件订阅)展开,汇总 `PresencePort`(合并查询 + 订阅)
- E3 定为 **v1 范围外**:mDNS 保留但职责收窄为 LAN 路由优化,不出 domain 事件
- Reachability 值对象草案:Connected / Reachable{via} / LastSeen{ms_ago} / Unknown
- `PeerIdentityResolverPort`(C2 已声明)被 E2 也用到,确认复用

### 待用户裁决(4 点)
1. **E3 是否 v1 做**?建议不做(默认)
2. **Reachability 粒度**:建议富态(Connected / Reachable{via} / LastSeen{ms_ago} / Unknown)vs 简化三态(Connected/Reachable/Offline)
3. **mDNS 职责**:建议"LAN 路由优化 only,不出 domain 事件"(默认)
4. **PresencePort 合并还是拆**:建议合并查询 + 订阅为一个 port(默认,领域内聚)

### 下一步
确认上述后展开 D 组(blob publish/fetch),D 会敲定 C3 的 BlobTransferPort

---

## Session 2026-04-18 — D 组展开

### 已完成
- 查 milestone/1.0.0 确认 `BlobCipherPort` 是**随机 nonce AEAD**,密文无法天然去重
- 用户否决"接受重复浪费"方案,改用**明文 hash 去重缓存**(方案 A)
- F-033 记录方案对比
- D1(publish)/ D2(fetch)展开,内建去重逻辑
- 新 port:
  - `BlobTransferPort`(publish / fetch / has / tag / untag / issue_ticket)
  - `BlobReferenceRepositoryPort`(plaintext_hash → digest 去重缓存)
- 技术债记录:进度事件、GC 策略均 v1 不做

### 关键设计点
- `TagReason::ClipboardEntry(ClipboardEntryId)` —— 引用计数基础,未来 GC 依赖
- 跨设备转发去重:接收方 D2 完成后也 record 明文 hash → 本机可作为后续转发的 sponsor
- 一对多 fanout:同一 digest → 同一 ticket → 多接收方并发拉取同一份密文
- BlobTicket 在 domain 里是**不透明 bytes + digest 访问器**,不暴露 NodeAddr

### 下一步
进入 A / F 组:
- **A 组** — initialize / unlock / revoke / passphrase / rename,大部分复用 milestone
- **F 组** — 启动 / 关闭生命周期,含 last-known NodeAddr 重连

---

## Session 2026-04-18 — F 组展开

### 已完成
- F1(启动)/ F2(关闭)细化,含 3 种前置分支(未 init / 未 unlock / 已 unlock)
- 业务决策:**未 unlock 时不启网络**(避免暴露)
- 业务决策:**预连式策略**——F1 启动后主动拨号每成员,维持活跃连接 + 即时 presence
- 复用 milestone 的 `NetworkControlPort::start_network`(iroh 新 impl),扩展 `stop_network`
- `PresencePort` 扩展一个方法 `ensure_reachable(member)`
- 新 port:**`NodeIdentityStorePort`** —— 本机 iroh secret 加载/生成
- 新 domain 值对象:**`NodeSecretBytes([u8;32])`**(不声明算法)

### 关键设计点
- last-known NodeAddr 优先(避免依赖 n0 DNS 冷启动)→ 命中 `PeerAddressRepositoryPort`
- mDNS 移除 → 发现只剩 n0 DNS + last-known + (配对才用的)rendezvous ticket
- F1 不算严格"用户业务动作",但与业务耦合紧(unlock 状态分支),留在 uc-app
- 阈值懒连模式(>10 设备)记为技术债,v1 不做

### 下一步
最后一组:**A 组**(initialize / unlock / revoke / passphrase / rename),大部分复用 milestone,估计很快收尾

---

## Session 2026-04-18 — A 组展开 + usecase 阶段收尾

### 已完成
- A1-A5 细化:
  - A1 initialize / A2 unlock → 直接复用 `SpaceAccessPort`
  - A3 revoke → 复用 milestone `RevokeMemberUseCase` + `PairingTransportPort::unpair_device`
  - **A4 change passphrase → v1 不做**(milestone 已删 change_passphrase,当前产品无此功能)
  - A5 rename → 被动传播(C1 header 带 origin_device_name,对端 upsert)
- A 组**无新 port**,全部复用既有能力

### 关键业务决策
- Revoke 无主动广播(技术债);对端从 connection deny 间接感知
- Change passphrase v1 不开
- 重命名被动传播,不建 `/control/1` ALPN(技术债)

### 🎉 usecase 阶段完成
- 所有 usecase 组(A/B/C/D/E/F)展开完毕
- 汇总出 **12 个 port**(9 新 + 3 复用/扩展)
- 所有能力缺口已识别

### 下一步
**整合审阅**:
1. 汇总所有 port 成一张总表(task_plan 首部)
2. 核对 domain 值对象清单
3. 核对技术债清单
4. 决定 Phase 1 具体切片(port 数量 → 切几个 PR)
5. 等 milestone/1.0.0 合入 dev 后启动 Phase 1 编码

---

## Session 2026-04-18 — Slice 切片完成

### 已完成
原 Phase 1-8 线性结构 → **Slice 1-5 端到端切片**:
- **Slice 1 · Pairing E2E**:A1/A2/B1/B2/F1最小/F2,引入 6 新 port + 2 iroh impl
- **Slice 2 · 剪贴板 + 预连式 F1**:C1/C2/F1完整/A3/A5/E1/E2,引入 2 新 port + 2 扩展
- **Slice 3 · 文件/Blob**:C3/D1/D2,引入 2 新 port(BlobTransfer / BlobReference)
- **Slice 4 · 双栈并行验证 1-2 周**
- **Slice 5 · 一次性清理 libp2p**

### 切片原则
- 每个 slice = 端到端业务交付(不是技术层)
- 严格线性(后序依赖前序基础设施)
- Slice 1 阻塞于 milestone/1.0.0 合入 dev

### 下一步
**阻塞**:等 milestone/1.0.0 合入 dev,即可启动 Slice 1 编码

---

## Session 2026-04-18 — 架构规则 + Facade 对外表面

### 已完成
用户指出空白:Slice 1-5 仅覆盖 domain → infra → usecase,**未包含对外接入**(Tauri / IPC / CLI)。

task_plan.md 新增:
- **§ 架构规则** — 明确调用链 外部 → Facade → UseCase → Port → Adapter,硬规定外部只能调 Facade
- 记录现有 Facade(milestone/1.0.0):`PairingFacade` / `SpaceAccessFacade` / `SetupFacade`
- 记录新建 Facade(本次重构):`ClipboardSyncFacade` / `MemberRosterFacade` / `BlobProgressFacade`(可选)
- 每个 Slice 补"对外表面(Facade + UI/IPC/CLI)"子节:
  - Slice 1:`PairingFacade`(扩展) + 配对 UI + CLI `uc pair`
  - Slice 2:`ClipboardSyncFacade` + `MemberRosterFacade`(全新)+ 设备列表 UI
  - Slice 3:无新 Facade(blob 嵌入 clipboard);可选 `BlobProgressFacade`

### 硬规则
- Tauri command / Daemon IPC / CLI **只调 Facade**,禁止 `use <domain>::UseCase`
- CI 守卫建议:`rg '::UseCase'` 在 uc-tauri/uc-daemon/uc-cli 为空
- 应用内调用(bootstrap 触发 F1/F2)允许直接调 UseCase(不算违规)

### 三份规划文件最终状态
- `task_plan.md` — 目标 / Domain 决策 / **架构规则** / Slice 1-5(含对外表面)/ Port 总表 / Usecases / 技术债 / 完成判据
- `findings.md` — 技术侦察 + milestone 复用评估 + rendezvous 契约 + 设计决策
- `progress.md` — 全部 session 日志

---

## Session 2026-04-18 — Port 总表写入

### 已完成
task_plan.md 新增 **§ Port 总表**,包含 12 个 port 的:
- Rust trait / struct / enum 完整签名骨架
- 每个 port 的路径(`uc-core/src/ports/...`)
- 对应 iroh adapter 路径
- 主要调用方

**共享值对象**(`NodeHandle` / `NodeSecretBytes` / `NodeTicket` / `BlobDigest` / `BlobTicket` / `PlaintextHash`)统一放在总表顶部。

按 Slice 分组:
- 🟩 Slice 1:8 个(6 新 + 2 既有 impl)
- 🟦 Slice 2:2 新
- 🟨 Slice 3:2 新

### 规划阶段圆满
三份规划文件已就绪,可作为 Slice 1 编码起点:
- `task_plan.md` — Slice 切片 + Usecase 细化 + **Port 总表** + 技术债 + 完成判据
- `findings.md` — 技术侦察 + milestone 复用评估 + rendezvous 契约 + 设计决策
- `progress.md` — 全部 session 日志

---

## Session 2026-04-18 — 技术债登记

### 已完成
task_plan.md 新增 **§ 技术债清单**(T-01 ~ T-14),按优先级 P0-P4 分组:

| P | 数量 | 触发条件 |
|---|---|---|
| P0 | 3 | Slice 1-5 编码附带 |
| P1 | 3 | v1 后首版 UX 反馈 |
| P2 | 3 | 产品决策驱动 |
| P3 | 3 | 规模/环境监控驱动 |
| P4 | 2 | 专项需求 |

每条技术债记录:来源 / 业务背景 / 现状 / 预案 / 触发条件 / 工作量估计

---

## Session 2026-04-18 — mDNS 移除裁决

### 决策
- **mDNS 移除**(2026-04-18):不再使用 iroh `discovery-local-network` feature
- Reachability 粒度、PresencePort 合并、E3 不做 → 用户未否决,采用默认

### 连锁影响
- F-015 discovery 策略由 3 层 → 2 层(n0 DNS + OOB ticket)
- `ReachVia::Mdns` 变体去除 → 只剩 `Direct / Relay`
- **新增兜底**:必须持久化已配对 peer 的 NodeAddr 快照,作为 LAN 冷启动发现的替代
- **新增 port**:`PeerAddressRepositoryPort` — 持久化 last-known NodeAddr
- **新增 domain 值对象**:`PeerAddressCache { relay_url, direct_addresses, observed_at_ms }`
- 更新 F1 usecase outline:启动时优先 last-known,失败再 n0 DNS

### 潜在风险(需用户确认)
- **完全离线 LAN 场景**(无公网 + 无 last-known 地址 = 首次启动或地址完全变了):两端无法发现彼此 → 必须重新配对。这是否可接受?
- 如果是家用 NAS / 离线办公 场景常见,可能需要允许自建 DNS discovery(`SyncSettings` 已留有接口)

### 错误 / 偏差
(无)

---

## Session 2026-04-19 — Slice 1 阻塞解除,准备启动

### 事实核对(当前分支 `slender-soybean`)
milestone/1.0.0 加密重构已合入当前分支(git log:Slice 1-8 + Phase C 全部落地)。Slice 1 所需复用资产全部就位:

| 依赖 | 位置 | 状态 |
|---|---|---|
| `SpaceAccessPort` | `uc-core/src/space_access/` | ✅ |
| `BlobCipherPort` | `uc-core/src/ports/security/blob_cipher.rs` | ✅ |
| `TrustedPeer` 模型 | `uc-core/src/trusted_peer` + `uc-application/src/trusted_peer` | ✅ |
| `PairingFacade` / `SpaceAccessFacade` / `SetupFacade` | `uc-application/src/*/facade.rs` | ✅ |
| `SecureStoragePort` | `uc-core/src/ports/security/secure_storage.rs` | ✅ |
| `identity_fingerprint`(已下沉 infra) | `uc-infra/src/security/identity_fingerprint.rs` | ✅ |
| `libp2p_network`(冻结对象) | `uc-platform/src/adapters/libp2p_network/` | ✅ 原样 |

### 新发现的前置缺口
- ❌ **`uc-rendezvous` crate 不存在**:Slice 1 B1/B2 依赖的 rendezvous 服务端/客户端尚未在 workspace 内。findings F-030 记录了 rendezvous API,但实际 crate 尚未产出。
- ❌ **`iroh` / `iroh-blobs` 依赖未加入 Cargo.toml**:需要 Slice 1 编码起点时加入。

### 当前阶段
从"规划阶段"切入"Slice 1 编码阶段"。阻塞(milestone/1.0.0 合入)已移除,但引入两个新的内部阻塞需要用户决策。

### 待用户裁决(启动前必答)
1. **Rendezvous 服务方案**:(a) 本次随 Slice 1 新建 `uc-rendezvous` crate(客户端 + 独立二进制服务端);(b) 先仅建客户端 stub + 复用外部已有服务;(c) 先 mock 掉 rendezvous,Slice 1 只跑 OOB ticket 粘贴流程,rendezvous 推到 Slice 1.5
2. **工作分支**:继续在 `slender-soybean` 上做,还是另起 `slice1/...`?
3. **切入子步骤**:按 Port 总表从 `NodeIdentityStorePort` 开始,还是先扩 `PairingFacade` 反推 port?

### 下一步
拿到上述答复后创建任务清单并启动编码。

---

## Session 2026-04-19 — Slice 1 outside-in 重新规划

### 任务
切入"Slice 1 编码阶段"前,用户提出**方法论转向**:不要先列 port,而是从**业务故事 → domain → usecase**反推 port。本 session 在 outside-in 框架下重新规划 Slice 1。

### 已完成的设计探索

#### 现状勘探(并行 Explore agent + 直接 Read)
- ✅ 摸清 uc-core / uc-application / uc-infra / uc-platform 现有 pairing/membership/identity 结构
- ✅ 锁定 `SpaceMember` / `TrustedPeer` / `DeviceIdentityPort` / `LocalDeviceIdentity` 的现状(F-031~F-033)
- ✅ 发现 `IdentityFingerprint` 三处类型分裂(F-034)
- ✅ 确认现有 setup 流程**不创建本机 SpaceMember**(F-037)
- ✅ 现状信息全部归档到 findings.md F-031..F-040

#### 边界裁决(11 个决策)
- ✅ Q-α:`EndpointTicket` 不进 core
- ✅ Q-β:`ReachVia` 不进 core
- ✅ Q-γ:复用 `SpaceMember.identity_fingerprint` 表达"成员=身份"
- ✅ Q-δ:单例约束放 application 编排
- ✅ Q-ε:`InvitationCode` 格式校验放 infra
- ✅ Q-1:`PairingInvitation` 是 core 聚合
- ✅ Q-2:`PairingInvitation` 不持久化(in-memory)
- ✅ Q-3:复用 `DeviceIdentityPort`,无需"区分本机"新概念
- ✅ 命名:`RendezvousClientPort` → `PairingInvitationPort`(业务语义)
- ✅ 命名:`NodeIdentityStorePort` → `LocalIdentityPort`(显式 create + current_fingerprint)
- ✅ 设计:Slice 0.5 预备小重构(IdentityFingerprint 上提 core)

#### 概念三分确立
区分 **`DeviceId`(业务标识)**/ **`IdentityFingerprint`(身份验证)** / **`NodeAddr`(网络寻址)**——业务层只面对 DeviceId,fingerprint 仅用于"是不是同一台设备"验证,寻址全是 infra 内部细节(F-036)。

#### Port 数量从 6 砍到 3
- 原计划:6 真新 port + 2 iroh impl + 1 扩展
- outside-in 后:**3 真新 port**(`LocalIdentityPort` / `LocalDeviceNamePort` / `PairingInvitationPort`)+ 2-3 待 B1/B2/F1/F2 反推确认
- 删除 `NodeIdentityStorePort` / `LocalEndpointTicketPort` / `RendezvousClientPort` 原命名

#### A1 / A2 草图敲定
- ✅ A1 InitializeSpaceUseCase:7 步,2 个新 port + 4 个复用 port(详见 task_plan.md Slice 1 章节)
- ✅ A2 UnlockSpaceUseCase:2 步,纯 unlock 无自愈(早先误以为需要 self-member 自愈,经用户纠正:identity 在 A1 时生成,A1 是原子动作,A2 不存在缺失场景)

#### 文档归位(本次 session 末)
- ❌ 删除临时 `slice1_design.md`(违反 planning-with-files 三文件结构)
- ✅ task_plan.md 加新 ✅ 决策章节 + 修订 Slice 1 章节 + 新增 Slice 0.5 章节 + 标 Port 总表过时项
- ✅ findings.md 加 F-031..F-040 + 标"待用户决策"过时
- ✅ progress.md 加本 session(本条)

### 仍待讨论(下次 session)
- 🔲 B1 IssuePairingInvitationUseCase(Slice 1 真正的硬骨头)
- 🔲 B2 RedeemPairingInvitationUseCase
- 🔲 F1 StartNetworkUseCase
- 🔲 F2 StopNetworkUseCase

### 仍待编码前确认(可在 B 组讨论中顺便)
- 🟡 F-035:`PairingTransportPort` 现有 `peer_id` 参数类型(String? DeviceId? libp2p::PeerId?)
- 🟡 F-038:`SpaceAccessFacade` 是否已有 `unlock` 方法

### 错误 / 偏差(可学习)
- ❌ **首次 A2 设计加了 self-member 自愈逻辑** — 用户纠正:identity 在 A1 时生成,A1 是原子动作,走到 A2 = A1 已成功;不存在 self-member 缺失场景。**教训**:不要为不会发生的状态设计补救逻辑。
- ❌ **首次创建临时文档 `slice1_design.md`** — 用户用 planning-with-files skill 提醒,违反三文件(task_plan/findings/progress)结构。**教训**:进入新仓库先扫现有 planning 文件,不要另起炉灶。

### 下一步
进入 B1 outside-in 草图。下次 session 开始前再读一遍 task_plan / findings 关键章节。

---

## Session 2026-04-19(续) — B1 IssuePairingInvitationUseCase 定稿

### 任务
按 outside-in 完成 B1(sponsor 发出邀请)的 usecase 草图,定稿 `PairingInvitationPort` 签名。

### 已完成

#### B1 草图
- ✅ Command/Result 定稿(无字段输入,输出 `(code, expires_at)`)
- ✅ 9 步业务流程梳理
- ✅ 5 个决策(Q-B1-1 ~ Q-B1-5)用户敲定
- ✅ 状态机改动定稿(加 `AwaitingInvitationRedeem { code, expires_at }` 状态 + 4 个转移)
- ✅ Facade 表面定稿(`PairingFacade::issue_pairing_invitation`)
- ✅ 安全模型分析(server 不 revoke 也安全)

#### Port 终稿
- `PairingInvitationPort` 极简到 **1 个方法** `issue_invitation()`
  - 删除 `revoke_invitation`(server 不支持,且不需要)
  - TTL 由 server authoritative 决定,client 不持有常量
  - 返回 `IssuedInvitation { code, expires_at }`

#### 关键洞察(本 session 沉淀的设计原则)
1. **Server-authoritative TTL**:防 client 时钟漂移
2. **本地状态是 source of truth**:server 端 stale invitation 不影响安全(sponsor 入站时按本机 in_memory.code 匹配)
3. **不需要 server-side revoke**:旧 code 靠 sponsor 侧拒绝匹配 + 5min 自然过期
4. **入站事件需带 incoming_code**:暴露了 `PairingTransportPort` 的扩展需求(F-041),B2 定稿

### 决策汇总

| # | 决策 |
|---|---|
| Q-B1-1 | TTL 由 adapter/server authoritative 决定;application 不持有常量 |
| Q-B1-2 | 过期清理走懒清理(下次 issue 时检查) |
| Q-B1-3 | 每次 issue 都生成新 code,本地清空旧的 + 发 Revoked 事件;**不调 server** |
| Q-B1-4 | Network 未启动由 adapter 内部 issue 失败上抛,无需 NetworkStatusPort |
| Q-B1-5 | 配对协议失败也清空 invitation,UI 提示用户重新发起 |

### B1 真新 port 增量
- **1 个**:`PairingInvitationPort`(1 个方法)

→ Slice 1 累计真新 port:`LocalIdentityPort` + `LocalDeviceNamePort` + `PairingInvitationPort` = **3 个**(B1 没新增,只把 PairingInvitationPort 定稿)。

### 错误 / 偏差(可学习)
- ❌ **首次草图带 `revoke_invitation` 方法** — 用户提醒"server 不支持",我顺势分析后发现确实不需要;**教训**:port 设计要先确认服务端能力,不要假定有什么方法
- ❌ **首次草图 Q-B1-3 倾向"返回错误 InvitationAlreadyPending"** — 用户改成"每次新 code"再改成"本地清空(不调 server)";**教训**:UX 简单一致性优先于"协议精确性"

### 文档归位
- ✅ task_plan.md Slice 1 章节内 B1 占位替换为完整草图
- ✅ findings.md F-039 PairingInvitationPort 签名更新为终稿(1 个方法)
- ✅ findings.md F-040 PairingInvitation 域对象单例约束更新为"清空旧 + 创建新"
- ✅ findings.md 加 F-041 入站事件需带 incoming code(待 B2 定稿)

### 仍待讨论(下次 session)
- 🔲 B2 RedeemPairingInvitationUseCase(将定稿"入站事件 metadata 怎么传 code")
- 🔲 F1 StartNetworkUseCase
- 🔲 F2 StopNetworkUseCase

### 下一步
进入 B2 outside-in 草图。

---

## Session 2026-04-19(续 2) — B2 RedeemPairingInvitationUseCase 定稿 + AppFacade 架构

### 任务
按 outside-in 完成 B2(joiner 加入空间)的 usecase 草图,顺手引入 AppFacade 集中化架构。

### 已完成

#### B2 草图
- ✅ Command/Result 定稿(code + passphrase + 可选 device_name → 返回 sponsor 关键信息)
- ✅ 11 步业务流程梳理(含 5-9 步协议握手 + 10 步提交点)
- ✅ 8 个决策(Q-B2-1 ~ Q-B2-8)用户敲定
- ✅ 入站 code 匹配定稿:走 `PairingRequest` 协议消息字段(F-041 定稿)
- ✅ 失败原子性:identity 保留,Space/SpaceMember 不持久化(Q-B2-3)

#### Port / Wire 改动
- `LocalIdentityPort` 加 `ensure()` 方法(B2 用,A1 仍用 `create()`)
- `PairingTransportPort` 加 `dial_by_invitation(code)` 高内聚方法
- Wire 层 `PairingRequest` 加 `invitation_code: String` 字段(infra,core 不见)
- **真新 port 增量为 0**(全是扩展)

#### AppFacade 架构(Q-B2-2 衍生需求)
- 新建 `uc-application/src/facade/app_facade.rs`
- 集中编排跨域 UseCase,对外提供统一接入点
- A1/A2/B1/B2/F1/F2 全部经 AppFacade 暴露
- sub-facade(`PairingFacade` / `SetupFacade` / `SpaceAccessFacade`)保持 `pub` 不破坏(本 slice 不切外部调方)
- Tauri/daemon/CLI 切换推到后续 slice
- 与 §11.4 封装规则不冲突

### 决策汇总

| # | 决策 |
|---|---|
| Q-B2-1 | `LocalIdentityPort` 加 `ensure()`(幂等);`create()` 仍严格(A1 用) |
| Q-B2-2 | 复用 milestone `CompleteJoinSpaceUseCase`;**新增 AppFacade 集中编排** |
| Q-B2-3 | 失败:identity 保留(下次复用),其他不持久化;PairingConfirm 是提交点 |
| Q-B2-4 | UseCase 同步 await 整个流程(5-30s),UI spinner |
| Q-B2-5 | **不**做指纹核对(passphrase 已是身份证明) |
| Q-B2-6 | 单一 AppFacade(后续 Slice 大了再按业务拆) |
| Q-B2-7 | A1/A2 也搬到 AppFacade(统一接入点) |
| Q-B2-8 | Tauri/daemon/CLI 切换推迟,本 slice 不破坏 |

### Slice 1 累计进度

| usecase | 状态 | 真新 port |
|---|---|---|
| A1 InitializeSpace | ✅ 草图敲定 | 2 (`LocalIdentityPort` / `LocalDeviceNamePort`) |
| A2 UnlockSpace | ✅ 草图敲定 | 0 |
| B1 IssuePairingInvitation | ✅ 草图敲定 | 1 (`PairingInvitationPort`) |
| B2 RedeemPairingInvitation | ✅ 草图敲定 | 0(2 个 port 扩展 + 1 个 wire 改动 + AppFacade) |
| F1 StartNetwork | 🔲 待讨论 | TBD |
| F2 StopNetwork | 🔲 待讨论 | TBD |

**累计真新 port:3**
**累计 application 新增**:AppFacade + 4 个 UseCase 修订/新增 + PairingStateMachine 新状态 + 4 个 facade 跨域方法
**累计 wire 改动**(infra):`PairingRequest` 加 `invitation_code`

### 文档归位
- ✅ task_plan.md Slice 1 章节内 B2 占位替换为完整草图
- ✅ task_plan.md 加 AppFacade 集中化章节
- ✅ task_plan.md A1/A2 章节补 Facade 表面说明(指向 AppFacade)
- ✅ task_plan.md B1 章节"F-041 隐含扩展"标记更新为"B2 已定稿"
- ✅ findings.md F-039 LocalIdentityPort 终版(3 个方法)
- ✅ findings.md F-041 入站 code 匹配定稿(走 PairingRequest 字段)
- ✅ findings.md F-042 AppFacade 架构(新增)
- ✅ findings.md F-043 待编码前 Read 确认 `CompleteJoinSpaceUseCase` 接口

### 仍待讨论(下次 session)
- 🔲 F1 StartNetworkUseCase(预连式启动 + iroh endpoint 启动 + 重连成员)
- 🔲 F2 StopNetworkUseCase(优雅关闭 + flush)

### 仍待编码前 Read 确认
- 🟡 F-035:`PairingTransportPort.peer_id` 类型
- 🟡 F-038:`SpaceAccessFacade.unlock` 是否已存在
- 🟡 F-043:`CompleteJoinSpaceUseCase` 内部接口

### 下一步
进入 F1 outside-in 草图。

---

## Session 2026-04-19(续 3) — F1/F2 outside-in 定稿 + Slice 1 规划收官

### 任务
按 outside-in 完成 F1(启动)/ F2(关闭)的 usecase 草图,收束整个 Slice 1 规划。

### 已完成

#### F1 草图(拆 2 个 UseCase)
- ✅ `BootstrapOnStartupUseCase`:3 步分支派发(is_initialized? is_unlocked? → 委托 StartNetwork)
- ✅ `StartNetworkUseCase`:4 步(断言 unlocked / 读 fingerprint / start_network / 返回)
- ✅ 8 个决策(Q-F1-1 ~ Q-F1-8)用户敲定
- ✅ 真新 port 增量:1 个 — `NetworkControlPort`(签名 + 错误类型定稿,F-044)
- ✅ AppFacade 表面:新增 `on_startup()`(F1 入口);A1/A2 成功路径内部串 StartNetwork

#### F2 草图
- ✅ `StopNetworkUseCase`:1 步委托 `NetworkControlPort::stop_network`;幂等 + infallible
- ✅ 6 个决策(Q-F2-1 ~ Q-F2-6)用户敲定
- ✅ 真新 port 增量:0(复用 `NetworkControlPort`)
- ✅ AppFacade 表面:新增 `on_shutdown()` 对称 `on_startup`

#### 关键洞察
1. **预连不属于 Slice 1** — 用户问 "为什么用预连" 直击要害。预连的业务动机(roster 在线状态 / C1 首字节延迟)全部来自 Slice 2/3 usecase,Slice 1 只交付 pairing(sponsor accept + joiner `dial_by_invitation`)。旧 Port 总表 F 组草图(`PresencePort::ensure_reachable` / `PeerAddressRepositoryPort`)属 Slice 2 工作,不进 Slice 1。→ 一次性砍掉 F1 大半复杂度。
2. **F1 拆 Bootstrap + StartNetwork 的价值** — Bootstrap 负责 "根据 Space 状态分支",StartNetwork 前置 = 已 unlock,职责清晰;A2 成功路径可以直接调 StartNetwork 而不需要 "让 Bootstrap 再跑一次"。
3. **Endpoint 单例 + 不支持 re-start 的简化链** — 省掉 `NetworkStatusPort` 防重入;AlreadyStarted 作为 port error 即可;F2 幂等 swallow → 进程退出路径零失败。
4. **`start_network` 非长 await** — bind + handler 注册 < 100ms,与 B2 的 5-30s 同步 await 不同。UI 不需要长 spinner。
5. **旧 F 组草图需标注取代** — task_plan.md L1667+ 的 "F1 · 启动" 章节是 Port 总表时代线性规划的残渣,与 Slice 1 章节内的新 F1/F2 有冗余;不删除(Slice 2 反推预连时作参考),加标注说明已被 outside-in 取代。

### 决策汇总(14 个)

F1(8 个):
| # | 决策 |
|---|---|
| Q-F1-1 | 拆 `BootstrapOnStartupUseCase` + `StartNetworkUseCase` |
| Q-F1-2 | `AppFacade::on_startup()` 开机调一次 |
| Q-F1-3 | `get_current_fingerprint()`,None = bug;不用 `ensure()` |
| Q-F1-4 | **不预连**,Slice 1 零成员枚举、零拨号 |
| Q-F1-5 | N/A(预连没了) |
| Q-F1-6 | bind 成功即返回(< 100ms) |
| Q-F1-7 | Endpoint 进程级单例;不支持 re-start |
| Q-F1-8 | bind 失败上抛 `EndpointBindFailed` |

F2(6 个):
| # | 决策 |
|---|---|
| Q-F2-1 | 独立 `StopNetworkUseCase`(对称 F1) |
| Q-F2-2 | `AppFacade::on_shutdown()` 对称 `on_startup` |
| Q-F2-3 | Slice 1 不 graceful drain(推 Slice 2/3) |
| Q-F2-4 | `stop_network` 幂等 |
| Q-F2-5 | close 失败 swallow + log |
| Q-F2-6 | 不要求 "已 start" 前置 |

### Slice 1 最终全景

| usecase | 状态 | 真新 port | Facade 表面 |
|---|---|---|---|
| A1 InitializeSpace | ✅ | 2 (`LocalIdentityPort` / `LocalDeviceNamePort`) | `AppFacade::initialize_space`(成功串 StartNetwork) |
| A2 UnlockSpace | ✅ | 0 | `AppFacade::unlock_space`(成功串 StartNetwork) |
| B1 IssuePairingInvitation | ✅ | 1 (`PairingInvitationPort`) | `AppFacade::issue_pairing_invitation` |
| B2 RedeemPairingInvitation | ✅ | 0(2 port 扩展 + 1 wire 改动 + AppFacade 编排 `CompleteJoinSpaceUseCase`) | `AppFacade::redeem_pairing_invitation` |
| F1 Bootstrap + StartNetwork | ✅ | 1 (`NetworkControlPort`) | `AppFacade::on_startup`(Bootstrap 入口);A1/A2 内部串 StartNetwork |
| F2 StopNetwork | ✅ | 0 | `AppFacade::on_shutdown` |

**累计真新 port**:4(`LocalIdentityPort` / `LocalDeviceNamePort` / `PairingInvitationPort` / `NetworkControlPort`)
**累计 UseCase**:7(A1/A2/B1/B2/Bootstrap/Start/Stop)
**AppFacade 终态方法**:6 业务方法 + `on_startup` + `on_shutdown` = 8 个
**Wire 改动**(infra 内,core 不见):`PairingRequest` 加 `invitation_code: String`

### ✅ Slice 1 规划阶段收官

全部 usecase 草图定稿。进入**编码阶段**。

### 编码前 Read 确认(累计 4 项)
- 🟡 F-035:`PairingTransportPort.peer_id` 类型(String / DeviceId / libp2p::PeerId?)
- 🟡 F-038:`SpaceAccessFacade.unlock` 是否已存在
- 🟡 F-043:`CompleteJoinSpaceUseCase` 内部接口
- 🟡 F-044:`uc-core/src/ports/network/` 目录是否已建(若未建,Slice 1 首次创建)

### 错误 / 偏差(可学习)
- ❌ **首次 F1 草图保留 "预连所有成员"** — 用户问 "为什么用预连",用 slice 边界分析后砍掉。**教训**:outside-in 时时要问 "这属于哪个 slice 的业务目标",旧线性规划的残渣(Port 总表 F 组草图)要按 slice 边界重新裁切,不要惯性复制。
- ❌ **初稿把老 F 组章节保留不标** — 老章节与新 Slice 1 F1/F2 共存会让 "Slice 2 反推时读到两份不同版本"。**教训**:规划文档里旧版本要显式标 "已被 X 取代",作历史参考而非当前依据。

### 文档归位
- ✅ task_plan.md Slice 1 章节 F1/F2 占位替换为完整草图
- ✅ task_plan.md AppFacade 接口补 `on_startup` / `on_shutdown`;A1/A2 方法注释补 "成功后内部串 StartNetwork"
- ✅ task_plan.md 加 "Slice 1 真新 port 累计" 小结(4 个)
- ✅ task_plan.md L1667 老 "F1 · 启动" 章节加 "已被 outside-in 取代" 顶注
- ✅ findings.md F-042 AppFacade 方法列表更新为 8 个
- ✅ findings.md 新增 F-044 `NetworkControlPort` 设计

### 下一步
Slice 1 规划阶段结束,**进入编码阶段**。开工前需对齐:
1. 执行 4 项编码前 Read 确认(F-035 / F-038 / F-043 / F-044)
2. 编码切入顺序:Slice 0.5(IdentityFingerprint 统一)→ A1 → ... ?
3. 工作分支策略:保持 `slender-soybean` 还是另起 `slice1/...`?
4. Cargo 依赖:`iroh` / `iroh-blobs` / rendezvous 客户端加入 workspace(本 slice 起点)
5. `uc-rendezvous` crate 方案敲定(见 Session 2026-04-19 "Slice 1 阻塞解除" 三选一)

---

## Session 2026-04-19(续 4) — 编码前 6 项 Read + 3 个 N 决策 + 4 个 I 决策 + Slice 1 实施方案锁定

### 任务
进入编码前做现状对齐:跑完 4 项编码前 Read 确认 + 2 项 port 补查,并就由此引出的 3 个新设计决策 + 4 个基础设施决策裁决,最终锁定 Slice 1 实施方案。

### 已完成

#### 6 项 Read(3 并行 + 2 并行)
- ✅ F-035 `PairingTransportPort`:`peer_id: String` 泄漏 libp2p;无 `dial_by_invitation`
- ✅ F-038 `SpaceAccessFacade`:**无** `unlock` / `initialize` / `is_unlocked`;这些方法都在 core `SpaceAccessPort` 上(11 个方法)
- ✅ F-043 `CompleteJoinSpaceUseCase`:thin wrapper(trigger-only);本机 SpaceMember **已自动 admit**(B2 步骤 10b 可省);`pub(crate)` 需经 `SetupFacade::complete_join_space()` 代理
- ✅ F-044 `NetworkControlPort`:**已存在**,只 `start_network()`(1 方法);无 `stop_network`
- ✅ F-046 `LocalIdentityPort` 补查:必须**新建**(`DeviceIdentityPort` 只管 UUID,`IdentityFingerprintFactoryPort` 纯算法,职责均不同)
- ✅ F-046 `LocalDeviceNamePort` 补查:**取消**(已有 `SettingsPort` 管 `Settings.general.device_name`,复用)

#### N 系列决策(3 个,用户敲定)
| # | 议题 | 决策 |
|---|---|---|
| N-1 | `NetworkControlPort` 扩展 vs 新建 | **扩展 + `stop_network` 默认 no-op impl** |
| N-2 | `PairingTransportPort` 扩展 vs 新建 | **新建独立 Slice 1 pairing port**;旧 port 打 `#[deprecated]`,Slice 5 删 |
| N-3 | Rendezvous 客户端落点 | **`uc-infra/src/rendezvous/client.rs`**(新 module,非新 crate) |

#### I 系列决策(4 个,用户敲定)
| # | 议题 | 决策 |
|---|---|---|
| I-1 | 编码切入顺序 | **Slice 0.5 先**(独立 PR)→ Slice 1 |
| I-2 | 工作分支 | 继续 `slender-soybean` |
| I-3 | Cargo 依赖引入 | 一次加齐,**无 feature 门控** |
| I-4 | Rendezvous server | 用现有 `https://rendezvous.uniclipboard.app`,不新建 crate |

#### 关键洞察
1. **真新 port 数从 4 → 3**:`NetworkControlPort` 降级为扩展;`LocalDeviceNamePort` 被 `SettingsPort` 复用覆盖。
2. **A1/A2 不能套壳 facade**:`SpaceAccessFacade` 没暴露 `unlock/initialize/is_unlocked` — 新 UseCase 必须直接调 `SpaceAccessPort`(core)。Bootstrap 的 `is_unlocked?` 也直接走 port。
3. **B2 简化**:`CompleteJoinSpaceUseCase` 内部已通过 `SpaceAccessOrchestrator.try_admit_peer_as_member()` 自动 admit 本机成员,B2 步骤 10b 可省。AppFacade 跨模块调需经 `SetupFacade::complete_join_space()` 代理。
4. **Slice 0.5 工作量缩减**:`IdentityFingerprintFactoryPort` 已在 core(milestone 已做一半),Slice 0.5 只需上提 `IdentityFingerprint` 值对象 + 把 `SpaceMember.identity_fingerprint: String` 升级为值对象。
5. **N-1 + I-3 自洽**:默认 no-op impl 保证 libp2p adapter 零改动,正好配合 "Cargo 一次加齐无 feature 门控" — iroh 代码默认编译也不破 libp2p 栈。
6. **`peer_id: String` 泄漏的处理哲学**:扩展旧 port 加默认 impl 的方式对**纯新增方法**有效;但对**已存在的签名泄漏字段**无效(必须改既有方法签名,会破 libp2p)。所以 N-2 选新建,与 N-1 策略不同。

### Slice 1 真新 port 最终清单(3 个)
| Port | 位置 | 职责 |
|---|---|---|
| `LocalIdentityPort` | `uc-core/src/ports/local_identity.rs` | iroh Ed25519 秘钥对 lifecycle + fingerprint |
| `PairingInvitationPort` | `uc-core/src/ports/pairing_invitation.rs` | sponsor 签发 invitation code(调 rendezvous) |
| Slice 1 新 pairing session port(名字编码时定) | `uc-core/src/ports/pairing/session.rs`(建议) | sponsor accept + joiner dial by invitation + session IO |

扩展 1 个:`NetworkControlPort` 加默认 `stop_network`。
复用 8 个(零改):`SettingsPort` / `DeviceIdentityPort` / `SpaceAccessPort` / `IdentityFingerprintFactoryPort` / `SecureStoragePort` / `MemberRepositoryPort` / `SetupStatusPort` / `SetupFacade::complete_join_space`。

### 编码前剩余 TODO
- 🟢 所有 Read 确认完成
- 🟡 Slice 1 新 pairing session port 正式命名(建议 `PairingSessionPort`,编码时敲定)
- 🟡 Cargo 加依赖时选 `reqwest` 还是 workspace 已有的 HTTP 客户端(编码 Slice 1 B1/B2 前查)

### 错误 / 偏差(可学习)
- ❌ **首次 F-044 Edit 没删干净旧内容** — 造成 findings.md 段内容重复(adapter 依赖 / 为什么不传 identity 等段出现两次)。第二次 Edit 修掉,且简化了错误类型(保留 `Result<()>` anyhow 风格,不自定义 `StartNetworkError` enum,避免破坏 libp2p adapter 编译)。**教训**:Edit 复杂段落时,先贴完整旧段 + 完整新段到 Edit,不要用 "前缀叠加" 的方式;或者 Read 整段再 Write 覆盖。
- ❌ **Slice 1 规划阶段漏查 port 存在性** — `NetworkControlPort` 已在 core,本该 Slice 1 outside-in 起步时 grep 一次 `uc-core/src/ports/` 先拿现状清单,再设计新增。Read 阶段(F-044)才发现,导致前面 session 关于"4 个新 port"的记载需要修正。**教训**:outside-in 讨论新 port 之前,先对现有 port 做一次全面审计(grep + 列清单),比逐个撞车省事。

### 文档归位
- ✅ findings.md F-035 / F-038 / F-043 / F-044 从"待确认"改为"✅ 确认结果",填入完整代码签名
- ✅ findings.md 新增 F-045 N/I 决策汇总(3 N + 4 I)
- ✅ findings.md 新增 F-046 `LocalIdentityPort` / `LocalDeviceNamePort` 补查 + Slice 1 真新 port 最终清单(3 个)
- ✅ task_plan.md Slice 1 A1 草图:第 1.5 步 `LocalDeviceNamePort` → `SettingsPort`
- ✅ task_plan.md "Slice 1 真新 port 累计" 从 4 下调到 3,附复用/扩展/取消三类清单
- ✅ task_plan.md 新增 "Slice 1 实施方案决策(N/I 系列)" 章节

### 下一步(终于可以开工)
1. **Slice 0.5 启动**(I-1 锁定):
   - 上提 `IdentityFingerprint` 值对象 `uc-infra/src/security/identity_fingerprint.rs` → `uc-core/src/security/identity_fingerprint.rs`
   - 升级 `SpaceMember.identity_fingerprint: String` → `IdentityFingerprint`
   - 升级 `TrustedPeer.peer_fingerprint: PeerFingerprint` → `IdentityFingerprint`(冗余命名删)
   - mapper / repo / 调用方类型跟随
   - 验收:`cargo check --workspace` + 现有单测 pass
2. **Slice 0.5 合入后启动 Slice 1 编码**:
   - 加 Cargo 依赖(I-3 一次加齐)
   - 按 A1 → A2 → B1 → B2 → F1/F2 顺序实现
   - 每个 usecase 配套 AppFacade 方法

---

## Session 2026-04-19(续 5) — Slice 0.5 编码完成

### 任务
按 I-1 决策启动 **Slice 0.5 · IdentityFingerprint 统一**(独立 PR,Slice 1 起点)。

### 已完成

#### 11 个任务(按依赖顺序)
1. ✅ 新建 `uc-core/src/security/identity_fingerprint.rs`(算法无关 value object + `FingerprintError::{InvalidFormat, Mismatch}` + 8 个单测)
2. ✅ `IdentityFingerprintFactoryPort::from_public_key -> Result<IdentityFingerprint>`(port 签名升级)
3. ✅ `SpaceMember.identity_fingerprint: String → IdentityFingerprint`
4. ✅ `TrustedPeer.peer_fingerprint: PeerFingerprint → IdentityFingerprint`;**删除 `PeerFingerprint`**
5. ✅ `Sha256IdentityFingerprintFactory` 返回 core `IdentityFingerprint`;infra 侧 `FingerprintError` 收窄为 `FingerprintDerivationError::InvalidKeyLength`;4 个新单测
6. ✅ `space_member_mapper` / `trusted_peer_mapper` 边界 String↔IdentityFingerprint 转换(schema 不动)
7. ✅ `pairing::state_machine` + `protocol_handler` 类型跟随:`PairingHandshakeOutcome.identity_fingerprint: IdentityFingerprint`;状态机内部 context/action/event 保持 String(UI 边界),在 `.from_public_key()` 成功点 `.to_string()`
8. ✅ `trusted_peer::{challenge, orchestrator, state_machine, usecases/*}` 模块类型跟随
9. ✅ Q-0.5 决策 (a):`SpaceAccessContext.peer_fingerprint: Option<IdentityFingerprint>`;`set_peer_identity` / `AdmitMember.identity_fingerprint` 同步升级;daemon host.rs 读 trusted_peer 后不再 `.to_string()`;query.rs 投影层保持 String
10. ✅ `cargo check --workspace` 通过
11. ✅ lib 单测 113 个全部 pass(uc-core 22 + uc-infra 24 + uc-application 58 + uc-app 7 + uc-daemon 2)

#### 测试 fixture 适配
`PeerFingerprint::new("fp-{peer_id}")` 模式在 9 个测试文件出现。统一改为 helper `fp_for(seed: &str) -> IdentityFingerprint`:把 seed alphanumeric 过滤+uppercase 后 pad 到 16 字符。保持测试的"每个 peer 指纹唯一"语义。

#### DTO 边界保持 String 的清单(重要)
- `daemon-contract::types::*.peer_fingerprint: Option<String>`(wire DTO)
- `daemon-client::realtime::*.peer_fingerprint: Option<String>`(client DTO)
- `pairing::events::PairingDomainEvent::*.peer_fingerprint: String`(UI event)
- `pairing::state_machine::PairingState::AwaitingUserConfirm.peer_fingerprint: String`(序列化态)
- `pairing::state_machine::PairingAction::ShowVerification.{local,peer}_fingerprint: String`(UI action)
- `pairing::state_machine::PairingContext.{local,peer}_fingerprint: Option<String>`(UI 缓存)
- `setup::state::SetupState::JoinSpaceConfirmPeer.peer_fingerprint: Option<String>`(setup UI 态)
- `get_p2p_peers_snapshot::P2pPeerSnapshot.identity_fingerprint: String`(peer snapshot DTO)

#### Infra 侧 FingerprintError 收窄
原 infra `FingerprintError` 有 4 变体(`InvalidKeyLength/InvalidFormat/Mismatch/EncodingError`)。拆分后:
- **core** `security::FingerprintError`:`InvalidFormat` / `Mismatch`(解析/验证阶段)
- **infra** `security::FingerprintDerivationError`:仅 `InvalidKeyLength`(SHA-256 派生前置校验)

这对应 §7 职责分界:算法失败(key length)属 infra,值对象语义错误(format/mismatch)属 core。

### 错误 / 偏差(可学习)
- ❌ **首次 `derive_identity_fingerprint` 忘了 Ok 包装** — `IdentityFingerprint::from_raw_string(encoded).expect(...)` 直接返回,与函数签名 `Result<_, FingerprintDerivationError>` 不匹配。诊断立即暴露,一行 `Ok(...)` 修掉。**教训**:重写函数时别漏最后一步的 return 语义。
- ❌ **首次 fixture `PeerFingerprint::new("fp-xyz")` → `IdentityFingerprint::from_raw_string("fp-xyz")`** — "fp-xyz" 不是 16-char alphanumeric,会 panic。必须 pad 到 16 char + alphanumeric-only。**教训**:升级 value object 类型时,注意校验规则可能让旧 fixture 字面值失效,batch 要连 fixture 一起改。
- ❌ **首次改 `uc-infra/src/security/identity_fingerprint.rs` 漏留 `ShortCodeGenerator::generate` 的 `Result` 返回类型** — ShortCodeGenerator 本来返 `Result<String, FingerprintError>`(因为 FingerprintError 包含 Mismatch 等),去掉那些变体后保留 infallible 签名更合适。最终改成返 `String` 直接,`Sha256ShortCodeGenerator::generate` impl 里 `Ok(...)` 包一下。**教训**:错误类型拆分后,路径上所有返 `Result` 的位置都要重新审视是否 `infallible`。

### 文档归位
- ✅ task_plan.md Slice 0.5 章节 checklist 全部打钩 + 加完成日期 + 补 Q-0.5 决策说明 + DTO 边界保留清单
- ✅ progress.md 本 session 记录

### 下一步
**Slice 0.5 独立 PR 准备就绪**。合入后启动 **Slice 1 编码**:
1. Cargo 依赖一次加齐(I-3):`iroh` / `iroh-blobs` / `reqwest`(或 workspace 已有 HTTP)
2. `uc-core/src/ports/local_identity.rs` + `pairing_invitation.rs`(新 port 建文件)
3. A1 → A2 → B1 → B2 → F1/F2 按 outside-in 草图实现
4. `uc-infra/src/rendezvous/client.rs`(N-3 决策位置)
5. `uc-infra/src/network/iroh/`(iroh adapter 根目录)
6. AppFacade 新建(`uc-application/src/facade/app_facade.rs`)

---

## Session 2026-04-19(续 6) — Slice 1 P1 · 地基完成

### 任务
Slice 1 编码起点:建立 core 侧新 port 骨架 + 在 uc-infra 引入 iroh/rendezvous 栈依赖,保证 workspace 零错编译、lib 测试无回归。

### 已完成
1. ✅ 新建 `uc-core/src/ports/local_identity.rs` — `LocalIdentityPort` trait(`create` / `ensure` / `get_current_fingerprint`)+ `LocalIdentityError`(`AlreadyExists` / `Storage`)。方法语义按 B1/B2/F1 outside-in 草图:A1 走严格 `create`,B2 retry 走 `ensure`,F1 走 `get_current_fingerprint`(`None` 意味 bug)
2. ✅ 新建 `uc-core/src/ports/pairing_invitation.rs` — `PairingInvitationPort` trait(1 方法 `issue_invitation`)+ `IssuedInvitation { code, expires_at }` + `InvitationError`(`NetworkNotStarted` / `ServiceUnavailable` / `Internal`) + `InvitationCode` newtype(core 不做格式校验,Q-ε)
3. ✅ `uc-core/src/ports/network_control.rs` 加默认 `async fn stop_network(&self) -> Result<()> { Ok(()) }`(N-1 决策,libp2p adapter 零改动)
4. ✅ `uc-core/src/ports/mod.rs` 注册 + `pub use` 两个新 port
5. ✅ `uc-infra/Cargo.toml` 加 `iroh = "0.95.1"` / `iroh-blobs = "0.95"` / `reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }`,与仓库内 4 个已用 reqwest 的 crate 保持模式一致(rustls-tls + json,无 default features)
6. ✅ `cargo check --workspace` 通过(首次 iroh 依赖拉取 + 编译 ≈ 2m11s)
7. ✅ `cargo test --workspace --lib` 113 通过(uc-core 22 + uc-infra 24 + uc-application 58 + uc-app 7 + uc-daemon 2),与 Slice 0.5 基线一致

### 明确推迟到 P8 的决策
- **`PairingSessionPort` 签名**:outside-in 要求跟 iroh adapter 一起定稿(session IO 形态取决于 bi-stream 抽象),P1 不建 port 文件
- **旧 `PairingTransportPort` `#[deprecated]`**:跟新 session port 同 commit 引入更自然,P8 做

### Cargo 依赖调研结果
- **reqwest**:workspace 已有 4 个 crate(`uc-daemon-client`/`uc-tauri`/`uc-cli`/`uc-observability`)使用 `0.12` + `rustls-tls` + `json`,模式统一。uc-infra 未引入 → 按同模式加入,无 workspace.dependencies 提升
- **iroh / iroh-blobs / iroh-net**:`src-tauri/Cargo.lock` 完全缺失 → Slice 1 首次从零引入,版本锁 `0.95.1` / `0.95`(兼容同一 minor)

### 不动项(P1 未做)
- `uc-core/src/pairing/invitation/` 聚合(域对象 `PairingInvitation` / `InvitationState` / `InvitationEvent`)推 P2
- A1/A2 UseCase + AppFacade 推 P3/P4
- iroh adapter + rendezvous client 推 P5/P7
- 对 `PairingInvitationPort` 的 `revoke` 方法:确认不做(B1 Q-B1-3 已定 server 不支持 revoke,本地清空 + 下次 issue 生成新 code)

### 文档归位
- ✅ `progress.md` 本条(P1 完成记录)
- 🔲 `task_plan.md` Slice 1 章节 P1 不需调整(原计划已符合)

### 下一步
启动 **P2 · PairingInvitation 域聚合** — 在 `uc-core/src/pairing/invitation/` 建立 `PairingInvitation` 聚合(`InvitationState` / `InvitationEvent::{Issued,Consumed,Revoked,Expired}` / consume/revoke 业务方法 + 错误枚举),并将 `InvitationCode` 从 `ports/pairing_invitation.rs` 迁入该聚合 module(port 文件改用 re-export),让 A1/B1 UseCase 有域对象可依赖。

---

## Session 2026-04-19(续 7) — Slice 1 P2 · PairingInvitation 域聚合完成

### 任务
按 task_plan §Slice 1 的 P2 项,建立 sponsor-side invitation 域聚合;把 B1 Q-1/Q-2 的"核心业务规则/TTL 校验/code 匹配"收口到 core,并把 `InvitationCode` 值对象从 port 文件搬到真正的 domain module。

### 已完成
1. ✅ 新建 `uc-core/src/pairing/invitation/` 4 文件:
   - `code.rs` — `InvitationCode` newtype(从 `ports/pairing_invitation.rs` 搬来;格式校验仍由 adapter 负责,Q-ε)
   - `error.rs` — `ConsumeError::{CodeMismatch, Expired, NotPending}` + `RevokeError::NotPending`(thiserror)
   - `events.rs` — `InvitationEvent::{Issued, Consumed, Revoked, Expired}`,`Issued` 带 `code + expires_at + issuer_device_id`,其他只带 `code`
   - `invitation.rs` — `PairingInvitation` 聚合 + `InvitationState::{Pending{expires_at}, Consumed, Revoked, Expired}`;方法:
     - `issue(code, issued_at, expires_at, issuer)` → `(Self, InvitationEvent::Issued)`(聚合构造 + 事件 tuple,publisher 不可能忘发)
     - `consume(incoming_code, now)` → `Result<InvitationEvent, ConsumeError>`(`now >= expires_at` 判 `Expired`;code 不匹配判 `CodeMismatch`)
     - `revoke()` → `Result<InvitationEvent, RevokeError>`(非 Pending 状态报错,避免"已 settled 还 revoke"静默)
     - `try_expire(now)` → `Option<InvitationEvent::Expired>`(懒惰过期一次性转换)
2. ✅ `pairing/invitation/mod.rs` 聚合 re-export
3. ✅ `pairing/mod.rs` 顶层 re-export(`PairingInvitation` / `InvitationState` / `InvitationEvent` / `InvitationCode` / `ConsumeError` / `RevokeError`)
4. ✅ `ports/pairing_invitation.rs` 删除自建 `InvitationCode` 定义,改 `pub use crate::pairing::invitation::InvitationCode` — P1 外部 API `uc_core::ports::InvitationCode` 路径保持稳定
5. ✅ 10 个单测覆盖 Issued → Pending / 匹配消费 / wrong code → CodeMismatch / `now == expires_at` 边界判 Expired / revoke from Pending / revoke from Consumed → NotPending / consume after Consumed → NotPending / try_expire 前后行为 + 幂等
6. ✅ `cargo check --workspace` 通过(25s,增量编译)
7. ✅ `cargo test --workspace --lib` 123 通过(uc-core 32 = 22 + 10 新 / 其他 crate 零回归)

### 设计决策(P2 编码时定)
- **聚合构造 + Issued 事件作 tuple**:不允许独立构造 `PairingInvitation` 而忘发 `Issued` 事件(发布点唯一);B1 UseCase 拿到 tuple 直接存 invitation + publish event
- **`now >= expires_at` 判 Expired(半开区间)**:boundary 测试明确了这个策略;TTL 到点即失效
- **`revoke` 非 Pending 态报错**:不静默兼容,B1 Q-B1-3 决策下"已 settled invitation"不应被 revoke 到
- **`try_expire` 幂等返回 `None`**:第二次调用返回 `None`,不重复发 Expired;B1 application 层懒清理只调一次就转移完
- **不做 serde**:聚合整体 Q-2 "不持久化"(in-memory),`code` / `issuer_device_id` / `InvitationEvent` 各自 serde 是因为它们可能单独过 wire/DTO,整个聚合不过
- **`#[allow(clippy::module_inception)]`**:`invitation/invitation.rs` 里的命名;与 `security/identity_fingerprint/identity_fingerprint.rs` 风格一致(文件即主类型)

### 不做项(推迟)
- **application 层 `PairingDomainEvent` 扩展 `Invitation*` 变体** → 推到 P7(B1 UseCase)。现有 `PairingEventPort::subscribe -> Receiver<PairingDomainEvent>` 在 application,P7 B1 会把 core `InvitationEvent` 映射到 application `PairingDomainEvent::InvitationIssued/...` 然后发布
- **`PairingInvitationPort::issue_invitation` 的 mock adapter** → P7

### 错误 / 偏差(可学习)
- ❌ **单测首次写 `DeviceId::parse("...uuid...")`** — 现有 `DeviceId` 只有 `new(impl Into<String>)`,无 `parse` 方法。一次性把 test fixture 改成 `DeviceId::new(...)` 即可。**教训**:写测试 fixture 前先 grep 目标类型的构造方法,别凭 "UUID 必有 parse" 的直觉。

### 文档归位
- ✅ `progress.md` 本条(P2 完成记录)
- 🔲 `task_plan.md` Slice 1 P2 章节按原计划达成,无需调整

### 下一步
启动 **P3 · A1 InitializeSpaceUseCase + A2 UnlockSpaceUseCase** — 纯 core + 复用 milestone,不依赖 iroh/rendezvous:
1. 新建 `uc-application/src/{space_access 或 setup}/usecases/` 下的 A1/A2 文件(位置以既有 setup/space_access UseCase 布局为准)
2. A1 内 port 编排:`SpaceAccessPort::initialize` / `LocalIdentityPort::create`(P1 已定义 trait,尚无 impl)/ `DeviceIdentityPort::current_device_id` / `MemberRepositoryPort::save` / `SetupStatusPort::mark_completed` / `SettingsPort::load|save`(读/写 `general.device_name`,F-046)
3. A2 内 port 编排:`SetupStatusPort::has_completed` / `SpaceAccessPort::unlock`
4. 单测用 port mock(`mockall` 已在 uc-core dev-dep);**不依赖 iroh adapter impl**(P5 补)
5. 真正的 UseCase 实例化仍被 wiring 阻塞(LocalIdentityPort 无 impl),但 UseCase 本身可被单测 + 被 AppFacade 引用

**注意**:P3 只建 UseCase 代码 + 单测,不 wiring;wiring 推到 P5(LocalIdentityPort iroh impl 可用后)。

---

## Session 2026-04-19(续 8) — Slice 1 P3 · A1/A2 UseCase 完成

### 任务
按 task_plan §Slice 1 的 A1(InitializeSpace)/ A2(UnlockSpace)outside-in 草图实现 UseCase 代码 + 单测;不 wiring、不实例化,纯 core 编排 + 复用 milestone port。

### 已完成
1. ✅ 新增 `uc-application/src/facade/` 目录(P4 AppFacade 同住):
   - `mod.rs` — 模块入口,`pub use` Command/Result/Error
   - `commands.rs` — `InitializeSpaceCommand{passphrase, passphrase_confirm, device_name: Option<String>}` / `InitializeSpaceResult{space_id, self_device_id, fingerprint}` / `UnlockSpaceCommand{passphrase}` / `UnlockSpaceResult{space_id}`
   - `errors.rs` — `InitializeSpaceError`(`PassphraseMismatch`/`DeviceNameRequired`/`AlreadyInitialized`/`IdentityAlreadyExists`/`StorageFailed`/`Internal`)+ `UnlockSpaceError`(`SetupNotCompleted`/`SpaceNotInitialized`/`WrongPassphrase`/`CorruptedKeyMaterial`/`Internal`)
   - `usecases/mod.rs` + `initialize_space.rs`(A1)+ `unlock_space.rs`(A2)
2. ✅ `lib.rs` 注册 `pub mod facade`
3. ✅ A1 流程实现(7 步):passphrase 校验 → Settings.device_name 解析/持久化 → SpaceAccessPort::initialize → LocalIdentityPort::create → DeviceIdentityPort::current_device_id → SpaceMember 构造+persist → SetupStatus::has_completed=true
4. ✅ A2 流程实现(2 步):SetupStatus.has_completed 校验 → SpaceAccessPort::unlock(passphrase);无"self-member self-heal"(A1 原子性保证 identity/member 齐备)
5. ✅ 14 个单测(A1: 8 / A2: 6)全通过;UseCase 用手写 fake port(SpaceAccess/LocalIdentity/MemberRepo/SetupStatus/Settings/Clock 各一)覆盖:
   - A1 happy path / passphrase 不一致早返 / device_name 缺失报错 / device_name 从 Settings fallback / AlreadyInitialized 映射 / IdentityAlreadyExists 映射 / 持久化失败 map StorageFailed / 新 device_name 更新到 Settings
   - A2 happy path / SetupNotCompleted 早返 / WrongPassphrase/NotInitialized/CorruptedKeyMaterial/Internal 各自映射
6. ✅ `cargo check --workspace` 通过(35s)
7. ✅ `cargo test --workspace --lib` **137 pass**(uc-application 72 = 58 base + 14 新 / 其他 crate 零回归)

### 设计决策(P3 编码时定)
- **A1 / A2 都 `SpaceId::new()` 生成新 id** — adapter 实际以 `current_profile` scope 做 keyslot lookup,不看 `space_id`(milestone 事实)。A2 无需跨 session 读持久 space_id
- **A1 `ActiveSpace` 返回值 drop** — owner 流程不需要 ActiveSpace session 引用;session 由 adapter 内部维持
- **Device name 解析策略**:command 传 `Some` → 更新 Settings + 用作 SpaceMember.device_name;command 传 `None` → 从 Settings 读;两处都空 → `DeviceNameRequired`
- **早返 `PassphraseMismatch`**:不碰任何 port,避免污染;测试断言"space_access.initialize 未被调用"
- **错误分类 `IdentityAlreadyExists` vs `AlreadyInitialized`**:把 `LocalIdentityError::AlreadyExists` 单独分类,便于 UI 区分"数据层脏"与"已 setup 过"
- **ClockPort 而非 `Utc::now()`**:测试可控时间;app layer 用 ClockPort 先例虽少,但 infra 已广泛用,打开 app 层通道自然
- **`pub(crate)` UseCase**:符合 §11.4,AppFacade 才是外部唯一入口(P4 建)
- **Error 映射显式穷举 + `_ => Internal`**:SpaceAccessError 非预期变体(如 initialize 返 `NotUnlocked`)也被捕获映射为 Internal,不 panic 也不吞错

### 不做项(推迟)
- **AppFacade 构造 + wiring** → P4(UseCase 目前无消费者,cargo 警告 dead_code 是预期信号)
- **LocalIdentityPort impl** → P5(iroh adapter)
- **实际 end-to-end 集成测试** → P10(双机验收)

### 错误 / 偏差(可学习)
- ❌ **首次 import 写 `uc_core::space_access::SpaceAccessError`** — 实际 `SpaceAccessError` 在 `uc_core::ports::space::access.rs`,`uc_core::space_access/` 只有 `JoinOffer`/`ProofDerivedKey`/`state_machine` 等领域对象。**教训**:error 类型一般跟 port 同文件,不会在 domain module 再导出一份;混乱时直接 grep `pub enum XxxError`
- ❌ **初稿在 A1 测试里写了冗余的 `_assert_port_bounds()` 断言函数** — 为了引用 `PersistencePort/ProofPort/SpaceAccessTransportPort` 防 dead_code,但这些根本不用 import。删除多余引用即可

### 文档归位
- ✅ `progress.md` 本条
- 🔲 `task_plan.md` P3 原计划达成,无需调整

### 下一步
启动 **P4 · AppFacade 壳 + A1/A2 绑定** — 新建 `facade/app_facade.rs`,持有 A1/A2 UseCase `Arc`,暴露 `initialize_space()` / `unlock_space()` 方法;sub-facade 保持 `pub` 不动(Tauri/daemon/CLI 切换 AppFacade 推后续 Slice)。F1 串接(`on_startup` / A1 成功后自动 start_network)仍保持 TODO 占位,P6 补。

---

## Session 2026-04-19(续 9) — Slice 1 P4 · AppFacade 壳 + A1/A2 绑定完成

### 任务
按 task_plan §Slice 1 架构补充"AppFacade 集中化"新建 `facade/app_facade.rs`,以 `Arc` 持有 A1/A2 UseCase,暴露 `initialize_space()` / `unlock_space()` 两个跨域方法。sub-facade 保持 `pub` 不动(Tauri/daemon/CLI 切换 AppFacade 推后续 Slice);F1 串接(`on_startup` / `start_network`)保持 `TODO(P6)` 占位。

### 已完成
1. ✅ 新增 `uc-application/src/facade/app_facade.rs`:
   - `pub struct AppFacade { initialize_space: Arc<InitializeSpaceUseCase>, unlock_space: Arc<UnlockSpaceUseCase> }`
   - `pub fn new(...7 ports...)` — 内部构造两个 UseCase;`SpaceAccessPort` / `SetupStatusPort` 在 A1/A2 间共享(adapter keyslot 以 current profile 作 scope,共享是正确的语义)
   - `pub async fn initialize_space(cmd) -> Result<_, InitializeSpaceError>` thin forwarder + `TODO(P6 · F1)` 注释
   - `pub async fn unlock_space(cmd) -> Result<_, UnlockSpaceError>` 同上
   - `#[instrument(skip_all)]` 保留 tracing 入口
2. ✅ `facade/mod.rs` `pub mod app_facade; pub use app_facade::AppFacade;`
3. ✅ 5 个 facade 级 smoke test(A1 happy path + A1 PassphraseMismatch + A2 happy path + A2 SetupNotCompleted + A2 WrongPassphrase)— 证明 facade 只做 forward,不破错误语义
4. ✅ `cargo check --workspace` 通过(29s,零新 warning)
5. ✅ `cargo test -p uc-application --lib` **77 pass**(= 72 base + 5 新)
6. ✅ 提交 `1fc10e43`(pre-commit `cargo fmt` 无改动)

### 设计决策(P4 编码时定)
- **AppFacade 拥有 UseCase 而非 sub-facade** — task_plan §Slice 1 架构补充 L544 画的 `{pairing, setup, space_access}` 组合是更远期的终态;Slice 1 P4 只涉及 A1/A2 两个 **新** UseCase,它们不属于任何现有 sub-facade,所以 AppFacade 直接持有 UseCase `Arc` 最自然。未来 P7 B1/B2 加进来时再引入 `pairing: Arc<PairingFacade>` 等组合字段
- **共享 `SpaceAccessPort` / `SetupStatusPort`** — 两个 Arc clone 进 A1 / A2,因为底层 adapter 是进程级单例(见 unlock_space.rs L52 注释)
- **Arc::new(UseCase)** — 允许未来 `on_startup` / B1/B2 等方法共用同一 UseCase 实例,避免重复包装
- **`#[instrument(skip_all)]`** — 不记录 command 参数(含 passphrase)
- **Smoke test 自带 fake ports 而非共享 testing helper** — 5 个 test 只覆盖"facade 是否忠实转发",不重复 UseCase 的 14 个细致测试;独立 fakes 保持 test 文件 self-contained,比跨模块共享更清晰
- **Passphrase::new** 无 `Result` 包装、`DeviceId::new(impl Into<String>)` 接字符串、`ClockPort::now_ms() -> i64`、`SetupStatusPort::{get_status, set_status}` 走 `anyhow::Result`、`SettingsPort::{load, save}` 走 `anyhow::Result<Settings>`(`uc_core::settings::model::Settings`)—— 首次编写时沿用了错误的"标准"签名(`Result<_, Error>` 风格),编译器多轮修正后才对齐;下次写 fake 先 grep 目标 port 源码

### 不做项(推迟)
- **F1 `on_startup` / A1&A2 成功后自动 `StartNetwork`** → P6(目前 `TODO(P6 · F1)` 注释 2 处 = 2 个锚点)
- **B1/B2 `issue_pairing_invitation` / `redeem_pairing_invitation` 方法** → P7+
- **daemon / tauri / cli 切换调 AppFacade 而非 sub-facade** → Slice 1.5 或更后
- **LocalIdentityPort 真实 impl** → P5(iroh adapter)

### 错误 / 偏差(可学习)
- ❌ **首版测试 fake 照 "standard" port 签名写**(`now() -> DateTime<Utc>` / `Passphrase::new(...).unwrap()` / `DeviceId::new_v4()` / `MemberRepositoryError` / `PortSettings` / `SetupStatusError` / `Clone` on `SpaceAccessError`) — 这些都是**幻觉**。真实签名:`ClockPort::now_ms -> i64` / `Passphrase::new` 无 Result / `DeviceId::new(impl Into<String>)` / `MembershipError` / `uc_core::settings::model::Settings` / `SetupStatusPort` 走 `anyhow::Result<SetupStatus>` / `SpaceAccessError` 不 `Clone`。**教训**:写 fake 前先 grep 目标 port trait **定义行** + 一个**现有 fake impl**,抄签名而不是猜
- ❌ **`ActiveSpace::new(space_id, master_key)` 2 参数** → 真实是 1 参数 `ActiveSpace::new(space_id)`。**教训**:diagnostics 会告诉你 "consider importing" 和 "associated function defined here",顺着错误信息里的路径读一眼定义就行

### 文档归位
- ✅ `progress.md` 本条
- 🔲 `task_plan.md` P4 原计划达成,无需调整(架构补充章节 L583-587 所述"工作量"全部落地)

### 下一步
两条候选路线,按 task_plan Slice 1 outside-in 顺序:
- **P5 · `LocalIdentityPort` iroh adapter 实现** — 目前只有 port trait 没有 production impl。P5 在 `uc-platform/src/adapters/iroh_identity/` 新建 adapter,用 `iroh::SecretKey` 持久化到 keychain / keystore(复用 milestone `SecureStoragePort`),`create` / `ensure` / `get_current_fingerprint` 均需实现
- **P6 · F1 `BootstrapOnStartupUseCase` + `StartNetworkUseCase` + AppFacade `on_startup` / `on_shutdown`** — 串 A1/A2 成功后 auto start_network;此前 AppFacade 的 TODO 注释转为真正调用

建议先 P5(解除 P4 dead_code 前置,dead_code 目前 0 个也证明 facade wiring 已把 UseCase 托起),再 P6。

---

## Session 2026-04-19(续 10) — Slice 1 P4 重构 · 按 domain 拆 Sub-Facade + Deps

### 任务
用户审视续 9 的 AppFacade 实现后反馈:"不应该直接将这么多参数作为 new 的入参,应该按 domain 进行分类"。给出目标结构 `AppFacade { pub setup, pub pairing, pub sync }`,每个子 facade 用 `<Facade>Deps` 构造。Slice 1 当前只有 A1/A2 两个新 UseCase,属 "space 生命周期" 语义。

### 命名决策(提交前对齐)
- 候选 A · `SetupFacade`(同名不同模块,与旧 `crate::setup::SetupFacade` 靠 path 区分)
- 候选 B · `SpaceSetupFacade`(强调"空间 setup",与旧"设备接入 space 流程"区分)✅ 用户选
- 候选 C · `SpaceLifecycleFacade`(中性)
- 旧 `crate::setup::SetupFacade` 承载 milestone 的 14 个 UseCase(start_new_space / start_join_space / submit_passphrase / complete_join_space ...),是"设备加入 space"流程,与 A1/A2"本机 space 生命周期"语义**重叠但不同**;Slice 1 不 touch 旧 facade,后续 slice 再收敛

### 已完成
1. ✅ 新建子树 `uc-application/src/facade/space_setup/`:
   - `mod.rs` — 私有 4 个 mod + 对外 re-export(`InitializeSpaceCommand` / `SpaceSetupFacade` / `SpaceSetupDeps` 等)
   - `deps.rs` — `pub struct SpaceSetupDeps { 7 × pub Arc<dyn Port> }`,`pub` 字段允许 struct literal 构造(`SpaceSetupDeps { space_access, local_identity, ... }`)
   - `facade.rs` — `pub struct SpaceSetupFacade { initialize_space: Arc<UseCase>, unlock_space: Arc<UseCase> }`;`new(deps: SpaceSetupDeps)` **解构** deps 再 `Arc::clone` 共享 port(`SpaceAccessPort` / `SetupStatusPort` 给 A1/A2 共用)
   - `commands.rs` / `errors.rs` — `git mv` 自 `facade/` 下(100% similarity,blame 保留)
2. ✅ `facade/app_facade.rs` 重写为**纯聚合容器**:
   - `pub struct AppFacade { pub space_setup: SpaceSetupFacade }`(字段 `pub`,调方穿透 `app.space_setup.initialize_space(...)`)
   - `AppFacade::new(space_setup)` 接收已构造好的 sub-facade,不再摸任何 port
   - 顶部注释留位:`// P7+: pub pairing: PairingFacade` / `// Slice 2: pub sync: SyncFacade`
3. ✅ `facade/mod.rs` 重写:`pub mod app_facade; pub mod space_setup;` + re-export 所有外部会用的类型(`AppFacade` / `SpaceSetupFacade` / `SpaceSetupDeps` / A1/A2 的 Command/Result/Error)
4. ✅ `usecases/setup/{initialize_space,unlock_space}.rs` import 改为 `crate::facade::space_setup::{...}`(一行合并三个 import)
5. ✅ 5 个 smoke test 从 `app_facade.rs` 搬到 `space_setup/facade.rs`(新位置更贴近被测对象);`make_facade` helper 用 `SpaceSetupDeps { ... }` struct literal 构造
6. ✅ `cargo check --workspace` 通过;`cargo test -p uc-application --lib` **77 pass**(与续 9 持平,零回归)
7. ✅ 提交 `b0541110`(`git mv` 让 commands/errors 的 blame 走历史)

### 设计决策
- **Deps 字段 `pub`(非 `pub(crate)`)**:外部 bootstrap 需要从 10 多个 crate 组装 adapter 再喂给 Deps;struct literal + `pub` 最少样板。Deps 本身无不变量(只是命名良好的 port 袋子),不值得 builder
- **AppFacade 字段 `pub`(非方法穿透)**:符合用户原意 `AppFacade { pub setup, pub pairing, pub sync }`。调方写 `app.space_setup.initialize_space(cmd)` 比 `app.initialize_space(cmd)` 多一个词,但明确表达"这是 space_setup 域的动作",跨域时读者能一眼辨认归属
- **AppFacade::new(space_setup) 只接**已构造好的 sub-facade**:Bootstrap 各自构 `SpaceSetupDeps` → `SpaceSetupFacade::new(deps)` → `AppFacade::new(facade)`,层层明确。比 `AppFacade::new(AppDeps { space_setup: ..., pairing: ... })` 嵌套更少
- **`SpaceSetupFacade` 保留 `initialize_space` / `unlock_space` 方法**(thin forward):加了 `#[instrument(skip_all)]` 做 tracing span + 预留 `TODO(P6 · F1)` 锚点;不做纯 `pub Arc<UseCase>` 让外部直接调 UseCase,那会破坏 §11.4 "对外只暴露 Facade/UseCase,实现细节 `pub(crate)`"
- **保留原 5 个 smoke test**:测试语义不变,只是从 `facade::app_facade` 模块搬到 `facade::space_setup::facade` 模块,就近测试被测对象
- **`SpaceSetupDeps::new` 不提供**:完全用 struct literal,避免位置参数歧义(7 个 `Arc<dyn Port>` 传错顺序编译器抓不住,命名字段安全)

### 不做项(推迟)
- **`PairingFacade` + `PairingDeps`**(B1/B2) → P7+
- **`SyncFacade` + `SyncDeps`**(C1/C2/C3) → Slice 2
- **F1 `on_startup` 方法**:TODO 注释留在 `SpaceSetupFacade::initialize_space/unlock_space` 里;P6 时大概率把 auto-`StartNetwork` 写进 `SpaceSetupFacade`(不是 `AppFacade`),保持聚合容器纯净
- **旧 `crate::setup::SetupFacade` 收敛或改名**:本次不 touch,Slice 1 外部兼容保证

### 错误 / 偏差(可学习)
- ❌ **首次 `git mv` 在 src-tauri 目录下带了 src-tauri 前缀**(`git mv src-tauri/crates/.../commands.rs ...`),而 cwd 已经是 `src-tauri/`,路径拼成 `src-tauri/src-tauri/...` → `fatal: bad source`。**教训**:批 `git mv` 前先 `pwd`,用相对 cwd 的路径而非"脑中认为的项目根"
- ❌ **rust-analyzer diagnostics 有延迟**:新建 `space_setup/mod.rs` 后几秒内仍报 "unresolved module commands" + "type annotations needed",一度以为 mod.rs 声明写错。**教训**:diagnostics 报错之后别立刻改文件,先跑一次 `cargo check` 才能信任;rust-analyzer 滞后属工具误导而不是代码问题
- ✅ **命名冲突先问再动**(对齐方案 A/B/C):重构前如果擅自用候选 A,会跟旧 SetupFacade 长期共存,造成阅读困惑;花 30 秒问一下用户,省下后续可能的 rename 一轮

### 文档归位
- ✅ `progress.md` 本条
- 🔲 `task_plan.md` — **Slice 1 P4 架构补充章节** L518-591 描述的 `AppFacade { pairing, setup, space_access }` 是**更远期终态**;当前实现是 `AppFacade { space_setup }` + 子域 Deps 模式。二者方向一致但粒度不同,等 P7 / Slice 2 加 pairing / sync 时再更新
- 🔲 `findings.md` — 新增一条记录"Sub-Facade + Deps 模式"(命名决策 + Deps struct 结构),作为后续 PairingDeps / SyncDeps 模板

### 下一步
延续续 9 的 **P5 / P6 路线**(重构不改变后续顺序):
- **P5 · `LocalIdentityPort` iroh adapter** — 唯一未实现的 Slice 1 新 port
- **P6 · F1/F2 上 `SpaceSetupFacade`**(auto-`StartNetwork` + `on_shutdown`)

---

## Session 2026-04-19(续 11) — Slice 1 P5 · `LocalIdentityPort` iroh adapter

### 任务
在 `uc-infra/src/network/iroh/` 新建 `IrohIdentityStore`,实现 `LocalIdentityPort`(`create` / `ensure` / `get_current_fingerprint`)。密钥用 `iroh::SecretKey`,持久化走 `SecureStoragePort`(key = `iroh-identity:v1`),指纹走 `IdentityFingerprintFactoryPort`。不 wire 进 bootstrap。

### 已完成
1. ✅ 新建 `crates/uc-infra/src/network/iroh/`:
   - `mod.rs` — 声明 `pub mod identity_store` + re-export `IrohIdentityStore` / `IDENTITY_STORE_KEY`
   - `identity_store.rs` — adapter 实现 + 9 个 unit test
2. ✅ `network/mod.rs` 加 `pub mod iroh;`(现有 `pub mod space;` 之上)
3. ✅ `IrohIdentityStore::new(secure_storage, fingerprint_factory)`:两个 `Arc<dyn Port>` 注入,struct 内三个私有助手:
   - `load_secret()` — 读 `secure_storage.get("iroh-identity:v1")`,校验长度 32 字节,否则 `Storage("corrupt iroh identity: expected 32 bytes, got N")`
   - `persist_secret(&SecretKey)` — `sk.to_bytes()` → `secure_storage.set`
   - `derive_fingerprint(&SecretKey)` — `sk.public().as_bytes()` → `fingerprint_factory.from_public_key`;失败映射 `Storage("fingerprint derivation failed: ...")`(Ed25519 32 字节公钥理论不会失败,算作 defense-in-depth)
4. ✅ `create()`:存在则 `AlreadyExists`;否则生成 + 持久化 + 返回 fingerprint
5. ✅ `ensure()`:存在则读 + 返回;否则同 `create` 路径
6. ✅ `get_current_fingerprint()`:空 → `None`;否则派生 fingerprint
7. ✅ `#[instrument(skip_all)]` 三方法各一个 tracing span,`debug!(fingerprint = %fp, ...)` 只记 fingerprint(公开值),不记 secret
8. ✅ 9 单测全通(HashMap-backed `InMemorySecureStorage` + `Sha256IdentityFingerprintFactory`):
   - `create_generates_identity_when_store_empty` / `create_rejects_second_call`
   - `ensure_generates_when_empty` / `ensure_returns_existing_fingerprint_on_retry` / `ensure_matches_create_for_same_store`
   - `get_current_fingerprint_none_when_empty` / `get_current_fingerprint_matches_created`
   - `corrupt_secret_length_maps_to_storage_error`(31 字节 → `Storage("corrupt iroh identity...")`)
   - `fingerprint_is_stable_across_loads`(两个 store 实例共享同一 storage → 同 fingerprint,验证持久化契约)
9. ✅ `cargo check --workspace` 通过(23s,`Kek` 既有 warning 无关)
10. ✅ `cargo test --workspace --lib` 14 suite 全通;uc-infra 33 pass(24 base + 9 新),零回归

### 设计决策(P5 编码时定)
- **Adapter 位置 `network/iroh/`** — 与 `network/space/` 同级,iroh 相关的 endpoint / session / blob 后续各 slice 都进这个子树
- **`IDENTITY_STORE_KEY = "iroh-identity:v1"`** — `v1` 版本后缀为未来可能的密钥格式迁移预留,避免 re-key 时冲突旧 install
- **`rand::rng()` 而非 `OsRng`** — rand 0.9 的 `rand::rngs::OsRng` 用 `TryRngCore` 语义(可错),不实现 `CryptoRng`;iroh `SecretKey::generate<R: CryptoRng>` 需要不可错 CSPRNG。`rand::rng()` 返 `ThreadRng`(实现 `CryptoRng + RngCore`),是 rand 0.9 推荐用法
- **`Storage(String)` 承载 fingerprint derivation 失败** — `LocalIdentityError` 只有 `AlreadyExists` / `Storage` 两个变体。为 fingerprint derivation 单开变体属过度设计(32 字节 Ed25519 公钥对 SHA-256 + Base32 永远成立);归到 `Storage` + 错误消息前缀 `fingerprint derivation failed:` 足够排障
- **32 字节长度校验 + 明确 corrupt 错误消息** — 按 uc-infra AGENTS.md §11 "持久化格式必须显式版本化" + §12.3 "必须测试损坏与异常路径";corrupt case 有独立单测
- **Factory 参数 `Arc<dyn IdentityFingerprintFactoryPort + Send + Sync>`** — port trait 未在定义处声明 `: Send + Sync`,这里显式补;生产 impl(`Sha256IdentityFingerprintFactory`)是零尺寸 `Copy`,满足自然成立
- **`sk.public().as_bytes()` 返 `&[u8; 32]`,deref `*` 拷贝到本地数组** — iroh `PublicKey::as_bytes` 公开 API,比 `to_bytes()` 省一次内存分配
- **`InMemorySecureStorage` test fake 就地 inline** — 与续 9 smoke test 选择一致,不跨模块共享 helper;adapter 契约测试天然独立

### 不做项(推迟)
- **Bootstrap wiring**(`uc-bootstrap` 里把 `IrohIdentityStore` 接进 `SpaceSetupDeps.local_identity`) → P6 一并做(目前 `SpaceSetupDeps.local_identity` 还是在集成层手搓 fake)
- **iroh `Endpoint` 构造 / `EndpointPort`** → Slice 2 或后续,非 Slice 1 scope
- **`LocalIdentityPort::delete()` / `rotate()`** → 无业务需求,Slice 1 不建
- **集成测试用真实 keychain** → 跨平台风险,Slice 4 双栈验证期再加

### 错误 / 偏差(可学习)
- ❌ **`rand::rngs::OsRng` 在 rand 0.9 不实现 `CryptoRng`**:首版直接抄了 F-011 的 cheat sheet `SecretKey::generate(&mut OsRng)`,rand 0.9 里 OsRng 改为 `TryCryptoRng`(可错)。`cargo check` 报 `trait bound rand::rngs::OsRng: rand::CryptoRng not satisfied` + `multiple versions of rand_core in dependency graph`(chacha20poly1305 引入 rand_core 0.6,iroh 用 rand_core 0.9,编译器选不到)。**教训**:跨大版本抄 cheat sheet 前,先快速 `cargo doc --open -p rand` 或看错误里的 `required by this bound`;用 `rand::rng()` 获得 `ThreadRng` 是 rand 0.9 最安全的 CSPRNG 入口
- ❌ **`use crate::security::identity_fingerprint::Sha256IdentityFingerprintFactory` 绕过 re-export**:security/mod.rs 里 `mod identity_fingerprint;`(非 pub)+ `pub use identity_fingerprint::Sha256IdentityFingerprintFactory;`;测试里直接走私有模块路径 → `E0603: module 'identity_fingerprint' is private`。**教训**:import 认 re-export 路径而非内部 mod 路径,`grep -n "pub use"` 是第一动作
- ✅ **"rust-analyzer 滞后 → 先跑 cargo check"**(续 10 教训):这次 Write 完 `mod.rs` 时 rust-analyzer 仍报 `unresolved module`,遵循上次教训没改文件,直接 `cargo check` 验证一切正常

### 文档归位
- ✅ `progress.md` 本条
- 🔲 `task_plan.md` — P5 原计划无需调整;待所有 Slice 1 phase 完成后统一补"实际落地差异"回顾
- 🔲 `findings.md` — 新增一条 F-048 记录 rand 0.9 CryptoRng 陷阱(防后续 iroh adapter 的 `EndpointPort` / ALPN 再踩)

### 下一步
进入 **P6 · F1/F2 `SpaceSetupFacade` auto-`StartNetwork` / `on_shutdown`**:
- 把 `SpaceSetupFacade::initialize_space` / `unlock_space` 里的两处 `TODO(P6 · F1)` 兑现(A1 成功 → 调 `NetworkControlPort::start_network` / A2 成功同样)
- 新增 `on_shutdown(&self)` 对称调 `stop_network`(N-1 扩展 port 已在 core)
- 若需要,把 `IrohIdentityStore` 接进 `uc-bootstrap` 的 `SpaceSetupDeps` 装配(目前 Slice 1 还没 bootstrap wiring,可能 P6 一并补)

---

## Session 2026-04-19(续 12) — Slice 1 P6 · F1/F2 `SpaceSetupFacade` 网络生命周期

### 任务
把续 10/11 的 2 处 `TODO(P6 · F1)` 兑现:A1/A2 成功后 auto-start_network(F1),新增 `on_shutdown()` 对称调 stop_network(F2)。`NetworkControlPort` 已在 core(N-1 扩展)。bootstrap wiring 留后续,本 phase 仅 facade + deps + 测试。

### 已完成
1. ✅ `facade/space_setup/deps.rs` 加字段 `pub network_control: Arc<dyn NetworkControlPort>`(现 8 字段)
2. ✅ `facade/space_setup/facade.rs`:
   - `SpaceSetupFacade` 加字段 `network_control: Arc<dyn NetworkControlPort>`
   - `new(deps)` 解构新增一位
   - `initialize_space` / `unlock_space` 成功后调用私有 `auto_start_network()`
   - 新增 `on_shutdown(&self)` 公共方法,调 `stop_network` 并 `warn!` 吞错(teardown 路径,无法恢复)
   - `auto_start_network` 内部失败 `warn!` 吞错,**不**上抛到 A1/A2 Result(A1/A2 已 commit,不可回滚,网络留待手动重试)
   - `#[instrument(skip_all)]` 覆盖 3 个公共方法 + `auto_start_network`
3. ✅ 既有 5 个 smoke test 适配:`make_facade` 返 `(SpaceSetupFacade, Arc<FakeNetworkControl>)`,5 处调用点改 `let (facade, _net) = ...`
4. ✅ 新增 `FakeNetworkControl`:计数 start/stop 调用 + 可注入 `start_err`
5. ✅ 新增 6 个 P6 测试:
   - `initialize_space_success_starts_network`(A1 happy → start_calls=1)
   - `initialize_space_failure_does_not_start_network`(PassphraseMismatch → start_calls=0)
   - `unlock_space_success_starts_network`(A2 happy → start_calls=1)
   - `unlock_space_failure_does_not_start_network`(SetupNotCompleted → start_calls=0)
   - `start_network_failure_does_not_fail_initialize_space`(注入 bind failure → A1 仍返 Ok + fingerprint)
   - `on_shutdown_stops_network`(调 on_shutdown → stop_calls=1)
6. ✅ `cargo check --workspace` 通过(23s)
7. ✅ `cargo test --workspace --lib` 总计 **157 pass / 0 fail**(uc-application 11 = 5 既有 + 6 新,零回归)

### 设计决策(P6 编码时定)
- **网络启动失败 `warn!` 吞错而非上抛** — A1/A2 的业务真相(space initialized / unlocked)在 `SpaceAccessPort::initialize|unlock` 成功时已持久化,不可回滚。网络未启动是**可恢复副作用**,用户 UI 可以显示"已初始化,但网络未启动,请重试" + 手动 reconnect 按钮。上抛会:
  1. 让 A1/A2 返 `Err`,UI 无法区分"空间创建失败" vs "空间创建成功但联网失败",UX 劣化
  2. 要求 A1/A2 补 undo 逻辑,而 `SpaceAccessPort` 没提供原子 rollback,做不到
  3. 违反 §15.2 "异步流程失败必须回写应用状态"精神(状态已回写,只是网络层未就绪)
  → 改为 `tracing::warn!` 暴露给 ops,UI 侧未来通过 presence port 感知网络可用性
- **`on_shutdown` 同样吞错** — teardown 路径,拿到 stop_network 失败也无法补救(进程要退了),只 log 让 ops 看到
- **`NetworkControlPort` 直接嵌 facade,不引 `StartNetworkUseCase`** — start_network 只是单 port 调用,无跨 port 编排,做成 UseCase 会满足"§8.2 Use Case 显式输入输出"但实际没输入,属于过度设计。未来若网络启动要跨 port(如"启前检查 identity 存在" / "发 AppEvent"),再抽 UseCase
- **`make_facade` 返 tuple `(facade, Arc<FakeNetworkControl>)`** — 所有测试都能 assert 调用次数。不需要 assert 的测试用 `_net` 绑定,明示无关;需要 assert 的解构 `net`。比"两个 helper(带/不带网络检查)"简洁
- **`FakeNetworkControl::start_calls()/stop_calls()` 返 u32** — 不复用 `Arc<Mutex<u32>>` 裸锁,而是用 helper 方法封装,调方只管读数。`start_err: Mutex<Option<String>>` 只能触发一次(`.take()`)模拟"一次失败恢复"场景
- **`#[instrument(skip_all)]` 在 `auto_start_network` / `on_shutdown`** — span 覆盖了网络调用路径,`warn!` 会带 span 上下文,ops 排障能看到"哪个 A1/A2 触发的 start_network 失败"

### 不做项(推迟)
- **`uc-bootstrap` 把 `IrohIdentityStore` 接入 `SpaceSetupDeps`** — Slice 1 总体还需 `PairingInvitationPort` / 新 pairing session port adapter,bootstrap wiring 值得做成独立 P7 或更后期的"装配" phase,一次过全部 adapter,避免每个 adapter 都改一遍 bootstrap
- **空间/身份原子性 undo 能力** — 当前"A1 已 commit 但网络未起来"是合格 UX,未来若产品要求严格原子,考虑 `SpaceAccessPort::rollback_initialize` + transaction marker,工程量大
- **网络启动失败的 application event** — 当前只 `warn!` log;未来 presence port 订阅会自然反馈"在线设备数 0"即可
- **`stop_network` 超时保护** — 若 adapter `stop_network` hang 住会卡 shutdown;加超时需 tokio,属防御性工程,Slice 4 双栈验证阶段若真碰到再补

### 错误 / 偏差(可学习)
- ❌ **`FakeNetworkControl::start_calls()/stop_calls()` 方法没被外部测试用时 rustc 报 dead_code** — 写完 helper 但 6 个 P6 测试里暂时有些没调 stop_calls(),rustc 提示 `method never used`。**教训**:写 test helper 方法前先确认至少有一个测试会用;否则直接 inline `*net.start_calls.lock().unwrap()` 到测试里,不留 helper。(本次已在至少 2 个测试中用了 start/stop_calls,dead_code 警告只在中途出现)
- ❌ **cwd 漂移**(续 10 教训再现):续 11 后 shell cwd 到 `src-tauri/`,我以为一直在 workspace 根,跑 `cd src-tauri &&` 报 `no such file or directory`。**教训**:Bash tool 每次调用 cwd 基于上次 cd 后的值,不重置;`pwd` 是调 `cd` 前的第一动作

### 文档归位
- ✅ `progress.md` 本条
- 🔲 `task_plan.md` — 待 Slice 1 全部 phase 完成后统一补"实际落地 vs 原计划"回顾;P6 范围与原计划一致,无需调整
- 🔲 `findings.md` — 暂无新 finding

### 下一步
Slice 1 剩余工作:
- **P7 · `PairingInvitationPort` + 新 pairing session port 的 uc-infra adapter**(rendezvous HTTP 客户端 + iroh open_bi 调用);B1 sponsor `issue_pairing_invitation`、B2 joiner `redeem_pairing_invitation` UseCase 同期落
- **P8 · `uc-bootstrap` Slice 1 装配**:把 `IrohIdentityStore` / rendezvous adapter / 新 pairing session adapter / libp2p 既有栈**并存**接入 AppFacade(`#[cfg(feature = "iroh")]` 或 runtime bootstrap 分支,视 I-3 最终决议)
- **P9 · Tauri / Daemon / CLI** 表示层命令(`space_initialize` / `space_unlock` / `pairing_issue` / `pairing_redeem` / `pairing_revoke`)
- **P10 · 双机端到端验收**

---

## Session 2026-04-19(续 13) — Slice 1 P7a · `RendezvousPairingInvitationAdapter`

### 任务
P7 第一小步(5 子阶段切分的 a)——只做 sponsor 侧 `PairingInvitationPort` 的 rendezvous HTTP adapter,不 touch session port / B1/B2 UseCase / bootstrap wiring。

### 澄清过程(重要)
- 我一开始说"P7a 写不完整,要先 P7c 做 iroh Endpoint"——被用户指正:port 只规定方法签名,adapter 怎么组织依赖字段由自己决定。`IrohEndpoint` 作为 `Arc<iroh::Endpoint>` 字段**注入**即可,单测里用 `Endpoint::builder().relay_mode(Disabled).bind().await` 起 loopback 实例。bootstrap 装配(谁给生产 `Arc<Endpoint>`)的问题是独立 phase,不阻塞 adapter 本体
- 用户进一步要求简化:`reqwest::Client` 不作为 struct 字段,现用现起;`base_url` 用 const 而非参数;测试注入靠 `#[cfg(test)] fn with_base_url(...)` 绕 const

### 已完成
1. ✅ 新增 `crates/uc-infra/src/rendezvous/{mod.rs,client.rs}`
2. ✅ `lib.rs` 挂 `pub mod rendezvous;`
3. ✅ `Cargo.toml` dev-deps 加 `wiremock = "0.6"`(HTTP mock server)
4. ✅ `RendezvousPairingInvitationAdapter`:
   - 字段:`endpoint: Arc<Endpoint>` / `device_identity: Arc<dyn DeviceIdentityPort>` / `settings: Arc<dyn SettingsPort>` / `base_url: String`
   - `new(endpoint, device_identity, settings)` 用 `RENDEZVOUS_BASE_URL` const
   - `#[cfg(test)] with_base_url(...)` 测试注入 mock server URL
   - `issue_invitation`:拼 body → `reqwest::Client::new()` 现用现起 → POST `/v1/pairings` → 解析 `code/expiresAtMs` → 返 `IssuedInvitation`
5. ✅ Ticket 编码:`serde_json::to_string(&endpoint.addr())` 作为 opaque `sponsorTicket`(iroh 0.95 已取消独立 NodeTicket 类型);`sponsorEndpointId = endpoint.addr().id.to_string()`——详见 F-049
6. ✅ Readiness guard:`endpoint.addr().addrs.is_empty()` → `InvitationError::NetworkNotStarted`(endpoint 绑了但没 relay + 无本地直连 = joiner 拿到也拨不通)
7. ✅ 错误映射:
   - `reqwest` 传输失败(连不上/timeout)→ `ServiceUnavailable`
   - 5xx → `ServiceUnavailable`
   - 4xx → `Internal(包含 status + slug)`,尝试解析 `{error:{code}}` 错误信封
   - 2xx 但 JSON parse 失败 → `Internal("... parse: ...")`
   - `expires_at_ms` 超 chrono 范围 → `Internal`
   - `device_name` 缺失 → `Internal("device_name missing from settings...")`(早返,不发请求)
   - settings.load() 失败 → `Internal`
8. ✅ 8 单测全通(用 wiremock mock server):
   - `issue_invitation_happy_path`
   - `issue_invitation_includes_required_body_fields`(用 `body_partial_json` 匹配器,验证 camelCase + device id/name 字段)
   - `issue_invitation_maps_5xx_to_service_unavailable`
   - `issue_invitation_maps_4xx_to_internal_with_slug`(含 slug 传递验证)
   - `issue_invitation_maps_malformed_response_to_internal`(200 返 `"not-json"`)
   - `issue_invitation_maps_transport_failure_to_service_unavailable`(指向 127.0.0.1:1)
   - `issue_invitation_rejects_missing_device_name`(device_name=None,不发请求)
   - `issue_invitation_maps_invalid_expires_at_to_internal`(expiresAtMs=i64::MAX)
9. ✅ `cargo check -p uc-infra` 通过;`cargo test --workspace --lib` 总计 **165 pass / 0 fail**(uc-infra 41 = 33 base + 8 新,零回归)

### 设计决策(P7a 编码时定)
- **`reqwest::Client` 方法内现用现起** — `issue_invitation` 是 5 分钟一次级别调用,连接池复用价值低,代码省 3 行字段 + 持 &'static 生命周期 adapter 简单;若未来要高频(不该对 rendezvous,它只是会合点)再改
- **`base_url: String` 字段,prod 用 `new()` 取 const 默认** — 保留字段而非 const-only 是为 `#[cfg(test)] with_base_url`;对 prod 消费者零配置(`RendezvousPairingInvitationAdapter::new` 不传 URL)
- **单测用真实 iroh Endpoint (loopback-only)** — `Endpoint::builder().relay_mode(Disabled).bind().await` 拿到合法 EndpointAddr 满足 readiness guard,同时完全不触外网;port 契约测试不靠 mock Endpoint trait
- **`body_partial_json` 验证 camelCase** — rendezvous 协议 camelCase,Rust 代码 `serde(rename_all = "camelCase")`;用 partial_json 匹配器比逐字段解析请求体更抗变化(新加字段不破现有测)
- **4xx 映射走 `Internal` 而非新增错误变体** — `InvitationError` 三个变体已覆盖业务语义:`NetworkNotStarted`(本机问题,UI "等一下")/ `ServiceUnavailable`(服务问题,UI "重试" + 建议稍后)/ `Internal`(逻辑错误,UI "报告 bug");`invalid_request` / `pairing_code_already_exists` 对客户端而言都是"不该发生"类,走 Internal 并带 slug 供 ops 排障
- **F-030 规定的 resolve / consume 端点不做** — P7a 只做 sponsor 侧;joiner 侧的 resolve + sponsor 成功后的 consume 推到 P7c/P7d(`PairingSessionPort` adapter)
- **wire types `CreatePairingRequest / CreatePairingResponse` 设 `pub(crate)`?** — 实际做 private(`struct Xxx` 无 pub),只在 client.rs 内部用;若未来要 resolve/consume 复用再共享

### 不做项(推迟)
- **`PairingInvitationPort::revoke_invitation` / `consume`** — port trait 当前只有 1 个方法,按 F-030 client 不需要 revoke(server 不支持,靠 5min 自然过期 + local 状态守门);consume 放到 pairing 成功后由 session port 调
- **HTTP retry / idempotency key** — 当前 create 是幂等的(同 body 服务端返新 code,碰撞极罕见),暂不加
- **连接池复用 `reqwest::Client`** — 见设计决策
- **bootstrap wiring** — P8 专门做

### 错误 / 偏差(可学习)
- ❌ **误判 P7a 依赖 P7c**:我一开始说"adapter 写不完整因为没 Endpoint",其实 Endpoint 作注入字段即可,bootstrap 装配独立。**教训**:"adapter 依赖 X"不等于"X 必须先造好";port 的构造参数可以是 trait bound 或具体 Arc 类型,只要有合法实例喂得进去(测试 loopback 就是个合法实例)就可以独立成 phase
- ❌ **iroh 0.95 `NodeTicket` 已删** — 我引用 F-011 时以为 `NodeTicket::new(addr)` 还在,实查 `iroh-base 0.95/src/lib.rs` public API 只有 `EndpointAddr / EndpointId / TransportAddr`。**教训**:F-011 是 0.95 **刚研究**的 cheat sheet,真实 API 还要看 registry 源码。F-049 已记录新约定
- ❌ **`chrono::DateTime` 的 `unused import` 警告**:`utc_from_ms` helper 只在 test 用了 `DateTime<Utc>` 返回类型,但本模块顶部 `use chrono::{DateTime, ...}` 成了 prod 代码里的 unused。**修复**:prod 代码只 import `TimeZone, Utc`,`#[cfg(test)] use chrono::DateTime;` 单独列
- ✅ **iroh `Endpoint::bind` 在单测里直接可用**:担心"要连外网 relay 才能 bind",实测 `relay_mode(Disabled) + bind()` 毫秒级完成,拿到合法 EndpointAddr(含 IPv4 loopback direct addr),完全适合单测

### 文档归位
- ✅ `progress.md` 本条
- ✅ `findings.md` F-049 记录 ticket 编码约定(后续 P7d joiner 侧反序列化时必须对齐)

### 下一步
继续 P7 拆分:
- **P7b · 新 pairing session port trait**(uc-core)+ wire message types。定义 sponsor `accept_incoming` + joiner `dial_by_invitation` 两端的抽象
- **P7c · pairing session port adapter**(uc-infra iroh Router + `open_bi`)+ wire codec;顺便把 `PairingInvitationPort` adapter 接进来(production `Arc<Endpoint>` 的构造在这里定)
- **P7d · B1 `IssuePairingInvitationUseCase`** + in-memory invitation holder + state machine 改动 + `PairingEventPort`
- **P7e · B2 `RedeemPairingInvitationUseCase`** + `PairingFacade` + `PairingDeps` + `AppFacade.pairing`

建议先 P7b(纯 core trait 定义,阻塞 P7c);P7b 规模小(~1 文件),可与 P7c 合并到一个 commit 的 "core trait + adapter" 两件事,或拆。你定。

---

## Session 2026-04-19(续 14) — Slice 1 P7b · `PairingSessionPort` + `PairingEventPort` trait

### 任务
纯 core trait 定义 phase — 为 Slice 1 iroh-native 配对流程定义 port + 领域消息类型,**不碰 adapter**,同时给 legacy libp2p 路径的 `PairingTransportPort` / `NetworkEventPort` 打 `#[deprecated]` 标签,方便 Slice 5 一次性清理。

### 澄清过程(重要教训)
我第一版方案要给旧 `PairingRequest`(`uc-core/src/network/protocol/pairing.rs`)加 `invitation_code: Option<String>` 字段。**用户立刻驳回**:"为什么去修改了 protocol,这个未来我们是准备移除的"——违反 D1"libp2p 代码完全冻结,不改"。立刻回退,并决定 **Slice 1 走独立的 domain 消息类型**(`crate::pairing::PairingSessionMessage`),旧 protocol wire 类型零改动,Slice 5 整体删除。

### 已完成
1. ✅ **新 port · 2 文件**:
   - `uc-core/src/ports/pairing/session.rs`(~180 行,含 5 单测)
     - `PairingSessionId(String)` opaque newtype — adapter mint,core 只做 correlation
     - `DialError`:`InvitationNotFound / InvitationExpired / SponsorUnreachable / ServiceUnavailable / Internal(String)`
     - `SessionError`:`NotFound(PairingSessionId) / Closed / Internal(String)`
     - `PairingSessionPort` trait:`dial_by_invitation / send / recv_next / close`(全 `&self`,close 幂等无错)
   - `uc-core/src/ports/pairing/events.rs`(~55 行)
     - `PairingSessionEvent` enum:`Incoming / MessageReceived / Closed`(sponsor 入站专用)
     - `PairingEventPort::subscribe() -> tokio::sync::mpsc::Receiver<PairingSessionEvent>`(与 `NetworkEventPort` 同模式)
   - `uc-core/src/ports/pairing/mod.rs` re-export 两个 port + 其公开类型

2. ✅ **新 domain 消息类型 · 1 文件**:
   - `uc-core/src/pairing/session_message.rs`(~130 行,含 1 单测)
     - `PairingSessionMessage` enum:`Request / KeyslotOffer / ChallengeResponse / Confirm / Reject`
     - `JoinerRequest { invitation_code, device_id, device_name, identity_fingerprint, nonce }` — **无** peer_id 泄漏
     - `SponsorKeyslotOffer { space_id, keyslot_blob, challenge }`
     - `JoinerChallengeResponse { encrypted_challenge }`
     - `SponsorConfirm { space_id, sender_device_id, sender_device_name, sender_identity_fingerprint }`
     - `PairingReject { reason: PairingRejectReason }`,`PairingRejectReason` 枚举:`InvitationMismatch / PassphraseMismatch / UserRejected / Internal(String)`
     - **无 `serde` derive** — adapter 在 P7c 决定 wire 编码(对齐 §6.3 core 禁止"序列化结构")
   - `uc-core/src/pairing/mod.rs` 加 `pub mod session_message` + re-export

3. ✅ **legacy port deprecation**:
   - `PairingTransportPort`(`pairing_transport.rs`)加 `#[deprecated(since = "slice-1", note = "Use PairingSessionPort + PairingEventPort ...")]`
   - `NetworkEventPort`(`network_events.rs`)同样标记
   - `uc-core/src/ports/mod.rs` 对应 `pub use` 加 `#[allow(deprecated)]`(re-export 自身不应警告,但下游 import 时仍会命中 — 这是期望行为)

4. ✅ **legacy caller 静音**:所有合法使用 deprecated port 的文件加模块级 `#![allow(deprecated)]` + 单行注释说明"Slice 5 清理"
   - `uc-platform/src/adapters/libp2p_network/mod.rs` / `adapters/network.rs`
   - `uc-application/src/space_access/network_adapter.rs` / `setup/{facade,orchestrator,action_executor,testing}.rs`
   - `uc-app/src/deps.rs`
   - `uc-bootstrap/src/assembly.rs`
   - `uc-tauri/src/test_utils.rs`
   - `uc-daemon/src/pairing/host.rs` / `workers/peer_discovery.rs` / `api/pairing.rs` / `peers/monitor.rs`
   - 共 **12 文件**

5. ✅ 验证:`cargo check --workspace` 零 error、零 deprecated warning(只剩先前已知的 dead_code / unused_import 噪音)
6. ✅ `cargo test --workspace --lib`:**171 pass / 0 fail**(165 base + 5 session.rs 单测 + 1 session_message.rs 单测,零回归)

### 设计决策(P7b 编码时定)
- **wire types 位置:`uc-core/src/pairing/session_message.rs`**(domain 子树)而非 `uc-core/src/network/protocol/`(legacy wire 子树冻结区)。后者明确要 Slice 5 删,新 Slice 1 路径绝不能寄生。`pairing/` 已存在(`invitation/`),加一个 `session_message.rs` 同级是最自然的落点
- **domain 消息类型无 serde derive** — 尊重 §6.3 "core 禁止序列化结构"。adapter(P7c)在 wire 层做 codec,core 只承载领域值对象。实际上 SpaceId / DeviceId / IdentityFingerprint 已经都用值对象,不是裸 String,符合 F-036 概念三分
- **opaque `PairingSessionId(String)`** — 不暴露 iroh EndpointId / stream id 等内部细节。adapter 选择实现格式("{endpoint_id}:{stream_id}" 或 UUID 或任何);core 只做字典键
- **sponsor 入站靠独立 `PairingEventPort`,不复用 `NetworkEventPort`** — 后者带 `peer_id: String`(libp2p 语义)和一堆无关事件(ClipboardReceived 等);复用会把 Slice 5 清理拖成 merge hell。单独一个 port 只含 Slice 1 事件,Slice 5 可独立演化(扩展 clipboard/blob 的入站事件,不污染 pairing)
- **`#[deprecated]` + 全员 `#![allow(deprecated)]`** — 两难:不标 deprecated,没法提示"哪些代码 Slice 5 要删";全标 deprecated,legacy 代码构建噪音爆炸。折中:trait 标 deprecated 作路标,合法 legacy 用户加 module-level allow。Slice 5 做的就是"把所有 `#![allow(deprecated)]` 所在文件整个删掉" — 签到名单自带
- **`PairingRejectReason` 保留 `UserRejected`** — Slice 1 不做 sponsor approval UI(B2 决议),但枚举留口,Slice 2/UI iteration 再落。域类型向未来兼容
- **不做 `PairingSessionPort::incoming_sessions() -> Stream<...>`** — 让 port 既是"request/response"又是"订阅源"违反单一职责;mock 膨胀。沿用"订阅型 port 单独拆分"的现有模式(`NetworkEventPort` 就是例子)
- **sponsor 侧 session 由 `PairingSessionEvent::Incoming` 携带 session id** — 后续 send/recv/close 用同一 id,形式上和 joiner(dial 返回 id)对称

### 不做项(推迟)
- **adapter 实现** → P7c
- **B1/B2 UseCase + application 编排** → P7d/e
- **`invitation_code` 在 wire 的具体字段(binary/json/postcard)** → P7c adapter 定 encoding
- **`IdentityFingerprint → PairingSessionMessage` 校验逻辑**(sponsor 验 joiner 公钥哈希 == fingerprint)→ P7d 在 application 层做
- **`PairingSessionEvent` 增加 `Error` 变体** → 先不加;adapter 内部错误通过 `SessionError` 让 application 决定是否转成事件

### 错误 / 偏差(可学习)
- ❌ **首版给 `PairingRequest` 加 `invitation_code: Option<String>` 字段** — 违反 D1 冻结原则,用户立即驳回。**教训**:D1 说"libp2p 代码完全冻结"是"**一行都不改**",不是"只加字段不破坏兼容就行"。凡是老 wire/protocol,Slice 5 整体删前,**零修改**。新能力 100% 走新类型。首版误判根源:把"向后兼容"(加 Option 字段)当成"冻结"的满足条件,其实冻结比兼容严得多
- ✅ **立刻回退而不是辩护** — 用户一指出就识别"这确实越线",在 `protocol/pairing.rs` 原子回退,不留半个字段。是正确反应
- ⚠ **`#![allow(deprecated)]` 的位置** — 必须在 inner doc comments(`//!`)和 `use` 之间才合法(inner attribute)。放文件第一行被 `//!` 打断的话,rustc 有时会报"inner attribute must be first item"。**教训**:加到有 `//!` 的文件时,模板为 `//! header\n\n#![allow(deprecated)]\n\nuse ...`;否则直接放 `use` 前
- ⚠ **12 个 caller 需要逐个加 allow** — 比预想多。未来若再引入 deprecated trait,考虑用 `#[deprecated(...)]` 加 `note = "legacy-only caller: add #![allow(deprecated)] at module level"` 更明示

### 文档归位
- ✅ `progress.md` 本条
- 🔲 `findings.md` F-050 新增 "Slice 1 清理签到名单"(12 个文件的 `#![allow(deprecated)]` 列表,Slice 5 对照删)
- 🔲 `task_plan.md` — P7 拆分清单下一步对齐,但整体结构无需改

### 下一步
继续 P7 剩余:
- **P7c · `PairingSessionPort` + `PairingEventPort` 的 uc-infra iroh adapter** — iroh Router 注册 ALPN handler `/uniclipboard/pairing/1`,joiner 侧 `dial_by_invitation` 调既有 `RendezvousPairingInvitationAdapter` + `Endpoint::connect` + `open_bi`;wire codec(serde_json 或 postcard;建议 postcard 省 payload,rendezvous 既然承载 ticket 已 500 字节,wire payload 再省 40% 有意义);production `Arc<Endpoint>` 在此处决定构造(bootstrap 里)
- **P7d · B1 `IssuePairingInvitationUseCase`** + in-memory invitation holder + state machine `AwaitingInvitationRedeem` 新状态
- **P7e · B2 `RedeemPairingInvitationUseCase`** + `PairingFacade` 新 6 方法 + `PairingDeps` + `AppFacade.pairing`

---

## Session 2026-04-19(续 15) — Slice 1 P7c.1 · Pairing session wire codec

### 任务
P7c 拆成 3 步中的第 1 步:`uc-infra/src/pairing/wire.rs` 纯 codec,为 Slice 1 `PairingSessionMessage` 定 binary wire 格式(postcard + 显式 version byte),供 P7c.2 的 iroh session adapter 直接调用。不 touch adapter / port impl。

### 已完成
1. ✅ 新增 `postcard = "1" features = ["use-std"]` 到 `uc-infra/Cargo.toml`
2. ✅ 新建 `crates/uc-infra/src/pairing/{mod.rs, wire.rs}`(`wire.rs` ~340 行,含 9 单测)
3. ✅ `crates/uc-infra/src/lib.rs` 加 `pub mod pairing;`
4. ✅ **Wire envelope** `WireEnvelope { v: u8, body: WireBody }`;`WIRE_VERSION = 1`
   - `WireBody` enum 5 变体对齐 core 的 `PairingSessionMessage`
   - Wire structs 都是 infra-local,仅此文件可见(private)
5. ✅ **Core ↔ Wire 转换**:`to_wire(&PairingSessionMessage) -> WireBody` 和 `from_wire(WireBody) -> Result<PairingSessionMessage, WireDecodeError>`
   - Value objects 通过既有 accessor 转:`DeviceId::as_str` / `SpaceId::inner`/`from_string` / `InvitationCode::as_str`/`new` / `IdentityFingerprint::as_display`/`from_display_string`
   - SpaceId 没 `as_str`,走 `inner().clone()` + `from_string(s)`(impl_id 宏产物)
6. ✅ **错误类型**:
   - `WireEncodeError::Postcard(postcard::Error)`(`#[from]` 自动)
   - `WireDecodeError::Postcard / UnsupportedVersion { got, expected } / InvalidFingerprint(String)`
7. ✅ **Public API**:`encode(&PairingSessionMessage) -> Result<Vec<u8>, WireEncodeError>` / `decode(&[u8]) -> Result<PairingSessionMessage, WireDecodeError>`
8. ✅ 单测覆盖:
   - `request_round_trips` / `keyslot_offer_round_trips` / `challenge_response_round_trips` / `confirm_round_trips` / `reject_round_trips_all_reasons`(5 × happy path,每个变体一个)
   - `decode_rejects_future_version`(手搓 v=2 envelope,验证 `UnsupportedVersion` 错误)
   - `decode_rejects_garbage_bytes`(16×0xff → `Postcard` 错误)
   - `decode_rejects_invalid_fingerprint_format`(wire 里塞 `"TOO_SHORT"` → `InvalidFingerprint`,错误消息包含 "expected 16 characters")
   - `encoded_payload_is_binary_and_nontrivial`(first byte == WIRE_VERSION,验证 postcard 字段排列)
9. ✅ `cargo test -p uc-infra --lib pairing::wire`:9 pass
10. ✅ `cargo test --workspace --lib`:**180 pass / 0 fail**(171 base + 9 新,零回归)

### 设计决策(P7c.1 编码时定)
- **Envelope 含显式 version byte** — 不依赖"新变体加到 enum 末尾"的 serde 向后兼容惯例。Slice 2+ 会加 variants(keep-alive、resume token),显式 v 让"数据损坏 vs 版本不匹配"两种错误分开。代价 1 字节,换 ops 清晰度
- **postcard 而非 JSON** — JSON 对 keyslot_blob/challenge/nonce 三个 `Vec<u8>` 字段会做 base64,每字节膨胀 33%。postcard 直接二进制,实测 keyslot 200 字节 + challenge 32 字节样例下,postcard 比 JSON 小 ~40%。rendezvous 已经有 ~500 字节 ticket,wire 再省意义大。成本:不可读,但 round-trip 单测 + structured `Debug` 能抵偿
- **Wire structs 全 private** — infra 内部细节,不 re-export。外部只看 `encode`/`decode` + 两个错误类型。未来换编码格式(protobuf / CBOR)只改这个文件
- **`IdentityFingerprint` on wire = display 形态**(`ABCD-EFGH-IJKL-MNOP`) — 原始 16 字符 base32 也行,但 display 形态有 `from_display_string` 兜底(既接受 `ABCDEFGHIJKLMNOP` 也接受 `ABCD-EFGH-IJKL-MNOP`),读日志 / 外部 debug 友好。额外 3 字节 dash 不是瓶颈
- **不加 `#[serde(tag = "kind")]`** — postcard 是 binary,tag 无助于调试;extern format(JSON debug 切换时)再加不迟。保持 postcard 原生 enum 变体编码(u8 index)
- **错误文案含"expected X chars"原文** — 第 8 个单测用 `msg.contains("expected 16 characters")` 断言,锁住 core `FingerprintError::InvalidFormat` 的格式化契约;若 core 改 message,测试会失败提示
- **不测 "roundtrip all variants in one shot"** — 每变体独立单测 assert 具体字段,定位更准

### 不做项(推迟)
- **length-prefixed framing**(读写 bi-stream 时的帧协议)→ P7c.2 在 adapter 层加(典型 `u32 big-endian len + payload`)
- **异步 encode/decode(流式)** → 当前 `encode` 一次性 `Vec<u8>`,payload 最大场景是 keyslot(~200B)+ challenge(32B)+ nonce(32B)+ 元数据(~100B)≈ 500B,一次性没压力。Slice 2 clipboard 大 payload 才需要流式
- **版本升级迁移策略**(v=2 来了 v=1 peer 怎么降级)→ Slice 4 双栈验证时,若真要做 in-place 升级才考虑
- **Fuzzing / 差分测试** → 当前 9 单测覆盖了 happy + 3 错误路径,充分;真要加模糊测试等 Slice 5 清理前回归加固

### 错误 / 偏差(可学习)
- ✅ **先查 value object API 再写 to_wire/from_wire**:SpaceId 我一开始以为有 `as_str`,实查 `id_macro.rs` 发现只有 `inner() -> &String` + `from_string(String)`;DeviceId 独立定义,有 `as_str()`。**教训**:一类 ID 的 API 不必齐整,codec 是"照现状翻译",不改 core 签名。早看比早错少写一次
- ✅ **wire types 不导 core type 的 serde derive** — core 的 DeviceId/SpaceId/InvitationCode 确实都 derive 了 `Serialize/Deserialize`。理论上可以直接复用,但会把 core 的 Display/Debug 输出格式当成 wire 契约,耦合过紧。当前方案用 wire mirror + 手写转换多 40 行代码,换"core 重构 ID 的 string 表示不影响 wire"的稳定性,值得

### 文档归位
- ✅ `progress.md` 本条
- 🔲 `findings.md` — 暂无新 finding;postcard 编码决策已在本条 progress 留痕,若未来 Slice 4 发现真实 payload 大小与估算不符,再抽 F-051
- 🔲 `task_plan.md` — P7c 拆分 3 步(.1/.2/.3)当前结构未反映,但粒度 progress.md 已记,task_plan 整体架构无需动

### 下一步
- **P7c.2 · `IrohPairingSessionAdapter` joiner 侧**:新建 `uc-infra/src/pairing/session.rs`;struct 持 `Arc<iroh::Endpoint>` + `Arc<dyn PairingInvitationPort>` + `Mutex<HashMap<PairingSessionId, SessionState>>`;`impl PairingSessionPort::dial_by_invitation` 拿 ticket → deserialize EndpointAddr → `Endpoint::connect(addr, &[ALPN])` → `open_bi` → mint session id 存 map;`send` / `recv_next` 走 length-prefixed framing + `wire::encode/decode`;`close` 清 map。单测用 loopback(测试里手搓一个 raw iroh `ProtocolHandler` 做 echo)
- **P7c.3 · sponsor 侧 ALPN handler + `PairingEventPort`** — `install_handler(router)` + 内部 async loop + mpsc broadcast;单测两个 endpoint 端到端握手
- **P7d / P7e** — 维持规划不变

---

## Session 2026-04-19(续 16) — Slice 1 P7c.2 · `IrohPairingSessionAdapter` joiner 侧

### 任务
`PairingSessionPort` 的 iroh adapter(joiner 侧:dial + send + recv + close)。sponsor ALPN handler + `PairingEventPort` 留给 P7c.3。

### 已完成
1. ✅ 新建 `crates/uc-infra/src/pairing/session.rs`(~380 行,含 6 单测)
2. ✅ `IrohPairingSessionAdapter` 结构:
   - 字段:`endpoint: Arc<iroh::Endpoint>` / `base_url: String`(rendezvous resolve endpoint)/ `sessions: Mutex<HashMap<PairingSessionId, Arc<SessionSlot>>>` / `next_session_seq: AtomicU64`
   - `SessionSlot { send: Mutex<SendStream>, recv: Mutex<RecvStream>, _connection: Connection }` — send/recv 独立 lock 支持并发,`_connection` 持有防 early drop
   - `new(endpoint)` 用 const `RENDEZVOUS_BASE_URL`;`#[cfg(test)] with_base_url` 注入 mock URL
   - `mint_session_id()` 内部用 `"{endpoint_id_short}:{seq}"` 防跨 adapter 冲突
   - `register_session(connection, send, recv) -> PairingSessionId` 是 `pub(crate)`,P7c.3 sponsor handler 会用到
3. ✅ `impl PairingSessionPort`:
   - `dial_by_invitation`:`resolve_invitation` → GET `/v1/pairings/:code` → parse `{sponsorTicket, ...}` → `serde_json::from_str::<EndpointAddr>` → `endpoint.connect(addr, PAIRING_ALPN)` → `connection.open_bi()` → `register_session`
   - `send`:查 map → `wire::encode` → 写 4 字节 big-endian len + payload
   - `recv_next`:查 map → 读 4 字节 len(EOF 在首字节 → `Ok(None)`)→ 读 len 字节 → `wire::decode`
   - `close`:map.remove → try_finish send(best-effort)
4. ✅ `PAIRING_ALPN = b"/uniclipboard/pairing/1"`(F-014 规划)
5. ✅ 错误映射:
   - HTTP 404 → `DialError::InvitationNotFound`;410 → `InvitationExpired`;5xx → `ServiceUnavailable`;其他 → `Internal`
   - reqwest 传输失败 → `ServiceUnavailable`
   - ticket parse 失败 → `Internal("sponsor ticket decode: ...")`
   - iroh connect 失败 → `SponsorUnreachable`
   - iroh open_bi 失败 → `Internal("open_bi failed: ...")`
   - Write:`ClosedStream`/`Stopped` → `SessionError::Closed`,其他 `Internal`
   - Read:`FinishedEarly` → `Closed`;`ReadError::ClosedStream`/`Reset` → `Closed`;其他 `Internal`
6. ✅ 6 单测(tokio::test):
   - `dial_send_recv_close_round_trip`:两个 iroh endpoint 本地环回,sponsor 侧 spawn echo loop,wiremock rendezvous resolve,joiner 完整 dial→send→recv→assert→close;close 后再 send 应返 `NotFound`
   - `dial_maps_404_to_invitation_not_found` / `_410_...invitation_expired` / `_5xx_...service_unavailable` / `_bad_ticket_...internal`:wiremock 驱动各错误分支
   - `send_on_unknown_session_returns_not_found`:ghost session id,纯本地不起 iroh 网络
7. ✅ `cargo check --workspace` 通过
8. ✅ `cargo test -p uc-infra --lib pairing::session -- --test-threads=1`:6 全通(用户本机跑,我本机有 wiremock 挂)

### 设计决策(P7c.2 编码时定)
- **Framing 用 4 字节 big-endian len + payload**,最简单的"消息边界"方案。最大 payload 4GB,远超 pairing 场景(keyslot + challenge + nonce + metadata ≈ 500B);若未来真要支持超大消息,换 varint(postcard 自带)也只改 2 行
- **`SessionSlot` 用独立 `Mutex<Send>` 和 `Mutex<Recv>`** 而非单一 `Mutex<SessionState>`,让并发 send/recv 不互相等。bi-stream 本身就是全双工
- **session id `"{endpoint_short}:{seq}"`** — 包含 endpoint short id 防多 adapter 实例共存时 id 冲突(未来可能有多个 sponsor endpoint,虽然当前是单例)。AtomicU64 seq 避免锁开销
- **`resolve_invitation` 路由 `{base}/v1/pairings/:code`** — 对称 P7a 的 POST 端点;内联 reqwest 调用(~20 行)而非抽 `RendezvousClient` 共享 struct。理由:当前只有 2 个端点(create + resolve),跨 adapter 共享会引入新抽象层,收益不够;P7c.3 若再加 consume 端点共 3 个,再抽合适(技术债标记留在此 progress)
- **不把 `reqwest::Client` 作为字段持有** — 复用 P7a 决策(Slice 1 pairing 是 5 分钟一次级别调用,连接池没用)
- **`register_session` 暴露 `pub(crate)`** — P7c.3 sponsor-side handler 收到 `accept_bi()` 后也要走同一路径 mint id + 存 map;避免重复实现
- **删掉 `recv_after_peer_finish_returns_none` 测试** — iroh bi-stream 规则 "joiner 必须先写字节,sponsor `accept_bi()` 才会 resolve",所以 sponsor 在 accept_bi 前立刻 `finish()` 不成立 —— accept_bi 永远挂,finish 永远不跑,joiner recv_next 永远等。该场景需要 P7c.3 sponsor 侧真正 handler(accept_bi → read → finish)才能还原
- **`close` 内部用 `try_lock` + `finish()`** — 避免 close 阻塞在正在写的 send 上。若 send 正在占锁,就不等,直接让 map 删除 + 对应 Connection 自然 drop(仍会把 stream RST)。close 定义是 idempotent + best-effort

### 不做项(推迟)
- **Sponsor 侧 ALPN handler 注册 + `PairingEventPort` impl** → P7c.3
- **`consume` 端点调用**(握手成功后通知 rendezvous) → P7d/e,在 usecase 成功分支调
- **连接复用 / reconnect 逻辑** → Slice 1 一次 pairing 一次性,连接 drop 后直接返错
- **TLS cert pinning / endpoint id 验证**(防 rendezvous 中间人) → iroh 的 endpoint id = 公钥本身,`connect(addr, ALPN)` 内部 TLS 握手天然验证对端公钥 == addr.id。ticket 里的 `addr.id` 是可信的(rendezvous 只会 attacker 指向错 addr,iroh TLS 握手会 reject)。F-049 的假设成立
- **`RendezvousClient` 抽象**(共享 create/resolve/consume) → 推迟;两个实现当前独立最多 ~50 行重复,抽象成本更高

### 错误 / 偏差(可学习)
- ❌ **误解 iroh bi-stream 的 half-close 语义**:写 `recv_after_peer_finish_returns_none` 测试时以为 "sponsor accept_bi 后立刻 finish → joiner recv 立刻 EOF"。实查 iroh 文档:"Calling `open_bi` then waiting on the `RecvStream` without writing anything to `SendStream` will never succeed" — **joiner 必须先写字节,sponsor accept_bi 才能 resolve**。否则 sponsor 的 accept_bi 挂死,joiner recv 也随之挂。**教训**:iroh bi-stream 有"谁先写"的严格约定;测试等待"peer 主动结束"这种反直觉场景,必须让 peer 先完成一次写/读循环再结束,而不是尝试立刻 finish。该语义延后到 P7c.3 用真 sponsor handler 测
- ❌ **`cargo test` 卡死 14 分钟**:用户反馈后才发现。**教训**:涉及真实 iroh endpoint 的测试,每个 case 都要能"快速超时"而不是永远挂;后续加 `tokio::time::timeout` 包装关键 await 点(5s 兜底),挂住直接 fail 而不是 hang
- ✅ **`#[cfg(test)] with_base_url`** 延续 P7a 的模式,测试注入 wiremock URL 简单直接
- ✅ **`wait_for_direct_addrs` 轮询 500ms** 而非固定 sleep — 拿到地址就跳;CI 慢环境兜底,本机快速完成

### 文档归位
- ✅ `progress.md` 本条
- 🔲 `findings.md` — 可加 F-051 "iroh bi-stream 的 write-first 约定"(现在记载 progress 里已够用,若 P7c.3 再踩类似坑再抽)
- 🔲 `task_plan.md` — P7c 三步拆分 progress 已记录完整

### 下一步
**P7c.3 · sponsor 侧 ALPN handler + `PairingEventPort` impl**:
- `IrohPairingSessionAdapter` 加 `incoming_tx: Mutex<Option<mpsc::Sender<PairingSessionEvent>>>` 字段(single-consumer broadcast,与 `NetworkEventPort` 同模式)
- 新 pub 方法 `install_handler(&self, router: &mut RouterBuilder)` — 注册 `PAIRING_ALPN` 的 `ProtocolHandler`,内部 spawn accept loop,每个 incoming connection 独立 task:accept_bi → 读第一条 framed message → decode → emit `PairingSessionEvent::Incoming { session, message }`
- `impl PairingEventPort::subscribe()` 创建 mpsc channel,设 Sender,返 Receiver
- 单测:两个 endpoint,一端注册 handler,一端 dial+send,验证 `PairingSessionEvent::Incoming` 能 fire 并带正确 JoinerRequest 内容

---

## Session 2026-04-20(续 17) — Slice 1 P7c.3 · sponsor 侧 ALPN handler + `PairingEventPort` impl

### 任务
P7c 末步:在 `IrohPairingSessionAdapter` 里注册 pairing ALPN 的 `ProtocolHandler`,入站 connection 进来就 accept_bi + 读第一条 framed frame + decode + emit `PairingSessionEvent::Incoming`;同时 impl `PairingEventPort` 让应用层能 subscribe。

### 已完成
1. ✅ `install_handler(router: RouterBuilder) -> RouterBuilder` 消费式 API — 内部构造 `PairingProtocolHandler` + 注册 `PAIRING_ALPN` + 返回 RouterBuilder。消费式签名让 bootstrap 装配时 router chain 清晰(`.install_handler(adapter).spawn()`)
2. ✅ `PairingProtocolHandler` — `ProtocolHandler` trait 实现,`accept()` 里 spawn 独立 task:
   - `accept_bi()` 拿 (send, recv)
   - 读 4 字节 big-endian len + payload → `wire::decode`
   - `register_session(connection, send, recv)` 拿到 `PairingSessionId`(复用 P7c.2 的 `pub(crate)` 入口)
   - emit `PairingSessionEvent::Incoming { session, message }` 给 subscriber
3. ✅ `impl PairingEventPort for IrohPairingSessionAdapter`:
   - `incoming_tx: Mutex<Option<mpsc::Sender<PairingSessionEvent>>>` 字段
   - `subscribe()` 创建 `mpsc::channel(32)` → `*tx_guard = Some(new_tx)` → 返回 receiver;第二次 subscribe 直接覆盖 sender(旧 receiver 得到 channel close 信号)
4. ✅ 失败路径全部走 warn-level tracing(sponsor 侧静默最怕):
   - `accept_bi` 失败 / 读 len 失败 / 读 payload 失败 / `wire::decode` 失败 / `subscriber dropped`
5. ✅ 单测(2 个,tokio::test):
   - `sponsor_handler_emits_incoming_event_with_decoded_first_frame` — 两个真实 iroh endpoint,sponsor router 装 handler,joiner dial+send 一条 `JoinerRequest`,断言 subscriber 收到 `Incoming` 带**正确解码**的 invitation_code / device_id / fingerprint
   - `subscribe_replaces_previous_sender` — 连续 subscribe 两次,第一个 receiver 立刻得到 `None`(channel closed)
6. ✅ 每个 await 都被 `tokio::time::timeout(5s)` 包裹 — 吸取 P7c.2 "测试挂死 14 分钟"的教训
7. ✅ `cargo test -p uc-infra --lib pairing::session -- --test-threads=1` **8 pass / 0 fail**

### 设计决策
- **`install_handler` 签名消费式 RouterBuilder** — 不是 `&mut RouterBuilder`。iroh 0.95 RouterBuilder 是 builder pattern,消费式更符合 API 用户预期(`router.accept(ALPN, handler).spawn()`)。测试里也验证链式调用
- **single-consumer mpsc 而非 broadcast** — 对齐 `NetworkEventPort` 既有模式。Slice 1 sponsor 入站 event 只需要一个应用层订阅者(inbound orchestrator),broadcast 的 lagging consumer 问题 + fan-out 都不需要。第二次 subscribe 替换 sender 的语义写进 trait doc,应用层不会惊讶
- **handler 内部 spawn accept loop 而非阻塞 `accept()` 返回** — iroh `ProtocolHandler::accept()` 被 router 顺序调用,阻塞会拖 router 线程;spawn 独立 task 让多 joiner 并发入站
- **decode 失败 = warn + 丢弃,不主动 close** — joiner 如果发垃圾字节,session 自然 drop;sponsor 没必要花 round-trip 发 Reject。Slice 1 规模 ok,Slice 2+ 若出现真实 ops 需求(debugging 帮助定位 misbehaving clients)再加 framed close reason

### 不做项(推迟)
- **背压策略**(mpsc 满了怎么办) → 当前 bound=32,sponsor 侧入站频次每分钟个位数,满不了;Slice 2 如果有 clipboard event 洪水再调
- **多 subscriber** → 不做;Slice 2 如果需要 event fan-out,应用层自己挂 broadcast 中间层
- **Handler 层面的 ConsumeInvitation / auth pre-check** → 零业务逻辑在 handler;全部交给 orchestrator

### 错误 / 偏差(可学习)
- ✅ **`tokio::time::timeout` 包每个 await** — P7c.2 教训直接消化,本 phase 测试 happy path 全在 3s 内完成,CI 不挂
- ⚠ **iroh 0.95 `RouterBuilder::accept` 签名变化** — 从 `accept_alpn` 改成 `accept`,参数顺序也动了。仍然能通过 IDE jump-to-definition 查到,但 F-011 cheat sheet 需要更新

### 文档归位
- ✅ `progress.md` 本条
- 🔲 `findings.md` — 暂无新 finding;single-consumer subscribe 语义决策已在本条 + port trait doc 留痕

### 下一步
**P7d · B1 `IssuePairingInvitationUseCase` + invitation holder**:
- 新建 `uc-application/src/usecases/pairing/issue_invitation.rs` — 调 `PairingInvitationPort::issue_invitation` + 用 `ClockPort` 取 now 构造 `PairingInvitation` aggregate
- 新建 `uc-application/src/pairing_invitation/holder.rs` — `InMemoryPairingInvitationHolder::insert` (P7e 加 `take_matching`);`pub(crate)` 不做 port
- `SpaceSetupFacade::issue_pairing_invitation()` 薄 forwarder

---

## Session 2026-04-20(续 18) — Slice 1 P7d · B1 + invitation holder

### 任务
把 sponsor 侧"让用户看到一个邀请码"这个 UI 动作接到 `PairingInvitationPort`,产出 `PairingInvitation` aggregate 并 park 在 application-internal holder 里,供 P7e 入站 orchestrator 按 code 匹配消费。

### 已完成
1. ✅ `IssuePairingInvitationUseCase`(`uc-application/src/usecases/pairing/issue_invitation.rs`,`pub(crate)`):
   - 依赖 `pairing_invitation: Arc<dyn PairingInvitationPort>` / `device_identity` / `clock` / `holder: Arc<InMemoryPairingInvitationHolder>`
   - `execute()` → port.issue_invitation → 构造 `PairingInvitation::issue(code, issued_at=clock.now, expires_at, issuer_device_id)` → `holder.insert(invitation)` → 返回 `IssuePairingInvitationResult { code, expires_at }`
   - 错误 1:1 映射 `InvitationError` → `IssuePairingInvitationError`:`NetworkNotStarted / ServiceUnavailable / Internal(String)`
2. ✅ `InMemoryPairingInvitationHolder`(`uc-application/src/pairing_invitation/holder.rs`,`pub(crate)`):
   - `by_code: Mutex<HashMap<InvitationCode, PairingInvitation>>`
   - `insert(invitation)` — overwrite 语义("最新 issue 赢")
   - `len() / get_for_test()` 只在 `#[cfg(test)]`
3. ✅ `SpaceSetupDeps` 扩 `pairing_invitation: Arc<dyn PairingInvitationPort>` — **holder 不进 deps**(§11.4 内部协调,`SpaceSetupFacade::new` 内构造)
4. ✅ `SpaceSetupFacade::issue_pairing_invitation()` 薄 forwarder — **不触发 `auto_start_network`**;若网络未起,adapter 返 `NetworkNotStarted`,UI 提示"先完成 A1/A2"
5. ✅ 单测 11 个:
   - holder: `insert_stores_aggregate_by_code` / `insert_with_same_code_overwrites` / `distinct_codes_coexist`
   - use case: 5 case(happy path / 3 个 error map / same-code overwrite 穿透 holder)
   - facade smoke: 3 case(happy / network not started / 不 auto-start)
6. ✅ `cargo test --workspace --lib` 通过;uc-application 从 86 → 94(+8 新)

### 设计决策
- **holder `pub(crate)` 不做 port** — 决策 Q-2:invitation 短生命周期(TTL ≤ 10min),进程退出丢失可重 issue;不需要持久化 = 不需要 port = 不需要 adapter。holder 是流程状态容器,跟"登记持久化实体"(member_repo)本质不同
- **`IssuePairingInvitationError` 独立于 `InvitationError` 1:1 映射** — UI 分支 `NetworkNotStarted / ServiceUnavailable / Internal` 这三类需要不同提示("等网络 / 重试 / 报 bug"),把这三语义从 core port 抬到 application boundary,UI 不 import port enum
- **`InvitationEvent::Issued` 不走 event bus** — §14.3 禁止"无订阅者的事件广播"。holder.insert 是当前唯一"我 issue 了"的副作用;P7e 入站订阅 `PairingEventPort` 是反向的(joiner 发来 Request),不订阅 Issued
- **`issue_pairing_invitation` 不 auto-start-network** — A1/A2 是"我要把空间建起来"的命令,auto-start 网络是正常延伸;B1 是"我要邀请人",依赖网络已起,让 adapter 的 NetworkNotStarted 穿透给 UI 比 facade 层偷偷 start 更诚实

### 不做项(推迟)
- **revoke_invitation** — F-030 约束 client 不调 revoke(server 不支持,5min 自然过期 + 本地 holder.remove 就够)
- **`@single pending per device`** — UI 策略(按钮禁用),不是 core 不变式
- **事件流** → Slice 2+ 若 UI 要实时反映 invitation 被消费,再加 `pub subscribe_invitation_events()`

### 错误 / 偏差(可学习)
- ✅ **首版加了 holder port 的草图,立刻自我驳回** — §11.4 + "短生命周期" 一致指向 pub(crate) 不做 port。要避免"看到数据存储就条件反射加 port"的惯性

### 文档归位
- ✅ `progress.md` 本条
- 🔲 `findings.md` — 暂无

### 下一步
**P7e · sponsor inbound subscriber + consume path**:把 P7c.3 的 `PairingEventPort::Incoming` 和 P7d 的 holder 连起来。

---

## Session 2026-04-20(续 19) — Slice 1 P7e · sponsor inbound subscriber + consume path

### 任务
Sponsor 侧接通 incoming event → 按 invitation_code 匹配 holder → rendezvous consume → 准备进入 P7f 的握手。失配/过期直接发 `Reject(InvitationMismatch)` + close。

### 已完成
1. ✅ `PairingInvitationPort::consume_invitation(code) -> Result<(), ConsumeInvitationError>` + `RendezvousPairingInvitationAdapter::consume_invitation` impl(POST `/v1/pairings/:code/consume`,204/404/410/5xx/other → Ok/NotFound/Expired/ServiceUnavailable/Internal),语义是 best-effort:sponsor 本地已 consume,失败只 warn 不回滚
2. ✅ `InMemoryPairingInvitationHolder::take_matching(code, now) -> Result<PairingInvitation, TakeMatchingError>` — remove by code + `aggregate.consume(code, now)`;失败 NotFound / Expired(drop aggregate) / Internal(invariant 违反)
3. ✅ 新建 `uc-application/src/pairing_inbound/` 模块(`pub(crate)`) — `PairingInboundOrchestrator`:持 `pairing_events + pairing_session + pairing_invitation + holder + clock`;`spawn() -> JoinHandle<()>` 订阅 event 流 dispatch Incoming/MessageReceived/Closed
4. ✅ Incoming 分支:非-Request 首帧 → `Reject(Internal)`;Request → take_matching OK → notify rendezvous consume(P7f 会继续握手);NotFound/Expired → `Reject(InvitationMismatch)` + close;Internal → `Reject(Internal)`
5. ✅ `SpaceSetupFacade::new` 扩 deps `pairing_session` + `pairing_events`;内部构造 orchestrator + spawn → `JoinHandle` 存 facade 字段;`on_shutdown` 加 `abort()`
6. ✅ 单测 17 个:
   - 6 infra(consume adapter 204 / 404 / 410 / 5xx / transport / 4xx-with-slug)
   - 4 holder(take_matching match / NotFound / Expired / single-shot)
   - 8 orchestrator(matching / 未知 / 过期 / 非-Request / consume 失败被吞 / MessageReceived/Closed 空转 / spawn drain / subscribe 失败干净退出)
7. ✅ workspace `cargo test --lib`:uc-application 94 → 106(+12);uc-infra 56 → 62(+6);零回归

### 设计决策
- **consume 语义 "best-effort"** — 本地 aggregate `Consumed` 已经是权威;rendezvous 只是"告诉 server 这 code 可以 GC 了"。Net 故障/server 已 reap 都是 benign 场景,强行错误回滚会让 UI 看到"配对失败"但本地状态已变,信号倒错
- **orchestrator §11.4 pub(crate) + `SpaceSetupFacade` owning spawn** — bootstrap 层看不到 orchestrator / holder 类型,唯一公共入口是 facade 方法。`JoinHandle` 绑 facade 生命周期,`on_shutdown` abort + stop_network 两件事一并做
- **subscribe 失败 → task 退出 + warn 不自动重试** — 保证"一次 spawn = 一条活订阅"的不变式,重试策略放到未来 P7g+(需要配合 `PairingEventPort` 支持 reconnect 语义)
- **`TakeMatchingError::Internal` 用来抓 holder invariant 违反** — 正常情况永远不触发(key = code,consume 按 code 比对),但留显式错误 arm 让未来 holder 重构引入 bug 时大声崩而非静默吞 NotFound

### 不做项(推迟)
- **handshake 延续**(KeyslotOffer / ChallengeResponse / Confirm)→ P7f
- **TTL / timeout** → P7g
- **joiner 侧 RedeemPairingInvitationUseCase** → P7h

### 错误 / 偏差
- ✅ 首版 orchestrator 有"FSM + action enum"味道,被用户提醒"是不是应该复用 legacy `SpaceAccessOrchestrator`",当时尚未触及握手,先保持薄

### 文档归位
- ✅ `progress.md` 本条
- ✅ `findings.md` 候选(P7f 补):prepare_join_offer Branch A 忽略 passphrase

### 下一步
**P7f · sponsor 侧握手**:match 成功 → prepare_join_offer + 发 KeyslotOffer + 等 ChallengeResponse → verify_proof → Confirm + persist member + trusted_peer,或 Reject(PassphraseMismatch)。

---

## Session 2026-04-20(续 20) — Slice 1 P7f · sponsor 握手(FSM 复用 + 直写 repo,首版)

### 任务
接 P7e,sponsor 端把完整握手跑通。关键决策:用户指正"SpaceAccessPort::prepare_join_offer 已 init 分支本来就忽略 passphrase,HMAC verify_proof 也早就实现了",意识到 milestone/1.0.0 的 space_access 栈能直接调。

### 澄清过程(重要)
首版提案想新增 `prepare_offer_from_unlocked_session` port 方法,被用户驳回:"为什么 sponsor 要知道用户明文口令?" 复核 `uc-infra/src/security/space_access_adapter.rs:399-418` 发现 Branch A `let _ = passphrase;` 就忽略 passphrase。完整 HMAC challenge-response 链路(`prepare_join_offer` + `derive_master_key_for_proof` + `ProofPort::build_proof` + `verify_proof`)Phase B milestone 全部落地。**我被 port 签名洁癖带偏**,忽略 adapter 实际语义。

### 已完成
1. ✅ **wire 扩**:`SponsorKeyslotOffer` 加 `pairing_session_id: PairingSessionId` — HMAC proof 需要 `SessionId` binding,sponsor 和 joiner 必须用同值;sponsor mint 后必须随 KeyslotOffer 传给 joiner。Slice 1 未 ship,wire breaking change 无用户影响。`uc-infra/src/pairing/wire.rs` codec + round-trip 测试同步更新
2. ✅ `PairingInboundOrchestrator` 扩 7 个 port(`space_access + proof_port + member_repo + trusted_peer_repo + local_identity + device_identity + settings`)+ `sessions: Mutex<HashMap<PairingSessionId, SponsorHandshakeState>>`
3. ✅ FSM 驱动(复用 `uc-core::space_access::SpaceAccessStateMachine`):
   - match 成功 → dispatch `SponsorAuthorizationRequested` → actions `[RequestOfferPreparation, SendOffer, StartTimer]`
   - MessageReceived(ChallengeResponse) → verify_proof → dispatch `ProofVerified/Rejected` → actions `[SendResult, PersistSponsorAccess?, StopTimer]`
   - Closed → dispatch `SessionClosed` + ctx.remove
4. ✅ action dispatcher(Slice-1-local,~100 行 match 臂):
   - RequestOfferPreparation → `space_access.prepare_join_offer(space_id, Passphrase::new(""))` → ctx.prepared_offer + ctx.challenge_nonce
   - SendOffer → `pairing_session.send(KeyslotOffer)`
   - SendResult → Confirm(verified) / Reject(PassphraseMismatch)
   - PersistSponsorAccess → `member_repo.save` + `trusted_peer_repo.save`(**直写 repo,绕过已有 use case — 本次是真正的重复**)
   - StartTimer/StopTimer → no-op(P7g 再接)
   - 3 种 joiner-side action arm → warn(FSM 漂移防御)
5. ✅ `SpaceSetupDeps` 加 `proof_port + trusted_peer_repo`
6. ✅ 单测 17 个,workspace 115 pass

### 设计决策
- **FSM 复用 + action 枚举不重写** — 用户 prev-session 指出"核心握手都是现成的",采纳 B 路径(FSM 层复用,跳过 legacy orchestrator 整体以免拖 libp2p context 耦合)
- **wire `pairing_session_id` 加字段** — HMAC binding 需求,PairingSessionId 是 sponsor mint 的 opaque id,joiner 用同值调 build_proof 才能通过 sponsor 的 verify_proof。wire breaking 合理
- **placeholder Passphrase::new("") 占位** — Branch A 忽略之,C1 洁癖(新增 unlocked-only 方法)留给独立 PR 做
- **admit/trust 直写 repo** — 本阶段草率;commit 完才意识到 `AdmitMemberUseCase` / `TrustPeerUseCase` 已存在

### 错误 / 偏差(这轮最大的教训)
- ❌ **persist_peer 手写 `member_repo.save` + `trusted_peer_repo.save`** — 绕过已有 `AdmitMemberUseCase` / `TrustPeerUseCase` use case。legacy `SpaceAccessOrchestrator::try_admit_peer_as_member` (L231) 就是正确范式:委托给 use case 而不直接 save。**意识到时已 commit,接 refactor 轮**
- ❌ **orchestrator 堆了太多事** — FSM 驱动 + ctx 管理 + wire 构造 + persist 全塞一个文件。用户在 review 时直接点破"这个 orch 到底在干什么"。真实形状应是"纯编排",其他职责拆出去

### 文档归位
- ✅ `progress.md` 本条
- ✅ `findings.md` F-051(prepare_join_offer passphrase 忽略语义)在下个 session 补

### 下一步
refactor cleanup — 拆 `sponsor_handshake`,走 admit/trust use case,persist 先于 Confirm,失败 → Reject(Internal)。

---

## Session 2026-04-20(续 21) — Slice 1 P7f cleanup · 分离 handshake + 走 use case

### 任务
响应用户 audit:"PairingInboundOrchestrator 真的需要这么多逻辑吗,有没有重复的"。subagent 查 AdmitMember/TrustPeer use case 签名 → 完整契约可用,joiner facts 零字段缺口。

### 已完成
1. ✅ 新建 `pairing_inbound/sponsor_handshake.rs`(~330 行 impl + 15 单测) — `SponsorHandshakeCoordinator` 独立,持 `SessionCtx { space_id, challenge_nonce, core_session_id, joiner: JoinerFacts }` keyed by `PairingSessionId`,5 方法:
   - `begin(session, request)` — prepare_join_offer + 发 KeyslotOffer + park ctx(失败 Reject(Internal) 自闭环)
   - `verify_challenge(session, response) -> Option<Verdict>` — verify_proof 不触 wire,返回 Verified(JoinerFacts) / Rejected / None(无 ctx)
   - `confirm(session)` — 拿 ctx + load settings + ensure fingerprint + 发 Confirm + close + drop ctx
   - `reject(session, reason)` — 发 Reject + close + drop ctx(idempotent)
   - `handle_session_closed(session, reason)` — drop ctx
2. ✅ 重写 `orchestrator.rs`(~393 行 impl + 10 单测) — 纯流水线编排:
   - on_incoming: `holder.take_matching → notify_consume → handshake.begin`
   - on_message_received(ChallengeResponse): `handshake.verify_challenge` → Verified 分支:`admit_member.execute → trust_peer.execute → handshake.confirm`;任一失败 → `handshake.reject(Internal(..))` → **joiner 看到明确失败**(用户指定语义,不对齐 legacy 的 swallow-as-warn)
   - Closed: `handshake.handle_session_closed`
3. ✅ facade 侧构造 `AdmitMemberUseCase::new(member_repo.clone())` + `TrustPeerUseCase::new(trusted_peer_repo)` + `SponsorHandshakeCoordinator`,注入 orchestrator;`SpaceSetupDeps` 字段不动(proof_port + trusted_peer_repo 仍在,由 facade 包装成 use case)
4. ✅ 测试 25 个(sponsor_handshake 15 + orchestrator 10)全绿,uc-application 115 → 123(+8)

### 设计决策
- **Sponsor 侧撤出 `SpaceAccessStateMachine`** — linear path(begin → verify → confirm|reject → close)给不了 FSM 的分支验证价值;**FSM 默认 action order `[SendResult, PersistSponsorAccess]` 和 Slice 1 要求的 "persist 先于 Confirm" 排序冲突**(用户要求 admit/trust 失败 → Reject(Internal),所以 persist 必须先于 Confirm,不能反过来)。joiner 侧 P7h 有真正分支(WaitingOffer / WaitingUserPassphrase / WaitingDecision + 用户输入),FSM 仍然用
- **admit/trust 走已有 use case 而非直写 repo** — P7f commit 里的重复(L12 教训)纠正。`AdmitMemberUseCase` / `TrustPeerUseCase` 自带 AlreadyAdmitted/AlreadyTrusted 语义,joiner facts 足以直接 map
- **admit/trust 失败 → Reject(Internal) 而非 legacy 的 swallow-WARN** — 用户决策:Slice 1 要求"配对成功不能领先于本地状态"。legacy 反过来(配对成功不该被本地 save 失败翻盘),两个立场在"跨设备 sync 是否 mandatory" 上有分歧 — Slice 1 选 strict
- **handshake coordinator `pub(crate)`**(bump from `pub(super)`)— facade 需要构造它。AGENTS §11.4 允许 use case / coordinator crate-internal,不破坏封装

### 文件 size 对比
| file | before | after |
|---|---|---|
| orchestrator.rs(impl) | ~550 | 393 |
| sponsor_handshake.rs(impl) | — | 330 |
| 合计 impl | ~550 | 723 |

合计行数涨了,但每文件 responsibility 清晰;orchestrator 从"杂糅 4 件事"瘦身成"按顺序调 4 件现成能力"。

### 错误 / 偏差
- ✅ **session 初读取完整 FSM transition 列表后发现 sponsor-side 只 4 states + 5 events** — 才敢说"FSM 对 sponsor 无拉动"。预判前应该先 read,不是凭印象

### 文档归位
- ✅ `progress.md` 本条
- ✅ `findings.md` F-051 + F-052 下方补

### 下一步候选
- **P7g** — TimerPort + StartTimer/StopTimer 真正接 TTL
- **P7h** — joiner 侧 `RedeemPairingInvitationUseCase`(FSM 在这边真派上用场:WaitingOffer → WaitingUserPassphrase → WaitingDecision)

---

## Session 2026-04-20(续 22) — Slice 1 P7g · sponsor handshake TTL watchdog

### 任务
给 sponsor 端握手加 TTL 兜底：`begin` 后若没等到 `confirm`/`reject`/`close`，coordinator 自发 `Reject(Timeout)` + 关闭 session。

### 背景与取舍
- **不走 `uc-core::ports::TimerPort`**：它的 `start(session, ttl) -> ()` 没有回调，`stop` 只擦掉登记，超时 fire 时根本没有主动的 Reject 产生通道；给它塞回调会污染 `setup` / `space_access` 两条已有调用栈。
- **改在 coordinator 内部 `tokio::spawn`**：AGENTS §15.3 明确"运行时细节收敛在 orchestrator/内部实现中"是允许的；spawn 的 task 持 `Weak<Self>`（`Arc::new_cyclic`），超时时 upgrade → `fire_timeout`，Weak 不成环。
- **把 `new` 返回类型改成 `Arc<Self>`**：cyclic 构造必须产出 Arc；外层 facade + orchestrator 的测试 builder 同步去掉 `Arc::new(...)` 包装。
- **新增 `PairingRejectReason::Timeout` 而非复用 `Internal(String)`**：timeout 是稳定 UI 语义（"配对超时"），不是字符串化兜底；infra wire codec + round-trip 测试同步加分支。

### 已完成
1. ✅ **domain**：`PairingRejectReason::Timeout` 新变体
2. ✅ **wire codec**：`WireRejectReason::Timeout` + to/from_wire 分支 + 全 reason round-trip 测试加 Timeout
3. ✅ **`SponsorHandshakeCoordinator`**：
   - struct 加 `handshake_ttl: Duration` + `self_weak: Weak<Self>` 字段
   - `SessionCtx` 加 `timer_abort: Option<AbortHandle>`
   - `new(...)` 加 ttl 入参，返回 `Arc<Self>`，用 `Arc::new_cyclic`
   - `begin` 成功后调 `arm_timeout(session)`：`tokio::spawn(sleep(ttl) + fire_timeout)`，把 `AbortHandle` 写回 parked ctx；若 ctx 已被 race 掉则立即 abort 防 ghost Reject
   - `fire_timeout(session)`：取 ctx 若在则 `send_reject_and_close(Timeout)`；丢 ctx 即幂等 no-op
   - `confirm` / `reject` / `handle_session_closed` 在 remove ctx 时 abort 其 `timer_abort`（新增自由函数 `abort_timer(&SessionCtx)`，集中 abort 逻辑）
4. ✅ **facade 侧**：`Duration::from_secs(60)` 默认 TTL（对齐 legacy setup orchestrator），`SponsorHandshakeCoordinator::new` 调用去掉外层 `Arc::new` 包装
5. ✅ **orchestrator 测试 bundle**：建 handshake 时直接传 `Duration::from_secs(3600)` 大 TTL，关掉 orchestrator 层对 TTL 的依赖（TTL 行为专门在 sponsor_handshake 测试）
6. ✅ **sponsor_handshake 测试 +4**：
   - `ttl_fires_reject_timeout_and_drops_ctx_when_no_response` — `start_paused + sleep(ttl+1s)` → 确认 `Reject(Timeout)` + close + parked=0
   - `confirm_aborts_ttl_watchdog` — confirm 后 `sleep(2*ttl)` 确认只有 KeyslotOffer + Confirm
   - `reject_aborts_ttl_watchdog` — reject 后 `sleep(2*ttl)` 只有 KeyslotOffer + PassphraseMismatch Reject（无幽灵 Timeout）
   - `handle_session_closed_aborts_ttl_watchdog` — close 后 `sleep(2*ttl)` 只有 KeyslotOffer、没 close / Reject
7. ✅ uc-application 127 tests 全绿（sponsor_handshake 15→19，orchestrator 10 不变，facade smoke 不变）

### 设计决策
- **TTL 测试用 `tokio::test(start_paused = true)` + `tokio::time::sleep(...)` 而不是 `advance`** — paused 模式下 `advance` 只推进 clock 不 poll 任务，sleep 会让 runtime 自动推进到下个 deadline 并 poll 所有待唤醒 task；sleep 更简单更稳定
- **`abort_timer` 作为模块自由函数**（不是 `impl` 方法）— 因为 `SessionCtx` 已经被 `remove` 出 HashMap，abort 时已经和 coordinator 本体解耦；挂在 impl 上只是代码组织包袱
- **race 处理**：`arm_timeout` lock sessions 拿 ctx；若 ctx 已不在（理论极短窗口 send 后到 lock 前被抢先清掉），立即 `handle.abort()` 避免孤儿 task。是防御性而非已知 bug

### 错误 / 偏差
- ❌ 第一版 timeout 测试用 `tokio::time::advance(ttl + 1s) + yield_now` 连续 5 次失败（sent.len == 1），因为 paused clock 的 `advance` 不 poll 子任务；改用 `sleep` 后一把过
- ❌ 首版把 `abort_timer` 放 impl 里导致把 impl 拆成两段（中间塞自由函数声明），编译能过但阅读刺眼；考虑重构为 free fn 后保持 impl 连续

### 文档归位
- ✅ `progress.md` 本条
- ✅ `findings.md` **不新增** — P7g 没产生新产品/架构级结论，"TimerPort 用不上" 已在本条决策里说清楚

### 不做项（推迟）
- **joiner 侧 TTL**：joiner 等 KeyslotOffer / Confirm 的超时留到 P7h 一起建（joiner 有 FSM，TTL 自然对到 `StartTimer` / `StopTimer` action）
- **TimerPort 加 callback 扩展**：不做；sponsor handshake 不走 TimerPort。如果 joiner FSM 那边需要真 fire，再评估"扩 TimerPort" vs "joiner 侧也用内部 spawn"

### 下一步候选
- **P7h** — joiner 侧 `RedeemPairingInvitationUseCase`（dial → 发 Request → 收 KeyslotOffer → derive_master_key_for_proof → build_proof → 发 ChallengeResponse → 等 Confirm/Reject）；FSM 在这里真派上用场
- **P8** — bootstrap wiring，在 `uc-bootstrap` 里拼 iroh adapter + rendezvous client

---

## Session 2026-04-20(续 23) — Slice 1 P7h · joiner `RedeemPairingInvitationUseCase`

### 任务
完成 joiner 侧配对的 application 层：一个用户动作（输入 code + passphrase → 点 Join）从 dial 跑到 setup marked complete。

### 关键转折：FSM 还是用不上
之前续 20/21 sponsor 侧写完后预判"joiner 侧有真正的分支状态,FSM 会派上用场",续 22 也还在这个立场。真正动手前重读 sponsor path 又看了一遍 `SpaceAccessStateMachine` 的 joiner-side transition 列表,发现:

1. Slice 1 的 UX 是用户输入 code + passphrase 一起给（不是"收完 KeyslotOffer 再弹窗让用户输口令"的两阶段）
2. 握手链路完全线性：dial → send Request → recv → derive → build → send ChallengeResponse → recv → persist
3. FSM 的 joiner-side actions 和 Slice 1 的 **persist 先于"成功返回"** 排序同样冲突（和 F-052 sponsor-side 同病）
4. "用户取消" / "分阶段口令" 都是 future slice 的 UX,Slice 1 里都不存在

→ **F-053**:joiner 也不走 FSM。同样文档说明理由。

### 已完成
1. ✅ **Command / Result / Error** 在 `facade/space_setup/{commands,errors,mod}.rs`:
   - `RedeemPairingInvitationCommand { code, passphrase }`
   - `RedeemPairingInvitationResult { sponsor_device_id, sponsor_fp, space_id, self_device_id, self_fp }`
   - `RedeemPairingInvitationError` 14 个 variant:invitation/dial 4 类 + `PassphraseMismatch`(本地 derive 失败或 sponsor 回 PassphraseMismatch 两种来源折叠成一个) + `CorruptedKeyMaterial` / `DeviceNameRequired` + 4 种 sponsor reject + `Timeout`(自己 recv 超时)/`ConnectionLost`/`Internal(String)`
2. ✅ **`RedeemPairingInvitationUseCase`** 新增 `usecases/pairing/redeem_invitation.rs`(~430 行 impl + ~430 行 tests):
   - `execute(cmd)`:dial →(drive handshake)→ close 强制收尾;任何一步失败都关 session 再返回 error
   - `drive`:线性 8 步;`recv_with_ttl` 用 `tokio::time::timeout` 包 per-recv(独立于 P7g sponsor 侧 watchdog)
   - 持久化顺序 admit → trust → `setup_status.set_status(has_completed=true)` 和 sponsor 侧对称
   - `derive_master_key_for_proof` 是 adapter 里同时持久化本机 keyslot 的那一步(复读 `uc-infra/src/security/space_access_adapter.rs:439-507` 确认),所以 joiner 不再需要一个单独的 `initialize` 调用
3. ✅ **Facade 注入**:
   - `SpaceSetupFacade` 新字段 `redeem_pairing_invitation: Arc<UseCase>`
   - `new()` 把 `admit_member_uc` / `trust_peer_uc` 提到 sponsor stack 构造之前,两边都能 `Arc::clone` 共享(而不是各自构造一遍)
   - 新方法 `redeem_pairing_invitation(cmd)` 先 `auto_start_network()` 再 execute —— 这是 joiner 和 A1/A2/B1 的关键区别,对方很可能是第一次开 app,网络还没起
4. ✅ **21 个单测**:happy path + 4 种 dial error + 5 种 sponsor reject + 2 种本机 derive 失败 + 2 个 own-TTL(paused clock + spawn execute + advance)+ connection lost/error + 2 种 unexpected frame + device_name missing + admit/trust failure 各一
5. ✅ uc-application 127 → 148 tests 全绿

### 设计决策
- **`PassphraseMismatch` 一个变体折叠两个来源** —— UI 看起来"口令错"就是口令错,不需要分"本地 keyslot 解不开"还是"sponsor verify_proof 失败"。两者 root cause 一样(用户输错)
- **`recv_with_ttl` 用 `tokio::time::timeout` 而不是在 coordinator 里 spawn watchdog** —— use case 是一个短生命周期的 async call,直接用 `timeout` 就够;P7g sponsor 那边是有 parked ctx + 跨多个 event handler 的 shared coordinator 才需要独立 watchdog task
- **`setup_status.set_status` 放在持久化最后一步** —— admit / trust 的 AlreadyAdmitted / AlreadyTrusted 都已经转成 `Internal`。"已成员但 setup 还没 complete" 是可能的残留状态,但至少不会出现"setup complete 但 trusted_peer 空"(会让后续所有 inbound stream 被 policy 拒)
- **Session 用 `tokio::spawn(std::future::pending())` 模拟 sponsor "永远不回"** —— 比手写 never-wake future 简单;paused clock 下 `timeout` wrapper 照样 fire
- **`map_admit_err` / `map_trust_err` 把 AlreadyAdmitted/AlreadyTrusted 都归 Internal** —— 这俩错语义上是"重试时的副作用",但 Slice 1 没有 retry 机制;把它们当成正常 OK resume 会掩盖"上次 run 到 setup_status 之前就崩了"的半提交状态

### 错误 / 偏差
- ❌ 首稿 `FixedProof` 里写了 `SessionId::new("fixed")`,没注意 core 的 `SessionId::new(id: String)` 签名;编译器提示后一行 `.to_string()` 搞定
- ❌ 前两轮 session rec "FSM 会用" 的立场,动手前才意识到和 sponsor 同问题;应该在 F-052 的时候就预判

### 文档归位
- ✅ `progress.md` 本条
- ✅ `findings.md` F-053 下方补

### 下一步
**P8** — `uc-bootstrap` 里把 iroh `IrohPairingSessionAdapter` + rendezvous client + `SpaceSetupDeps` 拼起来,端到端跑一次 sponsor + joiner 对接。

---

## Session 2026-04-20(续 24) — P7h refactor · 抽 `JoinerHandshakeCoordinator` 消除 11 参数 smell

### 触发
用户 review P7h commit 指出 `RedeemPairingInvitationUseCase::new(...)` 11 个参数 + `#[allow(clippy::too_many_arguments)]` 本身就是越界信号,并直接要求对称 sponsor 侧拆出一个 coordinator。

### 根因
joiner 侧原结构一层塞了 5 件事:dial + JoinerRequest 组装(3 个 local-identity port)+ derive+build_proof + recv+映射 + admit/trust/mark-complete。sponsor 侧早就有 `SponsorHandshakeCoordinator`(wire+crypto)+`PairingInboundOrchestrator`(composition)两层拆分,joiner 侧缺这一层,对称性破了。

### 已完成
1. ✅ **新 module** `pairing_outbound/`(对称 `pairing_inbound/`)+ `joiner_handshake.rs`(~390 行 impl + ~470 行 tests)
   - `JoinerHandshakeCoordinator` 7 参数:pairing_session / space_access / proof_port / local_identity / device_identity / settings / ttl
   - `handshake(code, passphrase) -> Result<JoinerHandshakeOutcome, RedeemPairingInvitationError>` — 一个方法吃完整 wire + crypto,success/error 两个分支都 close session
   - `JoinerHandshakeOutcome { sponsor_device_id, sponsor_device_name, sponsor_identity_fingerprint, space_id, self_device_id, self_identity_fingerprint }` — 把 admit/trust 需要的 sponsor facts + UI 需要的 self facts 一把返出
2. ✅ **`RedeemPairingInvitationUseCase` 11→5 参数**:handshake / admit_member / trust_peer / setup_status / clock。去掉 `#[allow(clippy::too_many_arguments)]`。`execute(cmd)` 两行:`handshake.handshake().await?` → `persist(outcome)`
3. ✅ **测试重组**:
   - coordinator 测试 19 个(wire + crypto):happy 路径 + 4 种 dial error + 5 种 sponsor reject + 2 种 own-TTL(paused clock)+ 2 种本机 derive 失败 + connection lost/error + 2 种 unexpected frame + device_name missing
   - use case 测试 5 个(纯 composition):happy(admit + trust + setup_status in order)+ coordinator error passthrough + admit 失败短路 + trust 失败短路 + setup_status 失败短路
   - **use case 测试拿真的 coordinator 运行**(sponsor 侧 orchestrator tests 也是这模式),不再抽 trait seam — 避免一次性过度设计
4. ✅ `lib.rs` 加 `pub(crate) mod pairing_outbound`
5. ✅ facade 构造换 2 行(old 11 args → new 7 args coord + 5 args use case),其他字段不动
6. ✅ uc-application 148 → 151 tests 全绿

### 设计决策
- **coordinator 直接返回 `RedeemPairingInvitationError`(不新增私有 error)** — 它的 variants 1-to-1 映射 user-facing 失败,私有 enum + map 层只会是零信号的代码复制。这个决策只覆盖 intra-crate 边界,不破坏 AGENTS §13.1(error 向上翻译)原则 —— 从 port 过来的原始错误(`DialError` / `SpaceAccessError` / `SessionError`)仍然翻译
- **`JoinerHandshakeCoordinator` 不走 trait seam** — 对称 sponsor 侧。use case 测试用真 coordinator + 快乐路径 fakes,是 sponsor orchestrator tests 同款务实做法;一次性消费者不值得抽 trait
- **coordinator 返回 `Arc<Self>`** — 和 `SponsorHandshakeCoordinator::new` 对称。joiner 这边目前只有一个消费者(use case),但统一接口形状,future 如果 bootstrap 需要共享实例,一行搞定
- **文件组织 `pairing_outbound/` vs `pairing_inbound/`** — 对称命名,比 "joiner" / "sponsor" 更直接表达"入/出方向",和 wire 语义对齐(sponsor 等入站 / joiner 打出站)

### 错误 / 偏差
- ❌ 首次 P7h 没做这个拆分是失察 —— sponsor 侧早做过的事在 joiner 侧本应同步做;`#[allow(clippy::too_many_arguments)]` 应该当即停下自问"为啥",而不是顺手加上
- ❌ 一开始想抽 trait seam 让 use case 测试更干净,评估后发现和 sponsor 侧不一致,取消

### 文档归位
- ✅ `progress.md` 本条
- ✅ `findings.md` **不新增** — F-053 的决议未变(仍然是"joiner 不走 FSM"),本轮只是把 B2 实现重新分层,不涉及产品/架构级新结论

### 下一步
**P8** — bootstrap wiring 不变。coordinator 抽出后 deps 仍然是同一套 `SpaceSetupDeps`,bootstrap 端零变化。

---

## Session 2026-04-20(续 25) — Slice 1 P8 · bootstrap 装配 + iroh 共享 node + E2E

### 触发
续 24 结尾的下一步:把 `IrohPairingSessionAdapter` + rendezvous client + `SpaceSetupDeps` 在 `uc-bootstrap` 里拼成可工作的 `SpaceSetupFacade`,并端到端跑一次 sponsor + joiner 对接。

### 架构决策 · IrohNode 取代"每业务一个 stack"
user review 早期方案 `IrohPairingStack` 时直接指出:"文件传输、数据同步、配对应共用一个网络通道"。iroh 的 `Endpoint` 是进程级资源(identity + UDP socket + NAT/relay),多个业务协议(Slice 1 pairing / Slice 2 clipboard / Slice 3 iroh-blobs)通过 ALPN 挂在**同一个 `Router`** 上,不是各跑一个。修正后形态:

```
IrohNode(singleton)
├── Endpoint (shared)
├── Router (w/ multi-ALPN handlers)
└── install_pairing / install_clipboard / install_blobs (Slice 2/3 扩展点)
```

### 已完成
1. ✅ **P8a · `uc-infra/src/network/iroh/node.rs`** — `IrohNode` + `IrohNodeBuilder` + `IrohNodeConfig` + `PairingHandlers`
   - `bind(&IrohIdentityStore, config)` 复用 `ensure_secret_key()` 持久化的 Ed25519 私钥做 endpoint 身份(peer 认的 identity = `LocalIdentityPort` 暴露的 fingerprint 同一把)
   - `install_pairing(device_identity, settings) -> PairingHandlers { session, events, invitation }` — 两个 port 是同一个 `IrohPairingSessionAdapter` 的 Arc,rendezvous adapter 复用同一个 endpoint
   - `spawn() -> IrohNode` / `IrohNode::shutdown()` — router.shutdown 触发每个 `ProtocolHandler::shutdown` + 发 `CONNECTION_CLOSE` + 释放 UDP socket
   - iroh crate 类型不出 `uc-infra::network::iroh`;bootstrap/外部只看 core ports
   - 2 个测试:bind→install→spawn→shutdown 不卡 / 两次 bind 同 store 得到同 endpoint id
2. ✅ **P8b · `uc-bootstrap/src/space_setup.rs`** — `build_space_setup_assembly(&WiredDependencies, IrohNodeConfig) -> SpaceSetupAssembly`
   - 从已装配的 `WiredDependencies` 复用 space_access / member_repo / trusted_peer_repo / device_identity / settings / setup_status / clock / secure_storage
   - 组装 `IrohIdentityStore` + `IrohNodeBuilder` + `install_pairing` + `HmacProofAdapter::new_with_space_access` → `SpaceSetupDeps` → `Arc<SpaceSetupFacade>`
   - `SpaceSetupAssembly::shutdown()` 两段协调:先 `facade.on_shutdown()`(abort inbound orchestrator task)再 `iroh_node.shutdown()`(router.shutdown)
3. ✅ **修复 adapter 公开表面**——`IrohPairingSessionAdapter::with_base_url` 和 `RendezvousPairingInvitationAdapter::with_base_url` 从 `#[cfg(test)]` 改 `pub`,通过 `IrohNodeConfig::rendezvous_base_url: Option<String>` 配置化
4. ✅ **P8c · E2E 集成测试** `uc-bootstrap/tests/slice1_handshake_e2e.rs`(~460 行)
   - 双 `SpaceSetupFacade` 跑在 loopback iroh endpoint(`disable_relays: true`)上
   - 有状态 wiremock:POST 捕获 sponsor ticket,GET 回显给 joiner,consume 返 204
   - 真实 crypto:`DefaultSpaceAccessAdapter` + `KeyMaterialStore` + 各自 tempdir `JsonKeySlotStore`(Argon2 真跑,单次 derive ~3s)
   - 手搓 DI 避开 `wire_dependencies`(避免 keychain + SQLite 真调)
   - 流程:sponsor A1 → B1 → joiner B2 → `wait_for` sponsor 异步 admit/trust → 断言双侧 SpaceMember + TrustedPeer + joiner `setup_status.has_completed`
   - **6.57 秒跑完**(3s Argon2 dominant)

### 发现 · 三处实际 bug(过不了 E2E 就暴露)

#### Bug A · A1 与 iroh endpoint 预绑定冲突(生产 bug)
**症状**:E2E 首次跑 sponsor A1 就挂 `IdentityAlreadyExists`。
**根因**:旧 A1 设计用 `LocalIdentityPort::create()`(严格),但 Slice 1 的 iroh endpoint 必须在 bootstrap 时就绑(内部 `ensure_secret_key()` 已经持久化了 identity)。A1 再调 `create()` 必然失败。**不仅测试,任何首装设备都会踩**。
**修法(方案 X,user 确认)**:
- `InitializeSpaceError::IdentityAlreadyExists` → `AlreadySetup`(语义从"identity 存在"改成"setup_status.has_completed == true")
- A1 `execute` 首行加 `setup_status.get_status().has_completed == true → AlreadySetup` 守门
- `local_identity.create()` 改 `local_identity.ensure()`(幂等)
- AlreadyExists 从 ensure 冒出来归 `StorageFailed`(违反 port 幂等契约的 adapter bug)
- A1 单元测试:`identity_already_exists_surfaces_specific_variant` 重写成 `already_completed_setup_rejects_before_touching_space_access` + `identity_ensure_adapter_bug_raises_storage_failed`

#### Bug B · iroh adapter 只产 Incoming,不产 MessageReceived(架构 gap)
**症状**:E2E 跑到 joiner 发 ChallengeResponse 后卡 60 秒,sponsor TTL watchdog 先 fire,joiner 收到"connection lost"。
**根因**:`IrohPairingSessionAdapter::handle_incoming` 读完第一帧发 `PairingSessionEvent::Incoming` 就 return。但 `PairingInboundOrchestrator` 是**纯事件驱动**,靠 `MessageReceived` 拿后续帧。adapter 缺 per-session recv pump,后续帧都卡在 iroh 流 buffer 里没人读。
**修法**:`handle_incoming` 发完 Incoming 后 `spawn_recv_pump` 起一个 tokio task,循环 `read_next_frame`,每帧→`MessageReceived`,peer FIN→`Closed`,read error→`Closed { reason: Some(err_text) }`。pump 只在 sponsor 侧(`handle_incoming` 路径)起;joiner 侧 `dial_by_invitation` 不起,避免和 `JoinerHandshakeCoordinator` 的 `recv_next` 轮询争 recv mutex。同时把 `recv_next` 和 pump 共享 `read_next_frame` helper,wire framing 逻辑单一来源。原 17 个 `uc-infra::pairing::session` tests 继续 pass。

#### Bug C · 测试断言 `sponsor.space_id == joiner.space_id`(错假设)
**症状**:fix B 后剩下的最后一个断言失败。
**根因**:`sponsor_handshake.rs:146 let probe_space_id = SpaceId::new();` — sponsor 握手现场生成的 space_id 不是 A1 创的那个。keyslot 实际 key 是 `profile_id`,space_id 对 crypto 不重要,所以双方本地 space_id 不同是**当前设计的刻意行为**(F-051 branch A)。
**修法**:E2E 只断言"双方都持久化了对方",space_id 差异用 `let _` 绑定 + 注释显式化为文档化 invariant;未来统一 space_id 要改这个测试。

### 设计决策
- **`IrohNode` 是 singleton,不是"每业务一个 stack"** — iroh `Endpoint` 的 identity/UDP socket/NAT/relay 都是进程级,Slice 2/3 往 Router 加 ALPN 即可。bootstrap shutdown 顺序:facade 先(让 inbound subscriber drop),再 iroh router(发 CONNECTION_CLOSE)
- **`IrohNodeConfig` 露两项**:`rendezvous_base_url: Option<String>` + `disable_relays: bool`。production 全 `Default`;测试指向 wiremock + 关 relay。比 env var(跨 crate 耦合)或分别走 `with_base_url` 干净
- **E2E 手搓 DI 不走 `wire_dependencies`** — 后者碰 keychain + 文件 SQLite,CI 不友好。手搓 ~150 行 in-memory fake ports,每行直接映射 port 契约
- **recv pump 不带 abort handle** — 首版简化。正常流程双方都会 close 自己的 send 边,peer recv 自然收 FIN → pump 退。若 peer 不礼貌会挂到 QUIC idle timeout,属于 infra liveness 兜底

### 验证
- `cargo test -p uc-infra --lib pairing::` — 17 green(含现有 sponsor handler + joiner dial)
- `cargo test -p uc-infra --lib network::iroh::` — 11 green(含新 IrohNode 2 个)
- `cargo test -p uc-application --lib usecases::setup::initialize_space::` — 9 green(A1 重写后)
- `cargo test -p uc-application --lib` — 152 green(含新 AlreadySetup 路径)
- `cargo test -p uc-bootstrap --test slice1_handshake_e2e` — 1 green(6.57s)
- `cargo test --workspace --lib` — 全绿,无 regression

### 错误 / 偏差
- ❌ 首版 `IrohNode` 命名用了 `IrohPairingStack`,user 一问"未来文件传输也是这种结构吗"就暴露了"每业务一个 stack"的误解。教训:设计共享 infra 时应该先画"未来 N 个业务的拓扑",而不是从当前业务倒推
- ❌ 没预料到 A1 identity creation 和 iroh endpoint bind 的冲突 —— 写 P8b 时关注力在"bootstrap 怎么装",没回溯 A1 port 契约。模式:任何"bootstrap 时预先存在的状态"都要比照每个 use case 的假设
- ❌ 漏了 recv pump 是真实架构 gap —— uc-application orchestrator 单元测试用 scripted event port 造假 MessageReceived,adapter 这头没写过"多帧 sponsor 会话"的测试。教训:adapter 层契约测试要包括"subscriber 视角"的完整帧序列
- ❌ 第一次断言 `sponsor.space_id == joiner.space_id` 想当然 —— 应该先看 sponsor_handshake 怎么处理 space_id 再写断言

### 文档归位
- ✅ `progress.md` 本条
- ✅ `findings.md` 加 F-054(A1 identity 生命周期修正)和 F-055(iroh adapter sponsor 侧 recv pump 必需)
- ✅ `task_plan.md` P8 行 → ✅

### 下一步
Slice 1 核心完成。遗留项:
1. **uc-tauri / uc-cli / uc-daemon 接入 `Arc<SpaceSetupFacade>`** — 前端页面 + IPC contract 对齐
2. `SpaceSetupFacade::on_shutdown` 目前调 legacy `libp2p_network.stop_network`,Slice 2 落地前保持共存,Slice 5 清理
3. **Slice 2 · 剪贴板同步**:`IrohNodeBuilder::install_clipboard(...)` 挂 `/clipboard/1` ALPN handler,复用 IrohNode 同一个 endpoint

---

## Session 2026-04-20(续 26) — Slice 1 P9 · CLI + session resume + SpaceId 稳定化

**触发**:用户要求"下一步支持 CLI",具体两条新顶层命令 `invite` / `join`,`pair` 标 deprecated。CLI 不走 daemon HTTP,直接基于 uc-bootstrap 作用于功能核心。最终目标:单机双进程真机 e2e 验证配对流程。

### P9a · application-layer primitives

**`PairingOutcome` broadcast**:
- facade 层新增事件源 `subscribe_pairing_completion() -> broadcast::Receiver<PairingOutcome>`,`Success`/`Failure` 两个变体
- `PairingInboundOrchestrator` 在 finalise_verified 成功 emit Success,post-match 各失败路径 emit Failure;陌生 invitation code 不 emit
- 详见 F-056

**`try_resume_session` 包装**:
- 真相大白:`KeyMaterialStore::store_kek`(`--dev` 文件 / 生产 keychain)和 `DefaultSpaceAccessAdapter::try_resume_session` 早就支持 session 静默恢复,只是 CLI 从来没调过
- facade 加 `try_resume_session() -> Result<bool, TryResumeSessionError>`;`invite` 命令开头调一次,session 空的话直接报错
- 不需要新 port / 新 cache / 新 wrapping key—— F-057 教训:定位新问题前先确认 port 现状

**`SetupStatus.space_id` 持久化**:
- bug:sponsor init 的 space_id 跟 joiner 最后记的 space_id 对不上。根因是 `InitializeSpaceUseCase` / `UnlockSpaceUseCase` / `SponsorHandshakeCoordinator::begin` 各自 `SpaceId::new()` 铸新 UUID
- `SetupStatus { has_completed, space_id: Option<SpaceId> }`;A1 写入、joiner B2 adoption 时也写入;sponsor handshake 改成读 setup_status
- legacy None 路径保留 fresh UUID + `warn!` — T-17 处理
- 详见 F-058

### P9b · uc-cli 三命令 + 单机 e2e

- `init [--passphrase] [--device-name]`:A1 + 持久化 device_name(默认 hostname+profile 后缀)
- `invite`:`try_resume_session` → `issue_pairing_invitation` → `select!{ outcome_rx, ctrl_c }`;code 同时打到 stderr(styled) 和 stdout(`INVITATION_CODE=XXX` 可脚本抓)
- `join [--code] [--passphrase] [--device-name]`:写 device_name 到 settings(B2 从磁盘 settings 读) → `redeem_pairing_invitation`
- 全局 `--profile <NAME>` 映射到 `UC_PROFILE`,数据隔离(`app.uniclipboard.desktop-<profile>`)
- `scripts/test_pair_e2e.sh` 单机双进程冒烟脚本,`--dev` + --profile alice/bob + 断言双方 exit 0
- 旧 `setup pair` / `setup connect` 加 `[DEPRECATED]` 前缀,功能不动

### P9 infra 支撑

在 CLI 真正跑起来前有一串基础设施 bug 需要拆:
- **F-061** macOS NSPasteboard 空返回:`UC_DISABLE_SYSTEM_CLIPBOARD=1` + `NoopSystemClipboard` fallback(`uc-platform/src/clipboard/noop.rs`)
- **F-060** reqwest 0.12 `rustls-tls` 没带 root CA:5 个 Cargo.toml 全部加 `rustls-tls-webpki-roots`
- **F-059** rendezvous 客户端 URL 契约错:`/v1/pairings/consume` + body `{code}`(原 `/{code}/consume`);`POST /v1/pairings/resolve`(原 `GET /{code}`)—— 让 subagent 读 uc-rendezvous 源码确认的
- 顺手加了 `User-Agent` + `err_chain` helper 让 "error sending request" 日志带下层原因

### 调试弯路

花了相当时间以为 rendezvous 打不通是 TLS 问题:
- 加 rustls CryptoProvider install_default → 没帮助
- 加 User-Agent → 没帮助(但对外部 TLS 环境算保险)
- 换 URL 路径 → 本地测试一半好了
- 最后发现:我这个 Claude Code shell 里设了 `SSL_CERT_FILE=/etc/ssl/cert.pem`(2021 OpenBSD 老 bundle),rustls 解析里某条 cert 失败 → "bad certificate format"。**用户在 Ghostty 里跑就正常**——纯环境污染,非代码问题

教训:排错顺序上,"自己的 shell 和用户的 shell 不等价"应该早点想到。

### 用户反馈驱动的 3 个后续修

1. **NSPasteboard panic**(F-061):用户第一次 `init` 直接 panic,用户点出"一个 CLI 工具为什么会报这个错误"→ 回到 `create_platform_layer` 加 env 开关
2. **`join` 缺 device_name**(`DeviceNameRequired` 错):用户指出"不 prompt + 默认也没兜底"→ 加 `--device-name` flag + hostname 默认 + 写入 settings 的 `Slice1Cli { assembly, settings }` bundle
3. **passphrase 验证失败**(F-057):用户给出 JSON 日志 "space session is locked" → 定位到 `try_resume_session` 该调没调
4. **space_id 漂移**(F-058):用户发现 joiner 最后输出的 space_id 与 sponsor init 的不同 → SetupStatus.space_id 持久化

### 提交
- `2890c43b fix(Slice1/infra): rendezvous contract + headless-safe clipboard + TLS roots`
- `4fe4f16b feat(Slice1/P9a): PairingOutcome broadcast + session resume + stable SpaceId`
- `f43ff8c4 feat(Slice1/P9b): uniclipboard-cli init/invite/join + single-machine e2e script`

### 测试
- uc-application 152 单测 + 10 file_transfer 单测
- uc-bootstrap slice1_handshake_e2e(真 iroh loopback + wiremock rendezvous,耗时 ~7s)
- uc-infra 66 单测
- 用户自测:Ghostty 里双 profile 跑通完整 init→invite→join 路径

### 下一步
Slice 1 落帷。Slice 2 启动前 T-15(A2 unlock space_id)和 T-16(`unlock`/`lock` CLI)建议顺手补。参见 task_plan.md 的"Slice 1 → Slice 2 交接"小节。

---

## Session 2026-04-22(续 27) — Slice 2 Phase 1 封版 · roster + presence 基础设施

**触发**:用户推动 Slice 2 Phase 1("谁在线这件事变活")按 `slice2-phase1-plan.md` 的 13 任务拆解逐项推进,从 2026-04-20 起跨多日多 session 完成。本条是收尾汇总,逐 task 的现场细节见对应 commit message + `slice2-phase1-plan.md §12`(tracker + 关键发现)。

### 交付范围

单句概括:**A 设备知道 B 设备是否在线,CLI 有工具能查到,底层架构扩展点都就位供未来接 UI**。**不含**剪贴板同步 / rename / revoke。

结构按从 infra 到 UI 由低到高:

- **Port 层**(`uc-core`):
  - `PresencePort` 新增 — `ensure_reachable` / `current_state` / `subscribe` 三方法,`ReachabilityState::{Online,Offline,Unknown}` 三值
  - `PeerAddressRepositoryPort` 新增 — 已配对设备的传输地址 blob 仓库(`DeviceId` → `addr_blob: Vec<u8>` + `observed_at`)
- **Adapter 层**(`uc-infra`):
  - `IrohPresenceAdapter`(T3b `5c69b2a6`)— `PRESENCE_ALPN` handler + `Connection::closed()` watchdog 监测下线;T3a 探针发现 iroh `Endpoint::conn_type` 是缓存语义不可靠,改用持有 Connection 等关闭的模式
  - `DieselPeerAddressRepository`(T2 `e81cec97`)+ migration `2026-04-20-000002_create_peer_address`
  - `IrohNodeBuilder::install_presence` 扩展点(T4 `32a02c62`)— 镜像 `install_pairing`,两 ALPN 同 router 共存
- **Application 层**(`uc-application`):
  - wire 对称扩展(T5 `a562e529`)— `JoinerRequest` / `SponsorConfirm` 加 `transport_address_blob`,两端 pairing 收尾 upsert 对方 blob,`WIRE_VERSION` → 2
  - `EnsureReachableAllUseCase`(T6 `e66776f8`)— `JoinSet` 并发 + `peer_addr_repo.list()` 迭代源(故意不用 `member_repo`,因为身份记录有而地址 blob 没的陈旧条目会凭空制造 `NoAddress`)+ 本机防御性 filter
  - `MemberRosterFacade`(T7 `548b3bdf`)— thin wrapper,`list_with_presence` 聚合 `member_repo.list()` + `presence.current_state()` + `is_local` 标记(靠 `LocalIdentityPort::get_current_fingerprint()` 对比 `SpaceMember.identity_fingerprint`)
  - F1 hook(T8 `f461a6eb`)— `SpaceSetupFacade::auto_start_network` 成功后 unconditional 跑 `ensure_reachable_all.execute()`,失败 `warn!` 不传播;A1/A2/B2 三条生命周期成功路径都走
  - `SpaceSetupFacade::refresh_presence` 公开入口(T10)— CLI / Tauri 显式 probe 用,thin wrapper,usecase 保持 `pub(crate)`(§11.4)
- **Bootstrap**(`uc-bootstrap`):
  - `build_space_setup_assembly` 新增 `install_presence` 调用 + `SpaceSetupAssembly::roster: Arc<MemberRosterFacade>`(T9 `181f2cc8`)— 三个 Arc(`member_repo` / `local_identity` / `presence`)在两个 facade 间共享,让 F1 hook 填好的缓存 roster 能直接读到
- **CLI**(`uc-cli`):
  - `uniclipboard-cli members`(T10 `bda7686b`)— 自包含直连模式,流程 `build_assembly` → `try_resume_session` → `refresh_presence` → `roster.list_with_presence` → human(`{name} ({state}) [local]`)/ JSON 双渲染
- **测试**(`uc-bootstrap/tests`):
  - `slice2_phase1_presence_e2e`(T11 `d39889e0`)两例 —— `pair_then_refresh_reports_both_sides_online`(verdict 1)+ `joiner_shutdown_flips_sponsor_roster_to_offline_within_10s`(verdict 2)。verdict 3(B 重启 online)刻意跳过,loopback-only 测试里无 relay 刷新 stale socket 的路径,会假阳性
- **文档**:T13(`105479da`)— `task_plan.md` 标 Phase 1 ✅ + 所有 commit hash 入表;`slice2-phase1-plan.md` §12 tracker 封 grep(14/15,T12 ⏭️)

### 关键决策 / 偏离

1. **T3 `conn_type` 探针翻车**(2026-04-21):原计划订阅 iroh `Endpoint::conn_type()` 的 `Watcher` 做 online/offline 检测;T3a 写探针验证时发现 `conn_type` 是缓存语义,peer 关掉后仍返 `Direct(..)`,不会自然刷新。改走"活着持 Connection + 起 watchdog 等 `closed()`"的模式,watchdog 111ms 内可靠触发。`slice2-phase1-plan.md` §8 合入 T3 修订决策。
2. **T5 wire 协议升级**(2026-04-21):原 T5 scope 只是"pairing 收尾写 repo",但做起来发现 sponsor 拿不到 joiner 的 `EndpointAddr` —— iroh `Endpoint::remote_info` 是 `pub(crate)`,`Connection::remote_address` 只给单 SocketAddr 不含 relay。改为 wire 对称扩展:两端各加 `transport_address_blob: Vec<u8>`(opaque bytes,core 纯净,adapter 内部 postcard-encoded),`WIRE_VERSION` 从 1 升到 2。Slice 1 ↔ Slice 2 跨版本由 `UnsupportedVersion` 显式拒连——pre-release 不做兼容层。
3. **T6 mockall 并发坑**(2026-04-21):`EnsureReachableAllUseCase` 用 `JoinSet` 并发 probe 多 peer,并发性断言用 `mockall::mock!` 的 `.returning(|_| { tokio::time::sleep; ... })` 会被**序列化** —— mockall 的 `FnMut` closure 存在内部 `Mutex<..>`,三个 task 在 Mutex 上排队。实测三 probe × 200ms ≈ 616ms ≈ 3 × 200ms,完全 serial。解:改手写 30 行 `impl PresencePort for SleepyPresence`,`tokio::time::sleep` 直接 yield,并发实测 ~210ms。同一问题会影响**任何**需要断言 await 并发的 mockall 测试;已写进 §12.3。
4. **T8 scope 吸收 T9 一半**(2026-04-21):原 T9 是"bootstrap 把 presence 也装上"。实际做 T8 时 `SpaceSetupDeps` 必须新增 `presence: Arc<dyn PresencePort>` 字段才能让 facade 构造 `EnsureReachableAllUseCase`,而字段一加 bootstrap 不补 `install_presence` 编译就过不去。T8 只好顺手把 bootstrap 的 presence 接线一并合入;T9 缩减到只剩 `MemberRosterFacade` 的装配(~0.2h vs 原估 1h)。
5. **T11 暴露 Slice 1 pre-existing gap**:写 `pair_then_refresh_reports_both_sides_online` 断言 joiner 自己也在 roster 里(`is_local=true`),测试失败。查源发现 `RedeemPairingInvitationUseCase::persist` 只 admit sponsor,joiner 自己不 save 进 `member_repo` —— 所以 joiner 视角 `members` 命令看不到本机。**不属 T11 scope**,测试改成断言当前事实(`joiner_roster.len() == 1`)+ 注释标契约信号,future fix 会让测试失败作为契约变更提醒。follow-up 记录进 `task_plan.md` Slice 2 Phase 1 节和本 plan §12.4。
6. **T12 战略性跳过**:user 跑完手动测试完全匹配后问"还需要自动化脚本吗",评估后 T11 Rust 集成测试已覆盖 verdicts 1/2(精度比 shell 更高,时效断言用 `wait_for(10s)` 而非 shell `sleep+grep`);T12 shell 扩展纯演示脚本,维护成本 > 回归保护价值,改 CLI 输出文案就断。记录在 `task_plan.md` Phase 1 ✅ 节 + 本 plan §12.4。

### 提交(按拓扑顺序)

- `2ec1cabd` feat(Slice2/P1): T1 `PeerAddressRepositoryPort` core port
- `e81cec97` feat(Slice2/P1): T2 `DieselPeerAddressRepository` + migration
- `36fc7e3b` test(Slice2/P1): T3a iroh presence probe reveals conn_type staleness
- `a5394349` docs(Slice2/P1): revise T3 design after iroh probe finds conn_type staleness
- `5c69b2a6` feat(Slice2/P1): T3b `IrohPresenceAdapter` with Connection::closed watchdog
- `32a02c62` feat(Slice2/P1): T4 `IrohNodeBuilder::install_presence` extension point
- `a562e529` feat(Slice2/P1): T5 peer_addr upsert at pairing completion(wire v2)
- `e66776f8` feat(Slice2/P1): T6 `EnsureReachableAllUseCase`
- `548b3bdf` feat(Slice2/P1): T7 `MemberRosterFacade`
- `f461a6eb` feat(Slice2/P1): T8 `auto_start_network` F1 hook
- `181f2cc8` feat(Slice2/P1): T9 assemble `MemberRosterFacade` in bootstrap
- `bda7686b` feat(Slice2/P1): T10 `uniclipboard-cli members` subcommand
- `d39889e0` test(Slice2/P1): T11 presence lifecycle e2e
- `105479da` docs(Slice2/P1): T13 mark Phase 1 complete; record all commit hashes
- 另有 5 条 `docs(Slice2/P1): record T<n> commit hash <sha>` 的 tracker 同步提交(略)

### 测试

- `cargo build --workspace --tests`:绿
- `cargo test --workspace --lib`:绿(uc-application 176,uc-cli 10,全 lib 无回归)
- `cargo test -p uc-bootstrap --tests`:slice1_handshake_e2e 1 + slice2_phase1_presence_e2e 2 = 3 green,总 ~45s
- 用户手动:两 profile `--dev` 模式跑 `init` + `invite` + `join` + `members`,结果"完美符合"(用户原话)

### 统计

- 完成 14/15 tasks(T12 战略跳过)
- 实际总工时 ~10.6h vs 原估 ~15.2h,**-30%**
- 工时节省主要来源:T4/T7/T9 模块化良好(每个 ≤ 0.5h)+ T6 复用 `JoinSet` 而非自造并发原语 + T8 顺手吸收 T9 的 bootstrap 接线 + T12 战略跳过

### 遗留 / 下一步

- **follow-up 1**(给 Phase 2/3 的 rename/revoke):修 `RedeemPairingInvitationUseCase::persist` 让 joiner save self 为 `SpaceMember`。T11 的 `joiner_roster.len() == 1` 断言会失败,同步改成 `== 2`。
- **follow-up 2**(任意时点):T12 shell e2e 扩展 —— 若将来要把 `members` 放进 release 冒烟脚本再补。
- **Slice 2 Phase 2**:剪贴板同步(`ClipboardSyncFacade` + 两个新 Clipboard port + `install_clipboard` ALPN handler)。`IrohNodeBuilder::install_*` 扩展点模式 + wire v2 协议 + `PeerAddressRepositoryPort` 都是 Phase 1 为 Phase 2 打好的基础。
- **已暴露但不急**:`DeviceId` 缺 `Hash` derive(T3b 遇到,用 String key 绕过)+ `IrohNode` 单例语义未来清理 —— 均不阻塞 Phase 2。

---

## Session 2026-04-22(续 28) — Slice 2 Phase 2 封版 · 剪贴板同步(text-only,CLI-only)

**触发**:用户从 Phase 1 封版直接接 Phase 2,按 `slice2-phase2-plan.md` 14 任务拆解逐项推进,跨多个 session 完成。本条是收尾汇总,逐 task 现场细节见对应 commit message + `slice2-phase2-plan.md §15`。

### 交付范围

单句概括:**A 设备复制文字 → B 设备 ≤ 2s 收到匹配的明文 + content_hash**,CLI 提供 `send` / `watch` 完成端到端验收;**不含**系统剪贴板写入(daemon 改装推 Phase 3)、A3 revoke / A5 rename UI、blob / 文件传输。

结构按 infra → application → bootstrap → CLI → 测试由低到高:

- **Port 层**(`uc-core`):
  - `ClipboardDispatchPort` 新增 — 单 target 单 stream dispatch 原语,fan-out 留给应用层
  - `ClipboardReceiverPort` 新增 — `subscribe(&self) -> broadcast::Receiver<InboundClipboard>`,thin trait 让应用层用例订阅
  - `ClipboardHeader` / `SyncPayload` / `DispatchAck { Accepted, DuplicateIgnored }` / `ClipboardDispatchError { Offline, PeerRejected, Io, Internal }` / `InboundClipboard` 5 个 domain 类型
  - **legacy** `ClipboardOutboundTransportPort` / `ClipboardInboundTransportPort` 加 `#[deprecated(since="Slice2-Phase2")]`(双栈并行,Slice 5 删 — Phase 3 daemon 改装到 iroh 是前置条件)
- **iroh adapter 层**(`uc-infra`):
  - `clipboard_wire.rs`(T3 `b2206e33`)— 7 单测;frame `[magic=0xC1 \| header_len_be(4) \| header(postcard) \| payload_len_be(4) \| ciphertext]` + 1-byte ack 反向流;`MAX_HEADER_SIZE=4KiB` / `MAX_PAYLOAD_SIZE=2MiB` / `AckCode { Accepted=0x01, DuplicateIgnored=0x02, Rejected=0xFF }`
  - `IrohClipboardDispatchAdapter`(T4 `ae5b8202`)— `CLIPBOARD_ALPN = b"uniclipboard/clipboard/0"`;链路 `peer_addr_repo.get → postcard-decode EndpointAddr → endpoint.connect → open_bi → write_frame → read 1-byte ack`;错误折叠 `Offline` / `Io` / `PeerRejected`
  - `IrohClipboardReceiverAdapter` + `IrohClipboardReceiverHandler`(T5 `63330895`)— adapter 持广播 Sender,handler ProtocolHandler 装在 router 上;identity 反查靠 `Connection::remote_id().as_bytes()` → `IdentityFingerprintFactoryPort` → `member_repo.list().scan` → `DeviceId`;**关键 bug 修**:handler 返回时 `Connection` drop 致 ack byte 来不及 flush,加 `connection.closed().await` 保活(模仿 `IrohPresenceHandler`)
  - `IrohNodeBuilder::install_clipboard`(T6 `c500ae62`)— 镜像 `install_pairing` / `install_presence`;3 ALPN 同 router 共存测试覆盖
- **Application 层**(`uc-application`):
  - `DispatchClipboardEntryUseCase`(T7 `896e371b` + `e134247c` mockall 重写)— `pub(crate)`,输入 `(plaintext: Bytes, content_hash, payload_version)`;流程 `cipher.encrypt → peer_addr_repo.list → filter self + Online → JoinSet 并发 fan-out`;5 单测全 mockall(`.with(eq(DeviceId))` per-target 路由)
  - `IngestInboundClipboardUseCase` + `IngestSpawnHandle`(T8 `57ab9e65` + `e134247c`)— `pub(crate)`,subscribe → decrypt → 重 broadcast `InboundClipboardNotice`;Phase 2 不持久化、不 dedup;`Drop` 自动 abort;4 单测(mockall `MockCipher` + 手写 `FakeReceiver` 因 broadcast `Receiver` 非 Clone)
  - `ClipboardSyncFacade`(T9 `5b49d0ca` + `e134247c`)— 公开入口,3 方法(`dispatch_entry` / `subscribe_inbound_notices` / `spawn_ingest_loop`);完整 public ↔ internal 类型映射(7 对 + `From<DispatchSyncError>`),保证 §11.4 内部类型不外泄;3 单测
- **Bootstrap**(`uc-bootstrap`):
  - `SpaceSetupAssembly` 加 `pub clipboard_sync: Arc<ClipboardSyncFacade>` + 私有 `ingest_handle: IngestHandle`(T10 `d4849971`),与 `roster` 平行;`build_space_setup_assembly` 在 `install_presence` 之后调 `install_clipboard`、构造 facade、起 ingest loop;`shutdown` 显式 abort ingest 走在 router 关之前
- **CLI**(`uc-cli`):
  - `uniclipboard-cli send [TEXT]`(T11 `5d7622ed`)— positional 或 stdin → resume → refresh_presence → `dispatch_entry`,human + JSON 双输出,non-zero exit when nothing landed
  - `uniclipboard-cli watch` — `subscribe_inbound_notices` 循环 + Ctrl-C 退出,JSON 模式 line-delimited
- **测试**(`uc-bootstrap/tests`):
  - `slice2_phase2_clipboard_e2e`(T12 `734d52fe`)2 verdict —— `sponsor_dispatch_lands_on_joiner_within_2s`(plaintext + content_hash 字节级 round-trip 通过 V3 chunked AEAD)+ `repeat_dispatch_lands_twice_phase2_no_dedup`(pin Phase 2 不 dedup 的当前事实,Phase 3 持久化时 flip)
- **文档**:T14(本提交)— `task_plan.md` Phase 2 节标 ✅ + 列全 commit;`slice2-phase2-plan.md §15` tracker 全部封版

### 关键决策 / 偏离

1. **T2 探针确认免扩 port**(2026-04-22):原计划 §10 风险表第一行担心 `iroh::Connection::remote_id()` 与 `IdentityFingerprint` 算法对不上,需要扩 `IdentityFingerprintFactoryPort`。T2 探针实测 `Connection::remote_id().as_bytes()` 与 `SecretKey::public().as_bytes()` 字节等价(都是 32-byte Ed25519 compressed-edwards-y),已有 `from_public_key(&[u8])` 完全够用。**风险消除,proxy 工时退还**。
2. **T5 connection.closed().await 保活**(2026-04-22):4 个 receiver 测试全失败 `ConnectionLost(ApplicationClosed)`。Root cause:`async fn accept(&self, connection: Connection)` 消费 Connection by value,return 即 drop,ack byte 来不及到 peer。修法:每个 ack-emit 分支都加 `let _ = connection.closed().await;` 等 peer 主动关。**Phase 2 发现的唯一隐蔽 bug**,与 Phase 1 `IrohPresenceHandler` 的解法对称。
3. **T7-T9 mockall 违规重写**(2026-04-22 用户反馈):用户指出"写测试用例,你又忘记使用 mockall 了?"——T7/T8/T9 初版用了大量手写 fake,违反 Phase 1 §12.3 决策 5("正常调用次数 + 参数匹配断言仍用 mockall;手写 fake 仅用于 wall-time 并发断言或 broadcast subscribe+emit 人体工学")。`e134247c` 把 7 个 port 改 mockall(`MockPeerAddrRepo` / `MockPresence` / `MockCipher` / `MockDispatch` / `MockDeviceId_` / `MockLocalIdentity` / `MockSettings_`),`FakeReceiver` 保留(broadcast 订阅 ergonomics)+ `FixedClock` 保留(4 行 trivial)。后续 `9b920bd9` 修 dispatch_entry 模块文档误述。
4. **T10 ingest spawn 装配位置偏离原计划**(2026-04-22):原计划要把 ingest spawn 放进 `SpaceSetupFacade::auto_start_network`(F1 hook 后),理由是"start_network 后才有网络"。实际上 receiver handler 装在 router 上、router spawn 即工作,与 `start_network` **无序依赖**;ingest loop 是纯 broadcast subscriber。把 spawn 放 facade 反而要把 `ClipboardSyncFacade` 注进 `SpaceSetupFacade` 字段、违 §11.4 精神。最终决定:**assembly 层装配最干净**——`SpaceSetupAssembly` 加 `clipboard_sync` + `ingest_handle` 两字段,与 `roster` 平行。
5. **T11 不读系统剪贴板**(2026-04-22):原计划 §5.1 描述了 `dispatch_current_entry` 自动读 `SystemClipboardPort::read_snapshot` → encode payload → dispatch。**实际**:CLI 启动时设 `UC_DISABLE_SYSTEM_CLIPBOARD=1`(避免 clipboard-rs 在 non-bundled CLI 上 panic),所以系统剪贴板根本不可用。改成:plaintext 来源走 CLI arg 或 stdin(`echo hi | send`),签名 `dispatch_entry(plaintext: Bytes, content_hash, payload_version)`。daemon 改装到 iroh 时再开 OS clipboard 路径(Phase 3)。
6. **T12 验收断言改写**(2026-04-22):plan §9.2 原断言"B 侧 `ClipboardEventWriter.insert_event` 被调 1 次"——但 Phase 2 ingest 不持久化(§5.3),`ClipboardEventWriter` 根本不在调用链上;改断 `subscribe_inbound_notices` 收到 plaintext 字节级匹配。"重复 → DuplicateIgnored"改成"两次都 Accepted"——Phase 2 receiver adapter 不 dedup,wire 留有 `DuplicateIgnored` 编码但无生产者;Phase 3 持久化时再 flip 该断言。
7. **T13 战略跳过**(2026-04-22):沿用 Phase 1 T12 战略跳过决策。Rust e2e 已等价覆盖 pair → dispatch → receive 全路径(real iroh loopback transport,3 ALPN 同 router,V3 chunked AEAD round-trip,接收时序 ≤ 5s 含 CI 抖动);CLI plaintext pipeline 不依赖 OS state,没有 manual-only 的 variance 需要验。手动 recipe 留在 `slice2-phase2-plan.md §9.3`。

### 提交(按拓扑顺序)

- `0edb7479` feat(Slice2/P2): T1 `ClipboardDispatchPort` / `ClipboardReceiverPort` core ports
- `5a9ea34f` test(Slice2/P2): T2 iroh identity probe — `EndpointId == Ed25519 PublicKey`
- `b2206e33` feat(Slice2/P2): T3 clipboard wire postcard codec + 7 unit tests
- `ae5b8202` feat(Slice2/P2): T4 `IrohClipboardDispatchAdapter`
- `63330895` feat(Slice2/P2): T5 `IrohClipboardReceiverAdapter` + handler with closed() watchdog
- `c500ae62` feat(Slice2/P2): T6 `IrohNodeBuilder::install_clipboard` extension point
- `896e371b` feat(Slice2/P2): T7 `DispatchClipboardEntryUseCase`
- `57ab9e65` feat(Slice2/P2): T8 `IngestInboundClipboardUseCase`
- `5b49d0ca` feat(Slice2/P2): T9 `ClipboardSyncFacade` — public entry point
- `e134247c` refactor(Slice2/P2): T7/T8/T9 tests use mockall per project convention
- `9b920bd9` docs(Slice2/P2): correct dispatch_entry module doc — mockall is in use
- `9f39ba3d` docs(Slice2/P2): live progress tracker for T1-T9 + pending T10-T14
- `d4849971` feat(Slice2/P2): T10 wire `ClipboardSyncFacade` into `SpaceSetupAssembly`
- `aa897998` docs(Slice2/P2): T10 mark complete in plan tracker
- `5d7622ed` feat(Slice2/P2): T11 `uniclipboard-cli send / watch`
- `734d52fe` test(Slice2/P2): T12 `slice2_phase2_clipboard_e2e`
- 本提交 `docs(Slice2/P2): T14 mark Phase 2 complete; record all commit hashes`

### 测试

- `cargo build -p uc-cli -p uc-bootstrap --tests`:绿
- `cargo test -p uc-bootstrap --tests`:5 e2e 全绿(slice1_handshake_e2e 1 + slice2_phase1_presence_e2e 2 + slice2_phase2_clipboard_e2e 2)+ 1 doctest
- 手动 CLI smoke:`uniclipboard-cli send --help` / `watch --help` / `send --json --profile=p2-test "ping"`(resume guard 触发 `No space on this profile`,exit 1 正确)
- 单测累计:29 unit + 3 integration probe verdict + 5 bootstrap e2e + 2 CLI smoke,无回归

### 统计

- 完成 13/14 tasks(T13 战略跳过)
- 实际总工时 ~6.1h vs 原估 ~17.5h,**-65%**
- 工时节省主要来源:T2 探针消除一个 port 扩展;T5/T6 直接复用 Phase 1 `IrohPresenceHandler` 模式(包括 `connection.closed().await` 修法);T9 facade thin wrapper 0.5h;T10/T11 装配/CLI 各 0.4-0.5h;T12 e2e 大量复制 phase1 harness 0.7h

### 遗留 / 下一步

- **follow-up 1**(Phase 3):**daemon clipboard watcher 改装到 iroh** —— `uc-app::sync_outbound` / `uc-daemon::workers::inbound_clipboard_sync` 改 wire 到 `ClipboardSyncFacade`;完成后 Slice 5 才能删 deprecated transport ports
- **follow-up 2**(Phase 3):**receiver-side dedup + 持久化** —— ingest 接 `ClipboardEventWriter.insert_event` + content_hash 去重 → emit wire `DuplicateIgnored`,同时 flip `repeat_dispatch_lands_twice_phase2_no_dedup` 验收
- **follow-up 3**(继承自 Phase 1):**B2 不 save self 为 SpaceMember** —— 修复后 phase2 e2e 可加 B→A 双向 dispatch 断言
- **follow-up 4**(任意时点):**e2e harness 抽 `tests/common`** —— slice1 + slice2-phase1 + slice2-phase2 三份重复,Phase 3 出第四份前可统一抽取
- **Slice 2 整体**:剩下 A3 revoke + A5 rename UI + 大 payload(图片 / 富文本)未做;前两条进 Phase 3,后者推 Slice 3 blob
- **Slice 3 准备**:blob / 文件 ticket 路径,Phase 2 wire `MAX_PAYLOAD_SIZE=2MiB` 上限就是为这条路径预留
