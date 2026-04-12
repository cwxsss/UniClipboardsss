use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{info, info_span, warn, Instrument};
use uc_core::ids::EntryId;
use uc_core::ports::{
    ClipboardEntryRepositoryPort, ClipboardEventWriterPort, ClipboardRepresentationRepositoryPort,
    ClipboardSelectionRepositoryPort, SearchIndexPort,
};

/// Use case for deleting clipboard entries with all associated data.
/// 删除剪贴板条目及其所有关联数据的用例。
pub struct DeleteClipboardEntry {
    entry_repo: Arc<dyn ClipboardEntryRepositoryPort>,
    selection_repo: Arc<dyn ClipboardSelectionRepositoryPort>,
    event_writer: Arc<dyn ClipboardEventWriterPort>,
    representation_repo: Arc<dyn ClipboardRepresentationRepositoryPort>,
    /// The managed file-cache directory. Only files located inside this directory
    /// are deleted from disk when an entry is removed. Files outside this boundary
    /// are user-owned originals and must never be touched.
    file_cache_dir: Option<PathBuf>,
    /// Optional search index port. When set, `execute()` synchronously removes the
    /// entry's document from the search index as part of the delete chain. Failures
    /// are logged at warn level and do not block the delete (SIDX-02, D-07).
    search_index: Option<Arc<dyn SearchIndexPort>>,
}

impl DeleteClipboardEntry {
    /// Constructs a `DeleteClipboardEntry` use case from repository and event-writer ports.
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

    /// Sets the managed file-cache directory.
    ///
    /// Only files whose path is inside this directory will be deleted from disk when
    /// an entry is removed. This prevents the deletion of user-owned original files.
    pub fn with_file_cache_dir(mut self, dir: PathBuf) -> Self {
        self.file_cache_dir = Some(dir);
        self
    }

    /// Inject a search index port so deletes cascade to the search index.
    ///
    /// When set, `execute()` will synchronously call `remove_entry(entry_id)` on
    /// the port as part of the delete chain. Failures are logged and do not
    /// block the delete (SIDX-02, D-07).
    pub fn with_search_index(mut self, search_index: Arc<dyn SearchIndexPort>) -> Self {
        self.search_index = Some(search_index);
        self
    }

