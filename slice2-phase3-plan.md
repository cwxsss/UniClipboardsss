# Slice 2 Phase 3 — daemon 接管 iroh 剪贴板同步

> 把 Phase 2 的 `ClipboardSyncFacade` 接到 daemon 的 `ClipboardWatcherWorker` / `InboundClipboardSyncWorker`,完成"系统剪贴板复制 → 自动 dispatch → 对端落库 + 写系统剪贴板"闭环;CLI `send` / `watch` 退化为可选验收工具。

---

## 1. 目标 + 验收

**单句**:用户在 A 复制文字,**不用任何命令**,B 端系统剪贴板 ≤ 2s 内被同样的内容覆盖,且双端 `ClipboardEntry` 历史库都有这条记录。

**验收条款(plan §1.1 / §15)**:
- [ ] A daemon 跑着,B daemon 跑着,A 用户复制文字 → ≤ 2s 内 B 系统剪贴板被覆盖,内容字节级相等
- [ ] 双端 daemon 都把这条 entry 落 `ClipboardEntry` 库,B 侧 `clipboard.new_content` WS 事件 fire 一次(`origin: "remote"`)
- [ ] B 端 daemon **不**会因为自己刚写系统剪贴板就反向 dispatch(回环防御 `ClipboardChangeOriginPort` 仍然生效)
- [ ] A 复制同样的内容两次 → B 系统剪贴板仍只被写一次,DB 也只多 1 条 entry(应用层 dedup)
- [ ] B daemon 离线时,A 复制 → A 端 daemon log 出 `0 accepted, 0 offline=1`,**不 panic**;B 重启后下一次 A 复制能收到
- [ ] CLI `send` / `watch` 仍可工作(验收工具不删,daemon mode 优先;Slice 5 才决定是否删)
- [ ] **没有 deprecated `ClipboardOutboundTransportPort` / `ClipboardInboundTransportPort` 的活跃消费者**(Phase 3 完工 = Slice 5 删 deprecated 的前置条件全部到位)

---

## 2. 范围(in scope / out of scope)

**in scope**:
1. **wire payload envelope V3 落地**:Phase 2 走 raw text bytes,Phase 3 sender 必须 wrap 成 `ClipboardBinaryPayload V3`(已在 `uc-core::network::protocol`),receiver 必须解 envelope → `Vec<BinaryRepresentation>` 才能写系统剪贴板。这是从"测试工具"升级到"daemon 产品"的必要 codec 升级
2. **daemon 装配 `ClipboardSyncFacade`**:`build_daemon_app` 输出 + `DaemonBootstrapContext` 字段 + entrypoint 注入到两个 worker
3. **`DaemonClipboardChangeHandler` 改装**(出站):`build_sync_outbound_clipboard_use_case()` 删除,改用 `clipboard_sync.dispatch_entry`;policy 过滤(global auto_sync 主开关)在 daemon 里短路,**per-member 偏好推 follow-up**
4. **`InboundClipboardSyncWorker` 改装**(入站):订阅源从 `ClipboardInboundTransportPort` → `clipboard_sync.subscribe_inbound_notices`;消息处理从 `SyncInboundClipboardUseCase` → 新的 `ApplyInboundClipboardUseCase`(decode envelope → persist + dedup → 写 OS 剪贴板 via `ClipboardWriteCoordinator` → emit WS)
5. **回环防御维持**:`ClipboardChangeOriginPort` 单例继续在两 worker 间共享;`ClipboardWriteCoordinator.write` 在写 OS 前注册 `RemotePush` guard,watcher 消费时跳过
6. **dedup 在应用层**:`ApplyInboundClipboardUseCase` 查 `ClipboardEntryRepositoryPort.exists_by_content_hash(...)`(若已存在 → 落 `Skipped(DuplicateLocalEntry)`,不写 OS,不 emit WS,不 broadcast)
7. **CLI `send` / `watch` 同步升级**:都用 envelope codec;CLI 端解 envelope 显示 representation 摘要

**out of scope**(明确推 Phase 3.5 / Slice 4 / Slice 5):
- **per-member sync preferences**(`MemberSyncPreferences.send_enabled` / `send_content_types`):Phase 3 简化为"发给所有 paired peer";policy 模块迁移到 facade 是 Phase 3.5
- **wire `DuplicateIgnored` ack**:receiver adapter 层 dedup 需要持久化层下沉,推 Phase 3.5;Phase 3 应用层 dedup 已能满足 acceptance #4
- **A3 revoke / A5 rename UI**:Slice 2 root 列表项,但是独立动作,Phase 4(Tauri 重接)再启动
- **大 payload(图片 / 富文本 / 文件)**:`MAX_PAYLOAD_SIZE=2MiB` 上限不变,>2MiB 的图片走 Slice 3 blob ticket 路径
- **删 deprecated transport ports**:Slice 5,前置条件 = Phase 3 + Slice 4 双栈并行验证
- **daemon 接收方 search index**:`DaemonClipboardChangeHandler` 走 LocalCapture 路径已 index;remote 入侵 entry 是否进 index 跟 Phase 2 双栈相比无差异,沿用现状

---

## 3. 文件改动地图

