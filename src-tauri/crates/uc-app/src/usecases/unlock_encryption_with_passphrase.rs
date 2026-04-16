//! Unlock encryption session using a user-provided passphrase.
//!
//! This use case derives the KEK from the passphrase + stored salt/kdf params,
//! unwraps the MasterKey, and sets it in the EncryptionSessionPort for
//! transparent encryption.
//!
//! ## Difference from AutoUnlockEncryptionSession
//!
//! - **AutoUnlock**: Loads KEK from keyring (no user input required).
//! - **UnlockWithPassphrase**: Derives KEK from user-provided passphrase.

use std::sync::Arc;
use tracing::{info, info_span, Instrument};

use uc_core::{
    ports::{
        security::{
            encryption_state::EncryptionStatePort,
            key_scope::{KeyScopePort, ScopeError},
        },
        EncryptionPort, EncryptionSessionPort, KeyMaterialPort,
    },
    security::{
        model::{EncryptionError, Passphrase},
        state::{EncryptionState, EncryptionStateError},
    },
};

#[derive(Debug, thiserror::Error)]
pub enum UnlockWithPassphraseError {
    #[error("encryption state check failed: {0}")]
    StateCheckFailed(#[source] EncryptionStateError),

    #[error("encryption not initialized — cannot unlock with passphrase")]
    NotInitialized,

    #[error("key scope resolution failed: {0}")]
    ScopeFailed(#[source] ScopeError),

    #[error("failed to load keyslot: {0}")]
    KeySlotLoadFailed(#[source] EncryptionError),

    #[error("keyslot has no wrapped master key")]
    MissingWrappedMasterKey,

    #[error("failed to derive KEK from passphrase: {0}")]
    KekDeriveFailed(#[source] EncryptionError),

    #[error("failed to unwrap master key: {0}")]
    UnwrapFailed(#[source] EncryptionError),

    #[error("failed to set master key in session: {0}")]
    SessionSetFailed(#[source] EncryptionError),
}

/// Use case for unlocking encryption with a user-provided passphrase.
///
/// ## Behavior
///
/// 1. Check encryption state — must be `Initialized` (returns `NotInitialized` if not)
/// 2. Resolve current key scope
/// 3. Load keyslot from storage
/// 4. Derive KEK from the provided passphrase + stored salt + kdf params
/// 5. Unwrap the MasterKey using the derived KEK
/// 6. Set the MasterKey in the encryption session
///
/// Returns `Ok(())` on success.
///
/// ## Errors
///
/// - `NotInitialized` — encryption has not been initialized
/// - `ScopeFailed` — could not resolve key scope
/// - `KeySlotLoadFailed` — could not load keyslot from storage
/// - `MissingWrappedMasterKey` — keyslot exists but has no wrapped master key
/// - `KekDeriveFailed` — KDF failed (wrong passphrase is one cause)
/// - `UnwrapFailed` — unwrap failed (wrong passphrase is one cause)
/// - `SessionSetFailed` — could not set master key in session
pub struct UnlockEncryptionWithPassphrase {
    encryption_state: Arc<dyn EncryptionStatePort>,
    key_scope: Arc<dyn KeyScopePort>,
    key_material: Arc<dyn KeyMaterialPort>,
    encryption: Arc<dyn EncryptionPort>,
    encryption_session: Arc<dyn EncryptionSessionPort>,
}

impl UnlockEncryptionWithPassphrase {
    pub fn new(
        encryption_state: Arc<dyn EncryptionStatePort>,
        key_scope: Arc<dyn KeyScopePort>,
        key_material: Arc<dyn KeyMaterialPort>,
        encryption: Arc<dyn EncryptionPort>,
        encryption_session: Arc<dyn EncryptionSessionPort>,
    ) -> Self {
        Self {
            encryption_state,
            key_scope,
            key_material,
            encryption,
            encryption_session,
        }
    }

    pub fn from_ports(
        encryption_state: Arc<dyn EncryptionStatePort>,
        key_scope: Arc<dyn KeyScopePort>,
        key_material: Arc<dyn KeyMaterialPort>,
        encryption: Arc<dyn EncryptionPort>,
        encryption_session: Arc<dyn EncryptionSessionPort>,
    ) -> Self {
        Self::new(
            encryption_state,
            key_scope,
            key_material,
            encryption,
            encryption_session,
        )
    }

    /// Execute the passphrase unlock flow.
    ///
    /// # Returns
    ///
    /// - `Ok(())` — Session unlocked successfully
    /// - `Err(UnlockWithPassphraseError)` — Unlock failed
    ///
    /// # Note on passphrase errors
    ///
    /// Both `KekDeriveFailed` and `UnwrapFailed` can occur when the wrong
    /// passphrase is provided. The underlying crypto may fail at the KDF stage
    /// (Argon2id password hashing) or at the AEAD unwrap stage (wrong key).
    pub async fn execute(&self, passphrase: Passphrase) -> Result<(), UnlockWithPassphraseError> {
        let span = info_span!("usecase.unlock_encryption_with_passphrase.execute");

        async {
            info!("Checking encryption state for passphrase unlock");

            // 1. Check encryption state
            let state = self
                .encryption_state
                .load_state()
                .await
                .map_err(UnlockWithPassphraseError::StateCheckFailed)?;

            if state == EncryptionState::Uninitialized {
                info!("Encryption not initialized, cannot unlock with passphrase");
                return Err(UnlockWithPassphraseError::NotInitialized);
            }

            info!("Encryption initialized, attempting passphrase unlock");

            // 2. Get key scope
            let scope = self
                .key_scope
                .current_scope()
                .await
                .map_err(UnlockWithPassphraseError::ScopeFailed)?;

            // 3. Load keyslot
            let keyslot = self
                .key_material
                .load_keyslot(&scope)
                .await
                .map_err(UnlockWithPassphraseError::KeySlotLoadFailed)?;

            // 4. Get wrapped master key
            let wrapped_master_key = keyslot
                .wrapped_master_key
                .ok_or(UnlockWithPassphraseError::MissingWrappedMasterKey)?;

            // 5. Derive KEK from passphrase + salt + kdf params
            let kek = self
                .encryption
                .derive_kek(&passphrase, &keyslot.salt, &keyslot.kdf)
                .await
                .map_err(UnlockWithPassphraseError::KekDeriveFailed)?;

            // 6. Unwrap master key
            let master_key = self
                .encryption
                .unwrap_master_key(&kek, &wrapped_master_key.blob)
                .await
                .map_err(UnlockWithPassphraseError::UnwrapFailed)?;

            // 7. Set master key in session
            self.encryption_session
                .set_master_key(master_key)
                .await
                .map_err(UnlockWithPassphraseError::SessionSetFailed)?;

            info!("Passphrase unlock completed successfully");
            Ok(())
        }
        .instrument(span)
        .await
    }
}