    /// Deletes a clipboard entry and its associated selection, event, and snapshot representations in the required order.
    /// For file entries (text/uri-list), also deletes the cache files from disk.
    ///
    /// Deletion order (respecting foreign key constraints):
    /// 1. Verify the entry exists (returns an error if missing).
    /// 1b. If entry has text/uri-list representation, delete cache files from disk.
    /// 2. Delete the clipboard selection associated with the entry.
    /// 3. Delete the clipboard entry (must be deleted before its referenced event).
    /// 4. Delete the event and its snapshot representations using the entry's `event_id`.
    #[tracing::instrument(
        name = "usecase.delete_clipboard_entry.execute",
        skip(self),
        fields(entry_id = %entry_id)
    )]
    pub async fn execute(&self, entry_id: &EntryId) -> Result<()> {
        // 1. Fetch entry to verify existence and get event_id
        let entry = async {
            self.entry_repo
                .get_entry(entry_id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("Clipboard entry not found: {}", entry_id))
        }
        .instrument(info_span!(
            "fetch_entry",
            entry_id = %entry_id
        ))
        .await?;
        let event_id = entry.event_id.clone();

        // 1b. Check for file representations and delete cache files.
        // Only files that live inside the managed file_cache_dir are deleted.
        // Files outside that boundary are user-owned originals and must not be touched.
        async {
            let Some(ref cache_dir) = self.file_cache_dir else {
                // No cache dir configured — skip file deletion entirely to be safe.
                return;
            };

            if let Ok(representations) = self
                .representation_repo
                .get_representations_for_event(&event_id)
                .await
            {
                for rep in &representations {
                    let mime = rep.mime_type.as_ref().map(|m| m.as_str()).unwrap_or("");
                    if mime.contains("uri-list") {
                        // Parse URI list content and delete only files inside the cache dir
                        if let Some(ref inline) = rep.inline_data {
                            let uri_text = String::from_utf8_lossy(inline);
                            for line in uri_text.lines() {
                                let line = line.trim();
                                if line.is_empty() || line.starts_with('#') {
                                    continue;
                                }
                                // Support both file:// URIs and native paths
                                let path = if line.starts_with("file://") {
                                    url::Url::parse(line)
                                        .ok()
                                        .and_then(|u| u.to_file_path().ok())
                                } else {
                                    Some(std::path::PathBuf::from(line))
                                };

                                let Some(path) = path else {
                                    continue;
                                };

                                // Guard: only delete files that are inside the managed cache dir.
                                // This prevents accidental deletion of user-owned original files.
                                if !path.starts_with(cache_dir) {
                                    info!(
                                        path = %path.display(),
                                        cache_dir = %cache_dir.display(),
                                        "Skipping file deletion — path is outside the managed file-cache directory (user-owned file)"
                                    );
                                    continue;
                                }

                                if let Err(e) = tokio::fs::remove_file(&path).await {
                                    warn!(
                                        path = %path.display(),
                                        error = %e,
                                        "Failed to delete cache file during entry cleanup"
                                    );
                                } else {
                                    info!(
                                        path = %path.display(),
                                        "Deleted cache file during entry cleanup"
                                    );
                                    // Try to remove the parent directory (e.g. UUID dir) if it's
                                    // now empty and still inside the cache boundary.
                                    if let Some(parent) = path.parent() {
                                        if parent != cache_dir.as_path()
                                            && parent.starts_with(cache_dir)
                                        {
                                            // remove_dir only succeeds when dir is empty
                                            let _ = tokio::fs::remove_dir(parent).await;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        .instrument(info_span!("cleanup_cache_files", event_id = %event_id))
        .await;

        // 1c. Remove entry from search index (non-authoritative — warn and continue on error).
        if let Some(search_index) = self.search_index.as_ref() {
            async {
                if let Err(e) = search_index.remove_entry(entry_id).await {
                    warn!(
                        error = %e,
                        entry_id = %entry_id,
                        "search index cleanup failed, continuing delete"
                    );
                }
            }
            .instrument(info_span!("cleanup_search_index", entry_id = %entry_id))
            .await;
        }

        // 2. Delete selection (references entry)
        self.selection_repo
            .delete_selection(entry_id)
            .instrument(info_span!(
                "delete_selection",
                entry_id = %entry_id
            ))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to delete selection: {}", e))?;

        // 3. Delete entry (references event - must delete before event)
        self.entry_repo
            .delete_entry(entry_id)
            .instrument(info_span!(
                "delete_entry",
                entry_id = %entry_id
            ))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to delete entry: {}", e))?;

        // 4. Delete event and representations (now safe since entry is gone)
        self.event_writer
            .delete_event_and_representations(&event_id)
            .instrument(info_span!(
                "delete_event",
                event_id = %event_id
            ))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to delete event: {}", e))?;

        info!(
            entry_id = %entry_id,
            event_id = %event_id,
            "Deleted clipboard entry successfully"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_mocks::{
        MockClipboardEntryRepository, MockClipboardEventWriter,
        MockClipboardRepresentationRepository, MockClipboardSelectionRepository, MockSearchIndex,
    };
    use uc_core::clipboard::{ClipboardEntry, PersistedClipboardRepresentation};
    use uc_core::clipboard::{MimeType, PayloadAvailability};
    use uc_core::ids::{EntryId, EventId};
    use uc_core::ids::{FormatId, RepresentationId};
    use uc_core::ports::ProcessingUpdateOutcome;

    fn build_entry(entry_id: EntryId, event_id: EventId) -> ClipboardEntry {
        ClipboardEntry::new(
            entry_id,
            event_id,
            1234567890,
            Some("Test Entry".to_string()),
            1024,
        )
    }

    fn make_default_representation_repo() -> MockClipboardRepresentationRepository {
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
             _: &[PayloadAvailability],
             _: Option<&uc_core::BlobId>,
             _: PayloadAvailability,
             _: Option<&str>| Ok(ProcessingUpdateOutcome::NotFound),
        );
        repo.expect_update_mime_type()
            .returning(|_: &RepresentationId, _: &MimeType| Ok(()));
        repo
    }

    fn build_uri_list_rep(uri_list_content: &str) -> PersistedClipboardRepresentation {
        PersistedClipboardRepresentation::new(
            RepresentationId::new(),
            FormatId::from("files"),
            Some(MimeType::uri_list()),
            uri_list_content.len() as i64,
            Some(uri_list_content.as_bytes().to_vec()),
            None,
        )
    }

    #[tokio::test]
    async fn test_execute_deletes_all_related_data() {
        let delete_entry_called = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let delete_selection_called =
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let delete_event_called = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

        let entry_id = EntryId::from("test-entry".to_string());
        let event_id = EventId::from("test-event".to_string());
        let entry = build_entry(entry_id.clone(), event_id.clone());

        let mut entry_repo = MockClipboardEntryRepository::new();
        let entry_for_get = entry.clone();
        entry_repo
            .expect_get_entry()
            .returning(move |_| Ok(Some(entry_for_get.clone())));
        let delete_entry_called_for_closure = delete_entry_called.clone();
        entry_repo.expect_delete_entry().returning(move |_| {
            delete_entry_called_for_closure.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        });

        let mut selection_repo = MockClipboardSelectionRepository::new();
        let delete_selection_called_for_closure = delete_selection_called.clone();
        selection_repo
            .expect_delete_selection()
            .returning(move |_| {
                delete_selection_called_for_closure
                    .store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            });

        let mut event_writer = MockClipboardEventWriter::new();
        let delete_event_called_for_closure = delete_event_called.clone();
        event_writer
            .expect_delete_event_and_representations()
            .returning(move |_| {
                delete_event_called_for_closure.store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            });

        let use_case = DeleteClipboardEntry::from_ports(
            Arc::new(entry_repo),
            Arc::new(selection_repo),
            Arc::new(event_writer),
            Arc::new(make_default_representation_repo()),
        );

        let result = use_case.execute(&entry_id).await;

        assert!(result.is_ok(), "Deletion should succeed");
        assert!(delete_selection_called.load(std::sync::atomic::Ordering::SeqCst));
        assert!(delete_event_called.load(std::sync::atomic::Ordering::SeqCst));
        assert!(delete_entry_called.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_execute_returns_not_found_for_nonexistent_entry() {
        let delete_entry_called = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let delete_selection_called =
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let delete_event_called = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

        let entry_id = EntryId::from("nonexistent".to_string());

        let mut entry_repo = MockClipboardEntryRepository::new();
        entry_repo.expect_get_entry().returning(|_| Ok(None));

        let mut selection_repo = MockClipboardSelectionRepository::new();
        let delete_selection_called_for_closure = delete_selection_called.clone();
        selection_repo
            .expect_delete_selection()
            .returning(move |_| {
                delete_selection_called_for_closure
                    .store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            });

        let mut event_writer = MockClipboardEventWriter::new();
        let delete_event_called_for_closure = delete_event_called.clone();
        event_writer
            .expect_delete_event_and_representations()
            .returning(move |_| {
                delete_event_called_for_closure.store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            });

        let use_case = DeleteClipboardEntry::from_ports(
            Arc::new(entry_repo),
            Arc::new(selection_repo),
            Arc::new(event_writer),
            Arc::new(make_default_representation_repo()),
        );

        let result = use_case.execute(&entry_id).await;

        assert!(result.is_err(), "Should return error for nonexistent entry");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("not found"),
            "Error should contain 'not found': {}",
            err
        );

        assert!(!delete_selection_called.load(std::sync::atomic::Ordering::SeqCst));
        assert!(!delete_event_called.load(std::sync::atomic::Ordering::SeqCst));
        assert!(!delete_entry_called.load(std::sync::atomic::Ordering::SeqCst));
    }

    fn make_test_use_case_with_uri_list(
        uri_list: &str,
        file_cache_dir: Option<std::path::PathBuf>,
    ) -> DeleteClipboardEntry {
        let entry_id = EntryId::from("test-entry".to_string());
        let event_id = EventId::from("test-event".to_string());
        let entry = build_entry(entry_id.clone(), event_id.clone());

        let mut entry_repo = MockClipboardEntryRepository::new();
        let entry_for_get = entry.clone();
        entry_repo
            .expect_get_entry()
            .returning(move |_| Ok(Some(entry_for_get.clone())));
        entry_repo.expect_delete_entry().returning(|_| Ok(()));

        let mut selection_repo = MockClipboardSelectionRepository::new();
        selection_repo
            .expect_delete_selection()
            .returning(|_| Ok(()));

        let mut event_writer = MockClipboardEventWriter::new();
        event_writer
            .expect_delete_event_and_representations()
            .returning(|_| Ok(()));

        let mut rep_repo = make_default_representation_repo();
        let uri_list_owned = uri_list.to_string();
        rep_repo
            .expect_get_representations_for_event()
            .returning(move |_| Ok(vec![build_uri_list_rep(&uri_list_owned)]));

        let uc = DeleteClipboardEntry::from_ports(
            Arc::new(entry_repo),
            Arc::new(selection_repo),
            Arc::new(event_writer),
            Arc::new(rep_repo),
        );
        if let Some(dir) = file_cache_dir {
            uc.with_file_cache_dir(dir)
        } else {
            uc
        }
    }

    /// Synced (cache) files whose path is inside file_cache_dir SHOULD be deleted.
    #[tokio::test]
    async fn test_cache_file_is_deleted_when_inside_cache_dir() {
        let tmp = tempfile::TempDir::new().unwrap();
        let cache_dir = tmp.path().join("file-cache");
        std::fs::create_dir_all(&cache_dir).unwrap();

        // Create a real file inside the cache dir to be deleted
        let cached_file = cache_dir.join("transfer-abc").join("photo.png");
        std::fs::create_dir_all(cached_file.parent().unwrap()).unwrap();
        std::fs::write(&cached_file, b"fake file data").unwrap();
        assert!(cached_file.exists());

        let uri_list = cached_file.to_string_lossy().to_string();
        let entry_id = EntryId::from("test-entry".to_string());

        let uc = make_test_use_case_with_uri_list(&uri_list, Some(cache_dir.clone()));
        uc.execute(&entry_id).await.unwrap();

        assert!(
            !cached_file.exists(),
            "Cached file inside cache_dir should have been deleted"
        );
        assert!(
            !cached_file.parent().unwrap().exists(),
            "Empty parent directory inside cache_dir should also be removed"
        );
    }

    /// Local (user-owned) files whose path is OUTSIDE file_cache_dir must NOT be deleted.
    #[tokio::test]
    async fn test_local_file_is_not_deleted_when_outside_cache_dir() {
        let tmp = tempfile::TempDir::new().unwrap();
        let cache_dir = tmp.path().join("file-cache");
        std::fs::create_dir_all(&cache_dir).unwrap();

        // Create a user-owned file OUTSIDE the cache dir
        let user_file = tmp.path().join("documents").join("report.pdf");
        std::fs::create_dir_all(user_file.parent().unwrap()).unwrap();
        std::fs::write(&user_file, b"important document").unwrap();
        assert!(user_file.exists());

        let uri_list = user_file.to_string_lossy().to_string();
        let entry_id = EntryId::from("test-entry".to_string());

        let uc = make_test_use_case_with_uri_list(&uri_list, Some(cache_dir.clone()));
        uc.execute(&entry_id).await.unwrap();

        assert!(
            user_file.exists(),
            "User-owned file outside cache_dir must NOT be deleted"
        );
    }

    /// When no file_cache_dir is configured, no files should be deleted (safe default).
    #[tokio::test]
    async fn test_no_files_deleted_when_cache_dir_not_configured() {
        let tmp = tempfile::TempDir::new().unwrap();

        let some_file = tmp.path().join("file.txt");
        std::fs::write(&some_file, b"data").unwrap();
        assert!(some_file.exists());

        let uri_list = some_file.to_string_lossy().to_string();
        let entry_id = EntryId::from("test-entry".to_string());

        // No file_cache_dir provided
        let uc = make_test_use_case_with_uri_list(&uri_list, None);
        uc.execute(&entry_id).await.unwrap();

        assert!(
            some_file.exists(),
            "File must not be deleted when no cache_dir is configured"
        );
    }

    /// Synced files referenced via file:// URI scheme inside cache_dir should also be deleted.
    #[tokio::test]
    async fn test_cache_file_uri_scheme_is_deleted() {
        let tmp = tempfile::TempDir::new().unwrap();
        let cache_dir = tmp.path().join("file-cache");
        std::fs::create_dir_all(&cache_dir).unwrap();

        let cached_file = cache_dir.join("xfer-1").join("image.jpg");
        std::fs::create_dir_all(cached_file.parent().unwrap()).unwrap();
        std::fs::write(&cached_file, b"image bytes").unwrap();
        assert!(cached_file.exists());

        // Use file:// URI format (legacy format used by older synced entries)
        let uri = url::Url::from_file_path(&cached_file).unwrap().to_string();
        let entry_id = EntryId::from("test-entry".to_string());

        let uc = make_test_use_case_with_uri_list(&uri, Some(cache_dir.clone()));
        uc.execute(&entry_id).await.unwrap();

        assert!(
            !cached_file.exists(),
            "Cached file referenced via file:// URI inside cache_dir should have been deleted"
        );
        assert!(
            !cached_file.parent().unwrap().exists(),
            "Empty parent directory inside cache_dir should also be removed (file:// URI case)"
        );
    }

    fn make_test_entry_and_use_case() -> (EntryId, DeleteClipboardEntry) {
        let entry_id = EntryId::from("test-entry-search".to_string());
        let event_id = EventId::from("test-event-search".to_string());
        let entry = build_entry(entry_id.clone(), event_id.clone());

        let mut entry_repo = MockClipboardEntryRepository::new();
        let entry_for_get = entry.clone();
        entry_repo
            .expect_get_entry()
            .returning(move |_| Ok(Some(entry_for_get.clone())));
        entry_repo.expect_delete_entry().returning(|_| Ok(()));

        let mut selection_repo = MockClipboardSelectionRepository::new();
        selection_repo
            .expect_delete_selection()
            .returning(|_| Ok(()));

        let mut event_writer = MockClipboardEventWriter::new();
        event_writer
            .expect_delete_event_and_representations()
            .returning(|_| Ok(()));

        let uc = DeleteClipboardEntry::from_ports(
            Arc::new(entry_repo),
            Arc::new(selection_repo),
            Arc::new(event_writer),
            Arc::new(make_default_representation_repo()),
        );
        (entry_id, uc)
    }

    #[tokio::test]
    async fn delete_with_search_index_calls_remove_entry() {
        let (entry_id, uc) = make_test_entry_and_use_case();

        let last_remove = std::sync::Arc::new(std::sync::Mutex::new(None::<EntryId>));
        let last_remove_for_closure = last_remove.clone();
        let mut search_index = MockSearchIndex::new();
        search_index.expect_remove_entry().returning(move |id| {
            *last_remove_for_closure.lock().unwrap() = Some(id.clone());
            Ok(())
        });

        let uc = uc.with_search_index(Arc::new(search_index));
        uc.execute(&entry_id).await.unwrap();

        let captured = last_remove.lock().unwrap().clone();
        assert_eq!(
            captured.as_ref(),
            Some(&entry_id),
            "SpySearchIndex should have captured the same EntryId"
        );
    }

    #[tokio::test]
    async fn delete_without_search_index_succeeds() {
        let (entry_id, uc) = make_test_entry_and_use_case();
        // No .with_search_index() call — backwards compatible
        let result = uc.execute(&entry_id).await;
        assert!(
            result.is_ok(),
            "Delete without search index should succeed: {:?}",
            result
        );
    }

    #[tokio::test]
    async fn delete_with_search_index_error_is_warn_and_continue() {
        use std::sync::atomic::AtomicBool;

        let entry_id = EntryId::from("test-entry-warn".to_string());
        let event_id = EventId::from("test-event-warn".to_string());
        let entry = build_entry(entry_id.clone(), event_id.clone());

        let delete_entry_called = Arc::new(AtomicBool::new(false));
        let delete_selection_called = Arc::new(AtomicBool::new(false));
        let delete_event_called = Arc::new(AtomicBool::new(false));

        let mut entry_repo = MockClipboardEntryRepository::new();
        let entry_for_get = entry.clone();
        entry_repo
            .expect_get_entry()
            .returning(move |_| Ok(Some(entry_for_get.clone())));
        let delete_entry_called_for_closure = delete_entry_called.clone();
        entry_repo.expect_delete_entry().returning(move |_| {
            delete_entry_called_for_closure.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        });

        let mut selection_repo = MockClipboardSelectionRepository::new();
        let delete_selection_called_for_closure = delete_selection_called.clone();
        selection_repo
            .expect_delete_selection()
            .returning(move |_| {
                delete_selection_called_for_closure
                    .store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            });

        let mut event_writer = MockClipboardEventWriter::new();
        let delete_event_called_for_closure = delete_event_called.clone();
        event_writer
            .expect_delete_event_and_representations()
            .returning(move |_| {
                delete_event_called_for_closure.store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            });

        let mut search_index = MockSearchIndex::new();
        search_index.expect_remove_entry().returning(|_| {
            Err(uc_core::search::SearchError::Internal(
                "simulated failure".into(),
            ))
        });

        let uc = DeleteClipboardEntry::from_ports(
            Arc::new(entry_repo),
            Arc::new(selection_repo),
            Arc::new(event_writer),
            Arc::new(make_default_representation_repo()),
        )
        .with_search_index(Arc::new(search_index));

        let result = uc.execute(&entry_id).await;

        assert!(
            result.is_ok(),
            "Delete should succeed even when search index remove_entry fails: {:?}",
            result
        );
        // The rest of the delete chain must still have run
        assert!(
            delete_selection_called.load(std::sync::atomic::Ordering::SeqCst),
            "selection_repo.delete_selection should have been called"
        );
        assert!(
            delete_entry_called.load(std::sync::atomic::Ordering::SeqCst),
            "entry_repo.delete_entry should have been called"
        );
        assert!(
            delete_event_called.load(std::sync::atomic::Ordering::SeqCst),
            "event_writer.delete_event_and_representations should have been called"
        );
    }

    #[tokio::test]
    async fn test_execute_propagates_repository_errors() {
        let delete_entry_called = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let delete_selection_called =
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let delete_event_called = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

        let entry_id = EntryId::from("test-entry".to_string());
        let event_id = EventId::from("test-event".to_string());
        let entry = build_entry(entry_id.clone(), event_id.clone());

        let mut entry_repo = MockClipboardEntryRepository::new();
        let entry_for_get = entry.clone();
        entry_repo
            .expect_get_entry()
            .returning(move |_| Ok(Some(entry_for_get.clone())));
        let delete_entry_called_for_closure = delete_entry_called.clone();
        entry_repo.expect_delete_entry().returning(move |_| {
            delete_entry_called_for_closure.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        });

        let mut selection_repo = MockClipboardSelectionRepository::new();
        let delete_selection_called_for_closure = delete_selection_called.clone();
        selection_repo
            .expect_delete_selection()
            .returning(move |_| {
                delete_selection_called_for_closure
                    .store(true, std::sync::atomic::Ordering::SeqCst);
                Err(anyhow::anyhow!("Mock delete_selection error"))
            });

        let mut event_writer = MockClipboardEventWriter::new();
        let delete_event_called_for_closure = delete_event_called.clone();
        event_writer
            .expect_delete_event_and_representations()
            .returning(move |_| {
                delete_event_called_for_closure.store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            });

        let use_case = DeleteClipboardEntry::from_ports(
            Arc::new(entry_repo),
            Arc::new(selection_repo),
            Arc::new(event_writer),
            Arc::new(make_default_representation_repo()),
        );

        let result = use_case.execute(&entry_id).await;

        assert!(result.is_err(), "Should return error when repo fails");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Failed to delete selection"),
            "Error should indicate which operation failed: {}",
            err
        );
    }
}
