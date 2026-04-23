---
phase: 260423-9do-windows-rep
plan: 01
subsystem: uc-platform / clipboard-write
tags: [windows, clipboard, multi-rep, atomic-write, CF_UNICODETEXT, CF_HTML]
dependency_graph:
  requires: []
  provides: [write_snapshot_multi_windows, write_snapshot_multi]
  affects: [common.rs, windows.rs]
tech_stack:
  added: []
  patterns:
    - "clipboard-win RAII Clipboard + raw::empty() + NoClear 累加多 format"
    - "cfg(target_os) 单点收敛在 common.rs write_snapshot_multi 方法"
key_files:
  modified:
    - src-tauri/crates/uc-platform/src/clipboard/common.rs
    - src-tauri/crates/uc-platform/src/clipboard/platform/windows.rs
decisions:
  - "set_string_with::<NoClear> 而非 set_string：实测 set_string 默认 DoClear 会清空前面写好的格式"
  - "dummy_ctx 模式：Windows 多 rep 路径提前 drop clipboard-rs ctx，用 dummy_ctx 满足签名，规避 OSError 1418"
  - "全 skip 时 bail! 而非静默返回 Ok：§6.1 平台层不替业务决定 fallback 策略"
metrics:
  duration: ~30min
  completed: 2026-04-23
  tasks_completed: 2
  files_modified: 2
---

# 260423-9do Windows 多表示原子写入 + 解除平台层单 rep 契约 — 执行总结

**一句话**：放宽 `write_snapshot` 的"仅支持 1-rep"限制，并为 Windows 平台层新增在单次 `OpenClipboard` 会话内原子写入 CF_UNICODETEXT + CF_HTML 的能力，修复 Chrome 复制内容同步到 Windows 后粘贴纯文本失败的 bug 根因。

---

## 任务执行摘要

### 任务 1：放宽 common.rs 的 1-rep 契约（commit `6f6c8d79`）

**实际改动（`common.rs`）**：

1. 删除 `ensure!(snapshot.representations.len() == 1, "platform::write expects exactly ONE representation")`。
2. 替换为：空 snapshot 用 `bail!`，`len > 1` 时调用新增的 `write_snapshot_multi` 方法。
3. 新增 `write_snapshot_multi`：
   - `#[cfg(target_os = "windows")]` 分支：委托给 `crate::clipboard::platform::windows::write_snapshot_multi_windows`。
   - `#[cfg(not(target_os = "windows"))]` 分支：`warn!` 显式降级日志 + 取第一个 rep 递归调 `write_snapshot`（满足 §9.3 不允许静默降级）。
4. 将顶部陈旧的 TODO doc comment 替换为三段式策略说明（保留 issue #92 链接）。
5. 移除不再使用的 `ensure` import（修复 warning）。

**Diff 约束验证**：仅修改 `common.rs`，其余文件零改动。

---

### 任务 2：Windows 原子多 rep 写入（commit `2dde3312`）

**实际改动（`windows.rs`）**：

1. 新增顶层函数 `pub(crate) fn write_snapshot_multi_windows(snapshot: SystemClipboardSnapshot) -> Result<()>`。

   核心流程：
   - `ClipboardWin::new_attempts(10)` RAII 打开剪贴板（drop 时自动 CloseClipboard）。
   - `cb_raw::empty()` 显式清空一次（避免"幽灵格式"与旧内容混入）。
   - 遍历 reps，按 MIME 分发：
     - `text/plain` → `cb_raw::set_string_with::<NoClear>(&text, NoClear)`（必须用 NoClear 版本，`set_string` 默认 DoClear 会抹掉前面写好的格式）。
     - `text/html` → `cb_raw::set_html(html_fmt, &html)`（默认 NoClear，内部构造 CF_HTML 标准头）。
     - 其他 mime → debug 日志跳过（image/rtf/files 留待后续 phase）。
   - 全 skip 时 `bail!`（§6.1）；部分 skip 时 debug 日志列清单；成功时 `info!` 记录总数。

2. 修改 `WindowsClipboard::write_snapshot`：在 `snapshot.representations.len() > 1` 分支，提前放弃 mutex 锁定（不走 `self.inner.lock()`），改用临时 `dummy_ctx` 进入 `CommonClipboardImpl::write_snapshot`，避免 clipboard-rs 持有 Windows 剪贴板句柄时与 `clipboard-win` 的 `OpenClipboard` 抢句柄（OSError 1418）。

