//! Data Transfer Objects and Projection Models
//!
//! This module contains data structures that are exposed to the frontend.
//! These separate the internal domain models from the API contract.
//!
//! 数据传输对象和投影模型
//!
//! 此模块包含暴露给前端的数据结构。
//! 这些将内部领域模型与 API 契约分离。

use serde::{Deserialize, Serialize};
use uc_app::usecases::clipboard::EntryProjectionDto;
use uc_app::usecases::LifecycleState;

/// Clipboard entries response with readiness status
/// 带就绪状态的剪贴板条目响应
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ClipboardEntriesResponse {
    /// Session is ready; entries are available
    Ready {
        entries: Vec<EntryProjectionDto>,
    },
    /// Session not ready yet (e.g., awaiting unlock)
    NotReady,
}

/// Full clipboard entry detail
/// 剪贴板条目完整详情
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardEntryDetail {
    /// Unique identifier for the entry
    pub id: String,
    /// Full content
    pub content: String,
    /// Total size in bytes
    pub size_bytes: i64,
    /// Content type description
    pub content_type: String,
    /// Whether the entry is favorited
    pub is_favorited: bool,
    /// Timestamp when last updated
    pub updated_at: i64,
    /// Timestamp of last access/use
    pub active_time: i64,
}

/// Clipboard entry resource metadata
/// 剪贴板条目资源元信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardEntryResource {
    /// Blob identifier for the entry payload (None for inline content)
    pub blob_id: Option<String>,
    /// MIME type for the payload
    pub mime_type: String,
    /// Payload size in bytes
    pub size_bytes: i64,
    /// Custom protocol URL for resource fetching (None for inline content)
    pub url: Option<String>,
    /// Base64-encoded inline data (present when content is stored inline, not in blob)
    pub inline_data: Option<String>,
}

/// Nested clipboard item representation for get_clipboard_item response.
/// Mirrors the frontend ClipboardItem interface.
/// 嵌套剪贴板条目表示，用于 get_clipboard_item 响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardItemDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<ClipboardTextItemDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<ClipboardImageItemDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<ClipboardLinkItemDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unknown: Option<serde_json::Value>,
}

/// Text item DTO for clipboard item response.
/// 文本条目 DTO。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardTextItemDto {
    pub display_text: String,
    pub has_detail: bool,
    pub size: i64,
}

/// Image item DTO for clipboard item response.
/// 图片条目 DTO。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardImageItemDto {
    pub thumbnail: Option<String>,
    pub size: i64,
    pub width: i64,
    pub height: i64,
}

/// Link item DTO for clipboard item response.
/// 链接条目 DTO。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardLinkItemDto {
    pub urls: Vec<String>,
    pub domains: Vec<String>,
}

/// Response DTO for get_clipboard_item command.
/// Matches the frontend ClipboardItemResponse interface.
/// 前端 ClipboardItemResponse 接口匹配的响应 DTO。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardItemResponse {
    pub id: String,
    pub is_downloaded: bool,
    pub is_favorited: bool,
    pub created_at: i64,
    pub updated_at: i64,
    pub active_time: i64,
    pub item: ClipboardItemDto,
}

/// Lifecycle status DTO for the frontend API.
/// 前端 API 的生命周期状态 DTO。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LifecycleStatusDto {
    /// Current lifecycle state (e.g. "Idle", "Ready", "WatcherFailed", etc.)
    pub state: LifecycleState,
}

