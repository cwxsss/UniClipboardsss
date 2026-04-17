//! Blob Writer Port
//!
//! This port writes plaintext bytes to blob store with deduplication.
//!
//! # Encryption boundary
//! - Callers pass **plaintext** bytes.
//! - Encryption (if any) is handled by the injected `BlobStorePort`
//!   decorator (e.g. `EncryptedBlobStore`).
//!
//! **Semantic:** "write_if_absent" = atomic write-if-absent with deduplication

use crate::{BlobId, ContentHash};

#[async_trait::async_trait]
pub trait BlobWriterPort: Send + Sync {
    /// Write plaintext bytes to blob store if content_id doesn't already exist.
    ///
    /// # Atomic semantics
    /// - If `content_id` already exists → return the existing `BlobId`
    /// - If `content_id` doesn't exist → write and return the new `BlobId`
    ///
    /// # Idempotence guarantee
    /// - Multiple concurrent calls with same content_id return the same `BlobId`
    /// - Data is written only once per content_id
    ///
    /// # Parameters
    /// - `content_id`: Hash-based identifier for deduplication (keyed hash)
    /// - `plaintext_bytes`: Plaintext payload to persist
    async fn write_if_absent(
        &self,
        content_id: &ContentHash,
        plaintext_bytes: &[u8],
    ) -> anyhow::Result<BlobId>;
}
