# UC-CORE 领域再划分：架构诊断与蓝图

> **状态**：架构诊断文档（不含实施）
> **目的**：为后续 milestone 级别的 uc-core 领域再划分提供决策依据和路线蓝图
> **触发背景**：试图将 `uc-core/src/network/` 整体搬迁到 `uc-platform`/`uc-infra` 的过程中，发现简单搬迁无法同时满足分层约束和领域边界约束，揭示出 uc-core 自身存在按"技术视角"组织而非"业务视角"组织的结构性问题

---

## 0. TL;DR

- **`uc-core/src/network/` 本身是技术视角的产物**：`network` 一词属于传输层词汇，把它作为 core 的顶层子目录等于把 infra 视角偷渡进了领域组织结构
- **该目录里混塞了至少 6 种不同业务的领域概念**，被"都和网络有关"这个技术标签硬绑在一起
- **5 个 port 的 trait 签名直接用"线上消息"作为参数**（`PairingMessage`、`NetworkEvent`、`DiscoveredPeer` 等），让 port 契约与传输格式强耦合
- **64 处 `uc_core::network` 引用 + 90 处 port 引用**贯穿所有下游 crate，是跨层流通单位错误（传输消息而非领域命令）的直接症状
- **真正的修复不是搬文件**，而是：
  1. 按业务域重新划分 uc-core
  2. 消息类型降格为 infra 私有实现细节
  3. **按 domain → usecase → port 的顺序**重建契约，port 作为派生物而非起点
- **方法论底线**：**不先设计 port**。port 是 usecase 执行中缺少的能力的抽象 —— 没有 usecase 在消费，port 就不应该存在
- **规模**：这是一次领域再划分（Domain Re-architecture），工作量在 3-6 周级别，需要拆成多个 milestone 渐进推进

---

## 1. 现状诊断

### 1.1 `uc-core/src/network/` 目前的内容

根据仓库当前状态，该目录至少包含以下模块（见 `uc-core/src/network/mod.rs`）：

```
uc-core/src/network/
  address_registry.rs        Address/Scope/Source（网络地址元数据）
  connection_policy.rs       ConnectionPolicy / AllowedProtocols / ProtocolKind
  daemon_api_strings.rs      Daemon API 字符串常量
  events.rs                  NetworkEvent / DiscoveredPeer / ConnectedPeer / NetworkStatus
  paired_device.rs           PairedDevice / PairingState
  pairing_state_machine.rs   PairingStateMachine / PairingAction / PairingEvent / Role / Failure
  protocol/                  ProtocolMessage / PairingMessage / ClipboardMessage / ...
    clipboard.rs
    clipboard_payload_v3.rs  UC3 二进制编解码（带 header + chunked AEAD）
    device_announce.rs
    file_transfer.rs         带 Read/Write 二进制编解码
    heartbeat.rs
    pairing.rs
    protocol_message.rs      frame_to_bytes / to_bytes / from_bytes
  protocol_ids.rs            libp2p 协议 ID 字符串常量
  session.rs                 SessionId
```

### 1.2 这个目录里塞了几种不同业务

把上述内容按"实际业务域"重新归类：

| 现有文件 | 实际业务域 | 是否"网络"相关 |
|---|---|---|
| `paired_device.rs` | **设备成员 / 信任关系** | 不是，是身份关系 |
| `pairing_state_machine.rs` | **配对流程** | 是一个跨设备业务，但核心是流程，不是传输 |
| `connection_policy.rs` | **基于信任状态的能力授权**（命名本身误导） | 不是"连接"策略，是"授权"策略 |
| `session.rs` | 配对/空间接入的**会话标识** | 领域概念，不依赖传输 |
| `events.rs` | 混合：部分是**对等方在场性**（presence），部分是传输细节 | 多数字段是传输 |
| `protocol/*` | **线上消息格式 + 序列化** | 传输细节 |
| `protocol_ids.rs` | libp2p 协议 ID | 传输细节 |
| `address_registry.rs` | 网络地址抽象 | 传输细节 |
| `daemon_api_strings.rs` | 进程间 API 字符串 | **表示层**，根本不属于 core |

**结论**：至少 6 种不同业务被硬塞进一个叫 `network/` 的目录。这不是领域建模，是按"传输视角"打包。

### 1.3 量化症状

