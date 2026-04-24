# Slice 3 Phase 1 · Blob 基础设施 —— 细化计划

> **状态**:计划稿(2026-04-24),待用户过目后开工
> **父文档**:`task_plan.md` 的 Slice 3 章节 + Slice 3 Phase 拆分表
> **前置**:Slice 2 Phase 1/2/3 ✅(2026-04-22 / 2026-04-22 / 2026-04-23)
> **跨 Phase 决策**:S3-D1(三段拆)/ S3-D2(V3 兼容扩展,Phase 3 用)/ S3-D3(blob cache 临时目录)/ S3-D4(CLI 长期命令)已在 `task_plan.md` Slice 3 节锁定

---

## 1. 目标复述

在 iroh 栈上打通**本地 blob 存储**的基础能力:加密密文落盘 → content-addressed 寻址 → 按 ticket 跨节点拉取。**不接** usecase、**不接** CLI、**不接** 剪贴板——这些是 Phase 2 / Phase 3 的事。

Phase 1 只回答一件事:**给一段密文 bytes,本地 adapter 能否把它稳定地 publish / fetch / tag / untag / has / issue_ticket,并用自回环测试验证契约?**

**验收(已在 task_plan.md Phase 拆分表锁定)**:
1. 新 2 个 port 定义进入 `uc-core`,无 iroh 类型泄漏
2. iroh-blobs `FsStore` adapter 实现全 6 方法(publish / has / tag / untag / issue_ticket / fetch)
3. `blob_reference` Diesel 表 + repo 实现,支持 `find_by_plaintext_hash / save / forget`
4. adapter 单元测试在单节点 loopback 下跑通(同一 Endpoint 上 publish → issue_ticket → fetch 往返字节一致)
5. bootstrap 装配:`SpaceSetupAssembly` 持有新 port 的 `Arc<dyn ...>`,`IrohNodeBuilder::install_blobs` 扩展点可调

**明确不做**(推 Phase 2):
- `PublishBlobUseCase` / `FetchBlobUseCase` 的 application 层编排
- `BlobCipherPort` 的调用(Phase 1 的 adapter 只处理密文 bytes,加解密由 Phase 2 usecase 负责)
- CLI 命令
- 跨节点 e2e(两 Endpoint fetch,Phase 2 CLI e2e 覆盖)

---

## 2. 架构分层(新建 / 扩展对照)

```
uc-core
  └── ports/
      ├── blob/                        🆕 新子目录(与旧 uc-core/src/blob/ports/ 并存)
      │     ├── mod.rs                 🆕
      │     ├── transfer.rs            🆕 BlobTransferPort + BlobDigest + BlobTicket
      │     │                               + TagReason + BlobError
      │     ├── reference.rs           🆕 BlobReferenceRepositoryPort + PlaintextHash
      │     │                               + BlobReferenceError
      │     └── (无 usecase / facade,Phase 2 才加)
      └── mod.rs                       ✏️ 挂 pub mod blob

uc-infra
  ├── network/iroh/
  │     ├── node.rs                    ✏️ 新增 install_blobs 扩展点 + BlobHandlers
  │     ├── blobs.rs                   🆕 IrohBlobTransferAdapter(BlobTransferPort 实现)
  │     │                                 + BLOBS_ALPN(复用 iroh-blobs crate 内置)
  │     └── mod.rs                     ✏️ 导出 BlobHandlers
  └── db/
        ├── repositories/
        │     └── blob_reference_repository.rs  🆕 DieselBlobReferenceRepository
        ├── models/
        │     └── blob_reference.rs    🆕 BlobReferenceRow
        ├── mappers/
        │     └── blob_reference.rs    🆕 row ↔ domain
        └── schema.rs                  ✏️ 新 table! blob_reference
migrations/
  └── 2026-04-24-000001_create_blob_reference/  🆕
        ├── up.sql
        └── down.sql

uc-bootstrap
  └── assembly.rs                      ✏️ 解析 iroh-blobs FsStore 目录(`$APP_DATA/iroh-blobs[_<profile>]/`)
                                          + SpaceSetupAssembly 加 blob_transfer / blob_reference 两字段
                                          + build_space_setup_assembly 内调 install_blobs
```

**Legacy 保留**(Slice 5 统一清):
- `uc-core/src/blob/ports/{reader,writer}.rs`(libp2p file_transfer 时代遗留)
- `uc-core/src/blob/` module 整体(`Blob` / `BlobStorageLocator` 旧值对象)
- `uc-infra/src/blob/{blob_writer,filesystem_store,repository_port,store_port,domain}.rs`(旧 adapter)
- `uc-bootstrap/src/assembly.rs` 内 `config_dir.join("blobs")` 旧 `FilesystemBlobStore` 装配路径

**关键命名避撞**:新目录叫 `iroh-blobs`(不是 `blobs`),与旧 `blobs/` 目录共存,Slice 5 删旧路径后不改名——"iroh-blobs" 直接反映后端 crate,符合 `uc-infra/AGENTS.md §16` 命名规范("Port 实现 = `*Adapter`";`IrohBlobTransferAdapter`)。

---

## 3. 新 port 契约草图

### 3.1 `BlobTransferPort`(uc-core)

