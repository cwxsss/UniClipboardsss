//! `SpaceAccessPort` 的基础设施适配器。
//!
//! Slice 3 - C8 起完全独立运行: 不再依赖任何已删除的 port trait
//! (EncryptionPort / EncryptionSessionPort / KeyMaterialPort),
//! 改用 uc-infra 内部具体类型 `KeyMaterialStore` + `InMemorySession`,
//! AEAD 算法走 `super::v1_aead` helper。
//!
//! 公共 port 边界保持稳定: `SpaceAccessPort` trait + 全部方法签名不变。
//! 字节级行为与历史 `EncryptionRepository` 一致——V1 加密协议
//! (Argon2id KDF + XChaCha20-Poly1305 wrap/unwrap) ironclad 保留。

use std::sync::Arc;

use async_trait::async_trait;
use hkdf::Hkdf;
use rand::RngCore;
use sha2::Sha256;
use tracing::{debug, error, info, info_span, warn, Instrument};

use uc_core::crypto::domain::{ActiveSpace, Passphrase as DomainPassphrase};
use uc_core::crypto::model::{EncryptionError, Passphrase as LegacyPassphrase};

use super::crypto_model::{KeyScope, KeySlot, WrappedMasterKey};
use super::secrets::MasterKey;
use uc_core::ids::{ProfileId, SpaceId};
use uc_core::ports::security::current_profile::CurrentProfilePort;
use uc_core::ports::space::{SpaceAccessError, SpaceAccessPort};
use uc_core::space_access::{JoinOffer, ProofDerivedKey};

use super::key_material::KeyMaterialStore;
use super::scope_identifier::scope_identifier;
use super::session::InMemorySession;
use super::v1_aead;

/// `SpaceAccessPort` 默认实现。
pub struct DefaultSpaceAccessAdapter {
    key_material: Arc<KeyMaterialStore>,
    current_profile: Arc<dyn CurrentProfilePort>,
    session: Arc<InMemorySession>,
}

impl DefaultSpaceAccessAdapter {
    pub fn new(
        key_material: Arc<KeyMaterialStore>,
        current_profile: Arc<dyn CurrentProfilePort>,
        session: Arc<InMemorySession>,
    ) -> Self {
        Self {
            key_material,
            current_profile,
            session,
        }
    }
}

/// Helper: 把端口返回的 `ProfileId` 包装成 key_material 使用的 `KeyScope`。
///
/// Slice 7 (U7) 过渡期间 `KeyScope` 仍是 uc-core 类型(磁盘 `KeySlotFile.scope`
/// 字段依赖);Slice 7 Commit 2 搬到 uc-infra 后这个 helper 可简化或消失。
fn key_scope_from_profile(profile: &ProfileId) -> KeyScope {
    KeyScope {
        profile_id: profile.as_ref().to_string(),
    }
}

fn map_encryption_error(err: EncryptionError) -> SpaceAccessError {
    match err {
        EncryptionError::WrongPassphrase => SpaceAccessError::WrongPassphrase,
        EncryptionError::CorruptedKeySlot
        | EncryptionError::CorruptedBlob
        | EncryptionError::UnsupportedKeySlotVersion
        | EncryptionError::UnsupportedBlobVersion => SpaceAccessError::CorruptedKeyMaterial,
        other => SpaceAccessError::Internal(other.to_string()),
    }
}

fn map_aead_error_for_unwrap(err: v1_aead::AeadError) -> SpaceAccessError {
    match err {
        v1_aead::AeadError::DecryptFailed => SpaceAccessError::WrongPassphrase,
        other => SpaceAccessError::Internal(other.to_string()),
    }
}