| 指标 | 数值 | 含义 |
|---|---|---|
| `uc_core::network::*` 引用（Rust 源文件） | **64 个文件** | 传输相关类型直接贯穿所有下游 crate |
| 5 个 "transport-ish" port 的引用 | **25 个文件，90 处** | 这些 port 的消费者分散，且几乎所有消费者同时依赖 port 签名里的消息类型 |
| 受影响的 crate 数 | uc-app、uc-application、uc-platform、uc-daemon、uc-bootstrap、uc-tauri | 几乎所有业务相关 crate 都持有传输消息 |

**解读**：消息类型（`PairingMessage` / `ProtocolMessage` / `DiscoveredPeer` 等）被所有层"直接消费"，而不是被 infra 隐藏。这说明 **跨层流通的基本单位是线上消息，而不是领域命令/事件**。

### 1.4 5 个 "transport-ish" port 的共同问题

```rust
// uc-core/src/ports/pairing_transport.rs
pub trait PairingTransportPort: Send + Sync {
    async fn send_pairing_on_session(&self, message: PairingMessage) -> Result<()>;
    //                                          ^^^^^^^^^^^^^^^^^^^^^ 线上消息
}

// uc-core/src/ports/network_events.rs
pub trait NetworkEventPort: Send + Sync {
    async fn subscribe_events(&self) -> Result<mpsc::Receiver<NetworkEvent>>;
    //                                                        ^^^^^^^^^^^^^ 传输事件聚合 enum
}

// uc-core/src/ports/peer_directory.rs
pub trait PeerDirectoryPort: Send + Sync {
    async fn get_discovered_peers(&self) -> Result<Vec<DiscoveredPeer>>;
    //                                                   ^^^^^^^^^^^^^^^ 含 peer_id/addresses
    async fn get_connected_peers(&self) -> Result<Vec<ConnectedPeer>>;
    fn local_peer_id(&self) -> String;         // libp2p PeerId 字符串化
    async fn announce_device_name(&self, device_name: String) -> Result<()>;
}

// uc-core/src/ports/discovery.rs
pub trait DiscoveryPort: Send + Sync {
    async fn list_discovered_peers(&self) -> Result<Vec<DiscoveredPeer>>;
}

// uc-core/src/ports/file_transport.rs
pub trait FileTransportPort: Send + Sync {
    async fn send_file_announce(&self, peer_id: &str, announce: FileTransferMessage) -> Result<()>;
    async fn send_file_data(&self, peer_id: &str, data: FileTransferMessage) -> Result<()>;
    async fn send_file_complete(&self, peer_id: &str, complete: FileTransferMessage) -> Result<()>;
    async fn cancel_transfer(&self, peer_id: &str, cancel: FileTransferMessage) -> Result<()>;
    async fn send_file(&self, peer_id: &str, file_path: PathBuf, transfer_id: String, ...) -> Result<()>;
}
```

**共同问题**：

1. **传线上消息，不传业务意图**：`PairingTransportPort::send_pairing_on_session(PairingMessage)` 实际上是一个"把这个字节流发出去"的 API，领域语义被完全遮蔽
2. **`peer_id: String` 泄漏传输身份**：业务应该关心的是 `DeviceId`（6 位稳定 ID），而不是 libp2p PeerId 字符串
3. **port 粒度太细，语义重叠**：`PeerDirectoryPort::get_discovered_peers` 和 `DiscoveryPort::list_discovered_peers` 签名几乎一样
4. **没有反方向（领域事件）的对称抽象**：`NetworkEventPort::subscribe_events` 只给了一个聚合 enum `NetworkEvent`，消费者要自己 match，相当于又一次"分发线上事件"

### 1.5 uc-platform 也被拖下水

`uc-platform/Cargo.toml` 直接依赖 `libp2p`，`adapters/libp2p_network/` 实质是一个网络 adapter 实现。这违反了 `uc-platform/AGENTS.md` §6.3「禁止变成 infra 大杂烩」，其中点名反例就是 "libp2p network adapter"。

### 1.6 其他顶级目录的嫌疑

用同样的"技术视角 vs 业务视角"尺子审视：

| 目录 | 嫌疑内容 | 真身猜测 |
|---|---|---|
| `uc-core/src/crypto/` | `MasterKey`、`KeySlotFile`、`EncryptionError` 等 | 需审视：是业务语义（`Passphrase` / `EncryptionPolicy`）还是技术参数（Argon2 参数、nonce 长度）？ |
| `uc-core/src/clipboard/` | 剪切板领域模型 | 是否混入了 OS 剪切板 API 相关概念？ |
| `uc-core/src/security/` | 安全策略 | 和 crypto 的边界是什么？ |

