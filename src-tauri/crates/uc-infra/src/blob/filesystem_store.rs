//! Filesystem-based blob storage.
//! 基于文件系统的 blob 存储。

use anyhow::{Context, Result};
use std::path::PathBuf;
use uc_core::blob::ports::BlobReaderPort;
use uc_core::BlobId;

use crate::blob::BlobStorePort;

/// Filesystem-based blob storage.
pub struct FilesystemBlobStore {
    base_dir: PathBuf,
}

impl FilesystemBlobStore {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    async fn ensure_dir(&self) -> Result<()> {
        tokio::fs::create_dir_all(&self.base_dir)
            .await
            .context("Failed to create blob directory")
    }

    fn blob_path(&self, blob_id: &BlobId) -> PathBuf {
        self.base_dir.join(blob_id.as_str())
    }
}

#[async_trait::async_trait]
impl BlobStorePort for FilesystemBlobStore {
    async fn put(&self, blob_id: &BlobId, data: &[u8]) -> Result<(PathBuf, Option<i64>)> {
        self.ensure_dir().await?;
        let path = self.blob_path(blob_id);

        let mut file = tokio::fs::File::create(&path)
            .await
            .context("Failed to create blob file")?;
        tokio::io::AsyncWriteExt::write_all(&mut file, data)
            .await
            .context("Failed to write blob data")?;
        tokio::io::AsyncWriteExt::flush(&mut file)
            .await
            .context("Failed to flush blob data")?;
        file.sync_all().await.context("Failed to sync blob file")?;

        // Raw filesystem store doesn't track compression.
        Ok((path, None))
    }

    async fn get(&self, blob_id: &BlobId) -> Result<Vec<u8>> {
        <Self as BlobReaderPort>::get(self, blob_id).await
    }
}

#[async_trait::async_trait]
impl BlobReaderPort for FilesystemBlobStore {
    async fn get(&self, blob_id: &BlobId) -> Result<Vec<u8>> {
        let path = self.blob_path(blob_id);
        let mut file = tokio::fs::File::open(&path)
            .await
            .context("Failed to open blob file")?;

        let mut data = Vec::new();
        tokio::io::AsyncReadExt::read_to_end(&mut file, &mut data)
            .await
            .context("Failed to read blob data")?;

        Ok(data)
    }
}
