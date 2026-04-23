use super::super::common::CommonClipboardImpl;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use clipboard_rs::ClipboardContext;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{
    NSPasteboard, NSPasteboardItem, NSPasteboardTypeHTML, NSPasteboardTypeString,
    NSPasteboardWriting,
};
use objc2_foundation::{NSArray, NSData};
use std::sync::{Arc, Mutex};
use tracing::{debug, debug_span, info, warn};
use uc_core::clipboard::{ObservedClipboardRepresentation, SystemClipboardSnapshot};
use uc_core::ports::SystemClipboardPort;

/// macOS clipboard implementation using clipboard-rs
pub struct MacOSClipboard {
    inner: Arc<Mutex<ClipboardContext>>,
}

impl MacOSClipboard {
    pub fn new() -> Result<Self> {
        let context = ClipboardContext::new()
            .map_err(|e| anyhow::anyhow!("Failed to create clipboard context: {}", e))?;
        Ok(Self {
            inner: Arc::new(Mutex::new(context)),
        })
    }
}

#[async_trait]
impl SystemClipboardPort for MacOSClipboard {
    fn read_snapshot(&self) -> Result<SystemClipboardSnapshot> {
        let span = debug_span!("platform.macos.read_clipboard");
        span.in_scope(|| {
            let mut ctx = self.inner.lock().unwrap();
            let snapshot = CommonClipboardImpl::read_snapshot(&mut ctx)?;

            debug!(
                formats = snapshot.representations.len(),
                total_size_bytes = snapshot.total_size_bytes(),
                "Captured system clipboard snapshot"
            );

            Ok(snapshot)
        })
    }

    fn write_snapshot(&self, snapshot: SystemClipboardSnapshot) -> Result<()> {
        let span = debug_span!(
            "platform.macos.write_clipboard",
            representations = snapshot.representations.len(),
        );
        span.in_scope(|| {
            let mut ctx = self.inner.lock().unwrap();
            CommonClipboardImpl::write_snapshot(&mut ctx, snapshot)?;

            debug!("Wrote clipboard snapshot to system");
            Ok(())
        })
    }
}

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

/// 把 `&[u8]` 包装成 `NSData`。
///
/// 实测 API（objc2-foundation 0.3.2 `src/data.rs`）：
/// - `NSData::with_bytes(bytes: &[u8]) -> Retained<NSData>`
///   内部调用 `initWithBytes:length:`，安全封装，无需 unsafe 调用方。
///
/// 其他候选项（均已在源码中确认存在但不用于此处）：
/// - `NSData::from_vec(Vec<u8>)` —— 需要 `block2` feature，不引入该依赖
/// - `NSData::dataWithBytes_length(ptr, len)` —— unsafe，需处理裸指针
fn make_nsdata(bytes: &[u8]) -> Retained<NSData> {
    NSData::with_bytes(bytes)
}