**本文档不给结论**，仅登记为需后续审视的开放问题。

---

## 2. 分层约束（必须遵守）

本次对话中用户明确的硬约束：

```
uc-core          ← 纯领域 + ports，不依赖任何下层
uc-app           → 只能依赖 uc-core（禁止 uc-infra / uc-platform）
uc-application   → 只能依赖 uc-core（禁止 uc-infra / uc-platform）
uc-platform      → 只能依赖 uc-core（禁止 uc-infra）
uc-infra         → 依赖 uc-core，实现 port
uc-bootstrap     → 装配层，唯一允许依赖所有其它 crate
```

**推论**：
- port trait 必须定义在 uc-core
- port trait 签名不能引用 uc-infra / uc-platform 里的类型
- 若线上消息被搬出 uc-core（例如搬到 uc-infra），它**不能**出现在 port 签名里
- 因此：线上消息必须**要么留在 uc-core，要么从 port 签名中移除**

这个推论是后续所有路线选择的根基。

---

## 3. 根本原因分析

| 层级 | 症状 | 根因 |
|---|---|---|
| 目录组织 | `uc-core/network/` 混装 6 种业务 | 按**传输视角**划分领域 |
| port 契约 | 签名用线上消息作为参数 | 没有把"业务 intent/outcome"作为跨层流通单位 |
| 下游使用 | 64/90 处广泛引用 | 跨层流通的是"数据结构"而非"契约动作" |
| adapter 位置 | libp2p adapter 在 uc-platform | 违反 platform vs infra 的边界（历史遗留） |

**根本问题一句话**：**uc-core 在多设备协作这条线上，没有用领域语言抽象 intent 和 outcome，而是直接把线上消息提升为领域接口**。所有下游问题都从这里派生。

### 3.1 方法论反模式：先定义 port

除"目录按技术视角划分"之外，本仓库还存在一类 **方法论级别的反模式** —— **port-first design**：

- port 被独立"设计"出来，然后再找 usecase 去调用它
- 或者 port 凭"感觉上系统需要"的能力预先列出，没有明确的 usecase 消费者
- 结果：port 粒度、签名、命名与业务意图不匹配；port 数量膨胀；跨 crate 引用散乱

**正确的顺序**：

```
domain       先定义（实体 / 值对象 / 策略 / 状态机 / 领域事件 / 领域命令）
  ↓
usecase      再定义（用户/系统要执行的完整业务操作及其编排）
  ↓
port         最后"发现"（usecase 执行中缺什么能力，就抽什么 port）
```

**核心原则**：

1. port 是**派生物**，不是起点
2. 每个 port 必须能回答"哪个 usecase 在调用它"
3. 如果一个 port 没有 usecase 消费，它不应该存在
4. port 签名的输入输出类型**必须是 domain 层类型**（实体 / 值对象 / 命令 / 事件），而不是传输层消息或技术库类型

本文档 §5 的目标蓝图、§6 的迁移路线，都严格遵守这个顺序。

---

## 4. 讨论过的路径与各自局限

本轮对话中依次被提出又被发现不足的方案：

| 代号 | 思路 | 局限 / 被否决原因 |
|---|---|---|
| **D1** | 仅把 `uc-platform::libp2p_network/` 等搬到 `uc-infra`，uc-core/network/ 不动 | 治标不治本，uc-core 里传输细节仍在，port 抽象仍错 |
| **D2** | 新建 `uc-proto` 薄契约 crate，放 `PairingMessage` 等纯数据类型，所有 crate 共用 | 绕过分层但并未解决"port 抽象层次错"问题；仅是把污染从 uc-core 搬到一个中立 crate |
| **D3 / D4** | port 签名改为领域命令（`PairingAction` / `PairingEvent`），线上消息降级为 infra 私有 | 方向正确，但**仍以 `network/` 做顶层目录**，没有挑战"按技术视角组织"的根问题 |
| **D5** | 按业务线渐进做 D4 | 同 D4，未触及 core 重组 |
| **D6（本文主张）** | 按业务域重新划分 uc-core + port 抽象升级 | 工作量最大，但唯一根治 |

---

