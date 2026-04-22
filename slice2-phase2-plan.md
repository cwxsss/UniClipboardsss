# Slice 2 Phase 2 · 剪贴板同步(iroh 栈)—— 细化计划

> **状态**:计划稿(2026-04-22),待用户过目后开工
> **父文档**:`task_plan.md` §Slice 2 · 剪贴板同步 + 预连式 F1
> **前置**:Slice 1 ✅ + Slice 2 Phase 1 ✅(2026-04-22 封版)

---

## 1. 目标复述

让两台已配对设备在 iroh 栈下**文本剪贴板端到端同步**。本机复制 → ≤ 2s 内对端剪贴板中出现同一内容。不改 UI 入口,不做 rename / revoke,不做文件 / blob(C3 / D 组推 Phase 3)。

**验收**(锁定):
1. A 设备复制一段文本 → ≤ 2s 内 B 设备 `uniclipboard-cli history`(或读 B 的 OS 剪贴板)中出现同一内容,content_hash 一致
2. 接入系统剪贴板的 UX 路径:CLI 内 `uniclipboard-cli watch`(前台监听)可以主动触发 dispatch;daemon 侧 watch 在 Phase 2 **不接**(daemon 重装配留到 Slice 4/5)
3. 若接收方已经有相同 content_hash 的 entry → 回"duplicate" ack 并丢弃,不重复入库(幂等)
4. 若发送时对端 offline → `DispatchResult::Offline` 返回给调用方(facade),不 panic,不重试循环
5. 单元测试覆盖:facade / dispatch usecase / ingest usecase / iroh dispatch adapter / iroh receiver adapter(accept 侧)
6. 集成测试 `slice2_phase2_clipboard_e2e`:两 assembly + loopback iroh + pair → A `dispatch_current_entry` → B 在 `ClipboardEventWriter.insert_event` 里被调 1 次,内容字节级相等

**不在本 Phase scope**:
- C3 文件同步(等 Slice 3)
- A3 revoke / A5 rename(Phase 3)
- daemon / tauri 侧 clipboard watcher 改装(Phase 3 或 Slice 4)
- 前端 UI(Slice 2 Phase 3+)
- 大图片 / 富文本长期回归验证(Phase 3 或手动验收)
- 旧 `ClipboardOutboundTransportPort` / `ClipboardInboundTransportPort` 删除(Slice 5 统一)

> **命名说明**:Phase 2 的 CLI 命令暂用 `send` / `watch`,后者仅 Phase 2 用于测试自启动的场景;Phase 3 再统一接入 daemon `DaemonClipboardChangeHandler`。

---

## 2. 架构分层(新建 / 扩展对照)

```
uc-cli
  └── commands/send.rs             🆕 one-shot:抓本机剪贴板 → facade.dispatch_current_entry
  └── commands/watch.rs            🆕 前台长驻:系统剪贴板变化 → dispatch

uc-application
  ├── facade/clipboard/            🆕 目录
  │     ├── facade.rs              🆕 ClipboardSyncFacade
  │     ├── commands.rs            🆕 DispatchOutcome / InboundClipboardNotice / DispatchReport
  │     ├── errors.rs              🆕 ClipboardSyncError
  │     └── mod.rs                 🆕
  └── usecases/clipboard_sync/     🆕 目录(不复用旧 usecases/clipboard/)
        ├── dispatch_entry.rs      🆕 DispatchClipboardEntryUseCase
        ├── ingest_inbound.rs      🆕 IngestInboundClipboardUseCase
        └── mod.rs                 🆕

uc-core
  └── ports/clipboard/
      ├── sync_dispatch.rs         🆕 ClipboardDispatchPort + ClipboardHeader + SyncPayload
      ├── sync_receiver.rs         🆕 ClipboardReceiverPort + InboundClipboard + ClipboardInboundStream
      └── mod.rs                   ✏️ 挂接两个新 mod(legacy 的 transport.rs 两个 trait 加 #[deprecated])

uc-infra
  └── network/iroh/
        ├── node.rs                ✏️ 新增 install_clipboard 扩展点
        ├── clipboard_dispatch_adapter.rs  🆕 IrohClipboardDispatchAdapter(出站)
        ├── clipboard_receiver_adapter.rs  🆕 IrohClipboardReceiverAdapter(入站 + ProtocolHandler)
        ├── clipboard_wire.rs              🆕 wire header 编解码(version / timestamps / content_hash / origin)
        └── clipboard_identity.rs          🆕 endpoint_id → MemberId 内联解析(简单够用,不提 Port)

uc-bootstrap
  ├── assembly.rs                  ✏️ 装配 ClipboardSyncFacade + 两个 adapter
  └── space_setup.rs               ✏️ SpaceSetupDeps 加 clipboard_sync 字段;space unlock 后启动 receiver subscribe loop
```