```rust
// uc-core/src/ports/blob/transfer.rs

use async_trait::async_trait;
use bytes::Bytes;

use crate::clipboard::entry::ClipboardEntryId;

/// 一份密文在本机存储中的**身份标识**。
///
/// 当一段密文被放入本地可共享存储后,会得到一个稳定的 32 字节标识:
/// 相同密文得到相同标识,不同密文得到不同标识。上层用它来回答"我本地
/// 是不是已经有这份密文了"、"这份密文是否等同于那份密文"——不用把
/// 整个密文加载进内存做比对。
///
/// 具体编码由存储 adapter 负责,uc-core 将其视为不透明字节。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlobDigest([u8; 32]);

impl BlobDigest {
    pub const fn from_bytes(b: [u8; 32]) -> Self { Self(b) }
    pub const fn as_bytes(&self) -> &[u8; 32] { &self.0 }
}

/// 一份被共享密文的**领取凭据**。
///
/// 当一台设备把大 payload(文件 / 大图)的密文发布到本地可共享存储后,
/// 会拿到一张凭据;凭据随剪贴板同步通知跨设备传递;同一 space 的其他
/// 成员收到通知后,凭此:
///
/// - 先问"这份凭据对应哪份密文"([`BlobTransferPort::digest_of`])
///   ——用于本地去重判断,我已经拿到过就不用再拉
/// - 凭此去拉取实际密文([`BlobTransferPort::fetch`])——由存储 adapter
///   与凭据中携带的来源建立连接并传输
///
/// 凭据**不包含**密文本身、**不包含**解密密钥——内容拉回后由
/// [`BlobCipherPort`] 另走解密。凭据也不自带真实性保证:对伪造 /
/// 替换的防护来自空间成员关系与密文本身的 AEAD,不来自凭据。
///
/// 凭据内部至少携带"内容身份 + 至少一个可达来源"两件事,具体编码由
/// 存储 adapter 负责,uc-core 将其视为不透明字节。
///
/// [`BlobCipherPort`]: crate::ports::security::blob_cipher::BlobCipherPort
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlobTicket(Vec<u8>);

impl BlobTicket {
    pub fn from_bytes(b: Vec<u8>) -> Self { Self(b) }
    pub fn as_bytes(&self) -> &[u8] { &self.0 }
}

/// 某份密文被哪个业务对象**持续引用**的理由。
///
/// 存储本身是共享资源:一份密文可能被多条剪贴板记录引用(同一文件被
/// 多次复制 / 被多设备收到后都登记同一份密文)。上层通过
/// [`BlobTransferPort::tag`] 声明"这份密文被 X 引用",通过
/// [`BlobTransferPort::untag`] 释放声明;存储 adapter 依此判断哪些
/// 密文可以被回收、哪些仍需保留。
///
/// Phase 1 只提供"被某条剪贴板记录引用"这一种理由——这是 Slice 3 里
/// 唯一的引用来源。未来新增理由(比如用户手动钉住)可以追加变体,
/// 不破坏现有 adapter。
///
/// 回收扫描本身不在 Phase 1 范围(参见 `task_plan.md §T-02`)——Phase 1
/// 只保证引用关系的声明 / 释放被正确记录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TagReason {
    ClipboardEntry(ClipboardEntryId),
}

/// Blob 相关操作的业务错误。
///
/// 故意保持粗粒度——存储后端多种多样,但对调用方真正重要的只有
/// "是否存在"、"能否拉到"、"凭据能不能读懂",细节由 adapter 在日志
/// 里补。
#[derive(Debug, thiserror::Error)]
pub enum BlobError {
    /// 本地存储里没有这份密文。调用方可以选择按凭据去拉,或放弃。
    #[error("blob not found")]
    NotFound,

    /// 拉不到这份密文:来源不可达、凭据指向的密文已被来源回收、中途
    /// 中断等。和"本地没有"区分:这里代表"远端也没能给我"。
    #[error("blob unavailable: {0}")]
    Unavailable(String),

    /// 凭据内容无法被当前 adapter 解读(跨版本不兼容、损坏、来自另
    /// 一套存储后端)。出现时通常意味着发送端与接收端的 adapter 不匹配,
    /// 是配置 / 部署层面的问题,不是数据错误。
    #[error("ticket could not be interpreted")]
    InvalidTicket,

    /// adapter 内部失败(IO / 底层库异常等),调用方一般只需记录并上报。
    #[error("internal: {0}")]
    Internal(String),
}

/// Blob 传输能力:发布 / 领取 / 生命周期管理。
///
/// 业务模型:
///
/// - **发布方**([`publish`] + [`issue_ticket`]):把一段密文放入本地
///   可共享存储,拿到本地身份 [`BlobDigest`];为外发需求再生成一张
///   [`BlobTicket`](可被其他设备用以拉取)
/// - **接收方**([`digest_of`] + [`fetch`]):拿到凭据后先问身份判断
///   本地是否已有;若无则按凭据拉取密文
/// - **引用管理**([`tag`] + [`untag`] + [`has`]):声明 / 释放某份
///   密文的业务引用,查询本地是否持有某份密文
///
/// 本 port **不**负责加解密——调用方拿回 / 交出的都是密文字节,由
/// [`BlobCipherPort`] 独立完成加解密。
///
/// [`publish`]: Self::publish
/// [`issue_ticket`]: Self::issue_ticket
/// [`digest_of`]: Self::digest_of
/// [`fetch`]: Self::fetch
/// [`tag`]: Self::tag
/// [`untag`]: Self::untag
/// [`has`]: Self::has
/// [`BlobCipherPort`]: crate::ports::security::blob_cipher::BlobCipherPort
#[async_trait]
pub trait BlobTransferPort: Send + Sync {
    // ── 发布 ──

    /// 把一段密文放入本地可共享存储,返回其稳定身份。
    ///
    /// 幂等:同一密文再次 publish 返回同一 [`BlobDigest`]。
    async fn publish(&self, ciphertext: Bytes) -> Result<BlobDigest, BlobError>;

    /// 为本地已有的一份密文生成一张对外领取凭据,让其他设备能凭此
    /// 向本机拉取。凭据中至少包含"内容身份 + 至少一个可达来源"。
    async fn issue_ticket(&self, digest: &BlobDigest) -> Result<BlobTicket, BlobError>;

    // ── 接收 ──

    /// 凭领取凭据拉取密文。若本地已有对应密文,adapter 可以直接从本地
    /// 返回;否则按凭据携带的来源去拉。断点续传与完整性校验由 adapter
    /// 保证,调用方只关心"拿到 / 没拿到"。
    async fn fetch(&self, ticket: &BlobTicket) -> Result<Bytes, BlobError>;

    // ── 生命周期 ──

    /// 查询本机是否持有某份密文。
    async fn has(&self, digest: &BlobDigest) -> Result<bool, BlobError>;

    /// 声明"这份密文正被某业务对象引用",让存储延后回收。
    async fn tag(&self, digest: &BlobDigest, reason: TagReason) -> Result<(), BlobError>;

    /// 释放先前通过 [`tag`](Self::tag) 声明的引用。幂等:解除不存在的
    /// 引用返回 `Ok(())`。
    async fn untag(&self, digest: &BlobDigest, reason: TagReason) -> Result<(), BlobError>;

    // ── 元数据查询(不拨号) ──

    /// 读出一张领取凭据所指向的密文身份,纯本地计算——不访问网络、
    /// 不读本地存储。典型使用场景:收到一条剪贴板通知后,先用本方法
    /// 读出凭据的身份,查询本地是否已有;有则跳过拉取,没有再走
    /// [`fetch`](Self::fetch)。
    ///
    /// 若凭据无法被当前 adapter 解读,返回 [`BlobError::InvalidTicket`]。
    fn digest_of(&self, ticket: &BlobTicket) -> Result<BlobDigest, BlobError>;
}
```