/// macOS 原子多 representation 写入。
///
/// ## 实测 API 版本（objc2-app-kit 0.3.2 / objc2-foundation 0.3.2 / objc2 0.6.3）
///
/// 核实的关键函数签名：
/// - `NSPasteboard::generalPasteboard() -> Retained<NSPasteboard>`（关联函数，`pub fn`，无 unsafe，无 MainThreadMarker 要求）
/// - `NSPasteboard::clearContents(&self) -> NSInteger`（`pub fn`，返回 changeCount）
/// - `NSPasteboard::writeObjects(&self, objects: &NSArray<ProtocolObject<dyn NSPasteboardWriting>>) -> bool`
/// - `NSPasteboardItem::new() -> Retained<Self>`
/// - `NSPasteboardItem::setData_forType(&self, data: &NSData, type: &NSPasteboardType) -> bool`
/// - `NSPasteboardTypeString` / `NSPasteboardTypeHTML`：`extern "C" { pub static ...: &'static NSPasteboardType; }`
/// - `NSData::with_bytes(bytes: &[u8]) -> Retained<NSData>`（objc2-foundation `src/data.rs` 安全封装）
/// - `NSArray::from_retained_slice(&[Retained<T>]) -> Retained<NSArray<T>>`（objc2-foundation `src/array.rs`）
/// - `ProtocolObject::from_retained(obj: Retained<T>) -> Retained<ProtocolObject<P>>`（objc2 `src/runtime/protocol_object.rs`，`pub fn`，无 unsafe）
///
/// 线程安全：`NSPasteboard` 在 objc2-app-kit 0.3.2 中**不**声明 MainThreadOnly
/// （源码中的 `extern_class!` 宏内没有 MainThreadMarker 约束）；所有写入方法均为
/// `pub fn`（非 `pub unsafe fn`），因此可在后台线程调用，与 tokio 异步环境兼容。
/// 若未来 objc2-app-kit 升级版本引入 MainThreadOnly 约束，需改为通过 dispatch_main
/// 或 `MainThreadMarker::new_unchecked()` 处理。
///
/// ## 为何用 NSPasteboardItem + writeObjects:
///
/// macOS 真正的"原子多 rep 写入"API 是 `NSPasteboard::writeObjects:` —— 把一个
/// 承载了多个 type/data 对的 `NSPasteboardItem` 一次性提交给 pasteboard。
/// 这保证目的地应用（无论是终端、TextEdit 纯文本模式、还是富文本 Pages）在同一
/// changeCount 下看到的是**同一组 representation**，而不是被分步写入时的中间状态。
///
/// 与 Windows 路径的对比：
/// - Windows 需要"提前 drop clipboard-rs ctx + dummy_ctx 绕路"，因为 Win32 OpenClipboard
///   是独占句柄模型，不能同时被两个调用者持有。
/// - macOS `NSPasteboard::generalPasteboard()` 返回系统单例，与 `clipboard-rs` 底层
///   同样通过该单例操作，**不存在句柄争抢**，因此本函数可直接 grab generalPasteboard
///   而无需先 drop clipboard-rs ctx。
///
/// ## MVP 范围
///
/// 本次仅支持 `text/plain`（→ NSPasteboardTypeString）+ `text/html`（→ NSPasteboardTypeHTML）。
/// 其他 mime（image / rtf / files）在多 rep 路径里跳过并 debug 日志，
/// 留待后续 phase 补齐 `NSPasteboardTypePNG / NSPasteboardTypeRTF` 等。
///
/// ## clearContents() 副作用防御
///
/// 函数开头扫描 snapshot 是否包含至少一条可写 rep（text/plain 或 text/html）。
/// 若扫描结果为空，直接 bail——**不调用 clearContents()**，避免把用户原本的 clipboard
/// 内容抹掉却什么都写不进去（与 Windows 任务的 `empty()` 副作用防御同构）。
pub(crate) fn write_snapshot_multi_macos(snapshot: SystemClipboardSnapshot) -> Result<()> {
    // 预扫描：snapshot 中至少要有一条可写 rep（text/plain 或 text/html）。
    // 否则直接 bail，不打开 / 不 clear pasteboard。
    let has_writable = snapshot.representations.iter().any(|rep| {
        matches!(
            resolve_multi_rep_mime(rep),
            Some("text/plain") | Some("text/html")
        )
    });

    if !has_writable {
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

    // 1. 拿 general pasteboard 单例（macOS NSPasteboard 无独占句柄模型，可直接获取）
    // objc2-app-kit 0.3.2 中 generalPasteboard() 是 `pub fn`（非 unsafe），直接调用。
    let pasteboard: Retained<NSPasteboard> = NSPasteboard::generalPasteboard();

    // 2. 清空旧内容；忽略返回的 changeCount（仅标识版本号，不代表错误）
    let _ = pasteboard.clearContents();

    // 3. 构造 item，依次 setData
    let item: Retained<NSPasteboardItem> = NSPasteboardItem::new();

    let mut wrote_any = false;
    let mut skipped: Vec<String> = Vec::new();

    for rep in &snapshot.representations {
        match resolve_multi_rep_mime(rep) {
            Some("text/plain") => {
                // text/plain 的字节是 UTF-8，NSPasteboardTypeString 期望 UTF-8 字节，
                // 直接写原始字节，不经 NSString 转换（避免对非法 UTF-8 误报）。
                let data = make_nsdata(&rep.bytes);
                // NSPasteboardTypeString 是 extern "C" 静态变量，访问需要 unsafe 块。
                // setData_forType 本身是 `pub fn`（安全方法）。
                let ok = unsafe { item.setData_forType(&data, NSPasteboardTypeString) };
                if ok {
                    debug!(bytes = rep.bytes.len(), "写入 NSPasteboardTypeString 成功");
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
                // NSPasteboardTypeHTML 是 extern "C" 静态变量，访问需要 unsafe 块。
                let ok = unsafe { item.setData_forType(&data, NSPasteboardTypeHTML) };
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
                    "macOS 多 rep 写入：跳过不支持的 rep（后续 phase 补齐 image/rtf/files）"
                );
                skipped.push(rep.format_id.as_str().to_string());
            }
        }
    }

    if !wrote_any {
        // 所有候选 rep 的 setData_forType 均返回 false —— 极罕见。
        // 此时 item 为空，writeObjects 不会提交有意义内容，但 clearContents 已执行。
        anyhow::bail!(
            "macOS 多 rep 写入：所有候选 rep setData_forType 均失败；\
             pasteboard 已被清空但无法写入；跳过的 rep = {:?}",
            skipped
        );
    }

    // 4. 把 NSPasteboardItem 包装成 NSArray<ProtocolObject<dyn NSPasteboardWriting>>
    //
    // 实测 API（objc2 0.6.3）：
    // - `ProtocolObject::from_retained(obj: Retained<T>) -> Retained<ProtocolObject<P>>`
    //   （`pub fn`，内部为 `Retained::cast_unchecked`，编译期检查 T: ImplementedBy<P>）
    // - `NSArray::from_retained_slice(&[Retained<T>]) -> Retained<NSArray<T>>`
    //   （objc2-foundation `src/array.rs`）
    //   注意：此处需要传入 `Vec` 再取切片，因为 `from_retained_slice` 消费 Retained
    let proto_item: Retained<ProtocolObject<dyn NSPasteboardWriting>> =
        ProtocolObject::from_retained(item);
    let items_vec = vec![proto_item];
    let items_array: Retained<NSArray<ProtocolObject<dyn NSPasteboardWriting>>> =
        NSArray::from_retained_slice(&items_vec);

    // 5. 原子提交；writeObjects: 返回 false 表示失败，上抛 Err
    // objc2-app-kit 0.3.2 中 writeObjects 是 `pub fn`（非 unsafe）。
    let ok = pasteboard.writeObjects(&items_array);
    if !ok {
        return Err(anyhow!(
            "NSPasteboard.writeObjects 返回 false；pasteboard 可能处于不一致状态\
             （已 clearContents 但本次写入失败）"
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
        skipped_count = skipped.len(),
        "macOS 原子多 rep 写入完成"
    );

    Ok(())
}
