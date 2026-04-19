//! `SpaceCryptoPort` 的基础设施实现。
//!
//! 把"创建空间"的完整业务动作封装成一个方法：
//! KDF 派生 → 子密钥派生 → 随机 DMK → AEAD 包装 → 持久化元数据 → 会话登记。
//!
//! 持久化通过 `SpaceMetadataRepositoryPort` 完成，具体存储由 adapter 外部
//! 注入（生产：SQLite；测试：内存）。
//!
//! Saga 回滚：元数据持久化失败时直接向上抛（HashMap 会话尚未登记，无副作用）；
//! 元数据成功但会话登记不会失败（内存操作）——未来引入更多步骤时在本方法内补回滚。
//!
//! AAD 约定（DMK 包装）：`space_id_utf8 || b"v2"` —— 防止跨空间/跨版本混用密文。

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chacha20poly1305::{
    aead::{Aead, Payload},
    KeyInit, XChaCha20Poly1305, XNonce,
};
use rand::RngCore;
use tokio::sync::Mutex;
use uuid::Uuid;

use uc_core::crypto::domain::{ActiveSpace, Passphrase};
use uc_core::ids::SpaceId;
use uc_core::ports::space_encryption::{SpaceCryptoError, SpaceCryptoPort};
use uc_core::ports::space_metadata_repository::SpaceMetadataRepositoryPort;

use super::kdf::{derive_srk, derive_subkeys};
use super::payload;
use super::types::{
    Dmk, KdfParams, SpaceMetadataV2, SpaceSeed, WrappedDmk, AEAD_NONCE_LEN, KEY_LEN,
};

/// v2 空间加密 adapter。
///
/// 持久化能力由注入的 `SpaceMetadataRepositoryPort` 提供——生产装配时传入
/// `DieselSpaceMetadataRepository`，测试可传入 `InMemorySpaceMetadataRepository`。
///
/// 会话（解锁后的 DMK 内存缓存）保留为 adapter 的内部 `HashMap`——Phase 3.2
/// 引入 Unlock 时会评估是否提取为独立 port。
pub struct SpaceCryptoAdapter {
    metadata_repo: Arc<dyn SpaceMetadataRepositoryPort>,
    sessions: Arc<Mutex<HashMap<SpaceId, Dmk>>>,
    kdf_params: KdfParams,
}

impl SpaceCryptoAdapter {
    /// 用默认 Argon2id 参数（D2：128 MiB / iters=3 / par=4）构造。
    pub fn new(metadata_repo: Arc<dyn SpaceMetadataRepositoryPort>) -> Self {
        Self::with_kdf_params(metadata_repo, KdfParams::default())
    }

    /// 显式注入 KDF 参数（测试或未来参数演化时使用）。
    pub fn with_kdf_params(
        metadata_repo: Arc<dyn SpaceMetadataRepositoryPort>,
        kdf_params: KdfParams,
    ) -> Self {
        Self {
            metadata_repo,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            kdf_params,
        }
    }

    /// 测试辅助：窥视某个空间当前是否有会话条目（内存会话表）。
    #[cfg(test)]
    pub(crate) async fn has_session(&self, id: &SpaceId) -> bool {
        self.sessions.lock().await.contains_key(id)
    }
}

/// 构造 DMK 包装时的 AAD：`space_id_utf8 || "v2"`。
fn dmk_wrap_aad(space_id: &str) -> Vec<u8> {
    let mut v = Vec::with_capacity(space_id.len() + 2);
    v.extend_from_slice(space_id.as_bytes());
    v.extend_from_slice(b"v2");
    v
}

