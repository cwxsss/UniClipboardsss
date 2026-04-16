use std::sync::Arc;

use async_trait::async_trait;
use rand::rngs::OsRng;
use rand::RngCore;
use tracing::{debug, error, info, info_span, warn, Instrument};

use uc_core::ids::SpaceId;
use uc_core::ports::security::encryption_state::EncryptionStatePort;
use uc_core::ports::security::key_scope::{KeyScopePort, ScopeError};
use uc_core::ports::space::CryptoPort;
use uc_core::ports::{EncryptionPort, EncryptionSessionPort, KeyMaterialPort};
use uc_core::security::model::{
    EncryptionAlgo, EncryptionError, KeySlot, MasterKey, Passphrase, WrappedMasterKey,
};
use uc_core::security::state::{EncryptionState, EncryptionStateError};
use uc_core::security::SecretString;

use super::SpaceAccessCryptoFactory;

#[derive(Debug, thiserror::Error)]
pub enum SpaceAccessCryptoError {
    #[error("encryption is already initialized")]
    AlreadyInitialized,
    #[error("failed to resolve key scope")]
    ScopeFailed(#[from] ScopeError),
    #[error("encryption failed: {0}")]
    EncryptionFailed(#[from] EncryptionError),
    #[error("failed to persist encryption state")]
    StatePersistenceFailed(#[from] EncryptionStateError),
}

pub struct SpaceAccessCryptoAdapter {
    passphrase: SecretString,
    encryption: Arc<dyn EncryptionPort>,
    key_material: Arc<dyn KeyMaterialPort>,
    key_scope: Arc<dyn KeyScopePort>,
    encryption_state: Arc<dyn EncryptionStatePort>,
    encryption_session: Arc<dyn EncryptionSessionPort>,
}

impl SpaceAccessCryptoAdapter {
    pub fn new(
        passphrase: SecretString,
        encryption: Arc<dyn EncryptionPort>,
        key_material: Arc<dyn KeyMaterialPort>,
        key_scope: Arc<dyn KeyScopePort>,
        encryption_state: Arc<dyn EncryptionStatePort>,
        encryption_session: Arc<dyn EncryptionSessionPort>,
    ) -> Self {
        Self {
            passphrase,
            encryption,
            key_material,
            key_scope,
            encryption_state,
            encryption_session,
        }
    }
}

pub struct DefaultSpaceAccessCryptoFactory {
    encryption: Arc<dyn EncryptionPort>,
    key_material: Arc<dyn KeyMaterialPort>,
    key_scope: Arc<dyn KeyScopePort>,
    encryption_state: Arc<dyn EncryptionStatePort>,
    encryption_session: Arc<dyn EncryptionSessionPort>,
}

impl DefaultSpaceAccessCryptoFactory {
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

impl SpaceAccessCryptoFactory for DefaultSpaceAccessCryptoFactory {
    fn build(&self, passphrase: SecretString) -> Box<dyn CryptoPort> {
        Box::new(SpaceAccessCryptoAdapter::new(
            passphrase,
            self.encryption.clone(),
            self.key_material.clone(),
            self.key_scope.clone(),
            self.encryption_state.clone(),
            self.encryption_session.clone(),
        ))
    }
}

#[async_trait]
impl CryptoPort for SpaceAccessCryptoAdapter {
    async fn generate_nonce32(&self) -> [u8; 32] {
        let mut nonce = [0u8; 32];
        OsRng.fill_bytes(&mut nonce);
        nonce
    }

    async fn export_keyslot_blob(&self, _space_id: &SpaceId) -> anyhow::Result<KeySlot> {
        let span = info_span!("usecase.space_access.export_keyslot_blob");
        async {
            info!("Starting new space keyslot creation");

            let state = self.encryption_state.load_state().await?;
            debug!(state = ?state, "Loaded encryption state");
            if state == EncryptionState::Initialized {
                return Err(SpaceAccessCryptoError::AlreadyInitialized.into());
            }

            let scope = self.key_scope.current_scope().await?;
            debug!(scope = %scope.to_identifier(), "Got key scope");

            let keyslot_draft = KeySlot::draft_v1(scope.clone())?;
            debug!("Keyslot draft created");

            let passphrase = Passphrase(self.passphrase.expose().to_string());
            let kek = self
                .encryption
                .derive_kek(&passphrase, &keyslot_draft.salt, &keyslot_draft.kdf)
                .await?;
            debug!("KEK derived");

            let master_key = MasterKey::generate()?;
            debug!("Master key generated");

            let blob = self
                .encryption
                .wrap_master_key(&kek, &master_key, EncryptionAlgo::XChaCha20Poly1305)
                .await?;
            debug!("Master key wrapped");

            let keyslot = keyslot_draft.finalize(WrappedMasterKey { blob });

            if let Err(e) = self.key_material.store_kek(&scope, &kek).await {
                error!(error = %e, "store_kek failed");
                return Err(e.into());
            }

            if let Err(e) = self.key_material.store_keyslot(&keyslot).await {
                error!(error = %e, "store_keyslot failed");
                if let Err(err) = self.key_material.delete_keyslot(&scope).await {
                    warn!(error = %err, "rollback delete_keyslot failed");
                }
                if let Err(err) = self.key_material.delete_kek(&scope).await {
                    warn!(error = %err, "rollback delete_kek failed");
                }
                return Err(e.into());
            }

            if let Err(e) = self.encryption_session.set_master_key(master_key).await {
                error!(error = %e, "set_master_key failed");
                if let Err(err) = self.key_material.delete_keyslot(&scope).await {
                    warn!(error = %err, "rollback delete_keyslot failed");
                }
                if let Err(err) = self.key_material.delete_kek(&scope).await {
                    warn!(error = %err, "rollback delete_kek failed");
                }
                return Err(e.into());
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
                return Err(e.into());
            }

            info!("New space keyslot stored");
            Ok(keyslot)
        }
        .instrument(span)
        .await
    }

    async fn derive_master_key_from_keyslot(
        &self,
        keyslot_blob: &[u8],
        passphrase: SecretString,
    ) -> anyhow::Result<MasterKey> {
        let span = info_span!("usecase.space_access.derive_master_key_from_keyslot");
        async {
            info!("Deriving master key from keyslot blob");

            let keyslot: KeySlot = serde_json::from_slice(keyslot_blob)
                .map_err(|_| EncryptionError::CorruptedKeySlot)?;
            let scope = keyslot.scope.clone();
            debug!(scope = %scope.to_identifier(), "Parsed keyslot from blob");

            let wrapped_master_key = keyslot
                .wrapped_master_key
                .as_ref()
                .ok_or(EncryptionError::CorruptedKeySlot)?;

            let passphrase = Passphrase(passphrase.expose().to_string());
            let kek = self
                .encryption
                .derive_kek(&passphrase, &keyslot.salt, &keyslot.kdf)
                .await?;
            debug!("KEK derived from passphrase and keyslot");

            if let Err(e) = self.key_material.store_kek(&scope, &kek).await {
                error!(error = %e, "store_kek failed");
                return Err(e.into());
            }

            if let Err(e) = self.key_material.store_keyslot(&keyslot).await {
                error!(error = %e, "store_keyslot failed");
                if let Err(err) = self.key_material.delete_keyslot(&scope).await {
                    warn!(error = %err, "rollback delete_keyslot failed");
                }
                if let Err(err) = self.key_material.delete_kek(&scope).await {
                    warn!(error = %err, "rollback delete_kek failed");
                }
                return Err(e.into());
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
                    return Err(e.into());
                }
            };
            debug!("Master key unwrapped");

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
                return Err(e.into());
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
                return Err(e.into());
            }

            info!("Master key derivation completed");
            Ok(master_key)
        }
        .instrument(span)
        .await
    }
}
