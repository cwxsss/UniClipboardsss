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
    ids::{EventId, RepresentationId},
    ports::{ClipboardRepresentationRepositoryPort, EncryptionPort, EncryptionSessionPort},
    security::aad,
    security::model::EncryptedBlob,
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

#[cfg(test)]
mod tests {
    use super::*;
    use mockall::mock;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use uc_core::{
        clipboard::{MimeType, PersistedClipboardRepresentation},
        ids::{BlobId, EventId, FormatId, RepresentationId},
        ports::clipboard::ClipboardRepresentationRepositoryPort,
        security::aad,
        security::model::{
            EncryptedBlob, EncryptionAlgo, EncryptionError, EncryptionFormatVersion, KdfParams,
            Kek, MasterKey, Passphrase,
        },
    };

    type RepresentationStore =
        Arc<Mutex<HashMap<(EventId, RepresentationId), PersistedClipboardRepresentation>>>;

    mock! {
        RepresentationRepo {}

        #[async_trait::async_trait]
        impl ClipboardRepresentationRepositoryPort for RepresentationRepo {
            async fn get_representation(
                &self,
                event_id: &EventId,
                representation_id: &RepresentationId,
            ) -> Result<Option<PersistedClipboardRepresentation>>;
            async fn get_representation_by_id(
                &self,
                representation_id: &RepresentationId,
            ) -> Result<Option<PersistedClipboardRepresentation>>;
            async fn get_representation_by_blob_id(
                &self,
                blob_id: &BlobId,
            ) -> Result<Option<PersistedClipboardRepresentation>>;
            async fn update_blob_id(
                &self,
                representation_id: &RepresentationId,
                blob_id: &BlobId,
            ) -> Result<()>;
            async fn update_blob_id_if_none(
                &self,
                representation_id: &RepresentationId,
                blob_id: &BlobId,
            ) -> Result<bool>;
            #[mockall::concretize]
            async fn update_processing_result(
                &self,
                rep_id: &RepresentationId,
                expected_states: &[PayloadAvailability],
                blob_id: Option<&BlobId>,
                new_state: PayloadAvailability,
                last_error: Option<&str>,
            ) -> Result<ProcessingUpdateOutcome>;
        }
    }

    mock! {
        Encryption {}

        #[async_trait::async_trait]
        impl uc_core::ports::EncryptionPort for Encryption {
            async fn derive_kek(
                &self,
                passphrase: &Passphrase,
                salt: &[u8],
                kdf_params: &KdfParams,
            ) -> Result<Kek, EncryptionError>;
            async fn wrap_master_key(
                &self,
                kek: &Kek,
                master_key: &MasterKey,
                aead: EncryptionAlgo,
            ) -> Result<EncryptedBlob, EncryptionError>;
            async fn unwrap_master_key(
                &self,
                kek: &Kek,
                blob: &EncryptedBlob,
            ) -> Result<MasterKey, EncryptionError>;
            async fn encrypt_blob(
                &self,
                master_key: &MasterKey,
                plaintext: &[u8],
                aad: &[u8],
                algo: EncryptionAlgo,
            ) -> Result<EncryptedBlob, EncryptionError>;
            async fn decrypt_blob(
                &self,
                master_key: &MasterKey,
                blob: &EncryptedBlob,
                aad: &[u8],
            ) -> Result<Vec<u8>, EncryptionError>;
        }
    }

    mock! {
        EncryptionSession {}

        #[async_trait::async_trait]
        impl EncryptionSessionPort for EncryptionSession {
            async fn is_ready(&self) -> bool;
            async fn get_master_key(&self) -> Result<MasterKey, EncryptionError>;
            async fn set_master_key(&self, master_key: MasterKey) -> Result<(), EncryptionError>;
            async fn clear(&self) -> Result<(), EncryptionError>;
        }
    }