impl DefaultSpaceAccessAdapter {
    /// 私有 helper：执行首次初始化的核心步骤
    /// （生成 KeySlot 草稿 → 派生 KEK → 生成 MasterKey → 包装 → 落盘 →
    /// 写入会话 → 标记 Initialized）。任何中间步骤失败时按依赖反向回滚。
    async fn do_first_time_init(
        &self,
        scope: &KeyScope,
        passphrase: &DomainPassphrase,
    ) -> Result<KeySlot, SpaceAccessError> {
        let keyslot_draft = KeySlot::draft_v1(scope.clone())
            .map_err(|e| SpaceAccessError::Internal(e.to_string()))?;
        debug!("keyslot draft created");

        let legacy = LegacyPassphrase(passphrase.expose().to_string());
        let kek = v1_aead::derive_kek_argon2id(&legacy, &keyslot_draft.salt, &keyslot_draft.kdf)
            .map_err(SpaceAccessError::Internal)?;
        debug!("KEK derived");

        let master_key =
            MasterKey::generate().map_err(|e| SpaceAccessError::Internal(e.to_string()))?;
        debug!("master key generated");

        let blob = v1_aead::wrap_master_key_xchacha(&kek, &master_key)
            .map_err(|e| SpaceAccessError::Internal(e.to_string()))?;
        debug!("master key wrapped");

        let keyslot = keyslot_draft.finalize(WrappedMasterKey { blob });

        if let Err(e) = self.key_material.store_kek(scope, &kek).await {
            error!(error = %e, "store_kek failed");
            return Err(map_encryption_error(e));
        }

        if let Err(e) = self.key_material.store_keyslot(&keyslot).await {
            error!(error = %e, "store_keyslot failed");
            if let Err(err) = self.key_material.delete_keyslot(scope).await {
                warn!(error = %err, "rollback delete_keyslot failed");
            }
            if let Err(err) = self.key_material.delete_kek(scope).await {
                warn!(error = %err, "rollback delete_kek failed");
            }
            return Err(map_encryption_error(e));
        }

        // session 写入是 in-memory 操作,不会失败——直接写。
        // Phase C 起不再写 `.initialized_encryption` marker 文件;"已初始化"
        // 真相由磁盘 keyslot 存在性 (`key_material.keyslot_exists()`) 回答,
        // setup 完成事实由 `SetupStatusPort.has_completed` 承载。
        self.session.set_master_key(master_key);

        Ok(keyslot)
    }
}

#[async_trait]
impl SpaceAccessPort for DefaultSpaceAccessAdapter {
    async fn initialize(
        &self,
        space_id: &SpaceId,
        passphrase: &DomainPassphrase,
    ) -> Result<ActiveSpace, SpaceAccessError> {
        let span = info_span!("infra.space_access.initialize", space_id = %space_id);
        async {
            info!("initializing new space");

            if self
                .key_material
                .keyslot_exists()
                .await
                .map_err(|e| SpaceAccessError::Internal(e.to_string()))?
            {
                return Err(SpaceAccessError::AlreadyInitialized);
            }

            let profile = self
                .current_profile
                .current_profile()
                .await
                .map_err(|e| SpaceAccessError::Internal(e.to_string()))?;
            let scope = key_scope_from_profile(&profile);
            debug!(scope = %scope_identifier(&scope), "got key scope");

            self.do_first_time_init(&scope, passphrase).await?;

            info!("space initialized successfully");
            Ok(ActiveSpace::new(space_id.clone()))
        }
        .instrument(span)
        .await
    }

