//! SpaceAccessPort 的基础设施适配器。
//!
//! Slice 1 阶段实现：内部委托现有的四件套（EncryptionPort / KeyMaterialPort /
//! EncryptionSessionPort / EncryptionStatePort）+ KeyScopePort,忠实保留现有
//! sponsor / joiner pairing 行为。`initialize / unlock / is_unlocked / lock`
//! 的调用方在 Slice 3 才会切过来——Slice 1 里这些方法返回未实现错误,保持
//! adapter 形状完整但不影响现有代码路径。

use std::sync::Arc;

use async_trait::async_trait;
use rand::RngCore;
use tracing::{debug, error, info, info_span, warn, Instrument};

use uc_core::crypto::domain::{ActiveSpace, Passphrase as DomainPassphrase};
use uc_core::crypto::model::{
    EncryptionAlgo, EncryptionError, KeySlot, MasterKey, Passphrase as LegacyPassphrase,
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

#[async_trait]
impl SpaceAccessPort for DefaultSpaceAccessAdapter {
    async fn initialize(
        &self,
        _space_id: &SpaceId,
        _passphrase: &DomainPassphrase,
    ) -> Result<ActiveSpace, SpaceAccessError> {
        Err(SpaceAccessError::Internal(
            "SpaceAccessPort::initialize not yet migrated (Slice 3)".into(),
        ))
    }

    async fn unlock(
        &self,
        _space_id: &SpaceId,
        _passphrase: &DomainPassphrase,
    ) -> Result<ActiveSpace, SpaceAccessError> {
        Err(SpaceAccessError::Internal(
            "SpaceAccessPort::unlock not yet migrated (Slice 3)".into(),
        ))
    }

    async fn is_unlocked(&self, _space_id: &SpaceId) -> bool {
        // Slice 3 接通调用方后再委托 encryption_session.is_ready()。
        false
    }

    async fn lock(&self, _space_id: &SpaceId) -> Result<(), SpaceAccessError> {
        Err(SpaceAccessError::Internal(
            "SpaceAccessPort::lock not yet migrated (Slice 3)".into(),
        ))
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
            // MasterKey 生成 + 包装 + 落盘。对应原 SpaceAccessCryptoAdapter 的
            // export_keyslot_blob 语义。
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

            if let Err(e) = self.encryption_session.set_master_key(master_key).await {
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
