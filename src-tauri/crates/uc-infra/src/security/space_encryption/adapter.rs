//! `SpaceCryptoPort` 的基础设施实现 —— 内存版本。
//!
//! 本 adapter 把"创建空间"的完整业务动作封装成一个方法：
//! KDF 派生 → 子密钥派生 → 随机 DMK → AEAD 包装 → 元数据登记 → 会话登记。
//! 任意步骤失败都会保持 adapter 内部状态不变（目前只有内存 `HashMap`，
//! 失败路径天然无副作用；Phase 3.1.b 加入持久化后需补完 saga 回滚逻辑）。
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

use super::kdf::{derive_srk, derive_subkeys};
use super::types::{
    Dmk, KdfParams, SpaceMetadataV2, SpaceSeed, WrappedDmk, AEAD_NONCE_LEN, KEY_LEN,
};

/// v2 内存版空间加密 adapter。
///
/// 职责范围：Phase 3.1.a——仅实现 `create_space`。3.1.b 加入 SQLite 持久化，
/// 3.2 之后扩展 `unlock / encrypt / decrypt / change_passphrase` 等方法。
pub struct InMemorySpaceCryptoAdapter {
    sessions: Arc<Mutex<HashMap<SpaceId, Dmk>>>,
    metadata: Arc<Mutex<HashMap<SpaceId, SpaceMetadataV2>>>,
    kdf_params: KdfParams,
}

impl InMemorySpaceCryptoAdapter {
    /// 生产场景入口 —— 使用 D2 决策的默认 Argon2id 参数（128 MiB / iters=3 / par=4）。
    pub fn new() -> Self {
        Self::with_kdf_params(KdfParams::default())
    }

    /// 显式注入 KDF 参数（测试或未来参数演化时使用）。
    pub fn with_kdf_params(kdf_params: KdfParams) -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            metadata: Arc::new(Mutex::new(HashMap::new())),
            kdf_params,
        }
    }

    /// 测试辅助：窥视某个空间当前是否有会话条目。
    #[cfg(test)]
    pub(crate) async fn has_session(&self, id: &SpaceId) -> bool {
        self.sessions.lock().await.contains_key(id)
    }

    /// 测试辅助：读取某个空间的元数据快照。
    #[cfg(test)]
    pub(crate) async fn peek_metadata(&self, id: &SpaceId) -> Option<SpaceMetadataV2> {
        self.metadata.lock().await.get(id).cloned()
    }
}

impl Default for InMemorySpaceCryptoAdapter {
    fn default() -> Self {
        Self::new()
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
impl SpaceCryptoPort for InMemorySpaceCryptoAdapter {
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

        // 8. 登记（内存）—— 这两步先 metadata 后 session，保证即使 session 未登记成功，
        //   也能通过 metadata 重建（未来 persist 版本会用同样顺序 + saga 回滚）。
        self.metadata
            .lock()
            .await
            .insert(space_id.clone(), metadata);
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

    fn test_adapter() -> InMemorySpaceCryptoAdapter {
        InMemorySpaceCryptoAdapter::with_kdf_params(KdfParams::insecure_test_defaults())
    }

    #[tokio::test]
    async fn create_space_returns_active_space_with_registered_session() {
        let crypto = test_adapter();
        let pp = Passphrase::from("correct horse battery staple");

        let active = crypto.create_space(&pp).await.unwrap();

        // ActiveSpace 的 space_id 非空
        assert!(!active.space_id().as_str().is_empty());
        // 会话与元数据都已登记
        assert!(crypto.has_session(active.space_id()).await);
        assert!(crypto.peek_metadata(active.space_id()).await.is_some());
    }

    #[tokio::test]
    async fn two_create_calls_yield_distinct_space_ids() {
        let crypto = test_adapter();
        let pp = Passphrase::from("same passphrase");

        let a = crypto.create_space(&pp).await.unwrap();
        let b = crypto.create_space(&pp).await.unwrap();

        assert_ne!(a.space_id(), b.space_id(), "每次 create 应产生新 space_id");

        // 即使 passphrase 相同，wrapped_dmk 也应不同（随机 DMK + 随机 nonce）
        let meta_a = crypto.peek_metadata(a.space_id()).await.unwrap();
        let meta_b = crypto.peek_metadata(b.space_id()).await.unwrap();
        assert_ne!(meta_a.wrapped_dmk.ciphertext, meta_b.wrapped_dmk.ciphertext);
        assert_ne!(meta_a.wrapped_dmk.nonce, meta_b.wrapped_dmk.nonce);
        assert_ne!(meta_a.space_seed.as_bytes(), meta_b.space_seed.as_bytes());
    }

    #[tokio::test]
    async fn created_metadata_carries_expected_fields() {
        let crypto = test_adapter();
        let pp = Passphrase::from("pp");

        let active = crypto.create_space(&pp).await.unwrap();
        let meta = crypto.peek_metadata(active.space_id()).await.unwrap();

        // space_id 与 ActiveSpace 一致
        assert_eq!(&meta.space_id, active.space_id());
        // nonce 长度匹配 XChaCha20-Poly1305
        assert_eq!(meta.wrapped_dmk.nonce.len(), AEAD_NONCE_LEN);
        // 密文为 DMK + Poly1305 tag
        assert_eq!(meta.wrapped_dmk.ciphertext.len(), KEY_LEN + 16);
        // KDF 参数来自 adapter 构造
        assert_eq!(meta.kdf_params, KdfParams::insecure_test_defaults());
    }

    #[tokio::test]
    async fn wrapped_dmk_can_be_unwrapped_with_derived_subkeys() {
        // 本测试证明 adapter 的封装过程与 kdf.rs 保持一致：
        // 用相同 passphrase + metadata 里的 seed/kdf_params 重新派生子密钥，
        // 能成功解包 wrapped_dmk。这是 Phase 3.2 "unlock" 的基础保证。
        let crypto = test_adapter();
        let pp_str = "unit-test-pp";
        let pp = Passphrase::from(pp_str);

        let active = crypto.create_space(&pp).await.unwrap();
        let meta = crypto.peek_metadata(active.space_id()).await.unwrap();

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
        assert_eq!(plaintext.len(), KEY_LEN, "解出的 DMK 应为 32 字节");
    }

    #[tokio::test]
    async fn wrong_passphrase_fails_to_unwrap() {
        // 验证 AEAD + SRK 派生的真实性：用错误口令派生出的 wrap_key 解包必失败
        let crypto = test_adapter();
        let active = crypto
            .create_space(&Passphrase::from("right"))
            .await
            .unwrap();
        let meta = crypto.peek_metadata(active.space_id()).await.unwrap();

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
        assert!(result.is_err(), "错误口令不应解出 DMK");
    }
}
