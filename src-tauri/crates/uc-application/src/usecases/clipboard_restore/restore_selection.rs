//! Reconstruct a system clipboard state from a historical entry, restoring
//! the primary selected representation only.

use anyhow::{bail, Result};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, info};

use uc_core::{
    blob::ports::BlobReaderPort,
    clipboard::{
        ClipboardIntegrationMode, ObservedClipboardRepresentation,
        PersistedClipboardRepresentation, SystemClipboardSnapshot,
    },
    ids::{EntryId, EventId, RepresentationId},
    ports::{
        ClipboardEntryRepositoryPort, ClipboardRepresentationRepositoryPort,
        ClipboardSelectionRepositoryPort,
    },
};

use crate::clipboard_write::{ClipboardWriteCoordinator, ClipboardWriteIntent};

use super::file_snapshot::{build_file_snapshot, build_path_list};

pub(crate) struct RestoreClipboardSelectionUseCase {
    clipboard_repo: Arc<dyn ClipboardEntryRepositoryPort>,
    coordinator: Arc<ClipboardWriteCoordinator>,
    selection_repo: Arc<dyn ClipboardSelectionRepositoryPort>,
    representation_repo: Arc<dyn ClipboardRepresentationRepositoryPort>,
    blob_store: Arc<dyn BlobReaderPort>,
    mode: ClipboardIntegrationMode,
}

impl RestoreClipboardSelectionUseCase {
    pub(crate) fn new(
        clipboard_repo: Arc<dyn ClipboardEntryRepositoryPort>,
        coordinator: Arc<ClipboardWriteCoordinator>,
        selection_repo: Arc<dyn ClipboardSelectionRepositoryPort>,
        representation_repo: Arc<dyn ClipboardRepresentationRepositoryPort>,
        blob_store: Arc<dyn BlobReaderPort>,
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

    async fn build_snapshot(&self, entry_id: &EntryId) -> Result<SystemClipboardSnapshot> {
        debug!(entry_id = %entry_id, "restore.build_snapshot start");
        let entry = self
            .clipboard_repo
            .get_entry(entry_id)
            .await?
            .ok_or(anyhow::anyhow!("Entry not found"))?;

        let selection = self
            .selection_repo
            .get_selection(entry_id)
            .await?
            .ok_or(anyhow::anyhow!("Selection not found"))?;

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

        for path in &file_paths {
            if !path.exists() {
                bail!("File deleted: {}", path.display());
            }
        }

        let snapshot = build_file_snapshot(&build_path_list(&file_paths));

        info!(
            entry_id = %entry_id,
            file_count = file_paths.len(),
            "restore.build_file_snapshot: files validated and snapshot built"
        );

        Ok(snapshot)
    }

    pub(crate) async fn execute(&self, entry_id: &EntryId) -> Result<()> {
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