## 5. 目标蓝图：业务视角的 uc-core

> **本章严格遵守 §3.1 方法论**：先 domain → 后 usecase → 最后派生 port。5.1/5.2 给出 domain 与 usecase 的提案形态，5.3 只给 port 派生**原则**（不列 port 清单 —— port 必须在 M-B/M-C 盘清 domain 和 usecase 后才能浮现）。

### 5.1 Domain 层：按业务域重新组织

**本章只涉及 domain，不涉及 usecase 编排，不涉及 port**。

```
uc-core/src/
  clipboard/              剪切板领域（审视内容是否纯领域）
  security/               安全策略领域（审视）
  crypto/                 密钥/加密业务语义（审视，必要时拆分）

  membership/       ← 新建   设备成员与信任关系（名词驱动）
    device.rs                Device 实体
    paired_device.rs         PairedDevice（从 network/ 迁入）
    pairing_state.rs         PairingState
    trust_policy.rs          "A 信任 B 时可以同步什么"

  pairing/          ← 新建   配对流程（流程驱动）
    state_machine.rs         PairingStateMachine（从 network/ 迁入）
    action.rs                PairingAction（领域命令）
    event.rs                 PairingEvent（领域事件）
    session.rs               SessionId
    capability_policy.rs     ← 原 connection_policy 改名。真实语义是"根据信任状态允许哪些业务能力"
    failure.rs               FailureReason / CancellationBy / TimeoutKind

  transfer/         ← 新建   传输业务（clipboard 和 file 的共同抽象）
    transfer_intent.rs       领域意图
    transfer_session.rs      TransferSessionId / 状态

  presence/         ← 新建   对等方在场性
    peer_presence.rs         DomainPeer { device_id, device_name, trust_state }
    presence_event.rs        PeerAppeared / PeerDisappeared（领域事件，非传输事件）

  connectivity/     ← 可选   若不需要可并入 presence
    network_status.rs        Connected / Disconnected 纯状态枚举
```

**注意**：此结构里**没有 `ports/` 子目录的提案**。port 的目录结构将在 M-D（port 派生）阶段从 usecase 清单反推出来，而不是在此预先定义。现有 `uc-core/src/ports/` 保持原地，按业务域的 port 在后续 milestone 内逐步派生和迁入。

### 5.2 Usecase 层：列出每个业务域的业务操作

**本章只识别 usecase，不设计 port**。

具体 usecase 目录（`USECASE_CATALOG_ZH.md`）应在 M-C 产出，此处仅示意每个业务域内的 usecase 类别，作为 M-C 的起点：

| 业务域 | usecase 类别举例 |
|---|---|
| membership | `pair_device` / `unpair_device` / `list_members` / `update_member_sync_settings` / `resolve_trust_state` |
| pairing | `initiate_pairing_session` / `confirm_pairing_pin` / `cancel_pairing` / `handle_incoming_pairing_request` / `observe_pairing_progress` |
| transfer | `send_clipboard_entry` / `receive_clipboard_entry` / `start_file_transfer` / `cancel_file_transfer` / `observe_transfer_progress` |
| presence | `list_discovered_devices` / `list_connected_devices` / `subscribe_presence_changes` / `announce_local_identity` |
| clipboard | `capture_local_clipboard` / `store_entry` / `list_history` / `restore_entry` |

**M-C 的工作**：对每个 usecase，列出：

1. 业务触发源（用户操作 / 外部事件 / 内部调度）
2. 输入（domain 类型）
3. 输出（domain 类型）
4. 编排的 domain 内操作（实体方法 / 策略 / 状态机转移）
5. **需要但 domain 自己做不了的能力**（数据库查询？网络发送？时间？随机数？文件系统？通知外部？）—— 这是 port 派生的依据

### 5.3 Port 派生原则（不在本文档给出 port 清单）

**原则**：

- port 的候选者，来自 M-C usecase 清单第 5 项"所需能力"
- 同一种能力被多个 usecase 需要 → 合并为一个 port
- 一个 port 不能被任何 usecase 调用 → 删除（或推迟到真正有消费者时再建）
- port 签名的**所有**输入输出必须是 domain 类型（来自 §5.1）或标准库类型，**不得**引用：
  - 传输层消息（`PairingMessage` / `ProtocolMessage` / `FileTransferMessage`）
  - 第三方库类型（`libp2p::PeerId` / `diesel::...`）
  - 表示层类型（HTTP DTO / Tauri command model）
