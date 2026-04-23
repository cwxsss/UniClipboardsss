# 260423-a3b Summary — Windows 多 rep 主流量激活

**状态**: ✅ 完成
**日期**: 2026-04-23
**依赖**: quick `260423-9do`（Windows 原子多 rep 写入能力）
**Commits**:
- `0ea88a27` — `refactor(uc-platform): use SelectRepresentationPolicyV1 in non-Windows multi-rep fallback`
- `0516b074` — `refactor(uc-application): stop narrowing inbound snapshot; send full snapshot to platform`

---

## 目标回顾

把上一个 quick 交付的 Windows 原子多 rep 写入能力接到主流量上。Seq 14:11:37 日志定位了 bug：`apply_inbound.rs::execute` step 4 的 `narrow_to_primary` 把 8 rep 的 snapshot 砍到 1 rep 后才送给 `coordinator.write`，platform 层永远走单 rep 快路径，新交付的 `write_snapshot_multi_windows` 零次触发 —— 浏览器复制同步到 Windows 后，记事本粘贴只看到 HTML 而非 CF_UNICODETEXT。

---

## 实际 diff 概述

### 任务 1 — `src-tauri/crates/uc-platform/src/clipboard/common.rs`

`write_snapshot_multi` 的 `#[cfg(not(target_os = "windows"))]` 降级分支：

- 原逻辑：`warn!` + 取 `representations.into_iter().next()` + 递归 `write_snapshot`
- 新逻辑：`SelectRepresentationPolicyV1::default().select(&snapshot)` 得到 `paste_rep_id` → `position()` 定位 rep → `reps.remove(chosen_idx)` 构造 1-rep snapshot → 递归 `write_snapshot`
- `warn!` 字段由 `formats = [...]` 改为 `paste_rep_id` + `chosen_format_id`，保持 §9.3 显式降级可观测性
- 模块级 doc comment 与 `write_snapshot` doc comment 同步更新：把 "只写第一个 rep" 改为 "用 `SelectRepresentationPolicyV1` 选 paste-priority rep 再走单 rep 快路径"
- `use` 语句收敛在 `#[cfg(not(target_os = "windows"))]` 块内，不污染全局 namespace

### 任务 2 — `src-tauri/crates/uc-application/src/usecases/clipboard_sync/apply_inbound.rs`

`ApplyInboundClipboardUseCase` 最终结构：

```rust
pub struct ApplyInboundClipboardUseCase {
    entry_repo: Arc<dyn ClipboardEntryRepositoryPort>,
    capture: Arc<dyn InboundCapture>,
    write: Arc<dyn InboundWrite>,
}

impl ApplyInboundClipboardUseCase {
    pub fn new(
        entry_repo: Arc<dyn ClipboardEntryRepositoryPort>,
        capture: Arc<dyn InboundCapture>,
        write: Arc<dyn InboundWrite>,
    ) -> Self { ... }
}
```

`execute` 的步骤 4 最终写法：

```rust
// 4. Write OS clipboard with RemotePush guard. Order matters —
// capture must complete first so the watcher's origin lookup
// sees the persisted row even if it fires immediately.
//
// 送入 full snapshot（不 narrow）：platform 层内部按能力差异消化多 rep。
self.write
    .write(snapshot_for_write)
    .await
    .map_err(|e| ApplyInboundError::WriteCoordinator(e.to_string()))?;
```

其他改动：
- 删除 `use uc_core::ports::SelectRepresentationPolicyPort`
- 从 `use crate::clipboard_write::{...}` 删除 `narrow_to_primary`（保留 `ClipboardWriteCoordinator, ClipboardWriteIntent`）
- 模块顶部 doc comment 的 Step 4 说明同步更新（说明 platform 层按 OS 能力分流）
- `#[cfg(test)] mod tests` 的 `build()` helper 降为 3 参数
- 删除 `use uc_core::clipboard::SelectRepresentationPolicyV1`

### 任务 2 — `src-tauri/crates/uc-daemon/src/entrypoint.rs`

