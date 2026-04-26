use std::path::PathBuf;
use std::sync::Arc;

use thiserror::Error;
use uc_core::blob::ports::BlobReaderPort;
use uc_core::clipboard::link_utils::extract_domain;
use uc_core::ids::EntryId;
use uc_core::ports::clipboard::{ClipboardPayloadResolverPort, ThumbnailRepositoryPort};
use uc_core::ports::search::search_index::SearchIndexPort;
use uc_core::ports::{
    ClipboardEntryRepositoryPort, ClipboardEventWriterPort, ClipboardRepresentationRepositoryPort,
    ClipboardSelectionRepositoryPort, FileTransferRepositoryPort,
};

use crate::usecases::clipboard_history::{
    compute_clipboard_stats, ClearClipboardHistoryUseCase, DeleteClipboardEntryUseCase,
    EntryDetailResult, EntryProjectionDto, EntryResourceResult, GetEntryDetailUseCase,
    GetEntryResourceUseCase, ListClipboardEntryProjectionsUseCase, ListProjectionsError,
    ToggleFavoriteClipboardEntryUseCase,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardListInput {
    pub limit: usize,
    pub offset: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryProjectionView {
    pub id: String,
    pub preview: String,
    pub has_detail: bool,
    pub size_bytes: i64,
    pub captured_at: i64,
    pub content_type: String,
    pub thumbnail_url: Option<String>,
    pub is_encrypted: bool,
    pub is_favorited: bool,
    pub updated_at: i64,
    pub active_time: i64,
    pub file_transfer_status: Option<String>,
    pub file_transfer_reason: Option<String>,
    pub link_urls: Option<Vec<String>>,
    pub link_domains: Option<Vec<String>>,
    pub file_sizes: Option<Vec<i64>>,
    pub image_width: Option<i32>,
    pub image_height: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryDetailView {
    pub id: String,
    pub content: String,
    pub size_bytes: i64,
    pub created_at_ms: i64,
    pub active_time_ms: i64,
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryResourceView {
    pub blob_id: Option<String>,
    pub mime_type: Option<String>,
    pub size_bytes: i64,
    pub url: Option<String>,
    pub inline_data: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardStatsView {
    pub total_items: i64,
    pub total_size: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClearHistoryResultView {
    pub deleted_count: u64,
    pub failed_entries: Vec<(String, String)>,
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum ClipboardHistoryError {
    #[error("entry not found")]
    NotFound,
    #[error("unsupported clipboard content")]
    UnsupportedContent,
    #[error("clipboard history operation failed: {0}")]
    Internal(String),
}

/// Dependency bundle for `ClipboardHistoryFacade`.
///
/// Composition roots (daemon, tauri runtime, tests) construct this from their
/// wiring deps and pass it once to `ClipboardHistoryFacade::new`. The facade
/// then owns the use cases internally; no per-call gateway adapter is needed.
pub struct ClipboardHistoryFacadeDeps {
    pub entry_repo: Arc<dyn ClipboardEntryRepositoryPort>,
    pub selection_repo: Arc<dyn ClipboardSelectionRepositoryPort>,
    pub representation_repo: Arc<dyn ClipboardRepresentationRepositoryPort>,
    pub event_writer: Arc<dyn ClipboardEventWriterPort>,
    pub payload_resolver: Arc<dyn ClipboardPayloadResolverPort>,
    pub blob_store: Arc<dyn BlobReaderPort>,
    pub thumbnail_repo: Arc<dyn ThumbnailRepositoryPort>,
    pub file_transfer_repo: Arc<dyn FileTransferRepositoryPort>,
    pub search_index: Option<Arc<dyn SearchIndexPort>>,
    pub file_cache_dir: Option<PathBuf>,
}

pub struct ClipboardHistoryFacade {
    list_uc: ListClipboardEntryProjectionsUseCase,
    detail_uc: GetEntryDetailUseCase,
    resource_uc: GetEntryResourceUseCase,
    toggle_favorite_uc: ToggleFavoriteClipboardEntryUseCase,
    delete_uc: DeleteClipboardEntryUseCase,
    clear_uc: ClearClipboardHistoryUseCase,
}

impl ClipboardHistoryFacade {
    pub fn new(deps: ClipboardHistoryFacadeDeps) -> Self {
        let ClipboardHistoryFacadeDeps {
            entry_repo,
            selection_repo,
            representation_repo,
            event_writer,
            payload_resolver,
            blob_store,
            thumbnail_repo,
            file_transfer_repo,
            search_index,
            file_cache_dir,
        } = deps;

        let list_uc = ListClipboardEntryProjectionsUseCase::new(
            entry_repo.clone(),
            selection_repo.clone(),
            representation_repo.clone(),
            thumbnail_repo,
            file_transfer_repo,
        );

        let detail_uc = GetEntryDetailUseCase::new(
            entry_repo.clone(),
            selection_repo.clone(),
            representation_repo.clone(),
            blob_store,
            payload_resolver.clone(),
        );

        let resource_uc = GetEntryResourceUseCase::new(
            entry_repo.clone(),
            selection_repo.clone(),
            representation_repo.clone(),
            payload_resolver,
        );

        let toggle_favorite_uc = ToggleFavoriteClipboardEntryUseCase::new(entry_repo.clone());

        let mut delete_uc = DeleteClipboardEntryUseCase::from_ports(
            entry_repo.clone(),
            selection_repo.clone(),
            event_writer.clone(),
            representation_repo.clone(),
        );
        if let Some(dir) = file_cache_dir.clone() {
            delete_uc = delete_uc.with_file_cache_dir(dir);
        }
        if let Some(idx) = search_index.clone() {
            delete_uc = delete_uc.with_search_index(idx);
        }

        let mut clear_uc = ClearClipboardHistoryUseCase::from_ports(
            entry_repo,
            selection_repo,
            event_writer,
            representation_repo,
        );
        if let Some(dir) = file_cache_dir {
            clear_uc = clear_uc.with_file_cache_dir(dir);
        }
        if let Some(idx) = search_index {
            clear_uc = clear_uc.with_search_index(idx);
        }

        Self {
            list_uc,
            detail_uc,
            resource_uc,
            toggle_favorite_uc,
            delete_uc,
            clear_uc,
        }
    }

    pub async fn list_entries(
        &self,
        input: ClipboardListInput,
    ) -> Result<Vec<EntryProjectionView>, ClipboardHistoryError> {
        let entries = self
            .list_uc
            .execute(input.limit, input.offset)
            .await
            .map_err(map_list_error)?;
        Ok(entries.into_iter().map(projection_to_view).collect())
    }

    pub async fn get_entry(
        &self,
        entry_id: &str,
    ) -> Result<EntryDetailView, ClipboardHistoryError> {
        let parsed_id = EntryId::from(entry_id);
        let detail = self
            .detail_uc
            .execute(&parsed_id)
            .await
            .map_err(map_history_error)?;
        Ok(detail_to_view(detail))
    }

    pub async fn delete_entry(&self, entry_id: &str) -> Result<(), ClipboardHistoryError> {
        let parsed_id = EntryId::from(entry_id);
        self.delete_uc
            .execute(&parsed_id)
            .await
            .map_err(map_history_error)
    }

    pub async fn toggle_favorite(
        &self,
        entry_id: &str,
        is_favorited: bool,
    ) -> Result<bool, ClipboardHistoryError> {
        let parsed_id = EntryId::from(entry_id);
        self.toggle_favorite_uc
            .execute(&parsed_id, is_favorited)
            .await
            .map_err(|err| ClipboardHistoryError::Internal(err.to_string()))
    }

    pub async fn stats(&self) -> Result<ClipboardStatsView, ClipboardHistoryError> {
        let entries = self
            .list_uc
            .execute(10_000, 0)
            .await
            .map_err(map_list_error)?;
        let stats = compute_clipboard_stats(&entries);
        Ok(ClipboardStatsView {
            total_items: stats.total_items,
            total_size: stats.total_size,
        })
    }

    pub async fn get_entry_resource(
        &self,
        entry_id: &str,
    ) -> Result<EntryResourceView, ClipboardHistoryError> {
        let parsed_id = EntryId::from(entry_id);
        let resource = self
            .resource_uc
            .execute(&parsed_id)
            .await
            .map_err(map_history_error)?;
        Ok(resource_to_view(resource))
    }

    pub async fn clear_history(&self) -> Result<ClearHistoryResultView, ClipboardHistoryError> {
        let result = self
            .clear_uc
            .execute()
            .await
            .map_err(|err| ClipboardHistoryError::Internal(err.to_string()))?;
        Ok(ClearHistoryResultView {
            deleted_count: result.deleted_count,
            failed_entries: result.failed_entries,
        })
    }
}

fn projection_to_view(entry: EntryProjectionDto) -> EntryProjectionView {
    let link_domains = entry
        .link_urls
        .as_ref()
        .map(|urls| urls.iter().filter_map(|url| extract_domain(url)).collect());
    EntryProjectionView {
        id: entry.id,
        preview: entry.preview,
        has_detail: entry.has_detail,
        size_bytes: entry.size_bytes,
        captured_at: entry.captured_at,
        content_type: entry.content_type,
        thumbnail_url: entry.thumbnail_url,
        is_encrypted: entry.is_encrypted,
        is_favorited: entry.is_favorited,
        updated_at: entry.updated_at,
        active_time: entry.active_time,
        file_transfer_status: entry.file_transfer_status,
        file_transfer_reason: entry.file_transfer_reason,
        link_urls: entry.link_urls,
        link_domains,
        file_sizes: entry.file_sizes,
        image_width: entry.image_width,
        image_height: entry.image_height,
    }
}

fn detail_to_view(detail: EntryDetailResult) -> EntryDetailView {
    EntryDetailView {
        id: detail.id,
        content: detail.content,
        size_bytes: detail.size_bytes,
        created_at_ms: detail.created_at_ms,
        active_time_ms: detail.active_time_ms,
        mime_type: detail.mime_type,
    }
}

fn resource_to_view(resource: EntryResourceResult) -> EntryResourceView {
    EntryResourceView {
        blob_id: resource.blob_id.map(|id| id.to_string()),
        mime_type: resource.mime_type,
        size_bytes: resource.size_bytes,
        url: resource.url,
        inline_data: resource.inline_data,
    }
}

fn map_history_error(err: anyhow::Error) -> ClipboardHistoryError {
    let message = err.to_string();
    let lower = message.to_lowercase();
    if lower.contains("not found") {
        ClipboardHistoryError::NotFound
    } else if lower.contains("not text content") || lower.contains("not text") {
        ClipboardHistoryError::UnsupportedContent
    } else {
        ClipboardHistoryError::Internal(message)
    }
}

fn map_list_error(err: ListProjectionsError) -> ClipboardHistoryError {
    ClipboardHistoryError::Internal(err.to_string())
}
