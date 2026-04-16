use anyhow::Result;
use std::sync::Arc;

use uc_core::{
    clipboard::MimeType,
    ports::{BlobStorePort, ClipboardRepresentationRepositoryPort},
    BlobId,
};

/// Resolve blob resource by blob id.
/// 通过 blob id 解析资源内容。
pub struct ResolveBlobResourceUseCase {
    representation_repo: Arc<dyn ClipboardRepresentationRepositoryPort>,
    blob_store: Arc<dyn BlobStorePort>,
}

/// Blob resource payload and metadata.
/// Blob 资源内容与元信息。
#[derive(Debug, Clone)]
pub struct BlobResourceResult {
    pub blob_id: BlobId,
    pub mime_type: Option<String>,
    pub bytes: Vec<u8>,
}

impl ResolveBlobResourceUseCase {
    pub fn new(
        representation_repo: Arc<dyn ClipboardRepresentationRepositoryPort>,
        blob_store: Arc<dyn BlobStorePort>,
    ) -> Self {
        Self {
            representation_repo,
            blob_store,
        }
    }

    pub async fn execute(&self, blob_id: &BlobId) -> Result<BlobResourceResult> {
        let representation = self
            .representation_repo
            .get_representation_by_blob_id(blob_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Representation not found"))?;

        if let Some(rep_blob_id) = representation.blob_id.as_ref() {
            if rep_blob_id != blob_id {
                return Err(anyhow::anyhow!("Representation blob_id mismatch"));
            }
        }

        let mime_type = representation
            .mime_type
            .as_ref()
            .map(MimeType::as_str)
            .map(String::from);

        let bytes = self.blob_store.get(blob_id).await?;

        Ok(BlobResourceResult {
            blob_id: blob_id.clone(),
            mime_type,
            bytes,
        })
    }
}
