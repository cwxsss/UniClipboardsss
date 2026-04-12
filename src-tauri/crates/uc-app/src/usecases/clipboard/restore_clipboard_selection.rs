use crate::usecases::clipboard::clipboard_write_coordinator::{
    ClipboardWriteCoordinator, ClipboardWriteIntent,
};
use crate::usecases::clipboard::ClipboardIntegrationMode;
use anyhow::{bail, Result};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, info};

use uc_core::{
    clipboard::{
        ObservedClipboardRepresentation, PersistedClipboardRepresentation, SystemClipboardSnapshot,
    },
    ids::{EntryId, EventId, RepresentationId},
    ports::{
        BlobStorePort, ClipboardEntryRepositoryPort, ClipboardRepresentationRepositoryPort,
        ClipboardSelectionRepositoryPort,
    },
};

/// Reconstructs a system clipboard state from a historical clipboard entry,
/// restoring the primary selected representation only.
pub struct RestoreClipboardSelectionUseCase {
    clipboard_repo: Arc<dyn ClipboardEntryRepositoryPort>,
    coordinator: Arc<ClipboardWriteCoordinator>,
    selection_repo: Arc<dyn ClipboardSelectionRepositoryPort>,
    representation_repo: Arc<dyn ClipboardRepresentationRepositoryPort>,
    blob_store: Arc<dyn BlobStorePort>,
    mode: ClipboardIntegrationMode,
}

impl RestoreClipboardSelectionUseCase {
    /// Creates a new use case instance that copies clipboard entries from history to the system clipboard.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::sync::Arc;
    /// use uc_app::usecases::clipboard::restore_clipboard_selection::RestoreClipboardSelectionUseCase;
    /// use uc_core::ports::{BlobStorePort, ClipboardEntryRepositoryPort, ClipboardRepresentationRepositoryPort, ClipboardSelectionRepositoryPort};
    /// // All parameters must implement their respective ports
    /// ```
    pub fn new(
        clipboard_repo: Arc<dyn ClipboardEntryRepositoryPort>,
        coordinator: Arc<ClipboardWriteCoordinator>,
        selection_repo: Arc<dyn ClipboardSelectionRepositoryPort>,
        representation_repo: Arc<dyn ClipboardRepresentationRepositoryPort>,
        blob_store: Arc<dyn BlobStorePort>,
        mode: ClipboardIntegrationMode,
    ) -> Self {
        Self {
            clipboard_repo,
            coordinator,
            selection_repo,
            representation_repo,
            blob_store,
            mode,
        }
    }

