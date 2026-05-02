use std::sync::Arc;

use tracing::instrument;
use uc_core::blob::ports::BlobReaderPort;
use uc_core::clipboard::MimeType;
use uc_core::ids::{BlobId, RepresentationId};
use uc_core::ports::{ClipboardRepresentationRepositoryPort, ThumbnailRepositoryPort};

#[derive(Clone)]
pub struct ResourceFacadeDeps {
    pub representation_repo: Arc<dyn ClipboardRepresentationRepositoryPort>,
    pub thumbnail_repo: Arc<dyn ThumbnailRepositoryPort>,
    pub blob_store: Arc<dyn BlobReaderPort>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinaryResourceView {
    pub mime_type: Option<String>,
    pub bytes: Vec<u8>,
}

#[derive(Debug, thiserror::Error)]
pub enum ResourceFacadeError {
    #[error("resource not found")]
    NotFound,
    #[error("resource mismatch: {0}")]
    Mismatch(String),
    #[error("failed to resolve resource: {0}")]
    Internal(String),
}

pub struct ResourceFacade {
    deps: ResourceFacadeDeps,
}

impl ResourceFacade {
    pub fn new(deps: ResourceFacadeDeps) -> Self {
        Self { deps }
    }

    #[instrument(skip_all, fields(blob_id = %blob_id))]
    pub async fn blob(&self, blob_id: &str) -> Result<BinaryResourceView, ResourceFacadeError> {
        let blob_id = BlobId::from(blob_id);
        let representation = self
            .deps
            .representation_repo
            .get_representation_by_blob_id(&blob_id)
            .await
            .map_err(|err| ResourceFacadeError::Internal(err.to_string()))?
            .ok_or(ResourceFacadeError::NotFound)?;

        if representation.blob_id.as_ref() != Some(&blob_id) {
            return Err(ResourceFacadeError::Mismatch(
                "representation blob id does not match request".to_string(),
            ));
        }

        let mime_type = representation.mime_type.as_ref().map(mime_to_string);
        let bytes = self
            .deps
            .blob_store
            .get(&blob_id)
            .await
            .map_err(|err| ResourceFacadeError::Internal(err.to_string()))?;

        Ok(BinaryResourceView { mime_type, bytes })
    }

    #[instrument(skip_all, fields(representation_id = %representation_id))]
    pub async fn thumbnail(
        &self,
        representation_id: &str,
    ) -> Result<BinaryResourceView, ResourceFacadeError> {
        let representation_id = RepresentationId::from(representation_id);
        let metadata = self
            .deps
            .thumbnail_repo
            .get_by_representation_id(&representation_id)
            .await
            .map_err(|err| ResourceFacadeError::Internal(err.to_string()))?
            .ok_or(ResourceFacadeError::NotFound)?;

        if metadata.representation_id != representation_id {
            return Err(ResourceFacadeError::Mismatch(
                "thumbnail representation id does not match request".to_string(),
            ));
        }

        let mime_type = Some(mime_to_string(&metadata.thumbnail_mime_type));
        let bytes = self
            .deps
            .blob_store
            .get(&metadata.thumbnail_blob_id)
            .await
            .map_err(|err| ResourceFacadeError::Internal(err.to_string()))?;

        Ok(BinaryResourceView { mime_type, bytes })
    }
}

fn mime_to_string(mime: &MimeType) -> String {
    mime.as_str().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    use anyhow::Result;
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::Mutex;
    use uc_core::clipboard::{
        PayloadAvailability, PersistedClipboardRepresentation, ThumbnailMetadata,
    };
    use uc_core::ids::{EventId, FormatId};
    use uc_core::ports::ProcessingUpdateOutcome;

    #[derive(Default)]
    struct FakeRepresentationRepo {
        by_blob_id: Mutex<HashMap<BlobId, PersistedClipboardRepresentation>>,
    }

    #[async_trait]
    impl ClipboardRepresentationRepositoryPort for FakeRepresentationRepo {
        async fn get_representation(
            &self,
            _event_id: &EventId,
            _representation_id: &RepresentationId,
        ) -> Result<Option<PersistedClipboardRepresentation>> {
            Ok(None)
        }

        async fn get_representation_by_id(
            &self,
            _representation_id: &RepresentationId,
        ) -> Result<Option<PersistedClipboardRepresentation>> {
            Ok(None)
        }

        async fn get_representation_by_blob_id(
            &self,
            blob_id: &BlobId,
        ) -> Result<Option<PersistedClipboardRepresentation>> {
            Ok(self
                .by_blob_id
                .lock()
                .expect("representation lock")
                .get(blob_id)
                .cloned())
        }

        async fn update_blob_id(
            &self,
            _representation_id: &RepresentationId,
            _blob_id: &BlobId,
        ) -> Result<()> {
            Ok(())
        }

        async fn update_blob_id_if_none(
            &self,
            _representation_id: &RepresentationId,
            _blob_id: &BlobId,
        ) -> Result<bool> {
            Ok(true)
        }

        async fn update_processing_result(
            &self,
            _rep_id: &RepresentationId,
            _expected_states: &[PayloadAvailability],
            _blob_id: Option<&BlobId>,
            _new_state: PayloadAvailability,
            _last_error: Option<&str>,
        ) -> Result<ProcessingUpdateOutcome> {
            Ok(ProcessingUpdateOutcome::NotFound)
        }
    }

    #[derive(Default)]
    struct FakeThumbnailRepo {
        by_representation_id: Mutex<HashMap<RepresentationId, ThumbnailMetadata>>,
    }

    #[async_trait]
    impl ThumbnailRepositoryPort for FakeThumbnailRepo {
        async fn get_by_representation_id(
            &self,
            representation_id: &RepresentationId,
        ) -> Result<Option<ThumbnailMetadata>> {
            Ok(self
                .by_representation_id
                .lock()
                .expect("thumbnail lock")
                .remove(representation_id))
        }

        async fn insert_thumbnail(&self, metadata: &ThumbnailMetadata) -> Result<()> {
            self.by_representation_id
                .lock()
                .expect("thumbnail lock")
                .insert(
                    metadata.representation_id.clone(),
                    ThumbnailMetadata::new(
                        metadata.representation_id.clone(),
                        metadata.thumbnail_blob_id.clone(),
                        metadata.thumbnail_mime_type.clone(),
                        metadata.original_width,
                        metadata.original_height,
                        metadata.original_size_bytes,
                        metadata.created_at_ms,
                    ),
                );
            Ok(())
        }
    }

    #[derive(Default)]
    struct FakeBlobStore {
        bytes: Mutex<HashMap<BlobId, Vec<u8>>>,
    }

    #[async_trait]
    impl BlobReaderPort for FakeBlobStore {
        async fn get(&self, blob_id: &BlobId) -> Result<Vec<u8>> {
            self.bytes
                .lock()
                .expect("blob lock")
                .get(blob_id)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("blob not found"))
        }
    }