impl LifecycleStatusDto {
    pub fn from_state(state: LifecycleState) -> Self {
        Self { state }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_entries_response_ready_serializes_correctly() {
        let response = ClipboardEntriesResponse::Ready { entries: vec![] };
        let json = serde_json::to_string(&response).expect("serialize failed");
        assert_eq!(json, r#"{"status":"ready","entries":[]}"#);
    }

    #[test]
    fn clipboard_entries_response_not_ready_serializes_correctly() {
        let response = ClipboardEntriesResponse::NotReady;
        let json = serde_json::to_string(&response).expect("serialize failed");
        assert_eq!(json, r#"{"status":"not_ready"}"#);
    }

    #[test]
    fn lifecycle_status_dto_serializes_with_camel_case() {
        // The struct field "state" is already one word, but we verify camelCase rename_all is applied
        let dto = LifecycleStatusDto {
            state: LifecycleState::Ready,
        };
        let value = serde_json::to_value(&dto).expect("serialize failed");
        // Verify it has "state" key (camelCase of "state" is still "state")
        assert!(
            value.get("state").is_some(),
            "expected 'state' field in JSON"
        );
        assert_eq!(value["state"], serde_json::json!("Ready"));

        // Verify all variants serialize as expected
        let idle = LifecycleStatusDto::from_state(LifecycleState::Idle);
        let idle_json = serde_json::to_value(&idle).expect("serialize failed");
        assert_eq!(idle_json["state"], serde_json::json!("Idle"));

        let watcher_failed = LifecycleStatusDto::from_state(LifecycleState::WatcherFailed);
        let wf_json = serde_json::to_value(&watcher_failed).expect("serialize failed");
        assert_eq!(wf_json["state"], serde_json::json!("WatcherFailed"));
    }

    #[test]
    fn clipboard_item_response_serializes_with_expected_keys() {
        let response = ClipboardItemResponse {
            id: "entry-1".to_string(),
            is_downloaded: true,
            is_favorited: false,
            created_at: 1000,
            updated_at: 2000,
            active_time: 3000,
            item: ClipboardItemDto {
                text: Some(ClipboardTextItemDto {
                    display_text: "hello world".to_string(),
                    has_detail: true,
                    size: 11,
                }),
                image: None,
                file: None,
                link: None,
                code: None,
                unknown: None,
            },
        };

        let value = serde_json::to_value(&response).expect("serialize failed");

        // Verify all top-level keys
        assert!(value.get("id").is_some(), "expected 'id'");
        assert!(
            value.get("is_downloaded").is_some(),
            "expected 'is_downloaded'"
        );
        assert!(
            value.get("is_favorited").is_some(),
            "expected 'is_favorited'"
        );
        assert!(value.get("created_at").is_some(), "expected 'created_at'");
        assert!(value.get("updated_at").is_some(), "expected 'updated_at'");
        assert!(value.get("active_time").is_some(), "expected 'active_time'");
        assert!(value.get("item").is_some(), "expected 'item'");

        // Verify nested text item
        let item = value.get("item").unwrap();
        assert!(item.get("text").is_some(), "expected 'text' in item");
        let text = item.get("text").unwrap();
        assert_eq!(text["display_text"], "hello world");
        assert_eq!(text["has_detail"], true);
        assert_eq!(text["size"], 11);

        // Verify camelCase is NOT used
        assert!(
            value.get("isDownloaded").is_none(),
            "unexpected camelCase 'isDownloaded'"
        );
        assert!(
            value.get("createdAt").is_none(),
            "unexpected camelCase 'createdAt'"
        );
    }

    #[test]
    fn clipboard_item_response_omits_none_item_fields() {
        let response = ClipboardItemResponse {
            id: "entry-2".to_string(),
            is_downloaded: true,
            is_favorited: true,
            created_at: 500,
            updated_at: 600,
            active_time: 700,
            item: ClipboardItemDto {
                text: None,
                image: Some(ClipboardImageItemDto {
                    thumbnail: Some("uc://thumbnail/rep-1".to_string()),
                    size: 2048,
                    width: 120,
                    height: 80,
                }),
                file: None,
                link: None,
                code: None,
                unknown: None,
            },
        };

        let value = serde_json::to_value(&response).expect("serialize failed");
        let item = value.get("item").unwrap();

        // text should be absent (skip_serializing_if)
        assert!(item.get("text").is_none(), "text should be omitted");
        // image should be present
        assert!(item.get("image").is_some(), "image should be present");
        let image = item.get("image").unwrap();
        assert_eq!(image["thumbnail"], "uc://thumbnail/rep-1");
        assert_eq!(image["size"], 2048);
    }

}
