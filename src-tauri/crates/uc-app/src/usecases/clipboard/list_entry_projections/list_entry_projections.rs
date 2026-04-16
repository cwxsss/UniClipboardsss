//! Use case for listing clipboard entry projections
//! 列出剪贴板条目投影的用例

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, warn};
use uc_core::clipboard::link_utils::{is_all_urls, is_single_url, parse_uri_list};
use uc_core::clipboard::PayloadAvailability;
use uc_core::network::protocol::MIME_IMAGE_PREFIX;
use uc_core::ports::{
    ClipboardEntryRepositoryPort, ClipboardRepresentationRepositoryPort,
    ClipboardSelectionRepositoryPort, FileTransferRepositoryPort, ThumbnailRepositoryPort,
};

/// DTO for clipboard entry projection (returned to command layer)
/// 剪贴板条目投影 DTO（返回给命令层）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntryProjectionDto {
    pub id: String,
    pub preview: String,
    pub has_detail: bool,
    pub size_bytes: i64,
    pub captured_at: i64,
    pub content_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail_url: Option<String>,
    // TODO: is_encrypted, is_favorited to be implemented later
    pub is_encrypted: bool,
    pub is_favorited: bool,
    pub updated_at: i64,
    pub active_time: i64,
    /// Aggregate file transfer status (String for serialization-friendly DTO).
    /// Maps from `TrackedFileTransferStatus` enum in the use case.
    /// None for non-file entries.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_transfer_status: Option<String>,
    /// Failure reason when `file_transfer_status` is `"failed"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_transfer_reason: Option<String>,
    /// Transfer IDs belonging to this entry (empty for non-file entries).
    /// Not serialized to JSON — internal field only.
    #[serde(skip)]
    pub file_transfer_ids: Vec<String>,
    /// Parsed link URLs when content is a link type.
    /// Built from full representation data (not truncated preview).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_urls: Option<Vec<String>>,
    /// Extracted domains for link entries (populated at command layer).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_domains: Option<Vec<String>>,
    /// Per-file sizes in bytes for file (uri-list) entries.
    /// Each element corresponds to a file URI parsed from inline_data.
    /// -1 means the file could not be stat'd (missing or non-local).
    /// None for non-file entries.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_sizes: Option<Vec<i64>>,
    /// Original image width in pixels (0 or None for non-image entries).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_width: Option<i32>,
    /// Original image height in pixels (0 or None for non-image entries).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_height: Option<i32>,
}

/// Error type for list projections use case
#[derive(Debug, thiserror::Error)]
pub enum ListProjectionsError {
    #[error("Invalid limit: {0}")]
    InvalidLimit(String),

    #[error("Repository error: {0}")]
    RepositoryError(String),

    #[error("Selection not found for entry {0}")]
    SelectionNotFound(String),

    #[error("Representation not found: {0}")]
    RepresentationNotFound(String),
}

/// Use case for listing clipboard entry projections
pub struct ListClipboardEntryProjections {
    entry_repo: Arc<dyn ClipboardEntryRepositoryPort>,
    selection_repo: Arc<dyn ClipboardSelectionRepositoryPort>,
    representation_repo: Arc<dyn ClipboardRepresentationRepositoryPort>,
    thumbnail_repo: Arc<dyn ThumbnailRepositoryPort>,
    file_transfer_repo: Arc<dyn FileTransferRepositoryPort>,
    max_limit: usize,
}

/// Detect link URLs from full representation content.
///
/// Returns `Some(urls)` when the content contains web links (http/https).
/// `text/uri-list` entries with only `file://` URIs return `None` — those are
/// file entries, not link entries.
/// Uses the full inline_data rather than truncated preview text.
fn detect_link_urls(content_type: &str, inline_data: Option<&[u8]>) -> Option<Vec<String>> {
    let full_text = inline_data.and_then(|d| std::str::from_utf8(d).ok())?;
    let ct = content_type.to_ascii_lowercase();

    if ct.starts_with("text/uri-list") {
        // Filter out file:// URIs — those represent copied files, not web links.
        let urls: Vec<String> = parse_uri_list(full_text)
            .into_iter()
            .filter(|u| !u.starts_with("file://"))
            .collect();
        if urls.is_empty() {
            None
        } else {
            Some(urls)
        }
    } else if ct.starts_with("text/plain") {
        if is_all_urls(full_text) {
            let urls: Vec<String> = full_text
                .lines()
                .map(|l| l.trim())
                .filter(|l| !l.is_empty())
                .map(|l| l.to_string())
                .collect();
            Some(urls)
        } else if is_single_url(full_text) {
            Some(vec![full_text.trim().to_string()])
        } else {
            None
        }
    } else {
        None
    }
}

