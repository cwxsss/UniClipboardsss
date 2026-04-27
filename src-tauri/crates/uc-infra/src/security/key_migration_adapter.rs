//! `KeyMigrationPort` 的基础设施适配器。
//!
//! 把一次性 32 字节 migration_key 持久化到 [`SecureStoragePort`]
//! （macOS Keychain / Windows Credential Manager / Linux secret-service /
//! 测试用内存 fake），加解密复用 `super::v1_aead` 的 V1 XChaCha20-Poly1305
//! 实现，与 `BlobCipherAdapter` 保持算法一致。
//!
//! Keyring entry 命名空间：`migration_key:v1:<run_id>`，与既有 `kek:v1:<scope>`
//! 命名规则平行；`v1:` 前缀做版本化预留，未来切算法时新版本走 `v2:`，
//! 旧 entry 可以共存。

use std::sync::Arc;

use async_trait::async_trait;
use rand::RngCore;
use uc_core::crypto::domain::{Aad, Ciphertext, Plaintext};
use uc_core::ports::security::{KeyMigrationError, KeyMigrationPort};
use uc_core::ports::SecureStoragePort;
use uc_core::setup::MigrationRunId;

use super::crypto_model::EncryptedBlob;
use super::secrets::MasterKey;
use super::v1_aead;

const KEYRING_PREFIX: &str = "migration_key:v1:";

pub struct DefaultKeyMigrationAdapter {
    secure_storage: Arc<dyn SecureStoragePort>,
}

impl DefaultKeyMigrationAdapter {
    pub fn new(secure_storage: Arc<dyn SecureStoragePort>) -> Self {
        Self { secure_storage }
    }

    fn keyring_name(run_id: &MigrationRunId) -> String {
        format!("{KEYRING_PREFIX}{}", run_id.as_str())
    }

    fn load_key(&self, run_id: &MigrationRunId) -> Result<MasterKey, KeyMigrationError> {
        let name = Self::keyring_name(run_id);
        let raw = self
            .secure_storage
            .get(&name)
            .map_err(|e| KeyMigrationError::Internal(format!("secure_storage.get: {e}")))?;
        match raw {
            None => Err(KeyMigrationError::NotFound(run_id.clone())),
            Some(bytes) => MasterKey::from_bytes(&bytes).map_err(|e| {
                KeyMigrationError::Internal(format!("invalid migration key bytes: {e}"))
            }),
        }
    }
}

/// 生成新的 run_id：`mig-{unix_ms}-{8-hex 随机后缀}`。
///
/// 时间戳前缀方便排查（人眼看一眼能知道是不是上周的旧 entry），后缀防
/// 同毫秒撞名。完整字符串只是 keyring entry 的本地命名，不参与加密。
fn fresh_run_id() -> MigrationRunId {
    let ts_ms = chrono::Utc::now().timestamp_millis();
    let mut suffix = [0u8; 4];
    rand::rng().fill_bytes(&mut suffix);
    let suffix_hex = hex::encode(suffix);
    MigrationRunId::new(format!("mig-{ts_ms}-{suffix_hex}"))
}

#[async_trait]
impl KeyMigrationPort for DefaultKeyMigrationAdapter {
    async fn prepare_migration_key(&self) -> Result<MigrationRunId, KeyMigrationError> {
        let run_id = fresh_run_id();
        let name = Self::keyring_name(&run_id);

        // 防御性检查：若 keyring 已经有同名 entry，立即报 AlreadyExists，
        // 让调用方决定"discard 后重试"还是"中止"，避免静默覆写。
        // 实操中由于 run_id 是时间戳 + 4 字节随机，撞名概率极低，主要
        // 防御外部脏数据残留（例如上次崩溃留下的 entry 没被 phase 4 清掉）。
        match self.secure_storage.get(&name) {
            Ok(Some(_)) => return Err(KeyMigrationError::AlreadyExists(run_id)),
            Ok(None) => {}
            Err(e) => {
                return Err(KeyMigrationError::Internal(format!(
                    "secure_storage.get: {e}"
                )))
            }
        }

        let mut bytes = [0u8; 32];
        rand::rng().fill_bytes(&mut bytes);
        self.secure_storage
            .set(&name, &bytes)
            .map_err(|e| KeyMigrationError::Internal(format!("secure_storage.set: {e}")))?;
        Ok(run_id)
    }