    struct TestDeps {
        facade: ResourceFacade,
        representation_repo: Arc<FakeRepresentationRepo>,
        thumbnail_repo: Arc<FakeThumbnailRepo>,
        blob_store: Arc<FakeBlobStore>,
    }

    fn test_deps() -> TestDeps {
        let representation_repo = Arc::new(FakeRepresentationRepo::default());
        let thumbnail_repo = Arc::new(FakeThumbnailRepo::default());
        let blob_store = Arc::new(FakeBlobStore::default());
        let facade = ResourceFacade::new(ResourceFacadeDeps {
            representation_repo: representation_repo.clone(),
            thumbnail_repo: thumbnail_repo.clone(),
            blob_store: blob_store.clone(),
        });
        TestDeps {
            facade,
            representation_repo,
            thumbnail_repo,
            blob_store,
        }
    }

    #[tokio::test]
    async fn blob_returns_bytes_and_mime_type() {
        let deps = test_deps();
        let blob_id = BlobId::from("blob-1");
        let representation = PersistedClipboardRepresentation::new(
            RepresentationId::from("rep-1"),
            FormatId::from("public.png"),
            Some(MimeType("image/png".to_string())),
            3,
            None,
            Some(blob_id.clone()),
        );
        deps.representation_repo
            .by_blob_id
            .lock()
            .expect("representation lock")
            .insert(blob_id.clone(), representation);
        deps.blob_store
            .bytes
            .lock()
            .expect("blob lock")
            .insert(blob_id, vec![1, 2, 3]);

        let view = deps.facade.blob("blob-1").await.expect("blob");

        assert_eq!(view.mime_type, Some("image/png".to_string()));
        assert_eq!(view.bytes, vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn blob_returns_not_found_when_representation_is_missing() {
        let deps = test_deps();

        let error = deps.facade.blob("missing").await.expect_err("missing");

        assert!(matches!(error, ResourceFacadeError::NotFound));
    }

    #[tokio::test]
    async fn thumbnail_returns_bytes_and_mime_type() {
        let deps = test_deps();
        let representation_id = RepresentationId::from("rep-1");
        let blob_id = BlobId::from("thumb-1");
        deps.thumbnail_repo
            .insert_thumbnail(&ThumbnailMetadata::new(
                representation_id,
                blob_id.clone(),
                MimeType("image/webp".to_string()),
                100,
                80,
                3,
                None,
            ))
            .await
            .expect("insert thumbnail");
        deps.blob_store
            .bytes
            .lock()
            .expect("blob lock")
            .insert(blob_id, vec![4, 5, 6]);

        let view = deps.facade.thumbnail("rep-1").await.expect("thumbnail");

        assert_eq!(view.mime_type, Some("image/webp".to_string()));
        assert_eq!(view.bytes, vec![4, 5, 6]);
    }
}
