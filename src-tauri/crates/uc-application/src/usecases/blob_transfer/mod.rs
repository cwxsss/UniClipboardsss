//! Blob 发布 / 拉取用例。
//!
//! 这里处理应用层编排:明文 hash 去重、业务 blob 加解密、iroh-blobs
//! 发布/拉取、引用标签登记。外部调用方只通过 facade 进入。

mod fetch_blob;
mod publish_blob;

pub(crate) use fetch_blob::{FetchBlobInput, FetchBlobUseCase};
pub(crate) use publish_blob::{PublishBlobInput, PublishBlobUseCase};

fn aad_for_entry(_entry_id: &uc_core::ids::EntryId) -> uc_core::crypto::domain::Aad {
    // blob 内容按明文 hash 去重,同一密文可能归属多个剪贴板记录。
    // `entry_id` 只负责 tag 归属,不能进入加密上下文。
    uc_core::crypto::domain::Aad::from(&b"uniclipboard:blob:v1"[..])
}

fn active_space_placeholder() -> uc_core::crypto::domain::ActiveSpace {
    uc_core::crypto::domain::ActiveSpace::new(uc_core::ids::SpaceId::from("space"))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use bytes::Bytes;
    use uc_core::crypto::domain::{Aad, ActiveSpace, Ciphertext, Plaintext};
    use uc_core::ids::EntryId;
    use uc_core::ports::blob::{
        BlobDigest, BlobError, BlobReferenceError, BlobReferenceRepositoryPort, BlobTicket,
        BlobTransferPort, PlaintextHash, TagReason,
    };
    use uc_core::ports::security::{BlobCipherError, BlobCipherPort};
    use uc_core::ports::ContentHashPort;
    use uc_core::{ContentHash, HashAlgorithm};

    use super::aad_for_entry;
    use super::{FetchBlobInput, FetchBlobUseCase, PublishBlobInput, PublishBlobUseCase};

    #[tokio::test]
    async fn publish_reuses_existing_digest_for_repeated_plaintext() {
        let hash = Arc::new(FakeHash);
        let cipher = Arc::new(FakeBlobCipher::default());
        let transfer = Arc::new(FakeBlobTransfer::default());
        let reference = Arc::new(FakeBlobReferenceRepository::default());
        let usecase =
            PublishBlobUseCase::new(hash, cipher.clone(), transfer.clone(), reference.clone());

        let plaintext = Bytes::from_static(b"same file bytes");
        let mut first_digest = None;
        for _ in 0..10 {
            let outcome = usecase
                .execute(PublishBlobInput {
                    plaintext: plaintext.clone(),
                    entry_id: EntryId::new(),
                })
                .await
                .expect("publish should succeed");
            if let Some(first_digest) = first_digest {
                assert_eq!(outcome.digest, first_digest);
                assert!(outcome.reused_existing);
            } else {
                first_digest = Some(outcome.digest);
                assert!(!outcome.reused_existing);
            }
        }

        assert_eq!(cipher.encrypt_count(), 1);
        assert_eq!(transfer.publish_count(), 1);
        assert_eq!(transfer.tag_count(), 10);
        assert_eq!(reference.save_count(), 1);
    }

    #[tokio::test]
    async fn fetch_reused_digest_with_new_entry_id_decrypts() {
        let hash = Arc::new(FakeHash);
        let cipher = Arc::new(FakeBlobCipher::default());
        let transfer = Arc::new(FakeBlobTransfer::default());
        let reference = Arc::new(FakeBlobReferenceRepository::default());
        let publish = PublishBlobUseCase::new(
            hash.clone(),
            cipher.clone(),
            transfer.clone(),
            reference.clone(),
        );
        let fetch = FetchBlobUseCase::new(hash, cipher, transfer, reference);

        let plaintext = Bytes::from_static(b"same file bytes");
        let first = publish
            .execute(PublishBlobInput {
                plaintext: plaintext.clone(),
                entry_id: EntryId::from("entry-one"),
            })
            .await
            .expect("first publish should succeed");
        let second = publish
            .execute(PublishBlobInput {
                plaintext: plaintext.clone(),
                entry_id: EntryId::from("entry-two"),
            })
            .await
            .expect("second publish should reuse digest");

        assert_eq!(first.digest, second.digest);
        assert!(second.reused_existing);

        let outcome = fetch
            .execute(FetchBlobInput {
                ticket: second.ticket,
                entry_id: EntryId::from("entry-two"),
            })
            .await
            .expect("reused digest should decrypt for the new entry tag");

        assert_eq!(outcome.plaintext, plaintext);
        assert_eq!(outcome.entry_id, EntryId::from("entry-two"));
    }

    #[tokio::test]
    async fn fetch_decrypts_saves_reference_and_tags_entry() {
        let hash = Arc::new(FakeHash);
        let cipher = Arc::new(FakeBlobCipher::default());
        let transfer = Arc::new(FakeBlobTransfer::default());
        let reference = Arc::new(FakeBlobReferenceRepository::default());
        let usecase = FetchBlobUseCase::new(
            hash.clone(),
            cipher.clone(),
            transfer.clone(),
            reference.clone(),
        );

        let entry_id = EntryId::new();
        let plaintext = Bytes::from_static(b"remote blob bytes");
        let aad = aad_for_entry(&entry_id);
        let ciphertext = cipher_bytes(&plaintext, &aad);
        let digest = transfer
            .publish(Bytes::from(ciphertext))
            .await
            .expect("seed publish should succeed");
        let ticket = transfer
            .issue_ticket(&digest)
            .await
            .expect("seed ticket should succeed");

        let outcome = usecase
            .execute(FetchBlobInput {
                ticket,
                entry_id: entry_id.clone(),
            })
            .await
            .expect("fetch should succeed");

        assert_eq!(outcome.plaintext, plaintext);
        assert_eq!(outcome.entry_id, entry_id);
        assert_eq!(outcome.digest, digest);
        assert_eq!(
            reference
                .find_by_plaintext_hash(&outcome.plaintext_hash)
                .await
                .expect("reference lookup should succeed"),
            Some(digest)
        );
        assert_eq!(transfer.tag_count(), 1);
    }

    #[derive(Default)]
    struct FakeHash;

    impl ContentHashPort for FakeHash {
        fn hash_bytes(&self, bytes: &[u8]) -> anyhow::Result<ContentHash> {
            Ok(ContentHash {
                alg: HashAlgorithm::Blake3V1,
                bytes: *blake3::hash(bytes).as_bytes(),
            })
        }
    }

    #[derive(Default)]
    struct FakeBlobCipher {
        encrypt_count: Mutex<usize>,
    }

    impl FakeBlobCipher {
        fn encrypt_count(&self) -> usize {
            *self.encrypt_count.lock().expect("lock encrypt count")
        }
    }

    #[async_trait]
    impl BlobCipherPort for FakeBlobCipher {
        async fn encrypt(
            &self,
            _space: &ActiveSpace,
            plaintext: &Plaintext,
            aad: &Aad,
        ) -> Result<Ciphertext, BlobCipherError> {
            *self.encrypt_count.lock().expect("lock encrypt count") += 1;
            Ok(Ciphertext::new(cipher_bytes(plaintext.as_bytes(), aad)))
        }

        async fn decrypt(
            &self,
            _space: &ActiveSpace,
            ciphertext: &Ciphertext,
            aad: &Aad,
        ) -> Result<Plaintext, BlobCipherError> {
            let prefix = cipher_prefix(aad);
            let bytes = ciphertext.as_bytes();
            if !bytes.starts_with(&prefix) {
                return Err(BlobCipherError::InvalidCiphertext);
            }
            Ok(Plaintext::new(bytes[prefix.len()..].to_vec()))
        }
    }

    #[derive(Default)]
    struct FakeBlobTransfer {
        store: Mutex<HashMap<BlobDigest, Bytes>>,
        tags: Mutex<Vec<(BlobDigest, TagReason)>>,
        publish_count: Mutex<usize>,
    }

    impl FakeBlobTransfer {
        fn publish_count(&self) -> usize {
            *self.publish_count.lock().expect("lock publish count")
        }

        fn tag_count(&self) -> usize {
            self.tags.lock().expect("lock tags").len()
        }
    }

    #[async_trait]
    impl BlobTransferPort for FakeBlobTransfer {
        async fn publish(&self, ciphertext: Bytes) -> Result<BlobDigest, BlobError> {
            *self.publish_count.lock().expect("lock publish count") += 1;
            let digest = digest_for(&ciphertext);
            self.store
                .lock()
                .expect("lock store")
                .insert(digest, ciphertext);
            Ok(digest)
        }

        async fn issue_ticket(&self, digest: &BlobDigest) -> Result<BlobTicket, BlobError> {
            Ok(BlobTicket::from_bytes(digest.as_bytes().to_vec()))
        }

        async fn fetch(&self, ticket: &BlobTicket) -> Result<Bytes, BlobError> {
            let digest = self.digest_of(ticket)?;
            self.store
                .lock()
                .expect("lock store")
                .get(&digest)
                .cloned()
                .ok_or(BlobError::NotFound)
        }

        async fn has(&self, digest: &BlobDigest) -> Result<bool, BlobError> {
            Ok(self.store.lock().expect("lock store").contains_key(digest))
        }

        async fn tag(&self, digest: &BlobDigest, reason: TagReason) -> Result<(), BlobError> {
            self.tags.lock().expect("lock tags").push((*digest, reason));
            Ok(())
        }

        async fn untag(&self, digest: &BlobDigest, reason: TagReason) -> Result<(), BlobError> {
            self.tags
                .lock()
                .expect("lock tags")
                .retain(|(d, r)| d != digest || r != &reason);
            Ok(())
        }

        fn digest_of(&self, ticket: &BlobTicket) -> Result<BlobDigest, BlobError> {
            let bytes: [u8; 32] = ticket
                .as_bytes()
                .try_into()
                .map_err(|_| BlobError::InvalidTicket)?;
            Ok(BlobDigest::from_bytes(bytes))
        }
    }

    #[derive(Default)]
    struct FakeBlobReferenceRepository {
        rows: Mutex<HashMap<PlaintextHash, BlobDigest>>,
        save_count: Mutex<usize>,
    }

    impl FakeBlobReferenceRepository {
        fn save_count(&self) -> usize {
            *self.save_count.lock().expect("lock save count")
        }
    }

    #[async_trait]
    impl BlobReferenceRepositoryPort for FakeBlobReferenceRepository {
        async fn find_by_plaintext_hash(
            &self,
            hash: &PlaintextHash,
        ) -> Result<Option<BlobDigest>, BlobReferenceError> {
            Ok(self.rows.lock().expect("lock rows").get(hash).copied())
        }

        async fn save(
            &self,
            hash: PlaintextHash,
            digest: BlobDigest,
        ) -> Result<(), BlobReferenceError> {
            *self.save_count.lock().expect("lock save count") += 1;
            self.rows.lock().expect("lock rows").insert(hash, digest);
            Ok(())
        }

        async fn forget(&self, hash: &PlaintextHash) -> Result<(), BlobReferenceError> {
            self.rows.lock().expect("lock rows").remove(hash);
            Ok(())
        }
    }

    fn cipher_bytes(plaintext: &[u8], aad: &Aad) -> Vec<u8> {
        let mut bytes = cipher_prefix(aad);
        bytes.extend_from_slice(plaintext);
        bytes
    }

    fn cipher_prefix(aad: &Aad) -> Vec<u8> {
        let mut prefix = b"fake-blob-cipher:".to_vec();
        prefix.extend_from_slice(aad.as_bytes());
        prefix.push(0);
        prefix
    }

    fn digest_for(bytes: &[u8]) -> BlobDigest {
        BlobDigest::from_bytes(*blake3::hash(bytes).as_bytes())
    }
}
