---
phase: 260423-mxu-macos-linux-rep-rep-policy
plan: 01
type: execute
wave: 1
depends_on: []
files_modified:
  - src-tauri/crates/uc-platform/Cargo.toml
  - src-tauri/crates/uc-platform/src/clipboard/platform/macos.rs
  - src-tauri/crates/uc-platform/src/clipboard/common.rs
autonomous: true
requirements:
  - MACREP-01  # macOS 平台层具备原子多 rep 写入能力（text/plain + text/html）
  - MACREP-02  # 替换 common.rs 中 macOS 分支的 "只写第一个 rep" 降级逻辑
  - LINUXREP-01  # Linux 降级路径显式化，消灭与 Windows 分支混在一起的"隐性"行为

must_haves:
  truths:
    - "`common.rs::write_snapshot_multi` 的 `#[cfg(not(target_os = \"windows\"))]` 分支被拆为两支：macOS 走新增的 `write_snapshot_multi_macos`；Linux 保留**显式**降级（`warn!` + V1 policy 选 paste-priority rep）。"
    - "macOS 收到 `representations.len() > 1` 的 snapshot 时，在**同一次** pasteboard 会话内（`clearContents` + 一次 `writeObjects([item])`）写入 `NSPasteboardTypeString` + `NSPasteboardTypeHTML`，纯文本目的地（终端 / 纯文本模式 TextEdit）与富文本目的地（富文本 TextEdit / Pages / 浏览器地址栏）均能粘贴到相应的内容。"
    - "macOS 多 rep 写入失败时返回 `anyhow::Result::Err`，调用方能看到 `setData:forType:` 的 bool 返回值 false / `writeObjects:` 返回 false 等具体原因；不静默降级（§9.3 / §19.5）。"
    - "Linux 分支行为语义上等价于改前：继续用 `SelectRepresentationPolicyV1` 选 paste-priority rep 降级，但 doc comment 明确标注"下一个 phase 专门解决"并带 FIXME；不会 panic / 不 bail。"
    - "Windows 分支行为**零变化**：`write_snapshot_multi_windows` 调用路径、`WindowsClipboard::write_snapshot` 的 dummy_ctx 绕道都不动。"
    - "`narrow_to_primary` / `apply_inbound.rs` 不动；本次只提供平台能力，不改主流量接线。"
  artifacts:
    - path: "src-tauri/crates/uc-platform/Cargo.toml"
      provides: "macOS 平台专属依赖 objc2 / objc2-app-kit / objc2-foundation"
      contains: "target.'cfg(target_os = \"macos\")'.dependencies"
    - path: "src-tauri/crates/uc-platform/src/clipboard/platform/macos.rs"
      provides: "macOS 原子多 rep 写入实现 write_snapshot_multi_macos"
      contains: "write_snapshot_multi_macos"
    - path: "src-tauri/crates/uc-platform/src/clipboard/common.rs"
      provides: "拆分 not-windows 分支：macOS 委派 + Linux 显式降级"
      contains: "cfg(target_os = \"macos\")"
  key_links:
    - from: "uc-platform::common::CommonClipboardImpl::write_snapshot_multi"
      to: "macos::write_snapshot_multi_macos"
      via: "cfg(target_os=\"macos\") 分流；len>1 时调用"
      pattern: "write_snapshot_multi_macos"
    - from: "macos::write_snapshot_multi_macos"
      to: "NSPasteboard::generalPasteboard + clearContents + writeObjects"
      via: "单次 pasteboard 会话内 NSPasteboardItem.setData_forType 多次 + writeObjects 原子提交"
      pattern: "NSPasteboardItem|writeObjects|clearContents"
    - from: "uc-platform::common::CommonClipboardImpl::write_snapshot_multi"
      to: "SelectRepresentationPolicyV1 降级（Linux 分支保留）"
      via: "cfg(target_os=\"linux\") 显式 warn! + 递归单 rep 路径；FIXME 指向下一个 phase"
      pattern: "SelectRepresentationPolicyV1|FIXME"
---

<objective>
为 macOS 补齐真正的**原子多 representation** 写入能力，消除 `common.rs::write_snapshot_multi` 中 `#[cfg(not(target_os = "windows"))]` 分支下"所有非 Windows 平台统一降级为单 rep"的掩盖式实现。Linux 在本次**显式保留降级**（本 phase 的 scope 精神：单个 quick 任务只处理 macOS；Linux 的 Wayland / X11 data source 留到独立 phase）。

**目的**：与 260423-9do-windows-rep 的 Windows 能力对齐 —— 让从浏览器复制的富文本同步到 macOS 后，粘贴到纯文本目的地（终端 / 纯文本 TextEdit）仍能拿到 `text/plain`，粘贴到富文本目的地仍能拿到 `text/html`。当前 macOS 下多 rep snapshot 会被 `SelectRepresentationPolicyV1` 砍成一条（paste-priority 通常是 html），纯文本目的地粘贴内容可能是 HTML 源码而不是 plain text。

**输出**：
1. `Cargo.toml` 新增 `[target.'cfg(target_os = "macos")'.dependencies]` 段，引入 `objc2` / `objc2-app-kit` / `objc2-foundation`（版本锁定见任务 1）。
2. `macos.rs` 新增 `pub(crate) fn write_snapshot_multi_macos(snapshot: SystemClipboardSnapshot) -> Result<()>`，用 `NSPasteboardItem::setData_forType` + `NSPasteboard::writeObjects` 原子写入 `NSPasteboardTypeString` + `NSPasteboardTypeHTML`。
3. `common.rs::write_snapshot_multi` 的 `#[cfg(not(target_os = "windows"))]` 分支拆成两支：macOS 委派新函数；Linux 保留既有 V1-policy 降级 + 更新 doc comment / 添加 FIXME 指向下一个 phase。

**非目标**（严格不做）：
- 不动 `windows.rs`（Windows 路径 260423-9do 已完成，本次**零改动**）。
- 不动 `apply_inbound.rs` / `narrow_to_primary`（主流量接线留给未来）。
- 不新增 image / rtf / files 的多 rep 写入（保持与 Windows 任务 MVP 一致；`write_snapshot_multi_macos` 遇到非 text/plain / 非 text/html 的 rep 走 debug 跳过，与 `write_snapshot_multi_windows` 语义对齐）。
- 不改 `SystemClipboardPort` 签名。
- 不为 Linux 引入 `wl-clipboard-rs` / `x11-clipboard`（下一个 phase 专项处理）。
- 不新增 Rust 测试（项目已无 Rust 测试，见 commit `6f1d6a2d`）。
</objective>

<execution_context>
@.planning/quick/260423-mxu-macos-linux-rep-rep-policy/260423-mxu-PLAN.md
</execution_context>

<context>

# 关键文件

@src-tauri/crates/uc-platform/AGENTS.md
@src-tauri/crates/uc-platform/Cargo.toml
@src-tauri/crates/uc-platform/src/clipboard/common.rs
@src-tauri/crates/uc-platform/src/clipboard/platform/macos.rs
@src-tauri/crates/uc-platform/src/clipboard/platform/linux.rs
@src-tauri/crates/uc-platform/src/clipboard/platform/windows.rs
@.planning/quick/260423-9do-windows-rep/260423-9do-PLAN.md
@.planning/quick/260423-9do-windows-rep/260423-9do-SUMMARY.md

<interfaces>
<!-- 关键契约 / API 与常量 — 执行者直接引用即可；不要凭记忆写 objc2 API 签名 -->

## A. 现有 `common.rs` 的多 rep 分流骨架（任务 3 要拆的那块）

