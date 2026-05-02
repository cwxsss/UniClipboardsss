---
phase: 260423-mxu-macos-linux-rep-rep-policy
plan: 01
subsystem: uc-platform / clipboard
tags: [macos, clipboard, multi-rep, objc2, pasteboard]
dependency_graph:
  requires: [260423-9do-windows-rep]
  provides: [macOS 原子多 rep 写入能力]
  affects: [uc-platform/clipboard/common.rs, uc-platform/clipboard/platform/macos.rs]
tech_stack:
  added:
    - objc2 = "0.6"（macOS target 专属）
    - objc2-app-kit = "0.3"（features: NSPasteboard, NSPasteboardItem）
    - objc2-foundation = "0.3"（features: NSArray, NSData, NSString）
  patterns:
    - NSPasteboardItem + writeObjects: 原子多 rep 写入模式
    - extern "C" 静态变量访问需 unsafe 块（方法本身可以是安全的）
key_files:
  created: []
  modified:
    - src-tauri/crates/uc-platform/Cargo.toml
    - src-tauri/crates/uc-platform/src/clipboard/platform/macos.rs
    - src-tauri/crates/uc-platform/src/clipboard/common.rs
decisions:
  - "使用 NSPasteboardItem::setData_forType + NSPasteboard::writeObjects: 实现原子多 rep 写入，而非旧式 declareTypes:owner: API"
  - "NSData 构造选 with_bytes(&[u8])，不选 from_vec（需 block2 feature）"
  - "macOS NSPasteboard API 在 objc2-app-kit 0.3.2 均为 pub fn（无 MainThreadMarker），可后台线程调用"
  - "Linux 保留 V1-policy 降级，加 FIXME(260423-mxu-next-phase) 注释，不提前引入 wl-clipboard-rs"
metrics:
  duration: "约 40 分钟"
  completed: "2026-04-23"
  tasks_completed: 3
  files_modified: 4
---

# Phase 260423-mxu Plan 01：macOS / Linux 多 rep 写入策略 总结

## 一句话总览

为 macOS 补齐原子多 representation 写入能力（`NSPasteboardItem + writeObjects:`），消除 `common.rs` 中非 Windows 平台统一降级的掩盖式实现，同时将 Linux 降级分支显式化（`FIXME` + `warn!` 日志）。

---

## 各任务实际改动概述

### 任务 1：Cargo.toml 新增 macOS 平台专属依赖

**commit：`b15ec09b`**

在 `src-tauri/crates/uc-platform/Cargo.toml` 末尾追加：

```toml
[target.'cfg(target_os = "macos")'.dependencies]
objc2 = "0.6"
objc2-app-kit = { version = "0.3", features = ["NSPasteboard", "NSPasteboardItem"] }
objc2-foundation = { version = "0.3", features = ["NSArray", "NSData", "NSString"] }
```

版本锁定与 workspace 内 arboard 3.4 已传递引入的版本一致（`objc2 0.6.3` / `objc2-app-kit 0.3.2` / `objc2-foundation 0.3.2`），不引入额外下载。三个 crate 只在 macOS target 下启用，不影响 Linux / Windows 构建。

### 任务 2：macos.rs 实现 write_snapshot_multi_macos

**commit：`9206b75f`**

在 `src-tauri/crates/uc-platform/src/clipboard/platform/macos.rs` 新增三个函数：

- `fn resolve_multi_rep_mime` — 与 `common.rs` 单 rep 快路径及 `windows.rs` 保持一致的 MIME 推断表
- `fn make_nsdata` — 包装 `NSData::with_bytes(&[u8])` 构造，把实际 API 名隔离到一处
- `pub(crate) fn write_snapshot_multi_macos(snapshot: SystemClipboardSnapshot) -> Result<()>` — 主实现

写入流程：
1. 预扫描是否有可写 rep（text/plain 或 text/html）；若无则 bail，不执行 `clearContents()`
2. `NSPasteboard::generalPasteboard()` 获取系统剪贴板单例
3. `clearContents()` 清空旧内容
4. `NSPasteboardItem::new()` 构造 item，逐 rep 调用 `setData_forType` 填入数据
5. `ProtocolObject::from_retained(item)` 包装，`NSArray::from_retained_slice` 构造数组
6. `writeObjects(&arr)` 原子提交；false → Err 上抛