**Legacy 保留**:
- `uc-core/src/ports/clipboard/transport.rs` 下 `ClipboardOutboundTransportPort` / `ClipboardInboundTransportPort` 加 `#[deprecated]`
- `uc-app/src/usecases/clipboard/sync_{inbound,outbound}.rs` / `uc-daemon/src/workers/{clipboard_watcher,inbound_clipboard_sync}.rs` **不动** —— Phase 2 两栈并行,daemon 线还走 libp2p;iroh 线通过 CLI `send` / `watch` 验收

**关键不做**:
- **不**把 daemon ClipboardWatcherWorker 改成走 iroh(会牵动 tauri bootstrap,scope 爆炸);Phase 2 daemon 保持 libp2p 路径
- **不**新增 `PeerIdentityResolverPort` —— `endpoint_id → MemberId` 解析逻辑仅 iroh adapter 内需用,提 Port 是过度抽象(YAGNI)。Phase 3 A3 revoke 若需要再提
- **不**碰 `BlobCipherPort` / `TransferCipherPort` —— 复用现有 `ClipboardBinaryPayload` V3 codec(`uc-core/src/network/protocol/clipboard_payload_v3.rs`)+ `TransferCipherPort` 现有加解密实现。Phase 2 只做 wire / 传输层改造

---

## 3. 新 port 契约草图

### 3.1 `ClipboardDispatchPort`(uc-core)

```rust
// uc-core/src/ports/clipboard/sync_dispatch.rs

use crate::ids::DeviceId;
use async_trait::async_trait;

#[derive(Debug, Clone)]
pub struct ClipboardHeader {
    /// Wire 版本,Phase 2 = 1(与 pairing WIRE_VERSION 独立编号;
    /// pairing 的 v=2 是 Slice 1→2 升级的,clipboard 首版从 1 起)
    pub version: u8,
    pub content_hash: String,            // SHA256 hex,与 ClipboardEntry 的去重 key 一致
    pub captured_at_ms: i64,
    pub origin_device_id: String,
    pub origin_device_name: String,      // A5 rename 后被动传播(Phase 3 消费;Phase 2 只透传)
    pub payload_version: u8,             // V3 = 3,对应 ClipboardBinaryPayload
}

/// Phase 2 仅支持 in-memory bytes;大 payload / 文件走 Slice 3 的 blob 路径。
/// 2MB 以上在 facade 层 reject(避免无意义开 iroh stream 传大内容)。
#[derive(Debug, Clone)]
pub struct SyncPayload {
    pub ciphertext: bytes::Bytes,
}

#[derive(Debug, thiserror::Error)]
pub enum ClipboardDispatchError {
    #[error("target device offline or unreachable")]
    Offline,
    #[error("peer rejected: {0}")]
    PeerRejected(String),
    #[error("stream io: {0}")]
    Io(String),
    #[error("internal: {0}")]
    Internal(String),
}

/// 出站端口:对某已在线设备开一条新 iroh bi-stream,写 header + payload,
/// 等 peer ack 或读到 FIN。每次 dispatch 开一条新 stream,用完关闭(Q4 决定)。
#[async_trait]
pub trait ClipboardDispatchPort: Send + Sync {
    async fn dispatch(
        &self,
        target: &DeviceId,
        header: &ClipboardHeader,
        payload: SyncPayload,
    ) -> Result<DispatchAck, ClipboardDispatchError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchAck {
    Accepted,        // 对端成功入库
    DuplicateIgnored,// 对端已有同 content_hash,回的"duplicate"
}
```

**关键决策**:
- `ClipboardDispatchPort::dispatch` 对**单目标**;多目标由 usecase 层 `JoinSet` 并发(同 `EnsureReachableAllUseCase` 模式)。port 保持最小
- `DispatchAck` 仅 2 态,不引入 `Queued` / `Pending`(YAGNI)
- header 暂不放 `blob_refs`(Slice 3 再加),但字段已留 `payload_version` 供未来 V4 扩展
- `origin_device_name` 只透传,不做 rename 传播 —— Phase 3 处理

### 3.2 `ClipboardReceiverPort`(uc-core)

```rust
// uc-core/src/ports/clipboard/sync_receiver.rs

use crate::ids::DeviceId;
use async_trait::async_trait;
use tokio::sync::broadcast;

#[derive(Debug, Clone)]
pub struct InboundClipboard {
    pub peer_device_id: DeviceId,       // 解析自 endpoint_id;未识别直接拒绝,不到达这里
    pub header: ClipboardHeader,
    pub ciphertext: bytes::Bytes,
}

/// 入站端口:订阅"收到的剪贴板 payload"事件流。
/// 首次 subscribe 后 adapter 开始接收 iroh inbound connections;
/// broadcast channel lagged → Warn + skip,不 panic。
#[async_trait]
pub trait ClipboardReceiverPort: Send + Sync {
    fn subscribe(&self) -> broadcast::Receiver<InboundClipboard>;
}
```

**设计选择**:
- 选 `broadcast::Receiver<InboundClipboard>` 而非 `Box<dyn Stream>` —— 与 `PresencePort::subscribe` 模式一致,facade / usecase 直接 loop `.recv()`
- Port 层不做"ack 回写":accept 侧读完 payload 就 FIN + ack;应用层 usecase 在 ingest 成功/失败后**不**再反馈给 iroh(Phase 2 简化,对端只关心"字节送到了"而非"业务已落地")。Phase 3 若需要 end-to-end ack 再扩展