```/Volumes/ExternalSSD/superset/uniclipboard/slender-soybean/src-tauri/crates/uc-platform/src/clipboard/common.rs#L741-786
#[cfg(not(target_os = "windows"))]
{
    // macOS / Linux：显式降级（§9.3 不允许静默降级）。
    //
    // 用 V1 policy 选出 paste-priority rep —— 与应用层原 `narrow_to_primary`
    // 等价。硬编码 V1：当前 uc-core 只有这一个 `SelectRepresentationPolicyPort`
    // 实现；出现 V2 时再考虑从调用方注入 policy。
    use uc_core::clipboard::SelectRepresentationPolicyV1;
    use uc_core::ports::SelectRepresentationPolicyPort;

    let policy = SelectRepresentationPolicyV1::default();
    let selection = policy
        .select(&snapshot)
        .map_err(|e| anyhow!("representation policy failed: {e}"))?;
    let paste_id = selection.paste_rep_id.clone();

    let chosen_idx = snapshot
        .representations
        .iter()
        .position(|rep| rep.id == paste_id)
        .ok_or_else(|| {
            anyhow!(
                "policy selected paste_rep_id {:?} not present in snapshot",
                paste_id
            )
        })?;

    warn!(
        rep_count,
        paste_rep_id = ?paste_id,
        chosen_format_id = %snapshot.representations[chosen_idx].format_id,
        "multi-representation write not yet supported on this platform; \
         falling back to single-rep path — writing the paste-priority rep \
         selected by SelectRepresentationPolicyV1"
    );

    let ts_ms = snapshot.ts_ms;
    let mut reps = snapshot.representations;
    let chosen = reps.remove(chosen_idx);
    let reduced = SystemClipboardSnapshot {
        ts_ms,
        representations: vec![chosen],
    };
    return Self::write_snapshot(ctx, reduced);
}
```

本次任务 3 的工作：把这个大分支按 `cfg(target_os = "macos")` / `cfg(target_os = "linux")`（以及 fallback 的其他 Unix）拆成两支。macOS 分支调 `write_snapshot_multi_macos`；Linux 分支保留原 V1 policy 降级逻辑但加上 FIXME 注释。

## B. `SystemClipboardSnapshot` / `ObservedClipboardRepresentation` 类型

本次用到的字段（来自 `uc-core::clipboard`，与 Windows 任务一致）：

```rust
pub struct SystemClipboardSnapshot {
    pub ts_ms: i64,
    pub representations: Vec<ObservedClipboardRepresentation>,
}
pub struct ObservedClipboardRepresentation {
    pub id: RepresentationId,
    pub format_id: FormatId,       // 例如 "text", "html", "public.utf8-plain-text"
    pub mime: Option<MimeType>,    // 例如 Some("text/plain"), Some("text/html")
    pub bytes: Vec<u8>,
}
```

## C. `objc2-app-kit` 0.3.2 NSPasteboard / NSPasteboardItem 实测 API

**版本核实**：`src-tauri/Cargo.lock` 实际锁定：
- `objc2 = "0.6.3"`
- `objc2-app-kit = "0.3.2"`（已被 arboard/tauri 等 transitively 引入）
- `objc2-foundation = "0.3.2"`

这些版本随 `arboard 3.4` 间接进入 workspace，直接添加到 `uc-platform` 的依赖图时会自动复用同一套解析。**不要**凭记忆写 `objc2 0.5.x / objc2-app-kit 0.2.x`（scope_guidance 里给的是过时参考版本）。

**API 核实清单**（执行者必须在写代码前确认每一项；源文件已定位在 `~/.cargo/registry/src/index.crates.io-*/objc2-app-kit-0.3.2/src/generated/NSPasteboard.rs` 与 `NSPasteboardItem.rs`）：

1. `NSPasteboard::generalPasteboard() -> Retained<NSPasteboard>`（关联函数，无 self）
2. `NSPasteboard::clearContents(&self) -> NSInteger`（返回 changeCount，不返回错误）
3. `NSPasteboard::writeObjects(&self, objects: &NSArray<ProtocolObject<dyn NSPasteboardWriting>>) -> bool`（bool 是成功标志，false 需当作错误上抛）
4. `NSPasteboardItem::new() -> Retained<Self>`（空构造，之后逐 type 填 data）
5. `NSPasteboardItem::setData_forType(&self, data: &NSData, r#type: &NSPasteboardType) -> bool`
6. `NSPasteboardType = NSString`（type alias）
7. 常量来自 `extern "C"`：`NSPasteboardTypeString` / `NSPasteboardTypeHTML`（均为 `&'static NSPasteboardType`）
8. `NSPasteboardItem` 实现了 `NSPasteboardWriting` 协议（因此可以被放进 `NSArray<ProtocolObject<dyn NSPasteboardWriting>>` 传给 `writeObjects:`）

**关键用法拼装**（伪代码，执行者照此写，不要脑补）：

```rust
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::AllocAnyThread;  // 用于 NSPasteboardItem::new()，如果需要；具体以 registry 源码为准
use objc2_app_kit::{
    NSPasteboard, NSPasteboardItem, NSPasteboardTypeHTML, NSPasteboardTypeString,
    NSPasteboardWriting,
};
use objc2_foundation::{NSArray, NSData, NSString};

// 1. 拿 general pasteboard（主线程调用；`objc2-app-kit` 的某些 NSPasteboard API
//    可能要求 MainThreadMarker，执行者需在源码里核实本次用到的 `generalPasteboard`
//    / `clearContents` / `writeObjects:` 是否需要 MainThreadMarker。若需要，
//    在函数签名上处理—例如增加 MainThreadMarker::new_unchecked() 或者用
//    MainThreadMarker::new().ok_or_else(...)。若 generalPasteboard 已在后台线程
//    安全（Apple 官方文档表明 NSPasteboard 支持后台线程），则无需引入 MTM。
//    —— 这一点务必用源码而非记忆确认！)
let pb: Retained<NSPasteboard> = unsafe { NSPasteboard::generalPasteboard() };

// 2. 清空
let _change_count = unsafe { pb.clearContents() };

// 3. 构造 NSPasteboardItem 并逐 type 填 data
let item: Retained<NSPasteboardItem> = unsafe { NSPasteboardItem::new() };
let text_data = NSData::with_bytes(plain_bytes);
let ok_text = unsafe { item.setData_forType(&text_data, NSPasteboardTypeString) };
let html_data = NSData::with_bytes(html_bytes);
let ok_html = unsafe { item.setData_forType(&html_data, NSPasteboardTypeHTML) };

// 4. 包装成 NSArray<ProtocolObject<dyn NSPasteboardWriting>> 并 writeObjects
let proto_item: Retained<ProtocolObject<dyn NSPasteboardWriting>> =
    ProtocolObject::from_retained(item);
let arr: Retained<NSArray<ProtocolObject<dyn NSPasteboardWriting>>> =
    NSArray::from_retained_slice(&[proto_item]);
let ok_write = unsafe { pb.writeObjects(&arr) };
```

> **再次强调**：以上伪代码是**方向性**描述。`NSData::with_bytes` 的具体名（可能是 `NSData::from_vec(Vec<u8>)` / `NSData::with_bytes(&[u8])` / `NSData::dataWithBytes_length(...)` 其中之一）、`NSArray::from_retained_slice` 的具体名、`ProtocolObject::from_retained` 的具体名 —— 这些在 `objc2 0.6.x` / `objc2-foundation 0.3.x` 里的 API 与旧版本差别很大。**必须**在实现前用以下命令核实：
>
> ```bash
> # 直接 grep 本机 registry 里的源文件
> grep -E 'pub fn (from|with|init|new|dataWith)' ~/.cargo/registry/src/index.crates.io-*/objc2-foundation-0.3.2/src/**/NSData* 2>/dev/null | head -30
> grep -E 'pub fn from_retained|pub fn from_slice' ~/.cargo/registry/src/index.crates.io-*/objc2-foundation-0.3.2/src/**/NSArray* 2>/dev/null | head -20
> grep -E 'pub fn from_retained' ~/.cargo/registry/src/index.crates.io-*/objc2-0.6.3/src/runtime/protocol_object.rs 2>/dev/null | head -10
> ```
>
> 或用 `cargo doc --open -p objc2-foundation` / `cargo doc --open -p objc2-app-kit` 查看。不要盲从伪代码！参考 Windows 任务 SUMMARY 的"偏差记录 / Rule 1 自动修正"—— 前一个 plan 里 `set_string` 的"内部 NoClear"就是凭感觉写错的，本次同类风险更高（objc2 生态 API 变化极快）。

