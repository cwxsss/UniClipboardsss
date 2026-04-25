# Findings — iroh-native domain 重构

> 随探索过程更新。格式:事实 / 来源 / 影响。

## Slice 3 Phase 1 · T0 iroh-blobs 探针发现(2026-04-24)

### F-030 · `iroh-blobs 0.95.0` 与当前 `iroh 0.95.1` 不同栈
- **事实**:`uc-infra` 直接依赖 `iroh 0.95.1`,但 `iroh-blobs 0.95.0` 自身依赖 `iroh 0.93.2`。
- **来源**:`Cargo.lock` + `cargo tree -p uc-infra -i iroh@0.93.2` / `cargo tree -p uc-infra -i iroh@0.95.1`
- **影响**:不能把 `iroh-blobs 0.95.0` 的 `BlobsProtocol` 直接挂到当前共享 `iroh::protocol::Router` 上;`BlobTicket` 的地址类型也不是当前 `iroh 0.95` 的 `EndpointAddr`。这会破坏 Slice 1/2 已建立的“单进程一个共享 iroh endpoint”设计。

### F-031 · `iroh-blobs 0.97.0` 才是当前 iroh 0.95 路线的匹配版本
- **事实**:`iroh-blobs 0.97.0` 的 Cargo 元数据依赖 `iroh 0.95`,`BlobTicket` 使用 `EndpointAddr` / `EndpointId`,和当前工程的 iroh 命名一致。
- **来源**:`cargo info iroh-blobs@0.97.0` + 本机 registry 源码 `iroh-blobs-0.97.0/Cargo.toml` / `src/ticket.rs`
- **影响**:T0 探针应把依赖从 `iroh-blobs = "0.95"` 升到 `0.97.0`,再锁定真实 API。计划里所有“iroh-blobs 0.95 API 探针”的表述需要在 T0 结束时改成“iroh 0.95 对齐栈 / iroh-blobs 0.97 API 探针”。

### F-032 · downloader 只吃 provider id,fetch 需要先带入 ticket 地址
- **事实**:`store.downloader(&endpoint).download(ticket.hash_and_format(), [ticket.addr().id])` 只传 provider id。T0 loopback 中,即使 StaticProvider 已添加 provider `EndpointAddr`,直接 download 仍失败;先 `endpoint.connect(ticket.addr().clone(), iroh_blobs::ALPN).await` 后再 download 成功。
- **来源**:`uc-infra/tests/iroh_blobs_probe.rs::blobs_protocol_router_and_downloader_fetch_between_loopback_nodes`
- **影响**:`IrohBlobTransferAdapter::fetch` 不能只从 ticket 取 id 交给 downloader;必须先用 ticket 内完整 `EndpointAddr` 走一次 public connect 路径,让 iroh endpoint 获得地址/连接状态,再调用 downloader。这个预热连接会成为 T5 实现要求。

## 现状审计结论(已完成)

### F-001 · libp2p adapter 规模与位置
- **事实**:`uc-platform/src/adapters/libp2p_network/` 14 文件 / **~6594 行**
- **来源**:`wc -l` 扫描
- **影响**:
  - 规模足够大,意味着 iroh 实现也非小工程
  - 位置违反 `uc-platform/AGENTS.md`(外部能力实现应属 `uc-infra`)
  - 新 iroh 实现**必须**落在 `uc-infra`,不再犯同样的错

### F-002 · core 中 wire 泄漏清单
- **事实**:`uc-core/src/network/` 含有完整 wire 协议(9 条 pairing 消息、V3 clipboard codec、file_transfer binary codec、frame_to_bytes)
- **影响**:这些**全部不迁移**,新 domain 里不再出现 wire 概念。旧的随 libp2p 一起 Phase 8 删除。

### F-003 · 已有干净抽象样板
- **事实**:`uc-core/src/ports/clipboard/transport.rs` 已经实现了干净的 `SyncTargetId` + `OutboundClipboardFrame`(帧字节不透明)
- **影响**:证明"透明传输层"在本仓库可行。**但** D2 决策(改流式)意味着新 domain 不沿用"发送一帧"形状,而是 open/read/write/close。这份 port 随旧 network 一起废弃。

### F-004 · 已有 port 签名泄漏点
- **事实**:5 个 port 签名中出现 `peer_id: String` / `FileTransferMessage`(wire)/ `DiscoveredPeer.addresses: Vec<String>`
- **位置**:`pairing_transport.rs` / `file_transport.rs` / `network_events.rs` / `discovery.rs` / `connection_policy.rs`
- **影响**:这些 port 在 Phase 8 全部删除,新 domain port 从零设计

### F-005 · Identity 复用性
- **事实**:libp2p identity 使用 Ed25519,iroh NodeId = Ed25519 pubkey(32B)
- **影响**:D4 决策(用户重新配对)意味着**不复用**现有 identity_store,新 domain 独立生成 + 保存密钥

### F-020 · milestone/1.0.0 分支观察(只读)
**时间**:2026-04-18
**主题**:空间加密层重构(Slice 1 migration)— 与 iroh 工作**关注点不同,冲突面小**

| 变更 | 与 iroh 工作的关系 |
|---|---|
| 新 `SpaceAccessPort`(initialize/unlock/join-offer) | ✅ 我们 pairing 会用到它(调用方),不改它 |
| 新 `BlobCipherPort`(ActiveSpace-bound) | ✅ 无直接交集,payload 加密仍走它 |
| `identity_fingerprint.rs` 从 `uc-platform` → `uc-infra/src/security/` | ✅ 基于公钥,对 Ed25519 通用 → **iroh NodeId 展示层可复用** |
| `paired_device` → `space_member` / `trusted_peer`(表 + repo + 聚合) | 🎯 **C.2 方案直接借用**:`trusted_peer_row` 就是我们想要的承载体 |
| `identity_store.rs` 仍在 platform(libp2p Keypair 专用) | ⚠️ **不能复用** — iroh 独立 identity store |
| 搜索层大量删减 | 无关 |

**对 Q3 的结论**:
- iroh 独立密钥:`uc-infra/src/network/iroh/identity_store.rs`(新文件)
- 存储复用 `uc_core::ports::SecureStoragePort`,key = `"iroh-identity:v1"`
- 指纹展示层复用 `uc-infra/src/security/identity_fingerprint.rs`

**对 Phase 时序的影响**:
- Phase 1(C.2 扩展 `space/`)与 milestone/1.0.0 的 `space_access`/`trusted_peer` 模型高度重合
- **必须**先等 milestone/1.0.0 合并,或从它分支起飞,否则会撞车
- Phase 0(iroh 技术侦察)无依赖,可先走

## iroh 技术调研(Phase 0 产出,2026-04-18)

### F-010 · 版本选型
- **iroh**:`0.95.1`(当前稳定,用 `/websites/rs_iroh_0_95_1_iroh` 文档源)
- **iroh-blobs**:latest(对应 iroh 0.95.x),主体 `store::fs::FsStore` + `BlobsProtocol`
- **重要命名变化**(0.95 引入):原 `NodeId` / `NodeAddr` 在核心 crate 中改为 `EndpointId` / `EndpointAddr`;但 `iroh-blobs::BlobTicket` 内部字段保留 `node` / `node_id`(wire 兼容)
- **domain 应对**:domain 只用自己的 `NodeHandle`,不关心 iroh 的命名摇摆
- **Cargo feature**:`discovery-local-network` 才启用 mDNS;默认启用 DNS + Pkarr(n0 官方)

### F-011 · iroh 核心 API cheat sheet

```rust
// ===== 构造 Endpoint =====
let sk = SecretKey::from_bytes(&key_bytes_32);  // 或 generate(&mut OsRng)
let ep = Endpoint::builder()
    .secret_key(sk)
    .alpns(vec![ALPN_PAIRING.to_vec(), ALPN_CLIPBOARD.to_vec(), iroh_blobs::ALPN.to_vec()])
    .relay_mode(RelayMode::Default)        // 用 n0 官方 relay
    .discovery(PkarrPublisher::n0_dns())   // 发布
    .discovery(DnsDiscovery::n0_dns())     // 查找
    .discovery(MdnsDiscovery::builder())   // LAN 发现(feature-gated)
    .bind()
    .await?;

// ===== 入站:Router 按 ALPN 分发 =====
let blobs = BlobsProtocol::new(&blob_store, ep.clone(), None);
let router = Router::builder(ep.clone())
    .accept(ALPN_PAIRING,    pairing_handler)       // impl ProtocolHandler
    .accept(ALPN_CLIPBOARD,  clipboard_handler)
    .accept(iroh_blobs::ALPN, blobs.clone())
    .spawn();

// ===== 出站:开双向流 =====
let conn = ep.connect(endpoint_addr, ALPN_PAIRING).await?;
let (mut send, mut recv) = conn.open_bi().await?;
send.write_all(&payload).await?;
send.finish()?;
let resp = recv.read_to_end(MAX).await?;

// ===== SecretKey 持久化 =====
let bytes: [u8; 32] = sk.to_bytes();       // 存
let sk = SecretKey::from_bytes(&bytes);    // 取
```

### F-012 · iroh-blobs 核心 API cheat sheet

```rust
// store 创建(持久化 FsStore)
let store = store::fs::FsStore::load(path).await?;

// 发布方:加入 bytes,得到 TempTag(含 Hash = blake3 32B)
let tag = store.add_bytes(payload_bytes).await?;          // or add_slice
let ticket: BlobTicket = blobs.ticket(tag).await?;         // NodeAddr + Hash + Format

// 接收方:用 ticket 下载
let downloader = store.downloader(&endpoint);
let progress = downloader.download(ticket.hash_and_format(), ticket.node_addr().clone());
progress.await?;

// BlobTicket 序列化 = postcard 二进制(也有 derive_more::Display 文本形式)
let ticket_bytes = ticket.to_bytes();       // Ticket trait
let ticket = BlobTicket::from_bytes(&bytes)?;
```

### F-013 · iroh 概念 → domain 映射表(最终)

| iroh 类型 | 职责 | domain 对应 | 在哪一层出现 |
|---|---|---|---|
| `iroh::Endpoint` | 节点运行时 | — | 只在 `uc-infra/src/network/iroh/` 内,adapter 持有 |
| `iroh::EndpointId`(= `PublicKey`) | 节点标识(Ed25519 pubkey 32B) | **`NodeHandle`**(不透明值对象) | `uc-core/src/ports/` + `space/` 扩展 |
| `iroh::EndpointAddr` | 节点可达地址(relay url + direct addrs) | — | 仅 adapter 内,domain 不感知 |
| `iroh::SecretKey`(32B) | 节点私钥 | — | 独立 `identity_store.rs`,bytes 进 `SecureStoragePort` |
| `iroh::PublicKey` / `fmt_short()` | 对端指纹展示 | 复用 `IdentityFingerprint`(milestone/1.0.0) | adapter 层做映射 |
| ALPN bytes | 协议分流 | **`Capability`** 枚举(Pairing/Clipboard/Blob) | domain 语义 |
| bi-directional stream | 字节双向流 | **`SessionTransportPort`** 的 open/read/write/close | domain port |
| NodeAddr(direct_addresses + relay_url 快照) | 已配对 peer 的地址缓存 | **`PeerAddressCache`** 值对象 + `PeerAddressRepositoryPort` | domain 持久化端口 |
| `iroh_blobs::Hash`(blake3 32B) | 内容寻址摘要 | **`BlobDigest`** 值对象 | domain |
| `iroh_blobs::BlobTicket` | 分享凭证(NodeAddr+Hash+Format,postcard 编码) | **`BlobTicket`** 值对象(对 domain 是 opaque bytes + digest) | domain(作为消息字段) |
| `iroh_blobs::BlobFormat` | Raw / HashSeq | — | adapter 内(domain 暂不区分) |

### F-014 · ALPN 命名 & 版本策略(最终)

| Capability | ALPN bytes | 备注 |
|---|---|---|
| Pairing | `b"/uniclipboard/pairing/1"` | 本项目自定义,版本号从 1 起 |
| Clipboard | `b"/uniclipboard/clipboard/1"` | 流式剪贴板同步 |
| Blob | `iroh_blobs::ALPN` | **直接复用官方**,不自定义 |

**版本协商**:不在单条 ALPN 内做 sub-version;若协议破坏性变更 → 新增 `/uniclipboard/pairing/2`,Endpoint 同时 register 多版本。

### F-015 · Discovery 策略(修订版 2026-04-18)

**两层**(原 3 层 → 去 mDNS):
1. **`DnsDiscovery::n0_dns()` + `PkarrPublisher::n0_dns()`** — 默认公网发现
2. **OOB NodeTicket** — 仅配对时用(rendezvous 已承担)

**mDNS 移除**的理由:
- 新配对流程由 rendezvous + shortcode 驱动,不再需要 LAN 发现做配对入口
- iroh 的 direct path 发现**不依赖 mDNS**——依靠 NodeAddr 里的 `direct_addresses`(已发布到 n0 DNS)+ relay hole-punch
- 少一个 feature(`discovery-local-network`)、少一个 domain 变体(`ReachVia::Mdns`)

**副作用与对策**:
- **完全离线 LAN 场景**(无公网,n0 DNS 不可达,双方刚启动未互见)→ 无法冷启动发现彼此
- **对策**:客户端必须**持久化已配对 peer 的 last-known NodeAddr**(direct_addresses + relay_url 快照),启动时**优先尝试 last-known**,失败再落到 n0 DNS
- 这进入 F 组 F1 的范畴

**SyncSettings 允许覆盖**:
- 关闭 n0 DNS / Pkarr(纯 LAN 模式下用 last-known NodeAddr + relay 打通)
- 切换到自建 DNS(企业内网场景)

### F-016 · Relay 策略(最终)

- 默认 `RelayMode::Default`(n0 官方公共 relay)
- `SyncSettings` 允许:
  - `RelayMode::Custom(RelayMap)` — 自建 relay
  - `RelayMode::Disabled` — 仅直连(LAN-only 场景)

### F-017 · iroh-blobs store vs 现有 `uc-infra/blob` 分工(关键)

**两层加密独立,不冲突**:

```
应用层加密(现有 BlobCipherPort,MasterKey)
    ↓ 输出加密后的密文 bytes
iroh-blobs FsStore(BLAKE3 内容寻址,分享 ticket)
    ↓ 传输
iroh QUIC(链路加密,TLS 1.3)
```

**存储布局建议**:

| 目录 | 职责 | 管理者 |
|---|---|---|
| `app_data/blobs/encrypted/` | 现有 `uc-infra/blob`(应用密文最终落地) | 维持不变 |
| `app_data/blobs/iroh-store/` | `iroh-blobs::FsStore` 本地缓存(已传输的密文) | 新增 |
| `app_data/identity/iroh-identity-v1` | iroh `SecretKey` 32 字节(通过 `SecureStoragePort`) | 新增 |

**传输流程**(发送方):
1. 应用生成加密 payload(沿用 `BlobCipherPort`)
2. `FsStore.add_bytes(encrypted)` → `TempTag` + `Hash`
3. `blobs.ticket(tag)` → `BlobTicket`
4. 将 ticket(作为消息字段)通过 clipboard / pairing stream 发给对端

**传输流程**(接收方):
1. 从 stream 读到 ticket
2. `downloader.download(ticket.hash_and_format(), ticket.node_addr().clone())` → 拉到 FsStore
3. 从 FsStore 读出密文 → `BlobCipherPort` 解密 → 业务处理

**优点**:
- iroh-blobs 自动处理断点续传、去重(按 hash)
- 同一份 payload 发给多个对端 0 额外工作(都是 hash 寻址)
- 应用层加密与传输层加密解耦

### F-018 · 五个底层 port 签名草稿(Phase 1 蓝本)

目标:放在 `uc-core/src/ports/`,**签名中无 iroh 类型**,只出现 domain 值对象。

```rust
// ports/node_endpoint.rs  —— 节点运行时生命周期
#[async_trait]
pub trait NodeEndpointPort: Send + Sync {
    async fn start(&self) -> Result<NodeHandle, NodeEndpointError>;
    async fn stop(&self) -> Result<(), NodeEndpointError>;
    async fn local_handle(&self) -> Result<NodeHandle, NodeEndpointError>;
}

// ports/discovery.rs  —— 重做,不再吐 addresses
#[async_trait]
pub trait DiscoveryPort: Send + Sync {
    async fn subscribe(&self) -> Result<Box<dyn DiscoveryEventSource>, DiscoveryError>;
}
#[async_trait]
pub trait DiscoveryEventSource: Send {
    async fn recv(&mut self) -> Result<DiscoveryEvent, DiscoveryError>;
}
pub enum DiscoveryEvent {
    NodeAppeared { handle: NodeHandle, hints: UserDataHints },
    NodeDisappeared { handle: NodeHandle },
}

// ports/session_opener.rs  —— 按 Capability 开双向字节流
#[async_trait]
pub trait SessionOpenerPort: Send + Sync {
    async fn open(
        &self,
        target: &NodeHandle,
        capability: Capability,
    ) -> Result<Box<dyn Session>, SessionError>;
}
#[async_trait]
pub trait Session: Send {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, SessionError>;
    async fn write_all(&mut self, buf: &[u8]) -> Result<(), SessionError>;
    async fn finish(&mut self) -> Result<(), SessionError>;
    async fn close(self: Box<Self>, reason: Option<String>) -> Result<(), SessionError>;
}

// ports/blob_transfer.rs  —— iroh-blobs 能力
#[async_trait]
pub trait BlobTransferPort: Send + Sync {
    async fn publish(&self, bytes: Vec<u8>) -> Result<BlobDigest, BlobError>;
    async fn issue_ticket(&self, digest: &BlobDigest) -> Result<BlobTicket, BlobError>;
    async fn fetch(&self, ticket: &BlobTicket) -> Result<BlobBytes, BlobError>;
    async fn drop_local(&self, digest: &BlobDigest) -> Result<(), BlobError>;
}

// ports/presence.rs  —— 查询某节点当前可达性
#[async_trait]
pub trait PresencePort: Send + Sync {
    async fn snapshot(&self) -> Result<PresenceSnapshot, PresenceError>;
    async fn is_reachable(&self, handle: &NodeHandle) -> Result<bool, PresenceError>;
}
```

