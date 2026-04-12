use async_trait::async_trait;
use std::sync::Arc;
use uc_core::{
    ports::{KeyMaterialPort, SecureStoragePort},
    security::model::{EncryptionError, Kek, KeyScope, KeySlot, KeySlotFile},
};

use crate::fs::key_slot_store::KeySlotStore;

pub struct DefaultKeyMaterialService {
    secure_storage: Arc<dyn SecureStoragePort>,
    keyslot_store: Arc<dyn KeySlotStore>,
}

impl DefaultKeyMaterialService {
    /// Create a new key material service
    /// 创建新的密钥材料服务
    pub fn new(
        secure_storage: Arc<dyn SecureStoragePort>,
        keyslot_store: Arc<dyn KeySlotStore>,
    ) -> Self {
        Self {
            secure_storage,
            keyslot_store,
        }
    }
}

fn kek_key(scope: &KeyScope) -> String {
    format!("kek:v1:{}", scope.to_identifier())
}

fn map_storage_error(err: uc_core::ports::SecureStorageError) -> EncryptionError {
    use uc_core::ports::SecureStorageError as StorageError;
    match err {
        StorageError::PermissionDenied(_) => EncryptionError::PermissionDenied,
        StorageError::Corrupt(_) => EncryptionError::KeyMaterialCorrupt,
        StorageError::Unavailable(msg) | StorageError::Other(msg) => {
            EncryptionError::KeyringError(msg)
        }
    }
}

#[async_trait]
impl KeyMaterialPort for DefaultKeyMaterialService {
    async fn load_kek(&self, scope: &KeyScope) -> Result<Kek, EncryptionError> {
        let key = kek_key(scope);
        let secret = self
            .secure_storage
            .get(&key)
            .map_err(map_storage_error)?
            .ok_or(EncryptionError::KeyNotFound)?;
        Kek::from_bytes(&secret)
            .map_err(|e| EncryptionError::KeyringError(format!("invalid KEK material: {e}")))
    }

    async fn store_kek(&self, scope: &KeyScope, kek: &Kek) -> Result<(), EncryptionError> {
        let key = kek_key(scope);
        self.secure_storage
            .set(&key, &kek.0)
            .map_err(map_storage_error)
    }

    async fn delete_kek(&self, scope: &KeyScope) -> Result<(), EncryptionError> {
        let key = kek_key(scope);
        self.secure_storage.delete(&key).map_err(map_storage_error)
    }

    async fn load_keyslot(&self, scope: &KeyScope) -> Result<KeySlot, EncryptionError> {
        let file = self.keyslot_store.load().await?;
        if &file.scope != scope {
            return Err(EncryptionError::KeyMaterialCorrupt);
        }
        Ok(file.into())
    }

    async fn store_keyslot(&self, keyslot: &KeySlot) -> Result<(), EncryptionError> {
        let file = KeySlotFile::try_from(keyslot).map_err(|_| EncryptionError::CorruptedKeySlot)?;
        self.keyslot_store.store(&file).await
    }