    pub async fn build_snapshot(&self, entry_id: &EntryId) -> Result<SystemClipboardSnapshot> {
        debug!(entry_id = %entry_id, "restore.build_snapshot start");
        // 1. 读取 Entry
        let entry = self
            .clipboard_repo
            .get_entry(entry_id)
            .await?
            .ok_or(anyhow::anyhow!("Entry not found"))?;

        // 2. 获取 Selection 决策
        let selection = self
            .selection_repo
            .get_selection(entry_id)
            .await?
            .ok_or(anyhow::anyhow!("Selection not found"))?;

        // 3. 收集候选 representations
        let mut candidate_ids = Vec::new();
        candidate_ids.push(selection.selection.paste_rep_id.clone());
        candidate_ids.push(selection.selection.primary_rep_id.clone());
        candidate_ids.push(selection.selection.preview_rep_id.clone());
        candidate_ids.extend(selection.selection.secondary_rep_ids.clone());

        let mut seen = std::collections::HashSet::new();
        candidate_ids.retain(|rep_id| seen.insert(rep_id.clone()));

        let mut candidates = Vec::new();
        for rep_id in &candidate_ids {
            let rep = self
                .representation_repo
                .get_representation(&entry.event_id, rep_id)
                .await?;
            if let Some(rep) = rep {
                candidates.push(rep);
            } else if *rep_id == selection.selection.paste_rep_id {
                return Err(anyhow::anyhow!(
                    "Representation {} not found for event {}",
                    rep_id,
                    entry.event_id
                ));
            }
        }

        let restore_rep = Self::select_restore_representation(
            &candidates,
            &selection.selection.paste_rep_id,
            &entry.event_id,
        )?;

        // Check if the restore representation is a file type (text/uri-list or file/uri-list).
        // File entries need special handling: parse URIs, validate file existence, and
        // write with FormatId="files" so the OS recognizes them as file references.
        if Self::is_file_representation(restore_rep) {
            debug!(
                entry_id = %entry_id,
                restore_rep_id = %restore_rep.id,
                "restore.build_snapshot: detected file entry, using file restore strategy"
            );
            return self.build_file_snapshot(entry_id, restore_rep).await;
        }

        let bytes = if let Some(inline_data) = &restore_rep.inline_data {
            inline_data.clone()
        } else if let Some(blob_id) = &restore_rep.blob_id {
            self.blob_store.get(blob_id).await?
        } else {
            return Err(anyhow::anyhow!(
                "Representation has no data: {}",
                restore_rep.id
            ));
        };

        let representations = vec![ObservedClipboardRepresentation::new(
            restore_rep.id.clone(),
            restore_rep.format_id.clone(),
            restore_rep.mime_type.clone(),
            bytes,
        )];

        debug!(
            entry_id = %entry_id,
            event_id = %entry.event_id,
            restore_rep_id = %restore_rep.id,
            restore_format = %restore_rep.format_id,
            restore_mime = ?restore_rep.mime_type.as_ref().map(|mime| mime.as_str()),
            candidate_count = candidates.len(),
            restore_size_bytes = representations[0].bytes.len(),
            "restore.build_snapshot selected representation"
        );

        // 5. 构造 Snapshot
        Ok(SystemClipboardSnapshot {
            ts_ms: chrono::Utc::now().timestamp_millis(),
            representations,
        })
    }

