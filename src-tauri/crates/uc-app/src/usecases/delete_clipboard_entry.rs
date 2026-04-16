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
