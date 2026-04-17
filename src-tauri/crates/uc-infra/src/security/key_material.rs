use async_trait::async_trait;
use std::sync::Arc;
use uc_core::{
    crypto::model::{EncryptionError, Kek, KeyScope, KeySlot, KeySlotFile},
    ports::{KeyMaterialPort, SecureStoragePort},
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