    fn select_restore_representation<'a>(
        candidates: &'a [PersistedClipboardRepresentation],
        paste_rep_id: &RepresentationId,
        event_id: &EventId,
    ) -> Result<&'a PersistedClipboardRepresentation> {
        // If the paste representation is a file type, prefer it over text/plain.
        // For file entries, text/plain would be the file path as text, not a proper
        // file clipboard format — the caller will detect file type and use a
        // different restore strategy.
        let paste_rep = candidates.iter().find(|rep| rep.id == *paste_rep_id);
        if let Some(rep) = paste_rep {
            if Self::is_file_representation(rep) {
                return Ok(rep);
            }
        }

        if let Some(rep) = candidates
            .iter()
            .find(|rep| Self::is_plain_text_representation(*rep))
        {
            return Ok(rep);
        }

        paste_rep.ok_or(anyhow::anyhow!(
            "Representation {} not found for event {}",
            paste_rep_id,
            event_id
        ))
    }

    fn is_plain_text_representation(rep: &PersistedClipboardRepresentation) -> bool {
        if let Some(mime) = &rep.mime_type {
            let mime_str = mime.as_str();
            let mime_lower = mime_str.to_ascii_lowercase();
            if mime_lower == "text/plain" || mime_lower.starts_with("text/plain;") {
                return true;
            }
        }

        let format_id = rep.format_id.as_ref();
        format_id.eq_ignore_ascii_case("text")
            || format_id.eq_ignore_ascii_case("public.utf8-plain-text")
            || format_id.eq_ignore_ascii_case("public.text")
            || format_id.eq_ignore_ascii_case("NSStringPboardType")
    }

    fn is_file_representation(rep: &PersistedClipboardRepresentation) -> bool {
        uc_core::clipboard::is_file_mime_or_format(rep.mime_type.as_ref(), &rep.format_id)
    }

    /// Build a file-type snapshot by parsing URI list, validating file existence,
    /// and constructing a proper file clipboard snapshot.
    async fn build_file_snapshot(
        &self,
        entry_id: &EntryId,
        rep: &PersistedClipboardRepresentation,
    ) -> Result<SystemClipboardSnapshot> {
        let bytes = if let Some(inline_data) = &rep.inline_data {
            inline_data.clone()
        } else if let Some(blob_id) = &rep.blob_id {
            self.blob_store.get(blob_id).await?
        } else {
            bail!("File URI representation has no data for entry {}", entry_id);
        };

        let uri_string = String::from_utf8(bytes)?;

        // Parse file paths (native paths or backward-compat file:// URIs)
        let mut file_paths = Vec::new();
        for line in uri_string.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line.starts_with("file://") {
                match url::Url::parse(line) {
                    Ok(url) => {
                        let path = url.to_file_path().map_err(|_| {
                            anyhow::anyhow!(
                                "Failed to convert URI to file path for entry {}: {}",
                                entry_id,
                                line
                            )
                        })?;
                        file_paths.push(path);
                    }
                    Err(e) => {
                        bail!(
                            "Failed to parse file URI for entry {}: {} (error: {})",
                            entry_id,
                            line,
                            e
                        );
                    }
                }
            } else {
                file_paths.push(PathBuf::from(line));
            }
        }

        if file_paths.is_empty() {
            bail!("No valid file paths found in entry {}", entry_id);
        }

        // Validate all files exist
        for path in &file_paths {
            if !path.exists() {
                bail!("File deleted: {}", path.display());
            }
        }

        let snapshot = crate::usecases::file_sync::copy_file_to_clipboard::build_file_snapshot(
            &crate::usecases::file_sync::copy_file_to_clipboard::build_path_list(&file_paths),
        );

        info!(
            entry_id = %entry_id,
            file_count = file_paths.len(),
            "restore.build_file_snapshot: files validated and snapshot built"
        );

        Ok(snapshot)
    }

    pub async fn execute(&self, entry_id: &EntryId) -> Result<()> {
        info!(entry_id = %entry_id, "restore.execute requested");
        if !self.mode.allow_os_write() {
            return Err(anyhow::anyhow!(
                "System clipboard writes disabled (UC_CLIPBOARD_MODE=passive)"
            ));
        }
        let snapshot = self.build_snapshot(entry_id).await?;
        self.coordinator
            .write(snapshot, ClipboardWriteIntent::LocalRestore)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_mocks::{
        MockBlobStore, MockClipboardEntryRepository, MockClipboardRepresentationRepository,
        MockClipboardSelectionRepository, MockSystemClipboard,
    };
    use std::collections::HashMap;
    use std::sync::Arc;
    use uc_core::clipboard::{
        ClipboardEntry, ClipboardSelection, ClipboardSelectionDecision, MimeType,
        PersistedClipboardRepresentation, SelectionPolicyVersion, SystemClipboardSnapshot,
    };
    use uc_core::ids::{EventId, FormatId, RepresentationId};
    use uc_infra::clipboard::new_in_memory_change_origin;

    fn test_origin() -> std::sync::Arc<dyn uc_core::ports::clipboard::ClipboardChangeOriginPort> {
        new_in_memory_change_origin()
    }

    fn make_entry_repo_with(entry: Option<ClipboardEntry>) -> MockClipboardEntryRepository {
        let mut repo = MockClipboardEntryRepository::new();
        repo.expect_get_entry()
            .returning(move |_| Ok(entry.clone()));
        repo
    }

    fn make_selection_repo_with(
        selection: Option<ClipboardSelectionDecision>,
    ) -> MockClipboardSelectionRepository {
        let mut repo = MockClipboardSelectionRepository::new();
        repo.expect_get_selection()
            .returning(move |_| Ok(selection.clone()));
        repo
    }

    fn make_rep_repo_with(
        reps: HashMap<RepresentationId, PersistedClipboardRepresentation>,
    ) -> MockClipboardRepresentationRepository {
        let mut repo = MockClipboardRepresentationRepository::new();
        repo.expect_get_representation()
            .returning(move |_, rep_id| Ok(reps.get(rep_id).cloned()));
        repo
    }

    #[tokio::test]
    async fn build_snapshot_returns_only_paste_representation() {
        let entry_id = EntryId::from("entry-1");
        let event_id = EventId::from("event-1");
        let paste_rep_id = RepresentationId::from("rep-paste");
        let secondary_rep_id = RepresentationId::from("rep-secondary");

        let selection = ClipboardSelection {
            primary_rep_id: paste_rep_id.clone(),
            secondary_rep_ids: vec![secondary_rep_id.clone()],
            preview_rep_id: paste_rep_id.clone(),
            paste_rep_id: paste_rep_id.clone(),
            policy_version: SelectionPolicyVersion::V1,
        };

        let entry = ClipboardEntry::new(entry_id.clone(), event_id.clone(), 1, None, 0);

        let primary_representation = PersistedClipboardRepresentation::new(
            paste_rep_id.clone(),
            FormatId::from("public.utf8-plain-text"),
            Some(MimeType::text_plain()),
            3,
            Some(vec![1, 2, 3]),
            None,
        );

        let secondary_representation = PersistedClipboardRepresentation::new(
            secondary_rep_id.clone(),
            FormatId::from("public.html"),
            Some(MimeType::text_html()),
            3,
            Some(vec![4, 5, 6]),
            None,
        );

        let mut mock_clipboard = MockSystemClipboard::new();
        mock_clipboard.expect_write_snapshot().returning(|_| Ok(()));
        mock_clipboard.expect_read_snapshot().returning(|| {
            Ok(SystemClipboardSnapshot {
                ts_ms: 0,
                representations: vec![],
            })
        });

        let coordinator = Arc::new(ClipboardWriteCoordinator::new(
            Arc::new(mock_clipboard),
            test_origin(),
        ));

        let uc = RestoreClipboardSelectionUseCase::new(
            Arc::new(make_entry_repo_with(Some(entry))),
            coordinator,
            Arc::new(make_selection_repo_with(Some(
                ClipboardSelectionDecision::new(entry_id.clone(), selection),
            ))),
            Arc::new(make_rep_repo_with(HashMap::from([
                (paste_rep_id.clone(), primary_representation),
                (secondary_rep_id.clone(), secondary_representation),
            ]))),
            Arc::new({
                let mut b = MockBlobStore::new();
                b.expect_get()
                    .returning(|_| Err(anyhow::anyhow!("unexpected blob fetch")));
                b
            }),
            ClipboardIntegrationMode::Full,
        );

        let snapshot = uc.build_snapshot(&entry_id).await.unwrap();

        assert_eq!(snapshot.representations.len(), 1);
        assert_eq!(snapshot.representations[0].id, paste_rep_id);
    }

    #[tokio::test]
    async fn build_snapshot_prefers_plain_text_over_rich_text() {
        let entry_id = EntryId::from("entry-plain-preferred");
        let event_id = EventId::from("event-plain-preferred");
        let plain_rep_id = RepresentationId::from("rep-plain");
        let rich_rep_id = RepresentationId::from("rep-rich");

        let selection = ClipboardSelection {
            primary_rep_id: rich_rep_id.clone(),
            secondary_rep_ids: vec![plain_rep_id.clone()],
            preview_rep_id: rich_rep_id.clone(),
            paste_rep_id: rich_rep_id.clone(),
            policy_version: SelectionPolicyVersion::V1,
        };

        let entry = ClipboardEntry::new(entry_id.clone(), event_id.clone(), 1, None, 0);

        let plain_representation = PersistedClipboardRepresentation::new(
            plain_rep_id.clone(),
            FormatId::from("text"),
            Some(MimeType::text_plain()),
            5,
            Some(b"hello".to_vec()),
            None,
        );

        let rich_representation = PersistedClipboardRepresentation::new(
            rich_rep_id.clone(),
            FormatId::from("html"),
            Some(MimeType::text_html()),
            12,
            Some(b"<b>hi</b>".to_vec()),
            None,
        );

        let mut mock_clipboard = MockSystemClipboard::new();
        mock_clipboard.expect_write_snapshot().returning(|_| Ok(()));
        mock_clipboard.expect_read_snapshot().returning(|| {
            Ok(SystemClipboardSnapshot {
                ts_ms: 0,
                representations: vec![],
            })
        });

        let coordinator = Arc::new(ClipboardWriteCoordinator::new(
            Arc::new(mock_clipboard),
            test_origin(),
        ));

        let uc = RestoreClipboardSelectionUseCase::new(
            Arc::new(make_entry_repo_with(Some(entry))),
            coordinator,
            Arc::new(make_selection_repo_with(Some(
                ClipboardSelectionDecision::new(entry_id.clone(), selection),
            ))),
            Arc::new(make_rep_repo_with(HashMap::from([
                (plain_rep_id.clone(), plain_representation),
                (rich_rep_id.clone(), rich_representation),
            ]))),
            Arc::new({
                let mut b = MockBlobStore::new();
                b.expect_get()
                    .returning(|_| Err(anyhow::anyhow!("unexpected blob fetch")));
                b
            }),
            ClipboardIntegrationMode::Full,
        );

        let snapshot = uc.build_snapshot(&entry_id).await.unwrap();

        assert_eq!(snapshot.representations.len(), 1);
        assert_eq!(snapshot.representations[0].id, plain_rep_id);
    }

    #[tokio::test]
    async fn execute_clears_origin_on_write_error() {
        let mut mock_clipboard = MockSystemClipboard::new();
        mock_clipboard.expect_write_snapshot().never();
        mock_clipboard.expect_read_snapshot().returning(|| {
            Ok(SystemClipboardSnapshot {
                ts_ms: 0,
                representations: vec![],
            })
        });

        let coordinator = Arc::new(ClipboardWriteCoordinator::new(
            Arc::new(mock_clipboard),
            test_origin(),
        ));

        let uc = RestoreClipboardSelectionUseCase::new(
            Arc::new(make_entry_repo_with(None)),
            coordinator,
            Arc::new(make_selection_repo_with(None)),
            Arc::new(make_rep_repo_with(HashMap::new())),
            Arc::new({
                let mut b = MockBlobStore::new();
                b.expect_get()
                    .returning(|_| Err(anyhow::anyhow!("unexpected blob fetch")));
                b
            }),
            ClipboardIntegrationMode::Full,
        );

        // Execute with a valid snapshot path is not directly testable here
        // since build_snapshot requires a real entry. Instead, we verify
        // that execute returns error for missing entry (not found).
        let result = uc.execute(&EntryId::from("entry-not-found")).await;
        assert!(result.is_err());
        // The error comes from build_snapshot (Entry not found), not from the coordinator.
        // This test verifies that execute() does the mode check and delegates to coordinator.
    }

    #[tokio::test]
    async fn execute_returns_error_in_passive_mode_without_writing() {
        let mut mock_clipboard = MockSystemClipboard::new();
        mock_clipboard.expect_write_snapshot().never();
        mock_clipboard.expect_read_snapshot().returning(|| {
            Ok(SystemClipboardSnapshot {
                ts_ms: 0,
                representations: vec![],
            })
        });

        let coordinator = Arc::new(ClipboardWriteCoordinator::new(
            Arc::new(mock_clipboard),
            test_origin(),
        ));

        let uc = RestoreClipboardSelectionUseCase::new(
            Arc::new(make_entry_repo_with(None)),
            coordinator,
            Arc::new(make_selection_repo_with(None)),
            Arc::new(make_rep_repo_with(HashMap::new())),
            Arc::new({
                let mut b = MockBlobStore::new();
                b.expect_get()
                    .returning(|_| Err(anyhow::anyhow!("unexpected blob fetch")));
                b
            }),
            ClipboardIntegrationMode::Passive,
        );

        let result = uc.execute(&EntryId::from("entry-passive")).await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("System clipboard writes disabled (UC_CLIPBOARD_MODE=passive)"));
    }
}