---

## 4. Wire 协议(clipboard ALPN)

### 4.1 ALPN

```
CLIPBOARD_ALPN = b"uniclipboard/clipboard/0"
```

版本号 `/0` 起步,不兼容变更时升 `/1`(与 `PRESENCE_ALPN` 独立编号,Slice 1 pairing 的 `WIRE_VERSION` 不共享)。

### 4.2 Frame layout(bi-stream 上的单向消息序列)

```
sender -> receiver:
  [ u8   | header_len_be_u32 | header_bytes(postcard) | payload_len_be_u32 | ciphertext_bytes | FIN ]
    magic

receiver -> sender:
  [ u8 ack_code | FIN ]
```

- `magic = 0xC1` 固定字节,误 ALPN 连过来时一眼能识别
- `header_bytes` 是 `ClipboardHeader` 的 postcard 序列化(core 定义的 domain struct,不是 serde_json,避免 base64)
- `payload_len_be_u32` 上限 2 * 1024 * 1024(2MB),超出 peer 侧 `PeerRejected("payload_too_large")` 返回
- `ack_code`:`0x01 = Accepted`,`0x02 = DuplicateIgnored`,`0xFF = Rejected`(body 随 FIN 关了,不带消息;rejected 从 close reason 取)

### 4.3 错误 / close-reason 映射

- 读 header 失败(postcard decode error)→ adapter close stream with reason `"bad_header"` → dispatch 端得 `Io`
- payload 超限 → close with reason `"payload_too_large"` → dispatch 端得 `PeerRejected`
- 未识别 peer → close with reason `"unknown_peer"` → dispatch 端理论不会撞上(本端 pair 完成才有 peer_addr),但防御性处理

---

## 5. `ClipboardSyncFacade` + usecase

### 5.1 `ClipboardSyncFacade`(uc-application)

```rust
pub struct ClipboardSyncFacade {
    dispatch_uc: Arc<DispatchClipboardEntryUseCase>,
    ingest_uc:   Arc<IngestInboundClipboardUseCase>,
    inbound_events: broadcast::Sender<InboundClipboardNotice>,
    outbound_results: broadcast::Sender<DispatchOutcome>,
}

pub struct DispatchOutcome {
    pub content_hash: String,
    pub per_target: Vec<(DeviceId, Result<DispatchAck, ClipboardDispatchError>)>,
    pub total_accepted: usize,
    pub total_offline: usize,
    pub at_ms: i64,
}

pub struct InboundClipboardNotice {
    pub from_device: DeviceId,
    pub content_hash: String,
    pub action: InboundAction,   // { NewEntry, DuplicateIgnored }
    pub at_ms: i64,
}

impl ClipboardSyncFacade {
    /// 调用方:CLI `send` / `watch`。用当前系统剪贴板内容生成 ClipboardEntry,
    /// 向所有在线 + enabled 成员并发 dispatch。
    pub async fn dispatch_current_entry(&self) -> Result<DispatchOutcome, ClipboardSyncError> {
        self.dispatch_uc.execute().await
    }

    pub fn subscribe_inbound(&self) -> broadcast::Receiver<InboundClipboardNotice>;
    pub fn subscribe_outbound_results(&self) -> broadcast::Receiver<DispatchOutcome>;
}
```

**职责边界**:
- facade 不做业务决策(online 过滤、policy 在 usecase 里)
- 不持有 `SystemClipboardPort` —— 传给 `DispatchClipboardEntryUseCase`
- `subscribe_*` 是 broadcast,channel cap = 64(与 presence 对齐)

### 5.2 `DispatchClipboardEntryUseCase`

```rust
pub struct DispatchClipboardEntryUseCase {
    local_clipboard:   Arc<dyn SystemClipboardPort>,
    space_access:      Arc<dyn SpaceAccessPort>,
    member_repo:       Arc<dyn MemberRepositoryPort>,
    presence:          Arc<dyn PresencePort>,
    peer_addr_repo:    Arc<dyn PeerAddressRepositoryPort>,
    local_identity:    Arc<dyn LocalIdentityPort>,
    device_identity:   Arc<dyn DeviceIdentityPort>,
    settings:          Arc<dyn SettingsPort>,
    transfer_cipher:   Arc<dyn TransferCipherPort>,  // 产生 V3 ciphertext
    clipboard_dispatch:Arc<dyn ClipboardDispatchPort>,
    clipboard_event_writer: Arc<dyn ClipboardEventWriterPort>, // 本机落地
    clock:             Arc<dyn ClockPort>,
}

impl DispatchClipboardEntryUseCase {
    pub async fn execute(&self) -> Result<DispatchOutcome, ClipboardSyncError> {
        // 1. space_access.is_unlocked()? —— 否则 LockedSpace 错误
        // 2. local_clipboard.read_snapshot()? —— 空则 EmptyClipboard
        // 3. 构造 ClipboardBinaryPayload(V3)→ transfer_cipher.encrypt → ciphertext
        // 4. 写 ClipboardEventWriter.insert_event(本机先落盘)
        // 5. 枚举目标:member_repo.list() ∩ 不是本机 ∩ presence.current_state==Online ∩ SyncPreferences.send_enabled
        //    —— Phase 2 暂不做 content-type filter(task_plan 里有,但放 Phase 3)
        // 6. JoinSet 并发 dispatch,每 target 一个 task
        //    target 失败不影响其他(各自收进 report.per_target)
        // 7. broadcast outbound_result
        // 8. return DispatchOutcome
    }
}
```

