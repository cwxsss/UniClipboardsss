---
created: 2026-04-23T20:00:00.000Z
title: 复制图片跨设备同步时 narrow_to_primary 选中发送端本地文件路径导致对端粘贴失效
area: clipboard-sync
files:
  - src-tauri/crates/uc-application/src/clipboard_write/primary_rep_selector.rs
  - src-tauri/crates/uc-core/src/clipboard/policy/v1.rs
  - src-tauri/crates/uc-application/src/clipboard_capture/usecase.rs
  - src-tauri/crates/uc-app/src/usecases/file_sync/sync_outbound.rs
---

## Problem

在同 LAN 下跨设备复制小图片（PixPin 截图）验证，**出站 dispatch 成功 (`accepted=1`)**，
但对端粘贴"什么都没有"或拿到一个文件占位。

真机日志（`~/Library/Application Support/app.uniclipboard.desktop-abc/logs/uniclipboard.json.2026-04-23`，行 1744-1784）显示 PixPin 往系统剪贴板塞了 7 种 representation：

```
format_ids = [
  "files",                          ← size 91   (临时文件路径)
  "image",                          ← size 2769 (image/png, TIFF→PNG 转换后)
  "CorePasteboardFlavorType 0x6675726C",
  "dyn.ah62d4rv4gu8y6y4grf0gn5xbrzw1gydcr7u1e3cytf2gn",
  "dyn.ah62d4rv4gu8yc6durvwwaznwmuuha2pxsvw0e55bsmwca7d3sbwu",
  "Apple URL pasteboard type",
  "com.trolltech.anymime.PixPinData",
]
```

`SelectRepresentationPolicyV1` (`uc-core/src/clipboard/policy/v1.rs`) 的 DefaultPaste
打分：`FileList=100 > RichText=90 > PlainText=80 > Image=70 > Uri=60 > Unknown=10`。
`"files"` rep 通过 `format_id.eq_ignore_ascii_case("files")` 兜底路径被归类为 FileList，
得分 100，**胜过真正的 image rep**。

对端 `ApplyInboundClipboardUseCase` 跑 `narrow_to_primary`，拿 `paste_rep_id` = files rep，
`write_snapshot` 收到的单 rep 只有 91 字节 —— **发送端本地的临时路径**：

```
/Users/mark/Library/Application Support/PixPin/Temp/PixPin_2026-04-23_19-57-48.png
```

对端把这串路径写进系统剪贴板，但**对端文件系统里根本没有那个文件**。Paste 时：
- Finder / 资源管理器：看到"文件"图标但打不开
- 图像类应用：一般表现为"没复制任何东西"

### 次要问题（非同根因，但同时存在）

Legacy libp2p `sync_outbound` 试图把 PNG 文件真的传过去（`uc_app::usecases::file_sync::sync_outbound` [1768-1771]），
失败原因：`"invalid peer id: base-58 decode error"` —— Slice 1 的 peer UUID 喂给 libp2p
的 base-58 PeerId 解码器必然挂。这条路径计划在 Slice 5 随 libp2p 一起清理；如果它能
通，本来可以帮跨设备 file path 场景补一个真实文件传输 fallback。

## Solution

三种可能方向，短期到长期：

1. **narrow_to_primary 层降级 FileList（最小改动）**
   改 `primary_rep_selector`（或策略层暴露第二入口 `SelectionTarget::CrossDeviceWrite`），
   跨设备写系统剪贴板时，**只要 envelope 里同时存在 `image/*` 或其他自包含 rep，
   就不选 FileList**。因为 FileList 的字节是指向 sender 本地文件系统的路径，本身
   就是 sender-local 的"外部引用"，跨设备无意义。
   - 代价：针对 paste "纯文件列表" 的场景要区分（用户真的想复制文件过去）。
     判据：rep 存在的前提下是否还有 self-contained 其他 rep。

2. **capture 时剥离 transient 本地路径（更彻底）**
   在 `CaptureClipboardUseCase` 识别"明显 sender-local 的 transient 路径"
   （OS 临时目录 / `PixPin/Temp` / 其他 app-local 缓存目录）并从 snapshot 里丢掉，
   不进 entry 也不进 envelope。
   - 代价：判定规则边界不清，可能误杀用户"正常 temp 目录下的文件"。

3. **真的把文件 bytes 传过去（Slice 5 方向）**
   完成 libp2p → iroh 的 blob/file transfer 迁移，file path rep 继续选（语义正确），
   bytes 在带外真实传过去，对端按 path 写临时目录 + 重写 path rep 让对端 paste 时
   指向对端本地文件。
   - 代价：工作量最大，但语义最正确。

## Decision log

- 2026-04-23：真机同 LAN 验证时暴露。先记录为已知问题，**不阻塞**跨 LAN 场景测试。
  等跨 LAN 验证 + Slice 3/5 的 blob/file 路径规划落定后，再决定走哪条修法。