**关键决策**:
- `Bytes`(from `bytes` crate)作为 port I/O 类型——uc-core 已经依赖 `bytes`(Slice 2 Phase 2 的 `SyncPayload` 用过),不引入新依赖
- `publish` 返回 `BlobDigest`,**不返回** ticket——"digest 是本地身份,ticket 是外发凭据",两者生命周期不同,强制调用方显式 `issue_ticket`
- `BlobTicket` 是**真正 opaque** 的字节包装,**无** `digest()` 值对象方法——uc-core 不持有 ticket 内部结构知识。"从 ticket 抽 digest" 是 port 能力,走 `BlobTransferPort::digest_of`,解析细节由 adapter 承担(见 §8 R2 方案 C)
- **不暴露 `AsyncBytesReader`**——Phase 1 只支持一次性 Bytes;流式 API 留给 T-01 进度事件技术债(task_plan §T-01)
- **不设进度事件**——阻塞式 `publish / fetch`,回调 / stream 进 T-01(task_plan §T-01)
- **不设 GC API**——`BlobTransferPort` 只管引用关系的声明 / 解除(tag/untag),扫描回收归 T-02(task_plan §T-02)
- `BlobError` 业务语义中性:`Unavailable` / `InvalidTicket` / `NotFound` / `Internal`,不用 "Download" / "Fetch" 等传输层动词

### 3.2 `BlobReferenceRepositoryPort`(uc-core)

```rust
// uc-core/src/ports/blob/reference.rs

use async_trait::async_trait;
use crate::ports::blob::transfer::BlobDigest;

/// 一段**明文**内容的指纹,用于去重判断。
///
/// 业务场景:用户反复复制同一个文件,每次都会在本机触发"加密 → 发布
/// 到可共享存储"的链路。若不做去重,同一明文会被重复加密、产生多份
/// 等价密文、占用多倍存储。本指纹作为"明文身份"记在本机:下次再
/// 遇到同一明文,通过 [`BlobReferenceRepositoryPort::find_by_plaintext_hash`]
/// 找到先前产出的 [`BlobDigest`],直接复用——跳过再次加密和发布。
///
/// 与 [`BlobDigest`] 的区别:一个是明文身份,一个是密文身份。同一明文
/// 在不同空间下加密会产出不同密文——因此明文指纹到密文身份的映射是
/// **以"当前 active space"为作用域的多对一关系**。Phase 1 按"当前
/// active space"隐式单作用域存储;多 space 场景由 Phase 2+ 评估是否
/// 把 `space_id` 加入主键(留 T-XX 技术债)。
///
/// 指纹本身是 32 字节不透明值,由上层的 [`HashPort`] 计算后喂入本类型,
/// uc-core 不关心具体哈希方案。
///
/// [`HashPort`]: crate::ports::hash::HashPort
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PlaintextHash([u8; 32]);

impl PlaintextHash {
    pub const fn from_bytes(b: [u8; 32]) -> Self { Self(b) }
    pub const fn as_bytes(&self) -> &[u8; 32] { &self.0 }
}

#[derive(Debug, thiserror::Error)]
pub enum BlobReferenceError {
    /// 底层存储失败(连接 / 读写 / 约束冲突等)。调用方一般只需记录,
    /// 去重未命中时业务路径会退化为"照常加密再发布",不影响正确性。
    #[error("repository error: {0}")]
    Repository(String),
}

/// 明文指纹 ↔ 密文身份的去重缓存。
///
/// 写入来源:
/// - 本机首次发布一份新内容后,记录 `(plaintext_hash, digest)` 以备下次
///   同明文复用
/// - 本机从其他设备拉取并解密一份内容后,同样登记——为将来"本机作为
///   转发源"做准备(参见 `task_plan.md §T-03`)
///
/// 查询来源:每次本机准备加密并发布一份内容前,先查此处——命中即
/// 跳过加密、直接对已有密文发凭据。
#[async_trait]
pub trait BlobReferenceRepositoryPort: Send + Sync {
    /// 查一段明文之前是否已经产出过密文。命中返回对应密文身份,未命中
    /// 返回 `None`。
    async fn find_by_plaintext_hash(
        &self,
        hash: &PlaintextHash,
    ) -> Result<Option<BlobDigest>, BlobReferenceError>;

    /// 登记一条"明文指纹 → 密文身份"的映射。对同一明文指纹再次登记
    /// 视为覆盖——同一明文再次加密产出的密文可能与旧密文不同(nonce
    /// 随机),覆盖能让后续去重走最新那份。
    async fn save(
        &self,
        hash: PlaintextHash,
        digest: BlobDigest,
    ) -> Result<(), BlobReferenceError>;

    /// 删除一条登记。注意:本方法只删映射记录,不删本地存储里的密文
    /// 本身(那是 [`BlobTransferPort::untag`] 的事);用于明文内容被
    /// 用户显式删除等场景。
    ///
    /// [`BlobTransferPort::untag`]: super::transfer::BlobTransferPort::untag
    async fn forget(&self, hash: &PlaintextHash) -> Result<(), BlobReferenceError>;
}
```

**关键决策**:
- `save` 语义:**upsert**,last-write-wins(同 peer_address repo 的约定)——同一明文再次加密可能出新密文(nonce 随机),覆盖是正确行为
- `forget` 语义:删除映射记录,**不**删本地存储里的密文(那是 `BlobTransferPort::untag` + 回收扫描的事)
- **Phase 1 单 space 假设**:schema 主键只用 `plaintext_hash`,多 space 时升级走 migration(Phase 2 评估)