**域值对象**(同步草稿,最终位置 `space/` 扩展):
```rust
pub struct NodeHandle([u8; 32]);              // 不透明,内容 = Ed25519 pubkey,但 domain 不依赖这个事实
pub enum Capability { Pairing, ClipboardSync, Blob }
pub struct BlobDigest([u8; 32]);              // blake3
pub struct BlobTicket { digest: BlobDigest, opaque_addr: Vec<u8> }  // opaque_addr = postcard bytes
pub struct UserDataHints { /* 对端通过 UserData discovery 发布的提示,如 device_name 短名 */ }
```

### F-019 · 需要 Phase 1 敲定的细节(遗留)
- [ ] `NodeHandle` 对 domain 是否暴露 `as_bytes()` / `fingerprint()`?(倾向暴露 fingerprint,不暴露 pubkey bytes)
- [ ] `BlobTicket` 里要不要拆出 `node: NodeHandle`(便于路由 + UI 显示"从哪个设备拉的")
- [ ] `Session` 的 `write_all` / `read` 要不要支持 timeout(iroh stream 本身支持)
- [ ] `Capability` 枚举是否允许 `Custom(&'static str)`(放入扩展点)— 倾向不允许,protocol 扩展要加枚举变体

---

## F-030 · uc-rendezvous 自建服务契约(2026-04-18 只读调研)

**仓库**:`/Volumes/ExternalSSD/myprojects/uc-rendezvous`
**栈**:Cloudflare Workers + Durable Objects(TypeScript;不是 Rust)
**部署模式**:CF 边缘,HTTPS 由平台终结,**应用层无强制 TLS 检查**
**安全治理**:**无** auth / rate-limit / CORS(客户端是桌面 app,CF 边缘已拦公网滥用)
**会话实体**:每个 shortcode 绑定一个 `PairingSessionDO` Durable Object 实例

### F-030.1 · Shortcode 规格

- **字符表**:Crockford-like Base32 `ABCDEFGHJKLMNPQRSTUVWXYZ23456789`(去 `I/O/0/1`)
- **长度格式**:8 字符,显示为 `XXXX-XXXX`
- **熵**:≈ 40 bit
- **生成**:`crypto.getRandomValues`(`src/lib/codes.ts:1-10`)
- **唯一性**:通过 `idFromName(code)` 做 DO 级碰撞检测(非全局索引);过期/已消费的 code 可被新 create 覆盖
- **TTL**:客户端指定 `ttlSecs`,默认 **300 秒(5 分钟)**,alarm-driven 过期

### F-030.2 · 状态机

```
pending ──resolve──▶ resolved ──consume──▶ consumed
   │                    │                     │
   └─alarm──▶ expired ◀─┴──alarm──▶ expired   └─(terminal,record 保留)
```

- `resolved` 可**重复 resolve**(幂等,仍返回 ticket)直到 consume 或 expire
- `consumed` 是**终态**,后续任何 resolve/consume 返 `409`
- 过期后 record 留在存储里,但对公网不可见

### F-030.3 · 三个业务端点(+ 一个 infra 端点)

#### ① `POST /v1/pairings`
**职责**:sponsor(旧设备)登记自己的 iroh ticket,换取 shortcode。

**Request**(`application/json`):
```json
{
  "sponsorDeviceId":    "string, required",
  "sponsorDeviceName":  "string, required",
  "sponsorEndpointId":  "string, required",
  "sponsorTicket":      "string, required  // opaque iroh ticket",
  "ttlSecs":            "number, optional, default 300"
}
```

**Response 200**:
```json
{ "code": "ABCD-EFGH", "expiresAtMs": 1700000000000 }
```

**Errors**:
- `400 invalid_request` — 字段缺失
- `409 pairing_code_already_exists` — 极罕见碰撞

#### ② `POST /v1/pairings/resolve`
**职责**:新设备用 shortcode 换回 sponsor 的 ticket + 元数据。

**Request**:`{ "code": "ABCD-EFGH" }`

**Response 200**:
```json
{
  "code": "...",
  "status": "pending" | "resolved" | "consumed" | "expired",
  "sponsorDeviceId": "...",
  "sponsorDeviceName": "...",
  "sponsorEndpointId": "...",
  "sponsorTicket": "...",
  "expiresAtMs": 0
}
```
**副作用**:首次 resolve 触发 `pending → resolved`,记 `resolvedAtMs`。**幂等可多次**直到 consume/expire。

**Errors**:
- `400 invalid_request`
- `404 pairing_not_found`
- `404 pairing_expired`
- `409 pairing_already_consumed`

#### ③ `POST /v1/pairings/consume`
**职责**:终态化,标 shortcode 作废(由 sponsor 在配对成功后调用)。

**Request**:`{ "code": "ABCD-EFGH" }`
**Response 200**:`{ "ok": true }`
**Errors**:同 resolve。

#### ④ `GET /healthz`(infra 辅助端点)
`{"ok": true}` — 不在"3 个业务端点"之内,但 public。

### F-030.4 · 错误信封

统一形式:`{"error": {"code": "<machine-readable-slug>"}}`

机器可读错误码清单(我们的 adapter 应针对这些分支):
`invalid_request` / `pairing_code_already_exists` / `pairing_not_found` / `pairing_expired` / `pairing_already_consumed`

### F-030.5 · 关键设计含义(对我们的影响)

1. **rendezvous 仅会合**:只中转 iroh ticket,不转发 challenge/response —— 配对协议主体走 **iroh QUIC 直连**(sponsor 的 NodeTicket 本身就包含 NodeAddr + relay hints,新设备可以直接拨号)
2. **rendezvous 不必可信**:即便被攻破,对手能拿到 sponsor ticket 和 NodeId,**但无法伪装 sponsor**(它没 sponsor 的私钥);也**无法自行加入 Space**(没 passphrase)
3. **sponsor 主导**:流程由旧设备起手(创建 code),不是新设备先广播。新设备纯被动:输入 code → 拉 ticket → 直连
4. **8 字符 Base32 + 5 分钟 TTL** 已由服务器固化,客户端无改动空间(只能调 ttlSecs 上限)
5. **consume 由谁调用**:服务器侧代码无法强制,按业务惯例应**由 sponsor 在确认新设备通过 challenge 后调用**(用客户端视角看 sponsor 最"权威")
6. **DTO 字段 `sponsorEndpointId`**:直接对应 iroh 0.95 的 `EndpointId`(pubkey 字符串形式),说明服务端和客户端语汇已对齐 iroh 新命名
7. **无 CORS**:浏览器端直接调不通,符合 app 场景

---

## F-031 · milestone/1.0.0 上的配对实现复用性评估(2026-04-18)

**Verdict(TL;DR)**:`SpaceAccessPort` + PairingMessage 消息定义 + 状态机骨架 **已经 transport-agnostic**,iroh 只需实现新 `PairingTransportPort`。但**状态机是 PIN 显示模型**,与新 rendezvous+shortcode 流程语义不同,**编排层需调整**。

### F-031.1 · 可直接复用(无需修改)

| 组件 | 位置 | 说明 |
|---|---|---|
| `SpaceAccessPort::prepare_join_offer(space_id, passphrase)` | `uc-core/src/ports/space/access.rs` | Sponsor 侧:生成 `JoinOffer { keyslot_blob, challenge_nonce }`。Adapter 在 `uc-infra/src/security/space_access_adapter.rs:112-265` |
| `SpaceAccessPort::derive_master_key_for_proof(offer, passphrase)` | 同上 | Joiner 侧:用 passphrase 解 keyslot_blob → MasterKey(proof 构造用) |
| `PairingMessage` 9 条消息 | `uc-core/src/network/protocol/pairing.rs` | 特别是 `KeyslotOffer { keyslot_file, challenge }` 和 `ChallengeResponse { encrypted_challenge }` 的 payload 结构可以直接沿用 |
| `PairingTransportPort` trait 签名 | `uc-core/src/ports/pairing_transport.rs:1-25` | 已无 libp2p imports,iroh 只需新实现 |
| `PairingFacade` / `PairingOrchestrator` / `PairingProtocolHandler` | `uc-application/src/pairing/` | 编排层接受 port 注入,不绑 libp2p |

### F-031.2 · 状态机结构(`uc-application/src/pairing/state_machine.rs`)

**状态(非终态 7 + 终态 3)**:
```
Idle
  ├─ Initiator: RequestSent → AwaitingUserConfirm ─UserAccept→ ResponseSent → Finalizing → Paired
  └─ Responder: AwaitingUserApproval ─UserAccept→ ChallengeSent → Finalizing → Paired
终态: Paired | Failed | Cancelled
```

**事件**:`StartPairing` / `RecvRequest` / `RecvChallenge` / `RecvResponse` / `RecvConfirm` / `UserAccept` / `UserReject` / `UserCancel` / `Timeout` / …

**passphrase 入口**:**不走状态机**,由 daemon/UI 调 `SpaceAccessPort::prepare_join_offer(..., passphrase)` 直接拿 `JoinOffer`,再塞进 `PairingMessage::KeyslotOffer` 发出去。

### F-031.3 · 状态机与新流程的语义差异

现有状态机是 **PIN-显示防 MitM** 模型:
- Initiator 看 PIN(`AwaitingUserConfirm { short_code, peer_fingerprint }`)
- Responder 弹"同意/拒绝"(`AwaitingUserApproval`)
- 双向用户操作确认是同一个会话

新流程是 **shortcode-rendezvous-passphrase** 模型:
- Sponsor(=旧设备)先在 UI 生成 shortcode
- Joiner 输入 shortcode → 自动经 rendezvous 拉 ticket → 自动拨号
- 没有 PIN 显示,没有双向 UI 确认
- 防 MitM 由"新设备必须提供正确 passphrase"来兜底

**需要的状态机调整**:
- Sponsor 侧新增 `AwaitingShortcodeRedeem { shortcode }`(等 joiner 接入 iroh)
- Sponsor 侧的 `AwaitingUserApproval`(用户弹窗)**可保留也可去掉**——如果认为"新设备能提供正确 passphrase"就足够可信,则可去掉;UX 上建议保留(允许 sponsor 用户拒绝不认识的设备)
- Joiner 侧几乎不需要 UI 状态,可从 `Idle` 直接进 `ResponseSent`(发出 ChallengeResponse)
- `AwaitingUserConfirm`(PIN 比对)**整个去掉**

### F-031.4 · Gap 清单(iroh 集成必须新增)

| Gap | 备注 |
|---|---|
| **Rendezvous HTTP client**(调 F-030 三端点) | uc-infra,放 `rendezvous/` 目录 |
| **Shortcode 客户端状态管理**("同时只允许 1 个 pending") | 单实例 policy 在 uc-app 层 |
| **NodeTicket 生成 / 解析** | iroh-base 已提供,adapter 包装 |
| **iroh Endpoint 生命周期** | Phase 2 任务 |
| **iroh `PairingTransportPort` 实现** | 把 `PairingMessage` 通过 iroh bi-stream 送达 |
| **状态机扩展**(Sponsor 侧 `AwaitingShortcodeRedeem` + 去掉 PIN 分支) | 小改,保留现有代码骨架 |

### F-032 · BlobCipherPort 加密语义(2026-04-18 确认)

milestone/1.0.0 `uc-core/src/ports/security/blob_cipher.rs`:

- **随机 nonce AEAD**(非确定性加密)
- `encrypt(space, plaintext, aad) → Ciphertext`,Ciphertext 不透明,含 adapter 自描述的 nonce / tag
- 相同 plaintext + 相同 space + 不同调用 → **不同密文 → 不同 BLAKE3 hash**

**对 iroh-blobs 的影响**:
- ✅ **隐私优势**:外界无法通过密文 hash 比对推断"这两次传的是同一东西"
- ⚠️ **去重按密文工作**:同一文件多次 publish → 多个 digest → 多份密文
- ✅ **一对多 fanout 仍高效**:**单次** publish → 单一 digest → 共享同一 ticket 发给多个接收方,iroh-blobs 自动多路并发拉取

**设计含义**:
- 每个 ClipboardEntry 的每个文件 publish **一次**,得 **一个** digest,广播同一 ticket
- **应用层必须做去重**(见 F-033)—— 用户重复复制同一文件不应造成密文堆积

### F-033 · Blob 去重策略(2026-04-18 决议)

**问题**:随机 nonce AEAD → 每次加密生成不同密文 → iroh-blobs 天然无法按内容去重 → 用户重复复制同一文件 / 同一文件发给多个 Space 成员后又转发 → 密文重复堆积,浪费存储与带宽。

**方案比较**:

| 方案 | 原理 | 优点 | 缺点 |
|---|---|---|---|
| **A · 明文 hash 缓存**(选) | 维护 `plaintext_hash → digest` 映射,命中复用 digest | 不改 BlobCipherPort、不改加密算法、domain 纯 | 命中时损失"每次新 nonce"——但成员本就可信,无实质威胁 |
| B · 确定性加密 | BlobCipherPort 加 deterministic 模式 | 天然去重 | 改 port 撞 milestone/1.0.0 改动;deterministic AEAD 会暴露"重复"信号 |
| C · 文件路径+mtime 缓存 | 按 path+mtime 判同一文件 | 省一次读文件 | 修改文件不改 mtime 会误判;跨设备不生效 |

**选 A** 的实现:
- 新 domain port:**`BlobReferenceRepositoryPort`**
  - `find_by_plaintext_hash(hash: [u8; 32]) → Option<BlobDigest>`
  - `save(plaintext_hash, digest)`
  - `forget(plaintext_hash)`(GC 时清缓存)
- D1 usecase 内置去重流程(见 task_plan D1)
- 一对多 fanout 天然受益:同一 digest 共享同一 ticket,多接收方并发拉同一份密文
- **跨设备转发去重**:接收方 D2 完成后也记入本地 `BlobReferenceRepositoryPort`,后续若有第三方成员需要同文件,本机可直接以 sponsor 身份发 ticket(v1 可做可不做,端口形状不阻碍)

### F-031.5 · 复用推论

**推论 1**:iroh 的 `PairingTransportPort` 实现几乎就是一个 "按 PairingMessage enum 写入/读取 iroh bi-stream" 的薄壳。

**推论 2**:状态机**不重写**,只加一条 shortcode 路径(branch),旧 PIN 路径随 libp2p 一起 Phase 8 删除。

**推论 3**:rendezvous client 和 shortcode 管理是**新逻辑**,但不进 uc-core—— 纯 uc-infra / uc-app 编排,因为它是"外部系统契约" + "UI 状态"。

## Domain 划分决策 — 方案 C(2026-04-18 已定)

### 两层结构

```
uc-core/
├── ports/                      ← 底层纯能力(像 ClockPort 那样,无领域色彩)
│   ├── endpoint.rs             EndpointPort
│   ├── discovery.rs            DiscoveryPort(重做,不再吐 libp2p addresses)
│   ├── session_opener.rs       SessionOpenerPort(流式)
│   ├── blob_transfer.rs        BlobTransferPort(iroh-blobs)
│   └── presence.rs             PresencePort
│
├── [C.1] trust/                ← 或 [C.2] 并入 space/ 或 [C.3] peerage/
│   ├── trusted_peer.rs         聚合根
│   ├── peer_identity.rs        值对象
│   ├── policy.rs               TrustPolicy(替代旧 ConnectionPolicy)
│   ├── capability.rs           Capability 枚举
│   └── events.rs
│
└── (既有)pairing/ clipboard/ file_transfer/ space/ 各自扩展
```

### C.1 / C.2 / C.3 三个变体对比

| 维度 | C.1 新建 `trust/` | C.2 并入 `space/` | C.3 新建 `peerage/` |
|---|---|---|---|
| 新概念数量 | 多一个子域 | 少 | 多一个子域 |
| 与 `space::SpaceMember` 关系 | 并列,需定义映射 | 合一 | 并列,需定义映射 |
| 词义清晰度 | `trust` 偏安全策略,歧义 | 借用现有概念 | `peerage` 贴"对端群体" |
| 对旧代码改动面 | 中 | 小 | 中 |

**F 团队倾向**:C.2(最少新概念,复用既有 `space` 业务模型;`TrustedPeer` 本质就是"某 Space 的 Member 从连接视角看的一面")