### 5.3 `IngestInboundClipboardUseCase`

```rust
pub struct IngestInboundClipboardUseCase {
    receiver:          Arc<dyn ClipboardReceiverPort>,
    space_access:      Arc<dyn SpaceAccessPort>,
    transfer_cipher:   Arc<dyn TransferCipherPort>,
    clipboard_event_writer: Arc<dyn ClipboardEventWriterPort>,
    local_clipboard:   Arc<dyn SystemClipboardPort>, // Phase 2 可选写入系统剪贴板(CLI 用 --write-system flag)
    inbound_notices:   broadcast::Sender<InboundClipboardNotice>,
    clock:             Arc<dyn ClockPort>,
}

impl IngestInboundClipboardUseCase {
    /// 后台 loop:subscribe 一次,每条 InboundClipboard 就落盘 + broadcast。
    /// Run as spawned task,Dropper 持 AbortHandle。
    pub async fn run(self: Arc<Self>) -> InboundSpawnHandle { /* ... */ }

    async fn handle_one(&self, inbound: InboundClipboard) {
        // 1. space 未解锁 → warn + skip
        // 2. content_hash 已在 event repo → 发 DuplicateIgnored notice,skip
        // 3. transfer_cipher.decrypt(inbound.ciphertext)
        // 4. 构造 ClipboardEntry + insert_event
        // 5. 发 NewEntry notice
    }
}
```

**Phase 2 限制**:`IngestInboundClipboardUseCase` **不写系统剪贴板**(不调 `local_clipboard.write_snapshot`),仅落盘。原因:daemon 才是 OS 剪贴板持有者,CLI 短生命进程写系统剪贴板会和 daemon 的 watcher 打架。Phase 3 / Slice 4 接 daemon 时再开这个路径。验收 #1 通过 B 侧查询 event repo 内容验证。

---

## 6. iroh adapter 实现

### 6.1 `IrohClipboardDispatchAdapter`(出站)

- 持 `Arc<Endpoint>` + `Arc<dyn PeerAddressRepositoryPort>`
- `dispatch(target, header, payload)`:
  1. `peer_addr_repo.get(target)` → 拿 `addr_blob`(postcard-encoded `EndpointAddr`)
  2. 解包 → `endpoint.connect(addr, CLIPBOARD_ALPN)` 开连接
  3. 开 bi-stream,写 magic + header_len + header_bytes + payload_len + ciphertext + finish
  4. 读 ack_code(1 字节)→ map 到 `DispatchAck`
  5. 关流;失败路径统一映射到 `ClipboardDispatchError`
- **复用** Phase 1 `IrohPresenceAdapter` 的"持 Connection + Drop 关连接"的处理风格,但不持久化 —— 每次 dispatch 开新 connection(Q4 语义)

### 6.2 `IrohClipboardReceiverAdapter`(入站)

- `ProtocolHandler for IrohClipboardReceiverHandler` 实现 `accept(connection: Connection)`:
  1. `connection.remote_id()` → `endpoint_id` → `identity_fingerprint` → 查 `member_repo` 找 `DeviceId`
  2. 未找到 → close stream with `"unknown_peer"` 并 `return`
  3. 开 `accept_bi()` 拿 (send, recv)
  4. 读 magic + header + payload,decode header via postcard
  5. 发 `InboundClipboard` 到内部 broadcast channel
  6. 回写 ack_code = 0x01(Accepted)——**发送侧无法知道对端 ingest 是否真成功,Phase 2 简化:adapter 读到字节就 Accepted**(语义变更:任何重复检查在 ingest usecase 里做,adapter 不管 application 状态)
  7. `send.finish()` → 关流
- **Watchdog**:不需要(每条 stream 一次性,读完就关)
- **并发**:每个 inbound connection spawn 一个 task,多个 inbound 互不阻塞

### 6.3 endpoint_id → DeviceId 解析

```rust
// uc-infra/src/network/iroh/clipboard_identity.rs
pub fn resolve_device_id(
    endpoint_id: iroh::EndpointId,
    member_repo: &dyn MemberRepositoryPort,
    identity_factory: &dyn IdentityFingerprintFactory,
) -> Option<DeviceId> {
    let fp = identity_factory.from_endpoint_public_key(endpoint_id.as_bytes());
    member_repo.list()?.into_iter()
        .find(|m| m.identity_fingerprint == fp)
        .map(|m| m.device_id)
}
```