    async fn unlock(
        &self,
        space_id: &SpaceId,
        passphrase: &DomainPassphrase,
    ) -> Result<ActiveSpace, SpaceAccessError> {
        let span = info_span!("infra.space_access.unlock", space_id = %space_id);
        async {
            info!("unlocking space with passphrase");

            if !self
                .key_material
                .keyslot_exists()
                .await
                .map_err(|e| SpaceAccessError::Internal(e.to_string()))?
            {
                return Err(SpaceAccessError::NotInitialized);
            }

            let profile = self
                .current_profile
                .current_profile()
                .await
                .map_err(|e| SpaceAccessError::Internal(e.to_string()))?;
            let scope = key_scope_from_profile(&profile);

            let keyslot = self
                .key_material
                .load_keyslot(&scope)
                .await
                .map_err(map_encryption_error)?;

            let wrapped_master_key = keyslot
                .wrapped_master_key
                .as_ref()
                .ok_or(SpaceAccessError::CorruptedKeyMaterial)?;

            let legacy = LegacyPassphrase(passphrase.expose().to_string());
            let kek = v1_aead::derive_kek_argon2id(&legacy, &keyslot.salt, &keyslot.kdf)
                .map_err(SpaceAccessError::Internal)?;
            debug!("KEK derived from passphrase");

            let master_key = v1_aead::unwrap_master_key_xchacha(&kek, &wrapped_master_key.blob)
                .map_err(map_aead_error_for_unwrap)?;
            debug!("master key unwrapped");

            // 把派生出的 KEK 重新写入 keyring,保持 keyring 与最新口令对齐
            // (让下次静默 startup 路径仍可命中)。失败仅 warn,不影响本次解锁。
            if let Err(e) = self.key_material.store_kek(&scope, &kek).await {
                warn!(error = %e, "store_kek refresh failed (non-fatal)");
            }

            self.session.set_master_key(master_key);

            info!("space unlocked successfully");
            Ok(ActiveSpace::new(space_id.clone()))
        }
        .instrument(span)
        .await
    }

    async fn is_unlocked(&self, _space_id: &SpaceId) -> bool {
        self.session.is_ready()
    }

    async fn lock(&self, _space_id: &SpaceId) -> Result<(), SpaceAccessError> {
        self.session.clear();
        Ok(())
    }

    async fn factory_reset(&self, space_id: &SpaceId) -> Result<(), SpaceAccessError> {
        let span = info_span!("infra.space_access.factory_reset", space_id = %space_id);
        async {
            let profile = self
                .current_profile
                .current_profile()
                .await
                .map_err(|e| SpaceAccessError::Internal(e.to_string()))?;
            let scope = key_scope_from_profile(&profile);

            // 幂等: 不存在的物料视为已经删除,不报错。
            match self.key_material.delete_keyslot(&scope).await {
                Ok(()) | Err(EncryptionError::KeyNotFound) => {}
                Err(e) => return Err(map_encryption_error(e)),
            }
            match self.key_material.delete_kek(&scope).await {
                Ok(()) | Err(EncryptionError::KeyNotFound) => {}
                Err(e) => return Err(map_encryption_error(e)),
            }
            self.session.clear();
            Ok(())
        }
        .instrument(span)
        .await
    }

    async fn try_resume_session(
        &self,
        space_id: &SpaceId,
    ) -> Result<Option<ActiveSpace>, SpaceAccessError> {
        let span = info_span!("infra.space_access.try_resume_session", space_id = %space_id);
        async {
            info!("attempting silent session resume from keyring");

            if !self
                .key_material
                .keyslot_exists()
                .await
                .map_err(|e| SpaceAccessError::Internal(e.to_string()))?
            {
                info!("no keyslot on disk, no session to resume");
                return Ok(None);
            }

            let profile = self
                .current_profile
                .current_profile()
                .await
                .map_err(|e| SpaceAccessError::Internal(e.to_string()))?;
            let scope = key_scope_from_profile(&profile);

            let keyslot = self
                .key_material
                .load_keyslot(&scope)
                .await
                .map_err(map_encryption_error)?;
            let wrapped_master_key = keyslot
                .wrapped_master_key
                .as_ref()
                .ok_or(SpaceAccessError::CorruptedKeyMaterial)?;

            // 静默路径: 直接读 keyring 缓存的 KEK,不重新派生。
            let kek = self
                .key_material
                .load_kek(&scope)
                .await
                .map_err(map_encryption_error)?;

            let master_key = v1_aead::unwrap_master_key_xchacha(&kek, &wrapped_master_key.blob)
                .map_err(map_aead_error_for_unwrap)?;

            self.session.set_master_key(master_key);

            info!("session resumed from keyring");
            Ok(Some(ActiveSpace::new(space_id.clone())))
        }
        .instrument(span)
        .await
    }

