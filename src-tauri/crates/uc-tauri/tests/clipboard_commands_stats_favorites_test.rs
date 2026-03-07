//! Tests for clipboard stats, favorite toggle, and get_clipboard_item contracts.
//!
//! These tests validate DTO serialization and JSON shape contracts that the
//! frontend depends on. Integration-level command invocation tests are deferred
//! until the pre-existing uc-tauri test compilation issues are resolved.

use uc_tauri::models::{
    ClipboardImageItemDto, ClipboardItemDto, ClipboardItemResponse, ClipboardStats,
    ClipboardTextItemDto,
};

#[test]
fn clipboard_stats_serialization_matches_contract() {
    let stats = ClipboardStats {
        total_items: 3,
        total_size: 42,
    };

    let value = serde_json::to_value(&stats).expect("serialize ClipboardStats");
    assert!(
        value.get("total_items").is_some(),
        "expected total_items key"
    );
    assert!(value.get("total_size").is_some(), "expected total_size key");
    assert!(
        value.get("totalItems").is_none(),
        "unexpected camelCase totalItems"
    );
    assert!(
        value.get("totalSize").is_none(),
        "unexpected camelCase totalSize"
    );
}

#[test]
fn clipboard_stats_round_trip() {
    let stats = ClipboardStats {
        total_items: 10,
        total_size: 2048,
    };

    let json = serde_json::to_string(&stats).expect("serialize");
    let deserialized: ClipboardStats = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(deserialized.total_items, 10);
    assert_eq!(deserialized.total_size, 2048);
}

/// Verify the toggle favorite use case returns correct found/not-found semantics
/// by exercising the app-layer use case directly with mock repositories.
mod toggle_favorite_usecase_tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use uc_app::usecases::clipboard::toggle_favorite_clipboard_entry::ToggleFavoriteClipboardEntryUseCase;
    use uc_core::clipboard::{ClipboardEntry, ClipboardSelectionDecision};
    use uc_core::ids::{EntryId, EventId};
    use uc_core::ports::ClipboardEntryRepositoryPort;

    struct MockEntryRepo {
        has_entry: bool,
    }

    #[async_trait]
    impl ClipboardEntryRepositoryPort for MockEntryRepo {
        async fn save_entry_and_selection(
            &self,
            _entry: &ClipboardEntry,
            _selection: &ClipboardSelectionDecision,
        ) -> anyhow::Result<()> {
            Ok(())
        }

        async fn get_entry(&self, entry_id: &EntryId) -> anyhow::Result<Option<ClipboardEntry>> {
            if self.has_entry {
                Ok(Some(ClipboardEntry::new(
                    entry_id.clone(),
                    EventId::from("evt-1"),
                    1000,
                    None,
                    64,
                )))
            } else {
                Ok(None)
            }
        }

        async fn list_entries(
            &self,
            _limit: usize,
            _offset: usize,
        ) -> anyhow::Result<Vec<ClipboardEntry>> {
            Ok(vec![])
        }

        async fn delete_entry(&self, _entry_id: &EntryId) -> anyhow::Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn toggle_favorite_returns_true_for_existing_entry() {
        let repo = Arc::new(MockEntryRepo { has_entry: true });
        let uc = ToggleFavoriteClipboardEntryUseCase::new(repo);
        let entry_id = EntryId::from("existing-entry");

        let result = uc.execute(&entry_id, true).await.expect("should not error");
        assert!(result, "expected Ok(true) for existing entry");
    }

    #[tokio::test]
    async fn toggle_favorite_returns_false_for_missing_entry() {
        let repo = Arc::new(MockEntryRepo { has_entry: false });
        let uc = ToggleFavoriteClipboardEntryUseCase::new(repo);
        let entry_id = EntryId::from("missing-entry");

        let result = uc.execute(&entry_id, true).await.expect("should not error");
        assert!(!result, "expected Ok(false) for missing entry");
    }
}

/// Verify ClipboardItemResponse DTO matches the frontend ClipboardItemResponse
/// interface, validating JSON keys and nested item structure.
mod get_clipboard_item_contract_tests {
    use super::*;

    #[test]
    fn clipboard_item_response_text_matches_frontend_contract() {
        let response = ClipboardItemResponse {
            id: "entry-abc".to_string(),
            is_downloaded: true,
            is_favorited: false,
            created_at: 1000,
            updated_at: 2000,
            active_time: 3000,
            item: ClipboardItemDto {
                text: Some(ClipboardTextItemDto {
                    display_text: "hello".to_string(),
                    has_detail: true,
                    size: 5,
                }),
                image: None,
                file: None,
                link: None,
                code: None,
                unknown: None,
            },
        };

        let value = serde_json::to_value(&response).expect("serialize");

        // Top-level keys match frontend interface
        assert_eq!(value["id"], "entry-abc");
        assert_eq!(value["is_downloaded"], true);
        assert_eq!(value["is_favorited"], false);
        assert_eq!(value["created_at"], 1000);
        assert_eq!(value["updated_at"], 2000);
        assert_eq!(value["active_time"], 3000);

        // Nested item with text present
        let item = &value["item"];
        assert!(item.get("text").is_some());
        assert_eq!(item["text"]["display_text"], "hello");
        assert_eq!(item["text"]["has_detail"], true);
        assert_eq!(item["text"]["size"], 5);

        // Other item types should be absent (skip_serializing_if)
        assert!(item.get("image").is_none());
        assert!(item.get("file").is_none());
        assert!(item.get("link").is_none());
        assert!(item.get("code").is_none());
    }

    #[test]
    fn clipboard_item_response_image_matches_frontend_contract() {
        let response = ClipboardItemResponse {
            id: "entry-img".to_string(),
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

        let value = serde_json::to_value(&response).expect("serialize");
        let item = &value["item"];

        assert!(item.get("text").is_none());
        assert!(item.get("image").is_some());
        assert_eq!(item["image"]["thumbnail"], "uc://thumbnail/rep-1");
        assert_eq!(item["image"]["size"], 2048);
        assert_eq!(item["image"]["width"], 120);
        assert_eq!(item["image"]["height"], 80);
    }

    #[test]
    fn clipboard_item_response_round_trip() {
        let response = ClipboardItemResponse {
            id: "round-trip".to_string(),
            is_downloaded: false,
            is_favorited: true,
            created_at: 111,
            updated_at: 222,
            active_time: 333,
            item: ClipboardItemDto {
                text: Some(ClipboardTextItemDto {
                    display_text: "rt".to_string(),
                    has_detail: false,
                    size: 2,
                }),
                image: None,
                file: None,
                link: None,
                code: None,
                unknown: None,
            },
        };

        let json = serde_json::to_string(&response).expect("serialize");
        let deserialized: ClipboardItemResponse = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(deserialized.id, "round-trip");
        assert!(!deserialized.is_downloaded);
        assert!(deserialized.is_favorited);
        assert_eq!(deserialized.created_at, 111);
        assert_eq!(deserialized.item.text.as_ref().unwrap().display_text, "rt");
    }
}