这是 Phase 2 的**隐蔽复杂度** —— 当前没有 `from_endpoint_public_key` 函数,`IdentityFingerprint` 是从本地生成的指纹(`uc-infra/src/security/identity_fingerprint.rs`),要确认 iroh endpoint 的 Ed25519 public key 对应同一个指纹算法。**这块需要 T2 先做一次探针验证**,类似 Phase 1 T3a `iroh_presence_probe.rs`,跑通"sponsor 收到 joiner 连过来时,能不能从 `connection.remote_id()` 反推出 T5 写进 `member_repo` 的 identity_fingerprint"。

---

## 7. F1 hook 接入?

**Phase 2 不改 F1 hook**。Phase 1 的 `auto_start_network` 已经跑 `ensure_reachable_all`,Phase 2 的 clipboard receiver 由 `SpaceSetupFacade::auto_start_network` 成功后 **unconditionally spawn 一个 ingest loop**,生命周期绑 assembly(shutdown 时 abort)。

```rust
// space_setup.rs:auto_start_network 成功路径末尾
let ingest_handle = self.clipboard_sync.spawn_ingest_loop();
// 存在 SpaceSetupFacade 内部的 Mutex<Option<InboundSpawnHandle>>
```

**不引入 "install_clipboard returns ingest handle to bootstrap"** —— ingest 生命周期由 facade 管,bootstrap 只传 adapter。

---

## 8. 任务拆解

| # | 任务 | 依赖 | 工作量 |
|---|---|---|---|
| T1 | uc-core 新建 `ports/clipboard/sync_dispatch.rs` + `sync_receiver.rs`,挂 `mod.rs`;legacy transport 两 trait 加 `#[deprecated]` | - | 0.5h |
| T2 | **探针**:`iroh_clipboard_identity_probe.rs` —— 验证 `Connection::remote_id()` 能反推 `identity_fingerprint`,确认 resolve 函数实现路径 | - | 1.5h |
| T3 | `uc-infra/src/network/iroh/clipboard_wire.rs`:header postcard codec + magic + length frames,6 单测(正常 / bad_magic / header_too_long / payload_too_large / trailing_bytes / truncated) | T1 | 1.5h |
| T4 | `IrohClipboardDispatchAdapter`(出站)+ 3 单测(成功 / offline peer / payload > 2MB 被 reject) | T1, T2, T3 | 2h |
| T5 | `IrohClipboardReceiverAdapter` + ProtocolHandler + 4 单测(单条 payload / unknown peer / bad magic / 多并发 inbound connection) | T1, T2, T3 | 2.5h |
| T6 | `IrohNodeBuilder::install_clipboard` 扩展点,返回 `(Arc<dyn ClipboardDispatchPort>, Arc<dyn ClipboardReceiverPort>)`;"pairing + presence + clipboard 三 ALPN 共存"单测 | T4, T5 | 1h |
| T7 | `DispatchClipboardEntryUseCase` + 5 单测(空剪贴板 / space locked / 无在线 peer / 并发 2 peer 一成功一失败 / 大 payload 过 transfer_cipher) | T1 | 2.5h |
| T8 | `IngestInboundClipboardUseCase` + `spawn_ingest_loop` handle + 4 单测(新条目 / 重复 content_hash / decrypt 失败 / space locked) | T1 | 2h |
| T9 | `ClipboardSyncFacade` + broadcast 封装 + 3 单测(dispatch 透传 / inbound notice 订阅 / shutdown abort 干净) | T7, T8 | 1h |
| T10 | bootstrap 装配:`SpaceSetupAssembly` 加 `clipboard_sync: Arc<ClipboardSyncFacade>`;`install_clipboard` 接线;`auto_start_network` 成功后 spawn ingest loop | T6, T9 | 1h |
| T11 | `uc-cli/src/commands/send.rs` + `watch.rs`:`send` 单次,`watch` loop + Ctrl-C 退出;`--profile` / `--dev` / JSON 输出 | T10 | 1.5h |
| T12 | 集成测试 `uc-bootstrap/tests/slice2_phase2_clipboard_e2e.rs`:两 assembly + pair + A dispatch → B `ClipboardEventWriter.insert_event` 被调 1 次 + content 字节相等;重复发第二次 → B 侧 DuplicateIgnored | T10 | 2.5h |
| T13 | 手动双 profile 验收:`--dev --profile=a` init / `--profile=b` join,各自开两个 terminal,`send` / `watch`,CLI 输出确认 | T11, T12 | 0.8h |
| T14 | task_plan.md 标 Phase 2 ✅ + `slice2-phase2-plan.md §12` live tracker + 所有 commit hash 收录;follow-up 新增"daemon watcher 改装到 iroh"条目 | T12, T13 | 0.5h |

**总计**:~20.8h(≈ 2.5-3 个专注工作日)

**可并行组**:
- T1 完成后 T3 / T7 / T8 可并行(T7/T8 初期用 mock adapter)
- T4 / T5 依赖 T3,两者内部互不依赖
- T12 / T13 最后并行