- port 命名由"能力"驱动，而不是由"谁来实现"驱动：
  - ✅ `TrustStateReader`（能力：读某 device 的信任状态）
  - ❌ `DatabaseDeviceRepository`（实现暗示）
  - ❌ `PairingTransportPort`（"transport"是实现，不是业务能力；正确问法："pairing usecase 需要什么？" → 发出配对命令 / 接收配对事件 → 两个 port）

**具体 port 清单和签名由 M-D 产出**，写入 `PORT_CATALOG_ZH.md`。本文档不做 port 设计。

### 5.4 跨层流通单位的基本方向（非 port 设计）

虽然本文档不设计 port，但可以先定下**跨层数据流通的基本原则**（这是 domain 层的约束，不是 port 设计）：

| 约束 | 说明 |
|---|---|
| 跨 crate 流通的是 domain 命令 / 事件 / 实体 | `PairingAction` / `PairingEvent` / `DomainPeer` / `TransferIntent` 等 |
| 线上消息不得跨层流通 | `PairingMessage` / `ProtocolMessage` / `FileTransferMessage` 仅存在于 infra 内部 |
| peer 标识跨层只用 `DeviceId` | libp2p `PeerId` 仅 infra 内部使用 |
| 传输错误转化为 domain 失败原因后再上传 | `FailureReason::NetworkUnreachable` 等领域错误，不直接暴露 `std::io::Error` |

### 5.5 消息类型的最终归宿

```
uc-infra/src/network/
  libp2p/                      ← 从 uc-platform 迁入
  pairing_stream/              ← 从 uc-platform 迁入
  file_transfer/               ← 从 uc-platform 迁入
  wire/                        ← 新建：infra 私有的线上消息
    protocol_message.rs          PairingMessage / ClipboardMessage / ProtocolMessage
    clipboard_payload_v3.rs      UC3 binary format
    file_transfer_message.rs
    device_announce.rs
    heartbeat.rs
  mappers/                     ← 新建：domain ⟷ wire 双向转换
    pairing_mapper.rs            PairingAction <-> PairingMessage
    clipboard_mapper.rs
    peer_mapper.rs               libp2p PeerId + metadata -> DomainPeer
    file_transfer_mapper.rs
  protocol_ids.rs              libp2p 协议 ID 字符串
  net_utils.rs                 LAN IP 检测
```

`uc-core` 从此**完全看不到线上消息**。

### 5.6 非领域内容的去向

| 现有内容 | 去处 |
|---|---|
| `daemon_api_strings.rs` | 进程间表示层 → 迁到 `uc-daemon-contract` 或 `uc-daemon`（不属于领域） |
| `address_registry.rs` | 网络地址抽象 → 迁到 `uc-infra::network::libp2p::addressing` |

---

## 6. 迁移路线（milestone 级别）

> **原则**：
> - 每个 milestone 产出可合并的、编译通过的、测试绿的 PR，中间态不能有"工作区破坏"状态
> - **严格遵守 domain → usecase → port 的顺序**：纸面设计阶段（M-A ~ M-D）按此顺序推进；实施阶段（M-E ~ M-J）每个子域内部也按此顺序推进

### 阶段 1 · 纸面设计（不动代码）

#### M-A：架构诊断（**本文档，已完成**）

- 产出：`DOMAIN_REARCH_ZH.md`
- 交付：现状诊断 / 根因 / 方法论原则 / 目标蓝图（domain 部分）/ 迁移路线 / 开放问题清单

#### M-B：**Domain 词汇表**定稿

- 产出：`DOMAIN_GLOSSARY_ZH.md`
- 对每个业务域（membership / pairing / transfer / presence / clipboard / security / crypto）列出：
  - 实体（Entity）+ 唯一标识 + 生命周期
  - 值对象（Value Object）
  - 领域命令（Domain Command / Action）
  - 领域事件（Domain Event）
  - 业务策略（Policy）
  - 状态机（State Machine）及其转移
  - 业务错误 / 失败原因枚举
- 回答本文档 §8 中与领域建模相关的开放问题（Q2/Q3/Q4/Q5/Q7/Q8）
- 评审后定稿
- **严禁**：本 milestone 不讨论 port，不讨论 crate 依赖
- 不动代码

#### M-C：**Usecase 目录**定稿