## D. `uc-platform/Cargo.toml` 当前 Windows 段作为模板

```/Volumes/ExternalSSD/superset/uniclipboard/slender-soybean/src-tauri/crates/uc-platform/Cargo.toml#L101-103
[target.'cfg(windows)'.dependencies]
clipboard-win = { version = "5.4" }
```

本次在其下方加：

```toml
[target.'cfg(target_os = "macos")'.dependencies]
objc2 = "0.6"
objc2-app-kit = { version = "0.3", features = ["NSPasteboard", "NSPasteboardItem"] }
objc2-foundation = { version = "0.3", features = ["NSArray", "NSData", "NSString"] }
```

**Feature 选取来源**（已核实自 `objc2-app-kit-0.3.2/Cargo.toml`）：
- `NSPasteboard` feature 内部自带依赖：`bitflags` + `objc2-foundation/{NSArray, NSData, NSDictionary, NSError, NSFileWrapper, NSSet, NSString, NSURL}`。
- `NSPasteboardItem` feature 内部自带依赖：`objc2-foundation/{NSArray, NSData, NSDictionary, NSError, NSSet, NSString}`。
- 即 `objc2-app-kit` 的 `NSPasteboard + NSPasteboardItem` 两个 feature 已经 **transitively** 启用了 `objc2-foundation` 所需的 `NSArray / NSData / NSString`。但为了可读性（避免"哪些 feature 由哪个 feature 传递启用"的心智负担），上面仍**显式**列出 `objc2-foundation` 的 `features = ["NSArray", "NSData", "NSString"]`。两种写法都能编过；**选显式这一种**，docstring 注释"显式列 features 便于阅读 / 避免将来 feature 传递规则变更后静默失效"。

## E. 既有 `macos.rs` 骨架（任务 2 要改的那个文件）

```/Volumes/ExternalSSD/superset/uniclipboard/slender-soybean/src-tauri/crates/uc-platform/src/clipboard/platform/macos.rs#L1-57
use super::super::common::CommonClipboardImpl;
use anyhow::Result;
use async_trait::async_trait;
use clipboard_rs::ClipboardContext;
use std::sync::{Arc, Mutex};
use tracing::{debug, debug_span};
use uc_core::clipboard::SystemClipboardSnapshot;
use uc_core::ports::SystemClipboardPort;

/// macOS clipboard implementation using clipboard-rs
pub struct MacOSClipboard { ... }
```

本次**不改** `MacOSClipboard::write_snapshot` —— 它继续通过 `CommonClipboardImpl::write_snapshot(&mut ctx, snapshot)` 进入 common.rs，common.rs 会自动分流到新增的 `write_snapshot_multi_macos`。

**与 Windows 的关键区别**：macOS 用 `objc2-app-kit::NSPasteboard::generalPasteboard()` **不占用** `clipboard-rs` 的 ctx（两者都只是对同一个系统级 NSPasteboard 单例的抽象）。因此 macOS 路径**不需要** Windows 上那种"提前 drop clipboard-rs ctx + dummy_ctx 绕路"的 workaround。直接在 `write_snapshot_multi_macos` 里 grab `generalPasteboard` 即可；`ctx` 可以继续被 `CommonClipboardImpl::write_snapshot` 的 `&mut` 借用持有（虽然本函数根本不用它）。

## F. 项目规范摘要

- `uc-platform/AGENTS.md` §4.4：`cfg(target_os = ...)` 必须收敛在平台层内部，上层不感知。
- `uc-platform/AGENTS.md` §6.1 / §11.3：平台层不定义业务规则；平台怪异行为（如"高层 API 隐式 clear"）由平台层消化。
- `uc-platform/AGENTS.md` §9.3：平台能力差异（Linux 本 phase 暂不支持原子多 rep）必须**显式表达**，不能静默降级 —— 用 `warn!` + FIXME + 可追踪的降级路径呈现。
- `uc-platform/AGENTS.md` §15.1：新增平台依赖必须回答"是否必须""是否会让条件编译复杂度失控"。本次 `objc2` 系列只在 macOS target 下启用，不影响 Linux / Windows 构建。
- 根 `AGENTS.md`：注释与 doc comments 使用中文；commit message 保持英文；`.planning/` 文档统一中文。
- 项目当前无 Rust 测试（commit `6f1d6a2d` 全删）；验证以 `cargo check -p uc-platform` 为主。

</interfaces>

</context>

<tasks>

<task type="auto">
  <name>任务 1：在 Cargo.toml 新增 macOS 平台专属依赖（objc2 / objc2-app-kit / objc2-foundation）</name>
  <files>src-tauri/crates/uc-platform/Cargo.toml</files>
  <action>
目标：让 `uc-platform` 在 macOS target 下能直接用 `objc2-app-kit::NSPasteboard` / `NSPasteboardItem` 与 `objc2-foundation::{NSArray, NSData, NSString}`，为任务 2 提供编译基础。

### 1. 版本核实（先做这一步，不要直接写版本号）

```bash
grep -E 'name = "objc2(|-app-kit|-foundation)"' /Volumes/ExternalSSD/superset/uniclipboard/slender-soybean/src-tauri/Cargo.lock
grep -B1 -A3 'name = "objc2"$' /Volumes/ExternalSSD/superset/uniclipboard/slender-soybean/src-tauri/Cargo.lock | head -10
grep -B1 -A3 'name = "objc2-app-kit"$' /Volumes/ExternalSSD/superset/uniclipboard/slender-soybean/src-tauri/Cargo.lock | head -10
grep -B1 -A3 'name = "objc2-foundation"$' /Volumes/ExternalSSD/superset/uniclipboard/slender-soybean/src-tauri/Cargo.lock | head -10
```

预期（截至 plan 写入时）：`objc2 0.6.3` / `objc2-app-kit 0.3.2` / `objc2-foundation 0.3.2`。版本号写 `Cargo.toml` 时用 **minor-compatible** 表达（`"0.6"` / `"0.3"`），避免未来 patch 升级时反复修订。

### 2. 修改 `src-tauri/crates/uc-platform/Cargo.toml`

在文件末尾（`[target.'cfg(windows)'.dependencies]` 段之后）**追加**：

```toml
# macOS 平台原子多 rep 写入（见 `clipboard/platform/macos.rs::write_snapshot_multi_macos`）。
# 显式启用 NSPasteboard + NSPasteboardItem feature，以拿到 NSPasteboardTypeString /
# NSPasteboardTypeHTML 常量与 setData_forType / writeObjects API。
# objc2-foundation 的 features 虽然被 NSPasteboard feature transitively 启用，
# 但这里显式列出以便阅读；避免未来 objc2-app-kit 的 feature 传递规则变更后静默失效。
[target.'cfg(target_os = "macos")'.dependencies]
objc2 = "0.6"
objc2-app-kit = { version = "0.3", features = ["NSPasteboard", "NSPasteboardItem"] }
objc2-foundation = { version = "0.3", features = ["NSArray", "NSData", "NSString"] }
```

### 3. 其他约束

- **不要**把这些依赖加到顶层 `[dependencies]`。Windows / Linux 构建不需要它们，加到顶层会让 `cargo check` 在 Linux / Windows target 上也下载并编译 objc2 系列（冗余 + 跨编译风险）。
- **不要**动顶层 `[dependencies]` 里的 `arboard = "3.4"` 或 `clipboard-rs` —— 任务 2 直接用 `objc2-app-kit` 原生 API，不走 arboard；但 arboard 仍可能在本 crate 的其他路径用，保持原样。
- **不要**新增 workspace-level 改动；只改这一个 Cargo.toml。
- 所有新增注释用中文（与项目规范一致）。

### 4. 验证

```bash
cd /Volumes/ExternalSSD/superset/uniclipboard/slender-soybean/src-tauri
cargo check -p uc-platform --target x86_64-apple-darwin 2>&1 | tail -30
# 或当前 host 就是 macOS 时：cargo check -p uc-platform
```

应：下载 / 复用 objc2 / objc2-app-kit / objc2-foundation 三个 crate，编译通过（目前 `macos.rs` 还没用到这些依赖，所以只能看到"unused dependency"级别的 warning，最多 `macos.rs` 什么都没引用时连 warning 都不会有）。