    async fn verify_keychain_access(&self) -> Result<bool, SpaceAccessError> {
        let span = info_span!("infra.space_access.verify_keychain_access");
        async {
            let profile = self
                .current_profile
                .current_profile()
                .await
                .map_err(|e| SpaceAccessError::Internal(e.to_string()))?;
            let scope = key_scope_from_profile(&profile);

            // 探测: 把"权限被拒绝"和"keyring 暂时不可用"都视为 "Always Allow 未授予"
            // (Ok(false));只有"KEK 不存在"才升格成 NotInitialized 报错给上层。
            match self.key_material.load_kek(&scope).await {
                Ok(_) => Ok(true),
                Err(EncryptionError::PermissionDenied) => Ok(false),
                Err(EncryptionError::KeyringError(_)) => Ok(false),
                Err(EncryptionError::KeyNotFound) => Err(SpaceAccessError::NotInitialized),
                Err(other) => Err(SpaceAccessError::Internal(other.to_string())),
            }
        }
        .instrument(span)
        .await
    }

    async fn derive_subkey(&self, salt: &[u8], info: &[u8]) -> Result<[u8; 32], SpaceAccessError> {
        if !self.session.is_ready() {
            return Err(SpaceAccessError::NotUnlocked);
        }
        let master_key = self
            .session
            .get_master_key()
            .map_err(map_encryption_error)?;

        let hkdf = Hkdf::<Sha256>::new(Some(salt), master_key.as_bytes());
        let mut okm = [0u8; 32];
        hkdf.expand(info, &mut okm)
            .map_err(|e| SpaceAccessError::Internal(format!("HKDF expand: {e}")))?;
        Ok(okm)
    }

    async fn current_session_proof_key(&self) -> Result<Option<ProofDerivedKey>, SpaceAccessError> {
        if !self.session.is_ready() {
            return Ok(None);
        }
        let master_key = self
            .session
            .get_master_key()
            .map_err(map_encryption_error)?;
        Ok(Some(ProofDerivedKey::from_bytes(master_key.0)))
    }

    async fn prepare_join_offer(
        &self,
        space_id: &SpaceId,
        passphrase: &DomainPassphrase,
    ) -> Result<JoinOffer, SpaceAccessError> {
        let span = info_span!("infra.space_access.prepare_join_offer", space_id = %space_id);
        async {
            info!("preparing sponsor join offer");

            let already_initialized = self
                .key_material
                .keyslot_exists()
                .await
                .map_err(|e| SpaceAccessError::Internal(e.to_string()))?;
            debug!(already_initialized, "checked keyslot existence");

            let profile = self
                .current_profile
                .current_profile()
                .await
                .map_err(|e| SpaceAccessError::Internal(e.to_string()))?;
            let scope = key_scope_from_profile(&profile);
            debug!(scope = %scope_identifier(&scope), "got key scope");

            // Branch A — 运行时已初始化的 sponsor 路径: 从 key_material 读已有 keyslot,
            // 不重新生成 MasterKey。passphrase 参数此时不参与派生。
            if already_initialized {
                let _ = passphrase;
                let keyslot = self
                    .key_material
                    .load_keyslot(&scope)
                    .await
                    .map_err(map_encryption_error)?;
                let keyslot_blob = serde_json::to_vec(&keyslot)
                    .map_err(|e| SpaceAccessError::Internal(format!("serialize keyslot: {e}")))?;
                let mut challenge_nonce = [0u8; 32];
                rand::rng().fill_bytes(&mut challenge_nonce);
                info!("sponsor join offer prepared (runtime, already initialized)");
                return Ok(JoinOffer {
                    space_id: space_id.clone(),
                    keyslot_blob,
                    challenge_nonce,
                });
            }

            // Branch B — 首次 setup sponsor 路径: 未初始化,走完整 KEK 派生 +
            // MasterKey 生成 + 包装 + 落盘 + 标记 Initialized。
            let keyslot = self.do_first_time_init(&scope, passphrase).await?;
            let keyslot_blob = serde_json::to_vec(&keyslot)
                .map_err(|e| SpaceAccessError::Internal(format!("serialize keyslot: {e}")))?;
            let mut challenge_nonce = [0u8; 32];
            rand::rng().fill_bytes(&mut challenge_nonce);

            info!("sponsor join offer prepared");
            Ok(JoinOffer {
                space_id: space_id.clone(),
                keyslot_blob,
                challenge_nonce,
            })
        }
        .instrument(span)
        .await
    }

