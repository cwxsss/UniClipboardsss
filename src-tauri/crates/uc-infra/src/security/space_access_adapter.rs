//! SpaceAccessPort 的基础设施适配器。
//!
//! Slice 3 阶段全部六个方法接通：内部委托既有四件套
//! （EncryptionPort / KeyMaterialPort / EncryptionSessionPort / EncryptionStatePort）
//! + KeyScopePort,忠实保留现有 V1 加密行为
//! （Argon2id KDF + XChaCha20-Poly1305 wrap/unwrap）。
//!
//! initialize / unlock / lock / is_unlocked 的具体调用方将在 Slice 3
//! 后续 commit 中分别从 InitializeEncryption / AutoUnlockEncryptionSession
//! 等 usecase 切过来。三件套 port（EncryptionPort / KeyMaterialPort /
//! EncryptionSessionPort）届时只剩下本 adapter 一个内部消费者,
//! 最终 commit 物理删除。

use std::sync::Arc;

use async_trait::async_trait;
use rand::RngCore;
use tracing::{debug, error, info, info_span, warn, Instrument};

use uc_core::crypto::domain::{ActiveSpace, Passphrase as DomainPassphrase};
use uc_core::crypto::model::{
    EncryptionAlgo, EncryptionError, KeyScope, KeySlot, MasterKey, Passphrase as LegacyPassphrase,
    WrappedMasterKey,
};
use uc_core::crypto::state::EncryptionState;
use uc_core::ids::SpaceId;
use uc_core::ports::security::encryption::EncryptionPort;
use uc_core::ports::security::encryption_session::EncryptionSessionPort;
use uc_core::ports::security::encryption_state::EncryptionStatePort;
use uc_core::ports::security::key_material::KeyMaterialPort;
use uc_core::ports::security::key_scope::KeyScopePort;
use uc_core::ports::space::{SpaceAccessError, SpaceAccessPort};
use uc_core::space_access::{JoinOffer, ProofDerivedKey};

/// Slice 1 的 SpaceAccessPort 实现。
pub struct DefaultSpaceAccessAdapter {
    encryption: Arc<dyn EncryptionPort>,
    key_material: Arc<dyn KeyMaterialPort>,
    key_scope: Arc<dyn KeyScopePort>,
    encryption_state: Arc<dyn EncryptionStatePort>,
    encryption_session: Arc<dyn EncryptionSessionPort>,
}