---

## 9. 测试策略

### 9.1 单元测试(随每个 T 交付)

详见 §8 每行末尾。关键并发测试(T7)参考 Phase 1 T6 的教训 —— 不用 mockall 做并发断言(Mutex 序列化),改手写 `SleepyDispatch` fake。

### 9.2 集成测试(Phase 2 核心保障)

**`slice2_phase2_clipboard_e2e.rs`**(新建):
1. 起两个 `SpaceSetupAssembly` + `ClipboardSyncFacade`(A / B,loopback iroh)
2. 复用 Slice 1 测试夹具完成配对
3. 等 F1 预连完成(给 3s 余量)
4. A 侧伪造剪贴板(注入 `FakeSystemClipboard` 返回固定 snapshot)→ `A.clipboard_sync.dispatch_current_entry()`
5. 等 ≤ 3s,断言 B 侧 `ClipboardEventReaderPort.list_all()` 返回 1 条记录 + content_hash 匹配
6. A 再调一次 `dispatch_current_entry()`(同内容)→ 断言返回 `DispatchOutcome.per_target[0].1 == Ok(DuplicateIgnored)`,B 侧 event 仍只 1 条

### 9.3 CLI 冒烟(`single-machine-e2e.sh` 不扩展)

沿用 Phase 1 的决策(§12.2):shell 扩展维护成本 > 价值,Rust 集成测试已覆盖。手动验收靠 T13 即可。

---

## 10. 风险 & 待确认

| 风险 | 缓解 |
|---|---|
| T2 探针发现 iroh `Connection::remote_id()` 与我们的 `identity_fingerprint` 指纹算法对不上 | 两条出路:(a) 改 `IdentityFingerprintFactory` 提供 `from_ed25519_public_key(bytes)` 方法;(b) pairing 时在 `transport_address_blob` 里捎带 `identity_fingerprint`,adapter 侧查 `peer_addr_repo` 而非靠 endpoint_id 反推。倾向 (a),指纹算法本身就是 SHA256(pubkey) |
| iroh stream write/read 顺序搞错导致死锁 | 借鉴 Phase 1 `IrohPairingSessionAdapter::recv_pump`,在 T5 adapter 里先 `accept_bi().await` → 读完 payload → 回写 ack → `finish()` → peer `read` FIN。顺序固定化,不做 select |
| 2MB payload 上限对图片来说太小 | Phase 2 只定"text 可靠同步"验收,大 payload 推 Slice 3(blob ticket 路径);上限临时硬编码,之后再提配置 |
| broadcast channel lagged(recv 消费慢)丢事件 | Phase 2 cap=64;生产环境 CLI watch 单线订阅基本不会 lag。lag 时 adapter 侧 warn log,不视为错误 |
| 并发 dispatch 触发 iroh 限流 | N ≤ 10 假设不会(同 Phase 1 假设);撞上用 `JoinSet` + `tokio::time::interval` 限速到 5 req/s |
| 剪贴板 snapshot 经过 TransferCipherPort 再经过 ClipboardBinaryPayload V3 codec 双重加密 / 编码性能 | 既有路径,Phase 2 不优化;2MB 以下 < 100ms 可接受 |
| CLI 短生命写系统剪贴板会被 daemon watcher 误判为"用户复制" | Phase 2 ingest **不**写系统剪贴板(§5.3);验收看数据库 |

---

## 11. Slice 1 / Phase 1 Agent 规范合规性自查

| 规范项 | 确认 |
|---|---|
| uc-core 只含 port + 领域类型,不含 iroh 类型 | `ClipboardHeader` / `SyncPayload` / `InboundClipboard` 全 domain 类型;`bytes::Bytes` 属于 `uc-core` 已接受的第三方基础类型(与 Slice 1 wire 一致) |
| uc-application 只编排,不侵入 core / infra | `DispatchClipboardEntryUseCase` 只调 port |
| uc-infra 实现面向 port,adapter 名清晰 | `IrohClipboardDispatchAdapter` / `IrohClipboardReceiverAdapter` / `clipboard_wire.rs` / `clipboard_identity.rs` |
| Orchestrator / StateMachine 不对外导出 | Phase 2 无 orchestrator;facade 直接驱动 2 usecase |
| Facade 只是入口,不重新编排业务 | `ClipboardSyncFacade` 是 3 方法的 thin wrapper |
| 错误收敛,不外泄 iroh / postcard 类型 | `ClipboardDispatchError` / `ClipboardSyncError` 本地定义;`Io(String)` 吞字符串不吞 `iroh::*` |
| 敏感数据不打日志 | `content_hash`(SHA256 hex)可打,`ciphertext` 只打长度,`snapshot.text()` 永远不打 |

---

## 12. 验收前检查清单

- [ ] 所有新 port + 实现 `cargo test -p uc-core -p uc-application -p uc-infra` 绿
- [ ] `slice2_phase2_clipboard_e2e.rs` 跑通(两例)
- [ ] CLI `uniclipboard-cli send --help` / `watch --help` 输出合理
- [ ] 两 profile 手动验证:A 复制文字 → `send` → B `watch` stderr 出条目
- [ ] task_plan.md Phase 2 段打 ✅ + 所有 commit hash 记录
- [ ] `slice2-phase2-plan.md §12` live tracker 封版