### 5. 不做

- 不改 `[features]` 段（本次的能力对所有 macOS 构建都要启用，不放到 feature 后面开关）。
- 不新增别的 objc2-* 子 crate（`objc2-app-kit` 已经把 `NSPasteboardItem` / `NSPasteboardWriting` 都带进来）。
- 不添加 Linux 平台专属依赖段（Linux 留给下一个 phase）。
  </action>
  <verify>
    <automated>cd /Volumes/ExternalSSD/superset/uniclipboard/slender-soybean/src-tauri && cargo check -p uc-platform --target x86_64-apple-darwin 2>&1 | tee /tmp/uc-platform-cargo-task1.log; ! grep -E "^error" /tmp/uc-platform-cargo-task1.log</automated>
  </verify>
  <done>
- `Cargo.toml` 新增 `[target.'cfg(target_os = "macos")'.dependencies]` 段，列 `objc2` / `objc2-app-kit` / `objc2-foundation` 三条。
- `cargo check -p uc-platform --target x86_64-apple-darwin` 通过（仅当前 host 为 macOS；若本地 host 不是 macOS，先装 target：`rustup target add x86_64-apple-darwin`）。
- 无新的 error / 无与依赖解析相关的 warning；若出现 "unused Cargo.toml 键" 之类 warning，说明版本或 features 写错，需修到零 warning。
- `Cargo.lock` 被 cargo 自动更新，新增 `uc-platform` → `objc2-app-kit / objc2-foundation` 的直接依赖边；该更新一并 commit。
- commit message 英文，例如 `build(uc-platform): add objc2/objc2-app-kit deps for macOS multi-rep write`。
  </done>
</task>

<task type="auto">
  <name>任务 2：macOS 原子多 rep 写入实现（write_snapshot_multi_macos）</name>
  <files>src-tauri/crates/uc-platform/src/clipboard/platform/macos.rs</files>
  <action>
目标：在 `macos.rs` 新增 `pub(crate) fn write_snapshot_multi_macos(snapshot: SystemClipboardSnapshot) -> Result<()>`，在**一次** `NSPasteboard::writeObjects` 调用内原子写入 `NSPasteboardTypeString`（对应 `text/plain`）+ `NSPasteboardTypeHTML`（对应 `text/html`）。

### 1. API 核实清单（实现前必须核实，不要凭 plan 里的伪代码脑补）

**参考任务背景中 scope_guidance / interfaces 节 C**：本次所有 objc2 API 细节都必须由执行者用以下方式之一核实：

方式 A（首选，最快）：直接 grep 本机 registry 源文件。
```bash
# NSPasteboard / NSPasteboardItem 签名
grep -E 'pub (unsafe )?fn|setData_forType|writeObjects|clearContents|generalPasteboard' \
  ~/.cargo/registry/src/index.crates.io-*/objc2-app-kit-0.3.2/src/generated/NSPasteboard.rs \
  ~/.cargo/registry/src/index.crates.io-*/objc2-app-kit-0.3.2/src/generated/NSPasteboardItem.rs | head -40

# NSData 构造函数实际名（极易出错 —— 旧版 objc2 用 dataWithBytes_length，新版可能是 with_bytes 或 from_vec）
find ~/.cargo/registry/src/index.crates.io-*/objc2-foundation-0.3.2/src -name 'NSData*' -exec grep -l 'pub fn\|pub unsafe fn' {} \;
grep -E 'pub (unsafe )?fn (new|init|from|with|dataWith)' ~/.cargo/registry/src/index.crates.io-*/objc2-foundation-0.3.2/src/**/NSData* 2>/dev/null | head -30

# NSArray<ProtocolObject<...>> 构造（from_retained_slice / from_slice / from_vec 三选一）
grep -E 'pub fn (from|new)' ~/.cargo/registry/src/index.crates.io-*/objc2-foundation-0.3.2/src/**/NSArray* 2>/dev/null | head -20

# ProtocolObject 的包装 API
grep -E 'pub fn from' ~/.cargo/registry/src/index.crates.io-*/objc2-0.6.3/src/runtime/protocol_object.rs 2>/dev/null | head -10

# NSPasteboardWriting 协议是否已自动为 NSPasteboardItem 实现
grep -E 'impl.*NSPasteboardWriting|unsafe impl ProtocolType' \
  ~/.cargo/registry/src/index.crates.io-*/objc2-app-kit-0.3.2/src/generated/NSPasteboardItem.rs | head -5

# MainThreadMarker 要求：看 generalPasteboard / writeObjects 是否要求 MainThreadOnly
grep -B2 -A2 'MainThreadMarker\|MainThreadOnly\|#\[unsafe(method(' \
  ~/.cargo/registry/src/index.crates.io-*/objc2-app-kit-0.3.2/src/generated/NSPasteboard.rs | head -50
```

方式 B：用 `cargo doc --open -p objc2-foundation` 和 `cargo doc --open -p objc2-app-kit` 打开本地 rustdoc。

**绝对不要**：凭 ChatGPT / 记忆 / 旧 blog 的 objc2 代码范例直接写。objc2 从 0.4 → 0.5 → 0.6 的 API 变化比一般 crate 剧烈；旧范例几乎肯定编不过。

核实后在**你实现的函数 doc comment 顶部**用中文写一段"实测 API 版本：objc2-app-kit 0.3.2，核实的函数签名如下……"的记录，便于后续维护。这是本任务 SUMMARY 必须包含的"偏差记录"素材。

### 2. 函数骨架

在 `macos.rs` 文件末尾（`impl SystemClipboardPort for MacOSClipboard` 块之后）**追加**：

