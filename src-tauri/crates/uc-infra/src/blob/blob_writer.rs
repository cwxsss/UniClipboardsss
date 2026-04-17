use anyhow::Result;
use async_trait::async_trait;
use tracing::{debug, debug_span, Instrument};
use uc_core::ports::ClockPort;
use uc_core::ports::{BlobStorePort, BlobWriterPort};
use uc_core::BlobId;
use uc_core::ContentHash;

use crate::blob::{Blob, BlobRepositoryPort, BlobStorageLocator};

pub struct BlobWriter<B, BR, C>
where
    B: BlobStorePort,
    BR: BlobRepositoryPort,
    C: ClockPort,
{
    blob_store: B,
    blob_repo: BR,
    clock: C,
}

impl<B, BR, C> BlobWriter<B, BR, C>
where
    B: BlobStorePort,
    BR: BlobRepositoryPort,
    C: ClockPort,
{
    pub fn new(blob_store: B, blob_repo: BR, clock: C) -> Self {
        BlobWriter {
            blob_store,
            blob_repo,
            clock,
        }
    }
}

#[async_trait]
impl<B, BR, C> BlobWriterPort for BlobWriter<B, BR, C>
where
    B: BlobStorePort,
    BR: BlobRepositoryPort,
    C: ClockPort,
{
    async fn write_if_absent(
        &self,
        content_id: &ContentHash,
        plaintext_bytes: &[u8],
    ) -> Result<BlobId> {
        let span = debug_span!(
            "infra.blob.write_if_absent",
            size_bytes = plaintext_bytes.len(),
            content_hash = %content_id,
        );
        async {
            if let Some(existing) = self.blob_repo.find_by_hash(content_id).await? {
                return Ok(existing.blob_id);
            }

            let blob_id = BlobId::new();

            // Encryption is handled by the injected BlobStorePort decorator (if any).
            let (storage_path, compressed_size) =
                self.blob_store.put(&blob_id, plaintext_bytes).await?;

            let created_at_ms = self.clock.now_ms();
            let blob_storage_locator = BlobStorageLocator::new_local_fs(storage_path);
            let record = Blob::new(
                blob_id.clone(),
                blob_storage_locator,
                plaintext_bytes.len() as i64,
                content_id.clone(),
                created_at_ms,
                compressed_size,
            );

            if let Err(err) = self.blob_repo.insert_blob(&record).await {
                if let Some(existing) = self.blob_repo.find_by_hash(content_id).await? {
                    debug!(
                        error = %err,
                        content_hash = %content_id,
                        "Insert raced with existing blob; returning existing record",
                    );
                    return Ok(existing.blob_id);
                }
                return Err(err);
            }
            Ok(blob_id)
        }
        .instrument(span)
        .await
    }
}
