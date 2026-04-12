//! Decrypting clipboard event repository decorator.
//!
//! Wraps ClipboardEventRepositoryPort and decrypts ObservedClipboardRepresentation.bytes on read.

use anyhow::{Context, Result};
use async_trait::async_trait;
use std::sync::Arc;
use tracing::trace;

use uc_core::{
    clipboard::ObservedClipboardRepresentation,
    ids::{EventId, RepresentationId},
    ports::{ClipboardEventRepositoryPort, EncryptionPort, EncryptionSessionPort},
    security::aad,
    security::model::EncryptedBlob,
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

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use mockall::mock;
    use std::sync::Arc;
    use uc_core::{
        clipboard::ObservedClipboardRepresentation,
        ids::{EventId, RepresentationId},
        ports::{ClipboardEventRepositoryPort, EncryptionPort, EncryptionSessionPort},
        security::aad,
        security::model::{
            EncryptedBlob, EncryptionAlgo, EncryptionError, EncryptionFormatVersion, KdfParams,
            Kek, MasterKey, Passphrase,
        },
    };

    mock! {
        EventRepo {}

        #[async_trait]
        impl ClipboardEventRepositoryPort for EventRepo {
            async fn get_representation(
                &self,
                id: &EventId,
                representation_id: &str,
            ) -> Result<ObservedClipboardRepresentation>;
        }
    }

    mock! {
        Encryption {}

        #[async_trait]
        impl EncryptionPort for Encryption {
            async fn derive_kek(
                &self,
                passphrase: &Passphrase,
                salt: &[u8],
                kdf: &KdfParams,
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
                wrapped: &EncryptedBlob,
            ) -> Result<MasterKey, EncryptionError>;
            async fn encrypt_blob(
                &self,
                master_key: &MasterKey,
                plaintext: &[u8],
                aad: &[u8],
                aead: EncryptionAlgo,
            ) -> Result<EncryptedBlob, EncryptionError>;
            async fn decrypt_blob(
                &self,
                master_key: &MasterKey,
                encrypted: &EncryptedBlob,
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

    fn make_passthrough_encryption() -> MockEncryption {
        let mut encryption = MockEncryption::new();
        encryption
            .expect_decrypt_blob()
            .returning(|_, blob, _| Ok(blob.ciphertext.clone()));
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

    /// Creates an encrypted representation for testing
    fn create_encrypted_observed_representation(
        plaintext: &[u8],
    ) -> ObservedClipboardRepresentation {
        let encrypted_blob = EncryptedBlob {
            version: EncryptionFormatVersion::V1,
            aead: EncryptionAlgo::XChaCha20Poly1305,
            nonce: vec![0u8; 24],
            ciphertext: plaintext.to_vec(),
            aad_fingerprint: None,
        };
        let encrypted_bytes = serde_json::to_vec(&encrypted_blob).unwrap();

        ObservedClipboardRepresentation::new(
            uc_core::ids::RepresentationId::from("test-rep"),
            uc_core::ids::FormatId::from("public.utf8-plain-text"),
            Some(uc_core::clipboard::MimeType("text/plain".to_string())),
            encrypted_bytes,
        )
    }

    /// Creates an unencrypted representation for testing
    fn create_unencrypted_observed_representation(
        plaintext: &[u8],
    ) -> ObservedClipboardRepresentation {
        ObservedClipboardRepresentation::new(
            uc_core::ids::RepresentationId::from("test-rep"),
            uc_core::ids::FormatId::from("public.utf8-plain-text"),
            Some(uc_core::clipboard::MimeType("text/plain".to_string())),
            plaintext.to_vec(),
        )
    }

    #[tokio::test]
    async fn test_decrypting_repo_decrypts_bytes() {
        // Test that bytes are decrypted when retrieved
        let event_id = EventId::new();
        let rep_id = String::from("test-rep");
        let plaintext = b"test plaintext data";
        let stored_representation = create_encrypted_observed_representation(plaintext);

        let mut inner = MockEventRepo::new();
        let expected_event_id = event_id.clone();
        let expected_rep_id = rep_id.clone();
        inner
            .expect_get_representation()
            .withf(move |id, rid| id == &expected_event_id && rid == expected_rep_id)
            .once()
            .return_once(move |_, _| Ok(stored_representation));

        let encryption = make_passthrough_encryption();
        let session =
            make_session_with_master_key(MasterKey::from_bytes(&[0u8; 32]).expect("valid key"));
        let repo = DecryptingClipboardEventRepository::new(
            Arc::new(inner),
            Arc::new(encryption),
            Arc::new(session),
        );

        // Retrieve it - should be decrypted
        let result = repo.get_representation(&event_id, &rep_id).await;

        assert!(result.is_ok(), "get_representation should succeed");
        let observed = result.unwrap();
        assert_eq!(
            observed.bytes,
            plaintext.to_vec(),
            "bytes should be decrypted"
        );
    }

    #[tokio::test]
    async fn test_decrypting_repo_fails_for_unencrypted_data() {
        // Test that unencrypted data causes an error
        let event_id = EventId::new();
        let rep_id = String::from("test-rep");
        let plaintext = b"test data";
        let stored_representation = create_unencrypted_observed_representation(plaintext);

        let mut inner = MockEventRepo::new();
        let expected_event_id = event_id.clone();
        let expected_rep_id = rep_id.clone();
        inner
            .expect_get_representation()
            .withf(move |id, rid| id == &expected_event_id && rid == expected_rep_id)
            .once()
            .return_once(move |_, _| Ok(stored_representation));

        let encryption = MockEncryption::new();
        let session = MockEncryptionSession::new();
        let repo = DecryptingClipboardEventRepository::new(
            Arc::new(inner),
            Arc::new(encryption),
            Arc::new(session),
        );

        // Try to retrieve it - should fail
        let result = repo.get_representation(&event_id, &rep_id).await;

        assert!(
            result.is_err(),
            "get_representation should fail for unencrypted data"
        );
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("not in encrypted format"),
            "error should indicate data is not encrypted: {}",
            err_msg
        );
    }

    #[tokio::test]
    async fn test_decrypting_repo_fails_when_session_not_ready() {
        // Test that an error is returned when the encryption session is not ready
        let event_id = EventId::new();
        let rep_id = String::from("test-rep");
        let plaintext = b"test data";
        let stored_representation = create_encrypted_observed_representation(plaintext);

        let mut inner = MockEventRepo::new();
        let expected_event_id = event_id.clone();
        let expected_rep_id = rep_id.clone();
        inner
            .expect_get_representation()
            .withf(move |id, rid| id == &expected_event_id && rid == expected_rep_id)
            .once()
            .return_once(move |_, _| Ok(stored_representation));

        let mut session = MockEncryptionSession::new();
        session
            .expect_get_master_key()
            .once()
            .return_once(|| Err(EncryptionError::Locked));
        let encryption = MockEncryption::new();
        let repo = DecryptingClipboardEventRepository::new(
            Arc::new(inner),
            Arc::new(encryption),
            Arc::new(session),
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
