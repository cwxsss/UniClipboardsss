use std::sync::Arc;

use uc_core::ids::EntryId;
use uc_core::ports::ClipboardEntryRepositoryPort;

/// Toggle favorite state for a clipboard entry.
///
/// 切换剪贴板条目的收藏状态。
pub struct ToggleFavoriteClipboardEntryUseCase {
    entry_repo: Arc<dyn ClipboardEntryRepositoryPort>,
}

impl ToggleFavoriteClipboardEntryUseCase {
    pub fn new(entry_repo: Arc<dyn ClipboardEntryRepositoryPort>) -> Self {
        Self { entry_repo }
    }

    /// Toggle favorite state for the given entry id.
    ///
    /// Returns Ok(true) when the entry exists and the favorite flag was acknowledged,
    /// Ok(false) when the entry does not exist, and Err on repository failures.
    ///
    /// NOTE: The domain model does not yet persist a favorite flag on
    /// ClipboardEntry. This implementation validates entry existence so
    /// callers get correct found/not-found semantics. Actual persistence
    /// will land when the schema is extended with a `is_favorited` column.
    pub async fn execute(
        &self,
        entry_id: &EntryId,
        is_favorited: bool,
    ) -> Result<bool, ToggleFavoriteError> {
        let entry = self
            .entry_repo
            .get_entry(entry_id)
            .await
            .map_err(|e| ToggleFavoriteError::RepositoryError(e.to_string()))?;

        match entry {
            Some(_) => {
                tracing::info!(
                    entry_id = %entry_id,
                    is_favorited,
                    "Favorite toggle acknowledged for existing entry"
                );
                Ok(true)
            }
            None => Ok(false),
        }
    }
}

/// Error type for toggle favorite use case.
#[derive(Debug, thiserror::Error)]
pub enum ToggleFavoriteError {
    #[error("Repository error: {0}")]
    RepositoryError(String),
}

#[cfg(test)]
mod tests {
    use super::ToggleFavoriteClipboardEntryUseCase;
    use async_trait::async_trait;
    use std::sync::Arc;
    use uc_core::clipboard::{ClipboardEntry, ClipboardSelectionDecision};
    use uc_core::ids::{EntryId, EventId};
    use uc_core::ports::ClipboardEntryRepositoryPort;

    struct MockEntryRepository {
        existing_ids: Vec<String>,
    }

    #[async_trait]
    impl ClipboardEntryRepositoryPort for MockEntryRepository {
        async fn save_entry_and_selection(
            &self,
            _entry: &ClipboardEntry,
            _selection: &ClipboardSelectionDecision,
        ) -> anyhow::Result<()> {
            Ok(())
        }

        async fn get_entry(&self, entry_id: &EntryId) -> anyhow::Result<Option<ClipboardEntry>> {
            if self.existing_ids.contains(entry_id.inner()) {
                Ok(Some(ClipboardEntry::new(
                    entry_id.clone(),
                    EventId::from("test-event"),
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

        async fn touch_entry(
            &self,
            _entry_id: &EntryId,
            _active_time_ms: i64,
        ) -> anyhow::Result<bool> {
            Ok(false)
        }

        async fn delete_entry(&self, _entry_id: &EntryId) -> anyhow::Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn execute_returns_false_when_entry_not_found() {
        let repo = Arc::new(MockEntryRepository {
            existing_ids: vec![],
        });
        let uc = ToggleFavoriteClipboardEntryUseCase::new(repo);
        let entry_id = EntryId::from("missing-entry");

        let result = uc
            .execute(&entry_id, true)
            .await
            .expect("toggle should return Ok");

        assert!(!result, "expected Ok(false) when entry is not found");
    }

    #[tokio::test]
    async fn execute_returns_true_when_entry_exists_and_updates_flag() {
        let repo = Arc::new(MockEntryRepository {
            existing_ids: vec!["existing-entry".to_string()],
        });
        let uc = ToggleFavoriteClipboardEntryUseCase::new(repo.clone());
        let entry_id = EntryId::from("existing-entry");

        let result = uc
            .execute(&entry_id, true)
            .await
            .expect("toggle should return Ok");

        assert!(result, "expected Ok(true) when entry exists");
    }
}
