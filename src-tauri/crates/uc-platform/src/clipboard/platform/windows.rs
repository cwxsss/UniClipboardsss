use super::super::common::CommonClipboardImpl;
use anyhow::Result;
use async_trait::async_trait;
use clipboard_rs::{Clipboard, ClipboardContext};
use std::ops::Range;
use std::sync::{Arc, Mutex};
use tracing::{debug, debug_span, error, info, warn};
use uc_core::clipboard::{MimeType, ObservedClipboardRepresentation, SystemClipboardSnapshot};
use uc_core::ids::RepresentationId;
use uc_core::ports::SystemClipboardPort;

/// 推断 rep 在 Windows 多 rep 写入路径下的"有效 MIME"。
///
/// 与 `write_snapshot_multi_windows` 主循环使用的推断逻辑保持一致 ——
/// 既用于前置 "有无可写 rep" 扫描，也用于主循环分派，避免两处逻辑漂移。
/// 与 `common.rs` 单 rep 快路径的 format_id → mime 推断表对齐。
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

/// Windows 原子多 representation 写入。
///
/// 在**单次** `OpenClipboard` 会话内写入多个剪贴板格式（CF_UNICODETEXT + CF_HTML），
/// 确保纯文本目的地（记事本、命令提示符等）和富文本目的地（Word、写字板等）都能正确粘贴。
///
/// ## 设计要点
///
/// ### 为何要手动 `empty()` 一次
/// `Clipboard::new_attempts` 只负责 `OpenClipboard`，**不**清空剪贴板现有内容。
/// 如果不手动调用 `raw::empty()`，旧的 representations（如上一次复制的图片 CF_DIB）
/// 会与新内容混在一起，产生"幽灵格式"。因此在会话开头显式清空一次。
///
/// ### 为何使用 `*_without_clear` / `set_html` / `set_string_with::<NoClear>` 系列
/// `raw::set()` 和 `raw::set_string()` 在写入时会内部调用 `EmptyClipboard`，
/// 将前面已写入的格式全部抹掉——正是本次要修复的 bug 根因。
/// `raw::set_without_clear` / `raw::set_html`（默认 NoClear）/
/// `raw::set_string_with::<NoClear>` 不调 `EmptyClipboard`，可以在同一 RAII 会话内
/// 累加多个 format 到同一 clipboard item。
///
/// ### 本次只处理 text + html
/// image (CF_DIB) / RTF (CF_RTF) / files (CF_HDROP) 的多 rep 互操作性需要更多验证
/// （CF_DIB 与 CF_UNICODETEXT 混写、CF_RTF format code 注册等），留待后续 phase 补齐。
/// 遇到非 text/html 的 rep 时记 debug 日志并跳过。
///
/// ### 调用方注意
/// 此函数由 `common.rs::write_snapshot_multi` 在 `#[cfg(target_os = "windows")]`
/// 分支调用。`WindowsClipboard::write_snapshot` 在多 rep 场景下会提前 drop
/// clipboard-rs ctx，避免与本函数的 `clipboard-win` OpenClipboard 抢句柄（OSError 1418）。
///
/// ### `empty()` 副作用的防御
/// `EmptyClipboard` 会立即清空用户当前的 OS 剪贴板。如果我们开了会话、清空了、
/// 然后发现全部 rep 都不认（例如 PixPin 截图：files + image + 5 个平台私有类型，
/// 没有 text/plain 也没有 text/html），用户看到的就是"粘贴为空"——原本剪贴板里的
/// 有效内容被静默抹掉了。为避免这个副作用，本函数**先**扫描 snapshot 判断至少
/// 有一条可写的 rep，再打开剪贴板并清空——没可写时直接 bail，OS 剪贴板保持原样。
/// bail 错误文案包含 "未清空 OS 剪贴板" 字样以便排障。
pub(crate) fn write_snapshot_multi_windows(snapshot: SystemClipboardSnapshot) -> Result<()> {
    use clipboard_win::formats::Html as HtmlFmt;
    use clipboard_win::options::NoClear;
    use clipboard_win::raw as cb_raw;
    use clipboard_win::Clipboard as ClipboardWin;

    // 前置扫描：如果没有任何 rep 是我们能写的（text/plain 或 text/html），
    // 直接 bail；**不**打开 Windows 剪贴板、**不**调 empty()。
    // 避免把用户原本的 OS 剪贴板清掉却什么都写不进去（见上方 doc comment "empty() 副作用的防御"）。
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
            "Windows 多 rep 写入：无可写 rep（支持 text/plain, text/html）；\
             未清空 OS 剪贴板；跳过的 rep = {:?}",
            skipped
        );
    }

    // RAII：作用域内整段占有 Windows 剪贴板，drop 时自动 CloseClipboard。
    let _clip = ClipboardWin::new_attempts(10)
        .map_err(|e| anyhow::anyhow!("OpenClipboard failed after retries: {}", e))?;

    // 在会话开头显式清空一次——后续每次写入都走 *_without_clear 系列，
    // 这样多次调用会把多个 format 累加到同一个 clipboard item，
    // 而不会把前面写好的 CF_UNICODETEXT 再次清掉（这是 raw::set / set_string 的默认行为）。
    cb_raw::empty().map_err(|e| anyhow::anyhow!("EmptyClipboard failed: {}", e))?;

    // 提前注册 "HTML Format"——此为 Windows 约定的自定义 format 名称，
    // RegisterClipboardFormat 对相同名称是幂等的，返回值在整个进程生命周期内固定。
    let html_fmt_opt: Option<u32> = HtmlFmt::new().map(|h| h.code());

    let mut wrote_any = false;
    let mut skipped: Vec<String> = Vec::new();

    for rep in &snapshot.representations {
        // 优先用显式 MIME，其次按 format_id 推断（与 common.rs 单 rep 路径保持一致）。
        // 与前置扫描使用同一个 helper，保证 "能否写" 的判定与主循环分派逻辑不漂移。
        let effective_mime = resolve_multi_rep_mime(rep);

        match effective_mime {
            Some("text/plain") => {
                // 写入 CF_UNICODETEXT。
                // 必须使用 set_string_with::<NoClear> 而非 set_string：
                // set_string 内部调用 DoClear（EmptyClipboard），会把已写的其他 format 抹掉。
                let text = String::from_utf8(rep.bytes.clone())
                    .map_err(|e| anyhow::anyhow!("text/plain rep is not valid UTF-8: {}", e))?;
                cb_raw::set_string_with::<NoClear>(&text, NoClear)
                    .map_err(|e| anyhow::anyhow!("set CF_UNICODETEXT failed: {}", e))?;
                debug!(bytes = rep.bytes.len(), "写入 CF_UNICODETEXT 成功");
                wrote_any = true;
            }
            Some("text/html") => {
                let Some(html_fmt) = html_fmt_opt else {
                    warn!("注册 HTML Format 失败，跳过 text/html rep");
                    skipped.push(rep.format_id.as_str().to_string());
                    continue;
                };
                let html = String::from_utf8(rep.bytes.clone())
                    .map_err(|e| anyhow::anyhow!("text/html rep is not valid UTF-8: {}", e))?;
                // set_html 默认走 NoClear 分支，内部构造 "Version:0.9 / StartHTML / EndHTML /
                // StartFragment / EndFragment" 头并包裹 BODY_HEADER/BODY_FOOTER，适合累加。
                cb_raw::set_html(html_fmt, &html)
                    .map_err(|e| anyhow::anyhow!("set CF_HTML failed: {}", e))?;
                debug!(bytes = rep.bytes.len(), "写入 CF_HTML 成功");
                wrote_any = true;
            }
            // 本次只支持 text/plain + text/html 的原子多 rep 写入。
            // image / rtf / files 的多 rep 写入留到后续 phase（需要 CF_DIB + CF_RTF
            // 的互操作性验证以及 RTF format code 注册），本次非目标——跳过并记录。
            other => {
                debug!(
                    format_id = %rep.format_id,
                    mime = ?other,
                    "Windows 多 rep 写入：跳过非 text/html 的 rep"
                );
                skipped.push(rep.format_id.as_str().to_string());
            }
        }
    }

    if !wrote_any {
        // 所有 rep 都被跳过——上游发来了一个全是 image/rtf 的多 rep snapshot。
        // 本函数不负责 fallback（§6.1 平台层不替业务决定），直接报错让调用方处理。
        anyhow::bail!(
            "Windows 多 rep 写入：无可写 rep（支持 text/plain, text/html）；\
             跳过的 rep = {:?}",
            skipped
        );
    }

    if !skipped.is_empty() {
        debug!(
            skipped_count = skipped.len(),
            skipped = ?skipped,
            "Windows 多 rep 写入：部分 rep 已跳过（非 text/html）"
        );
    }

    info!(
        total_reps = snapshot.representations.len(),
        skipped = skipped.len(),
        "Windows 原子多 rep 写入完成"
    );

    Ok(())
}