    async fn encrypt_with_migration_key(
        &self,
        run_id: &MigrationRunId,
        plaintext: &Plaintext,
        aad: &Aad,
    ) -> Result<Ciphertext, KeyMigrationError> {
        let key = self.load_key(run_id)?;
        let blob = v1_aead::encrypt_blob_xchacha(&key, plaintext.as_bytes(), aad.as_bytes())
            .map_err(|e| KeyMigrationError::Internal(e.to_string()))?;
        let bytes = serde_json::to_vec(&blob)
            .map_err(|e| KeyMigrationError::Internal(format!("serialize EncryptedBlob: {e}")))?;
        Ok(Ciphertext::new(bytes))
    }

    async fn decrypt_with_migration_key(
        &self,
        run_id: &MigrationRunId,
        ciphertext: &Ciphertext,
        aad: &Aad,
    ) -> Result<Plaintext, KeyMigrationError> {
        let key = self.load_key(run_id)?;
        let blob: EncryptedBlob = serde_json::from_slice(ciphertext.as_bytes())
            .map_err(|_| KeyMigrationError::InvalidCiphertext)?;
        if blob.aead != "XChaCha20Poly1305" || blob.version != "V1" {
            return Err(KeyMigrationError::InvalidCiphertext);
        }
        let plain =
            v1_aead::decrypt_blob_xchacha(&key, &blob.nonce, &blob.ciphertext, aad.as_bytes())
                .map_err(|e| match e {
                    v1_aead::AeadError::DecryptFailed => KeyMigrationError::InvalidCiphertext,
                    other => KeyMigrationError::Internal(other.to_string()),
                })?;
        Ok(Plaintext::new(plain))
    }