### 任务 3：common.rs 拆分 not-windows 分支

**commit：`0960e7ee`**（含任务 2 的 unsafe 修复）

将 `fn write_snapshot_multi` 内的单一 `#[cfg(not(target_os = "windows"))]` 块拆为两支：

```rust
#[cfg(target_os = "macos")]
{
    let _ = ctx;
    let _ = rep_count;
    return crate::clipboard::platform::macos::write_snapshot_multi_macos(snapshot);
}

// FIXME(260423-mxu-next-phase)：Linux 原子多 rep 需要 wl-clipboard-rs / X11 selection owner
#[cfg(any(target_os = "linux", not(any(target_os = "windows", target_os = "macos"))))]
{
    // 保留既有 V1-policy 降级逻辑，仅更新 warn! 日志文案（明确 Linux）
    ...
}
```

同步更新 `write_snapshot` 和 `write_snapshot_multi` 的 doc comment，改为 Windows / macOS / Linux 三段式策略描述，去掉"macOS 尚未支持"的旧表述。

**同 commit 包含的偏差修复**：任务 2 中对 `NSPasteboard::generalPasteboard` / `clearContents` / `NSPasteboardItem::new` / `writeObjects` 加了 `unsafe` 块，经 cargo check 确认这些方法在 objc2-app-kit 0.3.2 中是 `pub fn`（安全方法），只有访问 `NSPasteboardTypeString` / `NSPasteboardTypeHTML` 这类 `extern "C"` 静态变量才需要 `unsafe` 块。已修正，消除 4 个 `unnecessary unsafe` warning。

---

## API 核实 / 偏差记录

### 实测确认的关键 API（objc2-app-kit 0.3.2 / objc2-foundation 0.3.2 / objc2 0.6.3）

| API | 实际签名 | unsafe? | 与 PLAN 伪代码对比 |
|-----|---------|---------|-----------------|
| `NSPasteboard::generalPasteboard` | `pub fn generalPasteboard() -> Retained<NSPasteboard>` | 方法本身 **不需要** unsafe | 一致 |
| `NSPasteboard::clearContents` | `pub fn clearContents(&self) -> NSInteger` | 方法本身 **不需要** unsafe | 一致 |
| `NSPasteboard::writeObjects` | `pub fn writeObjects(&self, objects: &NSArray<ProtocolObject<dyn NSPasteboardWriting>>) -> bool` | 方法本身 **不需要** unsafe | 一致 |
| `NSPasteboardItem::new` | `pub fn new() -> Retained<Self>` | **不需要** unsafe | 一致 |
| `NSPasteboardItem::setData_forType` | `pub fn setData_forType(&self, data: &NSData, r#type: &NSPasteboardType) -> bool` | 方法本身 **不需要** unsafe | 一致 |
| `NSPasteboardTypeString` / `NSPasteboardTypeHTML` | `extern "C" { pub static ...: &'static NSPasteboardType; }` | 访问 **需要** unsafe | PLAN 未明确说明 |
| `NSData::with_bytes` | `pub fn with_bytes(bytes: &[u8]) -> Retained<Self>`（`src/data.rs`） | **不需要** unsafe | PLAN 标注为 `todo!()` 待核实，实测确认为 `with_bytes` |
| `NSArray::from_retained_slice` | `pub fn from_retained_slice(slice: &[Retained<ObjectType>]) -> Retained<Self>`（`src/array.rs`） | **不需要** unsafe | PLAN 标注为 `todo!()` 待核实，实测确认存在 |
| `ProtocolObject::from_retained` | `pub fn from_retained<T>(obj: Retained<T>) -> Retained<Self>`（`src/runtime/protocol_object.rs`） | **不需要** unsafe | PLAN 标注为伪代码，实测一致 |

### 偏差 1：extern "C" 静态变量访问需要 unsafe 块（Rule 1 自动修正）

**发现于**：任务 3 的 cargo check