    fn make_representation_repo_with_store() -> (MockRepresentationRepo, RepresentationStore) {
        let store: RepresentationStore = Arc::new(Mutex::new(HashMap::new()));
        let mut repo = MockRepresentationRepo::new();

        {
            let store = Arc::clone(&store);
            repo.expect_get_representation()
                .returning(move |event_id, representation_id| {
                    let entries = store.lock().expect("representation store poisoned");
                    Ok(entries
                        .get(&(event_id.clone(), representation_id.clone()))
                        .cloned())
                });
        }

        {
            let store = Arc::clone(&store);
            repo.expect_get_representation_by_id()
                .returning(move |representation_id| {
                    let entries = store.lock().expect("representation store poisoned");
                    Ok(entries.iter().find_map(|((_event_id, rep_id), rep)| {
                        if rep_id == representation_id {
                            Some(rep.clone())
                        } else {
                            None
                        }
                    }))
                });
        }

        repo.expect_get_representation_by_blob_id()
            .returning(|_| Ok(None));

        {
            let store = Arc::clone(&store);
            repo.expect_update_blob_id()
                .returning(move |representation_id, blob_id| {
                    let mut entries = store.lock().expect("representation store poisoned");
                    for ((_event_id, rep_id), rep) in entries.iter_mut() {
                        if rep_id == representation_id {
                            rep.blob_id = Some(blob_id.clone());
                        }
                    }
                    Ok(())
                });
        }

        {
            let store = Arc::clone(&store);
            repo.expect_update_blob_id_if_none()
                .returning(move |representation_id, blob_id| {
                    let mut entries = store.lock().expect("representation store poisoned");
                    let mut updated = false;
                    for ((_event_id, rep_id), rep) in entries.iter_mut() {
                        if rep_id == representation_id && rep.blob_id.is_none() {
                            rep.blob_id = Some(blob_id.clone());
                            updated = true;
                        }
                    }
                    Ok(updated)
                });
        }

        {
            let store = Arc::clone(&store);
            repo.expect_update_processing_result().returning(
                move |rep_id, expected_states, blob_id, new_state, last_error| {
                    let mut entries = store.lock().expect("representation store poisoned");
                    for ((_event_id, candidate_id), rep) in entries.iter_mut() {
                        if candidate_id == rep_id {
                            let current_state = rep.payload_state();
                            if !expected_states.contains(&current_state) {
                                return Ok(ProcessingUpdateOutcome::StateMismatch);
                            }

                            return Ok(ProcessingUpdateOutcome::Updated(
                                PersistedClipboardRepresentation::new_with_state(
                                    rep.id.clone(),
                                    rep.format_id.clone(),
                                    rep.mime_type.clone(),
                                    rep.size_bytes,
                                    rep.inline_data.clone(),
                                    blob_id.cloned(),
                                    new_state,
                                    last_error.map(|value| value.to_string()),
                                )?,
                            ));
                        }
                    }
                    Ok(ProcessingUpdateOutcome::NotFound)
                },
            );
        }

        (repo, store)
    }

    fn store_representation(
        store: &RepresentationStore,
        event_id: &EventId,
        rep: PersistedClipboardRepresentation,
    ) {
        store
            .lock()
            .expect("representation store poisoned")
            .insert((event_id.clone(), rep.id.clone()), rep);
    }

    fn make_encryption(should_fail_decrypt: bool) -> MockEncryption {
        let mut encryption = MockEncryption::new();
        encryption
            .expect_decrypt_blob()
            .returning(move |_, blob, _| {
                if should_fail_decrypt {
                    return Err(EncryptionError::CorruptedBlob);
                }
                Ok(blob.ciphertext.clone())
            });
        encryption
    }

    fn make_session_with_master_key(master_key: MasterKey) -> MockEncryptionSession {
        let mut session = MockEncryptionSession::new();
        session
            .expect_get_master_key()
            .returning(move || Ok(master_key.clone()));
        session
    }

    fn make_locked_session() -> MockEncryptionSession {
        let mut session = MockEncryptionSession::new();
        session
            .expect_get_master_key()
            .returning(|| Err(EncryptionError::Locked));
        session
    }

    /// Creates an encrypted representation for testing
    fn create_encrypted_representation(
        rep_id: RepresentationId,
        plaintext: &[u8],
    ) -> PersistedClipboardRepresentation {
        let encrypted_blob = EncryptedBlob {
            version: EncryptionFormatVersion::V1,
            aead: EncryptionAlgo::XChaCha20Poly1305,
            nonce: vec![0u8; 24],
            ciphertext: plaintext.to_vec(),
            aad_fingerprint: None,
        };
        let encrypted_bytes = serde_json::to_vec(&encrypted_blob).unwrap();

        PersistedClipboardRepresentation::new(
            rep_id,
            FormatId::from("public.utf8-plain-text"),
            Some(MimeType("text/plain".to_string())),
            plaintext.len() as i64,
            Some(encrypted_bytes),
            None,
        )
    }

    #[tokio::test]
    async fn test_decrypting_repo_decrypts_inline_data() {
        // Test that inline data is decrypted when retrieved
        let (inner, store) = make_representation_repo_with_store();
        let encryption = make_encryption(false);
        let session = make_session_with_master_key(MasterKey::from_bytes(&[0u8; 32]).unwrap());

        let repo = DecryptingClipboardRepresentationRepository::new(
            Arc::new(inner),
            Arc::new(encryption),
            Arc::new(session),
        );

        let event_id = EventId::new();
        let rep_id = RepresentationId::new();
        let plaintext = b"test plaintext data";

        // Store an encrypted representation
        store_representation(
            &store,
            &event_id,
            create_encrypted_representation(rep_id.clone(), plaintext),
        );

        // Retrieve it - should be decrypted
        let result = repo.get_representation(&event_id, &rep_id).await;

        assert!(result.is_ok(), "get_representation should succeed");
        let rep_opt = result.unwrap();
        assert!(rep_opt.is_some(), "representation should exist");

        let rep = rep_opt.unwrap();
        assert_eq!(
            rep.inline_data,
            Some(plaintext.to_vec()),
            "inline data should be decrypted"
        );
    }