## 剩余 Q2–Q5 建议默认

见 `task_plan.md`。

## 待用户决策(Q1–Q5 汇总) — ⚠ 已全部敲定

Q1-Q5 已敲定(`task_plan.md` ✅ 已敲定决策章节)。
Q-α~ε / Q-1~3 / 命名设计决策于 2026-04-19 outside-in discussion 完成,见 `task_plan.md` 同名章节。

---

## 现状勘探(2026-04-19, Slice 1 outside-in discussion 期间产出)

### F-031 · `SpaceMember` 当前结构
- **事实**:`uc-core/src/membership/member.rs`
  ```rust
  pub struct SpaceMember {
      pub device_id: DeviceId,
      pub device_name: String,
      pub identity_fingerprint: String,    // ← 公钥指纹(Base32),但是 String,未值对象化
      pub joined_at: DateTime<Utc>,
      pub sync_preferences: MemberSyncPreferences,
  }
  ```
- **来源**:Read 文件
- **影响**:
  - identity 已经在 SpaceMember 上了,无需新建 NodeIdentity 域对象(Q-γ)
  - `identity_fingerprint: String` 跟 `TrustedPeer.peer_fingerprint: PeerFingerprint`(已是值对象)不一致 → 触发 Slice 0.5

### F-032 · `TrustedPeer` 当前结构
- **事实**:`uc-core/src/trusted_peer/peer.rs`
  ```rust
  pub struct TrustedPeer {
      pub local_device_id: DeviceId,
      pub peer_device_id: DeviceId,
      pub peer_fingerprint: PeerFingerprint,    // ← 已是值对象
      pub trusted_at: DateTime<Utc>,
  }
  ```
- **`PeerFingerprint`**(`trusted_peer/fingerprint.rs`)注释明示"用于 re-verify 'still the same peer' on reconnect"——纯身份验证,与寻址无关
- **影响**:
  - `local_device_id` 字段已隐含"本机"概念,无需新增(Q-3)
  - `PeerFingerprint` 这个名字本质就是"公钥指纹的对端视角",名字冗余(指纹本身没有视角)

### F-033 · `DeviceIdentityPort` 已存在 + `LocalDeviceIdentity` 落地
- **事实**:
  - core:`uc-core/src/ports/device_identity.rs` 提供 `DeviceIdentityPort.current_device_id() -> DeviceId`
  - infra:`uc-infra/src/device/mod.rs` `LocalDeviceIdentity::load_or_create(config_dir)`(UUID v4,plain text 文件落盘)
  - 在 `crates/uc-bootstrap/src/assembly.rs` / `crates/uc-bootstrap/src/builders.rs` 中装配
- **影响**:
  - **A1 / B2 落本机 SpaceMember 无需新 port** — 复用 `DeviceIdentityPort`
  - DeviceId(UUID 业务标识)与 iroh secret/NodeId(网络层身份)是**两个完全独立的维度**

### F-034 · `IdentityFingerprint` 三处类型分裂
- **事实**:同一概念(Ed25519 公钥的 SHA-256 截断 Base32)在三处用三种类型表达:
  | 位置 | 类型 |
  |---|---|
  | `SpaceMember.identity_fingerprint` | `String` |
  | `TrustedPeer.peer_fingerprint` | `PeerFingerprint(String)` |
  | `uc-infra/src/security/identity_fingerprint.rs` | `IdentityFingerprint(String)` 值对象 + `verify()` |
- **算法**(infra 已实现):`SHA-256("uc-identity-fp-v1" || pub_key_bytes)[0..10] -> Base32 -> "ABCD-EFGH-IJKL-MNOP"`
- **影响**:Slice 0.5 把这三处统一到 `IdentityFingerprint`(上提到 core)

### F-035 · `PairingTransportPort` 当前签名 — ✅ 确认结果(2026-04-19)
- **位置**:`uc-core/src/ports/pairing_transport.rs`
- **完整签名**:
  ```rust
  #[async_trait]
  pub trait PairingTransportPort: Send + Sync {
      async fn open_pairing_session(&self, peer_id: String, session_id: String) -> Result<()>;
      async fn send_pairing_on_session(&self, message: PairingMessage) -> Result<()>;
      async fn close_pairing_session(&self, session_id: String, reason: Option<String>) -> Result<()>;
      async fn unpair_device(&self, peer_id: String) -> Result<()>;
  }
  ```
- **`peer_id` 类型 = `String`**(libp2p PeerId 字面量):语义泄漏
- **无 `dial_by_invitation` 方法**:Slice 1 B2 需要的"按 invitation code 直接拨号"能力不存在
- **入站对端标识** 不在 `PairingMessage` 里,走 wire 层 `uc-core/src/network/protocol/pairing.rs::PairingRequest.peer_id: String`(网络层字段,Slice 5 删)
- **被 `PairingOrchestrator / PairingProtocolHandler` 间接驱动**,facade 不直接暴露
- **Slice 1 决策**(N-2):不扩展此 port;**新建独立 Slice 1 pairing port**;旧 port 打 `#[deprecated]`,与 libp2p adapter 一起 Slice 5 删

### F-036 · 概念三分(确立的核心架构原则)
| 概念 | 类型 | 用途 | 出现位置 |
|---|---|---|---|
| `DeviceId` | UUID v4 | **业务标识**(主键 / 引用) | core 业务层 ID |
| `IdentityFingerprint` | Ed25519 pubkey SHA-256 截断 Base32 | **身份验证**("是不是同一台设备") | core SpaceMember/TrustedPeer 字段 |
| iroh `NodeAddr`(infra 内部) | relay url + direct addrs | **网络寻址**("怎么连到这台设备") | infra 内部不上浮 |

**调用流程**(业务无感):
```
业务层:  clipboard_dispatch.send(target: DeviceId, payload)
                                   ↓
infra:  查 SpaceMember(target_device_id) → 拿 identity_fingerprint
       iroh discovery (mDNS/n0 DNS) → 拿 NodeAddr
       iroh 拨号 → TLS 握手拿到对方公钥
       验证 SHA-256(对方公钥) == 存的 identity_fingerprint
       验证通过 → 发 payload
```
- 业务层永远不直接面对 fingerprint,也不面对 NodeAddr
- 业务层 port 签名都是 `DeviceId`
- 这条原则下,大量原计划 port(暴露 NodeHandle / NodeTicket / EndpointTicket 等)被取消

### F-037 · setup 流程与本机 SpaceMember 持久化的缺口
- **事实**:milestone 现有 `SubmitNewSpacePassphraseUseCase` 和 `SetupOrchestrator::submit_passphrase`,只调用 `SpaceAccessPort::initialize` 创建加密 vault,**不创建本机 SpaceMember 记录**
- **影响**:Slice 1 A1 必须扩展该流程,在 `SpaceAccess.initialize` 之后追加:
  1. `LocalIdentityPort::create()` 生成 iroh secret + 派生指纹
  2. 用指纹 + DeviceId + device_name 构造本机 SpaceMember
  3. `MemberRepositoryPort::save` 持久化
- **breaking change**:`SubmitNewSpacePassphraseCommand` 加 `device_name: Option<DeviceName>` 字段

### F-038 · `SpaceAccessFacade` / `SpaceAccessPort` 现状 — ✅ 确认结果(2026-04-19)
- **位置**:`uc-application/src/space_access/facade.rs` / `uc-core/src/ports/space/access.rs`
- **`SpaceAccessFacade` 现有方法**(**没有** `unlock` / `initialize` / `is_unlocked`):
  - `get_state` / `reset` / `set_sponsor_peer_id` / `initiate_joiner_flow(joiner_offer, passphrase, sponsor_peer_id)` / `peek_joiner_offer` / `peek_prepared_offer` / `set_peer_identity` / `start_sponsor_authorization` / `dispatch` / `context_handle` / `subscribe`
  - Facade 只对外"加入流程 / 订阅"层面,不暴露加解锁原语
- **`SpaceAccessPort` 完整方法**(core 层,11 个):
  - `initialize(space_id, passphrase) -> ActiveSpace`
  - `unlock(space_id, passphrase) -> ActiveSpace`
  - `is_unlocked(space_id) -> bool`
  - `lock(space_id)` / `factory_reset(space_id)` / `try_resume_session(space_id)` / `verify_keychain_access` / `derive_subkey(salt, info)` / `current_session_proof_key` / `prepare_join_offer(space_id, passphrase)` / `derive_master_key_for_proof(offer, passphrase)`
- **Slice 1 影响**:
  - **A2 UnlockSpaceUseCase 必须新建**,直接调 `SpaceAccessPort::unlock`,不能套壳 facade
  - **A1 InitializeSpaceUseCase** 同理 — facade 无 initialize 入口;但 milestone 已有 `SubmitNewSpacePassphraseUseCase`(via `SetupFacade::submit_passphrase`)做这件事(参见 F-037),A1 = 扩展它 + 追加 identity/member 步骤
  - `BootstrapOnStartupUseCase` 的 `is_unlocked()` 查询直接走 `SpaceAccessPort::is_unlocked`(core 有),**不**走 facade

### F-039 · `LocalIdentityPort` / `LocalDeviceNamePort` / `PairingInvitationPort` port 草图
- **事实**:见 `task_plan.md` 新章节"✅ 已敲定决策(2026-04-19)"中的命名/对称设计决策
- **完整签名草图**(B2 后定稿:LocalIdentityPort 加 `ensure()`):
  ```rust
  // uc-core/src/ports/local_identity.rs
  #[async_trait]
  pub trait LocalIdentityPort: Send + Sync {
      /// A1 严格 create:已存在则 AlreadyExists
      async fn create(&self) -> Result<IdentityFingerprint, LocalIdentityError>;
      /// B2 幂等 ensure:存在则返回,不存在则 create(joiner 重试友好)
      async fn ensure(&self) -> Result<IdentityFingerprint, LocalIdentityError>;
      /// 任何时候:取已存在的指纹,不存在则 NotInitialized
      async fn current_fingerprint(&self) -> Result<IdentityFingerprint, LocalIdentityError>;
  }

  // uc-core/src/ports/local_device_name.rs
  pub trait LocalDeviceNamePort: Send + Sync {
      fn current(&self) -> DeviceName;   // 总能返回(系统 hostname)
  }

  // uc-core/src/ports/pairing_invitation.rs (B1 草图后定稿:只 1 个方法)
  #[async_trait]
  pub trait PairingInvitationPort: Send + Sync {
      /// TTL 由 server authoritative 决定;返回 IssuedInvitation { code, expires_at }
      async fn issue_invitation(&self) -> Result<IssuedInvitation, InvitationError>;
  }
  pub struct IssuedInvitation {
      pub code: InvitationCode,
      pub expires_at: DateTime<Utc>,
  }
  pub enum InvitationError { NetworkNotStarted, ServiceUnavailable, Internal(String) }
  // 注:revoke_invitation 不需要 — server 不支持主动 revoke;
  //   旧 code 靠 5min 自然过期 + sponsor 侧入站时 code 匹配检查保安全
  // 注:joiner 侧 redeem+dial 合并到 PairingTransportPort::dial_by_invitation,这里不出现
  ```

### F-040 · `PairingInvitation` 域对象草图
```rust
// uc-core/src/pairing/invitation/
pub struct PairingInvitation {
    pub code: InvitationCode,
    pub issuer_device_id: DeviceId,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub state: InvitationState,
}

pub struct InvitationCode(String);  // 业务语义"用户可输入的串",不限格式

pub enum InvitationState {
    Pending,    // 已发出,等待 joiner 消费
    Consumed,   // 已被消费(配对完成)
    Revoked,    // sponsor 主动取消
    Expired,    // TTL 到期(懒判断)
}

pub enum InvitationEvent {
    Issued    { code, expires_at },
    Consumed  { code, by_device_id },
    Revoked   { code },
    Expired   { code },
}

impl PairingInvitation {
    pub fn is_expired(&self, now: DateTime<Utc>) -> bool;
    pub fn consume(&mut self, by: DeviceId, now: DateTime<Utc>) -> Result<InvitationEvent, ConsumeError>;
    pub fn revoke(&mut self) -> Result<InvitationEvent, RevokeError>;
}
```
- **不持久化**(Q-2):in-memory,application 编排层维护(`Arc<RwLock<Option<PairingInvitation>>>`),进程崩溃就丢
- **单例约束在 application**(Q-δ + B1 修订):`issue_pairing_invitation` 进入时若已有 pending 则**本地清空旧的(不调 server)+ 发 InvitationEvent::Revoked + 创建新**;不返回错误

### F-041 · 入站事件需带 incoming code(B2 后定稿:走 PairingRequest 字段)
- **事实**:sponsor 侧需要在入站连接事件中得知"对端用了哪个 invitation code",才能跟本机 `Arc<RwLock<Option<PairingInvitation>>>` 中的 code 比对
- **B2 定稿**:code **不放 stream metadata,放在 `PairingRequest` 协议消息字段里**
  - 理由:ALPN 不支持参数,自定义 stream header 复杂;`PairingRequest` 是协议第一个消息,扩展加字段最自然
- **影响**:
  - `PairingTransportPort` 入站事件接口**不需扩展**(原计划要扩,定稿后不扩)
  - wire 层 `PairingRequest` 加 `invitation_code: String` 字段(infra 改动,core 不见)
  - sponsor 侧 application 层在收 `PairingRequest` 后匹配:`in_memory.code == request.invitation_code`?
    匹配 → 进入 RecvRequest;不匹配 → 发 PairingReject + 关流
- **安全意义**:旧 code 即使被攻击者用,sponsor 应用层匹配时拒绝 → 不依赖 server revoke 也安全

### F-042 · AppFacade 集中化(B2 决策 Q-B2-2 引入)
- **事实**:milestone 当前各业务子域 facade 平铺(`PairingFacade` / `SetupFacade` / `SpaceAccessFacade`),外部调方(Tauri / daemon / CLI)需注入多个 sub-facade
- **B2 决策**:新增 `AppFacade` 作为统一对外入口
  - 位置:`uc-application/src/facade/app_facade.rs`(新目录)
  - 职责:跨域动作的编排 + 对外暴露稳定接口
  - sub-facade 仍存在(内部协调),AppFacade 持有它们
- **Slice 1 内的工作**:
  - 新建 AppFacade,实现 8 个方法:`initialize_space`(A1) / `unlock_space`(A2) / `issue_pairing_invitation`(B1) / `redeem_pairing_invitation`(B2) / `on_startup`(F1 Bootstrap 入口) / `on_shutdown`(F2 入口)—— A1/A2 成功路径内部串 `StartNetworkUseCase`
  - sub-facade 保持 `pub`(本 slice 不破坏旧接入)
  - Tauri/daemon/CLI 切到 AppFacade 推到 Slice 1.5 或后续 slice
- **与 §11.4 的关系**:不破坏(外部 crate 仍只见 facade,只是多了 AppFacade 一层);跨业务模块的 UseCase 保持 `pub(crate)` 但 AppFacade 在同 crate 可调
- **后续可能的演化**:
  - 单一 AppFacade 太大时,按业务流程拆(`OnboardingFacade` / `ClipboardFacade` / `MemberFacade`)
  - sub-facade 后续 slice 可考虑降级 `pub(crate)`(待 Tauri/daemon/CLI 全切完后)

### F-043 · `CompleteJoinSpaceUseCase`(milestone)— ✅ 确认结果(2026-04-19)
- **位置**:`uc-application/src/setup/usecases/complete_join_space.rs`
- **可见性**:`pub(crate)`;通过 `SetupFacade::complete_join_space()`(pub)对外暴露
- **完整接口**:
  ```rust
  pub(crate) struct CompleteJoinSpaceUseCase { orchestrator: Arc<SetupOrchestrator> }
  impl CompleteJoinSpaceUseCase {
      pub(crate) async fn execute(&self) -> Result<SetupState, SetupError> {
          self.orchestrator.complete_join_space().await
      }
  }
  // 内部:self.dispatch(SetupEvent::JoinSpaceSucceeded).await
  ```
- **输入字段**:无(trigger-only,不接 Command)
- **不需要**:master_key / passphrase / sponsor_identity / space_id — 全部在前序步骤(`StartJoinSpaceAccess`)内部已消费 / 由 `SpaceAccessFacade` 上下文管理
- **内部链路**:`dispatch(JoinSpaceSucceeded)` → 状态机转 `Completed` → 动作 `MarkSetupComplete` → `app_lifecycle.ensure_ready()` + `mark_setup_complete.execute()`
- **本机 SpaceMember 持久化** ✅ **已内置**:`SpaceAccessOrchestrator.dispatch()` 到 `Granted` 时自动 `try_admit_peer_as_member()`(若注入了 `AdmitMemberUseCase`)。**B2 步骤 10b 可省**
- **原子性**:非严格。admit 失败仅 WARN 不阻断;`Granted` 状态已落盘(`PersistJoinerAccess` / `PersistSponsorAccess`)
- **Slice 1 B2 编排**:AppFacade 跨模块调必须经 `SetupFacade::complete_join_space()` 代理(pub(crate) 不可直调)。符合 §11.4 封装规则

