use std::sync::Arc;

use bytes::Bytes;

use uc_core::crypto::domain::Plaintext;
use uc_core::ids::EntryId;
use uc_core::ports::blob::{
    BlobDigest, BlobReferenceRepositoryPort, BlobTicket, BlobTransferPort, PlaintextHash, TagReason,
};
use uc_core::ports::security::BlobCipherPort;
use uc_core::ports::ContentHashPort;

use super::{aad_for_entry, active_space_placeholder};

#[derive(Debug, Clone)]
pub(crate) struct PublishBlobInput {
    pub plaintext: Bytes,
    pub entry_id: EntryId,
}

#[derive(Debug, Clone)]
pub(crate) struct PublishBlobOutcome {
    pub ticket: BlobTicket,
    pub entry_id: EntryId,
    pub plaintext_hash: PlaintextHash,
    pub digest: BlobDigest,
    pub reused_existing: bool,
}

pub(crate) struct PublishBlobUseCase {
    hash: Arc<dyn ContentHashPort>,
    blob_cipher: Arc<dyn BlobCipherPort>,
    blob_transfer: Arc<dyn BlobTransferPort>,
    blob_reference: Arc<dyn BlobReferenceRepositoryPort>,
}

impl PublishBlobUseCase {
    pub fn new(
        hash: Arc<dyn ContentHashPort>,
        blob_cipher: Arc<dyn BlobCipherPort>,
        blob_transfer: Arc<dyn BlobTransferPort>,
        blob_reference: Arc<dyn BlobReferenceRepositoryPort>,
    ) -> Self {
        Self {
            hash,
            blob_cipher,
            blob_transfer,
            blob_reference,
        }
    }

    pub async fn execute(
        &self,
        input: PublishBlobInput,
    ) -> Result<PublishBlobOutcome, PublishBlobError> {
        if input.plaintext.is_empty() {
            return Err(PublishBlobError::EmptyPlaintext);
        }

        let plaintext_hash = PlaintextHash::from_bytes(
            self.hash
                .hash_bytes(&input.plaintext)
                .map_err(|e| PublishBlobError::Hash(e.to_string()))?
                .bytes,
        );

        if let Some(digest) = self.find_reusable_digest(&plaintext_hash).await? {
            self.blob_transfer
                .tag(&digest, TagReason::ClipboardEntry(input.entry_id.clone()))
                .await
                .map_err(|e| PublishBlobError::Transfer(e.to_string()))?;
            let ticket = self
                .blob_transfer
                .issue_ticket(&digest)
                .await
                .map_err(|e| PublishBlobError::Transfer(e.to_string()))?;
            return Ok(PublishBlobOutcome {
                ticket,
                entry_id: input.entry_id,
                plaintext_hash,
                digest,
                reused_existing: true,
            });
        }

        let plaintext = Plaintext::new(input.plaintext.to_vec());
        let ciphertext = self
            .blob_cipher
            .encrypt(
                &active_space_placeholder(),
                &plaintext,
                &aad_for_entry(&input.entry_id),
            )
            .await
            .map_err(|e| PublishBlobError::Cipher(e.to_string()))?;

        let digest = self
            .blob_transfer
            .publish(Bytes::from(ciphertext.into_bytes()))
            .await
            .map_err(|e| PublishBlobError::Transfer(e.to_string()))?;
        self.blob_reference
            .save(plaintext_hash, digest)
            .await
            .map_err(|e| PublishBlobError::Reference(e.to_string()))?;
        self.blob_transfer
            .tag(&digest, TagReason::ClipboardEntry(input.entry_id.clone()))
            .await
            .map_err(|e| PublishBlobError::Transfer(e.to_string()))?;
        let ticket = self
            .blob_transfer
            .issue_ticket(&digest)
            .await
            .map_err(|e| PublishBlobError::Transfer(e.to_string()))?;

        Ok(PublishBlobOutcome {
            ticket,
            entry_id: input.entry_id,
            plaintext_hash,
            digest,
            reused_existing: false,
        })
    }

    async fn find_reusable_digest(
        &self,
        plaintext_hash: &PlaintextHash,
    ) -> Result<Option<BlobDigest>, PublishBlobError> {
        let Some(digest) = self
            .blob_reference
            .find_by_plaintext_hash(plaintext_hash)
            .await
            .map_err(|e| PublishBlobError::Reference(e.to_string()))?
        else {
            return Ok(None);
        };

        let exists = self
            .blob_transfer
            .has(&digest)
            .await
            .map_err(|e| PublishBlobError::Transfer(e.to_string()))?;
        Ok(exists.then_some(digest))
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum PublishBlobError {
    #[error("blob plaintext is empty")]
    EmptyPlaintext,
    #[error("hash failed: {0}")]
    Hash(String),
    #[error("blob cipher failed: {0}")]
    Cipher(String),
    #[error("blob transfer failed: {0}")]
    Transfer(String),
    #[error("blob reference failed: {0}")]
    Reference(String),
}
