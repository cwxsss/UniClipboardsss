use anyhow::Result;
use std::sync::Arc;
use uc_core::{
    ids::EntryId,
    ports::clipboard::ResolvedClipboardPayload,
    ports::{
        ClipboardEntryRepositoryPort, ClipboardPayloadResolverPort,
        ClipboardRepresentationRepositoryPort, ClipboardSelectionRepositoryPort,
    },
    BlobId,
};

/// Get clipboard entry resource metadata (blob reference only).
/// 获取剪贴板条目资源元信息（仅 blob 引用）。
pub struct GetEntryResourceUseCase {
    entry_repo: Arc<dyn ClipboardEntryRepositoryPort>,
    selection_repo: Arc<dyn ClipboardSelectionRepositoryPort>,
    representation_repo: Arc<dyn ClipboardRepresentationRepositoryPort>,
    payload_resolver: Arc<dyn ClipboardPayloadResolverPort>,
}

/// Resource metadata result from GetEntryResourceUseCase
/// GetEntryResourceUseCase 返回的资源元信息结果
#[derive(Debug, Clone, serde::Serialize)]
pub struct EntryResourceResult {
    pub entry_id: String,
    pub blob_id: Option<BlobId>,
    pub mime_type: Option<String>,
    pub size_bytes: i64,
    pub url: Option<String>,
    /// Inline data bytes when content is stored inline (small content).
    /// When present, consumers should use this directly instead of fetching via URL.
    pub inline_data: Option<Vec<u8>>,
}

impl GetEntryResourceUseCase {
    pub fn new(
        entry_repo: Arc<dyn ClipboardEntryRepositoryPort>,
        selection_repo: Arc<dyn ClipboardSelectionRepositoryPort>,
        representation_repo: Arc<dyn ClipboardRepresentationRepositoryPort>,
        payload_resolver: Arc<dyn ClipboardPayloadResolverPort>,
    ) -> Self {
        Self {
            entry_repo,
            selection_repo,
            representation_repo,
            payload_resolver,
        }
    }

    pub async fn execute(&self, entry_id: &EntryId) -> Result<EntryResourceResult> {
        let entry = self
            .entry_repo
            .get_entry(entry_id)
            .await?
            .ok_or(anyhow::anyhow!("Entry not found"))?;

        let selection = self
            .selection_repo
            .get_selection(entry_id)
            .await?
            .ok_or(anyhow::anyhow!("Selection not found"))?;

        let preview_rep = self
            .representation_repo
            .get_representation(&entry.event_id, &selection.selection.preview_rep_id)
            .await?
            .ok_or(anyhow::anyhow!("Preview representation not found"))?;

        // Use payload resolver to handle Staged/Processing states correctly
        // This will attempt to get bytes from cache/spool when blob is not yet materialized
        let payload = self.payload_resolver.resolve(&preview_rep).await?;

        match payload {
            ResolvedClipboardPayload::Inline { mime, bytes } => Ok(EntryResourceResult {
                entry_id: entry.entry_id.to_string(),
                blob_id: None,
                mime_type: Some(mime),
                size_bytes: preview_rep.size_bytes,
                url: None,
                inline_data: Some(bytes),
            }),
            ResolvedClipboardPayload::BlobRef { mime, blob_id } => {
                let blob_id_clone = blob_id.clone();
                Ok(EntryResourceResult {
                    entry_id: entry.entry_id.to_string(),
                    blob_id: Some(blob_id),
                    mime_type: Some(mime),
                    size_bytes: preview_rep.size_bytes,
                    url: Some(format!("/clipboard/blobs/{}", blob_id_clone)),
                    inline_data: None,
                })
            }
        }
    }
}
