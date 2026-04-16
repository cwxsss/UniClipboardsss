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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_mocks::{MockBlobStore, MockClipboardRepresentationRepository};
    use uc_core::clipboard::{MimeType, PersistedClipboardRepresentation};
    use uc_core::ids::RepresentationId;
    use uc_core::BlobId;

    #[tokio::test]
    async fn test_resolve_blob_resource_returns_bytes() {
        let blob_id = BlobId::from("blob-1");
        let rep_id = RepresentationId::from("rep-1");
        let representation = PersistedClipboardRepresentation::new(
            rep_id,
            uc_core::ids::FormatId::from("public.png"),
            Some(MimeType("image/png".to_string())),
            128,
            None,
            Some(blob_id.clone()),
        );

        let mut rep_repo = MockClipboardRepresentationRepository::new();
        rep_repo
            .expect_get_representation_by_blob_id()
            .returning(move |_| Ok(Some(representation.clone())));

        let expected_blob_id = blob_id.clone();
        let mut blob_store = MockBlobStore::new();
        blob_store.expect_get().returning(move |id| {
            if *id == expected_blob_id {
                Ok(vec![1, 2, 3])
            } else {
                Err(anyhow::anyhow!("Blob not found"))
            }
        });

        let uc = ResolveBlobResourceUseCase::new(Arc::new(rep_repo), Arc::new(blob_store));

        let result = uc.execute(&blob_id).await.unwrap();

        assert_eq!(result.blob_id, blob_id);
        assert_eq!(result.mime_type, Some("image/png".to_string()));
        assert_eq!(result.bytes, vec![1, 2, 3]);
    }
}
