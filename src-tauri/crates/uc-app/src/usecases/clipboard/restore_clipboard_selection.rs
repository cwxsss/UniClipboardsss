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
        ClipboardEntryRepositoryPort, ClipboardRepresentationRepositoryPort,
        ClipboardSelectionRepositoryPort,
    },
};
use uc_infra::blob::BlobStorePort;

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
    /// use uc_core::ports::{ClipboardEntryRepositoryPort, ClipboardRepresentationRepositoryPort, ClipboardSelectionRepositoryPort};
    /// use uc_infra::blob::BlobStorePort;
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