构造点（line 175）最终写法：

```rust
let apply_inbound_uc = Arc::new(ApplyInboundClipboardUseCase::new(
    runtime.wiring_deps().clipboard.clipboard_entry_repo.clone(),
    apply_inbound_capture_uc as Arc<dyn ApplyInboundCapture>,
    Arc::clone(&clipboard_write_coordinator) as Arc<dyn ApplyInboundWrite>,
));
```

`runtime.wiring_deps().clipboard.representation_policy` 本身保留（仍被 `CaptureClipboardUseCase::new` 第 3 个参数使用，line 164），未动 wiring。

---

## 保留项（严格不做）

- `narrow_to_primary` / `PrimaryRepError` / `primary_rep_selector.rs` 本体保留
- `clipboard_write/mod.rs` 的 `pub use primary_rep_selector::{narrow_to_primary, PrimaryRepError}` 保留
- `ClipboardWriteCoordinator` / `origin_guard_key` / `meaningful_origin_key` / echo suppression 零改动
- `windows.rs` / `macos.rs` / `linux.rs` 平台层主路径零改动
- `Cargo.toml` 零改动

---

## `cargo check` 四项命令的实际结果

```
=== cargo check -p uc-platform ===
  Finished `dev` profile — 4 warnings（全部是 address_registry.rs 等旧代码，与本次改动无关）

=== cargo check -p uc-application ===
  Finished `dev` profile — 2 warnings（DispatchSyncError::LocalIdentity、InboundAction::DuplicateIgnored，旧代码）

=== cargo check --tests -p uc-application ===
  Finished `dev` profile — 2 warnings（lib 2 + test 2 duplicates）
  6 个 #[tokio::test] 编译通过

=== cargo check -p uc-daemon ===
  Finished `dev` profile — uc-app 的 4 个 deprecated warnings（已知遗留 legacy port，与本次改动无关）
```

所有编译零 error、零新 warning。`git diff --name-only 421dbb6f..HEAD -- ':!.planning'` 输出严格为：

```
src-tauri/crates/uc-application/src/usecases/clipboard_sync/apply_inbound.rs
src-tauri/crates/uc-daemon/src/entrypoint.rs
src-tauri/crates/uc-platform/src/clipboard/common.rs
```

---

## Windows 实机验证计划

本次改动在 macOS 开发机完成编译校验。实机行为验证需要用户重新部署 Windows build 后手动走以下流程：

1. 在 Windows 机器上 rebuild daemon + app，部署最新 `slender-soybean` 分支
2. 启动 daemon，确认与 macOS 侧通过同一 space 对等连通
3. 从 Chrome 复制一段带纯文本 + HTML 的内容（例如选中一段带超链接的段落 → Cmd+C）
4. 观察 Seq 日志：
   - 应**首次出现** `Wrote multi-representation clipboard atomically on Windows` INFO 日志
   - debug 级别下应看到 `wrote CF_UNICODETEXT` / `wrote CF_HTML`
   - `apply_inbound.execute` 不再出现 "narrow" 相关日志
5. Windows 端粘贴验证：
   - **记事本 Ctrl+V** → 应出现**纯文本**内容 ✅（本次修复的核心验证点）
   - **写字板 / Word / Chrome 地址栏 Ctrl+V** → 应出现带格式 / 纯文本（各目的地拿到对应 format）

### macOS 回归确认

本地开发机已完成编译校验。运行期回归（同样的 Chrome 多 rep 复制 → 本地 daemon 接收 → 写入 OS 剪贴板）需要一次手动验证：

1. Chrome 复制富文本段落
2. Cmd+V 粘贴到 TextEdit（富文本模式）、备忘录、Safari 地址栏
3. 预期与改动前完全一致：TextEdit / 备忘录显示格式化内容，Safari 地址栏显示纯文本
4. Seq 搜索 `multi-representation write not yet supported on this platform` —— 应能看到新字段 `paste_rep_id` + `chosen_format_id`，不再有 `formats = [...]` 列表

---

## guard key 行为变化（重要！）