```rust
use anyhow::anyhow;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{
    NSPasteboard, NSPasteboardItem, NSPasteboardTypeHTML, NSPasteboardTypeString,
    NSPasteboardWriting,
};
use objc2_foundation::{NSArray, NSData, NSString};
use tracing::{debug, info, warn};
use uc_core::clipboard::ObservedClipboardRepresentation;

/// 推断 rep 在 macOS 多 rep 写入路径下的"有效 MIME"。
///
/// 与 `common.rs` 单 rep 快路径、`windows.rs::resolve_multi_rep_mime` 保持一致的
/// 推断表：显式 mime → 使用；否则 format_id 映射。
fn resolve_multi_rep_mime(rep: &ObservedClipboardRepresentation) -> Option<&str> {
    rep.mime
        .as_ref()
        .map(|m| m.as_str())
        .or_else(|| match rep.format_id.as_str() {
            "public.utf8-plain-text" | "public.text" | "NSStringPboardType" | "text" => {
                Some("text/plain")
            }
            "public.html" | "Apple HTML pasteboard type" | "html" => Some("text/html"),
            _ => None,
        })
}

/// macOS 原子多 representation 写入。
///
/// 与 `windows.rs::write_snapshot_multi_windows` 的 MVP 对齐：仅支持 `text/plain`
/// （→ `NSPasteboardTypeString` / `public.utf8-plain-text`）+ `text/html`
/// （→ `NSPasteboardTypeHTML` / `public.html`）。其他 mime（image / rtf / files）
/// 在多 rep 路径里跳过并 debug 日志，留待后续 phase。
///
/// ## 为何用 NSPasteboardItem + writeObjects:
///
/// macOS 真正的"原子多 rep 写入" API 是 `NSPasteboard::writeObjects:` —— 把一个
/// 承载了多个 type/data 对的 `NSPasteboardItem` 一次性提交给 pasteboard。
/// 这保证目的地应用（无论是终端、TextEdit 纯文本模式、还是富文本 Pages）在同一
/// changeCount 下看到的是**同一组 representation**，而不是被分步写入时的中间状态。
///
/// 对比其他不可用方案：
/// - `setString:forType:` 每次调用都**不会**隐式 clear（这点与 Windows 不同），
///   但每次调用会创建一个新的 NSPasteboardItem，导致 pasteboard.items 数量膨胀，
///   部分目的地应用只取第一个 item，出现"看不到 HTML"的情况。
/// - `setData:forType:` 同上问题。
/// - `declareTypes:owner: + setData:forType:`（旧 API）会和 `NSPasteboardItem` 路径
///   语义混乱，Apple 文档明确建议新代码使用 `writeObjects:`。
///
/// ## 会话顺序
/// 1. `NSPasteboard::generalPasteboard()`（拿系统剪贴板单例）
/// 2. `clearContents()`（清空旧内容；返回 changeCount，这里忽略返回值）
/// 3. 构造 `NSPasteboardItem::new()`
/// 4. 对每个可写 rep，调用 `item.setData_forType(&NSData, type_const)`
/// 5. `writeObjects:` 一次提交（返回 bool；false 当 Err 上抛）
///
/// 中间任一 `setData:forType:` 返回 false（比如 type 已存在但类型冲突）都累计到
/// `skipped` 里，不中断流程——writeObjects 的原子性由 Apple 保证，只要成功返回就
/// 证明最终 pasteboard 里包含当次成功 set 的所有 type。
///
/// ## 线程安全
/// 根据实测 API 核实：`NSPasteboard::generalPasteboard` / `clearContents` /
/// `writeObjects:` 在 objc2-app-kit 0.3.2 中**不**要求 MainThreadMarker（
/// `#[unsafe(method(...))]` 标注而非 `#[method_id]` + MainThreadOnly）。因此可在
/// 后台线程直接调用，与现有 `MacOSClipboard` 的 tokio 异步使用场景兼容。
/// 若未来升级 objc2-app-kit 到要求 MainThreadMarker 的版本，需要改造为
/// `MainThreadMarker::new_unchecked()` 或经由 main thread dispatch。
/// （务必在**你的实现**里用实际核实结果替换这段 doc 注释！）
pub(crate) fn write_snapshot_multi_macos(
    snapshot: SystemClipboardSnapshot,
) -> Result<()> {
    // 预扫描：snapshot 中至少要有一条可写 rep（text/plain 或 text/html）。
    // 否则直接 bail，不打开 / 不 clear pasteboard ——
    // 避免 clearContents() 把用户原本的 clipboard 内容抹掉却什么都写不进去。
    // （与 Windows 任务的 "empty() 副作用防御" 同构。）
    let writable_reps: Vec<&ObservedClipboardRepresentation> = snapshot
        .representations
        .iter()
        .filter(|rep| {
            matches!(
                resolve_multi_rep_mime(rep),
                Some("text/plain") | Some("text/html")
            )
        })
        .collect();

    if writable_reps.is_empty() {
        let skipped: Vec<String> = snapshot
            .representations
            .iter()
            .map(|r| r.format_id.as_str().to_string())
            .collect();
        anyhow::bail!(
            "macOS 多 rep 写入：无可写 rep（支持 text/plain, text/html）；\
             未清空系统 pasteboard；跳过的 rep = {:?}",
            skipped
        );
    }

    // 1. 拿 general pasteboard 单例
    let pasteboard: Retained<NSPasteboard> = unsafe { NSPasteboard::generalPasteboard() };

    // 2. 清空旧内容；忽略返回的 changeCount。
    let _ = unsafe { pasteboard.clearContents() };

    // 3. 构造 item，依次 setData
    let item: Retained<NSPasteboardItem> = unsafe { NSPasteboardItem::new() };

    let mut wrote_any = false;
    let mut skipped: Vec<String> = Vec::new();

    for rep in &snapshot.representations {
        match resolve_multi_rep_mime(rep) {
            Some("text/plain") => {
                // 注意：setData_forType 接受原始字节 —— text/plain 的 bytes 是 UTF-8，
                // 但 macOS pasteboard 对 NSPasteboardTypeString 的字节格式其实就是
                // UTF-8（Apple 推荐），直接写原始字节即可。不要先 String::from_utf8 再
                // 回编码成 NSString（多一次转换，且对非法 UTF-8 会误报）。
                let data = make_nsdata(&rep.bytes);
                let ok = unsafe {
                    item.setData_forType(&data, NSPasteboardTypeString)
                };
                if ok {
                    debug!(
                        bytes = rep.bytes.len(),
                        "写入 NSPasteboardTypeString 成功"
                    );
                    wrote_any = true;
                } else {
                    warn!(
                        bytes = rep.bytes.len(),
                        "setData_forType(NSPasteboardTypeString) 返回 false"
                    );
                    skipped.push(rep.format_id.as_str().to_string());
                }
            }
            Some("text/html") => {
                let data = make_nsdata(&rep.bytes);
                let ok = unsafe {
                    item.setData_forType(&data, NSPasteboardTypeHTML)
                };
                if ok {
                    debug!(bytes = rep.bytes.len(), "写入 NSPasteboardTypeHTML 成功");
                    wrote_any = true;
                } else {
                    warn!(
                        bytes = rep.bytes.len(),
                        "setData_forType(NSPasteboardTypeHTML) 返回 false"
                    );
                    skipped.push(rep.format_id.as_str().to_string());
                }
            }
            other => {
                debug!(
                    format_id = %rep.format_id,
                    mime = ?other,
                    "macOS 多 rep 写入：跳过不支持的 rep"
                );
                skipped.push(rep.format_id.as_str().to_string());
            }
        }
    }

    if !wrote_any {
        // 所有 writable rep 都在 setData_forType 阶段返回 false —— 极罕见。
        // 此时 item 是空的，writeObjects 也不会提交有意义的内容；但我们已经调过
        // clearContents() —— OS pasteboard 已被清空。这是 macOS 路径下唯一会发生
        // "clear 了但没写进去"的情况，必须让上层知道。
        anyhow::bail!(
            "macOS 多 rep 写入：所有候选 rep setData_forType 均失败；\
             pasteboard 已被清空但无法写入；跳过的 rep = {:?}",
            skipped
        );
    }

    // 4. 包装成 writeObjects 需要的 NSArray<ProtocolObject<dyn NSPasteboardWriting>>
    let proto_item: Retained<ProtocolObject<dyn NSPasteboardWriting>> =
        ProtocolObject::from_retained(item);
    let items_array: Retained<NSArray<ProtocolObject<dyn NSPasteboardWriting>>> =
        NSArray::from_retained_slice(&[proto_item]);

    // 5. 原子提交
    let ok = unsafe { pasteboard.writeObjects(&items_array) };
    if !ok {
        return Err(anyhow!(
            "NSPasteboard.writeObjects 返回 false；pasteboard 可能处于不一致状态（已 clearContents 但本次写入失败）"
        ));
    }

    if !skipped.is_empty() {
        debug!(
            skipped_count = skipped.len(),
            skipped = ?skipped,
            "macOS 多 rep 写入：部分 rep 已跳过（不支持或 setData 失败）"
        );
    }

    info!(
        total_reps = snapshot.representations.len(),
        skipped = skipped.len(),
        "macOS 原子多 rep 写入完成"
    );

    Ok(())
}

