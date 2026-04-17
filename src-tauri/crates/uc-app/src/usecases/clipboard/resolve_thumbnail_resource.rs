use anyhow::Result;
use std::sync::Arc;
use uc_core::blob::ports::BlobReaderPort;
use uc_core::ids::RepresentationId;
use uc_core::ports::ThumbnailRepositoryPort;

/// Resolve thumbnail resource by representation id.
/// 通过表示 id 解析缩略图资源内容。
pub struct ResolveThumbnailResourceUseCase {
    thumbnail_repo: Arc<dyn ThumbnailRepositoryPort>,
    blob_store: Arc<dyn BlobReaderPort>,
}

/// Thumbnail resource payload and metadata.
/// 缩略图资源内容与元信息。
#[derive(Debug, Clone)]
pub struct ThumbnailResourceResult {
    pub representation_id: RepresentationId,
    pub mime_type: Option<String>,
    pub bytes: Vec<u8>,
}

impl ResolveThumbnailResourceUseCase {
    pub fn new(
        thumbnail_repo: Arc<dyn ThumbnailRepositoryPort>,
        blob_store: Arc<dyn BlobReaderPort>,
    ) -> Self {
        Self {
            thumbnail_repo,
            blob_store,
        }
    }

    #[tracing::instrument(
        name = "usecase.clipboard.resolve_thumbnail_resource.execute",
        skip(self)
    )]
    pub async fn execute(
        &self,
        representation_id: &RepresentationId,
    ) -> Result<ThumbnailResourceResult> {
        let metadata = self
            .thumbnail_repo
            .get_by_representation_id(representation_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Thumbnail not found"))?;

        if metadata.representation_id != *representation_id {
            return Err(anyhow::anyhow!("Thumbnail representation id mismatch"));
        }

        let bytes = self.blob_store.get(&metadata.thumbnail_blob_id).await?;
        let mime_type = Some(metadata.thumbnail_mime_type.as_str().to_string());

        Ok(ThumbnailResourceResult {
            representation_id: representation_id.clone(),
            mime_type,
            bytes,
        })
    }
}