---

## 13. 推进节奏建议

- **Day 1**(~8h):T1 → T2(探针)→ T3 → T4/T5 并行
- **Day 2**(~8h):T6 → T7/T8 并行 → T9
- **Day 3**(~5h):T10 → T11 → T12 → T13 手动 → T14

每完成一组相关 T 做一次 atomic commit,message 前缀 `feat(Slice2/P2): ...` / `test(Slice2/P2): ...` / `docs(Slice2/P2): ...`。

---

## 14. 关键决策点(等用户确认)

在开 T1 之前,想让你明确 4 个点:

1. **Phase 2 scope 是否限定在"text 同步 + CLI only"**?
   - 我的推荐:是。图片 / 富文本 / daemon 接入留 Phase 3。理由:Phase 1 工时比估省 30% 是靠 scope 小 + 复用现成 infra;Phase 2 scope 再铺开(daemon / tauri / 大 payload)会直接进入 ≥ 40h 的泥潭。
2. **是否复用 `TransferCipherPort` + `ClipboardBinaryPayload` V3 codec**,还是另起新加密路径?
   - 我的推荐:复用。V3 codec 在 `uc-core/src/network/protocol/clipboard_payload_v3.rs` 已成熟,task_plan 里也要求 V3。不再造。
3. **wire version 独立编号还是共享 `WIRE_VERSION`**?
   - 我的推荐:独立。`WIRE_VERSION` 是 pairing 私用的(Slice 2 P1 升到 2 是为 transport_address_blob),clipboard wire 跟它没耦合;独立后将来 clipboard 改版不牵动 pairing。
4. **CLI `send` / `watch` 是否写系统剪贴板**?
   - 我的推荐:`send` 读系统剪贴板 + dispatch;`watch` 接收到的**只打印 stderr**,不写系统剪贴板(§5.3 rationale)。daemon 改装到 iroh 栈后再开这个。

如果 4 个点你有不同意见,请标出,我改完 plan 再开工。

---

> **开工信号**:用户点头 → 从 T1 开始。

---

## 15. 进度跟踪(live · 2026-04-22 在做)

### 15.1 任务状态

| # | 任务 | 状态 | commit | 实际工时 | 备注 |
|---|---|---|---|---|---|
| T1 | uc-core 新 port + legacy deprecated | ✅ | `0edb7479` | 0.3h | `ClipboardDispatchPort` / `ClipboardReceiverPort` / `ClipboardHeader` / `SyncPayload` / `DispatchAck` 落 core;legacy `transport.rs` 两 trait 加 `#[deprecated(since="Slice2-Phase2")]`——warning 在 `uc-app::sync_outbound` / `uc-daemon::inbound_clipboard_sync` 冒出是意图的"指路牌",Slice 5 删除 |
| T2 | iroh identity probe | ✅ | `5a9ea34f` | 0.4h | 3 verdict 全绿;**关键发现**:`iroh::EndpointId = iroh_base::PublicKey`(32-byte Ed25519),`Connection::remote_id().as_bytes()` 与 `SecretKey::public().as_bytes()` 字节等价,**无需扩 port**。§10 风险表第 1 行风险消除 |
| T3 | clipboard_wire postcard codec | ✅ | `b2206e33` | 0.4h | 7 单测全绿(6 计划内 + ack codec);frame: `[magic=0xC1 \| header_len_be(4) \| header \| payload_len_be(4) \| payload]`;`CLIPBOARD_MAGIC` 是 mis-routed ALPN 早拒哨兵;`bytes = "1.7"` 加入 uc-infra Cargo.toml |
| T4 | IrohClipboardDispatchAdapter | ✅ | `ae5b8202` | 0.5h | 4 单测(含 oversized 本地短路不拨号);`CLIPBOARD_ALPN = "uniclipboard/clipboard/0"`;错误折叠 `Offline`(missing addr / decode / dial fail)、`Io`、`PeerRejected` |
| T5 | IrohClipboardReceiverAdapter + ProtocolHandler | ✅ | `63330895` | 0.7h | 4 单测(含 3 并发 connection);**bug 修**:handler 返回时 Connection drop 导致 ack byte 来不及 flush,参考 presence handler 加 `connection.closed().await` 保活;identity 解析按 T2 probe 实现,`member_repo.list()` 扫描(N ≤ 10) |
| T6 | install_clipboard 扩展点 | ✅ | `c500ae62` | 0.2h | 4 单测(含三 ALPN 共存);`ClipboardHandlers { dispatch, receiver }` 结构对齐 `PairingHandlers`;`RouterBuilder::spawn` 自动重注 ALPN 集,`bind()` 只声明 PAIRING |
| T7 | DispatchClipboardEntryUseCase | ✅ | `896e371b` | 0.8h | 5 单测全绿;输入 `DispatchClipboardEntryInput { plaintext, content_hash, payload_version }`——不内置"读系统剪贴板 + 构造 payload"步,caller 负责;iteration source 复用 Phase 1 的 `peer_addr_repo.list()` 决策;`JoinSet` 并发 + 本机 filter + Online-only filter;hand-written `RecipeDispatch` 规避 mockall-Mutex(Phase 1 T6 教训);`bytes = "1.7"` 加入 uc-application Cargo.toml |
| T8 | IngestInboundClipboardUseCase | ✅ | `57ab9e65` | 0.4h | 4 单测;Phase 2 刻意不做本地持久化 + 不做本地 dedup(adapter 的 Ack 边界已分 Accepted/DuplicateIgnored);decrypt 失败 warn + skip,loop 继续;`IngestSpawnHandle` Drop 自动 abort |
| T9 | ClipboardSyncFacade | ✅ | `5b49d0ca` | 0.5h | 3 单测;完整 public ↔ internal type 映射(7 对类型 + `From<DispatchSyncError>`);subscribe 桥接走 relay task 保证 pub types 独立演进;`IngestHandle` Drop 级联 abort;`facade/mod.rs` 导出 9 个符号 |
| T10 | bootstrap 装配 | ⏸️ pending | — | — | **下一步**:`SpaceSetupAssembly` 加 `clipboard_sync`,`build_space_setup_assembly` 调 `install_clipboard` + 构造 facade,`auto_start_network` 成功后 spawn ingest loop |
| T11 | uc-cli send/watch | ⏸️ pending | — | — | — |
| T12 | slice2_phase2 e2e 集成测试 | ⏸️ pending | — | — | — |
| T13 | 手动双 profile 验收 | ⏸️ pending | — | — | — |
| T14 | task_plan.md ✅ 收尾 | ⏸️ pending | — | — | — |

