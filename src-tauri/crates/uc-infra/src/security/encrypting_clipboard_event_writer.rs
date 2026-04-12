//! Encrypting clipboard event writer decorator.
//!
//! Wraps ClipboardEventWriterPort and encrypts inline_data before storage.

use anyhow::{Context, Result};
use async_trait::async_trait;
use std::sync::Arc;
use tracing::{debug, trace};

use uc_core::{
    clipboard::{ClipboardEvent, PersistedClipboardRepresentation},
    ids::EventId,
    ports::{ClipboardEventWriterPort, EncryptionPort, EncryptionSessionPort},
    security::aad,
    security::model::EncryptionAlgo,
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

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use mockall::mock;
    use std::sync::{Arc, Mutex};
    use uc_core::{
        clipboard::{ClipboardEvent, MimeType, PersistedClipboardRepresentation, SnapshotHash},
        ids::{BlobId, DeviceId, EventId, FormatId, RepresentationId},
        security::aad,
        security::model::{EncryptedBlob, EncryptionError, EncryptionFormatVersion, MasterKey},
        ContentHash,
    };

    mock! {
        EventWriter {}

        #[async_trait]
        impl ClipboardEventWriterPort for EventWriter {
            async fn insert_event(
                &self,
                event: &ClipboardEvent,
                representations: &Vec<PersistedClipboardRepresentation>,
            ) -> Result<()>;
            async fn delete_event_and_representations(&self, event_id: &EventId) -> Result<()>;
        }
    }

    mock! {
        Encryption {}

        #[async_trait]
        impl uc_core::ports::EncryptionPort for Encryption {
            async fn derive_kek(
                &self,
                passphrase: &uc_core::security::model::Passphrase,
                salt: &[u8],
                kdf_params: &uc_core::security::model::KdfParams,
            ) -> Result<uc_core::security::model::Kek, EncryptionError>;
            async fn wrap_master_key(
                &self,
                kek: &uc_core::security::model::Kek,
                master_key: &MasterKey,
                aead: uc_core::security::model::EncryptionAlgo,
            ) -> Result<EncryptedBlob, EncryptionError>;
            async fn unwrap_master_key(
                &self,
                kek: &uc_core::security::model::Kek,
                blob: &EncryptedBlob,
            ) -> Result<MasterKey, EncryptionError>;
            async fn encrypt_blob(
                &self,
                master_key: &MasterKey,
                plaintext: &[u8],
                aad: &[u8],
                algo: uc_core::security::model::EncryptionAlgo,
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

        #[async_trait]
        impl EncryptionSessionPort for EncryptionSession {
            async fn is_ready(&self) -> bool;
            async fn get_master_key(&self) -> Result<MasterKey, EncryptionError>;
            async fn set_master_key(&self, master_key: MasterKey) -> Result<(), EncryptionError>;
            async fn clear(&self) -> Result<(), EncryptionError>;
        }
    }

    fn make_event_writer_with_capture(
        inserted_reps: Arc<Mutex<Vec<PersistedClipboardRepresentation>>>,
        deleted_event_ids: Arc<Mutex<Vec<EventId>>>,
    ) -> MockEventWriter {
        let mut writer = MockEventWriter::new();

        let inserted_reps_capture = inserted_reps.clone();
        writer.expect_insert_event().returning(move |_, reps| {
            inserted_reps_capture.lock().unwrap().extend(reps.clone());
            Ok(())
        });

        writer
            .expect_delete_event_and_representations()
            .returning(move |event_id| {
                deleted_event_ids.lock().unwrap().push(event_id.clone());
                Ok(())
            });

        writer
    }

    fn make_encryption(should_fail: bool) -> MockEncryption {
        let mut encryption = MockEncryption::new();
        encryption
            .expect_encrypt_blob()
            .returning(move |_, plaintext, _, _| {
                if should_fail {
                    return Err(EncryptionError::EncryptFailed);
                }
                Ok(EncryptedBlob {
                    version: EncryptionFormatVersion::V1,
                    aead: uc_core::security::model::EncryptionAlgo::XChaCha20Poly1305,
                    nonce: vec![0u8; 24],
                    ciphertext: plaintext.to_vec(),
                    aad_fingerprint: None,
                })
            });
        encryption
    }

    fn make_session_with_master_key(master_key: MasterKey) -> MockEncryptionSession {
        let mut session = MockEncryptionSession::new();
        session
            .expect_get_master_key()
            .once()
            .return_once(move || Ok(master_key));
        session
    }

    fn make_locked_session() -> MockEncryptionSession {
        let mut session = MockEncryptionSession::new();
        session
            .expect_get_master_key()
            .once()
            .return_once(|| Err(EncryptionError::Locked));
        session
    }

    /// Creates a test clipboard event
    fn create_test_event() -> ClipboardEvent {
        let content_hash = ContentHash::from(&[0u8; 32]);
        ClipboardEvent {
            event_id: EventId::new(),
            captured_at_ms: 12345,
            source_device: DeviceId::new("test-device"),
            snapshot_hash: SnapshotHash(content_hash),
        }
    }

    /// Creates a test representation with inline data
    fn create_test_representation_with_inline_data() -> PersistedClipboardRepresentation {
        PersistedClipboardRepresentation::new(
            RepresentationId::new(),
            FormatId::from("public.utf8-plain-text"),
            Some(MimeType("text/plain".to_string())),
            16,
            Some(b"test plaintext data".to_vec()),
            None,
        )
    }

    /// Creates a test representation without inline data
    fn create_test_representation_without_inline_data() -> PersistedClipboardRepresentation {
        PersistedClipboardRepresentation::new(
            RepresentationId::new(),
            FormatId::from("public.png"),
            Some(MimeType("image/png".to_string())),
            0,
            None,
            Some(BlobId::from("blob-id-123")),
        )
    }

    #[tokio::test]
    async fn test_encrypting_writer_encrypts_inline_data() {
        // Test that inline data is encrypted before being passed to inner writer
        let inserted_reps = Arc::new(Mutex::new(Vec::new()));
        let deleted_event_ids = Arc::new(Mutex::new(Vec::new()));
        let inner = Arc::new(make_event_writer_with_capture(
            inserted_reps.clone(),
            deleted_event_ids,
        ));
        let encryption = Arc::new(make_encryption(false));
        let session = Arc::new(make_session_with_master_key(
            MasterKey::from_bytes(&[0u8; 32]).unwrap(),
        ));

        let writer = EncryptingClipboardEventWriter::new(inner.clone(), encryption, session);

        let event = create_test_event();
        let representations = vec![create_test_representation_with_inline_data()];

        let result = writer.insert_event(&event, &representations).await;

        assert!(result.is_ok(), "insert_event should succeed");

        let inserted_reps = inserted_reps.lock().unwrap().clone();
        assert_eq!(
            inserted_reps.len(),
            1,
            "should have inserted one representation"
        );

        let inserted_rep = &inserted_reps[0];
        assert!(
            inserted_rep.inline_data.is_some(),
            "should have inline data"
        );

        // Verify the inline data is an encrypted blob (serializes to JSON with expected fields)
        let encrypted_bytes = inserted_rep.inline_data.as_ref().unwrap();
        let encrypted_blob: EncryptedBlob = serde_json::from_slice(encrypted_bytes)
            .expect("inline data should be a valid encrypted blob");

        assert_eq!(encrypted_blob.version, EncryptionFormatVersion::V1);
        assert_eq!(
            encrypted_blob.aead,
            uc_core::security::model::EncryptionAlgo::XChaCha20Poly1305
        );
        assert_eq!(encrypted_blob.nonce.len(), 24);
        // Ciphertext should contain the original plaintext
        assert_eq!(encrypted_blob.ciphertext, b"test plaintext data".to_vec());
    }

    #[tokio::test]
    async fn test_encrypting_writer_preserves_representation_without_inline_data() {
        // Test that representations without inline data are passed through unchanged
        let inserted_reps = Arc::new(Mutex::new(Vec::new()));
        let deleted_event_ids = Arc::new(Mutex::new(Vec::new()));
        let inner = Arc::new(make_event_writer_with_capture(
            inserted_reps.clone(),
            deleted_event_ids,
        ));
        let encryption = Arc::new(make_encryption(false));
        let session = Arc::new(make_session_with_master_key(
            MasterKey::from_bytes(&[0u8; 32]).unwrap(),
        ));

        let writer = EncryptingClipboardEventWriter::new(inner.clone(), encryption, session);

        let event = create_test_event();
        let representations = vec![create_test_representation_without_inline_data()];

        let result = writer.insert_event(&event, &representations).await;

        assert!(result.is_ok(), "insert_event should succeed");

        let inserted_reps = inserted_reps.lock().unwrap().clone();
        assert_eq!(
            inserted_reps.len(),
            1,
            "should have inserted one representation"
        );

        let inserted_rep = &inserted_reps[0];
        assert!(
            inserted_rep.inline_data.is_none(),
            "should not have inline data"
        );
        assert_eq!(inserted_rep.blob_id, Some(BlobId::from("blob-id-123")));
    }

    #[tokio::test]
    async fn test_encrypting_writer_handles_multiple_representations() {
        // Test that multiple representations are encrypted correctly
        let inserted_reps = Arc::new(Mutex::new(Vec::new()));
        let deleted_event_ids = Arc::new(Mutex::new(Vec::new()));
        let inner = Arc::new(make_event_writer_with_capture(
            inserted_reps.clone(),
            deleted_event_ids,
        ));
        let encryption = Arc::new(make_encryption(false));
        let session = Arc::new(make_session_with_master_key(
            MasterKey::from_bytes(&[0u8; 32]).unwrap(),
        ));

        let writer = EncryptingClipboardEventWriter::new(inner.clone(), encryption, session);

        let event = create_test_event();
        let representations = vec![
            create_test_representation_with_inline_data(),
            create_test_representation_without_inline_data(),
            create_test_representation_with_inline_data(),
        ];

        let result = writer.insert_event(&event, &representations).await;

        assert!(result.is_ok(), "insert_event should succeed");

        let inserted_reps = inserted_reps.lock().unwrap().clone();
        assert_eq!(
            inserted_reps.len(),
            3,
            "should have inserted three representations"
        );

        // First representation should have encrypted inline data
        assert!(inserted_reps[0].inline_data.is_some());

        // Second representation should have no inline data
        assert!(inserted_reps[1].inline_data.is_none());

        // Third representation should have encrypted inline data
        assert!(inserted_reps[2].inline_data.is_some());
    }

    #[tokio::test]
    async fn test_encrypting_writer_fails_when_session_not_ready() {
        // Test that an error is returned when the encryption session is not ready
        let inserted_reps = Arc::new(Mutex::new(Vec::new()));
        let deleted_event_ids = Arc::new(Mutex::new(Vec::new()));
        let inner = Arc::new(make_event_writer_with_capture(
            inserted_reps,
            deleted_event_ids,
        ));
        let encryption = Arc::new(make_encryption(false));
        let session = Arc::new(make_locked_session());

        let writer = EncryptingClipboardEventWriter::new(inner.clone(), encryption, session);

        let event = create_test_event();
        let representations = vec![create_test_representation_with_inline_data()];

        let result = writer.insert_event(&event, &representations).await;

        assert!(
            result.is_err(),
            "insert_event should fail when session not ready"
        );
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("encryption session not ready"),
            "error should indicate session not ready: {}",
            err_msg
        );
    }

    #[tokio::test]
    async fn test_encrypting_writer_propagates_encryption_errors() {
        // Test that encryption errors are propagated
        let inserted_reps = Arc::new(Mutex::new(Vec::new()));
        let deleted_event_ids = Arc::new(Mutex::new(Vec::new()));
        let inner = Arc::new(make_event_writer_with_capture(
            inserted_reps,
            deleted_event_ids,
        ));
        let encryption = Arc::new(make_encryption(true));
        let session = Arc::new(make_session_with_master_key(
            MasterKey::from_bytes(&[0u8; 32]).unwrap(),
        ));

        let writer = EncryptingClipboardEventWriter::new(inner.clone(), encryption, session);

        let event = create_test_event();
        let representations = vec![create_test_representation_with_inline_data()];

        let result = writer.insert_event(&event, &representations).await;

        assert!(
            result.is_err(),
            "insert_event should fail when encryption fails"
        );
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("failed to encrypt inline_data"),
            "error should indicate encryption failure: {}",
            err_msg
        );
    }

    #[tokio::test]
    async fn test_encrypting_writer_delegates_deletion() {
        // Test that deletion is delegated to inner writer without modification
        let inserted_reps = Arc::new(Mutex::new(Vec::new()));
        let deleted_event_ids = Arc::new(Mutex::new(Vec::new()));
        let inner = Arc::new(make_event_writer_with_capture(
            inserted_reps,
            deleted_event_ids.clone(),
        ));
        let encryption = Arc::new(MockEncryption::new());
        let session = Arc::new(MockEncryptionSession::new());

        let writer = EncryptingClipboardEventWriter::new(inner.clone(), encryption, session);

        let event_id = EventId::new();

        let result = writer.delete_event_and_representations(&event_id).await;

        assert!(
            result.is_ok(),
            "delete_event_and_representations should succeed"
        );

        let deleted_ids = deleted_event_ids.lock().unwrap().clone();
        assert_eq!(deleted_ids.len(), 1, "should have deleted one event");
        assert_eq!(deleted_ids[0], event_id, "should delete the correct event");
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

        // Different rep ID should produce different AAD
        let different_rep_id = RepresentationId::from("different-rep-id");
        let aad4 = aad::for_inline(&event_id, &different_rep_id);
        assert_ne!(
            aad1, aad4,
            "AAD should differ for different representation IDs"
        );
    }
}