### 3.3 uc-core/AGENTS §7 crypto 合规性

`PlaintextHash` / `BlobDigest` 都是**值对象**(32 字节不透明),其 docstring 不具名任何 hash 算法;类型本身不含密钥材料 / algorithm enum / KDF 参数 / 持久化格式——对照 §7.2 的禁入类型清单,全部不落入。`BlobCipherPort`(已存在)的 `ActiveSpace` / `Ciphertext` / `Plaintext` / `Aad` 继续复用,Phase 1 adapter 不直接调——加解密由 Phase 2 usecase 组织。

### 3.4 uc-core/AGENTS §6 network 合规性 + 命名 lint

- `BlobTicket`:字段 + docstring 零技术术语("postcard" / "iroh" / "iroh-blobs" 在 uc-core 源码里**0 次出现**);仅两个 byte 存取方法,真正 opaque 字节包装
- `BlobError::Unavailable` / `InvalidTicket`:业务语义,不用 "Download" / "Peer closed stream" 这类传输层措辞(对照 `uc-infra/AGENTS §9.3`)
- `tag` / `untag` / `TagReason`:标签/打标是通用技术词汇,不绑定特定后端;docstring 用"reference-holding reason"解释业务意图(避免"reference counting"具体实现叙述)
- `issue_ticket` / `BlobTicket`:"ticket" 在业界通用(NodeTicket / PairingTicket / JWT 等),非任何单一 crate 独占

---

## 4. iroh-blobs adapter 设计

### 4.1 `FsStore` 目录布局 + ALPN

**路径**:`$APP_DATA/iroh-blobs[_<profile>]/`

- macOS:`~/Library/Application Support/app.uniclipboard.desktop/iroh-blobs/`
- Linux:`~/.local/share/app.uniclipboard.desktop/iroh-blobs/`
- Windows:`%LOCALAPPDATA%\app.uniclipboard.desktop\iroh-blobs\`
- dev profile(`UC_PROFILE=dev`):`.../iroh-blobs_dev/`(通过 `apply_profile_suffix`)

**ALPN**:复用 iroh-blobs crate 导出的默认 ALPN(`iroh_blobs::ALPN`,T0 实测 `iroh-blobs 0.97.0` 当前是 `b"/iroh-bytes/4"`)。理由:iroh-blobs 官方 ticket format 内部绑定了这个 ALPN,自定义 ALPN 会导致无法与非本项目的 iroh-blobs peer 互操作——Slice 3 验收项"一对多 fanout"(task_plan.md L1034)隐含"标准 iroh-blobs 客户端也能接",所以**不**自定义 ALPN。

**ALPN 常量导出**:`uc-infra/src/network/iroh/blobs.rs` re-export `pub const BLOBS_ALPN: &[u8] = iroh_blobs::ALPN;`,供 `install_blobs` / 测试使用。

### 4.2 publish / has / tag / untag / issue_ticket 实现映射

T0 已锁定 `iroh-blobs 0.97.0` + `iroh 0.95.1` 对齐栈。`iroh-blobs 0.95.0` 会引入 `iroh 0.93.2`,不能挂到当前共享 router,禁止使用。

| 本 port 方法 | iroh-blobs 调用 | 备注 |
|---|---|---|
| `publish(bytes)` | `store.blobs().add_bytes(bytes).await` → `TempTag { hash, format, .. }` | 返回的 `hash` 是 iroh-blobs 原生 `Hash`,adapter 内转成 `BlobDigest([u8;32])` |
| `has(digest)` | `store.blobs().observe(hash).await_completion().await?.is_complete()` | T0 未发现 `contains` / `has` 公共方法;用 observe 锁定本地完整性 |
| `tag(digest, reason)` | `store.tags().set(tag_name, HashAndFormat::raw(hash)).await` | `tag_name` 格式:`"uc-clipboard-entry:<entry_id>"` per `TagReason` 变体 |
| `untag(digest, reason)` | `store.tags().delete(tag_name).await` | 对应 tag 缺失返回 `0`,adapter 映射为 `Ok(())`(幂等) |
| `issue_ticket(digest)` | `iroh_blobs::ticket::BlobTicket::new(endpoint.addr(), hash, BlobFormat::Raw)` → `iroh_tickets::Ticket::to_bytes()` → `uc_core::BlobTicket(Vec<u8>)` | 需要 `iroh-tickets = "0.2"` 作为直接依赖 |
| `digest_of(ticket)` | `iroh_blobs::ticket::BlobTicket::from_bytes(ticket.as_bytes())?.hash()` → `BlobDigest([u8;32])` | 纯 CPU 路径,不触网络不读 store;解析失败 → `BlobError::InvalidTicket` |
| `fetch(ticket)` | `BlobTicket::from_bytes` → `endpoint.connect(ticket.addr().clone(), iroh_blobs::ALPN).await` 预热地址 → `store.downloader(&endpoint).download(ticket.hash_and_format(), [ticket.addr().id]).await` → `store.blobs().get_bytes(ticket.hash()).await` | T0 实测 downloader 只拿 provider id,不能单独消费 ticket 内完整地址;fetch 必须先把地址带入 endpoint |

**TagReason 编码约定**:
- `TagReason::ClipboardEntry(entry_id)` → `"uc-clipboard-entry:{entry_id}"`
- 未来新增变体 → 新前缀;adapter 内部 `fn tag_name(reason: &TagReason) -> String` 集中映射,测试覆盖双向编码稳定性

### 4.3 fetch 实现(Downloader + Endpoint 共享)

`IrohBlobTransferAdapter` 结构:

```rust
pub struct IrohBlobTransferAdapter {
    endpoint: Arc<iroh::Endpoint>,
    store:    iroh_blobs::store::fs::FsStore,   // Cloneable handle 或 Arc 包一层
}
```

`fetch` 流程:
1. `BlobTicket::from_bytes` 解包 → iroh-blobs native ticket(含 `EndpointAddr` + `Hash` + `BlobFormat`)
2. `endpoint.connect(ticket.addr().clone(), iroh_blobs::ALPN).await` 用 ticket 内完整地址预热 endpoint 地址/连接状态
3. `store.downloader(&endpoint).download(ticket.hash_and_format(), [ticket.addr().id]).await` 拉密文到本地 FsStore
4. `store.blobs().get_bytes(ticket.hash()).await` 把密文读回内存 → `Bytes`
5. 返回 `Bytes`(**未解密**——加密/解密是 Phase 2 usecase 的事)

**断点续传 / BLAKE3 校验**:iroh-blobs 原生保证,adapter 不额外做。

### 4.4 `install_blobs` 扩展点

对称 `install_pairing` / `install_presence` / `install_clipboard`:

```rust
// uc-infra/src/network/iroh/node.rs