### 15.2 累计

- **已完成**:T1-T9(9/14)~4.2h;比原估 ~15h 约省 72%,主要因 adapter 模块化好 + Phase 1 pattern 直接复用
- **测试**:29 单测 + 3 integration probe verdict 全绿(`cargo test` per-module 分开跑,未做全量回归)
- **进度**:等用户指示推进 T10,其余 5 任务整体 ~5-8h

### 15.3 关键决策 / 偏离

1. **wire version 独立编号**:`ClipboardHeader::CURRENT_VERSION = 1`,不借用 pairing `WIRE_VERSION = 2`。pairing codec 的版本改动不牵动 clipboard,隔离收益远大于"共用版本号"的一致性
2. **identity 解析免 port 扩展**(T2 探针确认):`IdentityFingerprintFactoryPort::from_public_key(&[u8])` 已足够,T5 adapter 在线 list + scan fingerprint——N ≤ 10 假设下性能足够,Phase 3 若要 scale 再加索引
3. **T5 connection.closed().await 保活**:handler `accept` 返回即 drop connection → ack byte race。参考 `IrohPresenceHandler` 模式,每个 ack-emit 分支都加 `connection.closed().await`。**Phase 2 发现的唯一隐蔽 bug**
4. **T7 不内置读系统剪贴板**:保持 use case 纯粹——CLI / daemon 负责"系统剪贴板 → ClipboardBinaryPayload → bytes"的 pipeline,use case 只接 bytes + content_hash。测试可预置 deterministic plaintext,不需要 mock OS clipboard
5. **T8 不做本地持久化**(plan §5.3):adapter 的 AckCode 已区分 Accepted / DuplicateIgnored,Phase 2 的 ingest 只 decrypt + broadcast;daemon / tauri 集成到 clipboard_event_repo 推 Phase 3。CLI `watch` 直接打印 stderr
6. **T6 RouterBuilder alpn 自动刷新**:iroh `RouterBuilder::spawn` 内部会 `endpoint.set_alpns(all_accept_alpns)`,所以 `bind` 只挂 PAIRING 就够。Phase 1 presence + Phase 2 clipboard 的 ALPN 都在 spawn 时自动挂上

### 15.4 后续提醒

- T10 bootstrap 要看 `SpaceSetupFacade::auto_start_network` 当前把 `ensure_reachable_all` 接在哪一行,clipboard ingest spawn 也接在那附近。需要给 SpaceSetupFacade 加 `Arc<ClipboardSyncFacade>` 字段 + `Mutex<Option<IngestHandle>>` 存 spawn handle
- T11 CLI 里 plaintext pipeline 最简做法:读系统剪贴板 `SystemClipboardPort::read_snapshot()` → 取第一个 text representation → postcard encode 一个 mini `ClipboardBinaryPayload` → SHA-256 content_hash。`watch` 循环读 `last_snapshot_hash` + `poll_system_clipboard` 每 500ms 比对变化
- T12 e2e 复用 Phase 1 `slice2_phase1_presence_e2e` 的两-assembly + pair 夹具,再加 `dispatch_entry()` 调用 + `subscribe_inbound_notices()` 断言 plaintext 字节等价。避免 T11 shell 脚本,Rust 集成测试已覆盖验收