**问题**：任务 2 对 `NSPasteboard` 的方法调用加了 `unsafe` 块（基于保守推断），导致编译器报 4 个 `unnecessary unsafe` warning。同时对 `NSPasteboardTypeString` / `NSPasteboardTypeHTML` 的访问**没有**加 `unsafe` 块，导致 2 个 `use of extern static is unsafe` error。

**根因**：
- `generalPasteboard` / `clearContents` / `NSPasteboardItem::new` / `writeObjects` 在 objc2-app-kit 0.3.2 中是 `pub fn`（安全），不需要 unsafe 块
- `NSPasteboardTypeString` / `NSPasteboardTypeHTML` 是 `extern "C"` 静态变量，Rust 规定访问 extern 静态变量需要 unsafe 块（即使值本身是安全的）

**修复**：
- 移除 4 个不必要的 unsafe 块（方法调用处）
- 保留 2 个必须的 unsafe 块（extern static 访问处），并更新注释说明原因

**PLAN 伪代码问题**：PLAN 对所有调用都套了 `unsafe {}`，这在 objc2 0.4.x / 0.5.x 时代是正确的，但 0.6.x 对大量 API 移除了 unsafe 要求，同时 extern static 的访问在 Rust 1.82+ 中开始产生警告/错误。实际代码比 PLAN 伪代码更精确地区分了"哪些地方真正需要 unsafe"。

### MainThreadMarker 核实结论

`NSPasteboard` 在 objc2-app-kit 0.3.2 的 `extern_class!` 宏定义中没有 `MainThreadOnly` 约束（源码中无 `MainThreadMarker` 引用），所有写入相关方法均为 `pub fn`，可在后台 tokio 线程调用。

---

## 编译验证结果

| Target | 状态 | 备注 |
|--------|------|------|
| `aarch64-apple-darwin`（本机 native） | **通过** 0 error / 4 pre-existing warning | 4 个 warning 均为 address_registry 等已有代码，与本次无关 |
| `x86_64-apple-darwin` | 未验证（target 未安装） | 可通过 `rustup target add x86_64-apple-darwin` 补验 |
| `x86_64-unknown-linux-gnu` | **交叉编译环境缺失**（`x86_64-linux-gnu-gcc` 不存在） | 与 260423-9do 处理 Windows 的方式一致，由 CI 验证；Linux 分支逻辑未变，零回归风险极低 |
| `x86_64-pc-windows-msvc` | 未验证（MSVC 工具链未安装） | Windows 代码路径零改动，由 CI 验证 |

---

## write_snapshot_multi_macos 实测状态

仅通过 `cargo check` 编译验证，**实机 Cmd+V 验证待做**（需在主流量接线完成后——即删除 `narrow_to_primary`，让 `apply_inbound` 直接 write 全 snapshot 后——才能在日常使用中触发 macOS 多 rep 路径）。

---

## 已知后续工作

1. **Linux 原子多 rep**（优先级最高）
   - 需要 `wl-clipboard-rs` 的 DataSource 接口（多 MIME type 注册）或 X11 selection owner 持久持有模型
   - 工作量与本次 macOS 任务相当，建议独立 phase 处理
   - 当前代码中已有 `FIXME(260423-mxu-next-phase)` 注释标注

2. **image / rtf / files 的多 rep 写入（macOS 侧）**
   - 本次 macOS 路径 MVP 只支持 `text/plain` + `text/html`
   - 后续需补齐 `NSPasteboardTypePNG` / `NSPasteboardTypeRTF` 等
   - 参考 `windows.rs` 的 CF_DIBV5 + 自定义 PNG format 双写策略

3. **接线：删除 `narrow_to_primary`，让 `apply_inbound` 直接 write 全 snapshot**
   - 这是让 macOS / Windows 多 rep 能力在**主流量**中真正触发的必要步骤
   - 当前 `write_snapshot_multi_macos` 虽已实现，但主流量仍被 `narrow_to_primary` 剪成单 rep
   - 完成接线后才能做实机 Cmd+V 端到端验证

---

## 已知 Stubs

无。本次改动不包含硬编码空值、placeholder 文本或未接线的组件——`write_snapshot_multi_macos` 是真实实现，不是 stub；Linux 分支是显式降级（有 `warn!` + FIXME），语义明确。