```text
新增:
  src-tauri/crates/uc-application/src/usecases/
    clipboard_capture/                🆕 (T0a) 迁自 uc-app/src/usecases/internal/capture_clipboard.rs
      mod.rs
      usecase.rs                       (含原 CaptureClipboardUseCase + 单测)
    clipboard_write/                  🆕 (T0b) 迁自 uc-app/src/usecases/clipboard/clipboard_write_coordinator.rs
      mod.rs
      coordinator.rs                   (含原 ClipboardWriteCoordinator + 单测)
    clipboard_sync/
      apply_inbound.rs                 🆕 (T4) ApplyInboundClipboardUseCase + ApplyOutcome
      payload_codec.rs                 🆕 (T2) encode_snapshot_to_v3 / decode_v3_to_snapshot + content_hash 工具

  src-tauri/crates/uc-bootstrap/src/
    (无新文件,改 builders.rs / space_setup.rs)

迁移 + shim(T0a / T0b):
  src-tauri/crates/uc-app/src/usecases/internal/capture_clipboard.rs
                                      改为 deprecated re-export shim:
                                      pub use uc_application::usecases::clipboard_capture::CaptureClipboardUseCase;
                                      #[deprecated(since="Slice2-Phase3", note="moved to uc-application")]
  src-tauri/crates/uc-app/src/usecases/clipboard/clipboard_write_coordinator.rs
                                      同形 shim,re-export ClipboardWriteCoordinator

修改:
  src-tauri/crates/uc-application/src/usecases/clipboard_sync/mod.rs
                                      pub use apply_inbound::* / payload_codec::*

  src-tauri/crates/uc-application/src/facade/clipboard/facade.rs
                                      新方法 dispatch_snapshot(snapshot, origin) — 内部调 payload_codec
                                      新方法 subscribe_inbound_decoded() — emit DecodedInboundNotice (含 representations)
                                      或保留 subscribe_inbound_notices,decode 在 ApplyInboundClipboardUseCase 内做

  src-tauri/crates/uc-application/src/facade/mod.rs
                                      re-export ApplyInboundClipboardUseCase / ApplyOutcome

  src-tauri/crates/uc-bootstrap/src/builders.rs
                                      build_daemon_app: 把 SpaceSetupAssembly.clipboard_sync 装进
                                      DaemonBootstrapContext 新字段 clipboard_sync_facade: Arc<ClipboardSyncFacade>

  src-tauri/crates/uc-daemon/src/entrypoint.rs
                                      从 ctx 拿 clipboard_sync_facade;构造两 worker 时注入

  src-tauri/crates/uc-daemon/src/workers/clipboard_watcher.rs
                                      DaemonClipboardChangeHandler 加 clipboard_sync 字段
                                      build_sync_outbound_clipboard_use_case 删除
                                      on_clipboard_changed 末尾的 outbound dispatch 改调 clipboard_sync.dispatch_snapshot

  src-tauri/crates/uc-daemon/src/workers/inbound_clipboard_sync.rs
                                      整个 worker 重写:订阅源改 clipboard_sync.subscribe_inbound_notices
                                      run_receive_loop 改用 ApplyInboundClipboardUseCase
                                      parse_clipboard_frame 删除(envelope decode 由 use case 负责)

  src-tauri/crates/uc-cli/src/commands/send.rs
                                      send 之前先把 plaintext wrap 成 V3 envelope:format_id="text/plain", mime=Some("text/plain")
                                      content_hash 改算 plaintext 而不是 envelope bytes(应用层语义)

  src-tauri/crates/uc-cli/src/commands/watch.rs
                                      收到 notice 后 decode envelope,展示 first text representation
                                      JSON 模式输出多 representation 摘要

  src-tauri/crates/uc-bootstrap/tests/slice2_phase2_clipboard_e2e.rs
                                      plaintext bytes 改成 V3 envelope bytes;断言 decode 后字节相等
                                      改名 → slice2_phase3_clipboard_e2e.rs?(看 Phase 3 决策,推荐保留旧文件 + 加 phase3 文件)

  scripts/test_clipboard_e2e.sh
                                      更新 watch JSON 解析:从 "plaintext_utf8" 字段改成 "text" 或新 schema

不改:
  src-tauri/crates/uc-app/src/usecases/clipboard/sync_outbound.rs    (deprecated, Slice 5 删)
  src-tauri/crates/uc-app/src/usecases/clipboard/sync_inbound.rs     (deprecated, Slice 5 删)
  uc-core/src/ports/clipboard/transport.rs                            (deprecated, Slice 5 删)
  uc-core/src/network/protocol/clipboard_payload_v3.rs                (复用既有 V3 codec)
```

---

## 4. 关键决策(待 user 在编码前确认)

### D1 · daemon 替换 vs 双栈并行

**问题**:Phase 3 daemon 的 clipboard 路径是**完全替换**(libp2p 路径不再 spawn)还是**双栈并行**(两个 worker 都跑,iroh 优先)?

**推荐:完全替换**。理由:
- Slice 1 已经把 pairing 从 libp2p 切到 iroh;daemon 的 clipboard worker 是"剪贴板 outbound/inbound" 的最后 libp2p 消费者
- 双栈并行带来重复 dispatch / 双倍解密 / WS event 重复 等一堆问题
- Slice 4("双栈并行验证")的语义是"feature flag 切换",不是同时跑两栈
- deprecated transport ports 留着不删(Slice 5 处理),消除消费者就够

**风险**:Phase 3 出炉那一刻,libp2p 栈在 daemon 内不再驱动 clipboard;一旦 iroh 路径有 bug → 用户失去剪贴板同步。缓解:Phase 3 验收最后一条要求"单机 daemon 双 profile 走通",bug 暴露在 verification 不进生产。

### D2 · payload envelope 落地强度

**问题**:V3 envelope 是 Phase 3 必做还是可推?

**推荐:必做**。理由:
- daemon 接管后,inbound 必须能把字节还原成 `BinaryRepresentation` 才能调 `ClipboardWriteCoordinator.write(snapshot, intent)`;没 envelope 就不知道 mime / format_id
- Phase 2 raw bytes 模式只对 CLI 验收成立,产品层面不存在"无 envelope 路径"
- V3 codec 已经写好(`uc-core::network::protocol::clipboard_payload_v3`),只是 sender 没用

**副作用**:Phase 2 e2e + CLI 都得改。算到 Phase 3 工时里,接受。

### D3 · per-member sync preferences 在 Phase 3 还是 follow-up?