### F-044 · `NetworkControlPort` — ✅ 确认:**已存在**,Slice 1 扩展(2026-04-19)
- **现状**:`uc-core/src/ports/network_control.rs` **已存在**(milestone 时代建),签名极简:
  ```rust
  #[async_trait]
  pub trait NetworkControlPort: Send + Sync {
      async fn start_network(&self) -> Result<()>;  // 仅此 1 方法
  }
  ```
- **邻居**:`uc-core/src/ports/network_events.rs::NetworkEventPort`(订阅网络事件)— 与 Slice 1 无直接关系
- **工作区约定**:port 直接平铺在 `uc-core/src/ports/` 下(单文件 port),不建 `ports/network/` 子目录(与 clipboard/security/space 等多 port 域的子目录惯例有别)
- **Slice 1 决策**(N-1):**扩展现 port,加 `stop_network` 默认 no-op impl**
  ```rust
  #[async_trait]
  pub trait NetworkControlPort: Send + Sync {
      async fn start_network(&self) -> Result<()>;
      /// 默认 no-op,供 libp2p adapter 冻结行为;iroh adapter override
      async fn stop_network(&self) {}
  }
  ```
- **效果**:libp2p adapter **零改动**(trait 默认 impl 兜底 `stop_network`),iroh adapter override 为真实 close。符合 D1 "libp2p 行为不变"
- **Slice 1 真新 port 影响**:`NetworkControlPort` 从"新建"降级为"扩展" → 真新 port 累计 -1
- **签名保留 `Result<()>`**(原 anyhow 风格,不自定义 `StartNetworkError` enum):否则改返回类型会破 libp2p adapter 编译。核心错误信息在 iroh adapter 内部 log + 粗粒度 Err 返回给 UseCase;UseCase 侧的 `NotUnlocked` / `LocalIdentityMissing` 断言失败直接由 UseCase 构造(不走 port)
- **adapter 依赖**:iroh 实现内部持有 `Arc<dyn LocalIdentityPort>` + 其自己的 secret store(SecureStoragePort 或专用 store),`start_network` 时自行拿 secret key;core 下令仅 "start",不传 identity(否则 UseCase 要知道密钥字节,破封装)
- **为什么 core 不传 identity 参数**:core 层只面对 `IdentityFingerprint`(展示用),secret key 是 adapter 持久化细节;adapter 组合 `LocalIdentityPort`(拿 fingerprint 判在不在)+ 其自己的 secret store(拿字节给 iroh)
- **Slice 2 扩展预期**:
  - `stop_network` 增加 drain 参数(timeout)供 C1/C2/D1/D2 in-flight 完成
  - 新增 `ensure_reachable(member)` 等预连方法(见旧 Port 总表 F 组汇总 L1728-1730,Slice 2 outside-in 反推后确认)
- **与旧 F 组草图的关系**:旧草图用 `NodeIdentityStorePort::load_or_generate()` + `NetworkControlPort` 分离,outside-in 后 `NodeIdentityStorePort` 被 `LocalIdentityPort`(F-039)替代 + secret store 留在 adapter 内;`NetworkControlPort` 是已有 port 的扩展,非新建(2026-04-19 Read 后修正)

### F-045 · Slice 1 实施方案决策(N 系列,2026-04-19)

由 F-035/F-038/F-043/F-044 的 Read 结果引出 3 个新决策:

| # | 议题 | 决策 | 理由 |
|---|---|---|---|
| **N-1** | `NetworkControlPort` 扩展还是新建 | **扩展 + `stop_network` 默认 no-op impl** | libp2p adapter 零改动(trait 默认体兜底),iroh override 为真实 close;符合 D1 "libp2p 行为不变" |
| **N-2** | `PairingTransportPort` 扩展还是新建 | **新建独立 Slice 1 pairing port;旧 port 打 `#[deprecated]`** | 旧 port `peer_id: String` 带 libp2p 语义泄漏,违反 "从零遵循六边形";deprecated 标记帮编译期提示 Slice 5 清理 |
| **N-3** | Rendezvous 客户端代码落点 | **`uc-infra/src/rendezvous/client.rs`**(新 module,非新 crate) | 语义独立(后续 Slice 1.x 换服务易),不污染 `uc-infra/src/network/iroh/` 目录 |

**配套基础设施决策**(用户 2026-04-19 锁定):

| # | 议题 | 决策 | 影响 |
|---|---|---|---|
| I-1 | 编码切入顺序 | **Slice 0.5(IdentityFingerprint 统一)→ Slice 1** | 独立 PR review 友好;A1 起步即用统一类型 |
| I-2 | 工作分支 | **继续 `slender-soybean`** | 不切新分支 |
| I-3 | Cargo 依赖引入 | **一次性加齐,无 `#[cfg(feature = "iroh")]` 门控** | iroh 代码从 day-1 即 buildable;双栈代码并存编译。关联:N-1 的默认 impl + N-2 的独立 port 保证 libp2p 栈零侵入 |
| I-4 | Rendezvous 服务 | **用现有 `https://rendezvous.uniclipboard.app`**,不建 `uc-rendezvous` crate | 客户端走 HTTP 调现有服务;server 端维护与本仓库脱耦 |

### F-046 · `LocalIdentityPort` / `LocalDeviceNamePort` 现状补查(2026-04-19)

**`LocalIdentityPort`** — ✅ 必须**新建**

- **现有 `DeviceIdentityPort`**(`uc-core/src/ports/device_identity.rs`)只管 `DeviceId`(UUID):
  ```rust
  pub trait DeviceIdentityPort: Send + Sync {
      fn current_device_id(&self) -> DeviceId;
  }
  ```
  被 `capture_clipboard` / `sync_inbound` / `sync_outbound` 等 UseCase 使用
- **现有 `IdentityFingerprintFactoryPort`**(`uc-core/src/ports/security/identity_fingerprint.rs`)是纯算法工厂:
  ```rust
  pub trait IdentityFingerprintFactoryPort {
      fn from_public_key(&self, public_key: &[u8]) -> Result<String>;
  }
  ```
  返回 `String`(未用值对象)
- **`IdentityFingerprint` 值对象** 仍在 `uc-infra/src/security/identity_fingerprint.rs`(Slice 0.5 要上提 core)
- **`LocalDeviceIdentity` 值对象** 在 `uc-infra/src/device/mod.rs`(实现 `DeviceIdentityPort`,只管 device_id)
- **结论**:`LocalIdentityPort` 职责与 `DeviceIdentityPort`/`IdentityFingerprintFactoryPort` 均不同(前者管"iroh Ed25519 秘钥对生命周期 + 指纹暴露",后两者分别是"UUID device_id" / "纯指纹算法"),**必须新建**
- **Slice 0.5 工作量精简**:`IdentityFingerprintFactoryPort` 已在 core,只需把 `IdentityFingerprint` 值对象上提 + `SpaceMember.identity_fingerprint: String` → `IdentityFingerprint`

**`LocalDeviceNamePort`** — ❌ **取消,改复用 `SettingsPort`**

- **现有 `SettingsPort`**(`uc-core/src/ports/settings.rs`):
  ```rust
  pub trait SettingsPort: Send + Sync {
      async fn load(&self) -> anyhow::Result<Settings>;
      async fn save(&self, settings: &Settings) -> anyhow::Result<()>;
  }
  ```
- **`Settings.general.device_name: Option<String>`** 已落在 `GeneralSettings` 结构里(`uc-core/src/settings/model.rs`)
- 持久化:`FileSettingsRepository`(JSON 文件)
- **注意**:milestone `SetupFacade::submit_passphrase(passphrase, confirm)` **不接 `device_name` 参数**——`device_name` 通过 `SettingsPort.save` 间接持久化,与 setup 流程解耦
- **结论**:新建 `LocalDeviceNamePort` 会造成"同一业务事实的两个真相源",**改复用 `SettingsPort`**
- **Slice 1 A1 改动**:第 1.5 步从 `LocalDeviceNamePort::current()` 改为 `SettingsPort::load` 读 `general.device_name`,若未设置则兜底用系统 hostname(adapter 侧 fallback,非 port 职责)或 UI 必填
- **B2 改动**:若允许 joiner 传 `device_name`,走 `SettingsPort::save` 更新 `general.device_name`

**Slice 1 真新 port 最终清单**(3 个):

| Port | 位置 | 职责 |
|---|---|---|
| `LocalIdentityPort` | `uc-core/src/ports/local_identity.rs` | iroh Ed25519 秘钥对 lifecycle + fingerprint 暴露 |
| `PairingInvitationPort` | `uc-core/src/ports/pairing_invitation.rs` | sponsor 签发配对 invitation code(rendezvous adapter 实现) |
| Slice 1 新 pairing port(名字待定) | `uc-core/src/ports/pairing/session.rs`(建议) | sponsor accept 入站 + joiner `dial_by_invitation` + session 消息收发。旧 `PairingTransportPort` 打 `#[deprecated]`,Slice 5 删 |

**Slice 1 port 扩展清单**(1 个):

| Port | 扩展内容 |
|---|---|
| `NetworkControlPort` | 加 `async fn stop_network(&self) {}` 默认 no-op |

**Slice 1 复用清单**(零改):

- `SettingsPort`(读写 `general.device_name`)
- `DeviceIdentityPort`(`current_device_id()`)
- `SpaceAccessPort`(A1 `initialize` / A2 `unlock` / Bootstrap `is_unlocked`)
- `IdentityFingerprintFactoryPort`(纯算法工厂)
- `SecureStoragePort`(iroh adapter 内部持久化 secret key)
- `MemberRepositoryPort`(owner/joiner 本机 SpaceMember 持久化)
- `SetupStatusPort`(A1 完成标记)
- `SetupFacade::complete_join_space()`(B2 经此代理调 `CompleteJoinSpaceUseCase`)

### F-047 · Sub-Facade + Deps 模式(P4 重构定稿,2026-04-19)
- **背景**:P4 首版 `AppFacade::new(7 个 Arc<dyn Port>...)` 把跨域 wiring 塞进一个构造器,用户审视后驳回:"应按 domain 分类"。重构后确立下列模式,后续 `PairingDeps` / `SyncDeps` 照抄
- **分层**:
  ```
  AppFacade { pub space_setup: SpaceSetupFacade, pub pairing: PairingFacade, ... }
              ↑ 纯聚合容器,字段 pub,无方法(除 new)
              ↓
  SpaceSetupFacade::new(SpaceSetupDeps)
              ↑ 域 facade,持有 Arc<UseCase>,按 §11.4 thin-forward 到 UseCase
              ↓
  SpaceSetupDeps { pub space_access, pub local_identity, ... }
              ↑ 纯 port 袋子,字段 pub,struct literal 构造,7 个 Arc<dyn Port>
  ```
- **为什么 Deps 字段 `pub` 而不 `pub(crate)`**:bootstrap 在外部 crate(`uc-bootstrap`)拼 adapter → 喂 Deps。`pub(crate)` 会迫使 Deps 提供 `new(7 位置参数...)`,绕一圈又回到命名混乱的起点。`pub` + struct literal = 最少样板 + 命名安全
- **为什么 AppFacade 字段 `pub` 而不走 thin-forward 方法**:用户明示 `AppFacade { pub setup, pub pairing }` 风格。调方写 `app.space_setup.initialize_space(cmd)` 比 `app.initialize_space(cmd)` 多一个词,但明确表达跨域归属;AppFacade 保持零逻辑,避免"第二个 app 层"反模式(AGENTS.md §21.2)
- **为什么 SpaceSetupFacade 保留 `initialize_space` 方法而不 `pub Arc<UseCase>`**:§11.4 要求对外只见 Facade/UseCase,UseCase `pub(crate)`;facade 方法做 thin forward + `#[instrument(skip_all)]` tracing span + 预留 `TODO(P6 · F1)` auto-start_network 锚点
- **Deps 无 `new` 方法**:7 个 `Arc<dyn Port>` 传位置参数容易错序,命名字段构造方式安全
- **命名冲突处理**:已存在 `crate::setup::SetupFacade`(milestone,14 个 UseCase,"设备加入 space"流程)。新 facade 改名 `SpaceSetupFacade`(强调"本机空间生命周期":init + unlock + 后续 lock/reset),两者语义有差异且并存;Slice 后续考虑合并
- **位置约定**:
  ```
  uc-application/src/facade/
    mod.rs                  # pub mod app_facade + pub mod <domain>; re-export
    app_facade.rs           # AppFacade 聚合
    <domain>/               # 每个 sub-facade 一个子目录
      mod.rs                # private mods + pub use
      deps.rs               # <Domain>Deps
      facade.rs             # <Domain>Facade + smoke tests
      commands.rs           # <Domain> 的 Command/Result(紧贴被用的 facade)
      errors.rs             # <Domain> 的 Error 枚举
  ```
- **后续 sub-facade 模板**(P7 PairingFacade / Slice 2 SyncFacade):照搬上述 6 文件结构;AppFacade 加一个 `pub` 字段 + new 多一个参数

### F-048 · rand 0.9 `OsRng` 不再实现 `CryptoRng`(P5 编码踩坑,2026-04-19)
- **事实**:rand 0.9 把 `rand::rngs::OsRng` 切成 `TryRngCore + TryCryptoRng` 语义(可错),**不**实现 rand_core 0.9 的 `CryptoRng`(不可错)。iroh 0.95 `SecretKey::generate<R: CryptoRng + ?Sized>` 要求不可错 CSPRNG
- **症状**:`trait bound rand::rngs::OsRng: rand::CryptoRng not satisfied` + 额外噪音 `multiple versions of rand_core in dependency graph`(`chacha20poly1305` 引入 rand_core 0.6,iroh 用 rand_core 0.9)
- **解法**:`SecretKey::generate(&mut rand::rng())`——`rand::rng()` 返 `ThreadRng`(实现 `CryptoRng + RngCore`),rand 0.9 推荐 CSPRNG 入口
- **影响/后续**:
  - Phase 0 F-011 cheat sheet 里的 `SecretKey::generate(&mut OsRng)` 在 rand 0.9 下**不能直接用**,后续 `EndpointPort` 等 iroh adapter 写作时沿用 `rand::rng()`
  - 如需一次性 `[u8; 32]`,也优先 `let mut buf = [0u8; 32]; rand::rng().fill_bytes(&mut buf);`(`RngCore::fill_bytes` trait 导入:`use rand::RngCore;` 或 `rand::Rng`)
  - 不要 `use rand::rngs::OsRng` 当"标配 CSPRNG",它已经是次选

### F-049 · sponsor ↔ joiner 的 ticket 编码约定(P7a 定稿,2026-04-19)
- **背景**:F-030 rendezvous 协议字段 `sponsorTicket: string` 规定是 opaque string,server 不解析。iroh 0.95 **已取消独立 `NodeTicket` 类型**(iroh-base 0.95 public API 只剩 `EndpointAddr / EndpointId / TransportAddr`),所以"ticket"的编码格式要我们自行约定
- **约定**:`sponsorTicket = serde_json::to_string(&endpoint.addr())`
  - `endpoint.addr() -> EndpointAddr`(iroh 0.95 方法,来自 `/iroh-0.95.1/src/endpoint.rs:812`)
  - `EndpointAddr { id: EndpointId, addrs: BTreeSet<TransportAddr> }`——已 derive `Serialize/Deserialize`
  - JSON 形式可读、易调试,体积上行 300-600 字节,CF 边缘无压力
- **适用范围**:P7a sponsor 侧写入;**P7d joiner 侧反序列化必须对称**(`serde_json::from_str::<EndpointAddr>(&ticket)`)
- **备选方案(未采纳)**:postcard + base32、CBOR、protobuf——都增加复杂度换取少量字节,Slice 1 不值得
- **`sponsorEndpointId` 字段**:`endpoint.addr().id.to_string()`——`EndpointId` 的 Display impl 是 z-base32 编码的 Ed25519 公钥
- **readiness guard**:adapter 在 `endpoint.addr().addrs.is_empty()` 时返 `InvitationError::NetworkNotStarted`——空 `addrs` 意味 endpoint bind 了但未连 relay 且无本地直连地址,joiner 拿到也拨不通

### F-050 · Slice 5 libp2p 清理签到名单(P7b 定稿,2026-04-19)

Slice 1 P7b 给 legacy `PairingTransportPort` / `NetworkEventPort` 打了 `#[deprecated(since = "slice-1", ...)]`,所有**合法**使用者通过模块级 `#![allow(deprecated)]` 静音。Slice 5 清理工作就是"删除这些文件里 `#![allow(deprecated)]` 所在模块"(或整个 libp2p adapter crate/子树)。

**清单**(文件 · 性质):
1. `uc-platform/src/adapters/libp2p_network/mod.rs` — libp2p adapter 根(整棵删)
2. `uc-platform/src/adapters/network.rs` — libp2p NoopPairingTransport fallback(删)
3. `uc-application/src/space_access/network_adapter.rs` — 桥接旧 pairing transport 到 space_access(Slice 5 换 iroh pairing session;文件可能保留但内部重写)
4. `uc-application/src/setup/facade.rs` — Setup facade 签名含旧 port(Slice 5 切到 iroh session port)
5. `uc-application/src/setup/orchestrator.rs` — 同上
6. `uc-application/src/setup/action_executor.rs` — 同上
7. `uc-application/src/setup/testing.rs` — 测试 scaffolding 对应改
8. `uc-app/src/deps.rs` — 应用层依赖组装(换 iroh port trait)
9. `uc-bootstrap/src/assembly.rs` — 装配入口(libp2p adapter 装配段删)
10. `uc-tauri/src/test_utils.rs` — tauri 测试 fake(换)
11. `uc-daemon/src/pairing/host.rs` — daemon pairing host(重写用 iroh session port)
12. `uc-daemon/src/workers/peer_discovery.rs` — 订阅 NetworkEventPort(换 PairingEventPort / 其他 Slice 2 event port)
13. `uc-daemon/src/api/pairing.rs` — unpair 走旧 port(换)
14. `uc-daemon/src/peers/monitor.rs` — 监听 NetworkEvent(换 domain-scoped event port)

