//! Encrypting clipboard event writer decorator.
//!
//! Wraps ClipboardEventWriterPort and encrypts inline_data before storage.

use anyhow::{Context, Result};
use async_trait::async_trait;
use std::sync::Arc;
use tracing::{debug, trace};

use uc_core::{
    clipboard::{ClipboardEvent, PersistedClipboardRepresentation},
    crypto::aad,
    crypto::model::EncryptionAlgo,
    ids::EventId,
    ports::{ClipboardEventWriterPort, EncryptionPort, EncryptionSessionPort},
};

/// Decorator that encrypts representation inline_data before storage.
pub struct EncryptingClipboardEventWriter {
    inner: Arc<dyn ClipboardEventWriterPort>,
    encryption: Arc<dyn EncryptionPort>,
    session: Arc<dyn EncryptionSessionPort>,
}

impl EncryptingClipboardEventWriter {
    pub fn new(
        inner: Arc<dyn ClipboardEventWriterPort>,
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
impl ClipboardEventWriterPort for EncryptingClipboardEventWriter {
    async fn insert_event(
        &self,
        event: &ClipboardEvent,
        representations: &Vec<PersistedClipboardRepresentation>,
    ) -> Result<()> {
        // Get master key from session
        let master_key = self
            .session
            .get_master_key()
            .await
            .context("encryption session not ready - cannot encrypt clipboard data")?;

        // Encrypt inline_data for each representation
        let mut encrypted_reps = Vec::with_capacity(representations.len());
        let mut encrypted_count = 0usize;
        let mut total_plaintext_bytes = 0usize;
        let mut total_ciphertext_bytes = 0usize;

        for rep in representations {
            let encrypted_inline_data = if let Some(ref plaintext) = rep.inline_data {
                // Encrypt the inline data
                let aad = aad::for_inline(&event.event_id, &rep.id);
                let encrypted_blob = self
                    .encryption
                    .encrypt_blob(
                        &master_key,
                        plaintext,
                        &aad,
                        EncryptionAlgo::XChaCha20Poly1305,
                    )
                    .await
                    .context("failed to encrypt inline_data")?;

                // Serialize to bytes
                let encrypted_bytes = serde_json::to_vec(&encrypted_blob)
                    .context("failed to serialize encrypted inline_data")?;

                trace!(
                    representation_id = %rep.id.as_ref(),
                    plaintext_bytes = plaintext.len(),
                    ciphertext_bytes = encrypted_bytes.len(),
                    "Encrypted inline_data for representation"
                );
                encrypted_count += 1;
                total_plaintext_bytes += plaintext.len();
                total_ciphertext_bytes += encrypted_bytes.len();

                Some(encrypted_bytes)
            } else {
                None
            };

            // Create new representation with encrypted inline_data, preserving
            // the original payload_state. Using ::new() here would re-infer state
            // from (inline_data, blob_id), converting Staged to Inline and preventing
            // the blob worker from materializing full content.
            encrypted_reps.push(PersistedClipboardRepresentation::new_with_state(
                rep.id.clone(),
                rep.format_id.clone(),
                rep.mime_type.clone(),
                rep.size_bytes,
                encrypted_inline_data,
                rep.blob_id.clone(),
                rep.payload_state(),
                rep.last_error.clone(),
            )?);
        }

        if encrypted_count > 0 {
            debug!(
                event_id = %event.event_id.as_ref(),
                representations = representations.len(),
                encrypted = encrypted_count,
                total_plaintext_bytes,
                total_ciphertext_bytes,
                "Encrypted inline_data for event"
            );
        }

        // Delegate to inner with encrypted representations
        self.inner.insert_event(event, &encrypted_reps).await
    }

    async fn delete_event_and_representations(&self, event_id: &EventId) -> Result<()> {
        // Deletion doesn't need encryption - just delegate
        self.inner.delete_event_and_representations(event_id).await
    }
}