**问题**:`SyncOutboundClipboardUseCase.apply_sync_policy` 实现了 master toggle + per-member preferences + content_type filter。Phase 3 的 `dispatch_entry` 没有这层。Phase 3 是否要把这部分迁过来?

**推荐:Phase 3 只迁 master toggle(global `auto_sync`),per-member 推 Phase 3.5**。理由:
- per-member preferences 是 A3 revoke / A5 rename 之后的 fine-tuning,Phase 3 验收只关心"daemon 能不能基本跑通同步"
- master toggle 对单元测试不友好(需要载入完整 settings),但是用户层面唯一会动的开关
- per-member 迁移涉及 `ClipboardSyncFacade.dispatch_entry` 增加一个 `target_filter` 或 `dispatch_entry_to_targets(targets, ...)` 方法,接口面要慎重设计 → 推到 Phase 3.5 单独讨论

**最小实现**:`DaemonClipboardChangeHandler` 在 dispatch 之前 load settings.sync.auto_sync,false 直接 return + log。

### D4 · 应用层 dedup 在哪?

**问题**:Phase 3 acceptance #4 要求重复内容只落库一次。dedup 逻辑放哪?

**推荐:`ApplyInboundClipboardUseCase` 内,通过 `ClipboardEntryRepositoryPort.exists_by_content_hash(hash)` 判定**。理由:
- receiver adapter 层做 dedup 需要持久化层下沉,违 §11.4 + uc-infra AGENTS §6.2(infra 不该懂业务规则)
- ingest use case 现在只 decrypt + broadcast,不该承担持久化判定
- 新建 `ApplyInboundClipboardUseCase` 是"daemon 落地一条 inbound entry"的应用动作,持有 `ClipboardEntryRepositoryPort` + `ClipboardEventWriterPort` + `ClipboardWriteCoordinator`,dedup 是它的天然职责

**副作用**:`exists_by_content_hash` 这个方法 `ClipboardEntryRepositoryPort` 上有没有? 如果没,Phase 3 顺手加一个(返回 `Result<bool, ...>`)。

### D5 · `ClipboardEventWriterPort` 还是 `CaptureClipboardUseCase`?

**问题**:remote inbound 落库,沿用 daemon 现有的 `CaptureClipboardUseCase`(persist + spool + emit event)还是直接写 `ClipboardEventWriterPort.insert_event`?

**推荐:复用 `CaptureClipboardUseCase`,但是用一个 origin 区分**(LocalCapture / RemotePush)。理由:
- `CaptureClipboardUseCase.execute_with_origin` 已经接受 `ClipboardChangeOrigin` 参数
- 沿用同一条 capture pipeline 保证 schema 一致(spool / cache / event_repo 都走过)
- 唯一差别是 `event_id`(LocalCapture 算本机 hash,RemotePush 用 origin_device_id)

**副作用**:current `CaptureClipboardUseCase.execute_with_origin` 接受 `SystemClipboardSnapshot`,所以 `ApplyInboundClipboardUseCase` 内部要把 V3 envelope decode 回 `SystemClipboardSnapshot`,这是个轻量构造。

### D6 · ClipboardSyncFacade 的接口扩展

**问题**:`dispatch_entry(plaintext, content_hash, payload_version)` 的签名是 Phase 2 raw-bytes 时代的;Phase 3 需不需要新方法 `dispatch_snapshot(snapshot, origin)` 来包装 envelope encoding?

**推荐:加新方法 `dispatch_snapshot`,保留 `dispatch_entry` 给 CLI 用**:
```rust
impl ClipboardSyncFacade {
    /// Phase 3 daemon 路径:接 SystemClipboardSnapshot,内部 encode V3 envelope + 算 content_hash
    pub async fn dispatch_snapshot(&self, snapshot: SystemClipboardSnapshot, origin: ClipboardChangeOrigin)
        -> Result<DispatchEntryOutcome, ClipboardSyncError>;

    /// Phase 2 CLI 路径(retained):raw bytes + 显式 content_hash,caller 负责 envelope
    pub async fn dispatch_entry(&self, input: DispatchEntryInput)
        -> Result<DispatchEntryOutcome, ClipboardSyncError>;
}
```

CLI Phase 3 升级后,`send` 可以构造一个伪 snapshot(单 text representation)走 `dispatch_snapshot`,或者继续走 `dispatch_entry` + 自己 encode。Phase 3 文档里说明 CLI 推荐路径是 `dispatch_snapshot`。

### D8 · 依赖 usecase 的 crate 归属(用户补充)

**问**:`ApplyInboundClipboardUseCase`(新写在 `uc-application`)会依赖 `CaptureClipboardUseCase` + `ClipboardWriteCoordinator`,两者当前都在 **`uc-app`**(legacy,正在退役)。依赖方向怎么办?

**推荐:迁移两者到 `uc-application`,`uc-app` 保留 deprecated re-export shim**。理由:
- `uc-application` 是新 app 层;`uc-app` 正在退役(D13 milestone,Slice 5/6 彻底删)
- 反向依赖(uc-application 反过来 import uc-app)会让 uc-app 退役更难,违 §3 依赖方向("应用层应该向前演进")
- `uc-app` 现存 consumer(daemon / tauri / bootstrap 等 18 个文件)不能一次性改;shim 方案兼容老 import 路径,编译面 0 破坏

**迁移范围**(T0a / T0b 两任务,T4 前置):
- `uc-app/src/usecases/internal/capture_clipboard.rs` → `uc-application/src/usecases/clipboard_capture/`(保持 `pub struct CaptureClipboardUseCase`,只换 crate)
- `uc-app/src/usecases/clipboard/clipboard_write_coordinator.rs` → `uc-application/src/usecases/clipboard_write/`
- 在老路径加 `pub use uc_application::...::*;` deprecated shim(`#[deprecated(since = "Slice2-Phase3", note = "moved to uc-application")]`)
- 所有 `uc-application` 内新代码直接 import 新路径;`uc-app` / daemon / tauri / bootstrap 等 18 个老 consumer 暂不动,shim 吞掉