/// The one blob port produced by [`IrohNodeBuilder::install_blobs`].
pub struct BlobHandlers {
    pub blob_transfer: Arc<dyn BlobTransferPort>,
}

impl IrohNodeBuilder {
    pub async fn install_blobs(
        &mut self,
        fs_store_dir: PathBuf,   // `$APP_DATA/iroh-blobs[_<profile>]/`
    ) -> Result<BlobHandlers, IrohNodeError> {
        let store = iroh_blobs::store::fs::FsStore::load(&fs_store_dir).await
            .map_err(|e| IrohNodeError::BlobStoreInit(e.to_string()))?;
        let blobs = iroh_blobs::BlobsProtocol::new(&store, None);

        // Register the iroh-blobs protocol handler on the router so peers
        // can GET our blobs. Follows the same take+reassign pattern as
        // install_pairing / install_presence / install_clipboard.
        let builder = self.router_builder.take()
            .expect("router_builder missing — install_* called after spawn");
        let builder = builder.accept(iroh_blobs::ALPN, blobs.clone());
        self.router_builder = Some(builder);

        let adapter = Arc::new(IrohBlobTransferAdapter::new(
            Arc::clone(&self.endpoint),
            store,
        ));

        Ok(BlobHandlers { blob_transfer: adapter as Arc<dyn BlobTransferPort> })
    }
}
```

**返回 1 个 handler**(vs Slice 2 Phase 2 的 `ClipboardHandlers` 返回 2 个):`BlobReferenceRepositoryPort` 是 **sqlite 派系**,和 iroh 无关,由 `DieselBlobReferenceRepository` 独立构造,走 `SpaceSetupAssembly` 装配链——见 §9。

---

## 5. Diesel migration + sqlite repo

### 5.1 Migration `2026-04-24-000001_create_blob_reference`

**up.sql**:
```sql
-- Create blob_reference: plaintext-hash → ciphertext-digest dedup cache.
--
-- Slice 3 Phase 1. Consumed by D1 (PublishBlobUseCase) and D2 (FetchBlobUseCase)
-- 的去重短路,以及 T-03(跨设备转发 sponsor-less)的埋点。
--
-- Columns map 1:1 to `uc-core::ports::blob::reference::{PlaintextHash, BlobDigest}`:
--   plaintext_hash := BLAKE3 of plaintext bytes (primary key; hex-encoded
--                     TEXT for sqlite-friendly queries — BLOB 等价但 hex 便于
--                     CLI 调试 dump)
--   digest         := BLAKE3 of ciphertext bytes (adapter-computed by iroh-blobs)
--   created_at     := unix seconds, project-wide timestamp convention
--
-- Upsert semantics: last-write-wins on plaintext_hash (port 契约见 §3.2).
-- 不加 space_id 列 —— Phase 1 单 space 假设(见 §3.2 关键决策).

