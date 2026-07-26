//! Encrypt inbound clipboard payloads before atomic persistence.

use anyhow::{Context, Result};
use async_trait::async_trait;
use std::sync::Arc;

use uc_core::clipboard::PersistedClipboardRepresentation;
use uc_core::crypto::aad;
use uc_core::crypto::domain::{Aad, ActiveSpace, Plaintext};
use uc_core::ids::SpaceId;
use uc_core::ports::security::BlobCipherPort;
use uc_core::ports::{
    CommitInboundReceivePort, InboundReceiveCommitError, InboundReceiveRecord,
    InboundReceiveSettlement,
};

/// Decorates atomic inbound commits so SQLite receives encrypted inline data.
pub struct EncryptingInboundReceiveCommit {
    inner: Arc<dyn CommitInboundReceivePort>,
    blob_cipher: Arc<dyn BlobCipherPort>,
}

impl EncryptingInboundReceiveCommit {
    pub fn new(
        inner: Arc<dyn CommitInboundReceivePort>,
        blob_cipher: Arc<dyn BlobCipherPort>,
    ) -> Self {
        Self { inner, blob_cipher }
    }

    async fn encrypt_representations(
        &self,
        event_id: &uc_core::ids::EventId,
        representations: &[PersistedClipboardRepresentation],
    ) -> Result<Vec<PersistedClipboardRepresentation>> {
        let active = ActiveSpace::new(SpaceId::from("space"));
        let mut encrypted = Vec::with_capacity(representations.len());

        for rep in representations {
            let inline_data = if let Some(plain_bytes) = &rep.inline_data {
                let aad = aad::for_inline(event_id, &rep.id);
                let ciphertext = self
                    .blob_cipher
                    .encrypt(
                        &active,
                        &Plaintext::new(plain_bytes.clone()),
                        &Aad::from(aad.as_slice()),
                    )
                    .await
                    .context("failed to encrypt inbound inline_data")?;
                Some(ciphertext.into_bytes())
            } else {
                None
            };

            encrypted.push(PersistedClipboardRepresentation::new_with_state(
                rep.id.clone(),
                rep.format_id.clone(),
                rep.mime_type.clone(),
                rep.size_bytes,
                inline_data,
                rep.blob_id.clone(),
                rep.payload_state(),
                rep.last_error.clone(),
            )?);
        }

        Ok(encrypted)
    }

    async fn encrypt_record(&self, record: &InboundReceiveRecord) -> Result<InboundReceiveRecord> {
        match record {
            InboundReceiveRecord::Create {
                entry,
                event,
                representations,
                selection,
            } => Ok(InboundReceiveRecord::Create {
                entry: entry.clone(),
                event: event.clone(),
                representations: self
                    .encrypt_representations(&event.event_id, representations)
                    .await?,
                selection: selection.clone(),
            }),
            InboundReceiveRecord::Replace {
                entry_id,
                new_event,
                new_representations,
                new_selection,
                new_total_size,
                new_content_category,
            } => Ok(InboundReceiveRecord::Replace {
                entry_id: entry_id.clone(),
                new_event: new_event.clone(),
                new_representations: self
                    .encrypt_representations(&new_event.event_id, new_representations)
                    .await?,
                new_selection: new_selection.clone(),
                new_total_size: *new_total_size,
                new_content_category: *new_content_category,
            }),
        }
    }
}

#[async_trait]
impl CommitInboundReceivePort for EncryptingInboundReceiveCommit {
    async fn commit_inbound_receive(
        &self,
        settlement: &InboundReceiveSettlement,
    ) -> Result<(), InboundReceiveCommitError> {
        let encrypted = match settlement {
            InboundReceiveSettlement::Complete {
                record,
                attempt_id,
                file_set,
                artifacts,
                now_ms,
            } => InboundReceiveSettlement::Complete {
                record: self
                    .encrypt_record(record)
                    .await
                    .map_err(|error| InboundReceiveCommitError::Backend(error.to_string()))?,
                attempt_id: attempt_id.clone(),
                file_set: file_set.clone(),
                artifacts: *artifacts,
                now_ms: *now_ms,
            },
            InboundReceiveSettlement::Partial {
                record,
                attempt_id,
                terminal,
                file_set,
                artifacts,
                now_ms,
            } => InboundReceiveSettlement::Partial {
                record: self
                    .encrypt_record(record)
                    .await
                    .map_err(|error| InboundReceiveCommitError::Backend(error.to_string()))?,
                attempt_id: attempt_id.clone(),
                terminal: *terminal,
                file_set: file_set.clone(),
                artifacts: *artifacts,
                now_ms: *now_ms,
            },
            InboundReceiveSettlement::NoEntry {
                entry_id,
                attempt_id,
                terminal,
                artifacts,
                now_ms,
            } => InboundReceiveSettlement::NoEntry {
                entry_id: entry_id.clone(),
                attempt_id: attempt_id.clone(),
                terminal: *terminal,
                artifacts: *artifacts,
                now_ms: *now_ms,
            },
        };

        self.inner.commit_inbound_receive(&encrypted).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::{BlobCipherAdapter, InMemorySession};
    use uc_core::clipboard::MimeType;
    use uc_core::crypto::domain::Ciphertext;
    use uc_core::ids::{EventId, FormatId, RepresentationId};

    #[tokio::test]
    async fn encrypts_inline_data_with_the_event_representation_aad() {
        let session = Arc::new(InMemorySession::new());
        session
            .set_master_key(crate::security::secrets::MasterKey::from_bytes(&[7u8; 32]).unwrap());
        let cipher: Arc<dyn BlobCipherPort> = Arc::new(BlobCipherAdapter::new(session));
        let inner: Arc<dyn CommitInboundReceivePort> = Arc::new(NoopCommit);
        let decorator = EncryptingInboundReceiveCommit::new(inner, cipher.clone());
        let event_id = EventId::from("event-test");
        let rep = PersistedClipboardRepresentation::new(
            RepresentationId::from("rep-test"),
            FormatId::from("text"),
            Some(MimeType("text/plain".into())),
            11,
            Some(b"hello world".to_vec()),
            None,
        );

        let encrypted = decorator
            .encrypt_representations(&event_id, &[rep])
            .await
            .unwrap();
        let aad_bytes = aad::for_inline(&event_id, &encrypted[0].id);
        let plaintext = cipher
            .decrypt(
                &ActiveSpace::new(SpaceId::from("space")),
                &Ciphertext::new(encrypted[0].inline_data.clone().unwrap()),
                &Aad::from(aad_bytes.as_slice()),
            )
            .await
            .unwrap();

        assert_eq!(plaintext.as_bytes(), b"hello world");
    }

    struct NoopCommit;

    #[async_trait]
    impl CommitInboundReceivePort for NoopCommit {
        async fn commit_inbound_receive(
            &self,
            _settlement: &InboundReceiveSettlement,
        ) -> Result<(), InboundReceiveCommitError> {
            Ok(())
        }
    }
}