/// 把 &[u8] 包装成 NSData。
///
/// 具体构造函数名（`NSData::with_bytes(&[u8])` / `NSData::from_vec(Vec<u8>)` /
/// `NSData::dataWithBytes_length(...)` 之一）执行者需按 API 核实的结果选择。
/// 本 helper 存在的意义：把"到底叫什么名"的细节收在一处，后续升级 objc2 版本时
/// 只改这里。
fn make_nsdata(bytes: &[u8]) -> Retained<NSData> {
    // TODO(执行者)：按 API 核实的实际函数名实现。可能是：
    //   NSData::with_bytes(bytes)
    //   NSData::from_vec(bytes.to_vec())
    //   unsafe { NSData::dataWithBytes_length(bytes.as_ptr() as *const _, bytes.len()) }
    // 不要保留 TODO —— 实现时必须替换为真实调用。
    todo!("按核实的 objc2-foundation 0.3.2 API 实现 NSData 构造")
}
```

> **重要**：上面的骨架里保留了 `todo!()` 与伪代码注释，是**有意**的 —— 让执行者必须把 API 核实结果落到实际代码里，不能复制粘贴就走。执行者提交前务必删掉 `todo!()`。

### 3. 调用位点 / 导出

- `write_snapshot_multi_macos` 必须是 `pub(crate)`。`common.rs` 在任务 3 中将通过 `crate::clipboard::platform::macos::write_snapshot_multi_macos(snapshot)` 调用，不跨 crate 暴露。
- `MacOSClipboard::write_snapshot` **不需要改动**：它当前通过 `CommonClipboardImpl::write_snapshot(&mut ctx, snapshot)` 进入 common.rs，common.rs 会自动分流到新增的 `write_snapshot_multi_macos`。
  - **关键差异（相比 Windows）**：macOS 不需要 "提前 drop clipboard-rs ctx + 临时 dummy_ctx" 的 workaround —— 因为 `objc2-app-kit::NSPasteboard::generalPasteboard()` 与 `clipboard-rs` 的底层（最终也是 NSPasteboard）**不会抢句柄**：macOS NSPasteboard 不是 Windows 那种独占 OpenClipboard 模型。在 doc comment 里明确写出这个对比。

### 4. 代码注释规范

- 所有新增注释与 doc comments 使用中文。
- 函数 doc comment 至少覆盖：
  1. 为何用 `NSPasteboardItem + writeObjects:` 而不是 `setString:forType:`（见骨架里的注释）
  2. API 核实版本（`objc2-app-kit 0.3.2`，关键函数签名）—— 并标注"若未来升级版本需重新核实"
  3. 与 Windows 路径的对比（macOS 不需要 dummy_ctx）
  4. 本次 MVP 仅 text/plain + text/html，其他 mime 留待后续
  5. `clearContents()` 副作用的防御（前置 writable 扫描）

### 5. 不做

- 不实现 image / rtf / files 的多 rep 写入（保持 MVP；注释里标注"后续 phase 补齐 NSPasteboardTypePNG / NSPasteboardTypeRTF 等"）。
- 不改 `MacOSClipboard::read_snapshot` / `MacOSClipboard::write_snapshot`。
- 不引入 `arboard` 的 NSPasteboard 封装（我们直接用 objc2-app-kit，边界更清楚）。
- 不增加任何单元测试（项目已无 Rust 测试）。

### 6. 自我审查清单（提交前逐项勾）

- [ ] `write_snapshot_multi_macos` 签名为 `pub(crate) fn ...(snapshot: SystemClipboardSnapshot) -> Result<()>`。
- [ ] 源码里没有遗留 `todo!()` / `unimplemented!()`。
- [ ] 使用了实测的 `NSData` / `NSArray` / `ProtocolObject` 构造函数名（在 doc comment 里记录了版本）。
- [ ] 前置 writable 扫描覆盖 "全不可写" 场景，不 clearContents 就 bail。
- [ ] `setData_forType` / `writeObjects` 的 bool 返回值都被检查；false 转 Err / warn + skip。
- [ ] 日志面向排障：哪个 rep、什么 mime、setData 成功 / 失败、总写入 / skip 数量。
- [ ] 所有新增注释为中文。
- [ ] 文件顶部没有 `#[cfg(target_os = "macos")]` 装饰 —— 不需要，`macos.rs` 整个文件已经在 `platform/mod.rs` 的 `#[cfg(target_os = "macos")]` 门控下。
  </action>
  <verify>
    <automated>cd /Volumes/ExternalSSD/superset/uniclipboard/slender-soybean/src-tauri && cargo check -p uc-platform --target x86_64-apple-darwin 2>&1 | tee /tmp/uc-platform-cargo-task2.log; ! grep -E "^error|\\btodo!\\(|unimplemented!\\(" /tmp/uc-platform-cargo-task2.log src-tauri/crates/uc-platform/src/clipboard/platform/macos.rs</automated>
  </verify>
  <done>
- `macos.rs` 新增 `pub(crate) fn write_snapshot_multi_macos(snapshot: SystemClipboardSnapshot) -> Result<()>`，在单次 pasteboard 会话内用 `NSPasteboardItem::setData_forType` + `NSPasteboard::writeObjects` 提交 `NSPasteboardTypeString` + `NSPasteboardTypeHTML`。
- `cargo check -p uc-platform --target x86_64-apple-darwin` 编译通过，零 error，零新 warning（与依赖相关的 unused import 也要消灭）。
- 源码中**没有**任何 `todo!()` / `unimplemented!()`；`NSData` / `NSArray` / `ProtocolObject` 的构造函数名已被替换为 API 核实的实际名称。
- 函数 doc comment 用中文写明：(1) MVP 仅 text/plain + text/html、(2) 前置 writable 扫描的防御理由、(3) 与 Windows 路径"不需要 dummy_ctx"的对比、(4) API 核实版本（`objc2-app-kit 0.3.2`）。
- `MacOSClipboard::write_snapshot` **零改动**；`read_snapshot` / 文件顶部 `use` 块仅按需增加（objc2 / objc2-app-kit / objc2-foundation 的 import）。
- git diff 约束：本任务仅改 `macos.rs`；`Cargo.toml` 已在任务 1 落地，不重复动。
- commit message 英文，例如 `feat(uc-platform/macos): atomic multi-rep clipboard write via NSPasteboardItem`。
  </done>
</task>

<task type="auto">
  <name>任务 3：拆分 common.rs 的 not-windows 分支（macOS 委派 + Linux 显式降级）</name>
  <files>src-tauri/crates/uc-platform/src/clipboard/common.rs</files>
  <action>
目标：把 `CommonClipboardImpl::write_snapshot_multi` 里 `#[cfg(not(target_os = "windows"))]` 的单一分支，拆成 `#[cfg(target_os = "macos")]` 与 `#[cfg(any(target_os = "linux", not(any(target_os = "windows", target_os = "macos"))))]` 两支。macOS 委派任务 2 新增的 `write_snapshot_multi_macos`；Linux 保留既有 V1-policy 降级实现，加上 FIXME 注释指向下一个 phase。

### 1. 定位现有代码

定位 `common.rs` 中 `fn write_snapshot_multi` 的 `#[cfg(not(target_os = "windows"))]` 块（当前在 `#L741-786`，已在 `<interfaces>` 节 A 里引用）。

### 2. 替换为两支分流

把整个 `#[cfg(not(target_os = "windows"))]` 块替换为：

```rust
#[cfg(target_os = "macos")]
{
    // macOS：具备真正的原子多 rep 写入能力（NSPasteboardItem + writeObjects:）。
    // 实现在 `clipboard::platform::macos::write_snapshot_multi_macos`。
    // 该函数自己通过 `NSPasteboard::generalPasteboard()` 拿系统剪贴板单例，
    // 不使用传入的 clipboard-rs `ctx`，也不需要 "提前 drop + dummy_ctx" 的绕道
    //（与 Windows 不同：macOS NSPasteboard 不是独占句柄模型）。
    let _ = ctx;  // 显式标注未使用，消除 unused-variable warning（若有）。
    return crate::clipboard::platform::macos::write_snapshot_multi_macos(snapshot);
}

// Linux 与其他非 Windows / 非 macOS 的 Unix：显式降级（§9.3 不允许静默降级）。
//
// FIXME(260423-mxu-next-phase)：Linux 的真正多 rep 原子写入需要 Wayland
// `wl-clipboard-rs` 的 DataSource 接口（多 MIME type 注册）或 X11 的
// selection owner 持久持有模型；二者与 `clipboard-rs` 高层 API 不兼容，
// 工作量与 macOS 相当，留到下一个独立 phase 补齐。本次保留以下 V1-policy
// 降级逻辑，语义与 260423-9do 改造前完全一致，保证浏览器复制到 Linux 的
// 粘贴行为不回归。
#[cfg(any(
    target_os = "linux",
    not(any(target_os = "windows", target_os = "macos"))
))]
{
    // 用 V1 policy 选出 paste-priority rep —— 与应用层原 `narrow_to_primary`
    // 等价。硬编码 V1：当前 uc-core 只有这一个 `SelectRepresentationPolicyPort`
    // 实现；出现 V2 时再考虑从调用方注入 policy。
    use uc_core::clipboard::SelectRepresentationPolicyV1;
    use uc_core::ports::SelectRepresentationPolicyPort;

    let policy = SelectRepresentationPolicyV1::default();
    let selection = policy
        .select(&snapshot)
        .map_err(|e| anyhow!("representation policy failed: {e}"))?;
    let paste_id = selection.paste_rep_id.clone();

    let chosen_idx = snapshot
        .representations
        .iter()
        .position(|rep| rep.id == paste_id)
        .ok_or_else(|| {
            anyhow!(
                "policy selected paste_rep_id {:?} not present in snapshot",
                paste_id
            )
        })?;

    warn!(
        rep_count,
        paste_rep_id = ?paste_id,
        chosen_format_id = %snapshot.representations[chosen_idx].format_id,
        "Linux: multi-representation atomic write not yet supported; \
         falling back to single-rep path via SelectRepresentationPolicyV1 \
         — will be addressed in a follow-up phase (wl-clipboard-rs / X11 \
         selection owner)."
    );

    let ts_ms = snapshot.ts_ms;
    let mut reps = snapshot.representations;
    let chosen = reps.remove(chosen_idx);
    let reduced = SystemClipboardSnapshot {
        ts_ms,
        representations: vec![chosen],
    };
    return Self::write_snapshot(ctx, reduced);
}
```

