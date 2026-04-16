//! Use case for clearing all clipboard history.
//! 清除所有剪贴板历史的用例。

use anyhow::Result;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{info, info_span, warn, Instrument};
use uc_core::ports::{
    ClipboardEntryRepositoryPort, ClipboardEventWriterPort, ClipboardRepresentationRepositoryPort,
    ClipboardSelectionRepositoryPort, SearchIndexPort,
};

/// Result of a bulk clipboard history clear operation.
/// 批量清除剪贴板历史操作的结果。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClearHistoryResult {
    /// Number of entries successfully deleted.
    pub deleted_count: u64,
    /// Entries that failed to delete: (entry_id, error_message).
    pub failed_entries: Vec<(String, String)>,
}

/// Use case for clearing all clipboard history entries via paginated listing and per-entry deletion.
/// 通过分页列出和逐条删除来清除所有剪贴板历史条目的用例。
pub struct ClearClipboardHistory {
    entry_repo: Arc<dyn ClipboardEntryRepositoryPort>,
    selection_repo: Arc<dyn ClipboardSelectionRepositoryPort>,
    event_writer: Arc<dyn ClipboardEventWriterPort>,
    representation_repo: Arc<dyn ClipboardRepresentationRepositoryPort>,
    file_cache_dir: Option<PathBuf>,
    search_index: Option<Arc<dyn SearchIndexPort>>,
}

const BATCH_SIZE: usize = 1000;

impl ClearClipboardHistory {
    /// Constructs a `ClearClipboardHistory` use case from the required port implementations.
    pub fn from_ports(
        entry_repo: Arc<dyn ClipboardEntryRepositoryPort>,
        selection_repo: Arc<dyn ClipboardSelectionRepositoryPort>,
        event_writer: Arc<dyn ClipboardEventWriterPort>,
        representation_repo: Arc<dyn ClipboardRepresentationRepositoryPort>,
    ) -> Self {
        Self {
            entry_repo,
            selection_repo,
            event_writer,
            representation_repo,
            file_cache_dir: None,
            search_index: None,
        }
    }

    /// Sets the managed file-cache directory used during per-entry cleanup.
    pub fn with_file_cache_dir(mut self, dir: PathBuf) -> Self {
        self.file_cache_dir = Some(dir);
        self
    }

    /// Injects the search index so bulk clear also removes indexed rows.
    pub fn with_search_index(mut self, search_index: Arc<dyn SearchIndexPort>) -> Self {
        self.search_index = Some(search_index);
        self
    }

    /// Clears all clipboard history by paginating through all entries and deleting each one.
    ///
    /// Returns a `ClearHistoryResult` containing the number of successfully deleted entries
    /// and a list of entries that failed to delete. Returns an error only if the initial
    /// listing fails or if ALL deletions fail.
    #[tracing::instrument(name = "usecase.clear_clipboard_history.execute", skip(self))]
    pub async fn execute(&self) -> Result<ClearHistoryResult> {
        // 1. Collect all entry IDs via pagination
        let entries = self.collect_all_entries().await?;

        let total = entries.len() as u64;
        info!(
            total_entries = total,
            "Starting bulk clipboard history deletion"
        );

        if total == 0 {
            return Ok(ClearHistoryResult {
                deleted_count: 0,
                failed_entries: Vec::new(),
            });
        }

        // 2. Delete each entry, tracking successes and failures
        let mut deleted_count = 0u64;
        let mut failed_entries: Vec<(String, String)> = Vec::new();

        let mut delete_uc = super::super::DeleteClipboardEntry::from_ports(
            self.entry_repo.clone(),
            self.selection_repo.clone(),
            self.event_writer.clone(),
            self.representation_repo.clone(),
        );
        if let Some(file_cache_dir) = self.file_cache_dir.clone() {
            delete_uc = delete_uc.with_file_cache_dir(file_cache_dir);
        }
        if let Some(search_index) = self.search_index.clone() {
            delete_uc = delete_uc.with_search_index(search_index);
        }

        for entry in &entries {
            let entry_id_str = entry.entry_id.inner().to_string();
            match delete_uc.execute(&entry.entry_id).await {
                Ok(()) => deleted_count += 1,
                Err(e) => {
                    warn!(
                        entry_id = %entry.entry_id,
                        error = %e,
                        "Failed to delete entry during bulk clear"
                    );
                    failed_entries.push((entry_id_str, e.to_string()));
                }
            }
        }

        info!(
            deleted = deleted_count,
            failed = failed_entries.len(),
            total = total,
            "Clipboard history cleared"
        );

        // If ALL deletions failed, return an error
        if deleted_count == 0 && !failed_entries.is_empty() {
            return Err(anyhow::anyhow!(
                "All {} entries failed to delete",
                failed_entries.len()
            ));
        }

        Ok(ClearHistoryResult {
            deleted_count,
            failed_entries,
        })
    }