**`uc-core` 侧清理**:
- `uc-core/src/network/protocol/pairing.rs` 以及整个 `network/protocol/` 子树(legacy wire types)
- `uc-core/src/network/events.rs`(`NetworkEvent` enum 以及 `DiscoveredPeer / ConnectedPeer` 等 libp2p 视角结构)
- `uc-core/src/ports/pairing_transport.rs` / `ports/network_events.rs`
- `uc-core/src/ports/mod.rs` 里 `#[allow(deprecated)] pub use` 两行 + `pub mod pairing_transport` / `pub mod network_events`

**Slice 5 验证策略**:删除后 `cargo check --workspace` 全过即证明 iroh 路径已完全替代。若某文件仍需保留(如 `space_access/network_adapter.rs` 的壳),去掉 `#![allow(deprecated)]` 后编译必然失败,再按失败点逐一切到 iroh port — 这是 self-driving 的 migration trail。

---

## F-051 · `SpaceAccessPort::prepare_join_offer` Branch A 忽略 passphrase(milestone/1.0.0 既定行为)

**发现时间**:2026-04-20 P7f 编码前复核
**来源**:`uc-infra/src/security/space_access_adapter.rs:375-437`(milestone/1.0.0 Phase B 合入)

**内容**:port 方法签名是 `prepare_join_offer(space_id, passphrase) -> JoinOffer`,**但 adapter 内部在 "已 init sponsor" 分支(Branch A)显式丢弃 passphrase**:
```rust
if already_initialized {
    let _ = passphrase;
    let keyslot = self.key_material.load_keyslot(&scope).await?;
    // ... 生成 32B nonce + 返回 JoinOffer
    return Ok(JoinOffer { ... });
}
// Branch B: 未 init 时才用 passphrase 做首次 KEK 派生
```

**语义含义**:Slice 1 sponsor 流程(已 A1/A2 完成,`keyslot_exists = true`)调 `prepare_join_offer` 时传什么 passphrase 都**不影响派生**。占位 `Passphrase::new("")` 合法。HMAC 完整链路(`derive_master_key_for_proof` + `ProofPort::build_proof` 和 `verify_proof`)各就其位,Slice 1 sponsor/joiner 直接用。

**影响范围**:
- Slice 1 P7f `SponsorHandshakeCoordinator::begin` 传占位 passphrase,Branch A 路径走通
- Slice 1 joiner 侧(P7h)仍然需要用户输入的**真** passphrase — 在 `derive_master_key_for_proof(offer, passphrase)` 里派生
- sponsor 不需要缓存 passphrase;A2 unlock 后 `SpaceAccessPort` 内部持有解锁会话的 master key,verify_proof fallback 链路(proof_adapter.rs:147-158)通过 `current_session_proof_key()` 回拿 → HMAC 比对

**未解决的 port 契约洁癖**:`prepare_join_offer` 签名带 passphrase 在已 unlocked sponsor 场景下是 vestigial(残余)。
- **选项 A**:新增 `prepare_offer_from_unlocked_session(space_id) -> JoinOffer`,已 init sponsor 专用,Slice 1 切到新方法
- **选项 B**:现状保留,文档里标 "Branch A 忽略 passphrase"(本 F 就是),物理上不阻塞
- **决议**:延后作独立清理 PR(跟 Slice 1 握手实现解耦),当前 Slice 1 走 Branch B 传占位

**教训**:port 签名 ≠ 语义真相。调用前应先查 adapter 具体分支;凭签名认知会误判"需要扩 port"(P7f 首版正是如此)。

---

## F-052 · Sponsor 侧不走 `SpaceAccessStateMachine` 的设计决议

**决策时间**:2026-04-20 P7f cleanup(合 commit `bdff9588`)
**场景**:Slice 1 sponsor-side 握手流程 = `Incoming(Request) → KeyslotOffer → ChallengeResponse → Confirm | Reject → Close`。

**核心问题 — FSM action order 与 Slice 1 排序要求冲突**:
- FSM `WaitingJoinerProof + ProofVerified` 产出 `[SendResult, PersistSponsorAccess, StopTimer]`
- Slice 1 项目规则(本轮确定):admit/trust 失败 → Reject(Internal);**persist 必须先于 Confirm**(否则 Confirm 已发 → joiner 认为配对成功 → 本地却未 save → 无法回滚 Confirm)
- 两者相反。让 FSM 主导排序会违背项目规则

**次要问题**:sponsor-side FSM 只有 `Idle → WaitingJoinerProof → Granted | Denied | Cancelled` 4 个 state + 线性 transition(verify 成功/失败是唯一分支)。enum 机制在这里不加分支验证价值,反增 enum ceremony。

**决议**:sponsor 侧 `SponsorHandshakeCoordinator` 写线性代码(`begin / verify_challenge / confirm / reject / handle_session_closed` 5 方法),放弃 FSM。

**例外**:joiner 侧(P7h 待开)保留 FSM — 有真正多状态分支:`WaitingOffer`(等 sponsor 发 KeyslotOffer)→ `WaitingUserPassphrase`(等用户输入)→ `WaitingDecision`(等 Confirm / Reject),每步可被 `CancelledByUser / Timeout / SessionClosed` 中断。FSM 在这边把每个 cancel-path 显式化是有价值的。

**排序意图对齐 legacy 的反方向**:
- legacy `SpaceAccessOrchestrator::try_admit_peer_as_member`(libp2p 路径)把 admit 错误 swallow 成 WARN — "pairing 成功不能被本地 admit 失败翻盘"
- Slice 1:**配对成功必须领先于本地状态已 commit** — 严格反过来
- 两种立场对应两种产品假设(legacy:cross-device 连通性优先 / Slice 1:local state 一致性优先)。Slice 1 选后者,因为 joiner 侧若看到假 Confirm,会认为自己已是 member 并尝试 sync,而 sponsor 本地没记录,会拒绝对应流量 — 信号倒错体验更差

**复用的 uc-core 子件**(FSM 不用,但以下仍然复用):
- `JoinOffer` / `SpaceAccessProofArtifact` / `ProofDerivedKey` value objects
- `ProofPort::verify_proof` HMAC 验证
- `AdmitMemberUseCase` / `TrustPeerUseCase` 持久化 use case

**范围**:仅 sponsor 侧。joiner 侧 FSM 使用计划**已在 P7h 被推翻** — 见 F-053。

---

## F-053 · Joiner 侧也不走 `SpaceAccessStateMachine`(P7h 推翻 F-052 的"joiner 保留 FSM"说法)

**Slice 1 decision · 决定于 2026-04-20 P7h 实施时**

F-052 的结尾说"joiner 侧(P7h 待开)保留 FSM — 有真正多状态分支"。P7h 真正动手时这个判断被推翻。**Slice 1 joiner 侧也走线性 use case,不使用 `SpaceAccessStateMachine`。**

**真正的分支在哪里**:
F-052 预想的 joiner FSM 分支是 `WaitingOffer → WaitingUserPassphrase → WaitingDecision` — 即"收 KeyslotOffer 后弹窗让用户输口令"的两阶段 UX。但 **Slice 1 UX 不是这样的**:

- 用户一次性输入 code + passphrase,点 "Join"
- Application 一个同步 use case 跑完:dial → Request → KeyslotOffer → derive → build_proof → ChallengeResponse → Confirm
- 没有"中途等用户输入"的状态

→ joiner 握手和 sponsor 一样线性,`WaitingUserPassphrase` 这种状态不存在。`CancelledByUser` / `Timeout` 两种路径则 Slice 1 只实现 `Timeout`(通过 `tokio::time::timeout` 包 `recv_next`),cancel button 留给后续 slice。

**持久化排序问题(和 F-052 同病)**:
即使单看"多状态"这个角度可以说服自己用 FSM,F-052 指出的排序冲突在 joiner 这边同样存在:

- Slice 1 要求 `admit → trust → setup_status.set_status(completed)` 全部成功再告诉用户 "Ok"
- `SpaceAccessStateMachine` 的 joiner-side `PersistJoinerAccess` action 和 `SendResult` 排序与这个要求不一致
- 重造 FSM 的 action 执行器只会变成"带一堆 noop action handler 的线性路径",不加安全性