> 注意 `rep_count` 变量仍在函数顶部被定义（本任务之前已存在），所以两个分支都能直接用。如果发现 `rep_count` 在改动后 macOS 分支下会触发 unused-variable warning，把 `let rep_count = snapshot.representations.len();` 行上方加 `#[allow(unused_variables)]`——或更清爽：把它移到 Linux 分支内部（因为 macOS 分支用不到它），但注意这会让 `rep_count` 的 shadowing 复杂化；**推荐**：保持顶部 `let rep_count = ...;` 并加 `let _ = rep_count;` 在 macOS 分支开头，消除 warning。

### 3. 更新 `write_snapshot` 的顶部 doc comment

现在 `#L571-586`（当前代码）说的"macOS / Linux：暂不支持原子多 rep，降级为…… 后续 phase 补齐 NSPasteboardItem / Wayland data source 实现"已经**不再是事实**（macOS 已经支持）。改为：

```rust
/// 写入 `SystemClipboardSnapshot` 到系统剪贴板。
///
/// 分流策略：
/// 1. `representations.len() == 1`：走 `clipboard-rs` 高层 API 快路径（跨平台）。
///    —— 由 `clipboard-rs` 封装 set_text / set_html / set_image / set_files 等，
///    行为与早期版本完全一致。
/// 2. `representations.len() > 1`：进入 `write_snapshot_multi` 分流：
///    - Windows：原子多 rep 写入（`write_snapshot_multi_windows`）——在单次
///      `OpenClipboard` 会话内用 `raw::set_without_clear` 累加 CF_UNICODETEXT
///      + CF_HTML 等多个 format，确保纯文本目的地也能粘贴。
///    - macOS：原子多 rep 写入（`write_snapshot_multi_macos`）——在单次
///      `NSPasteboard::writeObjects:` 调用内提交 `NSPasteboardItem`，承载
///      `NSPasteboardTypeString` + `NSPasteboardTypeHTML`，与 Windows 语义对齐。
///    - Linux / 其他 Unix：暂不支持原子多 rep（Wayland `wl-clipboard-rs` 与 X11
///      selection owner 模型与 `clipboard-rs` 不兼容，工作量独立），当前降级为
///      "用 `SelectRepresentationPolicyV1` 选出 paste-priority rep 再走单 rep
///      快路径"并 warn 日志。后续 phase 补齐（FIXME 见分支内注释）。
///
/// 历史背景见 https://github.com/UniClipboard/UniClipboard/issues/92
/// 以及 `uc-platform/src/clipboard/platform/{windows,macos}.rs`。
```

### 4. 更新 `write_snapshot_multi` 自身的 doc comment

定位 `#L715-726` 的 doc comment（"多 representation 写入入口……macOS / Linux：当前尚未支持……"），改为：

```rust
/// 多 representation 写入入口。
///
/// 平台能力差异：
/// - Windows：具备真正的原子多 rep 写入（`write_snapshot_multi_windows`），在单次
///   `OpenClipboard` 会话内用 `raw::set_without_clear` 累加 CF_UNICODETEXT + CF_HTML，
///   确保纯文本目的地（记事本等）也能粘贴到正确内容。
/// - macOS：具备真正的原子多 rep 写入（`write_snapshot_multi_macos`），在单次
///   `NSPasteboard::writeObjects:` 调用内提交 `NSPasteboardItem`。
/// - Linux / 其他 Unix：当前尚未支持（需 `wl-clipboard-rs` DataSource 或 X11
///   selection owner 重写），本次降级为 "用 `SelectRepresentationPolicyV1` 选出
///   paste-priority rep 再走单 rep 快路径"，并以 `warn!` 日志显式说明。行为与
///   应用层原 `narrow_to_primary` 等价，保证 Linux 粘贴语义零回归。后续 phase
///   再统一（§9.3：不允许静默降级）。
///
/// 注意：本方法不应被"单 rep 快路径"调用。调用者需保证 `snapshot.representations.len() >= 1`。
```

### 5. 检查点

- `#[cfg(target_os = "macos")]` 必须**只**出现在本文件 `write_snapshot_multi` 内部 + `macos.rs` 已经天然在 `platform/mod.rs` 的 `#[cfg(target_os = "macos")]` 门控下（§4.4）。
- `#[cfg(target_os = "linux")]` / `#[cfg(not(any(target_os = "windows", target_os = "macos")))]` 的组合必须保证"任何 target 至少命中其中一个分支"——上面的 `#[cfg(any(target_os = "linux", not(any(target_os = "windows", target_os = "macos"))))]` 能覆盖 Linux、FreeBSD、iOS 等所有非 Windows / 非 macOS 的 target。
- Windows 分支 `#[cfg(target_os = "windows")]` **不动**，保持既有委派行为。
- 不要改 `write_snapshot` 的单 rep 快路径、不要改 `read_snapshot`、不要改 port 签名。
- `anyhow!` 的 import 已经在文件顶部存在（`use anyhow::{anyhow, Result};`，`#L1`），不需要重复引入。

### 6. 不做

- 不引入新 crate 依赖。
- 不改 `macos.rs` / `windows.rs` / `linux.rs`。
- 不引入新 Rust 测试。
- 不处理 macOS 分支下 `write_snapshot_multi_macos` 自身 bail 的 fallback —— 按 §6.1 "平台层不替业务决定"：macOS 的 bail 直接作为错误上抛给调用方（`MacOSClipboard::write_snapshot` → `SystemClipboardPort::write_snapshot`），由 app 层决定如何处理（当前 app 层已经在错误路径有日志，不需本次处理）。
  </action>
  <verify>
    <automated>cd /Volumes/ExternalSSD/superset/uniclipboard/slender-soybean/src-tauri && cargo check -p uc-platform --target x86_64-apple-darwin 2>&1 | tee /tmp/uc-platform-cargo-task3.log; ! grep -E "^error|warning: unused" /tmp/uc-platform-cargo-task3.log</automated>
  </verify>
  <done>
- `common.rs::write_snapshot_multi` 内的单一 `#[cfg(not(target_os = "windows"))]` 分支已被拆为 `#[cfg(target_os = "macos")]`（委派）+ `#[cfg(any(target_os = "linux", not(any(target_os = "windows", target_os = "macos"))))]`（显式降级 + FIXME）。
- `cargo check -p uc-platform --target x86_64-apple-darwin` 通过，零 error 零新 warning（包括 `unused_variables` / `dead_code`）。
- `write_snapshot` 与 `write_snapshot_multi` 的顶部 doc comment 均已更新为三段式（Windows / macOS / Linux）策略，不再写"macOS 未支持"。
- Linux 分支保留既有 V1-policy 降级实现 + 新增 `FIXME(260423-mxu-next-phase)` 注释，清晰指向下一个 phase 的 wl-clipboard-rs / X11 工作。
- Windows 分支（`crate::clipboard::platform::windows::write_snapshot_multi_windows`）完全不动。
- git diff 约束：本任务仅改 `common.rs`；其余文件零改动。
- commit message 英文，例如 `refactor(uc-platform): wire macOS multi-rep, keep Linux explicit fallback`。
  </done>