`uc-core/src/clipboard/system.rs::meaningful_origin_key` 的选 rep 顺序：**files > plain-text > rich-text > image**。

- **改前**：`coordinator.write` 收到 narrowed 1-rep snapshot（仅含 rich-text 那一份）→ `origin_guard_key = rich-text:<hash>`
- **改后**：`coordinator.write` 收到 full snapshot（含 plain text）→ `origin_guard_key = text:<hash>`

### 潜在影响

Windows 端 `write_snapshot_multi_windows` 写入 OS 剪贴板后，本地 clipboard watcher 会捕获到 OS 回读的 snapshot，watcher 再次计算 `origin_guard_key` —— 此时：

- 若 OS 回读能拿到 CF_UNICODETEXT（本次期望）→ watcher 计算出 `text:<hash>` → 与 inbound 记录的 `text:<hash>` 匹配 → echo suppression 成功，无重复 entry ✅
- 若 OS 回读仅拿到 CF_HTML（不确定，需实测）→ watcher 计算出 `rich-text:<hash>` → 与 inbound 记录的 `text:<hash>` 不匹配 → 本地 DB 会多出一条 LocalCapture entry，与原 RemotePush entry 重复

**本次 quick 不修这个副作用**。影响评估：

- 不会 panic
- 不会阻塞同步
- 最坏情况只是 Windows 本地数据库多出 1 条与 inbound 等价的 LocalCapture 记录（可能还会被 outbound dispatcher 再转发一次回到 macOS）

若实机确认有此问题，单独开一个 quick 处理：调整 watcher 的 guard key 比对逻辑（例如改为"`origin_guard_key` 的 hash 部分匹配即命中"而不再严格要求 format 前缀相同），不在本次范围。

---

## 已知后续工作

1. **Windows echo suppression 失效验证** — 实机粘贴后观察 Seq 上 `origin_guard_key` 一致性。若失效，开 quick 修 watcher 的 guard key 比对（详见上方 guard key 小节）
2. **macOS `NSPasteboardItem` 原子多 rep 写入** — 让 macOS 端也具备与 Windows 等价的多 rep 写入能力，取代当前的 policy 降级
3. **Linux Wayland data source 多 rep 写入** — 同上，Linux 端
4. **image / rtf / files 的 Windows 多 rep 支持** — `windows.rs::write_snapshot_multi_windows` 目前只处理 text/plain + text/html，其他 rep 按 debug 日志跳过
5. **清理 `narrow_to_primary`** — 所有平台都补齐原子多 rep 写入能力后，删除 `primary_rep_selector.rs` 与 `PrimaryRepError`，以及 `clipboard_write/mod.rs` 的 `pub use`

---

## 验收清单对照

- [x] `cargo check -p uc-platform && cargo check -p uc-application && cargo check --tests -p uc-application && cargo check -p uc-daemon` 全部通过
- [x] `common.rs::write_snapshot_multi` 非 Windows 分支使用 `SelectRepresentationPolicyV1::default().select(...).paste_rep_id`
- [x] `apply_inbound.rs::execute` 不再调用 `narrow_to_primary`，直接 `self.write.write(snapshot_for_write).await`
- [x] `ApplyInboundClipboardUseCase::new` 签名为 3 参数（无 `representation_policy`）
- [x] `uc-daemon/src/entrypoint.rs:175` 构造点同步改 3 参数
- [x] `#[cfg(test)] mod tests::build()` helper 同步改 3 参数，6 个 `#[tokio::test]` 编译通过
- [x] `primary_rep_selector.rs` / `clipboard_write/mod.rs` 的 `pub use narrow_to_primary` 零改动
- [x] 新增 / 修改注释使用中文；commit message 英文；两个任务两个原子 commit
- [x] `git diff --name-only 421dbb6f..HEAD -- ':!.planning'` 只含 3 个目标文件
- [x] SUMMARY.md 明确标注 "guard key 从 rich-text:* 变为 text:*，Windows 实机 watcher 回环 echo suppression 可能受影响"
