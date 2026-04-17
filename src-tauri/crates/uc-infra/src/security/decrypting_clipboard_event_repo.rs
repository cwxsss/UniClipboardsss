//! Decrypting clipboard event repository decorator.
//!
//! Wraps ClipboardEventRepositoryPort and decrypts ObservedClipboardRepresentation.bytes on read.

use anyhow::{Context, Result};
use async_trait::async_trait;
use std::sync::Arc;
use tracing::trace;

use uc_core::{
    clipboard::ObservedClipboardRepresentation,
    crypto::aad,
    crypto::model::EncryptedBlob,
    ids::{EventId, RepresentationId},
    ports::{ClipboardEventRepositoryPort, EncryptionPort, EncryptionSessionPort},
};

/// Decorator that decrypts ObservedClipboardRepresentation.bytes on read.
pub struct DecryptingClipboardEventRepository {
    inner: Arc<dyn ClipboardEventRepositoryPort>,
    encryption: Arc<dyn EncryptionPort>,
    session: Arc<dyn EncryptionSessionPort>,
}

impl DecryptingClipboardEventRepository {
    pub fn new(
        inner: Arc<dyn ClipboardEventRepositoryPort>,
        encryption: Arc<dyn EncryptionPort>,
        session: Arc<dyn EncryptionSessionPort>,
    ) -> Self {
        Self {
            inner,
            encryption,
            session,
        }
    }
}

#[async_trait]
impl ClipboardEventRepositoryPort for DecryptingClipboardEventRepository {
    async fn get_representation(
        &self,
        event_id: &EventId,
        representation_id: &str,
    ) -> Result<ObservedClipboardRepresentation> {
        // Get from inner
        let mut observed = self
            .inner
            .get_representation(event_id, representation_id)
            .await?;

        // Decrypt bytes if present
        if !observed.bytes.is_empty() {
            // Try to deserialize as encrypted blob
            match serde_json::from_slice::<EncryptedBlob>(&observed.bytes) {
                Ok(encrypted_blob) => {
                    // Get master key
                    let master_key = self
                        .session
                        .get_master_key()
                        .await
                        .context("encryption session not ready - cannot decrypt")?;

                    // Decrypt
                    let aad = aad::for_inline(event_id, &RepresentationId::from(representation_id));
                    let plaintext = self
                        .encryption
                        .decrypt_blob(&master_key, &encrypted_blob, &aad)
                        .await
                        .context("failed to decrypt representation bytes")?;

                    trace!(
                        representation_id = %representation_id,
                        bytes = plaintext.len(),
                        "Decrypted representation bytes"
                    );

                    observed.bytes = plaintext;
                }
                Err(_) => {
                    // Not encrypted blob format - this could be:
                    // 1. Old unencrypted data (hard fail as per spec)
                    // 2. Corrupted data
                    anyhow::bail!(
                        "representation {} bytes are not in encrypted format - \
                         data may be from before encryption was enabled or corrupted",
                        representation_id
                    );
                }
            }
        }

        Ok(observed)
    }
}