**走别的代价**:
- (a) `uc-application` 反向 import `uc-app`:uc-app 退役前永远拔不掉;违规
- (b) 复制一份到 `uc-application` 长期两份:维护倾斜;迟早 drift
- (c) Phase 3 内把所有 18 个 consumer 全改到新路径:工时 +~3h,scope 吃不下

**副作用**:Phase 3 scope +~2h,但这笔账 Slice 5 uc-app 退役时总要还,早还早清

---

### D7 · subscribe_inbound_notices 返回类型

**问题**:Phase 2 `InboundNotice` 字段是 `plaintext: Bytes`(raw)。Phase 3 需要 envelope decode 后的 `Vec<BinaryRepresentation>`。改 `InboundNotice` schema?

**推荐:不改 `InboundNotice`,在 `ApplyInboundClipboardUseCase` 内 decode**。理由:
- `InboundNotice` 是 facade 公开类型,改 schema 是 breaking change(CLI watch 也得跟改)
- envelope decode 是应用层职责,放 use case 内更干净
- 后续若 CLI 想直接看 representations,再加一个 `subscribe_inbound_decoded()` 方法

---

## 5. `ApplyInboundClipboardUseCase` 设计

```rust
// uc-application/src/usecases/clipboard_sync/apply_inbound.rs

pub(crate) struct ApplyInboundClipboardUseCase {
    entry_repo: Arc<dyn ClipboardEntryRepositoryPort>,
    capture_uc: Arc<CaptureClipboardUseCase>,
    write_coordinator: Arc<ClipboardWriteCoordinator>,
    clock: Arc<dyn ClockPort>,
}

#[derive(Debug, Clone)]
pub enum ApplyOutcome {
    /// 新内容,落库 + 写 OS 剪贴板 + emit WS
    Applied { entry_id: ClipboardEntryId },
    /// content_hash 已在本地库存在,跳过(不写 OS,不 emit WS)
    DuplicateSkipped { content_hash: String, existing_entry_id: ClipboardEntryId },
    /// envelope decode 失败(向前兼容性问题或损坏 payload)
    DecodeFailed { reason: String },
}

impl ApplyInboundClipboardUseCase {
    pub(crate) async fn execute(&self, notice: InboundNotice) -> Result<ApplyOutcome, ApplyInboundError> {
        // 1. dedup 短路:content_hash 已在本地库
        if let Some(existing) = self.entry_repo.find_by_content_hash(&notice.content_hash).await? {
            return Ok(ApplyOutcome::DuplicateSkipped {
                content_hash: notice.content_hash,
                existing_entry_id: existing.id,
            });
        }

        // 2. envelope decode
        let payload = match decode_v3_envelope(&notice.plaintext) {
            Ok(p) => p,
            Err(e) => return Ok(ApplyOutcome::DecodeFailed { reason: e.to_string() }),
        };

        // 3. 还原 SystemClipboardSnapshot
        let snapshot = snapshot_from_v3(payload);

        // 4. capture(persist + spool + emit `CaptureClipboardEvent`)
        let entry_id = self
            .capture_uc
            .execute_with_origin(snapshot.clone(), ClipboardChangeOrigin::RemotePush, None)
            .await?
            .ok_or(ApplyInboundError::CapturePersistedNothing)?;

        // 5. 写 OS 剪贴板 via coordinator(注册 RemotePush guard 防回环)
        self.write_coordinator
            .write(&snapshot, ClipboardChangeOrigin::RemotePush)
            .await?;

        Ok(ApplyOutcome::Applied { entry_id })
    }
}
```

**关键约束**:
- 步骤 4 → 5 顺序固定:**先落库再写 OS**。如果倒过来,write 触发 watcher → 检查 origin → RemotePush → skip,但 capture 还没跑完,origin guard 已被消费,有 race
- `ClipboardEntryRepositoryPort.find_by_content_hash` 若不存在 → Phase 3 必须加。预计 5 min,Diesel adapter 加一个 `WHERE content_hash = $1 LIMIT 1`
- WS event(`clipboard.new_content`)由 daemon worker 在 use case 返回 `Applied { entry_id }` 后 emit,**不在** use case 内 — uc-application 不该知道 WS 协议(参考 Phase 2 §11.4)

---

## 6. 装配方案

### 6.1 `build_daemon_app` 改动

```rust
// uc-bootstrap/src/builders.rs
pub struct DaemonBootstrapContext {
    pub deps: AppDeps,
    pub pairing_facade: Arc<PairingFacade>,
    pub space_access_facade: Arc<SpaceAccessFacade>,
    pub trusted_peer_repo: Arc<dyn TrustedPeerRepositoryPort>,
    pub storage_paths: AppPaths,
    pub background: BackgroundRuntimeDeps,
    pub emitter_cell: Arc<RwLock<Arc<dyn HostEventEmitterPort>>>,

    // Slice 2 Phase 3 新增:daemon clipboard 路径所需 facade。
    // 共享同一个 SpaceSetupAssembly 内的实例,确保 ingest loop / dispatch
    // 路径都用 iroh 节点上的 CLIPBOARD_ALPN handler。
    pub clipboard_sync_facade: Arc<ClipboardSyncFacade>,
    // SpaceSetupAssembly 的 ingest_handle 留在 assembly 里,daemon 不直接持有
    // (assembly drop 时自然 abort)
    pub space_setup_assembly: SpaceSetupAssembly,
}
```

`build_daemon_app` 内部:
1. 老步骤:`build_core` → 装 `WiredDependencies`
2. 新步骤:调 `build_space_setup_assembly(&wired, IrohNodeConfig::default())` → `SpaceSetupAssembly`
3. 从 assembly 提取 `clipboard_sync_facade = Arc::clone(&assembly.clipboard_sync)`
4. 把 assembly 整体也 stash 到 ctx,daemon 自己 own 它的生命周期(shutdown 路径调 `assembly.shutdown().await`)