</task>

</tasks>

<verification>

## 整体验证

### 1. 编译（主硬性门槛）

```bash
cd /Volumes/ExternalSSD/superset/uniclipboard/slender-soybean/src-tauri

# macOS target：必须通过。任务 2 / 3 的真正代码在 macOS 下才被编译进去。
cargo check -p uc-platform --target x86_64-apple-darwin

# Linux target：必须通过。任务 3 的 Linux 分支覆盖率。
#（若本地未装 target，可尝试 rustup target add x86_64-unknown-linux-gnu；若因 libsodium
# 等系统库缺失而交叉编译失败，退化为仅 macOS 验证 + 注明 Linux 侧由 CI 验证；这与
# 260423-9do 任务对 Windows target 的处理一致。）
cargo check -p uc-platform --target x86_64-unknown-linux-gnu

# Windows target：不变性验证，确保任务 3 的 cfg 拆分没有误伤 Windows 分支。
#（与上同：若交叉编译不可用，退化为 CI 验证。）
cargo check -p uc-platform --target x86_64-pc-windows-msvc
```

三个目标都应零 error、零新 warning。若本地无法交叉编译所有 target，**至少** macOS target 必须通过（任务 2 的核心工作），其他 target 由 CI 验证并在 SUMMARY 中显式标注"待实机 / CI 验证"。

### 2. Git diff 约束

```bash
git diff --name-only
```

应**只**出现：
- `src-tauri/crates/uc-platform/Cargo.toml`
- `src-tauri/crates/uc-platform/Cargo.lock`（由 task 1 的 cargo 自动更新，如 workspace-level lock，则可能在 `src-tauri/Cargo.lock`）
- `src-tauri/crates/uc-platform/src/clipboard/platform/macos.rs`
- `src-tauri/crates/uc-platform/src/clipboard/common.rs`

不允许改动 `windows.rs`、`linux.rs`、`mod.rs`、`apply_inbound.rs`、`primary_rep_selector.rs`、`local_clipboard.rs`（port）。

### 3. 行为验证（手动，非硬要求）

1. **macOS（当前开发机）**：
   - 临时在 bootstrap / daemon 启动路径里加一个 `#[cfg(debug_assertions)]` 验证钩子，构造一个 `{text/plain + text/html}` 两 rep 的 `SystemClipboardSnapshot`，**绕开** `narrow_to_primary` 直接调 `MacOSClipboard::write_snapshot`（验证完删除）。
   - 在 TextEdit 富文本模式下 Cmd+V：应该看到带格式的 HTML 渲染。
   - 在 TextEdit 纯文本模式（Format → Make Plain Text）或终端 Cmd+V：应该看到纯文本内容（证明 `NSPasteboardTypeString` 被写入）。
   - 如上都成立，则 macOS 原子多 rep 写入能力闭环。
2. **macOS Linux 分支语义验证（只读代码）**：读 `common.rs` 的 `#[cfg(any(target_os = "linux", ...))]` 分支，确认其与改前的 `#[cfg(not(target_os = "windows"))]` 分支逻辑**除了 FIXME 注释外**字节级相同；保证 Linux 行为零回归。
3. **Windows（VM 或 CI）**：`cargo check -p uc-platform --target x86_64-pc-windows-msvc` 通过即可。本次不动任何 Windows 代码路径。

上述第 1 步不是本 plan 的 `<done>` 必须项（本 plan 只交付"能力"，不负责接线），但执行者若能顺手验证一次会极大加速后续"删除 narrow_to_primary，让 apply_inbound 直接 write 全 snapshot" phase 的信心。

### 4. API 核实记录（SUMMARY 必备）

任务 2 的 SUMMARY 必须包含"偏差记录 / API 核实"段，列出实际采用的：
- `NSData` 构造函数的确切名字（`NSData::with_bytes(...)` / `NSData::from_vec(...)` / `NSData::dataWithBytes_length` 中的哪一个）
- `NSArray::from_retained_slice` 的确切名字与签名
- `ProtocolObject::from_retained` 的确切签名
- 是否需要 `MainThreadMarker`（根据实际 objc2-app-kit 0.3.2 源码核实）

此记录是本 phase 的关键资产 —— 后续 phase（Linux / image-multi-rep / 删 narrow_to_primary）都可能再碰 objc2，有这份记录可以省掉再次踩坑。

</verification>

<success_criteria>

- [ ] `cargo check -p uc-platform --target x86_64-apple-darwin` 通过，无新 warning（三个任务都通过后）。
- [ ] `Cargo.toml` 新增 `[target.'cfg(target_os = "macos")'.dependencies]` 段，显式列 `objc2` / `objc2-app-kit` / `objc2-foundation`，features 经过核实。
- [ ] `macos.rs::write_snapshot_multi_macos` 存在，签名 `pub(crate) fn ... -> Result<()>`，在单次 `NSPasteboard::writeObjects:` 会话内原子写入 `NSPasteboardTypeString` + `NSPasteboardTypeHTML`。
- [ ] `macos.rs` 源码中无 `todo!()` / `unimplemented!()`；`NSData` / `NSArray` / `ProtocolObject` 的具体构造 API 已用核实结果替换（不是骨架里的 `todo!()`）。
- [ ] `common.rs::write_snapshot_multi` 的 `#[cfg(not(target_os = "windows"))]` 分支被拆为 macOS（委派）+ Linux（显式降级 + FIXME）两支；Windows 分支不变。
- [ ] `write_snapshot` 与 `write_snapshot_multi` 的 doc comment 均已更新为"Windows / macOS / Linux"三段式，不再写"macOS 未支持"。
- [ ] Linux 分支仍走 V1-policy 降级，**行为语义**与改前等价；新增 `FIXME(260423-mxu-next-phase)` 注释指向后续 phase。
- [ ] 所有新增代码注释使用中文；commit message 英文；每个任务一个原子 commit（共三个 commit：Cargo.toml + macos.rs + common.rs）。
- [ ] git diff 仅涉及 `Cargo.toml` / `Cargo.lock` / `macos.rs` / `common.rs` 四个文件；`windows.rs` / `linux.rs` / `mod.rs` / `apply_inbound.rs` / port 定义**零改动**。
- [ ] SUMMARY 包含 "API 核实 / 偏差记录" 段，列出实际采用的 `NSData` / `NSArray` / `ProtocolObject` 构造函数名与版本。

</success_criteria>

<output>
完成后创建 `.planning/quick/260423-mxu-macos-linux-rep-rep-policy/260423-mxu-SUMMARY.md`（中文），至少包含：

- 三个任务的实际 diff 概述（Cargo.toml 加了哪些依赖、macos.rs 新增了什么函数、common.rs 如何拆分）
- **API 核实 / 偏差记录**（必须）：
  - `NSData` 构造函数的实际名字与签名（与 plan 骨架里的伪代码 `NSData::with_bytes` 对比）
  - `NSArray::from_retained_slice` / `ProtocolObject::from_retained` 的实际签名
  - 是否需要 `MainThreadMarker`
  - 若在实现过程中发现 plan 骨架中有任何伪代码 / API 名字是错的，按 Rule 1 自动修正并在此详记
- `write_snapshot_multi_macos` 的实测状态（是否在 macOS 实机 Cmd+V 验证过；若仅 `cargo check` 通过，明确标注"实机 Cmd+V 验证待做"）
- Windows / Linux target 的编译验证状态（通过 / 交叉编译环境缺失 / 由 CI 验证）
- 已知后续工作（按 scope_guidance 原文对齐）：
  1. Linux 原子多 rep（wl-clipboard-rs DataSource / X11 selection owner）—— 下一个 phase
  2. image / rtf / files 的多 rep 写入（跨三平台）
  3. 删除 `narrow_to_primary`，让 `apply_inbound` 直接 write 全 snapshot，让 macOS / Windows 的多 rep 能力在主流量中真正触发
</output>