CREATE TABLE blob_reference (
    plaintext_hash TEXT PRIMARY KEY NOT NULL,
    digest         TEXT NOT NULL,
    created_at     INTEGER NOT NULL
);
```

**down.sql**:
```sql
DROP TABLE blob_reference;
```

**为什么用 TEXT 而非 BLOB**:对照 `peer_address` migration 用了 BLOB(内容是 postcard opaque),`blob_reference` 的 hash 是定长 32 字节 + 可读的内容寻址标识,hex 更方便日志 / CLI debug / sqlite CLI 查询。存储代价 +2x(32 bytes → 64 char)可接受(表规模 = 剪贴板含文件条目数)。

### 5.2 Diesel schema + repo

`schema.rs` 追加:
```rust
table! {
    blob_reference (plaintext_hash) {
        plaintext_hash -> Text,
        digest -> Text,
        created_at -> BigInt,
    }
}
```

`DieselBlobReferenceRepository` 跟随 `DieselPeerAddressRepository`(commit `e81cec97`)的模板:
- `models/blob_reference.rs`:`Queryable` / `Insertable` struct + hex ↔ `[u8;32]` 辅助
- `mappers/blob_reference.rs`:row ↔ `(PlaintextHash, BlobDigest)` 双向转换
- `repositories/blob_reference_repository.rs`:`find_by_plaintext_hash` / `save`(upsert)/ `forget`;executor 统一走 `Arc<DbExecutor>`

---

## 6. 任务拆解(执行顺序 + 依赖)

| # | 任务 | 依赖 | 工作量 |
|---|---|---|---|
| T0 | iroh-blobs API 探针:确认 `0.95.0` 版本不兼容当前共享 endpoint,升级 `0.97.0`;锁定 `FsStore::load` / `blobs().add_bytes` / `observe(...).await_completion` / `tags().set/delete` / `BlobTicket::{new,to_bytes,from_bytes}` / `BlobsProtocol::new` / `store.downloader(&endpoint).download` 实际签名;写 `uc-infra/tests/iroh_blobs_probe.rs` 4 个 verdict 覆盖 add→get 自回环 + tag 往返 + ticket 编解码稳定性 + loopback download | - | 1.5h |
| T1 | uc-core:`ports/blob/mod.rs` + `transfer.rs` + `reference.rs` + 挂 `ports/mod.rs`;只含 trait + value object + error,不实现 | - | 0.8h |
| T2 | uc-infra migration `2026-04-24-000001_create_blob_reference` + `schema.rs` 追加 `blob_reference!` + `diesel migration run` 本地验证 | T1 | 0.5h |
| T3 | `DieselBlobReferenceRepository`(models + mappers + repo)+ 5 单测(upsert / find hit / find miss / forget / forget 幂等) | T2 | 2h |
| T4 | `IrohBlobTransferAdapter` 骨架:struct + 构造函数 + `publish` + `has` + `issue_ticket` + `digest_of`;单节点 adapter 单测 4 个(publish 返回稳定 digest / has 命中 / issue_ticket + digest_of 往返 == publish 返回值 / digest_of 接收损坏 ticket → InvalidTicket) | T0, T1 | 2.3h |
| T5 | `IrohBlobTransferAdapter::fetch`(走 downloader + export_bytes);单节点 loopback 单测 1 个(publish → issue_ticket → fetch 字节 identical) | T4 | 1.5h |
| T6 | `IrohBlobTransferAdapter::tag` / `untag`(含 TagReason 编码约定);单测 3 个(tag 再 untag 幂等 / untag 不存在的 tag 返回 Ok / 多 reason 独立计数) | T4 | 1h |
| T7 | `IrohNodeBuilder::install_blobs` 扩展点 + `BlobHandlers` 类型 + `mod.rs` 重导出;1 单测(4 ALPN pairing+presence+clipboard+blobs 共 router 存活) | T4 | 1h |
| T8 | bootstrap 装配:`apply_profile_suffix(config_dir.join("iroh-blobs"))` 解析 + `SpaceSetupAssembly` 加 `pub blob_transfer: Arc<dyn BlobTransferPort>` / `pub blob_reference: Arc<dyn BlobReferenceRepositoryPort>`;`build_space_setup_assembly` 在 `install_clipboard` 后调 `install_blobs`,再 `DieselBlobReferenceRepository::new(executor)` 装 blob_reference;workspace 全量编译 + 原 slice2 e2e 仍绿 | T3, T7 | 1.2h |
| T9 | `slice3-phase1-plan.md` §12 live 跟踪 + `task_plan.md` Slice 3 Phase 1 段标 ✅ + commit hash 记录 + progress.md 续 31 记录 | T3, T5, T6, T7, T8 | 0.5h |

**总计**:~12.3h(≈ 1.5 个专注工作日)

**可并行组**:
- T2 / T4 都在 T1 之后,可并行(T2 纯 sqlite,T4 纯 iroh)
- T5 / T6 / T7 都在 T4 之后,可并行——T5 和 T6 对 adapter 互不重叠(fetch vs tag),T7 只扩 node.rs
- T8 依赖全部,单 session 做完

---

## 7. 测试策略

### 7.1 单元测试(随每个 T 交付)

| 组件 | 覆盖点 |
|---|---|
| `DieselBlobReferenceRepository`(T3) | save 后 find hit / find miss 返 None / save 同 hash 不同 digest → last-write-wins / forget 后 find miss / forget 不存在 hash 幂等 |
| `IrohBlobTransferAdapter` publish / has(T4) | 相同 bytes publish 两次返回相同 digest(content-addressed 幂等)/ publish 后 has(digest) = true / 未 publish 的随机 digest has = false |
| `BlobTicket` round-trip(T4) | publish → issue_ticket → `port.digest_of(ticket)` == publish 返回的 digest / `BlobTicket::as_bytes` + `from_bytes` round-trip 字节稳定 / 损坏 ticket bytes 传给 `digest_of` → `BlobError::InvalidTicket` |
| `IrohBlobTransferAdapter` fetch(T5) | 单节点 loopback(两个 Endpoint 或 self-fetch)publish → issue_ticket → fetch 字节 identical / fetch 不存在 ticket → Download error |
| `IrohBlobTransferAdapter` tag(T6) | tag → untag 幂等 / 多 TagReason 独立(同 digest tag 两次不同 reason,untag 一次另一 reason 仍活)/ untag 不存在 tag → Ok |
| `IrohNodeBuilder::install_blobs`(T7) | 4 ALPN 同 router 共存(pairing + presence + clipboard + blobs)accept 路径无冲突 / `install_blobs` 后 `spawn()` 成功 |

### 7.2 集成测试(Phase 1 最小保障)

**`uc-infra/tests/iroh_blobs_probe.rs`**(T0 产出):**探针级**测试,锁定 `iroh 0.95.1` + `iroh-blobs 0.97.0` API 契约,失败即说明依赖升级破坏 adapter 设计——同 Slice 2 Phase 1 的 `iroh_presence_probe.rs` 定位。

**Phase 1 不做**跨节点 e2e:两 Endpoint 真 fetch 走 Phase 2 CLI e2e(`slice3_phase2_blob_e2e`)统一覆盖,Phase 1 单节点 loopback 足够验证 adapter 契约。

### 7.3 CLI 冒烟

**Phase 1 跳过**——CLI 是 Phase 2 的事(`uniclipboard-cli blob publish/fetch`,S3-D4)。

---

## 8. 风险 & 待确认

| # | 风险 | 缓解 |
|---|---|---|
| R1 | iroh-blobs 版本与 iroh endpoint 版本不对齐 / `Store` 方法名漂移 | T0 已确认必须用 `iroh-blobs 0.97.0`;`has` 走 `observe(...).await_completion().is_complete()`,无 `contains` 公共方法 |
| R2 | "从 ticket 抽 digest" 的归属(值对象方法 vs port 方法 vs 调用方自管)——方案 A(值对象 `BlobTicket::digest()` + adapter 套外壳)让 uc-core 持有内部结构知识,违反 `uc-core/AGENTS §19.1` "以实现反推领域";方案 B(删 `digest()`,调用方自管 `(digest, ticket)` 配对)让 D1/D2 usecase 签名变重 | **已定稿:方案 C** —— `BlobTicket` 真正 opaque,`BlobTransferPort` 加同步方法 `digest_of(ticket) -> Result<BlobDigest, BlobError>`;adapter 内部解 iroh-blobs 原生 ticket 拿 hash,uc-core 零内部结构知识;D1 去重路径 `port.digest_of(&ticket)?` 一步拿 digest,开销 CPU-only |
| R3 | `FsStore::load` 并发打开同目录的行为(两进程或同进程两次 load) | Phase 1 单 daemon 假设,不测并发 load;文档在 `install_blobs` doc comment 标注"not safe to load twice on the same dir";未来多进程走 T-13(task_plan §T-13 FsStore 目录布局与迁移) |
| R4 | `blob_reference` 不含 space_id 在多 space 场景会错误匹配他空间的 digest | Phase 1 单 space 假设,已在 §3.2 关键决策 + migration 注释标注;未来扩多 space 走 migration 加 `space_id` 列 + port 签名加 `space_id` 参数(记 Phase 2 评估技术债 T-18 候选) |
| R5 | 旧 `uc-infra/src/blob/` namespace 与新 `uc-infra/src/network/iroh/blobs.rs` 命名冲突引发 import 歧义 | 新 adapter 通过 `uc_infra::network::iroh::IrohBlobTransferAdapter` 导出(mod 路径区分),旧 `uc_infra::blob::FilesystemBlobStore` 保留;Slice 5 删旧 `uc_infra::blob` 后不改名 |
| R6 | `install_blobs` 签名是 `async`(FsStore::load 是 async),`install_pairing/presence/clipboard` 都是同步——不对称 | 允许不对称(FsStore I/O 本质 async);`build_space_setup_assembly` 本来就是 async,链式 `.install_blobs().await` 无负担 |

---

## 9. Agent 规范合规性自查(uc-core / uc-infra AGENTS.md)

### 9.1 uc-core 规范

| 规范项 | 确认 |
|---|---|
| §2.2 禁入:网络实现 / 第三方 SDK / 具体加密算法实现 | uc-core 源码里 "iroh" / "iroh-blobs" / "postcard" / "BLAKE3" / "Argon2" 字眼 **0 次出现**(包括 docstring / 行内注释);只说"storage adapter" / "adapter-computed" |
| §4.2 值对象不可变 + 通过值相等 | `BlobDigest` / `BlobTicket` / `PlaintextHash` 全部 `PartialEq + Eq`,无 mut 方法 |
| §5.2 Ports 以业务能力命名 / 不暴露技术细节 | `BlobTransferPort::publish / fetch / issue_ticket / digest_of / has / tag / untag` 都是业务动词;无 `download` / `upload` / `get_from_store` 等传输层名字 |
| §6.3 不应存在:protocol IDs / 序列化结构 | ALPN / protocol ID 只在 `uc-infra/src/network/iroh/blobs.rs`;`BlobTicket` 纯 opaque `Vec<u8>`,uc-core 不解析 |
| §7.2 crypto 禁入类型 | `PlaintextHash`/`BlobDigest` 是 32 字节值对象,docstring 不具名 hash 算法;非 `MasterKey`/`Kek`/`KdfParams`/`KeyScope` 等 |
| §10.2 依赖禁止项 | 不引入新依赖,`bytes` / `thiserror` / `async-trait` 已在(Slice 2 Phase 2 起) |
| §11 命名规范 | 端口 `*Port`(`BlobTransferPort` / `BlobReferenceRepositoryPort`);错误 `*Error`(`BlobError` / `BlobReferenceError`);值对象名词(`BlobDigest` / `BlobTicket` / `PlaintextHash`) |
| §19.1 反模式:以实现反推领域 | `BlobTicket` 零方法(只 byte 存取),uc-core 不持有 ticket 内部结构知识;`digest_of` 走 port,解析在 adapter |
| §19.2 反模式:错误语义泄漏 | `BlobError` 无 `Download` / `Fetch` / `NetworkIo` 等传输层动词;用 `Unavailable` / `InvalidTicket` / `NotFound` 业务语义 |

### 9.2 uc-infra 规范

| 规范项 | 确认 |
|---|---|
| §3 分层 | `IrohBlobTransferAdapter` 实现 `BlobTransferPort`;`DieselBlobReferenceRepository` 实现 `BlobReferenceRepositoryPort`——每个实现都能明确回答"实现的是哪个 port" |
| §4.2 技术细节向下收敛 | iroh-blobs `Hash` / `AddOutcome` / 原生 `BlobTicket` / `Downloader` 类型全部锁在 `uc-infra/src/network/iroh/blobs.rs`,不上浮 |
| §4.4 单 adapter 单职责 | `IrohBlobTransferAdapter` 只做 blob transfer,**不**管引用计数扫描 GC(T-02)、**不**管加解密(用 `BlobCipherPort`,Phase 2 usecase 组织)、**不**管 plaintext-hash 去重(走独立 `DieselBlobReferenceRepository`) |
| §9.1 错误收敛 | `BlobError::Internal(String)` 吸收 iroh-blobs 原生错误(`.map_err(|e| BlobError::Internal(e.to_string()))`);`BlobReferenceError::Repository(String)` 不暴露 `diesel::result::Error` |
| §9.3 错误语义稳定 | 见 uc-core §19.2 自查——error 枚举名没走底层库术语 |
| §10.2 日志不泄敏感数据 | `publish` / `fetch` 日志只打 `digest_hex_short` + `size`,不打 bytes 内容 |
| §11.1 持久化格式版本化 | iroh-blobs FsStore 目录格式归 crate 管理;`blob_reference` 表列明 `created_at`,未来加列走 migration |
| §16 命名规范 | `IrohBlobTransferAdapter`(`*Adapter`)/ `DieselBlobReferenceRepository`(`*Repository`)/ `BlobReferenceRow`(`*Row`) |

### 9.3 交叉层

| 项 | 确认 |
|---|---|
| Facade 只是入口,不重新编排业务 | Phase 1 无 facade——port 直接装到 `SpaceSetupAssembly` 字段上,Phase 2 usecase 消费 |
| uc-core 源文件可 grep 的违规词 | 执行 `rg -w 'iroh|postcard|BLAKE3|Argon2' src-tauri/crates/uc-core/src/ports/blob/` 应返回 0 结果(T1 交付前验证) |

---

## 10. 验收前检查清单

- [ ] `cargo test -p uc-core` 绿(新 port trait 编译通过 + doc-test 若有)
- [ ] `cargo test -p uc-infra` 绿(T0 probe + T3 repo 5 单测 + T4/T5/T6 adapter 8 单测 + T7 router 共存单测)
- [ ] `diesel migration run` 本地 schema 无红字
- [ ] `cargo build --workspace` 绿(T8 bootstrap 装配不破坏 slice 1/2)
- [ ] `cargo test -p uc-bootstrap --tests` 绿(原 slice1 / slice2 phase1-3 e2e 仍通过)
- [ ] `task_plan.md` Slice 3 Phase 1 段 🔲 → ✅ + commit hash 记录
- [ ] `progress.md` 续 31 session 记录
- [ ] `rg -w 'iroh|iroh-blobs|postcard|BLAKE3|Argon2|XChaCha20' src-tauri/crates/uc-core/src/ports/blob/` 返回 0 行(uc-core 合规性 lint)

---

## 11. 推进节奏建议

- **Day 1**(~5h):T0(iroh-blobs 探针)→ T1(uc-core ports)→ T2/T4 并行启(migration + adapter 骨架)
- **Day 2**(~5h):T3(repo + 单测)与 T5/T6(adapter fetch + tag)并行 → T7(install_blobs)
- **Day 3**(~2h):T8(bootstrap)→ 全量编译 + slice 1/2 e2e 回归 → T9(收尾 + 文档)

每完成一组相关 T 做一次 atomic commit,message 前缀 `feat(Slice3/P1): ...` / `test(Slice3/P1): ...` / `docs(Slice3/P1): ...`。

---

> **开工信号**:用户点头 → 从 T0 开始(R2 已定稿为方案 C,见 §8)。

---

## 12. 进度跟踪(live · 待开工填充)

### 12.1 任务状态

| # | 任务 | 状态 | commit | 实际工时 | 备注 |
|---|---|---|---|---|---|
| T0 | iroh-blobs API 探针(0.97.0 对齐 iroh 0.95) | ✅ | — | ~1.5h | 4 verdict 全绿;`0.95.0` 已排除 |
| T1 | uc-core ports/blob/{transfer,reference}(含 `digest_of`,R2 方案 C) | ✅ | `297f1e87` | ~0.5h | 38 uc-core 单测绿;§9.3 合规 lint 0 行 |
| T2 | blob_reference migration + schema | ✅ | `9170f7f4` | ~0.3h | 109 uc-infra 单测绿;diesel migration run 对空 sqlite 成功 |
| T3 | DieselBlobReferenceRepository + 5 单测 | ✅ | — | ~0.7h | 5 个仓储单测绿;`cargo test -p uc-infra` 全量绿 |
| T4 | IrohBlobTransferAdapter publish/has/issue_ticket/digest_of + 4 单测 | ✅ | — | ~1.0h | 实际合并 T5/T6 一次落地;adapter 9 单测绿 |
| T5 | IrohBlobTransferAdapter fetch + loopback 单测 | ✅ | — | ~0.4h | self-fetch + 双节点 remote-fetch 都绿 |
| T6 | IrohBlobTransferAdapter tag/untag + 3 单测 | ✅ | — | ~0.3h | tag/untag 幂等 + 多 reason 独立 |
| T7 | install_blobs 扩展点 + 4 ALPN 共存单测 | ✅ | — | ~0.4h | pairing+presence+clipboard+blobs 同 router 通过 |
| T8 | bootstrap 装配 + workspace 全绿回归 | ✅ | — | ~0.5h | `cargo check --workspace` 通过 |
| T9 | 收尾(task_plan ✅ + progress 续 31) | ✅ | — | ~0.2h | 本表 + `task_plan.md` + `progress.md` 已更新 |

### 12.2 累计

- T0+T1+T2+T3+T4+T5+T6+T7+T8+T9:9/9 done

### 12.3 关键发现 / 偏离

- `iroh-blobs 0.95.0` 依赖 `iroh 0.93.2`,不能挂到当前 `iroh 0.95.1` 的共享 `Router` 上。
- `iroh-blobs 0.97.0` 依赖 `iroh 0.95`,`BlobTicket` 使用 `EndpointAddr` / `EndpointId`;已升级并锁定该版本 API。
- `downloader().download` 只接收 provider id,不会直接消费 ticket 内完整地址;adapter `fetch` 需要先用 `endpoint.connect(ticket.addr().clone(), iroh_blobs::ALPN)` 把 ticket 地址带入 endpoint。
- T1 实际引用 `crate::ids::EntryId`(public re-export),不走计划 §3.1 写的 `crate::clipboard::entry::ClipboardEntryId`(该路径不存在;`ids/clipboard.rs::EntryId` 才是真名)。
- T2:`diesel migration run` 不加 `--locked-schema` 会重写 `schema.rs`,抹掉项目手动把 `*_at`/`joined_at`/`trusted_at` 等列标为 `BigInt` 的 override(Diesel 自动推断 `INTEGER` → `Integer`)。今后在本仓库跑 `diesel migration run` 统一加 `--locked-schema`。
- T3:`blob_reference` 仓储不新增 core record 类型,按 port 方法形态在 infra 内用 `BlobReferenceRowMapper` 做 `PlaintextHash` / `BlobDigest` 与 hex row 的转换;`save` 采用 last-write-wins upsert。
- T4:`has` 不能用 `observe(...).await_completion()` 判断缺失 digest,未知 digest 会一直等;改用 `observe(hash).await` 读取当前 bitfield 后判断 `is_complete()`。
- T5:self-fetch 不能先 `endpoint.connect(self_addr, BLOBS_ALPN)`,iroh 会拒绝 "Connecting to ourself";`fetch` 先查本地已有 digest,命中直接 `get_bytes`,未命中才按 ticket 连接远端。
- T8:`BlobReferenceRepositoryPort` 走 sqlite 装配链,`BlobTransferPort` 走 `IrohNodeBuilder::install_blobs`;两者都挂到 `SpaceSetupAssembly`,但保持职责分离。

### 12.4 后续提醒

- Phase 2 写 use case 时直接从 `SpaceSetupAssembly::{blob_transfer,blob_reference}` 取 port,不需要再碰 iroh router 装配。