- 前置：M-B 完成
- 产出：`USECASE_CATALOG_ZH.md`
- 对每个业务域，列出所有 usecase。每个 usecase 描述：
  1. 业务名称与触发源（用户操作 / 外部事件 / 定时器）
  2. 输入（domain 类型）
  3. 输出（domain 类型）
  4. 编排步骤（domain 内操作）
  5. **所需外部能力**（"这个步骤 domain 自己做不了，需要..."）—— 这是 port 派生的输入
- 回答本文档 §8 中与 usecase 相关的开放问题（Q1：uc-app vs uc-application 的 usecase 归属）
- **严禁**：本 milestone 仍不设计 port
- 不动代码

#### M-D：**Port 目录**派生

- 前置：M-B、M-C 完成
- 产出：`PORT_CATALOG_ZH.md`
- 方法：
  1. 聚合 M-C 中每个 usecase 的"所需外部能力"
  2. 合并相同能力 → 候选 port
  3. 对每个候选 port，列出：名称（能力驱动）/ trait 签名（只用 domain 类型）/ 消费此 port 的 usecase 清单
  4. 无消费者的候选 port → 删除
- 回答 §8 Q6（port 粒度，到此自然浮现）、Q10（fitness function 初步设计）
- 评审后定稿
- 不动代码

### 阶段 2 · 实施（动代码）

> **每个子域的实施顺序（子 phase）**：
> 1. domain 迁入 / 新建 uc-core 子目录
> 2. usecase 从 uc-app / uc-application 对应位置迁移或新建
> 3. port 按 M-D 派生结果在 uc-core/ports 下建立或重命名
> 4. uc-infra 侧的 adapter 更新 / mapper 建立
> 5. 批量更新调用点 import
> 6. 测试绿 + UAT

#### M-E：membership 子域实施

- 风险：低
- 范围：`PairedDevice` / `PairingState` / `Device` / `TrustPolicy`
- 不涉及线上消息或 pairing 状态机，属于最安全的首个实施子域（适合验证整个方法论流水线）

#### M-F：pairing 子域实施

- 风险：高（配对是最关键业务）
- 范围：state machine / Action / Event / Session / capability_policy（`connection_policy` 改名）
- 关键动作：
  1. domain 迁入 `uc-core/pairing/`
  2. usecase（initiate / respond / confirm / cancel / observe）按 M-C 清单重排
  3. 按 M-D 派生的 pairing 相关 port 建立（替换老的 `PairingTransportPort` / 部分 `NetworkEventPort`）
  4. uc-infra 建立 `PairingAction ⟷ PairingMessage ⟷ bytes` 的 mapper 链
  5. 全链路 UAT 配对流程
- 前置：mapper 必须先有单元测试保证 wire format 不变

#### M-G：presence / transfer 子域实施

- 风险：中
- 范围：
  - presence：`DomainPeer` 替换 `DiscoveredPeer` / `ConnectedPeer`，按 M-D 派生 presence 相关 port
  - transfer：`TransferIntent` / `TransferSession` 替换 `FileTransferMessage` 在 port 中的位置
- 清理 `peer_id: String` 泄漏，改用 `DeviceId`

#### M-H：platform → infra 物理搬迁

- 风险：中
- 前置：M-F、M-G 完成（否则下游仍依赖 uc-platform 的网络类型会造成返工）
- 动作：
  1. 把 `uc-platform/adapters/libp2p_network/` 等搬到 `uc-infra/network/libp2p/`
  2. 把 `uc-platform/Cargo.toml` 里的 libp2p / bytes / local-ip-address 等搬到 `uc-infra/Cargo.toml`
  3. `uc-bootstrap` 里装配代码改 import
  4. 验证 uc-app / uc-application / uc-platform 不再依赖 uc-infra 里的类型（只有 uc-bootstrap 可以）

#### M-I：uc-app vs uc-application 定论 + crypto / clipboard 真身审视

- 风险：中到高，视结论而定
- 前置：M-B/M-C 已给出定论；此 milestone 是实施层面的收敛
- 可能动作：crate 合并 / usecase 再分布 / crypto 拆分到 core+infra / clipboard 重新建模

#### M-J：清理与守恒验证

- 删除 `uc-core/network/` 目录（所有内容已迁出）
- 更新所有 AGENTS.md
- 实施 fitness function / 架构测试（Q10 的答案）：
  - CI 检查 uc-app / uc-application / uc-platform 的 Cargo.toml 不含 uc-infra
  - CI 检查 uc-core 的公共 API 不含传输消息类型
