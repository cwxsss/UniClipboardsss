# 260423-b8f Summary — Windows 多 rep 写入副作用止血

**状态**: ✅ 完成（代码 commit；Windows 实机行为验证待部署）
**日期**: 2026-04-23
**依赖**: quick `260423-a3b-windows-rep-apply-inbound-narrow`
**Commits**:
- `8e4d828f` — `fix(uc-platform/windows): guard multi-rep writer against silent clipboard wipe`

---

## 背景

上一个 quick `260423-a3b` 移除了 `apply_inbound.execute` 的 `narrow_to_primary` 调用，full snapshot 直送 platform 层。实机跑出来后暴露了一个长期潜伏的副作用 bug：

PixPin 截图 → macOS 剪贴板放进 7 rep（files + image + 5 个平台私有类型，**无 text/plain、无 text/html**）→ snapshot 跨设备送到 Windows → `write_snapshot_multi_windows` 执行：

1. 打开 Windows 剪贴板
2. **`EmptyClipboard` 清空用户当前剪贴板** ← 副作用
3. 遍历 7 rep，全部不认，全跳过
4. `wrote_any = false` → `bail!`

上层 `inbound_clipboard_sync.rs:106` 只 `warn!` 了一下，**没有任何恢复动作**。用户看到的：原本剪贴板里的内容被静默清空了，Ctrl+V 粘出空白。

---

## 改动

单文件：`src-tauri/crates/uc-platform/src/clipboard/platform/windows.rs`

### 1. 抽 helper `resolve_multi_rep_mime`

把原来内联在循环里的 "mime 优先 / format_id fallback" 推断逻辑抽成文件内私有 fn：

```rust
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
```

前置扫描与主循环复用同一个 helper，保证 "能否写" 判定与实际分派逻辑不会漂移。

### 2. 前置扫描 + 早期 bail

在 `ClipboardWin::new_attempts(10)` **之前**：

```rust
let has_writable = snapshot
    .representations
    .iter()
    .any(|rep| matches!(resolve_multi_rep_mime(rep), Some("text/plain") | Some("text/html")));

if !has_writable {
    let skipped: Vec<String> = snapshot.representations.iter()
        .map(|r| r.format_id.as_str().to_string())
        .collect();
    anyhow::bail!(
        "Windows 多 rep 写入：无可写 rep（支持 text/plain, text/html）；\
         未清空 OS 剪贴板；跳过的 rep = {:?}",
        skipped
    );
}
```

错误文案新增 "未清空 OS 剪贴板" 字样，和旧错误区别开，方便 Seq 排障。

### 3. 主循环改为调 helper

```rust
for rep in &snapshot.representations {
    let effective_mime = resolve_multi_rep_mime(rep);
    match effective_mime { ... }
}
```

循环结构和 match 分支不动，只去掉内联 mime 推断。

### 4. doc comment 追加 "empty() 副作用的防御" 一节

把"为什么要前置扫描、为什么错误文案包含'未清空 OS 剪贴板'"写进函数 doc comment，未来改代码的人一看就懂。

### 5. 末尾防御 bail 保留

函数末尾的 `if !wrote_any { bail!(...) }` 保留作 defensive path（理论上不会再触发，但 `String::from_utf8` 失败等 corner case 仍可能走到那里）。

---

## 编译结果

```
$ cd src-tauri && cargo check -p uc-platform
...
warning: `uc-platform` (lib) generated 4 warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 4.97s
```

零 error、零新 warning。4 个 warning 全部是 `address_registry.rs` / `local_discovery` 等旧代码（与本次改动无关）。

注意：本地开发机是 macOS，`cfg(target_os = "windows")` 块只做了语法级编译验证。Windows 实机运行验证需要重新部署 build。

---

## Windows 实机验证计划

1. 在 Windows 机器上 rebuild uc-daemon + 部署
2. **先在 Windows 本地复制一段纯文本**（例如从 Notepad 复制 `hello`）到系统剪贴板
3. 在 macOS 端用 PixPin 截图 → Cmd+C（产生 7 rep 纯图片 snapshot）
4. 观察 Windows 端：
   - Seq 上应出现 `inbound apply failed error=Windows 多 rep 写入：无可写 rep（...）；**未清空 OS 剪贴板**；跳过的 rep = [...]` warn
   - **关键验证**：此时在 Windows 上按 Ctrl+V，应粘出之前的 `hello`，**而不是空白** ✅
5. 非回归：再从 Chrome 复制一段带纯文本 + HTML 的内容 → 观察 `Wrote multi-representation clipboard atomically on Windows` INFO 日志、记事本 Ctrl+V 拿到纯文本（`260423-a3b` 已验证过的主流量）

---

## 与 `260423-a3b` 的关系

| quick | 作用 |
|---|---|
| `260423-9do` | 交付 Windows 原子多 rep 写入能力（`write_snapshot_multi_windows`） |
| `260423-a3b` | 把上述能力接到主流量（`apply_inbound` 不再 narrow） |
| **`260423-b8f`** | **止血 `a3b` 暴露的副作用**（`empty()` 在判能写前被调用） |

这三个是一串串起来的修复：
- `9do` 加了能力
- `a3b` 用上了能力，同时暴露了副作用
- `b8f` 修副作用

`b8f` **不**解决"Windows 端图片跨设备同步失效"这个更大的问题——那需要：
1. Windows multi-rep writer 扩展到 image / files（下个 phase）
2. 文件同步功能交付（独立 milestone）

两条独立的轨道，`b8f` 不掺和。

---

## 已知遗留

1. **Windows multi-rep writer 仍然只支持 text/plain + text/html**
   - 纯图片 / 纯文件的 snapshot 仍然会落到 `has_writable == false` 分支 → bail
   - 用户粘贴体验：原剪贴板保留（b8f 之前是清空），但**没拿到图片/文件**
   - 下一步：扩展 writer 支持 image（CF_DIB / CF_PNG），让纯图片 snapshot 真的能粘出图片
2. **`files` rep 跨设备无效**
   - 发送端本地路径在对端不存在
   - 根因：文件同步未实装
   - 下一步：独立 milestone 处理，不在 clipboard writer 层修补
3. **`inbound_clipboard_sync` 的 warn-only 策略**
   - 当前对 `ApplyInboundError::WriteCoordinator` 只 log warn，没有任何用户可见反馈
   - 未来可考虑把"无可写 rep"类错误降级为 debug 级别（既然 OS 剪贴板未受影响，也不算需要告警的错误）

---

## 验收清单对照

- [x] `cargo check -p uc-platform` 通过，零新 warning
- [x] `write_snapshot_multi_windows` 在打开 Windows 剪贴板前完成 "能否写" 判定；没可写 → 直接 bail，不触碰 OS 剪贴板
- [x] `resolve_multi_rep_mime` helper 抽出，扫描 + 主循环复用
- [x] 错误文案包含 "未清空 OS 剪贴板" 字样
- [x] 主函数 doc comment 追加 "empty() 副作用的防御" 一节
- [x] 仅修改 `windows.rs` 一个文件
- [x] Commit message 英文、中文注释，1 个原子 commit
- [ ] Windows 实机验证（待用户重新部署）