**风险**:`build_space_setup_assembly` 是 async,需要在 tokio runtime 内调。`build_daemon_app` 是 sync 入口(`entrypoint::run` 在 tokio runtime 外面调)。两条路:
- (a) 在 `entrypoint::run` 创建的 tokio runtime 里 `block_on(build_space_setup_assembly(...))`(看现有 daemon 怎么 block_on iroh)
- (b) `build_daemon_app` 改成 async,所有 caller 包一层 runtime

**推荐 (a)**,改动面小;`entrypoint::run` 已经创了 multi-thread runtime 跑 daemon 主循环,在它的 handle 上 block_on 一次 assembly 构造无副作用。

### 6.2 `entrypoint.rs` 改动

```rust
// 现有:
let ctx = build_daemon_app()?;
// ...
let watcher = ClipboardWatcherWorker::new(...);

// Phase 3:
let ctx = build_daemon_app()?;  // 现在内部 block_on assembly 构造
let clipboard_sync = ctx.clipboard_sync_facade.clone();
// ... DaemonClipboardChangeHandler 注入 clipboard_sync
let watcher = ClipboardWatcherWorker::new(handler, ...);
let inbound = InboundClipboardSyncWorker::new(
    runtime,
    event_tx,
    clipboard_write_coordinator,
    clipboard_sync.clone(),  // 新参数
    apply_inbound_uc,         // 新参数
    file_cache_dir,
    file_transfer_lifecycle,
);
```

shutdown 路径加 `ctx.space_setup_assembly.shutdown().await`(覆盖 ingest loop abort + iroh router 关闭)。

### 6.3 `DaemonClipboardChangeHandler` 改动

```rust
pub struct DaemonClipboardChangeHandler {
    runtime: Arc<CoreRuntime>,
    event_tx: broadcast::Sender<DaemonWsEvent>,
    clipboard_change_origin: Arc<dyn ClipboardChangeOriginPort>,
    file_transfer_lifecycle: Arc<FileTransferLifecycle>,
    capture_gate: Arc<AtomicBool>,
    clipboard_sync: Arc<ClipboardSyncFacade>,  // 🆕
}

impl DaemonClipboardChangeHandler {
    // 删除 build_sync_outbound_clipboard_use_case
    // on_clipboard_changed 末尾的 dispatch 段改:
    
    async fn on_clipboard_changed(...) -> Result<()> {
        // ... existing capture / search / origin checks 不变
        
        // outbound dispatch — Phase 3 新路径
        if origin == ClipboardChangeOrigin::LocalCapture {
            // master toggle short-circuit
            let settings = self.runtime.wiring_deps().settings.load().await?;
            if !settings.sync.auto_sync {
                debug!("global auto_sync off, skipping outbound dispatch");
                return Ok(());
            }
            
            match self.clipboard_sync.dispatch_snapshot(
                outbound_snapshot,
                ClipboardChangeOrigin::LocalCapture,
            ).await {
                Ok(outcome) => info!(
                    accepted = outcome.total_accepted,
                    duplicate = outcome.total_duplicate,
                    offline = outcome.total_offline,
                    errored = outcome.total_errored,
                    "daemon outbound dispatch via iroh"
                ),
                Err(e) => warn!(error = %e, "daemon outbound dispatch failed"),
            }
        }
        Ok(())
    }
}
```

**注意**:删的不只是 `build_sync_outbound_clipboard_use_case`,还有 `tokio::task::spawn_blocking(|| executor::block_on(...))` 这段 — Phase 3 `dispatch_snapshot` 是 native async,直接 `.await`。

### 6.4 `InboundClipboardSyncWorker` 改动

```rust
pub struct InboundClipboardSyncWorker {
    runtime: Arc<CoreRuntime>,
    event_tx: broadcast::Sender<DaemonWsEvent>,
    clipboard_write_coordinator: Arc<ClipboardWriteCoordinator>,
    clipboard_sync: Arc<ClipboardSyncFacade>,        // 🆕
    apply_inbound_uc: Arc<ApplyInboundClipboardUseCase>,  // 🆕
    file_cache_dir: Option<PathBuf>,
    file_transfer_lifecycle: Option<Arc<FileTransferLifecycle>>,
}

#[async_trait]
impl DaemonService for InboundClipboardSyncWorker {
    async fn start(&self, cancel: CancellationToken) -> anyhow::Result<()> {
        let mut rx = self.clipboard_sync.subscribe_inbound_notices();
        loop {
            tokio::select! {
                _ = cancel.cancelled() => { return Ok(()); }
                recv = rx.recv() => match recv {
                    Ok(notice) => self.handle_one(notice).await,
                    Err(broadcast::error::RecvError::Lagged(missed)) => {
                        warn!(missed, "inbound clipboard sync lagged");
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        info!("inbound clipboard sync channel closed");
                        return Ok(());
                    }
                }
            }
        }
    }
}

impl InboundClipboardSyncWorker {
    async fn handle_one(&self, notice: InboundNotice) {
        match self.apply_inbound_uc.execute(notice).await {
            Ok(ApplyOutcome::Applied { entry_id }) => {
                self.emit_ws_event(entry_id);
            }
            Ok(ApplyOutcome::DuplicateSkipped { .. }) => {
                debug!("inbound dropped: duplicate of existing entry");
            }
            Ok(ApplyOutcome::DecodeFailed { reason }) => {
                warn!(reason, "inbound payload decode failed");
            }
            Err(e) => {
                warn!(error = %e, "inbound apply failed");
            }
        }
    }
}
```

**parse_clipboard_frame 整段删掉** — Phase 3 不再走 `ClipboardInboundTransportPort` 那条 JSON+trailing wire(那个是 libp2p 时代的协议)。

