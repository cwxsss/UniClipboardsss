//! Tests for clipboard stats, favorite toggle, and get_clipboard_item contracts.
//!
//! These tests validate DTO serialization and JSON shape contracts that the
//! frontend depends on. Integration-level command invocation tests are deferred
//! until the pre-existing uc-tauri test compilation issues are resolved.

use uc_tauri::models::ClipboardStats;

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