    /// Collects all clipboard entries by paginating through the repository.
    async fn collect_all_entries(&self) -> Result<Vec<uc_core::clipboard::ClipboardEntry>> {
        let mut entries = Vec::new();
        let mut offset = 0usize;

        loop {
            let batch = self
                .entry_repo
                .list_entries(BATCH_SIZE, offset)
                .instrument(info_span!(
                    "list_entries_batch",
                    batch_size = BATCH_SIZE,
                    offset = offset
                ))
                .await
                .map_err(|e| anyhow::anyhow!("Failed to list entries for bulk delete: {}", e))?;

            if batch.is_empty() {
                break;
            }

            let batch_len = batch.len();
            entries.extend(batch);
            offset += batch_len;

            if batch_len < BATCH_SIZE {
                break;
            }
        }

        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_mocks::{
        MockClipboardEntryRepository, MockClipboardEventWriter,
        MockClipboardRepresentationRepository, MockClipboardSelectionRepository, MockSearchIndex,
    };
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use uc_core::clipboard::{ClipboardEntry, ClipboardEvent, PersistedClipboardRepresentation};
    use uc_core::ids::{EntryId, EventId, RepresentationId};
    use uc_core::ports::ProcessingUpdateOutcome;
    use uc_core::search::{
        SearchDocument, SearchIndexMeta, SearchPosting, SearchQuery, SearchResultsPage,
    };

    fn make_entry_repo(
        entries: Vec<ClipboardEntry>,
    ) -> (
        MockClipboardEntryRepository,
        Arc<Mutex<HashMap<String, ClipboardEntry>>>,
    ) {
        let store = Arc::new(Mutex::new(
            entries
                .into_iter()
                .map(|entry| (entry.entry_id.to_string(), entry))
                .collect::<HashMap<_, _>>(),
        ));
        let mut repo = MockClipboardEntryRepository::new();
        repo.expect_save_entry_and_selection()
            .returning(|_, _| Ok(()));
        let store_for_get = store.clone();
        repo.expect_get_entry().returning(move |entry_id| {
            Ok(store_for_get
                .lock()
                .unwrap()
                .get(entry_id.as_str())
                .cloned())
        });
        let store_for_list = store.clone();
        repo.expect_list_entries()
            .returning(move |_, _| Ok(store_for_list.lock().unwrap().values().cloned().collect()));
        let store_for_delete = store.clone();
        repo.expect_delete_entry().returning(move |entry_id| {
            store_for_delete.lock().unwrap().remove(entry_id.as_str());
            Ok(())
        });
        (repo, store)
    }

    fn make_selection_repo() -> MockClipboardSelectionRepository {
        let mut repo = MockClipboardSelectionRepository::new();
        repo.expect_get_selection().returning(|_| Ok(None));
        repo.expect_delete_selection().returning(|_| Ok(()));
        repo
    }

    fn make_event_writer(deleted_events: Arc<AtomicUsize>) -> MockClipboardEventWriter {
        let mut writer = MockClipboardEventWriter::new();
        writer
            .expect_insert_event()
            .returning(|_: &ClipboardEvent, _: &Vec<PersistedClipboardRepresentation>| Ok(()));
        writer
            .expect_delete_event_and_representations()
            .returning(move |_: &EventId| {
                deleted_events.fetch_add(1, Ordering::SeqCst);
                Ok(())
            });
        writer
    }

    fn make_representation_repo() -> MockClipboardRepresentationRepository {
        let mut repo = MockClipboardRepresentationRepository::new();
        repo.expect_get_representation()
            .returning(|_: &EventId, _: &RepresentationId| Ok(None));
        repo.expect_get_representation_by_id()
            .returning(|_: &RepresentationId| Ok(None));
        repo.expect_get_representation_by_blob_id()
            .returning(|_: &uc_core::BlobId| Ok(None));
        repo.expect_update_blob_id()
            .returning(|_: &RepresentationId, _: &uc_core::BlobId| Ok(()));
        repo.expect_update_blob_id_if_none()
            .returning(|_: &RepresentationId, _: &uc_core::BlobId| Ok(false));
        repo.expect_update_processing_result().returning(
            |_: &RepresentationId,
             _: &[uc_core::clipboard::PayloadAvailability],
             _: Option<&uc_core::BlobId>,
             _: uc_core::clipboard::PayloadAvailability,
             _: Option<&str>| Ok(ProcessingUpdateOutcome::NotFound),
        );
        repo
    }

    fn make_search_index(removed_entry_ids: Arc<Mutex<Vec<String>>>) -> MockSearchIndex {
        let mut index = MockSearchIndex::new();
        index
            .expect_index_entry()
            .returning(|_: SearchDocument, _: Vec<SearchPosting>| Ok(()));
        index.expect_remove_entry().returning(move |entry_id| {
            removed_entry_ids.lock().unwrap().push(entry_id.to_string());
            Ok(())
        });
        index.expect_search().returning(|_: SearchQuery| {
            Ok(SearchResultsPage {
                items: vec![],
                total: 0,
                has_more: false,
            })
        });
        index
            .expect_rebuild()
            .returning(|_: Vec<(SearchDocument, Vec<SearchPosting>)>, _| Ok(()));
        index.expect_get_index_meta().returning(|| {
            Ok(SearchIndexMeta {
                index_version: "search-v2".into(),
                search_blocked: false,
                last_rebuild_started_at_ms: None,
                last_rebuild_completed_at_ms: None,
            })
        });
        index
    }

    #[tokio::test]
    async fn execute_removes_search_index_entries_during_bulk_clear() {
        let entry = ClipboardEntry::new(
            EntryId::from("bulk-clear-entry-1"),
            EventId::from("bulk-clear-event-1"),
            1_777_000_000_000,
            Some("bulk clear".into()),
            42,
        );
        let (entry_repo, store) = make_entry_repo(vec![entry.clone()]);
        let deleted_events = Arc::new(AtomicUsize::new(0));
        let removed_entry_ids = Arc::new(Mutex::new(Vec::new()));
        let event_writer = Arc::new(make_event_writer(deleted_events.clone()));
        let search_index = Arc::new(make_search_index(removed_entry_ids.clone()));

        let result = ClearClipboardHistory::from_ports(
            Arc::new(entry_repo),
            Arc::new(make_selection_repo()),
            event_writer.clone(),
            Arc::new(make_representation_repo()),
        )
        .with_search_index(search_index.clone())
        .execute()
        .await
        .expect("bulk clear should succeed");

        assert_eq!(result.deleted_count, 1);
        assert!(result.failed_entries.is_empty());
        assert!(
            store.lock().unwrap().is_empty(),
            "bulk clear should delete stored entries"
        );
        assert_eq!(
            *removed_entry_ids.lock().unwrap(),
            vec![entry.entry_id.to_string()]
        );
        assert_eq!(deleted_events.load(Ordering::SeqCst), 1);
    }
}