    async fn delete_keyslot(&self, scope: &KeyScope) -> Result<(), EncryptionError> {
        let file = self.keyslot_store.load().await?;
        if &file.scope != scope {
            return Err(EncryptionError::KeyMaterialCorrupt);
        }
        self.keyslot_store.delete().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use mockall::mock;
    use std::sync::Arc;
    use uc_core::ports::SecureStorageError;
    use uc_core::security::model::{
        EncryptionAlgo, EncryptionFormatVersion, KdfParams, KeyScope, KeySlotVersion,
        WrappedMasterKey,
    };

    mock! {
        SecureStorage {}

        impl SecureStoragePort for SecureStorage {
            fn get(&self, key: &str) -> Result<Option<Vec<u8>>, SecureStorageError>;
            #[mockall::concretize]
            fn set(&self, key: &str, value: &[u8]) -> Result<(), SecureStorageError>;
            fn delete(&self, key: &str) -> Result<(), SecureStorageError>;
        }
    }

    mock! {
        KeySlotStore {}

        #[async_trait]
        impl KeySlotStore for KeySlotStore {
            async fn load(&self) -> Result<KeySlotFile, EncryptionError>;
            #[mockall::concretize]
            async fn store(&self, slot: &KeySlotFile) -> Result<(), EncryptionError>;
            async fn delete(&self) -> Result<(), EncryptionError>;
        }
    }

    fn sample_scope(profile_id: &str) -> KeyScope {
        KeyScope {
            profile_id: profile_id.to_string(),
        }
    }

    fn sample_kek() -> Kek {
        Kek([7u8; 32])
    }

    fn sample_keyslot(scope: KeyScope) -> KeySlot {
        KeySlot {
            version: KeySlotVersion::V1,
            scope,
            kdf: KdfParams::for_initialization(),
            salt: vec![1u8; 32],
            wrapped_master_key: Some(WrappedMasterKey {
                blob: uc_core::security::model::EncryptedBlob {
                    version: EncryptionFormatVersion::V1,
                    aead: EncryptionAlgo::XChaCha20Poly1305,
                    nonce: vec![1u8; 24],
                    ciphertext: vec![2u8; 32],
                    aad_fingerprint: None,
                },
            }),
        }
    }

    #[tokio::test]
    async fn load_kek_reads_from_secure_storage() {
        let scope = sample_scope("profile-1");
        let kek = sample_kek();
        let key = kek_key(&scope);

        let mut storage = MockSecureStorage::new();
        let expected_key = key.clone();
        storage
            .expect_get()
            .withf(move |k| k == expected_key)
            .once()
            .return_once(move |_| Ok(Some(kek.0.to_vec())));

        let keyslot_store = MockKeySlotStore::new();
        let service = DefaultKeyMaterialService::new(
            Arc::new(storage) as Arc<dyn SecureStoragePort>,
            Arc::new(keyslot_store) as Arc<dyn KeySlotStore>,
        );

        let loaded = service.load_kek(&scope).await.expect("load kek");
        assert_eq!(loaded, kek);
    }

    #[tokio::test]
    async fn store_kek_writes_to_secure_storage() {
        let scope = sample_scope("profile-2");
        let kek = sample_kek();
        let key = kek_key(&scope);

        let mut storage = MockSecureStorage::new();
        let expected_key = key.clone();
        storage
            .expect_set()
            .withf(move |k, v| k == expected_key && *v == kek.0)
            .once()
            .return_once(|_, _| Ok(()));

        let keyslot_store = MockKeySlotStore::new();
        let service = DefaultKeyMaterialService::new(
            Arc::new(storage) as Arc<dyn SecureStoragePort>,
            Arc::new(keyslot_store) as Arc<dyn KeySlotStore>,
        );

        service.store_kek(&scope, &kek).await.expect("store kek");
    }

    #[tokio::test]
    async fn delete_kek_writes_to_secure_storage() {
        let scope = sample_scope("profile-3");
        let key = kek_key(&scope);

        let mut storage = MockSecureStorage::new();
        let expected_key = key.clone();
        storage
            .expect_delete()
            .withf(move |k| k == expected_key)
            .once()
            .return_once(|_| Ok(()));

        let keyslot_store = MockKeySlotStore::new();
        let service = DefaultKeyMaterialService::new(
            Arc::new(storage) as Arc<dyn SecureStoragePort>,
            Arc::new(keyslot_store) as Arc<dyn KeySlotStore>,
        );

        service.delete_kek(&scope).await.expect("delete kek");
    }

    #[tokio::test]
    async fn load_keyslot_rejects_scope_mismatch() {
        let scope = sample_scope("profile-a");
        let file = KeySlotFile::try_from(&sample_keyslot(sample_scope("profile-b"))).unwrap();
        let storage = MockSecureStorage::new();
        let mut keyslot_store = MockKeySlotStore::new();
        keyslot_store
            .expect_load()
            .once()
            .return_once(move || Ok(file));
        let service = DefaultKeyMaterialService::new(
            Arc::new(storage) as Arc<dyn SecureStoragePort>,
            Arc::new(keyslot_store) as Arc<dyn KeySlotStore>,
        );

        let err = service
            .load_keyslot(&scope)
            .await
            .expect_err("scope mismatch");

        assert!(matches!(err, EncryptionError::KeyMaterialCorrupt));
    }

    #[tokio::test]
    async fn load_keyslot_returns_keyslot_on_match() {
        let scope = sample_scope("profile-ok");
        let keyslot = sample_keyslot(scope.clone());
        let file = KeySlotFile::try_from(&keyslot).unwrap();
        let storage = MockSecureStorage::new();
        let mut keyslot_store = MockKeySlotStore::new();
        keyslot_store
            .expect_load()
            .once()
            .return_once(move || Ok(file));
        let service = DefaultKeyMaterialService::new(
            Arc::new(storage) as Arc<dyn SecureStoragePort>,
            Arc::new(keyslot_store) as Arc<dyn KeySlotStore>,
        );

        let loaded = service.load_keyslot(&scope).await.expect("load keyslot");

        assert_eq!(loaded, keyslot);
    }

    #[tokio::test]
    async fn store_keyslot_persists_file_representation() {
        let keyslot = sample_keyslot(sample_scope("profile-store"));
        let expected_file = KeySlotFile::try_from(&keyslot).unwrap();
        let storage = MockSecureStorage::new();
        let mut keyslot_store = MockKeySlotStore::new();
        keyslot_store
            .expect_store()
            .withf(move |slot| *slot == expected_file)
            .once()
            .return_once(|_| Ok(()));
        let service = DefaultKeyMaterialService::new(
            Arc::new(storage) as Arc<dyn SecureStoragePort>,
            Arc::new(keyslot_store) as Arc<dyn KeySlotStore>,
        );

        service
            .store_keyslot(&keyslot)
            .await
            .expect("store keyslot");
    }

    #[tokio::test]
    async fn delete_keyslot_rejects_scope_mismatch_without_delete() {
        let scope = sample_scope("profile-x");
        let file = KeySlotFile::try_from(&sample_keyslot(sample_scope("profile-y"))).unwrap();
        let storage = MockSecureStorage::new();
        let mut keyslot_store = MockKeySlotStore::new();
        keyslot_store
            .expect_load()
            .once()
            .return_once(move || Ok(file));
        keyslot_store.expect_delete().never();
        let service = DefaultKeyMaterialService::new(
            Arc::new(storage) as Arc<dyn SecureStoragePort>,
            Arc::new(keyslot_store) as Arc<dyn KeySlotStore>,
        );

        let err = service
            .delete_keyslot(&scope)
            .await
            .expect_err("scope mismatch");

        assert!(matches!(err, EncryptionError::KeyMaterialCorrupt));
    }

    #[tokio::test]
    async fn delete_keyslot_deletes_on_match() {
        let scope = sample_scope("profile-del");
        let file = KeySlotFile::try_from(&sample_keyslot(scope.clone())).unwrap();
        let storage = MockSecureStorage::new();
        let mut keyslot_store = MockKeySlotStore::new();
        keyslot_store
            .expect_load()
            .once()
            .return_once(move || Ok(file));
        keyslot_store.expect_delete().once().return_once(|| Ok(()));
        let service = DefaultKeyMaterialService::new(
            Arc::new(storage) as Arc<dyn SecureStoragePort>,
            Arc::new(keyslot_store) as Arc<dyn KeySlotStore>,
        );

        service
            .delete_keyslot(&scope)
            .await
            .expect("delete keyslot");
    }
}
