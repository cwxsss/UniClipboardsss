use anyhow::Result;
use std::sync::Arc;
use uc_core::ids::RepresentationId;
use uc_core::ports::{BlobStorePort, ThumbnailRepositoryPort};

/// Resolve thumbnail resource by representation id.
/// 通过表示 id 解析缩略图资源内容。
pub struct ResolveThumbnailResourceUseCase {
    thumbnail_repo: Arc<dyn ThumbnailRepositoryPort>,
    blob_store: Arc<dyn BlobStorePort>,
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
        blob_store: Arc<dyn BlobStorePort>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_mocks::{MockBlobStore, MockThumbnailRepository};
    use uc_core::clipboard::{MimeType, ThumbnailMetadata};
    use uc_core::ids::RepresentationId;
    use uc_core::BlobId;

    #[tokio::test]
    async fn test_resolve_thumbnail_resource_returns_bytes() {
        let rep_id = RepresentationId::from("rep-1");
        let blob_id = BlobId::from("thumb-1");
        let metadata = ThumbnailMetadata::new(
            rep_id.clone(),
            blob_id.clone(),
            MimeType("image/webp".to_string()),
            120,
            80,
            1024,
            None,
        );

        let metadata_rep_id = metadata.representation_id.clone();
        let metadata_blob_id = metadata.thumbnail_blob_id.clone();
        let metadata_mime = metadata.thumbnail_mime_type.clone();
        let metadata_w = metadata.original_width;
        let metadata_h = metadata.original_height;
        let metadata_size = metadata.original_size_bytes;
        let metadata_created = metadata.created_at_ms;

        let mut thumbnail_repo = MockThumbnailRepository::new();
        thumbnail_repo
            .expect_get_by_representation_id()
            .returning(move |_| {
                Ok(Some(ThumbnailMetadata::new(
                    metadata_rep_id.clone(),
                    metadata_blob_id.clone(),
                    metadata_mime.clone(),
                    metadata_w,
                    metadata_h,
                    metadata_size,
                    metadata_created,
                )))
            });

        let expected_bytes = vec![1u8, 2, 3];
        let expected_blob_id = blob_id.clone();
        let mut blob_store = MockBlobStore::new();
        blob_store.expect_get().returning(move |id| {
            if *id == expected_blob_id {
                Ok(expected_bytes.clone())
            } else {
                Err(anyhow::anyhow!("Blob not found"))
            }
        });

        let uc =
            ResolveThumbnailResourceUseCase::new(Arc::new(thumbnail_repo), Arc::new(blob_store));

        let result = uc.execute(&rep_id).await.unwrap();
        assert_eq!(result.mime_type, Some("image/webp".to_string()));
        assert_eq!(result.bytes, vec![1, 2, 3]);
    }
}