/// Compute per-file sizes from a `text/uri-list` inline payload.
///
/// For each `file://` URI, stats the local file and returns its size in bytes.
/// Returns `-1` for URIs that are not `file://` or where the file cannot be found.
fn compute_file_sizes(inline_data: &[u8]) -> Vec<i64> {
    let text = match std::str::from_utf8(inline_data) {
        Ok(t) => t,
        Err(_) => return vec![],
    };
    parse_uri_list(text)
        .iter()
        .map(|uri| match url::Url::parse(uri) {
            Ok(parsed) if parsed.scheme() == "file" => match parsed.to_file_path() {
                Ok(path) => match std::fs::metadata(&path) {
                    Ok(meta) => meta.len() as i64,
                    Err(_) => -1,
                },
                Err(_) => -1,
            },
            _ => -1,
        })
        .collect()
}

impl ListClipboardEntryProjections {
    /// Create a new use case instance
    pub fn new(
        entry_repo: Arc<dyn ClipboardEntryRepositoryPort>,
        selection_repo: Arc<dyn ClipboardSelectionRepositoryPort>,
        representation_repo: Arc<dyn ClipboardRepresentationRepositoryPort>,
        thumbnail_repo: Arc<dyn ThumbnailRepositoryPort>,
        file_transfer_repo: Arc<dyn FileTransferRepositoryPort>,
    ) -> Self {
        Self {
            entry_repo,
            selection_repo,
            representation_repo,
            thumbnail_repo,
            file_transfer_repo,
            max_limit: 1000,
        }
    }