### 6.5 回环防御链路确认

Phase 2 已有的链路在 Phase 3 仍然成立:
- daemon entrypoint 持有**唯一一份** `clipboard_change_origin: Arc<dyn ClipboardChangeOriginPort>`(从 `runtime.wiring_deps().clipboard.clipboard_change_origin` 拿)
- 这个 Arc 同时注入 `DaemonClipboardChangeHandler`(消费者)+ `ClipboardWriteCoordinator`(标记者)
- `ApplyInboundClipboardUseCase.execute` 第 5 步 `coordinator.write(snapshot, RemotePush)` 内部:
  1. 计算 snapshot.origin_guard_key
  2. 调 `clipboard_change_origin.set_next_origin(guard_key, RemotePush, TTL=60s)`
  3. 实际 `system_clipboard.write(snapshot)` 写 OS
- OS clipboard 触发 watcher → `DaemonClipboardChangeHandler.on_clipboard_changed`
- handler 第 2 步 `consume_origin_for_snapshot_or_default(guard_key, LocalCapture)` 拿到 RemotePush
- 第 4 步 dispatch 路径有 `if origin == LocalCapture` 守护,RemotePush 直接 short-circuit ✅

无需新代码,Phase 3 只要保证 coordinator 这条 Arc 没换,就自动继承防御。

---

## 7. 任务拆解 + 工时估算

| # | 任务 | 依赖 | 估算 | 备注 |
|---|---|---|---|---|
| **T0a** | **迁移 `CaptureClipboardUseCase`** `uc-app` → `uc-application/src/usecases/clipboard_capture/`;`uc-app` 老路径留 deprecated re-export shim;workspace 编译绿 | — | 1.0h | shim 文件保持 `mod capture_clipboard.rs` 改为 `pub use uc_application::usecases::clipboard_capture::CaptureClipboardUseCase;`;原文件搬过去后内部 imports `uc_app::*` 全换 `uc_core::*` 或 `uc_application::*`;原现有 6 单测一并搬 |
| **T0b** | **迁移 `ClipboardWriteCoordinator`** `uc-app` → `uc-application/src/usecases/clipboard_write/`;同样留 deprecated shim;workspace 编译绿 | — | 0.8h | 跟 T0a 同形,作用面更小(单一类型) |
| T1 | `ClipboardEntryRepositoryPort.find_by_content_hash` 加一个方法 + Diesel adapter impl + 1 单测 | — | 0.4h | port 加方法 / impl / migration 不需要(列已有) |
| T2 | `payload_codec.rs` 新建:`encode_snapshot_to_v3` / `decode_v3_to_snapshot` / `content_hash_of_plaintext` 三个 pub fn + 4 单测(round-trip / 损坏字节 / 单 rep / 多 rep) | — | 0.6h | 复用 `uc-core::network::protocol::ClipboardBinaryPayload` 既有 codec |
| T3 | `ClipboardSyncFacade::dispatch_snapshot(snapshot, origin)` 方法 + 2 单测(基于 mockall mock) | T2 | 0.3h | 内部调 payload_codec + 复用 dispatch_entry 的 dispatch_uc |
| T4 | `ApplyInboundClipboardUseCase` + `ApplyOutcome` + 5 单测(Applied / DuplicateSkipped / DecodeFailed / capture failed / write_coordinator failed) | T0a, T0b, T1, T2 | 1.0h | mockall 全套,no broadcast;`use uc_application::usecases::clipboard_capture::CaptureClipboardUseCase`(T0a 后的新路径) |
| T5 | `build_daemon_app` 装配 `SpaceSetupAssembly` + `DaemonBootstrapContext.clipboard_sync_facade` 字段 + block_on 处理 | — | 0.6h | 难点在 sync/async 边界 |
| T6 | `entrypoint.rs` 注入 clipboard_sync + apply_inbound_uc 到两 worker;shutdown 路径加 `assembly.shutdown().await` | T3, T4, T5 | 0.4h | |
| T7 | `DaemonClipboardChangeHandler` 改装:删 build_sync_outbound_*,改 on_clipboard_changed dispatch 段 | T3, T6 | 0.5h | master toggle 短路 |
| T8 | `InboundClipboardSyncWorker` 重写:订阅源切换 + handle_one 走 ApplyInboundClipboardUseCase | T4, T6 | 0.6h | parse_clipboard_frame 整段删 |
| T9 | CLI `send` 升级走 envelope codec(payload_codec 复用) | T2 | 0.3h | |
| T10 | CLI `watch` 升级 decode envelope + 调整 JSON 输出 schema | T2 | 0.4h | |
| T11 | Phase 2 e2e 更新:用 V3 envelope bytes,断言 decode 后字节相等 | T2 | 0.3h | |
| T12 | 新 e2e `slice2_phase3_daemon_e2e.rs`:两 daemon profile 自动 dispatch + receive + 写 OS clipboard 验证 | T6, T7, T8 | 2.5h | 用 spawn 两 daemon process + `--dev` profile;系统剪贴板需要绕过(用 `ClipboardWriteCoordinator` mock?或开 `UC_DISABLE_SYSTEM_CLIPBOARD=0` 在 CI 跑只能在本地?难度评估见 §10) |
| T13 | `scripts/test_clipboard_e2e.sh` 更新 watch JSON schema | T10 | 0.2h | |
| T14 | 真机双 profile 验收:复制文字 → daemon 自动同步 → B 系统剪贴板被改 | T7, T8 | 1.0h | 必跑(daemon 改装核心) |
| T15 | `task_plan.md` Phase 3 ✅ + tracker 封版 + progress.md session 续 29 + follow-up 列表 | T14 | 0.4h | |

**总估算**:~11.3h(乐观),~15h(保守) — 含 T0a/T0b 迁移 +1.8h

