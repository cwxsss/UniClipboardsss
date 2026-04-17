//! Blob Writer Port.
//!
//! Write-side abstraction for blob storage with content-hash deduplication.
//! Callers pass plaintext bytes; encryption (if any) is handled by the
//! infrastructure layer via decorator.
//!
//! Lives in `uc-core` because the contract speaks only in domain types
//! (`ContentHash` in, `BlobId` out). Concrete implementations live in
//! `uc-infra`.

use crate::{BlobId, ContentHash};

#[async_trait::async_trait]
pub trait BlobWriterPort: Send + Sync {
    /// Write plaintext bytes if the content hash isn't already stored.
    ///
    /// # Atomic semantics
    /// - If `content_id` already exists → return the existing `BlobId`
    /// - If `content_id` doesn't exist → write and return the new `BlobId`
    ///
    /// # Idempotence guarantee
    /// - Multiple concurrent calls with same `content_id` return the same `BlobId`
    /// - Data is written only once per `content_id`
    async fn write_if_absent(
        &self,
        content_id: &ContentHash,
        plaintext_bytes: &[u8],
    ) -> anyhow::Result<BlobId>;
}