    async fn derive_master_key_for_proof(
        &self,
        offer: &JoinOffer,
        passphrase: &DomainPassphrase,
    ) -> Result<ProofDerivedKey, SpaceAccessError> {
        let span = info_span!("infra.space_access.derive_master_key_for_proof", space_id = %offer.space_id);
        async {
            info!("deriving master key from pairing offer");

            let keyslot: KeySlot = serde_json::from_slice(&offer.keyslot_blob)
                .map_err(|_| SpaceAccessError::CorruptedKeyMaterial)?;
            let scope = keyslot.scope.clone();
            debug!(scope = %scope_identifier(&scope), "parsed keyslot from offer blob");

            let wrapped_master_key = keyslot
                .wrapped_master_key
                .as_ref()
                .ok_or(SpaceAccessError::CorruptedKeyMaterial)?;

            let legacy = LegacyPassphrase(passphrase.expose().to_string());
            let kek = v1_aead::derive_kek_argon2id(&legacy, &keyslot.salt, &keyslot.kdf)
                .map_err(SpaceAccessError::Internal)?;
            debug!("KEK derived from passphrase and offer keyslot");

            if let Err(e) = self.key_material.store_kek(&scope, &kek).await {
                error!(error = %e, "store_kek failed");
                return Err(map_encryption_error(e));
            }

            if let Err(e) = self.key_material.store_keyslot(&keyslot).await {
                error!(error = %e, "store_keyslot failed");
                if let Err(err) = self.key_material.delete_keyslot(&scope).await {
                    warn!(error = %err, "rollback delete_keyslot failed");
                }
                if let Err(err) = self.key_material.delete_kek(&scope).await {
                    warn!(error = %err, "rollback delete_kek failed");
                }
                return Err(map_encryption_error(e));
            }

            let master_key =
                match v1_aead::unwrap_master_key_xchacha(&kek, &wrapped_master_key.blob) {
                    Ok(mk) => mk,
                    Err(e) => {
                        error!(error = ?e, "unwrap_master_key failed");
                        if let Err(err) = self.key_material.delete_keyslot(&scope).await {
                            warn!(error = %err, "rollback delete_keyslot failed");
                        }
                        if let Err(err) = self.key_material.delete_kek(&scope).await {
                            warn!(error = %err, "rollback delete_kek failed");
                        }
                        return Err(map_aead_error_for_unwrap(e));
                    }
                };
            debug!("master key unwrapped");

            // 把字节注入会话(让 sponsor 后续 verify 走 fallback 路径),
            // 同时包装一份成不透明凭据返回 joiner 侧调用方。
            // Phase C 起不再写 `.initialized_encryption` marker 文件;
            // "本机已初始化" 的真相由磁盘 keyslot 文件存在性回答。
            self.session.set_master_key(master_key.clone());
            let derived = ProofDerivedKey::from_bytes(master_key.0);

            info!("master key derivation completed");
            Ok(derived)
        }
        .instrument(span)
        .await
    }
}
