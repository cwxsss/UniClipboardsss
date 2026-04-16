use super::model::ContentTypes;
use crate::clipboard::link_utils::{is_all_urls, is_single_url};
use crate::clipboard::SystemClipboardSnapshot;

/// Categories of clipboard content determined by MIME type analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentTypeCategory {
    Text,
    Image,
    RichText,
    Link,
    File,
    CodeSnippet,
    Unknown,
}

/// Classify a clipboard snapshot by examining its representations' MIME types.
///
/// File URIs are given highest priority: if **any** representation contains a
/// `text/uri-list` with `file://` URIs, the snapshot is classified as `File`
/// regardless of representation order.  This is necessary because macOS (and
/// some Linux DEs) place convenience representations (e.g. `image/png` for an
/// image file copy) *before* the `text/uri-list` representation, which would
/// otherwise cause the snapshot to be mis-classified as `Image`.
///
/// After the file-URI pre-scan, the remaining representations are checked in
/// order using first-match semantics.
///
/// For `text/uri-list`, the representation data is inspected to distinguish between
/// file URIs (`file://`) and web links (`http://`, `https://`, etc.).
pub fn classify_snapshot(snapshot: &SystemClipboardSnapshot) -> ContentTypeCategory {
    // Pre-scan: if any representation is a file URI list, this is a file copy.
    // This must happen before first-match iteration so that an `image/*`
    // representation placed earlier by the OS does not shadow the file URI.
    for rep in &snapshot.representations {
        if let Some(ref mime) = rep.mime {
            if mime.0.as_str() == "text/uri-list"
                && classify_uri_list(&rep.bytes) == ContentTypeCategory::File
            {
                return ContentTypeCategory::File;
            }
        }
    }

    for rep in &snapshot.representations {
        if let Some(ref mime) = rep.mime {
            let m = mime.0.as_str();
            // Order matters: check specific patterns before generic ones.
            // text/html and text/uri-list must match before the text/plain check.
            match m {
                "text/html" => return ContentTypeCategory::RichText,
                "text/uri-list" => return classify_uri_list(&rep.bytes),
                "text/plain" => {
                    // Check if the plain text content is URL(s)
                    if let Ok(text) = std::str::from_utf8(&rep.bytes) {
                        if is_single_url(text) || is_all_urls(text) {
                            return ContentTypeCategory::Link;
                        }
                    }
                    return ContentTypeCategory::Text;
                }
                "application/octet-stream" => return ContentTypeCategory::File,
                _ if m.starts_with("image/") => return ContentTypeCategory::Image,
                _ => {}
            }
        }
    }
    ContentTypeCategory::Unknown
}

/// Sub-classify a `text/uri-list` representation by inspecting the URI data.
///
/// Per RFC 2483, lines starting with `#` are comments and ignored.
/// The first non-empty, non-comment line determines classification:
/// - Starts with `file://` (case-insensitive) => `File`
/// - Otherwise => `Link`
/// - If data is not valid UTF-8 => `Link` (fallback)
fn classify_uri_list(bytes: &[u8]) -> ContentTypeCategory {
    let text = match std::str::from_utf8(bytes) {
        Ok(s) => s,
        Err(_) => return ContentTypeCategory::Link,
    };

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.len() >= 7 && trimmed[..7].eq_ignore_ascii_case("file://") {
            return ContentTypeCategory::File;
        }
        return ContentTypeCategory::Link;
    }

    // No non-comment URIs found; default to Link
    ContentTypeCategory::Link
}

/// Check whether a content type category is allowed by the given content type toggles.
///
/// `Text`, `Image`, `File`, and `Link` are filterable. All other categories (including `Unknown`)
/// always return `true` — unimplemented types always sync.
pub fn is_content_type_allowed(category: ContentTypeCategory, ct: &ContentTypes) -> bool {
    match category {
        ContentTypeCategory::Text => ct.text,
        ContentTypeCategory::Image => ct.image,
        ContentTypeCategory::File => ct.file,
        ContentTypeCategory::Link => ct.link,
        // Unimplemented types always sync regardless of toggle state
        ContentTypeCategory::RichText
        | ContentTypeCategory::CodeSnippet
        | ContentTypeCategory::Unknown => true,
    }
}