- 全链路 UAT

---

## 7. 守恒 / 非目标

本架构重构**不应**改变：

- ✅ 用户可见功能（剪切板同步、配对、文件传输的行为）
- ✅ 线上协议 wire format（跨版本兼容）
- ✅ SQLite schema / 迁移文件
- ✅ 配对流程、state machine 的业务语义
- ✅ 加密格式与密钥派生
- ✅ libp2p protocol IDs

这些是硬契约，重构只搬位置、改命名、改 port 签名，不变业务真相。

---

## 8. 开放问题

> 需要在 M-B 前回答。本文档不给答案，只登记。

### Q1：`uc-app` 和 `uc-application` 并存的意义

- 两个 crate 当前分工是什么？（Phase 3 membership 迁移把一部分 use case 从 uc-app 挪到了 uc-application）
- 是否应合并？如果不合并，分工契约是什么？
- 新建 `uc-core/pairing/` / `uc-core/membership/` 时，对应 use case 归谁？

### Q2：`uc-core/crypto/` 的真身

- 里面是 `MasterKey` / `KeySlotFile` 的业务定义（纯领域），还是 Argon2 参数 / nonce 长度（技术参数）？
- 如果是后者，应拆：领域侧留 `EncryptionPolicy` / `UnlockContext`，参数迁 `uc-infra::crypto`

### Q3：`uc-core/clipboard/` 的真身

- 是剪切板同步的业务模型（`ClipboardEntry` / 保留策略），还是 OS 剪切板 API 相关？
- 新 `uc-core/transfer/` 里的 `TransferIntent` 与 `ClipboardEntry` 的关系？

### Q4：`connectivity/` 是否保留

- `NetworkStatus::{Connected, Disconnected}` 是否有领域价值？
- 或者直接合并到 `presence/`（"至少一个 peer 在场"等价于 connected）？

### Q5：命名字典确认

- `DiscoveredPeer` → `PeerPresence`？
- `ConnectedPeer` → ？（是否和 presence 合并，或保留为"已建立加密会话的 peer"领域概念）
- `ConnectionPolicy` → `CapabilityPolicy`？
- `SessionId` 是 pairing 专属还是 pairing + space-access 通用？

### Q6：port 粒度

- **本问题不应在 M-A/M-B 阶段回答**。port 粒度是 usecase 清单（M-C）盘清之后自然浮现的结果：
  - 相同能力被多个 usecase 共用 → 合并
  - 单 usecase 独占的能力 → 独立 port
  - 无 usecase 消费 → 不建
- 延迟到 M-D 回答。此处仅登记为"会在 M-D 自动关闭"的问题。

### Q7：`NetworkEvent` 聚合 enum 的处置

- 当前 `NetworkEvent` 是一个 enum 包含所有跨层事件（peer/pairing/clipboard/status）
- 拆成多个领域事件后，是否需要一个顶层 "any domain event" 聚合类型，还是每个业务域自己的 port 发自己的事件？

### Q8：`peer_id: String` 泄漏的清理策略

- 领域侧应只暴露 `DeviceId`（6 位稳定 ID）
- libp2p `PeerId` 应成为 infra 内部索引
- 但 pairing 流程的 `PairingRequest.peer_id` 字段是协议级字段（wire format），领域与 wire 的映射如何设计？
- 这个问题的答案决定了 mapper 设计的核心

### Q9：`uc-platform` 清理后还剩什么

- 如果 libp2p adapter 全部离开 uc-platform，该 crate 的合理职责是什么？
- 是否回到 AGENTS §2.1 定义的"平台差异收敛"（app_dirs / secure_storage / clipboard OS API / autostart）？
- encryption.rs 里的 `InMemoryEncryptionSessionPort` 是否该继续留在 platform？

### Q10：架构测试 / fitness function

- 如何在 CI 层面防止"uc-app 依赖 uc-infra"这种违规再次出现？
- 用 cargo-deny？自定义脚本扫描 Cargo.toml？proc-macro 检查？
- 这个机制是本次重构成功的守护者，必须在 M-H 之前确立

---

## 9. 附录 A：本次对话的核心发现时间线