依赖图(ASCII):
```
T0a ─┐
T0b ─┤
T1  ─┼─→ T3 ─┐
T2  ─┼──────┤
     └─→ T4 ─┤
             ├─→ T6 ─┬─→ T7 ─┐
T5  ─────────┘       └─→ T8 ─┤
                             ├─→ T12 ─→ T14 ─→ T15
T9  ─→ T11 ──────────────────┤
T10 ─────────────────────────┘
```

---

## 8. 测试策略

**单元测试**(预计 +12 个):
- T1:1(Diesel find_by_content_hash hits/miss)
- T2:4(payload_codec round-trip)
- T3:2(facade.dispatch_snapshot 用 mockall)
- T4:5(ApplyInboundClipboardUseCase 全分支)

**集成测试 / e2e**:
- T11:更新现有 phase2 e2e(envelope codec)
- T12:新 phase3 daemon e2e — 两 daemon process 自动 dispatch(技术难点,见 §10)
- T13:更新 shell e2e

**手动验收**(必跑):
- T14:两 profile daemon 双开,A 复制文字,**不输入任何命令**,B 系统剪贴板被改 + B daemon WS 客户端收到 `clipboard.new_content` 事件

---

## 9. 风险表

| 风险 | 缓解 |
|---|---|
| **build_daemon_app 内调 async**(T5):`build_space_setup_assembly` 是 async 但 daemon 入口是 sync | tokio Handle::block_on(daemon 已用 multi-thread runtime);worst case `build_daemon_app` 改 async 整链上推 |
| **e2e T12 系统剪贴板写测**:CI runner 没 OS clipboard,本地跑也会污染开发者剪贴板 | 把 `ClipboardWriteCoordinator` 注入一个 `MockSystemClipboard` 作 `--dev` 路径下的替身;或者 e2e 只断到"`ApplyOutcome::Applied` + DB 多 1 行 + WS event 收到",不验证真 OS 写 |
| **回环防御 race**:capture 还没完成 + write 已注册 guard,watcher 抢跑 | use case 顺序固定:**capture 完成 → 再 write**(§5 已论);如果 watcher 在 capture 前抢跑,origin_guard_key 还没注册 → 默认 LocalCapture → 又会被 dispatch 出去...所以 write 之前 guard 必须先注册。Coordinator 的 `write` 内部已先 `set_next_origin` 再调 `system_clipboard.write`,顺序对 |
| **iroh router 在 daemon shutdown 时没正确关**:assembly 内 ingest_handle.abort + iroh_node.shutdown 都需要 .await | entrypoint.rs daemon 主循环 graceful shutdown 段加 `ctx.space_setup_assembly.shutdown().await`,放在 worker.stop() 之后 |
| **deprecated transport ports 没人用了 → unused warnings 大量爆发** | Phase 3 改装后,`ClipboardOutboundTransportPort`/`ClipboardInboundTransportPort` 在 daemon 内 0 消费者;`uc-app::sync_outbound`/`sync_inbound` 仍 import 并 implement(deprecated 但活着)。Slice 5 处理 |
| **Phase 2 raw bytes 数据 → Phase 3 envelope-only 接收**:升级到 Phase 3 daemon 后,Phase 2 CLI 还在跑的 sender 发出去的 raw bytes 会被 receiver decode 失败 → DecodeFailed | 验收里说明 Phase 2 CLI 升级 walk-through 是 T9;开发者升级 binary 时 sender/receiver 同步切换。production 没人 stuck 在 Phase 2 |
| **per-member preferences 缺失 → 之前用户配置全失效** | Phase 3 release notes 明确"per-member preferences 暂时失效,master toggle 仍生效";Phase 3.5 补回来 |

---

## 10. AGENTS 合规自查

| 规范项 | 确认 |
|---|---|
| uc-core 只含 port + 领域类型,不含 daemon/iroh 类型 | T1 给 `ClipboardEntryRepositoryPort` 加方法,纯 port 扩展;T2 payload_codec 在 uc-application,不进 core |
| uc-application 只编排,不侵入 core / infra | `ApplyInboundClipboardUseCase` 调 4 个 port + `CaptureClipboardUseCase`(uc-app 内,跨 crate 调用是合法的) |
| uc-app 不能反向依赖 daemon | daemon 持有 `ApplyInboundClipboardUseCase` 的 Arc,use case 自己不知道 daemon |
| Orchestrator / StateMachine 不对外导出(§11.4) | Phase 3 没有 orchestrator;两 worker 是 daemon 表示层,合规 |
| Facade 只是入口,不重新编排业务 | `dispatch_snapshot` thin wrapper;`ApplyInboundClipboardUseCase` 是 use case 不是 facade |
| 错误收敛,不外泄 iroh / postcard 类型 | `ApplyInboundError` 本地定义 |
| 敏感数据不打日志 | content_hash 可打,plaintext 永不打;snapshot 只打 representation 数 + size |
| 后台任务可控可关闭 | InboundClipboardSyncWorker 是 DaemonService,有 cancel token + stop() |

---

## 11. 验收前清单

- [ ] 所有新 port 方法 + use case + facade 方法 `cargo test -p uc-core -p uc-application -p uc-bootstrap` 绿
- [ ] Phase 2 e2e 更新通过(envelope round-trip)
- [ ] Phase 3 e2e 通过(daemon 自动 dispatch + receive)
- [ ] 真机两 daemon 双 profile 验收:复制 → 自动同步 → B 系统剪贴板被改
- [ ] B daemon log 看到 `Applied { entry_id: ... }` + WS event 推出
- [ ] B 重复同样内容 → daemon log `DuplicateSkipped`,DB 不增加 entry,系统剪贴板不变
- [ ] B daemon 关掉 → A 复制 → A daemon log `0 accepted, 1 offline`,不 panic
- [ ] B 重启 daemon → A 下次复制 → B 收到 + 系统剪贴板改
- [ ] CLI `send` / `watch` 仍能跑(envelope codec 升级后)
- [ ] task_plan.md Slice 2 Phase 3 ✅ + 所有 commit hash 入表
- [ ] `slice2-phase3-plan.md §15` tracker 封版

