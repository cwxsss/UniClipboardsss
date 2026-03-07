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
use uc_app::usecases::LifecycleState;

/// Clipboard entry projection for frontend API.
/// 前端 API 的剪贴板条目投影。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardEntryProjection {
    /// Unique identifier for the entry
    pub id: String,
    /// Preview content (truncated for large text, placeholder for images)
    pub preview: String,
    /// Whether full detail is available (has blob or is expandable)
    pub has_detail: bool,
    /// Total size in bytes
    pub size_bytes: i64,
    /// Timestamp when captured (Unix timestamp)
    pub captured_at: i64,
    /// Content type description
    pub content_type: String,
    /// Optional thumbnail URL for image entries
    pub thumbnail_url: Option<String>,
    /// Whether the content is encrypted
    pub is_encrypted: bool,
    /// Whether the entry is favorited
    pub is_favorited: bool,
    /// Timestamp when last updated
    pub updated_at: i64,
    /// Timestamp of last access/use
    pub active_time: i64,
}

/// Clipboard entries response with readiness status
/// 带就绪状态的剪贴板条目响应
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ClipboardEntriesResponse {
    /// Session is ready; entries are available
    Ready {
        entries: Vec<ClipboardEntryProjection>,
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
    /// Blob identifier for the entry payload
    pub blob_id: String,
    /// MIME type for the payload
    pub mime_type: String,
    /// Payload size in bytes
    pub size_bytes: i64,
    /// Custom protocol URL for resource fetching
    pub url: String,
}

/// Clipboard statistics DTO for frontend API.
/// 前端 API 使用的剪贴板统计信息 DTO。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardStats {
    /// Total number of clipboard items
    pub total_items: i64,
    /// Total size of all clipboard items in bytes
    pub total_size: i64,
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
    fn clipboard_entry_projection_preserves_snake_case() {
        let entry = ClipboardEntryProjection {
            id: "test-id".to_string(),
            preview: "hello".to_string(),
            has_detail: true,
            size_bytes: 100,
            captured_at: 1234567890,
            content_type: "text/plain".to_string(),
            thumbnail_url: None,
            is_encrypted: false,
            is_favorited: false,
            updated_at: 1234567890,
            active_time: 1234567890,
        };
        let value = serde_json::to_value(&entry).expect("serialize failed");
        // Verify snake_case field names (not camelCase)
        assert!(
            value.get("has_detail").is_some(),
            "expected snake_case 'has_detail'"
        );
        assert!(
            value.get("size_bytes").is_some(),
            "expected snake_case 'size_bytes'"
        );
        assert!(
            value.get("captured_at").is_some(),
            "expected snake_case 'captured_at'"
        );
        assert!(
            value.get("content_type").is_some(),
            "expected snake_case 'content_type'"
        );
        assert!(
            value.get("thumbnail_url").is_some(),
            "expected snake_case 'thumbnail_url'"
        );
        assert!(
            value.get("is_encrypted").is_some(),
            "expected snake_case 'is_encrypted'"
        );
        assert!(
            value.get("is_favorited").is_some(),
            "expected snake_case 'is_favorited'"
        );
        assert!(
            value.get("updated_at").is_some(),
            "expected snake_case 'updated_at'"
        );
        assert!(
            value.get("active_time").is_some(),
            "expected snake_case 'active_time'"
        );
        // Ensure camelCase variants are NOT present
        assert!(
            value.get("hasDetail").is_none(),
            "unexpected camelCase 'hasDetail'"
        );
        assert!(
            value.get("sizeBytes").is_none(),
            "unexpected camelCase 'sizeBytes'"
        );
    }

    #[test]
    fn clipboard_stats_serializes_with_snake_case_fields() {
        let stats = ClipboardStats {
            total_items: 5,
            total_size: 1024,
        };
        let value = serde_json::to_value(&stats).expect("serialize failed");

        assert!(
            value.get("total_items").is_some(),
            "expected snake_case 'total_items'",
        );
        assert!(
            value.get("total_size").is_some(),
            "expected snake_case 'total_size'",
        );
        assert!(
            value.get("totalItems").is_none(),
            "unexpected camelCase 'totalItems'",
        );
        assert!(
            value.get("totalSize").is_none(),
            "unexpected camelCase 'totalSize'",
        );
    }
}
