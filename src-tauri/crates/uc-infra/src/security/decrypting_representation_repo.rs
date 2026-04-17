//! Decrypting clipboard representation repository decorator.
//!
//! Wraps ClipboardRepresentationRepositoryPort and decrypts inline_data on read.

use anyhow::{Context, Result};
use async_trait::async_trait;
use std::sync::Arc;
use tracing::{debug, trace};

use uc_core::ports::clipboard::ProcessingUpdateOutcome;
use uc_core::{
    clipboard::{PayloadAvailability, PersistedClipboardRepresentation},
    crypto::aad,
    crypto::model::EncryptedBlob,
    ids::{EventId, RepresentationId},
    ports::{ClipboardRepresentationRepositoryPort, EncryptionPort, EncryptionSessionPort},
    BlobId,
};

/// Decorator that decrypts representation inline_data on read.
pub struct DecryptingClipboardRepresentationRepository {
    inner: Arc<dyn ClipboardRepresentationRepositoryPort>,
    encryption: Arc<dyn EncryptionPort>,
    session: Arc<dyn EncryptionSessionPort>,
}

impl DecryptingClipboardRepresentationRepository {
    pub fn new(
        inner: Arc<dyn ClipboardRepresentationRepositoryPort>,
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
impl ClipboardRepresentationRepositoryPort for DecryptingClipboardRepresentationRepository {
    async fn get_representation(
        &self,
        event_id: &EventId,
        representation_id: &RepresentationId,
    ) -> Result<Option<PersistedClipboardRepresentation>> {
        // Get from inner
        let rep_opt = self
            .inner
            .get_representation(event_id, representation_id)
            .await?;

        let Some(rep) = rep_opt else {
            return Ok(None);
        };

        // Decrypt inline_data if present
        let decrypted_inline_data = if let Some(ref encrypted_bytes) = rep.inline_data {
            // Deserialize encrypted blob
            let encrypted_blob: EncryptedBlob = serde_json::from_slice(encrypted_bytes)
                .context("failed to deserialize encrypted inline_data - data may be corrupted")?;

            // Get master key
            let master_key = self
                .session
                .get_master_key()
                .await
                .context("encryption session not ready - cannot decrypt")?;

            // Decrypt
            let aad = aad::for_inline(event_id, representation_id);
            let plaintext = self
                .encryption
                .decrypt_blob(&master_key, &encrypted_blob, &aad)
                .await
                .context("failed to decrypt inline_data")?;

            trace!(
                representation_id = %representation_id.as_ref(),
                bytes = plaintext.len(),
                "Decrypted inline_data for representation"
            );

            Some(plaintext)
        } else {
            None
        };

        // Return representation with decrypted data, preserving the original payload_state.
        // Using ::new() here would re-infer the state from (inline_data, blob_id), which
        // incorrectly converts Staged (with preview inline_data) to Inline, causing
        // has_detail=false in list projections and preventing full content loading.
        Ok(Some(PersistedClipboardRepresentation::new_with_state(
            rep.id,
            rep.format_id,
            rep.mime_type,
            rep.size_bytes,
            decrypted_inline_data,
            rep.blob_id,
            rep.payload_state,
            rep.last_error,
        )?))
    }

    async fn get_representation_by_id(
        &self,
        representation_id: &RepresentationId,
    ) -> Result<Option<PersistedClipboardRepresentation>> {
        let rep_opt = self
            .inner
            .get_representation_by_id(representation_id)
            .await?;

        let Some(rep) = rep_opt else {
            return Ok(None);
        };

        if rep.inline_data.is_some() {
            trace!(
                representation_id = %representation_id.as_ref(),
                "Skipping inline_data decryption: event_id unavailable"
            );
        }

        Ok(Some(rep))
    }

    async fn get_representation_by_blob_id(
        &self,
        blob_id: &BlobId,
    ) -> Result<Option<PersistedClipboardRepresentation>> {
        let rep_opt = self.inner.get_representation_by_blob_id(blob_id).await?;

        let Some(rep) = rep_opt else {
            return Ok(None);
        };

        if rep.inline_data.is_some() {
            trace!(
                blob_id = %blob_id.as_ref(),
                "Skipping inline_data decryption: event_id unavailable"
            );
        }

        Ok(Some(rep))
    }

    async fn update_blob_id(
        &self,
        representation_id: &RepresentationId,
        blob_id: &BlobId,
    ) -> Result<()> {
        // No encryption needed for blob_id update - just delegate
        self.inner.update_blob_id(representation_id, blob_id).await
    }

    async fn update_blob_id_if_none(
        &self,
        representation_id: &RepresentationId,
        blob_id: &BlobId,
    ) -> Result<bool> {
        // No encryption needed for blob_id update - just delegate
        self.inner
            .update_blob_id_if_none(representation_id, blob_id)
            .await
    }

    async fn update_processing_result(
        &self,
        rep_id: &RepresentationId,
        expected_states: &[PayloadAvailability],
        blob_id: Option<&BlobId>,
        new_state: PayloadAvailability,
        last_error: Option<&str>,
    ) -> Result<ProcessingUpdateOutcome> {
        // Delegate to inner repo - this method is for state updates, not data reading
        // The returned representation may contain encrypted inline_data, which is expected
        // for update operations. Use get_representation to get decrypted data.
        self.inner
            .update_processing_result(rep_id, expected_states, blob_id, new_state, last_error)
            .await
    }

    async fn get_representations_for_event(
        &self,
        event_id: &EventId,
    ) -> Result<Vec<PersistedClipboardRepresentation>> {
        let reps = self.inner.get_representations_for_event(event_id).await?;
        let input_count = reps.len();
        let mut decrypted_count = 0usize;
        let mut decrypted_bytes = 0usize;
        let mut result = Vec::with_capacity(reps.len());
        for rep in reps {
            if let Some(ref encrypted_bytes) = rep.inline_data {
                match serde_json::from_slice::<EncryptedBlob>(encrypted_bytes) {
                    Ok(encrypted_blob) => {
                        let master_key = self
                            .session
                            .get_master_key()
                            .await
                            .context("encryption session not ready - cannot decrypt")?;
                        let aad = aad::for_inline(event_id, &rep.id);
                        match self
                            .encryption
                            .decrypt_blob(&master_key, &encrypted_blob, &aad)
                            .await
                        {
                            Ok(plaintext) => {
                                decrypted_count += 1;
                                decrypted_bytes += plaintext.len();
                                result.push(PersistedClipboardRepresentation::new_with_state(
                                    rep.id,
                                    rep.format_id,
                                    rep.mime_type,
                                    rep.size_bytes,
                                    Some(plaintext),
                                    rep.blob_id,
                                    rep.payload_state,
                                    rep.last_error,
                                )?);
                            }
                            Err(_) => {
                                // If decryption fails, return with encrypted data
                                result.push(rep);
                            }
                        }
                    }
                    Err(_) => {
                        // Not encrypted, return as-is
                        result.push(rep);
                    }
                }
            } else {
                result.push(rep);
            }
        }
        if decrypted_count > 0 {
            debug!(
                event_id = %event_id.as_ref(),
                representations = input_count,
                decrypted = decrypted_count,
                decrypted_bytes,
                "Decrypted representations for event"
            );
        }
        Ok(result)
    }

    async fn update_mime_type(
        &self,
        rep_id: &RepresentationId,
        mime: &uc_core::clipboard::MimeType,
    ) -> Result<()> {
        self.inner.update_mime_type(rep_id, mime).await
    }
}