**API 调整说明（与 PLAN.md 描述的偏差）**：

PLAN.md 中写"`set_string` 内部用 NoClear"——实测源码（`raw.rs:588`）确认 `set_string` 默认调用 `DoClear::EMPTY_FN`（即 `EmptyClipboard`），**并非** NoClear。因此实现中改用 `set_string_with::<NoClear>(&text, NoClear)`。此为 Rule 1 自动修正（若按 PLAN.md 错误注释实现，会产生 CF_UNICODETEXT 写完后清空已有格式的 bug）。

---

## 编译验证结果

| 目标 | 结果 | 备注 |
|------|------|------|
| `x86_64-apple-darwin`（macOS 开发机）| 通过，零 warning | 任务 1 commit 后验证；任务 2 commit 后再次验证均通过 |
| `x86_64-pc-windows-msvc` | 未安装，无法验证 | **Windows 侧待实机验证** |
| `x86_64-pc-windows-gnu` | 安装但 libsodium 交叉编译环境缺失，编译失败 | 属于环境问题，非代码问题 |

`windows.rs` 整体位于 `platform/mod.rs` 的 `#[cfg(target_os = "windows")]` 门控下，macOS 开发机不编译该文件，Windows 目标代码的正确性需在实机或 CI 上验证。

---

## 已知后续工作

1. **macOS `NSPasteboardItem` 原子多 rep**：`write_snapshot_multi` 的 macOS 路径目前只写第一个 rep 并 warn；补齐需要用 `objc2-app-kit` 的 `NSPasteboardItem` API 一次性提交多个 UTI 数据。
2. **Linux Wayland data source 多 rep**：需要基于 `wl-clipboard-rs` 的 DataSource 接口注册多个 mime type，与当前 `clipboard-rs` 高层 API 不兼容。
3. **image / rtf / files 的多 rep 写入（Windows）**：`write_snapshot_multi_windows` 中跳过非 text/html 的 rep；后续需要验证 CF_DIB + CF_UNICODETEXT 混写的互操作性，以及 CF_RTF format code 注册（`RegisterClipboardFormat("Rich Text Format")`）。
4. **删除 `narrow_to_primary`，让 `apply_inbound` 直接 write 全 snapshot**：当前 `apply_inbound.rs` 仍先 `narrow_to_primary` 再 write，Windows 多 rep 能力暂时不会在主流量中触发。删除 `narrow_to_primary` 后，浏览器复制的 8-rep snapshot 将直接进入 `write_snapshot_multi_windows`，CF_UNICODETEXT + CF_HTML 才会真正同时写入。

---

## 偏差记录

### Rule 1 自动修正：set_string API 行为与 PLAN.md 注释不符

- **发现于**：任务 2 实现前 API 核实
- **问题**：PLAN.md `<interfaces>` 中注释"set_string 内部 set_string_inner 用 NoClear::EMPTY_FN，所以不会清空剪贴板"。实测 `clipboard-win-5.4.1/src/raw.rs:588` 中 `set_string` 调用 `set_string_inner(data, options::DoClear::EMPTY_FN)`，**会**清空剪贴板。
- **修正**：改用 `cb_raw::set_string_with::<NoClear>(&text, NoClear)`，该变体（`raw.rs:598`）传入 `C::EMPTY_FN`（NoClear 时为 noop），确保不清空。
- **影响文件**：`windows.rs`
- **提交**：`2dde3312`

---

## Orchestrator 备注（executor 不感知）

- Executor 在隔离 worktree 中产生的原始两个 commit（`a5d11cf2` / `9f17331b`）因 worktree EnterWorktree 初始 base 错误而同时带入了 700+ 个无关文件的 drift，无法直接 merge 回主 branch。
- Orchestrator 改用 `git checkout <worktree-branch> -- <file>` 按文件提取 `common.rs` 与 `windows.rs` 到当前 HEAD，重新打包为两个干净 commit `6f6c8d79` / `2dde3312`，commit message 保留原意图。
- 原始 worktree branch 已清理。本 SUMMARY 中引用的 hash 已统一为合并后的实际 hash。