1. 起始诉求：`uc-core/src/network/` 整体迁出到 `uc-platform`
2. 诊断 1：发现 `connection_policy.rs` 是 port 契约的一部分，不能搬
3. 调整 1：改为"仅迁 events + protocol 到 uc-platform，其余留 core"
4. 诊断 2：发现 uc-platform 承接 libp2p adapter 本身就违规（AGENTS §6.3）
5. 调整 2：改为"一次性迁到 uc-infra"
6. 诊断 3：物理搬迁后发现 uc-core 自身的 5 个 port 反向依赖被搬走的类型
7. 考虑路线 1：把 5 个 port 也搬到 uc-infra
8. 诊断 4：用户明确"uc-app/application/platform 不能依赖 uc-infra"—— 路线 1 不成立
9. 提出方向 D2 / D3 / D4 / D5 作为替代
10. 诊断 5（用户追问）："core 怎么还是以网络进行领域划分，不是从业务角度"—— 触及真正根问题
11. 诊断 6（用户追问）："我们不应该直接就定义 port。正确的流程，应该是先定义 domain，然后定义 usecase，根据 usecase 所需能力，缺少什么能力，我们就定义什么 port"—— 方法论级别的修正，触及"port-first 反模式"
12. 本文档结论：需要一次 Domain Re-architecture，按 domain → usecase → port 顺序推进，共 M-A ~ M-J 十个 milestone

## 10. 附录 B：当前工作区状态

**⚠ 本次对话中已发生的物理代码变更（未提交）**：

| 文件 | 状态 |
|---|---|
| `uc-infra/Cargo.toml` | 已加入 libp2p / libp2p-request-response / libp2p-stream / local-ip-address / bytes / futures / tokio-util / tokio "full" feature |
| `uc-infra/src/network/events.rs` | 已创建（从 uc-core 拷贝，改 `crate::pairing` → `uc_core::pairing`）|
| `uc-infra/src/network/protocol/` | 已创建（从 uc-core 拷贝，`pairing.rs` 里 `crate::crypto::model` → `uc_core::crypto::model`）|
| `uc-infra/src/network/mod.rs` | 用户已回滚到只有 `pub mod space;` |
| `uc-core/src/network/events.rs` | 已删除 |
| `uc-core/src/network/protocol/` | 已删除 |
| `uc-core/src/network/mod.rs` | 用户已改动（加入了 `address_registry` / `daemon_api_strings` / `paired_device` / `pairing_state_machine` / `protocol_ids`，并保留 `events` / `protocol` 声明 —— 后者指向已删除的物理文件） |
| `task_plan.md` / `findings.md` / `progress.md` | 本轮创建的规划文件，包含过时决策（基于 D1/D2 思路）|

**工作区编译状态**：**uc-core 编译失败**（`mod.rs` 引用了已删除的 `events` 和 `protocol` 模块；`uc-infra/src/network/protocol/clipboard.rs` 还缺 `serde_with` 依赖）。

**回到干净状态的两种路径**：

- **路径 P1（回滚 uc-infra 侧，恢复 uc-core 侧）**：
  1. 从 `uc-infra/src/network/protocol/` 和 `uc-infra/src/network/events.rs` 拷回 `uc-core/src/network/`（并把 `uc_core::` → `crate::` 复原）
  2. 删除 `uc-infra/src/network/events.rs` 和 `uc-infra/src/network/protocol/`
  3. 回滚 `uc-infra/Cargo.toml` 中添加的网络相关依赖
  4. 留 `uc-core/src/network/mod.rs` 为用户当前状态
- **路径 P2（保留当前改动并按 M-B 计划推进）**：不回滚，让下个 milestone 从当前状态继续

**建议**：使用 P1，因为当前状态混合了两种思路的中间态，保留会让 M-B 规划复杂化。

## 11. 附录 C：文档留存

以下规划文件应在本次任务结束前决定去留：

- `task_plan.md`、`findings.md`、`progress.md` —— 基于已被否定的 D1/D2 思路。建议：**删除** 或 **移动到 `.planning/archive/` 作为历史记录**
- `DOMAIN_REARCH_ZH.md`（本文档）—— 保留，作为 M-B 的输入

---

**文档状态**：**最终版（M-A 交付物）**，经方法论修正（v2）
**下一步**：由用户决定是否启动 M-B（Domain 词汇表定稿）。**注意**：M-B 只做 domain，不做 port；port 要到 M-D 才出现。