    #[tokio::test]
    async fn test_decrypting_repo_preserves_representation_without_inline_data() {
        // Test that representations without inline data are passed through unchanged
        let (inner, store) = make_representation_repo_with_store();
        let encryption = make_encryption(false);
        let session = make_session_with_master_key(MasterKey::from_bytes(&[0u8; 32]).unwrap());

        let repo = DecryptingClipboardRepresentationRepository::new(
            Arc::new(inner),
            Arc::new(encryption),
            Arc::new(session),
        );

        let event_id = EventId::new();
        let rep_id = RepresentationId::new();

        // Store a representation without inline data
        let rep = PersistedClipboardRepresentation::new(
            rep_id.clone(),
            FormatId::from("public.png"),
            Some(MimeType("image/png".to_string())),
            0,
            None,
            Some(BlobId::from("blob-123")),
        );
        store_representation(&store, &event_id, rep);

        // Retrieve it - should be unchanged
        let result = repo.get_representation(&event_id, &rep_id).await;

        assert!(result.is_ok(), "get_representation should succeed");
        let rep_opt = result.unwrap();
        assert!(rep_opt.is_some(), "representation should exist");

        let retrieved_rep = rep_opt.unwrap();
        assert!(
            retrieved_rep.inline_data.is_none(),
            "inline data should remain None"
        );
        assert_eq!(retrieved_rep.blob_id, Some(BlobId::from("blob-123")));
    }

    #[tokio::test]
    async fn test_decrypting_repo_returns_none_for_missing_representation() {
        // Test that None is returned for non-existent representations
        let (inner, _store) = make_representation_repo_with_store();
        let encryption = make_encryption(false);
        let session = make_session_with_master_key(MasterKey::from_bytes(&[0u8; 32]).unwrap());

        let repo = DecryptingClipboardRepresentationRepository::new(
            Arc::new(inner),
            Arc::new(encryption),
            Arc::new(session),
        );

        let event_id = EventId::new();
        let rep_id = RepresentationId::new();

        let result = repo.get_representation(&event_id, &rep_id).await;

        assert!(result.is_ok(), "get_representation should succeed");
        assert!(result.unwrap().is_none(), "representation should not exist");
    }

    #[tokio::test]
    async fn test_decrypting_repo_fails_when_session_not_ready() {
        // Test that an error is returned when the encryption session is not ready
        let (inner, store) = make_representation_repo_with_store();
        let encryption = make_encryption(false);
        let session = make_locked_session();

        let repo = DecryptingClipboardRepresentationRepository::new(
            Arc::new(inner),
            Arc::new(encryption),
            Arc::new(session),
        );

        let event_id = EventId::new();
        let rep_id = RepresentationId::new();
        let plaintext = b"test data";

        // Store an encrypted representation
        store_representation(
            &store,
            &event_id,
            create_encrypted_representation(rep_id.clone(), plaintext),
        );

        // Try to retrieve it - should fail
        let result = repo.get_representation(&event_id, &rep_id).await;

        assert!(
            result.is_err(),
            "get_representation should fail when session not ready"
        );
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("encryption session not ready"),
            "error should indicate session not ready: {}",
            err_msg
        );
    }

    #[tokio::test]
    async fn test_decrypting_repo_delegates_update_blob_id() {
        // Test that update_blob_id is delegated without modification
        let (inner, _store) = make_representation_repo_with_store();
        let encryption = make_encryption(false);
        let session = make_locked_session();

        let repo = DecryptingClipboardRepresentationRepository::new(
            Arc::new(inner),
            Arc::new(encryption),
            Arc::new(session),
        );

        let rep_id = RepresentationId::new();
        let blob_id = BlobId::from("new-blob");

        let result = repo.update_blob_id(&rep_id, &blob_id).await;

        assert!(result.is_ok(), "update_blob_id should succeed");
    }

    #[tokio::test]
    async fn test_aad_generation_is_deterministic() {
        // Test that AAD generation is deterministic for same event and rep
        let event_id = EventId::from("test-event-id");
        let rep_id = RepresentationId::from("test-rep-id");

        let aad1 = aad::for_inline(&event_id, &rep_id);
        let aad2 = aad::for_inline(&event_id, &rep_id);

        assert_eq!(aad1, aad2, "AAD should be deterministic for same inputs");

        // Different event ID should produce different AAD
        let different_event_id = EventId::from("different-event-id");
        let aad3 = aad::for_inline(&different_event_id, &rep_id);
        assert_ne!(aad1, aad3, "AAD should differ for different event IDs");
    }
}
