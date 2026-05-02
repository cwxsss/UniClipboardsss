//! Blob Store Port (infra-internal).
//!
//! Abstracts the on-disk (or encrypted-on-disk) blob storage backend.
//! Exposes the low-level put/get operations used by `BlobWriter` and by
//! use cases that need to read raw blob bytes.
//!
//! This port lives in `uc-infra` rather than `uc-core` because its `put`
//! contract returns a `PathBuf` and an optional on-disk compressed size —
//! both are storage-implementation concerns with no domain meaning.

use anyhow::Result;
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;
use uc_core::BlobId;

#[async_trait]
pub trait BlobStorePort: Send + Sync {
    /// Write bytes into blob storage, returning (storage_path, compressed_size).
    ///
    /// The `Option<i64>` is the on-disk byte count after any compression+encryption.
    /// Returns `None` if the store does not track compressed size (e.g., raw filesystem).
    async fn put(&self, blob_id: &BlobId, data: &[u8]) -> Result<(PathBuf, Option<i64>)>;

    /// Read bytes from blob storage.
    async fn get(&self, blob_id: &BlobId) -> Result<Vec<u8>>;
}

#[async_trait]
impl<T: BlobStorePort + ?Sized> BlobStorePort for Arc<T> {
    async fn put(&self, blob_id: &BlobId, data: &[u8]) -> Result<(PathBuf, Option<i64>)> {
        (**self).put(blob_id, data).await
    }

    async fn get(&self, blob_id: &BlobId) -> Result<Vec<u8>> {
        (**self).get(blob_id).await
    }
}