#[async_trait]
impl SpaceCryptoPort for SpaceCryptoAdapter {
    async fn create_space(&self, passphrase: &Passphrase) -> Result<ActiveSpace, SpaceCryptoError> {
        // 1. 生成 SpaceId (UUIDv4)
        let space_id_str = Uuid::new_v4().to_string();
        let space_id = SpaceId::from(space_id_str.clone());

        // 2. 随机 space_seed
        let seed = SpaceSeed::generate();

        // 3. 派生 SRK
        let srk = derive_srk(
            passphrase.expose().as_bytes(),
            &space_id_str,
            &seed,
            &self.kdf_params,
        )
        .map_err(|e| SpaceCryptoError::Internal(anyhow::anyhow!(e)))?;

        // 4. HKDF 派生子密钥
        let subkeys = derive_subkeys(&srk, &space_id_str, &seed)
            .map_err(|e| SpaceCryptoError::Internal(anyhow::anyhow!(e)))?;

        // 5. 随机 DMK
        let dmk = Dmk::generate();

        // 6. AEAD 包装 DMK
        let mut nonce = [0u8; AEAD_NONCE_LEN];
        rand::rng().fill_bytes(&mut nonce);
        let cipher = XChaCha20Poly1305::new_from_slice(&subkeys.dmk_wrap_key)
            .map_err(|e| SpaceCryptoError::Internal(anyhow::anyhow!("wrap key init: {e}")))?;
        let aad = dmk_wrap_aad(&space_id_str);
        let ciphertext = cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: dmk.as_bytes(),
                    aad: &aad,
                },
            )
            .map_err(|e| SpaceCryptoError::Internal(anyhow::anyhow!("dmk wrap: {e}")))?;
        debug_assert_eq!(
            ciphertext.len(),
            KEY_LEN + 16,
            "XChaCha20-Poly1305 应产生 32+16 字节密文"
        );

        let wrapped_dmk = WrappedDmk {
            nonce: nonce.to_vec(),
            ciphertext,
        };

        // 7. 构造元数据
        let metadata = SpaceMetadataV2 {
            space_id: space_id.clone(),
            space_seed: seed,
            kdf_params: self.kdf_params.clone(),
            wrapped_dmk,
            created_at: chrono::Utc::now(),
        };

        // 8. 序列化并持久化 —— 失败则整个动作失败，不登记会话
        let payload_bytes = payload::encode(&metadata)
            .map_err(|e| SpaceCryptoError::Internal(anyhow::anyhow!(e)))?;
        self.metadata_repo.save(&space_id, &payload_bytes).await?;

        // 9. 登记会话（内存）
        self.sessions.lock().await.insert(space_id.clone(), dmk);

        Ok(ActiveSpace::new(space_id))
    }
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::space_encryption::repository::InMemorySpaceMetadataRepository;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn test_adapter() -> (SpaceCryptoAdapter, Arc<InMemorySpaceMetadataRepository>) {
        let repo = Arc::new(InMemorySpaceMetadataRepository::new());
        let adapter =
            SpaceCryptoAdapter::with_kdf_params(repo.clone(), KdfParams::insecure_test_defaults());
        (adapter, repo)
    }

    #[tokio::test]
    async fn create_space_registers_session_and_persists_metadata() {
        let (crypto, repo) = test_adapter();
        let pp = Passphrase::from("correct horse battery staple");

        let active = crypto.create_space(&pp).await.unwrap();

        assert!(!active.space_id().as_str().is_empty());
        assert!(crypto.has_session(active.space_id()).await);

        // 元数据 payload 已落盘并可反序列化为等价结构
        let saved = repo.load(active.space_id()).await.unwrap().unwrap();
        let decoded = payload::decode(&saved).unwrap();
        assert_eq!(&decoded.space_id, active.space_id());
        assert_eq!(decoded.wrapped_dmk.nonce.len(), AEAD_NONCE_LEN);
        assert_eq!(decoded.wrapped_dmk.ciphertext.len(), KEY_LEN + 16);
    }

    #[tokio::test]
    async fn two_create_calls_yield_distinct_space_ids() {
        let (crypto, _) = test_adapter();
        let pp = Passphrase::from("same passphrase");
        let a = crypto.create_space(&pp).await.unwrap();
        let b = crypto.create_space(&pp).await.unwrap();
        assert_ne!(a.space_id(), b.space_id());
    }

    #[tokio::test]
    async fn persisted_payload_round_trip_matches_live_dmk() {
        // 证明 adapter 持久化的 payload 与 create_space 内部的 DMK 一致：
        // 用相同 passphrase + 落盘元数据重新派生子密钥，能解包出 DMK。
        let (crypto, repo) = test_adapter();
        let pp_str = "unit-test-pp";
        let pp = Passphrase::from(pp_str);

        let active = crypto.create_space(&pp).await.unwrap();
        let saved = repo.load(active.space_id()).await.unwrap().unwrap();
        let meta = payload::decode(&saved).unwrap();

        let srk = derive_srk(
            pp_str.as_bytes(),
            active.space_id().as_str(),
            &meta.space_seed,
            &meta.kdf_params,
        )
        .unwrap();
        let sk = derive_subkeys(&srk, active.space_id().as_str(), &meta.space_seed).unwrap();

        let cipher = XChaCha20Poly1305::new_from_slice(&sk.dmk_wrap_key).unwrap();
        let aad = dmk_wrap_aad(active.space_id().as_str());
        let plaintext = cipher
            .decrypt(
                XNonce::from_slice(&meta.wrapped_dmk.nonce),
                Payload {
                    msg: &meta.wrapped_dmk.ciphertext,
                    aad: &aad,
                },
            )
            .expect("解包应成功");
        assert_eq!(plaintext.len(), KEY_LEN);
    }

    #[tokio::test]
    async fn wrong_passphrase_fails_to_unwrap_persisted_dmk() {
        let (crypto, repo) = test_adapter();
        let active = crypto
            .create_space(&Passphrase::from("right"))
            .await
            .unwrap();
        let meta = payload::decode(&repo.load(active.space_id()).await.unwrap().unwrap()).unwrap();

        let srk = derive_srk(
            b"wrong",
            active.space_id().as_str(),
            &meta.space_seed,
            &meta.kdf_params,
        )
        .unwrap();
        let sk = derive_subkeys(&srk, active.space_id().as_str(), &meta.space_seed).unwrap();

        let cipher = XChaCha20Poly1305::new_from_slice(&sk.dmk_wrap_key).unwrap();
        let aad = dmk_wrap_aad(active.space_id().as_str());
        let result = cipher.decrypt(
            XNonce::from_slice(&meta.wrapped_dmk.nonce),
            Payload {
                msg: &meta.wrapped_dmk.ciphertext,
                aad: &aad,
            },
        );
        assert!(result.is_err());
    }

    // ------------------------------------------------------------------
    // Saga 回滚：元数据持久化失败时不得登记会话
    // ------------------------------------------------------------------

    /// 构造一个第 N 次 save 失败的 repo，用于验证 saga 失败路径。
    struct FailingRepo {
        calls: Arc<AtomicU32>,
        fail_on_call: u32,
    }

    #[async_trait]
    impl SpaceMetadataRepositoryPort for FailingRepo {
        async fn save(
            &self,
            _id: &SpaceId,
            _payload: &[u8],
        ) -> Result<(), uc_core::ports::space_metadata_repository::SpaceMetadataError> {
            let n = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
            if n == self.fail_on_call {
                Err(
                    uc_core::ports::space_metadata_repository::SpaceMetadataError::Backend(
                        anyhow::anyhow!("simulated disk full"),
                    ),
                )
            } else {
                Ok(())
            }
        }
        async fn load(
            &self,
            _id: &SpaceId,
        ) -> Result<Option<Vec<u8>>, uc_core::ports::space_metadata_repository::SpaceMetadataError>
        {
            Ok(None)
        }
    }

    #[tokio::test]
    async fn save_failure_does_not_register_session() {
        let repo = Arc::new(FailingRepo {
            calls: Arc::new(AtomicU32::new(0)),
            fail_on_call: 1,
        });
        let crypto = SpaceCryptoAdapter::with_kdf_params(repo, KdfParams::insecure_test_defaults());

        let err = crypto
            .create_space(&Passphrase::from("pp"))
            .await
            .expect_err("元数据保存失败时 create_space 必须失败");

        match err {
            SpaceCryptoError::Metadata(_) => {}
            other => panic!("期望 Metadata 错误，实际 {:?}", other),
        }

        // 会话 map 不应留下任何条目
        assert_eq!(crypto.sessions.lock().await.len(), 0);
    }
}