**决定**:`RedeemPairingInvitationUseCase`(`uc-application/src/usecases/pairing/redeem_invitation.rs`)直接用 Rust async control flow(match on recv'd message / early-return on error)。FSM 整个不进这条路径。

**未来什么时候回头看这个决定**:
1. **Slice 2+ 引入两阶段口令 UX**(收到 KeyslotOffer 后才弹窗要口令):状态机有价值。
2. **Slice 2+ 引入 cancel button**:cancel vs timeout vs sponsor-reject vs connection-lost 的 4 路分支值得显式建模。
3. **多轮口令重试**(用户第一次输错,不关闭 session 再试):状态转换有价值。

**影响的 legacy 代码**:`uc-core/src/space_access/{state_machine,action,event,state}.rs` 的 joiner-side 分支现阶段在 Slice 1 无使用方 —— 保留(joiner FSM 整体和 sponsor-side `space_access::initialize_new_space` 共享状态机实现,删 joiner 分支会破坏那条路径)。Slice 5 libp2p pairing 清理时统一审视是否下沉到 dead-code 状态。

**范围**:Slice 1 sponsor-side(F-052)+ joiner-side(本 finding)都不走 FSM。`SpaceAccessStateMachine` 在 Slice 1 只被 `space_access/initialize_new_space.rs`(A1 路径)使用 —— 和 pairing 无关。

---

## F-054 · A1 identity 生命周期归 bootstrap,不归 A1

**Slice 1 decision · 决定于 2026-04-20 P8 实施时**

原 A1 设计(F-016 / P3)把 Ed25519 identity 的创建当作 A1 use case 的一个步骤 —— `LocalIdentityPort::create()` 严格语义,首装 A1 创,之后 A2 `ensure()` 读。Slice 1 引入 iroh 共享 endpoint 后这个模型破了。

**为什么破**:
iroh `Endpoint` 绑定时必须已有 `SecretKey` —— peer 通过 endpoint id 认身份,**bootstrap 必须在 facade 可用前就 bind endpoint**(rendezvous adapter 和 session adapter 都直接持 `Arc<Endpoint>`)。Bootstrap 的 `IrohNodeBuilder::bind` 内部调 `IrohIdentityStore::ensure_secret_key()` 生成/持久化 secret key。等 A1 跑时 identity 已经存在,`create()` 必然报 `AlreadyExists`,A1 失败。**不仅测试,任何首装设备都会踩**。

**修复(方案 X)**:
identity 的生命周期从"A1 创建"改成"bootstrap 时存在"。对应变动:

- `InitializeSpaceError::IdentityAlreadyExists` → `AlreadySetup`
- A1 execute 首行加 `setup_status.get_status().has_completed == true → AlreadySetup` 守门
- `local_identity.create()` → `local_identity.ensure()`(幂等,读已有 / 懒生成)
- `LocalIdentityError::AlreadyExists` 从 `ensure()` 冒出来是违反 port 幂等契约的 adapter bug,归 `StorageFailed` 而非专用 variant

**语义层面**:
- `identity` 的生命周期 = **设备**,由 bootstrap 保证存在(iroh endpoint bind 时顺便 ensure)
- `setup_status` 的生命周期 = **一次性事件**,由 `has_completed` 标志唯一代表"用户已完成 A1"
- "首次/二次 A1" 的判别从"identity 是否存在"正确化为"setup_status.has_completed 是否为 false"

**测试变更**:
- A1 测试用例 `identity_already_exists_surfaces_specific_variant`(旧)→ 删
- 新 `already_completed_setup_rejects_before_touching_space_access` —— 验证 `setup_status.has_completed == true` 时 A1 在任何 port call 之前就 short-circuit
- 新 `identity_ensure_adapter_bug_raises_storage_failed` —— 验证 adapter 若违反幂等契约 A1 归 `StorageFailed`

**未来影响**:
1. 任何 use case 想以"identity 不存在"作为"首装"信号都是错的 —— 改看 `SetupStatusPort`
2. 若 Slice N 需要"手动 factory reset 后重跑 A1",factory_reset 必须重置 `setup_status.has_completed = false`(identity 则可保持,因为 endpoint 也还在用)
3. `LocalIdentityPort::create()` 作为 API 现在只剩 uc-core port 签名,实际使用方 = 0。保留以防将来 rotation 场景;Slice 5 清理时评估是否删

**范围**:A1 `InitializeSpaceUseCase`。A2 `UnlockSpaceUseCase` 本来就不操作 identity。B2 joiner path 也用 `ensure`。

---

## F-055 · iroh sponsor 侧 adapter 必须 spawn per-session recv pump

**Slice 1 decision · 决定于 2026-04-20 P8 E2E 实施时**

`IrohPairingSessionAdapter` 作为 `PairingEventPort` 的实现必须把"收到一帧"翻译成"发一个 `MessageReceived` 事件"。原始实现只发了第一帧的 `Incoming` 就 return,后续帧(sponsor 等待的 `JoinerChallengeResponse` 等)卡在 iroh 流 buffer 里没人读。

**为什么原始实现看起来够用**:
`PairingEventPort` 的契约(`uc-core/src/ports/pairing/events.rs`)定义了三种事件:`Incoming` / `MessageReceived` / `Closed`。`PairingInboundOrchestrator` 是**纯事件驱动**(与 joiner 侧的 `JoinerHandshakeCoordinator` 不同,后者用 `recv_next` 轮询)。所以 sponsor 侧 adapter 必须产 `MessageReceived`。但 sponsor side 的 adapter 只在 `handle_incoming` 里读了一帧就结束,这是个**真实实现 gap** —— 过去没暴露是因为 orchestrator 单元测试用 scripted 事件 port 造假后续帧,没有真实 adapter + real frames 的测试。

**修复**:
`handle_incoming` 发完 `Incoming` 后 `spawn_recv_pump(session, tx)` 起一个 tokio task:

```text
loop {
    match read_next_frame(&mut slot.recv.lock().await) {
        Ok(Some(msg))  → tx.send(MessageReceived { session, msg });
        Ok(None)       → tx.send(Closed { session, reason: None }); return;   // peer FIN
        Err(SessionError::Closed)     → tx.send(Closed { session, reason: None });      return;
        Err(other)                    → tx.send(Closed { session, reason: Some(err) }); return;
    }
}
```

pump 只在 **sponsor 侧**(`handle_incoming` 触发路径)起;**joiner 侧** `dial_by_invitation` 不起,因为 `JoinerHandshakeCoordinator` 通过 `pairing_session.recv_next(&session)` 轮询 —— 两者共起会争同一个 `SessionSlot.recv` mutex,死锁。

`recv_next` 和 pump 共享同一个 `read_next_frame(recv)` helper 保证 wire framing 逻辑单点真相。

**设计权衡**:
- **pump 没带 abort handle**:首版简化。正常流程双方都会 close 自己的 send 边,peer 的 recv 自然收 FIN → pump 退。若 peer 不礼貌会挂到 QUIC idle timeout(iroh 默认约数十秒)。属于 infra liveness 兜底,不阻塞握手成功 case
- **pump 的 Arc<SessionSlot> 生命周期独立于 `sessions` 表**:`close()` 从表里 remove,pump 仍持 Arc 继续读。正确行为 —— close 的语义是"这个 session 的 orchestrator 已经不管了",不是"立刻断流"。流的自然终止靠 peer FIN
- **tx 是 `mpsc::Sender<PairingSessionEvent>` 的 clone,不是 Weak**:subscriber drop 时 tx.send() 返 error → pump 退。最简

**未来影响**:
- Slice 2 加 `install_clipboard` 时,clipboard adapter 若也是 event-driven(sponsor 被动 receive 剪贴板推送),需要同款 pump;若是 active pull(joiner 拉),直接 `recv_next` 轮询,不起 pump
- 单元测试策略:adapter 层契约测试要包括"subscriber 视角的多帧完整序列",不只是第一帧

**范围**:`uc-infra/src/pairing/session.rs` `IrohPairingSessionAdapter::handle_incoming` + 新增 `spawn_recv_pump` + 提取 `read_next_frame`。contract 在 `PairingEventPort` 本身未变。

## F-056 · `PairingOutcome` broadcast:sponsor 侧握手结果作为应用事件(2026-04-20 · Slice 1 P9a)

**决策**:在 `uc-application` facade 层新增"sponsor 侧握手已完结"的广播通道,上层订阅一次 Receiver,在"邀请被成功消费 / 失败关闭"时拿到终态事件。daemon 常驻进程、GUI、短命 CLI 全部共享这个合同。

**数据形状**:
- `enum PairingOutcome { Success { peer_device_id, peer_device_name, peer_fingerprint } | Failure { reason: String } }`
- `tokio::sync::broadcast::channel(16)` —— 多订阅者、滞后会丢旧事件不阻塞 orchestrator
- `SpaceSetupFacade::subscribe_pairing_completion() -> broadcast::Receiver<PairingOutcome>`

**触发时机(orchestrator 内)**:
- Success:`finalise_verified` 里 admit + trust + Confirm 全部落地后 emit 一次
- Failure:邀请**已匹配**后的任何失败路径——proof 拒(passphrase 不对)/ admit 失败 / trust 失败 / Confirm 送出失败 / 邀请已过期(`TakeMatchingError::Expired`)/ 持有者不变式破损 / clock 越界
- 不 emit 的场景:`InvitationMismatch`(陌生 code,不是我们的邀请),非-Request 首帧(无法归属到邀请),中间 ghost session

**为什么不 emit "stranger reject"**:防止 CLI `invite` 命令被过路扫描流量误退出。Outcome 语义是"一次我们在等的配对流程走到了终点",不是"handler 收到了什么"。

**CLI invite 消费**:
```rust
let mut outcome_rx = facade.subscribe_pairing_completion();
// issue B1
select! {
    outcome = outcome_rx.recv() => handle(outcome),
    _ = signal::ctrl_c() => graceful_exit(),
}
```

**GUI/daemon 消费方向**:未来可以把同一个 Receiver 喂给 Tauri event emitter 或 IPC 事件流,保持上层合同稳定。

**范围**:新 `uc-application/src/facade/space_setup/events.rs`、`facade.rs` 加 `pairing_outcome_tx` 字段 + subscribe 方法、`orchestrator.rs` 加 `outcome_tx` 构造参数 + `emit_failure`/`Success emit` 两个触发点、测试覆盖 6 个分支(Success/passphrase/admit/trust/expired/stranger 不 emit)。

---

## F-057 · CLI session resume:`try_resume_session` 是已有基础设施,不需要新 cache 层(2026-04-20)

**场景**:CLI 是短命进程。`init`(A1)在进程 A 里解锁 master_key 进内存 session,进程 A 退出,`invite`(新进程 B)启动时 session 是空的。Joiner ChallengeResponse 到达时 sponsor 的 HMAC proof verify 读 `current_session_proof_key()` 拿 None → `proof verification failed: space session is locked` → Reject(PassphraseMismatch)。用户看到的是"双方口令明明一致却配对失败"。

**错误诊断**:看起来是 HmacProofAdapter 的 bug 或要新增"wrapped master_key 本地缓存"的 feature。最初的设计方案是加 `SpaceAccessPort::cache_unlocked_session` + `clear_cached_session`、在 init/unlock 成功后写入 `SecureStoragePort`,加 CLI `lock` 命令清理——工程量中等。

**真正的发现**:基础设施**已经全部在了**,只是 CLI 从来没调用过:
- `KeyMaterialStore::store_kek` 在 `init` 成功(`space_access_adapter.rs:112`)和 `unlock` 成功(`:222`)时就会把 KEK 经由 `SecureStoragePort` 写进 keychain(生产)或 0600 文件(`--dev`)
- `DefaultSpaceAccessAdapter::try_resume_session` 已经有完整的"读 keyslot + 读 keyring KEK + 解包 master_key + 注入 `InMemorySession`"路径
- daemon/GUI 原本就在 startup 走这条路,CLI 只是没有对应的入口

**修复**:
- `uc-application`:`SpaceSetupFacade::try_resume_session() -> Result<bool, TryResumeSessionError>`,包装 port 方法;`Ok(true) = 就绪`、`Ok(false) = 没东西可 resume`(setup_status.has_completed == false 或 keyslot 不存在)、Err = KeyringMiss / CorruptedKeyMaterial / Internal
- `uc-cli/invite`:开头 `facade.try_resume_session().await`,失败按具体错误给用户提示

**关键教训**:定位新问题前先看现有 port 是否已经能解决。这条 session resume 路径之前只有长命进程(daemon/GUI)用,加 CLI 是同样契约,不需要新 port 方法、新 adapter 代码、新 cache key 方案。

**未来影响**:
- 未来 `uniclipboard-cli unlock` 命令(当 KeyringMiss 时)用 `facade.unlock_space` 即可,不用新接口
- 未来 `uniclipboard-cli lock` 要设计"只清内存会话" vs "也清 keyring KEK" 两种语义——目前 `SpaceAccessPort::lock` 契约只清内存。如果要机器本地"hard lock",需要单独的 `clear_cached_session` 端口方法(本次未加)
- macOS keychain 的 login-session 绑定是天然的"锁屏即失效"语义,符合安全直觉;`--dev` 下是 0600 文件,没 OS 级保护,这是开发/CI 模式的既定折中

**范围**:`uc-application/facade/space_setup/{errors.rs,facade.rs,mod.rs}`、`uc-cli/commands/invite.rs`。

---

## F-058 · `SpaceId` 必须在 `SetupStatus` 里持久化,否则 sponsor/joiner 漂移(2026-04-20)

**bug 复现**:用户用 `--profile x`(sponsor)和 `--profile y`(joiner)跑通了握手,双方 CLI 都退出 0,但最后输出:
- sponsor `init` 报告 space_id: `a01610f2-6791-45fb-...`
- joiner `join` 成功后报告 space_id: `7a872c8f-09f5-4818-...`

两端对"同一个 space"的指认已经分叉。

**根因**:代码里 `SpaceId` 从来没被持久化。每个独立进程都 `SpaceId::new()` 铸一个 fresh UUID:
- `InitializeSpaceUseCase::execute`:line 116 `let space_id = SpaceId::new();`
- `UnlockSpaceUseCase::execute`:line 53 `let space_id = SpaceId::new();` + 注释 "adapter keys keyslot lookup off profile, not this id"
- `SponsorHandshakeCoordinator::begin`:line 146 `let probe_space_id = SpaceId::new();` + 注释 "adapter's Branch A does not consult it"

`DefaultSpaceAccessAdapter` 本身的 keyslot 查找确实是 profile-based 不依赖 space_id,所以 keyslot 层无感;但 `prepare_join_offer` 把 handshake 现场铸的 probe_space_id 塞进 `JoinOffer.space_id` 发给 joiner,joiner `RedeemPairingInvitationUseCase` 把这个 id 当 sponsor 的空间记进自己的 `SetupStatus.has_completed = true`(只有 bool,没记 space_id),下次任何 CLI 起来都是新 UUID。

**修复**:让 `SpaceId` 在 A1 初始化那一刻被持久化,之后各处从同一个真相源读。
- `uc-core`:`SetupStatus { has_completed: bool, space_id: Option<SpaceId> }`(`#[serde(default)]` 兼容老数据)
- `uc-application`:
  - `InitializeSpaceUseCase`:A1 成功时 `setup_status.set_status(SetupStatus { has_completed: true, space_id: Some(minted_id) })`
  - `RedeemPairingInvitationUseCase`:joiner B2 成功后 `set_status(SetupStatus { has_completed: true, space_id: Some(outcome.space_id) })`——从 sponsor 收到的 JoinOffer 里拿,不铸新的
  - `SponsorHandshakeCoordinator`:加 `SetupStatusPort` 依赖,`begin()` 里读 `status.space_id`,None 就 fallback 到 fresh UUID + `warn!`(legacy 兼容路径)
- `A2 UnlockSpaceUseCase` 的 fresh UUID 留着不动:comment 已说明 adapter 不看这个 id,此处的修改会级联到多个测试,按 incremental 改

**为什么 SetupStatus 是正确的归属地**:`SetupStatus` 就是"本 profile 上 setup 的语义状态",space_id 是这个状态的核心参数之一。放这里不跨越 port,不污染加密层持久化格式。

**legacy 路径**:老 profile(这次升级前已 init)的 `SetupStatus.space_id == None`,sponsor handshake 会走 fallback UUID,配对出来的新 joiner 会持有一个跟 sponsor 任何历史 id 都对不上的新 id——仍然不一致,但至少 fresh profile 立刻正确。用户发现后可以手动 `/bin/rm -rf ~/Library/Application Support/app.uniclipboard.desktop-<profile>` 清理重试。

**未来影响**:
- `SpaceId` 的真相源现在是 `SetupStatus.space_id`;任何需要它的新代码(Slice 2+ clipboard/file transfer)都从这里读,不要 `SpaceId::new()`
- `A2 UnlockSpaceUseCase` 返回的 `UnlockSpaceResult.space_id` 下一轮需要对齐:改成从 `SetupStatus.space_id` 读,而不是 mint。等下个 slice 在 API review 时顺手做
- 长期:`SpaceId::new()` 在 application 层应该全部禁用——只允许在 A1 `initialize_space` 里出现一次。加 lint / grep 白名单守住。

**范围**:`uc-core/src/setup/status.rs`、`uc-application/src/usecases/setup/initialize_space.rs`、`uc-application/src/usecases/pairing/redeem_invitation.rs`、`uc-application/src/pairing_inbound/sponsor_handshake.rs`、`uc-application/src/facade/space_setup/facade.rs`、所有测试的 `SetupStatus { ... }` 字面量。

---

## F-059 · rendezvous 客户端 URL 形态与服务端不匹配(2026-04-20)

**问题**:Slice 1 的 rendezvous 客户端代码写的是:
- sponsor consume:`POST /v1/pairings/{code}/consume`(路径参数)
- joiner resolve:`GET /v1/pairings/{code}`(GET + 路径参数)

但 uc-rendezvous 服务端(`/Volumes/ExternalSSD/myprojects/uc-rendezvous`)实际只暴露:
- `POST /v1/pairings/consume` + body `{ "code": "..." }`
- `POST /v1/pairings/resolve` + body `{ "code": "..." }`

`POST /v1/pairings`(create)双方约定一致,没问题。

**怎么发现的**:CLI `invite` 一直报 `pairing invitation service unavailable`,本地 test 环境下根本没法联通 rendezvous。初期按 TLS / 证书方向查了一圈,最后让 subagent 读 uc-rendezvous 源码,才确认 API 形态不对。

**误导路径总结**(后来确认不是根因,但代码里留着是正当保险):
- 加了 User-Agent 头——CF bot management 某些场景确实会对匿名 UA 做 TCP 重置,但对我们这个 endpoint 没触发
- 加了 `rustls-tls-webpki-roots` feature——见 F-060,这条是必需的
- 加了 `rustls::crypto::ring::default_provider().install_default()` ——iroh-quinn + reqwest 共用 rustls 0.23,process-wide provider 冲突理论风险;实测没触发但留着无害

**真正的症结**:我所在 Claude Code 工具 shell 里设了 `SSL_CERT_FILE=/etc/ssl/cert.pem`(2021 年的 OpenBSD 老 CA bundle),rustls 解析里面某条 cert 的 DER 失败 → `CertificateError::BadEncoding` → 表现为 "bad certificate format" transport 错误。用户在 Ghostty 终端里跑就好了——那里没设这个环境变量。**这是工具环境污染,不是代码问题**。

**修复**:
- `uc-infra/rendezvous/client.rs`:URL 改 `/v1/pairings/consume` + JSON body `{code}`;409 映射到 `NotFound`(server 的 `pairing_already_consumed` 是幂等场景)
- `uc-infra/pairing/session.rs`:URL 改 `POST /v1/pairings/resolve` + JSON body `{code}`
- 两处 reqwest client 都加 `User-Agent` + `err_chain` helper(把 `Error::source()` 链拍平成字符串,让 "error sending request" 这条日志真的告诉你底层原因)
- 所有 wiremock 测试 / bootstrap e2e mock 跟着改路径

**未来影响**:rendezvous 协议契约权威文档应在 `findings.md#F-030` 基础上更新,把两个实际路径记准,避免下次又猜。

**范围**:`uc-infra/src/rendezvous/client.rs`、`uc-infra/src/pairing/session.rs`、`uc-bootstrap/tests/slice1_handshake_e2e.rs`。

---

## F-060 · reqwest 0.12 `rustls-tls` 不自带 root CA,必须显式加 `rustls-tls-webpki-roots`(2026-04-20)

**问题**:reqwest 0.12 把 rustls TLS 根证书 store 拆成了独立 feature:
- `rustls-tls` = 仅开 rustls TLS,没 root CA store
- `rustls-tls-webpki-roots` = webpki-roots(Mozilla list,跨平台)
- `rustls-tls-native-roots` = OS 原生信任链(macOS Keychain / Windows 信任链 / Secret Service)

本仓库所有 5 个用 reqwest 的 crate(uc-infra、uc-cli、uc-tauri、uc-daemon-client、uc-observability)之前只开了 `"json", "rustls-tls"`,**没有任何 root CA 可用**。任何 HTTPS 出站请求都会在 cert 验证时失败。

**之前为什么没炸**:这些 reqwest 客户端没怎么被真实出站过——observability OTLP 默认禁用,daemon-client 是内部 127.0.0.1,uc-tauri 用于 update-check 大概只跑过几次没人注意。Slice 1 CLI 是第一个高频打外部 HTTPS 的 caller,把这条缺失暴露出来。

**修复**:5 个 Cargo.toml 统一加 `"rustls-tls-webpki-roots"`——选 webpki-roots 而不是 native-roots,因为 webpki 跨平台一致、体积小、不依赖 OS 信任链配置;未来如果需要企业 CA,再切 native。

**未来影响**:新加 reqwest 依赖的 crate 必须同时开 `rustls-tls-webpki-roots`,不然又会重蹈覆辙。workspace Cargo 没统一 reqwest 别名,这条靠 code review 守。

**范围**:5 个 crate 的 `Cargo.toml`。

---

## F-061 · 非-bundled macOS CLI 的 NSPasteboard 空返回 → 全量 panic(2026-04-20)

**问题**:macOS 上,`clipboard-rs`(`ClipboardContext::new`)在构造时调 `+[NSPasteboard generalPasteboard]`。如果进程不是正经 `.app` bundle、也没连到 WindowServer(典型:从 SSH、从 CI、从 Claude Code 工具这类 detached shell 启动),这个 Objective-C call 返回 NULL,`objc2-app-kit` 内置检查直接 Rust panic:
```
thread 'main' panicked at .../NSPasteboard.rs:323:5:
unexpected NULL returned from +[NSPasteboard generalPasteboard]
```

栈上的 caller 是 `uc_bootstrap::assembly::create_platform_layer`,所以**任何触发 `wire_dependencies` 的短命 CLI 在这些环境都会直接崩**。

**为什么之前没暴露**:同样是"CLI 新路径",Slice 1 之前只有 Tauri GUI(bundled .app,`NSPasteboard` 正常)和 daemon(由 launchd 管,也有 WindowServer)。legacy `setup pair` 路径虽然也跑 `build_cli_runtime`,但执行场景几乎总是用户从 `open -a`/UI 起来,不是纯 shell。

**修复**:在 `create_platform_layer` 里加一道环境开关:
```rust
if std::env::var_os("UC_DISABLE_SYSTEM_CLIPBOARD").is_some() {
    tracing::info!("UC_DISABLE_SYSTEM_CLIPBOARD set; substituting NoopSystemClipboard");
    // read_snapshot 返空、write_snapshot 无操作
} else {
    LocalClipboard::new()? // 原路径
}
```

CLI 的 `slice1_common::build_assembly` 在调用 `build_slice1_cli_context` 之前 `std::env::set_var("UC_DISABLE_SYSTEM_CLIPBOARD", "1")`,GUI/daemon 保持未设置,继续用真实 adapter。

**新增 adapter**:`uc-platform/src/clipboard/noop.rs` 的 `NoopSystemClipboard` 显式实现 `SystemClipboardPort`,经过 blanket impl 自动满足 `PlatformClipboardPort`。遵循 `uc-platform/AGENTS.md` §9.3 "明确返回 Unsupported" 的原则。

**关联尝试**:先试过 `extern "C" fn NSApplicationLoad()` 在 main 起来时调一次初始化 AppKit——文档里说这是非 .app 进程的标准启动方式,但实测在真正 headless 的 shell 下 NSPasteboard 仍然返 NULL。保留了这条调用(无害、某些情景可能有用),主修靠 noop fallback。

**未来影响**:
- Slice 1 CLI 的 init/invite/join 都不走 clipboard,永远 noop,没功能损失
- 未来如果 CLI 要支持"一次性复制到剪贴板"这种命令(例如 `uniclipboard get`),要么新设计(打印到 stdout 让 pipe 处理),要么把 clipboard adapter 做成 lazy-on-first-use(调到了再 `generalPasteboard`,panic 时 catch_unwind fallback)
- Linux/Windows 的对应路径(`x11-clipboard`/`WindowsClipboard`)没遇到这个问题,因为它们的 eager 构造不需要 display/windowserver——所以 `UC_DISABLE_SYSTEM_CLIPBOARD` 是 macOS 专属保险,但开关做成通用的,将来任何平台的 headless 场景都能复用

**范围**:`uc-platform/src/clipboard/{mod.rs,noop.rs}`、`uc-bootstrap/src/assembly.rs`、`uc-cli/src/commands/slice1_common.rs`。

---

## libp2p 删除影响面深度分析(2026-04-24 · Slice 4 规划用)

> **背景**:用户决定取消 plan 原本"双栈并行验证 1-2 周"的 Slice 4,改为先一次性删 libp2p 业务代码,再单独做后续优化。本节用 grep 证据回答"删什么、有没有 iroh 替代、风险点在哪"。

### F-100 · 关键发现:GUI 进程里的 libp2p 业务路径已经是空跑死代码

**证据**:
- `uc-tauri/src/bootstrap/runtime.rs:379-382` 定义工厂方法 `sync_outbound_clipboard()` 构造 `SyncOutboundClipboardUseCase`,**但全仓库零调用方**(`rg -n "sync_outbound_clipboard\(" src-tauri/crates/` 仅命中定义本身)
- `uc-app` 内部代码 0 处引用 `SyncOutbound/SyncInbound/FileSync/sync_outbound/sync_inbound`(`rg -n "SyncOutbound|SyncInbound|FileSync|sync_outbound|sync_inbound" src-tauri/crates/uc-app/src/ | grep -v "//"` 0 命中)
- `uc-daemon` 完全不消费 `uc_app::usecases::clipboard::sync_*` 或 `uc_app::usecases::file_sync::*`(0 命中)

**含义**:v0.4.0 milestone 完成 daemon-first 之后,GUI 进程的 `CoreRuntime` 虽然仍在 `AppDeps` 里持有 libp2p adapter Arc,但相关 usecase 没有被任何启动路径触发。**删除这些代码不会损失任何运行中功能**。

**风险**:剩余风险只在**编译**(`AppDeps` 字段被 `Arc<dyn FileTransportPort>` 等占着,删 port 要同步删字段)和**装配**(`bootstrap/runtime.rs` 仍 wire libp2p adapter,删了要拆装配链)。

---

### F-101 · libp2p 直接依赖文件清单(13 + 文档 1)

`rg -l "use libp2p" src-tauri/crates/`:

| # | 文件 | 角色 |
|---|---|---|
| 1 | `uc-platform/src/adapters/libp2p_network/behaviour.rs` | swarm 行为定义 |
| 2 | `uc-platform/src/adapters/libp2p_network/business_command.rs` | 出站命令处理 |
| 3 | `uc-platform/src/adapters/libp2p_network/business_stream.rs` | 业务流帧处理 |
| 4 | `uc-platform/src/adapters/libp2p_network/dial_strategy.rs` | dial 策略 |
| 5 | `uc-platform/src/adapters/libp2p_network/discovery.rs` | mDNS / 发现 |
| 6 | `uc-platform/src/adapters/libp2p_network/mod.rs` | adapter 入口,实现 5 个 port |
| 7 | `uc-platform/src/adapters/libp2p_network/peer_cache.rs` | peer 缓存 |
| 8 | `uc-platform/src/adapters/libp2p_network/recovery_probe.rs` | 恢复探针 |
| 9 | `uc-platform/src/adapters/libp2p_network/stream_handler.rs` | 流处理 |
| 10 | `uc-platform/src/adapters/libp2p_network/swarm_event_loop.rs` | swarm 事件循环 |
| 11 | `uc-platform/src/adapters/file_transfer/service.rs` | 文件传输服务 |
| 12 | `uc-platform/src/adapters/pairing_stream/service.rs` | 配对流服务 |
| 13 | `uc-platform/src/identity_store.rs` | libp2p 专用 identity store(F-046 标注 frozen) |
| 14 | `uc-core/AGENTS.md` | 文档,可忽略 |

`uc-platform/src/adapters/libp2p_network/` 目录下另有 5 个文件未直接 `use libp2p` 但属同一目录,删目录时一并删:`address_registry.rs`、`platform_signals.rs`、`recovery_coordinator.rs`、`recovery_events.rs`、`mod.rs` 同目录扩展。

---

### F-102 · port-by-port 替代度对照表

| 旧 port | 文件 | impl 位置 | 业务消费者 | iroh 替代 | 完整覆盖? |
|---|---|---|---|---|---|
| `PairingTransportPort` | `uc-core/src/ports/pairing_transport.rs` | `libp2p_network/mod.rs:825` + `adapters/network.rs::DisabledPairingTransport` + 测试 fakes | `uc-application/setup/{action_executor,orchestrator,testing}.rs`、`uc-application/space_access/network_adapter.rs`、`uc-app/deps.rs` | `uc-core/src/ports/pairing/{events,session}.rs` 的 `PairingEventPort` + `PairingSessionPort`(Slice 1 P9) | **否**:setup orchestrator 还在通过旧 port 走"unpair_device"等动作。删除前要把 `setup/action_executor.rs` 等的剩余 4 处 import 切到新 port |
| `NetworkEventPort` | `uc-core/src/ports/network_events.rs` | `libp2p_network/mod.rs` 唯一实现 | `uc-daemon/src/peers/monitor.rs`、`uc-daemon/src/workers/peer_discovery.rs`、`uc-app/deps.rs` | `uc-core/src/ports/presence.rs` 的 `PresencePort`(Slice 2 P1) | **部分**:daemon 的 peer_discovery / peer monitor 当前**还订阅旧 NetworkEvent**,要切到 `PresencePort` 事件流。这是删除工作中风险最高的一项 |
| `FileTransportPort` | `uc-core/src/ports/file_transport.rs` | `libp2p_network/mod.rs` 唯一 | `uc-app/usecases/file_sync/sync_outbound.rs`(死代码)、`uc-app/deps.rs` 字段 | `BlobTransferPort`(Slice 3) | ✅ blob 路径已端到端跑通(daemon 出入站均接通) |
| `ConnectionPolicyResolverPort` | `uc-core/src/ports/connection_policy.rs` | `uc-app/usecases/pairing/resolve_connection_policy.rs` 提供 logic | 仅 `libp2p_network/{swarm_event_loop,stream_handler,business_stream,business_command,mod}.rs` | 无需替代 | ✅ libp2p 删后无消费者,整个 port 一并删 |
| `DiscoveryPort` | `uc-core/src/ports/discovery.rs` | `uc-bootstrap/assembly.rs:1045` 内联 `NetworkDiscoveryPort` + `EmptyDiscoveryPort` | `uc-application/setup/{orchestrator,action_executor,facade,testing}.rs` | iroh 栈无对应 port——已被 rendezvous + DNS discovery 替代 | **部分**:setup 还在传 `Arc<dyn DiscoveryPort>` 当占位参数,删除时要清理 setup orchestrator 签名 |
| `ClipboardOutboundTransportPort` | `uc-core/src/ports/clipboard/transport.rs:59` | `libp2p_network/mod.rs` + 测试 fake | `uc-bootstrap/builders.rs`、`uc-app/deps.rs`、`uc-app/usecases/clipboard/sync_outbound.rs`(死代码) | `ClipboardDispatchPort`(Slice 2 P2 T1)、`ClipboardSyncFacade::dispatch_snapshot[_with_blob_refs]`(daemon 已接) | ✅ 完整 |
| `ClipboardInboundTransportPort` | `uc-core/src/ports/clipboard/transport.rs:87` | `libp2p_network/mod.rs` + 测试 fake | `uc-bootstrap/builders.rs`、`uc-app/deps.rs` | `ClipboardReceiverPort`(Slice 2 P2 T1)、`ApplyInboundClipboardUseCase`(daemon 已接) | ✅ 完整 |
| `NetworkControlPort` | `uc-core/src/ports/network_control.rs` | 多处 | 多处 | **保留**——已在 Slice 1 N-1 决议为通用 port,iroh adapter 实现 `stop_network()` | **不删** |

---

### F-103 · `uc-core/src/network/` 模块去留

`uc-core/src/network/` 不能整个删——有些类型在新栈中仍被复用:

| 子模块 | 内容 | 新栈仍用? |
|---|---|---|
| `events.rs` | `NetworkEvent`、`ConnectedPeer`、`DiscoveredPeer`、`PeerTrustStatus` | ❌ 仅旧路径用,删 |
| `connection_policy.rs` | 旧 connection policy types | ❌ 删 |
| `session.rs` | `SessionId` | ✅ 被 `uc-application/pairing/{protocol_handler,session_manager}.rs`、`uc-daemon/pairing/host.rs` 复用——**保留** |
| `protocol/pairing.rs` | `PairingMessage`、`PairingChallenge`、`PairingResponse`、`PairingBusy` | ⚠️ `PairingMessage`、`PairingBusy` 被新 `space_access/network_adapter.rs` 用;`PairingChallenge`、`PairingResponse` 仅旧 `pairing/state_machine.rs`(死代码,见 F-105)用——**拆,保前两个** |
| `protocol/clipboard.rs` | `ClipboardBinaryPayload`、`BinaryRepresentation`、`MIME_IMAGE_PREFIX`、`ClipboardPayloadVersion`、`ClipboardMessage`、`ProtocolMessage`、`ProtocolDirection` | 部分:`ClipboardBinaryPayload`/`BinaryRepresentation` 被 `uc-application/usecases/clipboard_sync/payload_codec.rs` 用,**保留**;`ClipboardMessage`/`ProtocolMessage`/`ProtocolDirection`/`ClipboardPayloadVersion` 仅旧 `uc-app/usecases/clipboard/sync_outbound.rs`(死代码)+ libp2p adapter 用,**删** |
| `protocol/clipboard_payload_v3.rs` | V3 wire 编解码 | ✅ 保留 |
| `protocol/file_transfer.rs` | 旧 file transfer wire | ❌ 删 |
| `protocol/heartbeat.rs` | 旧 heartbeat | ❌ 删(iroh 自带 keepalive,Phase 3 已加 PeerKeepAliveWorker) |
| `protocol/device_announce.rs` | `DeviceAnnounceMessage` | ⚠️ 仅 libp2p adapter 用,删 |
| `protocol/protocol_message.rs` | 旧 wire 信封 | ❌ 删 |
| `protocol/mod.rs` | 模块导出 | ⚠️ 重构 |
| `mod.rs` | network 顶层 mod | ⚠️ 删除 events/connection_policy 子模块导出 |

**结论**:`uc-core/src/network/` 不删整个目录,但要做内部清理。建议保留为 `uc-core/src/network/` 但只剩 `session.rs` + `protocol/{pairing,clipboard,clipboard_payload_v3}.rs` 的瘦身版。

---

### F-104 · `uc-core/src/ids/peer_id.rs` 实际依赖范围

`rg -n "PeerId\b" src-tauri/crates/uc-core/src/`:
- `uc-core/src/ids/mod.rs:16` `pub use peer_id::PeerId`
- `uc-core/src/lib.rs:38` `pub use ids::{BlobId, DeviceId, PeerId, SessionId}`
- `uc-core/src/ports/connection_policy.rs:1,15` 旧 port 内部用
- `uc-core/src/ports/clipboard/transport.rs:6` 仅注释提到("不直接绑定 libp2p::PeerId / iroh::NodeId")
- `uc-core/src/network/protocol/pairing.rs:24,31,32` 注释提到 + 字段(`PairingChallenge`/`PairingResponse` 用 `peer_id: String`,**不**用 `PeerId` 类型本身)

**结论**:`PeerId` 类型实际**0 处**被业务代码当类型参数使用。删 `connection_policy.rs` 后 `PeerId` 即孤立,可一并删除文件 + `ids/mod.rs:16` re-export + `lib.rs:38` re-export。

---

### F-105 · 旧 pairing 状态机的去留

- `uc-application/src/pairing/state_machine.rs` 包含 `PairingState::AwaitingUserConfirm`、`PairingChallenge { ... }`、`PairingResponse { ... }`(行 75 / 722-858 / 765 / 1112 等)
- `rg -l "uc_application::pairing::state_machine|use crate::pairing::state_machine|PairingStateMachine\b"` **0 个外部消费者**

**结论**:`uc-application/src/pairing/state_machine.rs` 是孤立死代码,可整文件删除。会带掉 `AwaitingUserConfirm` / `PairingChallenge` / `PairingResponse` 残留。

但 `uc-application/src/pairing/{protocol_handler,session_manager,facade}.rs` 是 milestone/1.0.0 复用项,**不**是状态机的依赖——见 F-031.5(已 2026-04-18 评估"复用推论")。删 state_machine 不影响这些文件。

---

### F-106 · `uc-app` 旧 usecase 死代码清单

完全孤立(0 个外部消费者):
- `uc-app/src/usecases/clipboard/sync_outbound.rs`(`SyncOutboundClipboardUseCase`)
- `uc-app/src/usecases/clipboard/sync_inbound.rs`
- `uc-app/src/usecases/file_sync/sync_outbound.rs`(`SyncOutboundFileUseCase`)
- `uc-app/src/usecases/file_sync/sync_inbound.rs`(`SyncInboundFileUseCase`)
- `uc-app/src/usecases/file_sync/sync_policy.rs`(伴随)
- `uc-app/src/usecases/file_sync/cleanup.rs`(伴随)——除非 `CleanupExpiredFilesUseCase` 还有消费者,需复查
- `uc-app/src/usecases/pairing/resolve_connection_policy.rs`(`ConnectionPolicyResolverPort` 唯一实现)

需保留(被 daemon 或 GUI 真路径消费):
- `uc-app/src/usecases/clipboard/clipboard_write_coordinator.rs`(daemon 接收路径用)
- `uc-app/src/usecases/clipboard/list_entry_projections/`(GUI 列表用)
- `uc-app/src/usecases/clipboard/{get_entry_detail,delete_clipboard_entry,toggle_favorite_clipboard_entry,touch_clipboard_entry,clear_history,...}` 等(GUI / daemon 业务命令)

---

### F-107 · `uc-platform/adapters/` 整体去留

整目录 25 个文件:

**全删**:
- `libp2p_network/`(14 文件)
- `pairing_stream/`(3 文件)
- `file_transfer/`(6 文件)

**保留**:
- `network.rs`(`PairingRuntimeOwner` 还有意义,但 `DisabledPairingTransport` 随旧 port 删除——文件保留,内容大幅瘦身)
- `protocol_ids.rs`(全局协议 ID 常量,保留)
- `mod.rs`(模块声明,清理 re-exports)

`uc-platform/src/identity_store.rs`:
- libp2p 专用 `SystemIdentityStore`,F-046 已标 frozen
- iroh 栈用 `uc-infra/src/network/iroh/identity_store.rs` 独立实现
- **整文件删**;`uc-platform/src/lib.rs` 同步去掉 export

---

### F-108 · uc-tauri 装配链改动点

`uc-tauri/Cargo.toml` 仍 `uc-app = { path = "../uc-app" }`——保留(uc-app 删旧 usecase 后仍是有用 crate)。

`uc-tauri/src/bootstrap/runtime.rs`:
- 第 35 行 `use uc_app::{runtime::CoreRuntime, App, AppDeps};`——`AppDeps` 字段会变小,需同步
- 第 379-388 `sync_outbound_clipboard()` 工厂——零调用方,**整方法删**
- 注释行(L18,L127)的 `#[tauri::command]` 示例 docstring,改示例

`uc-tauri/src/commands/`:**完全无影响**——业务命令早已不在这里(7 个命令文件全是 autostart / quick_panel / startup / storage / tray / updater)

---

### F-109 · uc-daemon 残留 libp2p 痕迹

- `uc-daemon/src/pairing/host.rs:1` `#![allow(deprecated)] // frozen libp2p PairingTransportPort consumer; replaced in Slice 5`
- `uc-daemon/src/pairing/host.rs:20` `use uc_core::network::{...}` 复用 wire 类型(F-103 已说明这些类型部分保留)
- `uc-daemon/src/peers/monitor.rs:18` `use uc_core::network::NetworkEvent;`
- `uc-daemon/src/workers/peer_discovery.rs:9` `use uc_core::network::NetworkEvent;`

**风险点**:`peer_discovery` 和 `peer monitor` worker **当前还订阅 `NetworkEvent`**——这是 `NetworkEventPort` 的 wire 类型。删 `NetworkEventPort` 必须把这两个 worker 切到 `PresencePort` 事件流。如果 daemon 在 iroh 路径下不再需要 peer_discovery worker(Slice 2 P1 已经把 roster + presence 事件化),可能整 worker 可以删掉。**需要在 plan 里单独立为子任务**。

---

### F-110 · 数据库 schema 风险

`uc-core/src/network/protocol/pairing.rs:24` 注释 "libp2p PeerId (network layer, stable while identity is persisted)"——暗示 schema 里某些字段历史上是 libp2p PeerId 字符串。

需要单独 grep 数据库 migration / schema 里的 `peer_id` 列,确认:
- 是不是已经被 `node_id` / `device_id` 替代
- 是否有数据需要迁移
- 删除是否需要新 migration

(本节因主分析未涉及 schema,留给 Slice 4 实施时核实)

---

### F-111 · 删除清单总表(执行级)

#### 一次性整体删除(目录/文件)
| 路径 | 大小估计 | 风险 |
|---|---|---|
| `uc-platform/src/adapters/libp2p_network/`(整目录,14 文件) | 大 | 低(F-100) |
| `uc-platform/src/adapters/pairing_stream/`(整目录,3 文件) | 中 | 低 |
| `uc-platform/src/adapters/file_transfer/`(整目录,6 文件) | 中 | 低 |
| `uc-platform/src/identity_store.rs` | 小 | 低 |
| `uc-app/src/usecases/clipboard/sync_outbound.rs` | 中 | 低(F-100) |
| `uc-app/src/usecases/clipboard/sync_inbound.rs` | 中 | 低 |
| `uc-app/src/usecases/file_sync/sync_outbound.rs` | 中 | 低 |
| `uc-app/src/usecases/file_sync/sync_inbound.rs` | 中 | 低 |
| `uc-app/src/usecases/file_sync/sync_policy.rs` | 小 | 低(伴随) |
| `uc-app/src/usecases/pairing/resolve_connection_policy.rs` | 小 | 低 |
| `uc-application/src/pairing/state_machine.rs` | 大 | 低(F-105) |
| `uc-core/src/ports/pairing_transport.rs` | 小 | 中(F-102) |
| `uc-core/src/ports/network_events.rs` | 小 | **高**(F-109) |
| `uc-core/src/ports/file_transport.rs` | 小 | 低 |
| `uc-core/src/ports/connection_policy.rs` | 小 | 低 |
| `uc-core/src/ports/discovery.rs` | 小 | 中(F-102) |
| `uc-core/src/ids/peer_id.rs` | 小 | 低(F-104) |
| `uc-core/src/network/events.rs` | 小 | 中 |
| `uc-core/src/network/connection_policy.rs` | 小 | 低 |
| `uc-core/src/network/protocol/file_transfer.rs` | 中 | 低 |
| `uc-core/src/network/protocol/heartbeat.rs` | 小 | 低 |
| `uc-core/src/network/protocol/device_announce.rs` | 小 | 低 |
| `uc-core/src/network/protocol/protocol_message.rs` | 小 | 低 |

#### 文件内部清理(部分修改)
- `uc-app/src/deps.rs` —— 删 `clipboard_outbound`/`clipboard_inbound`/`pairing`/`events`/`file_transfer` 字段
- `uc-app/src/usecases/clipboard/mod.rs`、`uc-app/src/usecases/file_sync/mod.rs`、`uc-app/src/usecases/pairing/mod.rs` —— 同步去掉 mod 声明 + re-export
- `uc-app/src/usecases/file_sync/cleanup.rs`/`copy_file_to_clipboard.rs` —— 视消费情况保留
- `uc-app/src/runtime.rs`、`uc-app/src/lib.rs` —— 清掉旧路径
- `uc-tauri/src/bootstrap/runtime.rs` —— 删 `sync_outbound_clipboard()` 工厂 + `AppDeps` 装配链
- `uc-platform/src/adapters/{network,mod}.rs` —— 删 `DisabledPairingTransport`,`mod.rs` 去掉 `libp2p_network`/`pairing_stream`/`file_transfer`/`identity_store` 子模块导出
- `uc-platform/src/lib.rs` —— 同步
- `uc-bootstrap/src/builders.rs`、`uc-bootstrap/src/assembly.rs` —— 删 libp2p 装配分支 + `DiscoveryPort`/`NetworkDiscoveryPort`/`EmptyDiscoveryPort` 占位
- `uc-application/src/setup/{orchestrator,action_executor,facade,testing}.rs` —— 删除 `Arc<dyn DiscoveryPort>` 参数 + `Arc<dyn PairingTransportPort>` 参数
- `uc-application/src/space_access/network_adapter.rs` —— 替换 `PairingTransportPort` 调用为 iroh `PairingSessionPort`/`PairingEventPort`
- `uc-application/src/pairing/mod.rs` —— 去掉 `state_machine` 导出
- `uc-application/src/setup/mod.rs` —— 同步
- `uc-core/src/ids/mod.rs:16` —— 去掉 `pub use peer_id::PeerId`
- `uc-core/src/lib.rs:38` —— 删 `PeerId` re-export
- `uc-core/src/network/mod.rs` —— 删 `events`/`connection_policy` 子模块
- `uc-core/src/network/protocol/mod.rs` —— 删 `file_transfer`/`heartbeat`/`device_announce`/`protocol_message`
- `uc-core/src/network/protocol/pairing.rs` —— 删 `PairingChallenge`/`PairingResponse` 类型(被 state_machine 唯一消费,state_machine 删后孤立)
- `uc-core/src/ports/mod.rs` —— 同步
- `Cargo.toml`(workspace + 各 crate) —— 移除 `libp2p`/`libp2p-stream` 依赖
- `uc-daemon/src/pairing/host.rs:1` —— 去掉 `#![allow(deprecated)]` 顶级开关,清理 frozen 注释

#### 需先迁移再删除(高风险项)
- **`uc-daemon/src/peers/monitor.rs` + `uc-daemon/src/workers/peer_discovery.rs`** —— 当前订阅 `NetworkEventPort`,要切到 `PresencePort`(或证明 iroh 栈下整 worker 可删)。**这是 Slice 4 唯一一项需要写新代码的子任务**

---

### F-112 · 执行摘要

**完全可删(零功能损失)**:14 个目录 / 文件级整体删除(libp2p_network/、pairing_stream/、file_transfer/、4 个旧 sync usecase、state_machine.rs、5 个旧 port、4 个旧 wire protocol)

**已有 iroh 替代且 daemon 已接通**:
- `ClipboardOutboundTransportPort` → `ClipboardDispatchPort` + `ClipboardSyncFacade::dispatch_snapshot[_with_blob_refs]` ✅
- `ClipboardInboundTransportPort` → `ClipboardReceiverPort` + `ApplyInboundClipboardUseCase` ✅
- `FileTransportPort` → `BlobTransferPort` + `BlobTransferFacade` ✅
- `PairingTransportPort` → `PairingEventPort` + `PairingSessionPort`(daemon 已用,setup orchestrator 还需切换)⚠️

**唯一需要补的工作**:
- `uc-daemon` peer_discovery / peer_monitor worker 切到 `PresencePort` 或证明可删除

**风险**:数据库 schema 里若有遗留 libp2p `peer_id` 列,需在 Slice 4 实施时核实并决定是否带 migration

---

### F-113 · DB schema `peer_id` 列状态(已坐实)

**结论**:已经全部清理完毕,**无需新 migration**。

**证据**:
- `rg -n "peer_id" src-tauri/crates/uc-infra/src/db/schema.rs` **0 命中**
- migration `2026-04-18-000001_create_space_member/up.sql` 把旧 `paired_device.peer_id` 一次性 INSERT 为 `space_member.device_id`,后接 `2026-04-20-000001_drop_paired_device` 把旧表整张删掉
- `2026-04-20-000002_create_peer_address` 用 `device_id` 作主键,无 peer_id

**migration 历史不可变**:旧 migration up.sql 里仍有 `peer_id` 字面量,但那是历史现场,不是当前 schema。append-only 约束意味着这些文件本身**不删**,但不影响 Slice 4 的运行时。

---

### F-114 · PresencePort 替代 NetworkEventPort 的可行性(已坐实)

**`PresencePort` 已具备承接事件流的全部能力**:
```rust
// uc-core/src/ports/presence.rs
fn subscribe(&self) -> broadcast::Receiver<PresenceEvent>;
// PresenceEvent { device_id, state: Online|Offline|Unknown, at }
```

**`PeerMonitor` 当前(`uc-daemon/src/peers/monitor.rs`)转发的 5 种 NetworkEvent 映射**:

| 旧 NetworkEvent | iroh 路径下状态 | PresenceEvent 等效 | 处置 |
|---|---|---|---|
| `PeerConnected` | iroh dial 成功后触发 | `state: Online` | ✅ 1:1 替换 |
| `PeerDisconnected` | iroh 连接关闭后触发 | `state: Offline` | ✅ 1:1 替换 |
| `PeerDiscovered` | mDNS 已从 v1 移除(T-04 标),**永不触发** | 无 | ❌ 删除事件类型 + ws 投影 |
| `PeerLost` | 同上,永不触发 | 无 | ❌ 删除 |
| `PeerNameUpdated` | T-08 标 v1 不做主动 rename 广播,改名走 C1 header 被动传播——事件源**永不触发** | 无 | ❌ 删除 |

**`PeerDiscoveryWorker`(`uc-daemon/src/workers/peer_discovery.rs`)处置**:
- 当前逻辑:订阅 `PeerDiscovered`,收到后调用 `peer_directory.announce_device_name`
- iroh 路径下 `PeerDiscovered` 永不触发 → **整 worker 删除**,从 daemon service registry 移除

**WebSocket 投影 schema 影响**:daemon 删 `PeerNameUpdatedPayload` / `PeersChangedFullPayload` 中"是否包含已发现未配对设备"的相关字段(若有)。前端订阅 `peers_changed` topic 的代码需配合检查——这部分由前端工作处理。

**`uc-daemon` 当前 `PresencePort` 消费情况**:
- `rg -ln "PresencePort\b" src-tauri/crates/uc-daemon/src/` **0 命中** —— daemon 当前**还没**接 PresencePort 事件流
- 接入点应在 `DaemonPairingHost` 创建 `PeerMonitor` 的位置同样创建一个新的 `PresenceMonitor` service,持有 `Arc<dyn PresencePort>` + ws event_tx

**Slice 4 子任务定义**:
1. 在 `uc-daemon/src/peers/` 新增 `presence_monitor.rs`,实现 `DaemonService` trait,内部 `port.subscribe()` 拿 receiver,转发为 `peer_connection_changed` ws event
2. 删除 `peer_discovery.rs` worker 整文件 + service 注册点
3. 删除 `monitor.rs`(整文件——逻辑搬到 `presence_monitor.rs`)
4. 清理 `DaemonPairingHost` / runtime 装配链中对旧 worker 的引用

---

### F-115 · 死代码补遗(file_sync helpers)

`rg -ln "CleanupExpiredFilesUseCase|copy_file_to_clipboard::|CopyFileToClipboard" src-tauri/crates/` **0 命中**。

**结论**:
- `uc-app/src/usecases/file_sync/cleanup.rs`(`CleanupExpiredFilesUseCase` / `check_device_quota` / `QuotaExceededError`)
- `uc-app/src/usecases/file_sync/copy_file_to_clipboard.rs`

均无外部消费者,加入 F-111 整体删除清单。同时 `uc-app/src/usecases/file_sync/mod.rs` 的所有 mod 声明 + re-export 全清空 —— 整个 `file_sync/` 子目录可整体 `rm -rf`。

---

### F-116 · P3-pre 决策研究:`SpaceAccessNetworkAdapter` 是 dead leg,推荐方案 D(整套删除)

**发现时间**:2026-04-24(Slice 4 Phase 3 P3-pre 决策研究)
**触发**:Phase 2 B 重新评估暴露 PairingSessionMessage 没有 envelope 变体,需要决定 iroh-side space-access 协议怎么走

**核心证据**:iroh 路径已经完整覆盖 space-access 协议三段,**根本不依赖 SpaceAccessNetworkAdapter / PairingTransportPort / PairingMessage::Busy envelope**:

| 旧路径(libp2p,经 Busy envelope) | 新路径(iroh,经 PairingSessionMessage 专用变体) | 字节级一致 |
|---|---|---|
| `space_access_offer { space_id, nonce, keyslot }` envelope on `PairingMessage::Busy` | `PairingSessionMessage::KeyslotOffer { space_id, keyslot_blob, challenge, pairing_session_id }` 由 `sponsor_handshake.rs:222` 直接发送 | ✅ keyslot 仍为 JSON Value,challenge 是 32B nonce |
| `space_access_proof { proof_bytes, challenge_nonce, ... }` envelope | `PairingSessionMessage::ChallengeResponse { encrypted_challenge }` 由 `joiner_handshake.rs:247` 发送,**字段名变 `encrypted_challenge` 但字节直接来自 `proof.proof_bytes`**;sponsor 侧 `sponsor_handshake.rs:321` 把收到的 `encrypted_challenge` 当 `proof_bytes` 喂 `verify_proof` | ✅ |
| `space_access_result { space_id, success, deny_reason }` envelope | 成功侧 `PairingSessionMessage::Confirm { space_id, sender_device_id, sender_device_name, sender_identity_fingerprint, transport_address_blob }`(sponsor_handshake.rs:385);失败侧 `PairingSessionMessage::Reject { reason: PairingRejectReason::* }`(sponsor_handshake.rs:436) | ✅ Confirm 多带 sender 身份 + 传输地址(Slice 2 Phase 1 T5 强化);deny_reason 由 PairingRejectReason 枚举接管 |

**进一步证据**:
- `sponsor_handshake.rs:11–21` 顶部注释明文:"Sponsor path is linear ... running it through `SpaceAccessStateMachine` gives us enum ceremony without extra correctness guarantees, and the FSM's action order for the verified branch (`SendResult` → `PersistSponsorAccess`) is **inverted** from the ordering Slice 1 wants" —— 即 iroh sponsor 侧 **明文绕开** FSM
- F-052 已经标记 sponsor 不走 `SpaceAccessStateMachine`(P7f cleanup)
- `findings.md:1077`(F-091 附近)记录 joiner FSM 在 Slice 1 也不被 active 路径触发,joiner_handshake.rs 直接驱动

**`SpaceAccessNetworkAdapter` 残留消费者(全部是 FSM transport leg)**:
- `uc-application/src/space_access/orchestrator.rs:445/514/519` —— `SendOffer/SendProof/SendResult` action 触发
- `uc-core/src/space_access/state_machine.rs:56/108/126/144/352` —— FSM 状态转移生成这三个 action
- `uc-core/src/ports/space/transport.rs` —— port 定义本体,**唯一**的 application impl 是 `SpaceAccessNetworkAdapter`

**A1 路径风险点**:`uc-application/src/space_access/initialize_new_space.rs:74` 调 `start_sponsor_authorization` 会触发 FSM 进入 `SponsorAuthorizationRequested` 转移(state_machine.rs:42–66),生成 `[RequestOfferPreparation, SendOffer, StartTimer]`。但 A1 是用户首次设密的**单机**动作,没有 joiner,SendOffer 在 iroh-only 配置下走 `SpaceAccessNetworkAdapter` → `DisabledPairingTransport`(`uc-platform/src/adapters/network.rs:32`)会**直接报错**"local pairing runtime is disabled"。

> 这意味着两种可能:(1) 当前 e2e 跑 A1 走的是 libp2p stack(`PairingRuntimeOwner::CurrentProcess` → `Libp2pNetworkAdapter` impl),iroh-only 配置下 A1 实际未被验证;(2) 调用方提前短路或注入 fake adapter 跳过这条 leg。Phase 3 实施时**必须先核实 A1 路径在 iroh-only 配置下的实际行为**——这是删除工作的入口风险。

**推荐方案 D · 删除整条 `SpaceAccessTransportPort` 套件**(替代之前列的 A/B):

P3-pre 实施清单:
1. 删 `uc-application/src/space_access/network_adapter.rs`(整文件)
2. 删 `uc-core/src/ports/space/transport.rs`(整文件)
3. 删 `SpaceAccessAction` enum 三个 transport 变体:`SendOffer` / `SendProof` / `SendResult`(uc-core/src/space_access/action.rs)
4. 改 `state_machine.rs`:转移序列里去掉这三个 action(state_machine.rs:56/108/126/144/352)——FSM 状态机本身保留(initialize_new_space 仍用)
5. 改 `SpaceAccessExecutor`:删 `transport: &mut dyn SpaceAccessTransportPort` 字段(executor.rs:7)
6. 改 `SpaceAccessOrchestrator::execute_actions`:删 `SendOffer/SendProof/SendResult` 分支(orchestrator.rs:442–520)
7. 改 setup `{orchestrator,action_executor,facade,testing}.rs`:移除 `transport_port: Arc<TokioMutex<dyn SpaceAccessTransportPort>>` 参数链
8. 改 `bootstrap/assembly.rs:1189–1194`:删 `transport_port` 装配 + `SpaceAccessNetworkAdapter::new` 调用
9. 删 `daemon/pairing/host.rs:1436–1530+` 的 `PairingMessage::Busy` envelope 解析段(随 libp2p adapter 整体删除一并清理,Phase 3 主体阶段)
10. 删 `parse_space_access_busy_payload` / `SpaceAccessBusyPayload` 等 helper(位置待 grep,主体在 daemon 侧)

**优势**:
- 删除而非替换,工作量比方案 A/B 小一个量级
- 不需要新增 wire 类型 / 新 port / 新 codec
- iroh adapter / sponsor_handshake / joiner_handshake 完全无需改动
- F-103 修正:`PairingMessage` / `PairingBusy` wire 类型可一并删除(原 task_plan.md:1180 "保 PairingBusy" 自动失效)

**不确定项(进入 Phase 3 时必查)**:
- A1 路径在 iroh-only 配置下的当前行为(上面已标)
- `SpaceAccessOrchestrator::dispatch` 在 joiner 侧是否被 setup orchestrator 直接调用——若是,需要核对 joiner_handshake.rs 是否已替代该路径
- `SpaceAccessExecutor` 在 setup 流程里其他字段是否仍需要(timer / store / proof)——这些应该不删

**结论**:P3-pre 推荐方案 D。task_plan.md 中关于 P3-pre 的"方案 A/B 二选一"决策记录可改写为"方案 D 已选定,等待 A1 路径行为验证"。

---