impl DefaultSpaceAccessAdapter {
    pub fn new(
        encryption: Arc<dyn EncryptionPort>,
        key_material: Arc<dyn KeyMaterialPort>,
        key_scope: Arc<dyn KeyScopePort>,
        encryption_state: Arc<dyn EncryptionStatePort>,
        encryption_session: Arc<dyn EncryptionSessionPort>,
    ) -> Self {
        Self {
            encryption,
            key_material,
            key_scope,
            encryption_state,
            encryption_session,
        }
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

impl DefaultSpaceAccessAdapter {
    /// 私有 helper：执行首次初始化的核心步骤
    /// （生成 KeySlot 草稿 → 派生 KEK → 生成 MasterKey → 包装 → 落盘 → 写入会话 → 标记 Initialized）。
    ///
    /// 返回构造完成的 keyslot（caller 可序列化为 JoinOffer 用）以及 master_key
    /// 的拷贝（caller 可包装成 ActiveSpace 时无需关心，但 prepare_join_offer
    /// 不需要它，因此通过返回值保留 owned）。任何中间步骤失败时按依赖反向回滚。
    async fn do_first_time_init(
        &self,
        scope: &KeyScope,
        passphrase: &DomainPassphrase,
    ) -> Result<KeySlot, SpaceAccessError> {
        let keyslot_draft = KeySlot::draft_v1(scope.clone())
            .map_err(|e| SpaceAccessError::Internal(e.to_string()))?;
        debug!("keyslot draft created");

        let legacy = LegacyPassphrase(passphrase.expose().to_string());
        let kek = self
            .encryption
            .derive_kek(&legacy, &keyslot_draft.salt, &keyslot_draft.kdf)
            .await
            .map_err(map_encryption_error)?;
        debug!("KEK derived");

        let master_key =
            MasterKey::generate().map_err(|e| SpaceAccessError::Internal(e.to_string()))?;
        debug!("master key generated");

        let blob = self
            .encryption
            .wrap_master_key(&kek, &master_key, EncryptionAlgo::XChaCha20Poly1305)
            .await
            .map_err(map_encryption_error)?;
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

        if let Err(e) = self.encryption_session.set_master_key(master_key).await {
            error!(error = %e, "set_master_key failed");
            if let Err(err) = self.key_material.delete_keyslot(scope).await {
                warn!(error = %err, "rollback delete_keyslot failed");
            }
            if let Err(err) = self.key_material.delete_kek(scope).await {
                warn!(error = %err, "rollback delete_kek failed");
            }
            return Err(map_encryption_error(e));
        }

        if let Err(e) = self.encryption_state.persist_initialized().await {
            error!(error = %e, "persist_initialized failed");
            if let Err(err) = self.encryption_session.clear().await {
                warn!(error = %err, "rollback clear master key failed");
            }
            if let Err(err) = self.key_material.delete_keyslot(scope).await {
                warn!(error = %err, "rollback delete_keyslot failed");
            }
            if let Err(err) = self.key_material.delete_kek(scope).await {
                warn!(error = %err, "rollback delete_kek failed");
            }
            return Err(SpaceAccessError::Internal(e.to_string()));
        }

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

            let state = self
                .encryption_state
                .load_state()
                .await
                .map_err(|e| SpaceAccessError::Internal(e.to_string()))?;
            if state == EncryptionState::Initialized {
                return Err(SpaceAccessError::AlreadyInitialized);
            }

            let scope = self
                .key_scope
                .current_scope()
                .await
                .map_err(|e| SpaceAccessError::Internal(e.to_string()))?;
            debug!(scope = %scope.to_identifier(), "got key scope");

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

            let state = self
                .encryption_state
                .load_state()
                .await
                .map_err(|e| SpaceAccessError::Internal(e.to_string()))?;
            if state == EncryptionState::Uninitialized {
                return Err(SpaceAccessError::NotInitialized);
            }

            let scope = self
                .key_scope
                .current_scope()
                .await
                .map_err(|e| SpaceAccessError::Internal(e.to_string()))?;
            debug!(scope = %scope.to_identifier(), "got key scope");

            // 用持久化的 keyslot + 用户口令派生 KEK,unwrap MasterKey。
            // 不读 keyring：unlock 是显式口令路径,行为独立于 keyring 是否
            // 还有缓存。adapter 顺手把 KEK 重新写入 keyring,让后续静默
            // 恢复（startup auto-unlock）能命中。
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
            let kek = self
                .encryption
                .derive_kek(&legacy, &keyslot.salt, &keyslot.kdf)
                .await
                .map_err(map_encryption_error)?;
            debug!("KEK derived from passphrase");

            let master_key = self
                .encryption
                .unwrap_master_key(&kek, &wrapped_master_key.blob)
                .await
                .map_err(map_encryption_error)?;
            debug!("master key unwrapped");

            // 把派生出的 KEK 重新写入 keyring,保持 keyring 与最新口令对齐
            // （让下次静默 startup 路径仍可命中）。失败仅 warn,不影响本次解锁。
            if let Err(e) = self.key_material.store_kek(&scope, &kek).await {
                warn!(error = %e, "store_kek refresh failed (non-fatal)");
            }

            self.encryption_session
                .set_master_key(master_key)
                .await
                .map_err(map_encryption_error)?;

            info!("space unlocked successfully");
            Ok(ActiveSpace::new(space_id.clone()))
        }
        .instrument(span)
        .await
    }

    async fn is_unlocked(&self, _space_id: &SpaceId) -> bool {
        // 当前 EncryptionSession 是单空间模型,不分 SpaceId。
        // 多空间路由由后续阶段（按 SpaceId 索引会话）引入时再展开。
        self.encryption_session.is_ready().await
    }

    async fn lock(&self, _space_id: &SpaceId) -> Result<(), SpaceAccessError> {
        // 持久化的 keyslot/KEK 不动——后续仍可 unlock。仅清空内存会话。
        self.encryption_session
            .clear()
            .await
            .map_err(map_encryption_error)
    }

    async fn prepare_join_offer(
        &self,
        space_id: &SpaceId,
        passphrase: &DomainPassphrase,
    ) -> Result<JoinOffer, SpaceAccessError> {
        let span = info_span!("infra.space_access.prepare_join_offer", space_id = %space_id);
        async {
            info!("preparing sponsor join offer");

            let state = self
                .encryption_state
                .load_state()
                .await
                .map_err(|e| SpaceAccessError::Internal(e.to_string()))?;
            debug!(state = ?state, "loaded encryption state");

            let scope = self
                .key_scope
                .current_scope()
                .await
                .map_err(|e| SpaceAccessError::Internal(e.to_string()))?;
            debug!(scope = %scope.to_identifier(), "got key scope");

            // Branch A — 运行时已初始化的 sponsor 路径：从 key_material 读已有
            // keyslot,不重新生成 MasterKey,忠实对应原 LoadedKeyslotSpaceAccessCrypto
            // 的 export_keyslot_blob 语义。passphrase 参数此时不参与派生,
            // 只是调用契约对齐——保留未来演进空间（比如换口令路径复用此方法）。
            if state == EncryptionState::Initialized {
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

            // Branch B — 首次 setup sponsor 路径：未初始化,走完整 KEK 派生 +
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
            debug!(scope = %scope.to_identifier(), "parsed keyslot from offer blob");

            let wrapped_master_key = keyslot
                .wrapped_master_key
                .as_ref()
                .ok_or(SpaceAccessError::CorruptedKeyMaterial)?;

            let legacy = LegacyPassphrase(passphrase.expose().to_string());
            let kek = self
                .encryption
                .derive_kek(&legacy, &keyslot.salt, &keyslot.kdf)
                .await
                .map_err(map_encryption_error)?;
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

            let master_key = match self
                .encryption
                .unwrap_master_key(&kek, &wrapped_master_key.blob)
                .await
            {
                Ok(master_key) => master_key,
                Err(e) => {
                    error!(error = %e, "unwrap_master_key failed");
                    if let Err(err) = self.key_material.delete_keyslot(&scope).await {
                        warn!(error = %err, "rollback delete_keyslot failed");
                    }
                    if let Err(err) = self.key_material.delete_kek(&scope).await {
                        warn!(error = %err, "rollback delete_kek failed");
                    }
                    return Err(map_encryption_error(e));
                }
            };
            debug!("master key unwrapped");

            if let Err(e) = self
                .encryption_session
                .set_master_key(master_key.clone())
                .await
            {
                error!(error = %e, "set_master_key failed");
                if let Err(err) = self.key_material.delete_keyslot(&scope).await {
                    warn!(error = %err, "rollback delete_keyslot failed");
                }
                if let Err(err) = self.key_material.delete_kek(&scope).await {
                    warn!(error = %err, "rollback delete_kek failed");
                }
                return Err(map_encryption_error(e));
            }

            if let Err(e) = self.encryption_state.persist_initialized().await {
                error!(error = %e, "persist_initialized failed");
                if let Err(err) = self.encryption_session.clear().await {
                    warn!(error = %err, "rollback clear master key failed");
                }
                if let Err(err) = self.key_material.delete_keyslot(&scope).await {
                    warn!(error = %err, "rollback delete_keyslot failed");
                }
                if let Err(err) = self.key_material.delete_kek(&scope).await {
                    warn!(error = %err, "rollback delete_kek failed");
                }
                return Err(SpaceAccessError::Internal(e.to_string()));
            }

            info!("master key derivation completed");
            // 把 MasterKey 字节包装成不透明凭据返回——领域层只看到
            // "本次 proof 链路的 32 字节秘密"，不再暴露 MasterKey 类型。
            // adapter 内部仍然把 master_key 写进了 EncryptionSession，所以
            // 这里消耗它取字节是安全的。
            Ok(ProofDerivedKey::from_bytes(master_key.0))
        }
        .instrument(span)
        .await
    }
}