---

## 12. 待 user 确认的决策(✅ = 已确认 2026-04-22)

1. **D1 替换 vs 双栈**:✅ 完全替换
2. **D2 envelope 强度**:✅ Phase 3 必做
3. **D3 per-member preferences**:✅ Phase 3 follow-up,只迁 master toggle
4. **D4 dedup 位置**:✅ ApplyInboundClipboardUseCase
5. **D5 落库路径**:✅ 复用 `CaptureClipboardUseCase.execute_with_origin`
6. **D6 facade 接口**:✅ 加 `dispatch_snapshot`,保留 `dispatch_entry`
7. **D7 InboundNotice schema**:✅ 不改,decode 在 use case
8. **D8 依赖 usecase crate 归属**(用户补充):✅ `uc-app` → `uc-application` 迁移,老路径留 deprecated re-export shim;迁移作 T0a / T0b 前置任务

---

## 13. 时间线建议

按 §7 任务图:
- **Day 1**(~2h):**T0a + T0b**(uc-app → uc-application 迁移,workspace 编译绿)
- **Day 2**(~3.5h):T1 + T2 + T3 + T4(应用层全部就位,跑通单测)
- **Day 3**(~2h):T5 + T6 + T7 + T8(daemon 装配 + 两 worker 改装)
- **Day 4**(~2h):T9 + T10 + T11 + T13(CLI / e2e / shell 更新)
- **Day 5**(~3.5h):T12(新 e2e)+ T14(真机验收)+ T15(收尾)

总计 ~13h 跨 4-5 sessions(比 Phase 2 长 +25%,主要是 T0a/T0b 迁移)。

---

## 14. Phase 3 完工后,Slice 2 / Slice 3 之间还剩什么

Slice 2 root 罗列覆盖 usecase:`C1 outbound / C2 inbound / F1 完整版 / A3 revoke / A5 rename / E1 roster / E2 presence events`

Phase 1 + 2 + 3 完工后状态:
- ✅ C1 outbound(text 限定,大 payload 推 Slice 3)
- ✅ C2 inbound(text 限定)
- ✅ F1 完整版(Phase 1 hook + Phase 3 daemon 路径)
- ⏸️ A3 revoke(无 UI)
- ⏸️ A5 rename(无 UI)
- ✅ E1 roster(Phase 1)
- ✅ E2 presence events(Phase 1)

Slice 2 没全完(A3/A5 缺 UI),但 **C/E/F 三组核心 usecase 全到位**,Slice 3(blob / 文件)前置条件就绪。A3/A5 的 UI 推到 Phase 4 或 Slice 6 集中做。

---

## 15. 进度跟踪(留空,执行时填)

### 15.1 任务状态

| # | 任务 | 状态 | commit | 实际工时 | 备注 |
|---|---|---|---|---|---|
| T0a | 迁移 CaptureClipboardUseCase → uc-application | ✅ | `cb4ac588` | 0.4h | 新 `uc-application/src/clipboard_capture/`(mod.rs + usecase.rs);`uc-app` 老路径 16 行 shim;workspace + 188 uc-application 测试 + 7 uc-app 测试全绿 |
| T0b | 迁移 ClipboardWriteCoordinator → uc-application | ✅ | `ad5ac7ac` | 0.3h | 新 `uc-application/src/clipboard_write/`;同形 shim;workspace 绿 |
| T1 | find_by_content_hash | ✅ | `9ce27893` | 0.3h | port 加 default method 返 `Ok(None)`;Diesel 走两步查询(event → entry)避开 JoinDsl 导入冲突;2 直插 fixture 单测全绿;签名定为 `find_entry_id_by_snapshot_hash(&str) -> Option<EntryId>`(返回 id 不返全 entry,dedup 场景只需知道"存不存在") |
| T2 | payload_codec | ✅ | `68f89b31` | 0.4h | `encode_snapshot_to_v3_bytes` 返 `(Bytes, content_hash)`,`decode_v3_bytes_to_snapshot` 反向;content_hash 走 `snapshot.snapshot_hash().to_string()` 与本地 `clipboard_event.snapshot_hash` 列对齐;decoder 给 representations 全分配新 `RepresentationId`(receiver-local);4 单测全绿(roundtrip 单 / 多 rep + None mime + 二进制 / hash 确定性 / truncate 错误) |
| T3 | dispatch_snapshot | ⏸️ pending | — | — | — |
| T4 | ApplyInboundClipboardUseCase | ⏸️ pending | — | — | — |
| T5 | build_daemon_app 装配 | ⏸️ pending | — | — | — |
| T6 | entrypoint 注入 | ⏸️ pending | — | — | — |
| T7 | DaemonClipboardChangeHandler 改装 | ⏸️ pending | — | — | — |
| T8 | InboundClipboardSyncWorker 重写 | ⏸️ pending | — | — | — |
| T9 | CLI send envelope 升级 | ⏸️ pending | — | — | — |
| T10 | CLI watch decode 升级 | ⏸️ pending | — | — | — |
| T11 | phase 2 e2e 更新 | ⏸️ pending | — | — | — |
| T12 | phase 3 daemon e2e | ⏸️ pending | — | — | — |
| T13 | shell e2e schema 更新 | ⏸️ pending | — | — | — |
| T14 | 真机验收 | ⏸️ pending | — | — | — |
| T15 | 收尾文档 | ⏸️ pending | — | — | — |

### 15.2 累计

- 已完成:0 / 17
- 实际工时:0
- 估算:~13h(含 T0a/T0b 迁移 +1.8h)

### 15.3 关键决策 / 偏离

(执行时填)

### 15.4 后续提醒

(执行时填)