    async fn discard_migration_key(
        &self,
        run_id: &MigrationRunId,
    ) -> Result<(), KeyMigrationError> {
        let name = Self::keyring_name(run_id);
        // SecureStoragePort.delete 在大多数后端对不存在 key 不报错；
        // 万一某后端报错，统一映射成 Internal 让调用方决定重试。port
        // 文档约定本方法幂等，所以 happy path 上重复调用应当无副作用。
        self.secure_storage
            .delete(&name)
            .map_err(|e| KeyMigrationError::Internal(format!("secure_storage.delete: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    use uc_core::ports::SecureStorageError;

    /// 内存版 `SecureStoragePort` fake：足够覆盖本 adapter 的契约测试。
    struct InMemorySecureStorage {
        entries: Mutex<HashMap<String, Vec<u8>>>,
    }
    impl InMemorySecureStorage {
        fn new() -> Self {
            Self {
                entries: Mutex::new(HashMap::new()),
            }
        }
    }
    impl SecureStoragePort for InMemorySecureStorage {
        fn get(&self, key: &str) -> Result<Option<Vec<u8>>, SecureStorageError> {
            Ok(self.entries.lock().unwrap().get(key).cloned())
        }
        fn set(&self, key: &str, value: &[u8]) -> Result<(), SecureStorageError> {
            self.entries
                .lock()
                .unwrap()
                .insert(key.to_string(), value.to_vec());
            Ok(())
        }
        fn delete(&self, key: &str) -> Result<(), SecureStorageError> {
            self.entries.lock().unwrap().remove(key);
            Ok(())
        }
    }

    fn build_adapter() -> (DefaultKeyMigrationAdapter, Arc<InMemorySecureStorage>) {
        let storage = Arc::new(InMemorySecureStorage::new());
        let adapter =
            DefaultKeyMigrationAdapter::new(Arc::clone(&storage) as Arc<dyn SecureStoragePort>);
        (adapter, storage)
    }

    #[tokio::test]
    async fn prepare_then_round_trip_encrypts_and_decrypts() {
        let (adapter, _) = build_adapter();
        let run_id = adapter.prepare_migration_key().await.unwrap();

        let plaintext = Plaintext::new(b"hello migration".to_vec());
        let aad = Aad::new(b"evt-1|rep-1".to_vec());
        let ct = adapter
            .encrypt_with_migration_key(&run_id, &plaintext, &aad)
            .await
            .unwrap();

        let pt2 = adapter
            .decrypt_with_migration_key(&run_id, &ct, &aad)
            .await
            .unwrap();
        assert_eq!(pt2.as_bytes(), b"hello migration");
    }

    #[tokio::test]
    async fn decrypt_with_wrong_aad_returns_invalid_ciphertext() {
        let (adapter, _) = build_adapter();
        let run_id = adapter.prepare_migration_key().await.unwrap();
        let pt = Plaintext::new(b"x".to_vec());
        let ct = adapter
            .encrypt_with_migration_key(&run_id, &pt, &Aad::new(b"good-aad".to_vec()))
            .await
            .unwrap();
        let err = adapter
            .decrypt_with_migration_key(&run_id, &ct, &Aad::new(b"bad-aad".to_vec()))
            .await
            .unwrap_err();
        assert!(matches!(err, KeyMigrationError::InvalidCiphertext));
    }

    #[tokio::test]
    async fn encrypt_with_unknown_run_id_returns_not_found() {
        let (adapter, _) = build_adapter();
        let unknown = MigrationRunId::new("never-prepared");
        let err = adapter
            .encrypt_with_migration_key(
                &unknown,
                &Plaintext::new(b"x".to_vec()),
                &Aad::new(b"a".to_vec()),
            )
            .await
            .unwrap_err();
        match err {
            KeyMigrationError::NotFound(id) => assert_eq!(id.as_str(), "never-prepared"),
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn discard_then_decrypt_returns_not_found() {
        let (adapter, _) = build_adapter();
        let run_id = adapter.prepare_migration_key().await.unwrap();
        let ct = adapter
            .encrypt_with_migration_key(
                &run_id,
                &Plaintext::new(b"x".to_vec()),
                &Aad::new(b"a".to_vec()),
            )
            .await
            .unwrap();

        adapter.discard_migration_key(&run_id).await.unwrap();

        let err = adapter
            .decrypt_with_migration_key(&run_id, &ct, &Aad::new(b"a".to_vec()))
            .await
            .unwrap_err();
        assert!(matches!(err, KeyMigrationError::NotFound(_)));
    }

    #[tokio::test]
    async fn discard_unknown_run_id_is_idempotent() {
        let (adapter, _) = build_adapter();
        let unknown = MigrationRunId::new("never-existed");
        // 首次 discard：底层 storage 直接 remove 不存在的 key 不报错。
        adapter.discard_migration_key(&unknown).await.unwrap();
        // 再次 discard 也应当 OK——port 文档约定幂等。
        adapter.discard_migration_key(&unknown).await.unwrap();
    }

    #[tokio::test]
    async fn prepare_keyring_entry_uses_v1_prefix() {
        let (adapter, storage) = build_adapter();
        let run_id = adapter.prepare_migration_key().await.unwrap();
        let expected_key = format!("migration_key:v1:{}", run_id.as_str());
        assert!(storage.entries.lock().unwrap().contains_key(&expected_key));
    }

    #[tokio::test]
    async fn fresh_run_id_starts_with_mig_prefix() {
        let id = fresh_run_id();
        assert!(id.as_str().starts_with("mig-"), "got {id}");
    }
}