    /// Execute the use case for a single entry by ID
    pub async fn execute_single(
        &self,
        entry_id: &str,
    ) -> Result<Option<EntryProjectionDto>, ListProjectionsError> {
        use uc_core::ids::EntryId;

        let id = EntryId::from(entry_id);
        let entry = self
            .entry_repo
            .get_entry(&id)
            .await
            .map_err(|e| ListProjectionsError::RepositoryError(e.to_string()))?;

        let entry = match entry {
            Some(e) => e,
            None => return Ok(None),
        };

        let entry_id_str = entry.entry_id.inner().clone();
        let event_id_str = entry.event_id.inner().clone();
        let captured_at = entry.created_at_ms;
        let active_time = entry.active_time_ms;

        // Get selection for this entry
        let selection = match self.selection_repo.get_selection(&entry.entry_id).await {
            Ok(Some(selection)) => selection,
            Ok(None) => {
                warn!(
                    entry_id = %entry_id_str,
                    "Entry has no selection"
                );
                return Ok(None);
            }
            Err(e) => {
                return Err(ListProjectionsError::RepositoryError(format!(
                    "Selection lookup failed for {}: {}",
                    entry_id_str, e
                )));
            }
        };

        // Get preview representation
        let preview_rep_id = selection.selection.preview_rep_id.inner().clone();
        let representation = match self
            .representation_repo
            .get_representation(&entry.event_id, &selection.selection.preview_rep_id)
            .await
        {
            Ok(Some(rep)) => rep,
            Ok(None) => {
                warn!(
                    event_id = %event_id_str,
                    preview_rep_id = %preview_rep_id,
                    "Preview representation missing"
                );
                return Ok(None);
            }
            Err(e) => {
                return Err(ListProjectionsError::RepositoryError(format!(
                    "Representation lookup failed for {}: {}",
                    event_id_str, e
                )));
            }
        };

        let is_image = representation
            .mime_type
            .as_ref()
            .map(|mt| {
                mt.as_str()
                    .to_ascii_lowercase()
                    .starts_with(MIME_IMAGE_PREFIX)
            })
            .unwrap_or(false);

        let preview = if let Some(data) = representation.inline_data.as_ref() {
            String::from_utf8_lossy(data).trim().to_string()
        } else if is_image {
            format!("Image ({} bytes)", representation.size_bytes)
        } else {
            entry
                .title
                .as_ref()
                .map(|title| title.trim().to_string())
                .filter(|title| !title.is_empty())
                .unwrap_or_else(|| {
                    "Text content (full payload in background processing)".to_string()
                })
        };

        let content_type = representation
            .mime_type
            .as_ref()
            .map(|mt| mt.as_str().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let (thumbnail_url, image_width, image_height) = if is_image {
            match self
                .thumbnail_repo
                .get_by_representation_id(&selection.selection.preview_rep_id)
                .await
            {
                Ok(Some(metadata)) => (
                    Some(format!("/clipboard/thumbnails/{}", preview_rep_id)),
                    Some(metadata.original_width),
                    Some(metadata.original_height),
                ),
                Ok(None) => (None, None, None),
                Err(err) => {
                    tracing::error!(
                        error = %err,
                        entry_id = %entry_id_str,
                        "Failed to fetch thumbnail metadata"
                    );
                    (None, None, None)
                }
            }
        } else {
            (None, None, None)
        };

        let is_uri_list = content_type
            .to_ascii_lowercase()
            .starts_with("text/uri-list");
        let link_urls = detect_link_urls(&content_type, representation.inline_data.as_deref());

        let file_sizes = if is_uri_list {
            representation
                .inline_data
                .as_deref()
                .map(compute_file_sizes)
        } else {
            None
        };

        let has_detail = representation.blob_id.is_some()
            || matches!(
                representation.payload_state(),
                PayloadAvailability::Staged | PayloadAvailability::Processing
            );

        // Query aggregate file transfer status for this entry.
        let (file_transfer_status, file_transfer_reason, file_transfer_ids) = match self
            .file_transfer_repo
            .get_entry_transfer_summary(&entry_id_str)
            .await
        {
            Ok(Some(summary)) => (
                Some(summary.aggregate_status.as_str().to_string()),
                summary.failure_reason,
                summary.transfer_ids,
            ),
            Ok(None) => (None, None, vec![]),
            Err(e) => {
                warn!(
                    entry_id = %entry_id_str,
                    error = %e,
                    "Failed to query file transfer summary for entry"
                );
                (None, None, vec![])
            }
        };

        Ok(Some(EntryProjectionDto {
            id: entry_id_str,
            preview,
            has_detail,
            size_bytes: representation.size_bytes,
            captured_at,
            content_type,
            thumbnail_url,
            is_encrypted: false,
            is_favorited: false,
            updated_at: captured_at,
            active_time,
            file_transfer_status,
            file_transfer_reason,
            file_transfer_ids,
            link_urls,
            link_domains: None,
            file_sizes,
            image_width,
            image_height,
        }))
    }

    /// Execute the use case
    pub async fn execute(
        &self,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<EntryProjectionDto>, ListProjectionsError> {
        // Validate limit
        if limit == 0 {
            return Err(ListProjectionsError::InvalidLimit(format!(
                "Must be at least 1, got {}",
                limit
            )));
        }

        if limit > self.max_limit {
            return Err(ListProjectionsError::InvalidLimit(format!(
                "Must be at most {}, got {}",
                self.max_limit, limit
            )));
        }

        // Query entries from repository
        let entries = self
            .entry_repo
            .list_entries(limit, offset)
            .await
            .map_err(|e| ListProjectionsError::RepositoryError(e.to_string()))?;

        let mut projections = Vec::with_capacity(entries.len());

        for entry in entries {
            let entry_id_str = entry.entry_id.inner().clone();
            let event_id_str = entry.event_id.inner().clone();
            let captured_at = entry.created_at_ms;
            let active_time = entry.active_time_ms;

            // Get selection for this entry
            let selection = match self.selection_repo.get_selection(&entry.entry_id).await {
                Ok(Some(selection)) => selection,
                Ok(None) => {
                    warn!(
                        entry_id = %entry_id_str,
                        "Skipping entry without selection while listing projections"
                    );
                    continue;
                }
                Err(e) => {
                    warn!(
                        entry_id = %entry_id_str,
                        error = %e,
                        "Skipping entry due to selection lookup failure"
                    );
                    continue;
                }
            };

            // Get preview representation
            let preview_rep_id = selection.selection.preview_rep_id.inner().clone();
            let representation = match self
                .representation_repo
                .get_representation(&entry.event_id, &selection.selection.preview_rep_id)
                .await
            {
                Ok(Some(rep)) => rep,
                Ok(None) => {
                    warn!(
                        event_id = %event_id_str,
                        preview_rep_id = %preview_rep_id,
                        "Skipping entry because preview representation is missing"
                    );
                    continue;
                }
                Err(e) => {
                    warn!(
                        event_id = %event_id_str,
                        preview_rep_id = %preview_rep_id,
                        error = %e,
                        "Skipping entry due to preview representation lookup failure"
                    );
                    continue;
                }
            };

            let is_image = representation
                .mime_type
                .as_ref()
                .map(|mt| {
                    mt.as_str()
                        .to_ascii_lowercase()
                        .starts_with(MIME_IMAGE_PREFIX)
                })
                .unwrap_or(false);

            let preview = if let Some(data) = representation.inline_data.as_ref() {
                String::from_utf8_lossy(data).trim().to_string()
            } else if is_image {
                format!("Image ({} bytes)", representation.size_bytes)
            } else {
                entry
                    .title
                    .as_ref()
                    .map(|title| title.trim().to_string())
                    .filter(|title| !title.is_empty())
                    .unwrap_or_else(|| {
                        "Text content (full payload in background processing)".to_string()
                    })
            };

            // Get content type from representation
            let content_type = representation
                .mime_type
                .as_ref()
                .map(|mt| mt.as_str().to_string())
                .unwrap_or_else(|| "unknown".to_string());

            let (thumbnail_url, image_width, image_height) = if is_image {
                match self
                    .thumbnail_repo
                    .get_by_representation_id(&selection.selection.preview_rep_id)
                    .await
                {
                    Ok(Some(metadata)) => (
                        Some(format!("/clipboard/thumbnails/{}", preview_rep_id)),
                        Some(metadata.original_width),
                        Some(metadata.original_height),
                    ),
                    Ok(None) => (None, None, None),
                    Err(err) => {
                        tracing::error!(
                            error = %err,
                            entry_id = %entry_id_str,
                            "Failed to fetch thumbnail metadata"
                        );
                        (None, None, None)
                    }
                }
            } else {
                (None, None, None)
            };

            let is_uri_list = content_type
                .to_ascii_lowercase()
                .starts_with("text/uri-list");
            let link_urls = detect_link_urls(&content_type, representation.inline_data.as_deref());

            let file_sizes = if is_uri_list {
                representation
                    .inline_data
                    .as_deref()
                    .map(compute_file_sizes)
            } else {
                None
            };

            // has_detail controls whether frontend should try fetching full content.
            // For staged/processing payloads, full content may become available via blob shortly.
            let has_detail = representation.blob_id.is_some()
                || matches!(
                    representation.payload_state(),
                    PayloadAvailability::Staged | PayloadAvailability::Processing
                );

            // Query aggregate file transfer status for this entry.
            let (file_transfer_status, file_transfer_reason, file_transfer_ids) = match self
                .file_transfer_repo
                .get_entry_transfer_summary(&entry_id_str)
                .await
            {
                Ok(Some(summary)) => (
                    Some(summary.aggregate_status.as_str().to_string()),
                    summary.failure_reason,
                    summary.transfer_ids,
                ),
                Ok(None) => (None, None, vec![]),
                Err(e) => {
                    warn!(
                        entry_id = %entry_id_str,
                        error = %e,
                        "Failed to query file transfer summary for entry in list"
                    );
                    (None, None, vec![])
                }
            };

            projections.push(EntryProjectionDto {
                id: entry_id_str,
                preview,
                has_detail,
                size_bytes: representation.size_bytes,
                captured_at,
                content_type,
                thumbnail_url,
                is_encrypted: false, // TODO: implement later
                is_favorited: false, // TODO: implement later
                updated_at: captured_at,
                active_time,
                file_transfer_status,
                file_transfer_reason,
                file_transfer_ids,
                link_urls,
                link_domains: None,
                file_sizes,
                image_width,
                image_height,
            });
        }

        debug!(
            limit,
            offset,
            projections = projections.len(),
            "Listed clipboard entry projections"
        );

        Ok(projections)
    }
}