/// Windows clipboard implementation using clipboard-rs and clipboard-win
pub struct WindowsClipboard {
    inner: Arc<Mutex<ClipboardContext>>,
}

impl WindowsClipboard {
    pub fn new() -> Result<Self> {
        let context = ClipboardContext::new()
            .map_err(|e| anyhow::anyhow!("Failed to create clipboard context: {}", e))?;
        Ok(Self {
            inner: Arc::new(Mutex::new(context)),
        })
    }
}

#[async_trait]
impl SystemClipboardPort for WindowsClipboard {
    fn read_snapshot(&self) -> Result<SystemClipboardSnapshot> {
        let span = debug_span!("platform.windows.read_clipboard");
        span.in_scope(|| {
            let mut ctx = self.inner.lock().map_err(|poison| {
                error!("Failed to lock clipboard context in read_snapshot (poisoned mutex)");
                anyhow::anyhow!(
                    "mutex poisoned locking inner in read_snapshot: {}",
                    poison.to_string()
                )
            })?;
            let mut snapshot = CommonClipboardImpl::read_snapshot(&mut ctx)?;

            // Check if clipboard-rs already captured an image
            let has_image = snapshot.representations.iter().any(|rep| {
                rep.mime
                    .as_ref()
                    .is_some_and(|m| m.as_str().starts_with("image/"))
            });

            if has_image {
                debug!(
                    formats = snapshot.representations.len(),
                    total_size_bytes = snapshot.total_size_bytes(),
                    "Captured system clipboard snapshot (image via clipboard-rs)"
                );
                return Ok(snapshot);
            }

            // No image from clipboard-rs -- try Windows native fallback.
            // MUST drop the mutex guard before calling clipboard-win to avoid
            // double clipboard open (clipboard-rs may still hold it internally).
            drop(ctx);

            match read_image_windows_as_png() {
                Ok(png_bytes) => {
                    info!(
                        size_bytes = png_bytes.len(),
                        "Read image via Windows native CF_DIB fallback"
                    );
                    snapshot
                        .representations
                        .push(ObservedClipboardRepresentation::new(
                            RepresentationId::new(),
                            "image".into(),
                            Some(MimeType("image/png".to_string())),
                            png_bytes,
                        ));
                }
                Err(err) => {
                    // Not necessarily an error -- clipboard may genuinely have no image.
                    // Use debug level (not warn) to avoid log noise when user copies text.
                    debug!(error = %err, "Windows native image fallback unavailable");
                }
            }

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
            "platform.windows.write_clipboard",
            representations = snapshot.representations.len(),
        );
        span.in_scope(|| {
            let text_fallback_eligible = is_single_text_plain_snapshot(&snapshot);
            let image_fallback_eligible = is_single_image_snapshot(&snapshot);
            let expected_text = if text_fallback_eligible {
                extract_text_plain_utf8(&snapshot)?
            } else {
                None
            };
            // Extract image bytes before passing snapshot to CommonClipboardImpl
            // (which consumes it by reference but we need the bytes for fallback).
            let image_bytes = if image_fallback_eligible {
                snapshot.representations.first().map(|rep| rep.bytes.clone())
            } else {
                None
            };

            // 多 rep 场景：write_snapshot_multi_windows 内部会自己 OpenClipboard（通过
            // clipboard-win）。若 clipboard-rs 的 ctx 仍持有剪贴板句柄，两者会争抢同一个
            // Windows 剪贴板句柄，产生 OSError 1418（"无法打开剪贴板"）。
            // 解决方案：提前 drop clipboard-rs ctx，再用一个临时 dummy ctx 满足
            // CommonClipboardImpl::write_snapshot 的签名要求——common.rs 内部会立即分流到
            // write_snapshot_multi_windows，不会真正使用这个 dummy ctx。
            if snapshot.representations.len() > 1 {
                // 不需要持有 ctx——多 rep 路径完全由 write_snapshot_multi_windows 接管。
                let mut dummy_ctx = ClipboardContext::new().map_err(|e| {
                    anyhow::anyhow!("创建多 rep 分发用临时 clipboard ctx 失败: {}", e)
                })?;
                return CommonClipboardImpl::write_snapshot(&mut dummy_ctx, snapshot);
            }

            let mut ctx = self.inner.lock().map_err(|poison| {
                error!("Failed to lock clipboard context in write_snapshot (poisoned mutex)");
                anyhow::anyhow!(
                    "mutex poisoned locking inner in write_snapshot: {}",
                    poison.to_string()
                )
            })?;
            let write_result = CommonClipboardImpl::write_snapshot(&mut ctx, snapshot);
            if let Err(err) = write_result {
                // Drop clipboard-rs context before native fallback to avoid double clipboard open
                drop(ctx);

                if text_fallback_eligible {
                    if let Some(text) = expected_text.as_deref() {
                        warn!(
                            error = %err,
                            text_len = text.len(),
                            "Primary clipboard-rs write failed; using Windows Unicode text fallback"
                        );
                        write_text_windows_native(text)?;
                        info!("Wrote clipboard text via Windows Unicode fallback");
                        return Ok(());
                    }
                }

                if image_fallback_eligible {
                    if let Some(bytes) = image_bytes.as_deref() {
                        warn!(
                            error = %err,
                            image_size = bytes.len(),
                            "Primary clipboard-rs image write failed; using Windows native Bitmap fallback"
                        );
                        write_image_windows(bytes)?;
                        info!("Wrote clipboard image via Windows native Bitmap fallback");
                        return Ok(());
                    }
                }

                return Err(err);
            }

            let mut needs_fallback = false;
            if let Some(expected) = expected_text.as_deref() {
                match ctx.get_text() {
                    Ok(actual_text) => {
                        if actual_text != expected {
                            warn!(
                                expected_len = expected.len(),
                                actual_len = actual_text.len(),
                                "Post-write clipboard text mismatch; enabling Windows Unicode fallback"
                            );
                            needs_fallback = true;
                        }
                    }
                    Err(err) => {
                        warn!(
                            error = %err,
                            expected_len = expected.len(),
                            "Post-write clipboard text read failed; enabling Windows Unicode fallback"
                        );
                        needs_fallback = true;
                    }
                }
            }
            drop(ctx);

            if needs_fallback {
                if let Some(text) = expected_text.as_deref() {
                    write_text_windows_native(text)?;
                    info!("Rewrote clipboard text via Windows Unicode fallback after verification");
                }
            }

            info!("Wrote clipboard snapshot to system");
            Ok(())
        })
    }
}

fn extract_text_plain_utf8(snapshot: &SystemClipboardSnapshot) -> Result<Option<String>> {
    let maybe_text_rep = snapshot.representations.iter().find(|rep| {
        rep.mime
            .as_ref()
            .is_some_and(|mime| mime.as_str().eq_ignore_ascii_case("text/plain"))
    });

    let Some(text_rep) = maybe_text_rep else {
        return Ok(None);
    };

    let text = String::from_utf8(text_rep.bytes.clone())
        .map_err(|err| anyhow::anyhow!("Failed to decode text/plain snapshot as UTF-8: {}", err))?;
    Ok(Some(text))
}

fn is_single_text_plain_snapshot(snapshot: &SystemClipboardSnapshot) -> bool {
    if snapshot.representations.len() != 1 {
        return false;
    }

    snapshot.representations[0]
        .mime
        .as_ref()
        .is_some_and(|mime| mime.as_str().eq_ignore_ascii_case("text/plain"))
}

fn is_single_image_snapshot(snapshot: &SystemClipboardSnapshot) -> bool {
    if snapshot.representations.len() != 1 {
        return false;
    }

    snapshot.representations[0]
        .mime
        .as_ref()
        .is_some_and(|mime| mime.as_str().starts_with("image/"))
}

fn write_text_windows_native(text: &str) -> Result<()> {
    clipboard_win::set_clipboard_string(text)
        .map_err(|e| anyhow::anyhow!("Failed to set Windows Unicode clipboard text: {}", e))
}

/// Windows-specific: Read image from clipboard as CF_DIB and convert to PNG bytes.
///
/// Uses `clipboard-win` to read raw CF_DIB data (BITMAPINFOHEADER + pixel data,
/// without the 14-byte BMP file header), then delegates to the cross-platform
/// `dib_to_png` converter.
fn read_image_windows_as_png() -> Result<Vec<u8>> {
    use clipboard_win::{formats, get_clipboard};

    let dib_data: Vec<u8> = get_clipboard(formats::RawData(formats::CF_DIB))
        .map_err(|e| anyhow::anyhow!("No DIB image on clipboard: {}", e))?;

    debug!(
        dib_size_bytes = dib_data.len(),
        "Read CF_DIB from Windows clipboard"
    );
    crate::clipboard::image_convert::dib_to_png(&dib_data)
}

/// Windows-specific: Write image to clipboard as CF_DIB format.
///
/// Uses clipboard-win's `Clipboard` struct for explicit open/close control
/// with retry logic, avoiding the OSError(1418) failures seen with
/// clipboard-rs's set_image() on Windows.
///
/// Accepts raw image bytes in any format supported by the `image` crate
/// (PNG, TIFF, JPEG, BMP, etc.), decodes them, and writes as CF_DIB
/// (BITMAPINFOHEADER + pixel data, without 14-byte BMP file header).
fn write_image_windows(bytes: &[u8]) -> Result<()> {
    use clipboard_win::{formats, Clipboard as ClipboardWin, Setter};

    // Decode image bytes (supports PNG, TIFF, JPEG, BMP, etc. via `image` crate)
    let img = image::load_from_memory(bytes)
        .map_err(|e| anyhow::anyhow!("Failed to decode image for Windows native write: {}", e))?;

    // Convert to full BMP format then strip the 14-byte file header to get CF_DIB data.
    // CF_DIB = BITMAPINFOHEADER (40 bytes) + pixel data (no BMP file header).
    let bmp_bytes = to_bitmap(&img);
    let dib_bytes = &bmp_bytes[14..]; // Skip BITMAPFILEHEADER (14 bytes)

    // Use clipboard-win's Clipboard struct with retry (up to 10 attempts).
    // This handles OpenClipboard/EmptyClipboard/CloseClipboard atomically.
    let _clip = ClipboardWin::new_attempts(10)
        .map_err(|e| anyhow::anyhow!("Failed to open clipboard for image write: {}", e))?;

    clipboard_win::raw::set(formats::CF_DIB, dib_bytes)
        .map_err(|e| anyhow::anyhow!("Failed to write CF_DIB to clipboard: {}", e))?;

    Ok(())
}

/// Convert image to BMP format (Windows Bitmap)
/// Generates BMP file header + info header + pixel data
fn to_bitmap(img: &image::DynamicImage) -> Vec<u8> {
    use image::GenericImageView;

    // Flip image vertically because BMP scan lines are stored bottom to top
    let img = img.flipv();

    // Generate the 54-byte header
    let mut byte_vec = get_bmp_header(img.width(), img.height());

    // Add pixel data (BGRA format)
    for (_, _, pixel) in img.pixels() {
        let pixel_bytes = pixel.0;
        byte_vec.push(pixel_bytes[2]); // B
        byte_vec.push(pixel_bytes[1]); // G
        byte_vec.push(pixel_bytes[0]); // R
        byte_vec.push(pixel_bytes[3]); // A (unused in BMP spec but included)
    }

    byte_vec
}

/// Generate BMP file header and info header (54 bytes total)
fn get_bmp_header(width: u32, height: u32) -> Vec<u8> {
    let mut vec = vec![0; 54];

    // BM signature
    vec[0] = 66; // 'B'
    vec[1] = 77; // 'M'

    // File size
    let file_size = width * height * 4 + 54;
    set_bytes(&mut vec, &file_size.to_le_bytes(), 2..6);

    // Reserved (unused)
    set_bytes(&mut vec, &0_u32.to_le_bytes(), 6..10);

    // Offset to pixel data
    let offset = 54_u32;
    set_bytes(&mut vec, &offset.to_le_bytes(), 10..14);

    // Info header size
    let header_size = 40_u32;
    set_bytes(&mut vec, &header_size.to_le_bytes(), 14..18);

    // Width
    set_bytes(&mut vec, &width.to_le_bytes(), 18..22);

    // Height
    set_bytes(&mut vec, &height.to_le_bytes(), 22..26);

    // Planes (must be 1)
    let planes = 1_u16;
    set_bytes(&mut vec, &planes.to_le_bytes(), 26..28);

    // Bits per pixel (32 bits for BGRA)
    let bits_per_pixel = 32_u16;
    set_bytes(&mut vec, &bits_per_pixel.to_le_bytes(), 28..30);

    // Compression (0 = no compression)
    set_bytes(&mut vec, &0_u32.to_le_bytes(), 30..34);

    // Compressed size (0 when no compression)
    set_bytes(&mut vec, &0_u32.to_le_bytes(), 34..38);

    // Horizontal resolution (0 is allowed)
    set_bytes(&mut vec, &0_u32.to_le_bytes(), 38..42);

    // Vertical resolution (0 is allowed)
    set_bytes(&mut vec, &0_u32.to_le_bytes(), 42..46);

    // Colors used (0 is allowed)
    set_bytes(&mut vec, &0_u32.to_le_bytes(), 46..50);

    // Important colors (0 is allowed)
    set_bytes(&mut vec, &0_u32.to_le_bytes(), 50..54);

    vec
}

/// Helper to set bytes in a slice at a specific range
fn set_bytes(to: &mut [u8], from: &[u8], range: Range<usize>) {
    for (from_idx, i) in range.enumerate() {
        to[i] = from[from_idx];
    }
}
